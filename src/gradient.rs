//! Data-only prop.map XML subset used by AE gradient snapshots. No entities,
//! DTDs, external references, schema fetching or XML evaluation are supported.
use crate::{
    Error,
    decode::{diag, span},
    error::Budget,
    model::*,
    rifx::Chunk,
};

pub(crate) fn property(
    c: &Chunk<'_>,
    ticks: u32,
    budget: &mut Budget<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(Property, DecodeStatus), Error> {
    let mut p = Property::default();
    if let Some(inner) = c.list(b"tdbs") {
        p.metadata = crate::metadata::property(inner, None, budget)?;
        if let Some(expr) = inner.child(b"Utf8") {
            p.expression = Some(budget.text(expr.data)?);
            p.expression_enabled = inner
                .child(b"tdb4")
                .and_then(|c| c.data.get(0x77))
                .map(|n| *n == 0);
        }
    }
    let mut status = DecodeStatus::Decoded;
    if let Some(keys) = c.list(b"GCky") {
        for xml in keys.children.iter().filter(|n| n.id == *b"Utf8") {
            let gradient = match decode(xml.data, budget) {
                Ok(g) => Some(g),
                Err(Error::InvalidFormat { .. }) => {
                    status = DecodeStatus::Partial;
                    diag(
                        diagnostics,
                        budget,
                        "unsupported-gradient-xml",
                        xml.offset,
                        "malformed or unsupported gradient XML; original snapshot remains accessible",
                    )?;
                    None
                }
                Err(e) => return Err(e),
            };
            budget.push(
                &mut p.gradient_snapshots,
                GradientSnapshot {
                    gradient,
                    timing: None,
                    source: span(xml),
                },
            )?;
        }
    }
    if p.gradient_snapshots.is_empty() {
        status = DecodeStatus::Unsupported;
    }
    if p.gradient_snapshots.iter().any(|s| s.gradient.is_some()) {
        p.metadata.control_type = Some(11);
        p.metadata.value_type = PropertyValueType::Custom;
    }
    let time_table = c.list(b"tdbs").and_then(|n| n.list(b"list"));
    if let Some(time_table) = time_table {
        status = DecodeStatus::Partial;
        if let Some((count, 64, data)) = crate::decode::table(time_table)
            .filter(|(count, _, _)| *count == p.gradient_snapshots.len())
        {
            budget.keyframes = budget
                .keyframes
                .checked_add(count)
                .ok_or(Error::Allocation)?;
            Budget::check(
                "keyframe count",
                budget.keyframes,
                budget.limits.max_keyframes,
            )?;
            let base = time_table.child(b"ldat").ok_or(Error::Allocation)?.offset;
            for (index, snapshot) in p.gradient_snapshots.iter_mut().enumerate() {
                let offset = index.checked_mul(64).ok_or(Error::Allocation)?;
                let record = data.get(offset..offset + 64).ok_or(Error::Allocation)?;
                let time_ticks = crate::binary::u32(record, 0).ok_or(Error::Allocation)?;
                snapshot.timing = Some(GradientTiming {
                    time_ticks,
                    time_seconds: f64::from(time_ticks) / f64::from(ticks),
                    in_interpolation: record.get(4).copied().ok_or(Error::Allocation)?,
                    out_interpolation: record.get(5).copied().ok_or(Error::Allocation)?,
                    raw_flags: [
                        record.get(6).copied().ok_or(Error::Allocation)?,
                        record.get(7).copied().ok_or(Error::Allocation)?,
                    ],
                    source: RawSpan {
                        offset: base
                            .checked_add(8)
                            .and_then(|n| n.checked_add(offset))
                            .ok_or(Error::Allocation)?,
                        length: 64,
                    },
                });
            }
            diag(
                diagnostics,
                budget,
                "gradient-easing-unverified",
                time_table.offset,
                "gradient times and interpolation codes decoded; easing and interpolation evaluation remain unverified",
            )?;
        } else {
            diag(
                diagnostics,
                budget,
                "gradient-timing-unverified",
                time_table.offset,
                "gradient time table is malformed, has an unsupported stride or disagrees with snapshot count",
            )?;
        }
    } else if p.gradient_snapshots.len() > 1 {
        status = DecodeStatus::Partial;
        diag(
            diagnostics,
            budget,
            "gradient-timing-unverified",
            c.offset,
            "multiple gradient snapshots have no time table; no animation times are inferred",
        )?;
    }
    Ok((p, status))
}

struct Element<'a> {
    name: &'a str,
    text: &'a str,
    version: Option<&'a str>,
    children: Vec<Element<'a>>,
}
impl<'a> Element<'a> {
    fn child(&self, name: &str) -> Option<&Self> {
        self.children.iter().find(|c| c.name == name)
    }
    fn list(&self, key: &str) -> Option<&Self> {
        self.children
            .iter()
            .filter(|n| n.name == "prop.pair")
            .find(|n| n.child("key").is_some_and(|k| k.text.trim() == key))?
            .child("prop.list")
    }
    fn values<const N: usize>(&self, key: &str) -> Option<[Number; N]> {
        let pair = self.children.iter().find(|n| {
            n.name == "prop.pair" && n.child("key").is_some_and(|k| k.text.trim() == key)
        })?;
        let mut floats = pair
            .child("array")?
            .children
            .iter()
            .filter(|n| n.name == "float");
        let mut values = [Number::from(0.0); N];
        for value in &mut values {
            *value = floats.next()?.text.trim().parse::<f64>().ok()?.into();
        }
        Some(values)
    }
    fn stops(&self, group: &str) -> Option<&Self> {
        self.list(group)?.list("Stops List")
    }
}
fn malformed(offset: usize) -> Error {
    Error::InvalidFormat {
        offset,
        message: "malformed or unsupported gradient XML",
    }
}
pub(crate) fn decode(raw: &[u8], budget: &mut Budget<'_>) -> Result<Gradient, Error> {
    let end = raw.iter().position(|n| *n == 0).unwrap_or(raw.len());
    // Chunk strings may have a NUL terminator, but no nonzero suffix may be hidden.
    if raw
        .get(end..)
        .is_some_and(|tail| tail.iter().any(|n| *n != 0))
    {
        return Err(malformed(end));
    }
    let text =
        std::str::from_utf8(raw.get(..end).ok_or(Error::Allocation)?).map_err(|_| malformed(0))?;
    let mut parser = Xml {
        text,
        pos: 0,
        nodes: 0,
        budget,
    };
    parser.ws();
    if parser.rest().starts_with("<?xml ") {
        let len = parser
            .rest()
            .find("?>")
            .ok_or_else(|| malformed(parser.pos))?;
        parser.pos += len + 2;
    }
    parser.ws();
    let root = parser.element(0)?;
    parser.ws();
    if parser.pos != text.len() || root.name != "prop.map" {
        return Err(malformed(parser.pos));
    }
    let data = root
        .children
        .iter()
        .filter(|n| n.name == "prop.list")
        .find_map(|n| n.list("Gradient Color Data"))
        .ok_or_else(|| malformed(0))?;
    let mut out = Gradient {
        version: root.version.map(|s| parser.budget.owned(s)).transpose()?,
        color_stops: Vec::new(),
        alpha_stops: Vec::new(),
    };
    if let Some(list) = data.stops("Color Stops") {
        for (i, pair) in list
            .children
            .iter()
            .filter(|n| n.name == "prop.pair")
            .enumerate()
        {
            stop_index(pair, i)?;
            let values = pair
                .child("prop.list")
                .and_then(|n| n.values::<5>("Stops Color"))
                .ok_or_else(|| malformed(0))?;
            let [offset, midpoint, r, g, b] = values;
            parser.budget.push(
                &mut out.color_stops,
                GradientColorStop {
                    offset,
                    midpoint,
                    color: [r, g, b],
                },
            )?;
        }
    }
    if let Some(list) = data.stops("Alpha Stops") {
        for (i, pair) in list
            .children
            .iter()
            .filter(|n| n.name == "prop.pair")
            .enumerate()
        {
            stop_index(pair, i)?;
            let [offset, midpoint, alpha] = pair
                .child("prop.list")
                .and_then(|n| n.values::<3>("Stops Alpha"))
                .ok_or_else(|| malformed(0))?;
            parser.budget.push(
                &mut out.alpha_stops,
                GradientAlphaStop {
                    offset,
                    midpoint,
                    alpha,
                },
            )?;
        }
    }
    Ok(out)
}
fn stop_index(pair: &Element<'_>, i: usize) -> Result<(), Error> {
    if pair
        .child("key")
        .and_then(|n| n.text.trim().strip_prefix("Stop-"))
        .and_then(|s| s.parse::<usize>().ok())
        == Some(i)
    {
        Ok(())
    } else {
        Err(malformed(0))
    }
}
struct Xml<'a, 'b, 'c> {
    text: &'a str,
    pos: usize,
    nodes: usize,
    budget: &'b mut Budget<'c>,
}
impl<'a> Xml<'a, '_, '_> {
    fn rest(&self) -> &'a str {
        self.text.get(self.pos..).unwrap_or_default()
    }
    fn ws(&mut self) {
        self.pos += self.rest().len()
            - self
                .rest()
                .trim_start_matches([' ', '\t', '\r', '\n'])
                .len();
    }
    fn name(&mut self) -> Result<&'a str, Error> {
        if !self
            .rest()
            .as_bytes()
            .first()
            .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
        {
            return Err(malformed(self.pos));
        }
        let len = self
            .rest()
            .bytes()
            .take_while(|c| c.is_ascii_alphanumeric() || b"._-".contains(c))
            .count();
        if len == 0 {
            return Err(malformed(self.pos));
        }
        let name = self.rest().get(..len).ok_or_else(|| malformed(self.pos))?;
        self.pos += len;
        Ok(name)
    }
    fn eat(&mut self, s: &str) -> Result<(), Error> {
        if !self.rest().starts_with(s) {
            return Err(malformed(self.pos));
        }
        self.pos += s.len();
        Ok(())
    }
    fn element(&mut self, depth: usize) -> Result<Element<'a>, Error> {
        Budget::check("gradient XML depth", depth, self.budget.limits.max_depth)?;
        self.nodes = self.nodes.checked_add(1).ok_or(Error::Allocation)?;
        Budget::check(
            "gradient XML nodes",
            self.nodes,
            self.budget.limits.max_chunks,
        )?;
        self.eat("<")?;
        let name = self.name()?;
        let mut out = Element {
            name,
            text: "",
            version: None,
            children: Vec::new(),
        };
        loop {
            let previous = self.pos;
            self.ws();
            if self.rest().starts_with("/>") {
                self.pos += 2;
                return Ok(out);
            }
            if self.rest().starts_with('>') {
                self.pos += 1;
                break;
            }
            if self.pos == previous {
                return Err(malformed(self.pos));
            }
            let attr = self.name()?;
            self.ws();
            self.eat("=")?;
            self.ws();
            let quote = self
                .rest()
                .as_bytes()
                .first()
                .copied()
                .filter(|q| *q == b'\'' || *q == b'"')
                .ok_or_else(|| malformed(self.pos))?;
            self.pos += 1;
            let len = self
                .rest()
                .find(char::from(quote))
                .ok_or_else(|| malformed(self.pos))?;
            let value = self.rest().get(..len).ok_or_else(|| malformed(self.pos))?;
            if value.contains(['<', '&']) {
                return Err(malformed(self.pos));
            }
            if attr == "version" {
                if out.version.is_some() {
                    return Err(malformed(self.pos));
                }
                out.version = Some(value);
            }
            self.pos += len + 1;
        }
        loop {
            let len = self.rest().find('<').ok_or_else(|| malformed(self.pos))?;
            let text = self.rest().get(..len).ok_or_else(|| malformed(self.pos))?;
            if text.contains('&') || text.contains("]]>") {
                return Err(malformed(self.pos));
            }
            if !text.trim().is_empty() {
                if !out.text.is_empty() || !out.children.is_empty() {
                    return Err(malformed(self.pos));
                }
                out.text = text;
            }
            self.pos += len;
            if self.rest().starts_with("</") {
                self.pos += 2;
                if self.name()? != name {
                    return Err(malformed(self.pos));
                }
                self.ws();
                self.eat(">")?;
                return Ok(out);
            }
            if !out.text.is_empty() {
                return Err(malformed(self.pos));
            }
            let child = self.element(depth + 1)?;
            self.budget.push(&mut out.children, child)?;
        }
    }
}
