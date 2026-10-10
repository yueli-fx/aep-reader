//! Bounded parser for the data-only PostScript-style text-engine dictionary.
//! No PostScript or JavaScript evaluation takes place.
use crate::{
    DecodeStatus, Error, Number, TextDocument, TextParagraph, TextRange, TextStyleRun, binary as b,
    error::Budget,
};

enum Ps<'a> {
    Num(f64),
    Str(Vec<u8>),
    Bool(bool),
    Atom,
    Dict(Vec<(&'a [u8], Ps<'a>)>),
    Array(Vec<Ps<'a>>),
}
impl Ps<'_> {
    fn path(&self, path: &str) -> Option<&Self> {
        let mut v = self;
        for s in path.split('/').filter(|s| !s.is_empty()) {
            v = match v {
                Self::Dict(d) => &d.iter().find(|(k, _)| *k == s.as_bytes())?.1,
                Self::Array(a) => a.get(s.parse::<usize>().ok()?)?,
                _ => return None,
            };
        }
        Some(v)
    }
    fn boolean(&self) -> Option<bool> {
        if let Self::Bool(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    fn num(&self) -> Option<Number> {
        if let Self::Num(n) = self {
            Some((*n).into())
        } else {
            None
        }
    }
}
fn invalid(offset: usize) -> Error {
    Error::InvalidFormat {
        offset,
        message: "malformed text-engine dictionary",
    }
}

impl TextStyleRun {
    pub fn font_name<'a>(&self, document: &'a TextDocument) -> Option<&'a str> {
        let index = self.font_index?.value?;
        if !index.is_finite()
            || index < 0.0
            || index.fract() != 0.0
            || index >= document.fonts.len() as f64
        {
            return None;
        }
        document.fonts.get(index as usize).map(String::as_str)
    }
    pub fn all_caps(&self) -> Option<bool> {
        caps_flag(self.caps_option, &[2.0, 3.0], 3.0)
    }
    pub fn small_caps(&self) -> Option<bool> {
        caps_flag(self.caps_option, &[1.0, 3.0], 3.0)
    }
    pub fn superscript(&self) -> Option<bool> {
        caps_flag(self.baseline_option, &[1.0], 2.0)
    }
    pub fn subscript(&self) -> Option<bool> {
        caps_flag(self.baseline_option, &[2.0], 2.0)
    }
}
fn caps_flag(value: Option<Number>, matches: &[f64], max: f64) -> Option<bool> {
    let value = value?.value?;
    (value.is_finite() && value >= 0.0 && value <= max && value.fract() == 0.0)
        .then_some(matches.contains(&value))
}

