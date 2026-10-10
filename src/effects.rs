//! Decode saved project and instance parameter definitions without filling values.
use crate::{
    Error, binary as b,
    decode::{diag, span},
    error::Budget,
    model::*,
    rifx::Chunk,
};

pub(crate) fn catalog(
    root: &Chunk<'_>,
    budget: &mut Budget<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<EffectDefinition>, Error> {
    let mut out = Vec::new();
    for group in root.children.iter().filter(|c| c.form == Some(*b"EfdG")) {
        for entry in group.children.iter().filter(|c| c.form == Some(*b"EfDf")) {
            for (i, name) in entry.children.iter().enumerate() {
                if name.id != *b"tdmn" {
                    continue;
                }
                let Some(wrapper) = entry
                    .children
                    .get(i + 1)
                    .filter(|c| c.form == Some(*b"sspc"))
                else {
                    diag(
                        diagnostics,
                        budget,
                        "invalid-effect-catalog",
                        name.offset,
                        "project effect name has no adjacent sspc definition",
                    )?;
                    continue;
                };
                if wrapper.list(b"parT").is_none() {
                    diag(
                        diagnostics,
                        budget,
                        "missing-effect-parameter-table",
                        wrapper.offset,
                        "project effect definition has no parT; no defaults inferred",
                    )?;
                }
                let definition = EffectDefinition {
                    match_name: budget.text(name.data)?,
                    parameters: definitions(wrapper, budget, diagnostics)?,
                    source: span(wrapper),
                };
                budget.push(&mut out, definition)?;
            }
        }
    }
    Ok(out)
}

pub(crate) fn definitions(
    c: &Chunk<'_>,
    budget: &mut Budget<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<EffectParameterDefinition>, Error> {
    let mut out = Vec::new();
    let Some(table) = c.list(b"parT") else {
        return Ok(out);
    };
    let mut header = true;
    let mut i = 0;
    while let Some(c) = table.children.get(i) {
        i += 1;
        if c.id != *b"tdmn" {
            continue;
        }
        let Some(pard) = table.children.get(i).filter(|c| c.id == *b"pard") else {
            diag(
                diagnostics,
                budget,
                "invalid-effect-definition",
                c.offset,
                "parameter name has no adjacent pard; definition remains raw",
            )?;
            continue;
        };
        i += 1;
        if header {
            header = false;
            continue;
        }
        let match_name = budget.text(c.data)?;
        let mut def = definition(&match_name, pard, budget)?;
        if def.control_type == 7 {
            for extra in table.children.get(i..).unwrap_or_default() {
                if extra.id == *b"tdmn" {
                    break;
                }
                if extra.id != *b"pdnm" {
                    continue;
                }
                let text = embedded_text(extra.data);
                if let Some(text) = text {
                    let text = budget.text(text)?;
                    for option in text.split('|') {
                        let s = budget.owned(option.trim())?;
                        budget.push(&mut def.enum_options, s)?;
                    }
                } else {
                    def.decode_status = DecodeStatus::Partial;
                }
                break;
            }
        }
        if def.decode_status != DecodeStatus::Decoded {
            diag(
                diagnostics,
                budget,
                "partial-effect-definition",
                pard.offset,
                &match_name,
            )?;
        }
        budget.push(&mut out, def)?;
    }
    Ok(out)
}
fn embedded_text(d: &[u8]) -> Option<&[u8]> {
    if d.get(..4) != Some(b"Utf8") {
        return None;
    }
    let length = usize::try_from(b::u32(d, 4)?).ok()?;
    d.get(8..8usize.checked_add(length)?)
}
fn value(values: &[f64], budget: &mut Budget<'_>) -> Result<Value, Error> {
    let mut out = Vec::new();
    for n in values {
        budget.push(&mut out, Number::from(*n))?;
    }
    Ok(Value::Numbers(out))
}
fn definition(
    match_name: &str,
    c: &Chunk<'_>,
    budget: &mut Budget<'_>,
) -> Result<EffectParameterDefinition, Error> {
    let d = c.data;
    let mut def = EffectParameterDefinition {
        match_name: budget.owned(match_name)?,
        name: match d.get(16..48) {
            Some(s) if s.first() != Some(&0) => budget.text(s)?,
            _ => budget.owned(match_name)?,
        },
        control_type: d.get(15).copied().unwrap_or(15),
        last_value: None,
        default_value: None,
        min_value: None,
        max_value: None,
        option_count: None,
        enum_options: Vec::new(),
        decode_status: DecodeStatus::Partial,
        source: span(c),
    };
    let Some(body) = d.get(56..) else {
        return Ok(def);
    };
    let mut values = [0.0; 3];
    let (last, default) = match def.control_type {
        2 if body.len() >= 82 => {
            let n = f64::from(b::i32(body, 0).ok_or(Error::Allocation)?) / 65536.0;
            def.min_value = b::u16(body, 76).map(|n| Number::from(f64::from(n as i16)));
            def.max_value = b::u16(body, 80).map(|n| Number::from(f64::from(n as i16)));
            (Some(n), Some(n))
        }
        3 if body.len() >= 4 => {
            let n = f64::from(b::i32(body, 0).ok_or(Error::Allocation)?) / 65536.0;
            (Some(n), Some(n))
        }
        4 if body.len() >= 5 => (
            b::u32(body, 0).map(f64::from),
            body.get(4).map(|n| f64::from(*n)),
        ),
        6 if body.len() >= 8 => {
            for (i, v) in values.iter_mut().take(2).enumerate() {
                *v = f64::from(b::i32(body, i * 4).ok_or(Error::Allocation)?) / 128.0;
            }
            def.last_value = Some(value(values.get(..2).ok_or(Error::Allocation)?, budget)?);
            def.default_value = Some(value(values.get(..2).ok_or(Error::Allocation)?, budget)?);
            (None, None)
        }
        7 if body.len() >= 12 => {
            def.option_count = b::i32(body, 4).map(|n| n >> 16);
            def.min_value = Some(1.0.into());
            def.max_value = def.option_count.map(|n| f64::from(n).into());
            (
                b::u32(body, 0).map(f64::from),
                b::i32(body, 8).map(f64::from),
            )
        }
        10 if body.len() >= 64 => {
            def.max_value = b::f32(body, 60).map(Number::from);
            let n = b::f64(body, 0, false);
            (n, n)
        }
        18 if body.len() >= 24 => {
            for (i, v) in values.iter_mut().enumerate() {
                *v = b::f64(body, i * 8, false).ok_or(Error::Allocation)? * 512.0;
            }
            def.last_value = Some(value(&values, budget)?);
            def.default_value = Some(value(&values, budget)?);
            (None, None)
        }
        // These define a control but Go has no numerical payload decoder.
        0 | 9 | 11 | 12 | 13 => {
            def.decode_status = DecodeStatus::Decoded;
            return Ok(def);
        }
        _ => return Ok(def),
    };
    if let Some(n) = last {
        def.last_value = Some(value(&[n], budget)?);
    }
    if let Some(n) = default {
        def.default_value = Some(value(&[n], budget)?);
    }
    def.decode_status = DecodeStatus::Decoded;
    Ok(def)
}
pub(crate) fn apply(
    c: &Chunk<'_>,
    node: &mut PropertyNode,
    budget: &mut Budget<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), Error> {
    node.effect_parameters = definitions(c, budget, diagnostics)?;
    // A bounded sorted index avoids quadratic matching on hostile effect trees.
    let mut lookup = Vec::new();
    for (i, def) in node.effect_parameters.iter().enumerate() {
        budget.push(&mut lookup, (i, def))?;
    }
    lookup.sort_unstable_by(|(a, da), (b, db)| da.match_name.cmp(&db.match_name).then(a.cmp(b)));
    apply_children(&lookup, &mut node.children, budget)?;
    Ok(())
}
fn apply_children(
    lookup: &[(usize, &EffectParameterDefinition)],
    nodes: &mut [PropertyNode],
    budget: &mut Budget<'_>,
) -> Result<(), Error> {
    for node in nodes {
        if let Some(p) = &mut node.property {
            let first = lookup.partition_point(|(_, d)| d.match_name < node.match_name);
            if let Some((_, source)) = lookup
                .get(first)
                .filter(|(_, d)| d.match_name == node.match_name)
            {
                let def = copy_definition(source, budget)?;
                // Unknown pard type 15 must not erase a tdb4-derived type.
                if def.control_type != 15 {
                    p.metadata.control_type = Some(def.control_type);
                }
                p.metadata.value_type = match def.control_type {
                    0 => PropertyValueType::LayerIndex,
                    12 => PropertyValueType::MaskIndex,
                    9 | 11 => PropertyValueType::Custom,
                    13 => PropertyValueType::NoValue,
                    5 => PropertyValueType::Color,
                    6 => {
                        if p.metadata.spatial == Some(true) {
                            PropertyValueType::TwoDSpatial
                        } else {
                            PropertyValueType::TwoD
                        }
                    }
                    18 => {
                        if p.metadata.spatial == Some(true) {
                            PropertyValueType::ThreeDSpatial
                        } else {
                            PropertyValueType::ThreeD
                        }
                    }
                    1..=4 | 7 | 10 => PropertyValueType::OneD,
                    _ => p.metadata.value_type,
                };
                p.effect_definition = Some(def);
            }
        }
        apply_children(lookup, &mut node.children, budget)?;
    }
    Ok(())
}
fn copy_value(v: &Option<Value>, budget: &mut Budget<'_>) -> Result<Option<Value>, Error> {
    let Some(Value::Numbers(values)) = v else {
        return Ok(None);
    };
    let mut out = Vec::new();
    for n in values {
        budget.push(&mut out, *n)?;
    }
    Ok(Some(Value::Numbers(out)))
}
fn copy_definition(
    d: &EffectParameterDefinition,
    budget: &mut Budget<'_>,
) -> Result<EffectParameterDefinition, Error> {
    let mut options = Vec::new();
    for s in &d.enum_options {
        let s = budget.owned(s)?;
        budget.push(&mut options, s)?;
    }
    Ok(EffectParameterDefinition {
        match_name: budget.owned(&d.match_name)?,
        name: budget.owned(&d.name)?,
        control_type: d.control_type,
        last_value: copy_value(&d.last_value, budget)?,
        default_value: copy_value(&d.default_value, budget)?,
        min_value: d.min_value,
        max_value: d.max_value,
        option_count: d.option_count,
        enum_options: options,
        decode_status: d.decode_status,
        source: d.source,
    })
}