pub(crate) fn decode(raw: &[u8], budget: &mut Budget<'_>) -> Result<TextDocument, Error> {
    let mut off = 0;
    let mut body = None;
    while off < raw.len() {
        if raw.get(off..off.saturating_add(4)) != Some(b"LIST") {
            return Err(invalid(off));
        }
        let size = usize::try_from(
            b::u32(raw, off.checked_add(4).ok_or(Error::Allocation)?)
                .ok_or_else(|| invalid(off))?,
        )
        .map_err(|_| Error::Allocation)?;
        let end = off
            .checked_add(8)
            .and_then(|n| n.checked_add(size))
            .ok_or_else(|| invalid(off))?;
        if size < 4 || end > raw.len() {
            return Err(invalid(off));
        }
        if raw.get(off + 8..off + 12) == Some(b"btdk") {
            body = raw.get(off + 12..end);
            break;
        }
        off = end.checked_add(size % 2).ok_or_else(|| invalid(off))?;
    }
    let body = body.ok_or_else(|| invalid(off))?;
    let mut parser = Parser {
        data: body,
        pos: 0,
        budget,
    };
    let root = parser.dict(false, 0)?;
    let text = match root.path("/1/1/0/0/0") {
        Some(Ps::Str(s)) => decode_string(s, parser.budget)?,
        _ => return Err(invalid(0)),
    };
    let mut fonts = Vec::new();
    let mut font_axes = Vec::new();
    if let Some(Ps::Array(entries)) = root.path("/0/1/0") {
        for e in entries {
            let font = match e.path("/0/0/0") {
                Some(Ps::Str(s)) => decode_string(s, parser.budget)?,
                _ => String::new(),
            };
            parser.budget.push(&mut fonts, font)?;
            let mut axes = Vec::new();
            if let Some(Ps::Array(a)) = e.path("/0/0/4") {
                for v in a {
                    if let Ps::Num(n) = v {
                        parser.budget.push(&mut axes, (*n / 65536.0).into())?;
                    }
                }
            }
            parser.budget.push(&mut font_axes, axes)?;
        }
    }
    let mut style_runs = Vec::new();
    let text_units = text.encode_utf16().count();
    let mut run_cursor = Some(0);
    if let Some(Ps::Array(entries)) = root.path("/1/1/0/0/6/0") {
        for e in entries {
            let style = e.path("/0/0/6");
            let n = |p| style.and_then(|s| s.path(p)).and_then(Ps::num);
            let flag = |p| style.and_then(|s| s.path(p)).and_then(Ps::boolean);
            parser.budget.push(
                &mut style_runs,
                TextStyleRun {
                    range: checked_range(e, &mut run_cursor, text_units),
                    font_index: n("/0"),
                    font_size: n("/1"),
                    tracking: n("/8"),
                    leading: n("/5"),
                    fill_argb: paint(style, "/53/0/1"),
                    stroke_argb: paint(style, "/54/0/1"),
                    faux_bold: flag("/2"),
                    faux_italic: flag("/3"),
                    auto_leading: flag("/4"),
                    horizontal_scale: n("/6"),
                    vertical_scale: n("/7"),
                    baseline_shift: n("/9"),
                    tsume: n("/36"),
                    apply_stroke: flag("/57"),
                    stroke_width: n("/63"),
                    auto_kern_type: n("/11"),
                    caps_option: n("/12"),
                    baseline_option: n("/13"),
                    no_break: flag("/52"),
                    stroke_over_fill: flag("/58"),
                    line_join_type: n("/62"),
                    digit_set: n("/70"),
                },
            )?;
        }
    }
    let mut paragraphs = Vec::new();
    let mut paragraph_cursor = Some(0);
    if let Some(Ps::Array(a)) = root.path("/1/1/0/0/5/0") {
        for entry in a {
            let style = entry.path("/0/0/5");
            let n = |p| style.and_then(|s| s.path(p)).and_then(Ps::num);
            let flag = |p| style.and_then(|s| s.path(p)).and_then(Ps::boolean);
            parser.budget.push(
                &mut paragraphs,
                TextParagraph {
                    range: checked_range(entry, &mut paragraph_cursor, text_units),
                    justification: n("/0"),
                    first_line_indent: n("/1"),
                    start_indent: n("/2"),
                    end_indent: n("/3"),
                    space_before: n("/4"),
                    space_after: n("/5"),
                    leading_type: n("/8"),
                    auto_hyphenate: flag("/9"),
                    hanging_roman: flag("/21"),
                    direction: n("/33"),
                },
            )?;
        }
    }
    let box_bounds = match root.path("/0/8/0/0/0/1/0") {
        Some(Ps::Array(a)) if a.len() >= 8 && a.len() % 2 == 0 => bounds(a),
        _ => None,
    };
    let mut manual_kerning = Vec::new();
    if let Some(Ps::Array(a)) = root.path("/1/1/0/0/8/0") {
        for entry in a {
            if let Some(n) = entry.path("/0/0").and_then(Ps::num) {
                parser.budget.push(&mut manual_kerning, n)?;
            }
        }
    }
    let kerning = root.path("/1/1/0/0/7").and_then(Ps::num);
    let justification = root.path("/1/1/0/0/5/0/0/0/0/5/0").and_then(Ps::num);
    Ok(TextDocument {
        text,
        fonts,
        font_axes,
        paragraphs,
        box_bounds,
        manual_kerning,
        kerning,
        style_runs,
        justification,
        decode_status: DecodeStatus::Partial,
    })
}
fn checked_range(entry: &Ps<'_>, cursor: &mut Option<usize>, total: usize) -> Option<TextRange> {
    let result = (|| {
        let start = (*cursor)?;
        let length = entry.path("/1")?.num()?.value?;
        if !length.is_finite()
            || length < 0.0
            || length.fract() != 0.0
            || length >= usize::MAX as f64
        {
            return None;
        }
        let length = length as usize;
        let end = start.checked_add(length).filter(|n| *n <= total)?;
        *cursor = Some(end);
        Some(TextRange {
            utf16_start: start,
            utf16_length: length,
        })
    })();
    if result.is_none() {
        *cursor = None;
    }
    result
}
fn paint(style: Option<&Ps<'_>>, path: &str) -> Option<[Number; 4]> {
    let Ps::Array(a) = style?.path(path)? else {
        return None;
    };
    if a.len() != 4 {
        return None;
    }
    Some([
        a.first()?.num()?,
        a.get(1)?.num()?,
        a.get(2)?.num()?,
        a.get(3)?.num()?,
    ])
}
fn bounds(a: &[Ps<'_>]) -> Option<[Number; 4]> {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for pair in a.chunks_exact(2) {
        let x = pair.first()?.num()?.value?;
        let y = pair.get(1)?.num()?.value?;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    Some([min_x.into(), min_y.into(), max_x.into(), max_y.into()])
}
fn decode_string(bytes: &[u8], budget: &mut Budget<'_>) -> Result<String, Error> {
    if let Some(utf16) = bytes.strip_prefix(&[0xfe, 0xff]) {
        if utf16.len() % 2 != 0 {
            return Err(invalid(0));
        }
        let capacity = utf16.len().checked_mul(3).ok_or(Error::Allocation)?;
        budget.charge(capacity)?;
        let mut s = String::new();
        s.try_reserve_exact(capacity)
            .map_err(|_| Error::Allocation)?;
        for c in char::decode_utf16(utf16.chunks_exact(2).filter_map(|u| b::u16(u, 0))) {
            let c = c.map_err(|_| invalid(0))?;
            s.push(c);
        }
        Ok(s)
    } else {
        // PostScript strings may contain NUL; unlike chunk names, retain it.
        let cap = bytes.len().checked_mul(6).ok_or(Error::Allocation)?;
        budget.charge(cap)?;
        Ok(String::from_utf8_lossy(bytes).into_owned())
    }
}

struct Parser<'a, 'b, 'c> {
    data: &'a [u8],
    pos: usize,
    budget: &'b mut Budget<'c>,
}
impl<'a> Parser<'a, '_, '_> {
    fn ws(&mut self) {
        loop {
            while self
                .data
                .get(self.pos)
                .is_some_and(|b| b.is_ascii_whitespace() || *b == 0)
            {
                self.pos += 1;
            }
            if self.data.get(self.pos) == Some(&b'%') {
                while self
                    .data
                    .get(self.pos)
                    .is_some_and(|b| *b != b'\n' && *b != b'\r')
                {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }
    fn name(&mut self) -> Result<&'a [u8], Error> {
        self.ws();
        if self.data.get(self.pos) != Some(&b'/') {
            return Err(invalid(self.pos));
        }
        self.pos += 1;
        self.atom()
    }
    fn atom(&mut self) -> Result<&'a [u8], Error> {
        let start = self.pos;
        while self
            .data
            .get(self.pos)
            .is_some_and(|b| !b.is_ascii_whitespace() && !b"<>[]()/\0%".contains(b))
        {
            self.pos += 1;
        }
        if start == self.pos {
            return Err(invalid(self.pos));
        }
        self.data
            .get(start..self.pos)
            .ok_or_else(|| invalid(self.pos))
    }
    fn dict(&mut self, explicit: bool, depth: usize) -> Result<Ps<'a>, Error> {
        Budget::check("text dictionary depth", depth, self.budget.limits.max_depth)?;
        let mut entries = Vec::new();
        loop {
            self.ws();
            if explicit && self.data.get(self.pos..self.pos.saturating_add(2)) == Some(b">>") {
                self.pos += 2;
                break;
            }
            if self.pos >= self.data.len() {
                if explicit {
                    return Err(invalid(self.pos));
                }
                break;
            }
            let key = self.name()?;
            let value = self.value(depth + 1)?;
            self.budget.push(&mut entries, (key, value))?;
        }
        Ok(Ps::Dict(entries))
    }
    fn value(&mut self, depth: usize) -> Result<Ps<'a>, Error> {
        Budget::check("text dictionary depth", depth, self.budget.limits.max_depth)?;
        self.ws();
        match self.data.get(self.pos).copied() {
            Some(b'<') if self.data.get(self.pos + 1) == Some(&b'<') => {
                self.pos += 2;
                self.dict(true, depth)
            }
            Some(b'[') => {
                self.pos += 1;
                let mut a = Vec::new();
                loop {
                    self.ws();
                    if self.data.get(self.pos) == Some(&b']') {
                        self.pos += 1;
                        break;
                    }
                    let v = self.value(depth + 1)?;
                    self.budget.push(&mut a, v)?;
                }
                Ok(Ps::Array(a))
            }
            Some(b'(') => self.string(),
            Some(b'/') => {
                self.name()?;
                Ok(Ps::Atom)
            }
            Some(_) => {
                let token = self.atom()?;
                let number = std::str::from_utf8(token)
                    .ok()
                    .and_then(|s| s.parse::<f64>().ok());
                Ok(match number {
                    Some(n) => Ps::Num(n),
                    None => match token {
                        b"true" => Ps::Bool(true),
                        b"false" => Ps::Bool(false),
                        _ => Ps::Atom,
                    },
                })
            }
            None => Err(invalid(self.pos)),
        }
    }
    fn string(&mut self) -> Result<Ps<'a>, Error> {
        self.pos += 1;
        let mut nesting = 1;
        let mut output = Vec::new();
        while let Some(c) = self.data.get(self.pos).copied() {
            self.pos += 1;
            let decoded = match c {
                b'(' => {
                    nesting += 1;
                    Budget::check("text string nesting", nesting, self.budget.limits.max_depth)?;
                    Some(c)
                }
                b')' => {
                    nesting -= 1;
                    if nesting == 0 {
                        return Ok(Ps::Str(output));
                    }
                    Some(c)
                }
                b'\\' => {
                    let next = self
                        .data
                        .get(self.pos)
                        .copied()
                        .ok_or_else(|| invalid(self.pos))?;
                    self.pos += 1;
                    match next {
                        b'n' => Some(b'\n'),
                        b'r' => Some(b'\r'),
                        b't' => Some(b'\t'),
                        b'b' => Some(8),
                        b'f' => Some(12),
                        b'\n' => None,
                        b'\r' => {
                            if self.data.get(self.pos) == Some(&b'\n') {
                                self.pos += 1;
                            }
                            None
                        }
                        b'0'..=b'7' => {
                            let mut n = u16::from(next - b'0');
                            for _ in 0..2 {
                                if let Some(d @ b'0'..=b'7') = self.data.get(self.pos).copied() {
                                    n = n * 8 + u16::from(d - b'0');
                                    self.pos += 1;
                                } else {
                                    break;
                                }
                            }
                            Some(n as u8)
                        }
                        _ => Some(next),
                    }
                }
                _ => Some(c),
            };
            if let Some(c) = decoded {
                self.budget.push(&mut output, c)?;
            }
        }
        Err(invalid(self.pos))
    }
}
