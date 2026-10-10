use crate::{Error, binary as b, error::Budget, model::*, rifx::Chunk};

pub(crate) fn span(c: &Chunk<'_>) -> RawSpan {
    RawSpan {
        offset: c.offset,
        length: c
            .data
            .len()
            .saturating_add(if c.form.is_some() { 12 } else { 8 }),
    }
}
fn ratio(d: &[u8], o: usize, signed: bool) -> Option<Ratio> {
    Some(Ratio {
        numerator: if signed {
            i64::from(b::i32(d, o)?)
        } else {
            i64::from(b::u32(d, o)?)
        },
        denominator: b::u32(d, o.checked_add(4)?)?,
    })
}
fn name(c: &Chunk<'_>, budget: &mut Budget<'_>) -> Result<String, Error> {
    if let Some(n) = c.child(b"Utf8") {
        budget.text(n.data)
    } else {
        Ok(String::new())
    }
}
pub(crate) fn diag(
    d: &mut Vec<Diagnostic>,
    budget: &mut Budget<'_>,
    code: &'static str,
    offset: usize,
    message: &str,
) -> Result<(), Error> {
    let message = budget.owned(message)?;
    budget.push(
        d,
        Diagnostic {
            code,
            offset,
            message,
        },
    )
}

pub(crate) fn project(root: &Chunk<'_>, budget: &mut Budget<'_>) -> Result<Project, Error> {
    let color_depth_code = root
        .child(b"nhed")
        .and_then(|c| c.data.get(15))
        .or_else(|| root.child(b"nnhd").and_then(|c| c.data.get(24)))
        .copied();
    let mut p = Project {
        bits_per_channel: match color_depth_code {
            Some(0) => Some(8),
            Some(1) => Some(16),
            Some(2) => Some(32),
            _ => None,
        },
        color_depth_code,
        ..Project::default()
    };
    if color_depth_code.is_some() && p.bits_per_channel.is_none() {
        diag(
            &mut p.diagnostics,
            budget,
            "unknown-color-depth",
            root.offset,
            "unknown project bit-depth code remains raw",
        )?;
    }
    p.settings = crate::metadata::project(root, budget, &mut p.diagnostics)?;
    p.effect_definitions = crate::effects::catalog(root, budget, &mut p.diagnostics)?;
    if let Some(effects) = root.list(b"Pefl") {
        for c in effects.children.iter().filter(|c| c.id == *b"pjef") {
            let name = budget.text(c.data)?;
            budget.push(&mut p.effect_names, name)?;
        }
    }
    if let Some(fold) = root.list(b"Fold") {
        items(fold, 0, &mut p, budget)?;
    }
    p.render_queue = crate::queue::decode(root, budget, &mut p.diagnostics)?;
    Ok(p)
}
fn items(
    container: &Chunk<'_>,
    parent: u32,
    p: &mut Project,
    budget: &mut Budget<'_>,
) -> Result<(), Error> {
    for (order, c) in container
        .children
        .iter()
        .filter(|c| c.form == Some(*b"Item"))
        .enumerate()
    {
        let data = c.child(b"idta").map(|n| n.data).unwrap_or_default();
        let (raw_type, id) = match (b::u16(data, 0), b::u32(data, 16)) {
            (Some(t), Some(id)) => (t, id),
            _ => {
                diag(
                    &mut p.diagnostics,
                    budget,
                    "invalid-item",
                    c.offset,
                    "item has missing or truncated idta",
                )?;
                (0, 0)
            }
        };
        let kind = match raw_type {
            1 => ItemKind::Folder,
            4 => ItemKind::Composition,
            7 => ItemKind::Footage,
            _ => ItemKind::Unknown,
        };
        let item_name = name(c, budget)?;
        match kind {
            ItemKind::Composition => {
                let comp = composition(c, id, budget, &mut p.diagnostics)?;
                budget.push(&mut p.compositions, comp)?;
            }
            ItemKind::Footage => {
                let f = footage(c, id, budget, &mut p.diagnostics)?;
                budget.push(&mut p.footage, f)?;
            }
            ItemKind::Unknown => diag(
                &mut p.diagnostics,
                budget,
                "unsupported-item",
                c.offset,
                "unknown item type remains in the raw tree",
            )?,
            _ => {}
        }
        let comment = match c.child(b"cmta") {
            Some(c) => budget.text(c.data)?,
            None => String::new(),
        };
        budget.push(
            &mut p.items,
            Item {
                id,
                parent_id: parent,
                order,
                name: item_name,
                comment,
                kind,
                raw_type,
                label: data.get(0x3a).copied(),
                source: span(c),
            },
        )?;
        if kind == ItemKind::Folder {
            if let Some(sub) = c.list(b"Sfdr") {
                items(sub, id, p, budget)?;
            }
        }
    }
    Ok(())
}
fn composition(
    c: &Chunk<'_>,
    id: u32,
    budget: &mut Budget<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Composition, Error> {
    let d = c.child(b"cdta").map(|n| n.data).unwrap_or_default();
    if d.len() < 0xcc {
        diag(
            diagnostics,
            budget,
            "incomplete-composition",
            c.offset,
            "cdta is shorter than the AE2020 layout",
        )?;
    }
    let frame_rate_fixed = b::u32(d, 0x9c);
    let frame_rate = frame_rate_fixed
        .map(|n| f64::from(n) / 65536.0)
        .filter(|n| *n > 0.0);
    let ticks = b::u32(d, 8).filter(|n| *n > 0);
    if ticks.is_none() {
        diag(
            diagnostics,
            budget,
            "legacy-timebase",
            c.offset,
            "missing keyframe timebase: using legacy 8000 ticks per second",
        )?;
    }
    let mut comp = Composition {
        id,
        name: name(c, budget)?,
        width: b::u16(d, 0x8c),
        height: b::u16(d, 0x8e),
        frame_rate,
        frame_rate_fixed,
        pixel_aspect: ratio(d, 0x90, false),
        duration_seconds: None,
        display_start: ratio(d, 0xa4, true),
        work_area_start: ratio(d, 0x1c, false),
        work_area_end: ratio(d, 0x24, false),
        work_area_uses_duration: b::u32(d, 0x24) == Some(u32::MAX),
        background_rgb: d.get(0x34..0x37).and_then(|s| s.try_into().ok()),
        keyframe_ticks_per_second: ticks.unwrap_or(8000),
        shutter_angle: b::u16(d, 0xae),
        shutter_phase: b::i32(d, 0xb4),
        renderer: None,
        resolution_factor: (d.len() >= 4)
            .then_some([b::u16(d, 0).unwrap_or(0), b::u16(d, 2).unwrap_or(0)]),
        motion_blur_adaptive_sample_limit: b::i32(d, 0xc4),
        motion_blur_samples_per_frame: b::i32(d, 0xc8),
        markers: Vec::new(),
        guides: Vec::new(),
        essential_graphics: crate::panels::decode(c, budget, diagnostics)?,
        layers: Vec::new(),
        source: span(c),
    };
    if comp.work_area_uses_duration {
        comp.work_area_end = None;
    }
    comp.duration_seconds = ratio(d, 0x2c, false).and_then(|r| r.value());
    if let (None, Some(fps)) = (comp.duration_seconds, frame_rate) {
        diag(
            diagnostics,
            budget,
            "composition-duration-fallback",
            c.offset,
            "missing or zero saved duration denominator; using legacy nominal/frame-count fallback",
        )?;
        let nominal = b::u16(d, 6)
            .map(|n| f64::from(n) * fps.round())
            .filter(|n| *n > 0.0);
        comp.duration_seconds = match (b::u32(d, 0x2c).filter(|n| *n > 0), nominal) {
            (Some(t), Some(rate)) => Some(f64::from(t) / rate),
            _ => b::u32(d, 0xb0).map(|n| f64::from(n) / fps),
        };
    }
    if let Some(prin) = c.list(b"PRin").and_then(|n| n.child(b"prin")) {
        if let Some(s) = prin.data.get(4..) {
            comp.renderer = Some(budget.text(s)?);
        }
    }
    for (index, l) in c
        .children
        .iter()
        .filter(|l| l.form == Some(*b"Layr"))
        .enumerate()
    {
        let decoded = layer(
            l,
            index,
            comp.keyframe_ticks_per_second,
            budget,
            diagnostics,
        )?;
        budget.push(&mut comp.layers, decoded)?;
    }
    comp.markers =
        crate::metadata::comp_markers(c, comp.keyframe_ticks_per_second, budget, diagnostics)?;
    comp.guides = crate::metadata::guides(c, budget, diagnostics)?;
    for layer in &mut comp.layers {
        crate::defaults::assign(layer, comp.width, comp.height);
    }
    Ok(comp)
}
fn layer(
    c: &Chunk<'_>,
    index: usize,
    tick_rate: u32,
    budget: &mut Budget<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Layer, Error> {
    let d = c.child(b"ldta").map(|n| n.data).unwrap_or_default();
    if d.len() < 0xa0 {
        diag(
            diagnostics,
            budget,
            "incomplete-layer",
            c.offset,
            "ldta is shorter than the AE2020 layout",
        )?;
    }
    let switches = d
        .get(0x25..0x28)
        .and_then(|v| <[u8; 3]>::try_from(v).ok())
        .map(|[a, b, c]| Switches {
            visible: c & 1 != 0,
            audio: c & 2 != 0,
            effects: c & 4 != 0,
            motion_blur: c & 8 != 0,
            frame_blending: c & 16 != 0,
            locked: c & 32 != 0,
            shy: c & 64 != 0,
            collapse_transformations: c & 128 != 0,
            adjustment: b & 2 != 0,
            three_d: b & 4 != 0,
            solo: b & 8 != 0,
            null: b & 128 != 0,
            markers_locked: b & 16 != 0,
            guide: a & 2 != 0,
            bicubic: a & 64 != 0,
            pixel_motion: a & 4 != 0,
            raw: [a, b, c],
        });
    let mut kind = match d.get(0x83) {
        Some(1) => LayerKind::Light,
        Some(2) => LayerKind::Camera,
        Some(5) => LayerKind::Model3d,
        _ => LayerKind::Av,
    };
    if kind == LayerKind::Av {
        if switches.as_ref().is_some_and(|s| s.adjustment) {
            kind = LayerKind::Adjustment;
        } else if switches.as_ref().is_some_and(|s| s.null) || b::u32(d, 0x28) == Some(0) {
            kind = LayerKind::Null;
        }
    }
    let mut properties = Vec::new();
    for group in c.children.iter().filter(|n| n.form == Some(*b"tdgp")) {
        nodes(group, &mut properties, tick_rate, budget, diagnostics)?;
    }
    let text_source = if let Some(source) = c.find_list(b"btds") {
        kind = LayerKind::Text;
        match crate::text::decode(source.data, budget) {
            Ok(text) => {
                diag(
                    diagnostics,
                    budget,
                    "partial-text-style",
                    source.offset,
                    "static text and checked run ranges decoded; source-text animation remains unverified",
                )?;
                Some(text)
            }
            Err(Error::InvalidFormat { .. }) => {
                diag(
                    diagnostics,
                    budget,
                    "unsupported-text",
                    source.offset,
                    "text-engine payload is missing or malformed",
                )?;
                None
            }
            Err(e) => return Err(e),
        }
    } else {
        None
    };
    if properties
        .iter()
        .any(|n| n.find("ADBE Root Vectors Group").is_some())
    {
        kind = LayerKind::Shape;
    }
    annotate_nodes(&mut properties, 0);
    let markers = match c.find_list(b"mrst") {
        Some(m) => crate::metadata::markers(m, tick_rate, budget, diagnostics)?,
        None => Vec::new(),
    };
    Ok(Layer {
        id: b::u32(d, 0).unwrap_or(0),
        index,
        name: name(c, budget)?,
        comment: match c.child(b"cmta") {
            Some(n) => budget.text(n.data)?,
            None => String::new(),
        },
        kind,
        source_id: b::u32(d, 0x28),
        parent_id: b::u32(d, 0x84),
        start_time: ratio(d, 0xc, true),
        source_in_point: ratio(d, 0x14, true),
        source_out_point: ratio(d, 0x1c, true),
        stretch: match (b::i32(d, 8), b::u32(d, 0x6c)) {
            (Some(n), Some(den)) => Some(Ratio {
                numerator: i64::from(n),
                denominator: den,
            }),
            _ => None,
        },
        label: d.get(0x3d).copied(),
        quality: b::u16(d, 4),
        blend_mode: d.get(0x63).copied(),
        track_matte_mode: d.get(0x6b).copied(),
        track_matte_layer_id: b::u32(d, 0xa0),
        light_kind: if kind == LayerKind::Light {
            b::u32(d, 0x88)
        } else {
            None
        },
        preserve_transparency: d.get(0x67).map(|n| *n != 0),
        alternate_source_id: alternate_source(c),
        auto_orient: d.get(0x25..0x28).and_then(|v| {
            Some(if v.first()? & 16 != 0 {
                AutoOrient::CharactersTowardCamera
            } else if v.get(1)? & 32 != 0 {
                AutoOrient::CameraOrPointOfInterest
            } else if v.get(1)? & 1 != 0 {
                AutoOrient::AlongPath
            } else {
                AutoOrient::None
            })
        }),
        markers,
        switches,
        properties,
        text_source,
        source: span(c),
    })
}
fn footage(
    c: &Chunk<'_>,
    id: u32,
    budget: &mut Budget<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Footage, Error> {
    let pin = c.list(b"Pin ").unwrap_or(c);
    let d = pin.child(b"sspc").map(|n| n.data).unwrap_or_default();
    let opt = pin.child(b"opti").map(|n| n.data).unwrap_or_default();
    let tag = opt.get(..4);
    let mut f = Footage {
        id,
        name: name(c, budget)?,
        path: None,
        width: b::u16(d, if d.len() >= 38 { 0x20 } else { 0 }),
        height: b::u16(d, if d.len() >= 38 { 0x24 } else { 2 }),
        format_tag: match tag {
            Some(t) => Some(budget.text(t)?),
            None => None,
        },
        solid_color: None,
        is_placeholder: tag == Some(b"Plac"),
        is_solid: tag == Some(b"Soli"),
        missing_at_save: d
            .get(0x78)
            .map(|n| tag != Some(b"Soli") && tag != Some(b"Plac") && *n != 0),
        audio_sample_rate: b::f64(d, 0xa0, false).map(Number::from),
        start_frame: b::u32(d, 0xac),
        end_frame: b::u32(d, 0xb0),
        sequence_members: Vec::new(),
        source: span(c),
    };
    if tag == Some(b"Soli") {
        if let (Some(r), Some(g), Some(b)) =
            (b::f32(opt, 0xe), b::f32(opt, 0x12), b::f32(opt, 0x16))
        {
            f.solid_color = Some([r.into(), g.into(), b.into()]);
        }
    }
    if let Some(path) = pin.child(b"Cpth") {
        f.path = Some(budget.text(path.data)?);
    }
    if let Some(alias) = c.find(b"alas") {
        // JSON string extraction preserves escaping without importing a JSON runtime.
        let alias_value = match crate::json::parse(alias.data, budget) {
            Ok(j) => j
                .get("fullpath")
                .and_then(crate::json::Json::string)
                .map(|s| budget.owned(s))
                .transpose()?,
            Err(Error::InvalidFormat { .. }) => None,
            Err(e) => return Err(e),
        };
        match alias_value {
            Some(path) => f.path = Some(path),
            None => diag(
                diagnostics,
                budget,
                "unsupported-footage-alias",
                alias.offset,
                "alias has no supported fullpath string; relinking may be needed",
            )?,
        }
    }
    if let Some(seq) = pin.list(b"StVc") {
        for s in seq.children.iter().filter(|s| s.id == *b"Utf8") {
            let member = budget.text(s.data)?;
            budget.push(&mut f.sequence_members, member)?;
        }
    }
    Ok(f)
}

fn nodes(
    c: &Chunk<'_>,
    out: &mut Vec<PropertyNode>,
    ticks: u32,
    budget: &mut Budget<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), Error> {
    let mut i = 0;
    while let Some(ch) = c.children.get(i) {
        i += 1;
        if ch.id != *b"tdmn" {
            continue;
        }
        let match_name = budget.text(ch.data)?;
        if match_name == "ADBE Group End" {
            break;
        }
        let mut mask = None;
        if match_name == "ADBE Mask Atom" {
            if let Some(m) = c.children.get(i).filter(|m| m.id == *b"mkif") {
                mask = Some(MaskInfo {
                    inverted: m.data.first().map(|n| *n != 0),
                    locked: m.data.get(1).map(|n| *n != 0),
                    mode: b::u32(m.data, 4),
                    index: b::u32(m.data, 8),
                    color: m.data.get(0x2d..0x30).and_then(|v| v.try_into().ok()),
                    motion_blur: m.data.get(2).copied(),
                    feather_falloff: m.data.get(3).copied(),
                });
                i += 1;
            }
        }
        let Some(payload) = c.children.get(i).filter(|n| n.id != *b"tdmn") else {
            diag(
                diagnostics,
                budget,
                "orphan-property",
                ch.offset,
                "named property lacks a payload",
            )?;
            let name = budget.owned(&match_name)?;
            budget.push(
                out,
                PropertyNode {
                    enabled: None,
                    match_name,
                    name,
                    name_source: NameSource::MatchName,
                    kind: NodeKind::Opaque,
                    decode_status: DecodeStatus::Failed,
                    children: Vec::new(),
                    property: None,
                    mask,
                    index: 0,
                    depth: 0,
                    effect_parameters: Vec::new(),
                    source: span(ch),
                },
            )?;
            continue;
        };
        i += 1;
        let (name, name_source) = display_name(payload, &match_name, budget, diagnostics)?;
        let mut node = PropertyNode {
            enabled: payload
                .child(b"tdsb")
                .or_else(|| {
                    (payload.form == Some(*b"sspc"))
                        .then(|| payload.list(b"tdgp").and_then(|c| c.child(b"tdsb")))
                        .flatten()
                })
                .and_then(|c| b::u32(c.data, 0))
                .map(|flags| flags & 1 != 0),
            name,
            name_source,
            match_name,
            kind: NodeKind::Opaque,
            decode_status: DecodeStatus::Unsupported,
            children: Vec::new(),
            property: None,
            mask,
            index: 0,
            depth: 0,
            effect_parameters: Vec::new(),
            source: span(payload),
        };
        match payload.form.as_ref() {
            Some(b"tdbs") => {
                let (p, status) =
                    numeric_property(payload, ticks, false, None, budget, diagnostics)?;
                node.kind = NodeKind::Property;
                node.property = Some(p);
                node.decode_status = status;
            }
            Some(b"otst") => {
                if let Some(inner) = payload.list(b"tdbs") {
                    let (mut p, mut status) =
                        numeric_property(inner, ticks, true, Some(3), budget, diagnostics)?;
                    if !p.keyframes.is_empty() {
                        let mut values = payload
                            .list(b"otky")
                            .into_iter()
                            .flat_map(|n| &n.children)
                            .filter(|n| n.id == *b"otda");
                        for k in &mut p.keyframes {
                            k.value = match values.next() {
                                Some(v) => numbers(v.data, 0, 3, false, budget)?,
                                None => None,
                            };
                        }
                        status = DecodeStatus::Partial;
                        diag(
                            diagnostics,
                            budget,
                            "orientation-ease-unverified",
                            payload.offset,
                            "orientation values decoded; interpolation semantics require host verification",
                        )?;
                    }
                    node.kind = NodeKind::Property;
                    node.property = Some(p);
                    node.decode_status = status;
                }
            }
            Some(b"om-s") => {
                let (p, status) = path_property(payload, ticks, budget, diagnostics)?;
                node.kind = NodeKind::Property;
                node.property = Some(p);
                node.decode_status = status;
            }
            Some(b"GCst") => {
                node.kind = NodeKind::Property;
                let (p, status) = crate::gradient::property(payload, ticks, budget, diagnostics)?;
                node.property = Some(p);
                node.decode_status = status;
            }
            Some(b"btds") | Some(b"btdk") => {
                node.decode_status = DecodeStatus::Partial;
            }
            Some(b"mrst") => {
                node.kind = NodeKind::Group;
                node.decode_status = DecodeStatus::Partial;
            }
            _ => {
                wrapped_nodes(payload, &mut node.children, ticks, budget, diagnostics)?;
                if !node.children.is_empty() || payload.form == Some(*b"tdgp") {
                    node.kind = NodeKind::Group;
                    node.decode_status = if payload.form == Some(*b"tdgp") {
                        DecodeStatus::Decoded
                    } else {
                        DecodeStatus::Partial
                    };
                }
            }
        }
        if payload.list(b"parT").is_some() {
            crate::effects::apply(payload, &mut node, budget, diagnostics)?;
        }
        if node.decode_status == DecodeStatus::Unsupported {
            diag(
                diagnostics,
                budget,
                "unsupported-property",
                payload.offset,
                &node.match_name,
            )?;
        }
        budget.push(out, node)?;
    }
    Ok(())
}
fn wrapped_nodes(
    c: &Chunk<'_>,
    out: &mut Vec<PropertyNode>,
    ticks: u32,
    budget: &mut Budget<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), Error> {
    if c.opaque {
        return Ok(());
    }
    if c.children.iter().any(|n| n.id == *b"tdmn") {
        return nodes(c, out, ticks, budget, diagnostics);
    }
    for child in &c.children {
        // parT defines the effect UI; its tdmn/pard pairs are not values.
        if child.form.is_some() && child.form != Some(*b"parT") {
            wrapped_nodes(child, out, ticks, budget, diagnostics)?;
        }
    }
    Ok(())
}
fn display_name(
    c: &Chunk<'_>,
    fallback: &str,
    budget: &mut Budget<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(String, NameSource), Error> {
    for (id, source) in [
        (b"tdsn", NameSource::InstanceName),
        (b"fnam", NameSource::EffectInstanceName),
    ] {
        let Some(n) = c.child(id) else {
            continue;
        };
        let text = (n.data.get(..4) == Some(b"Utf8"))
            .then(|| b::u32(n.data, 4))
            .flatten()
            .and_then(|size| usize::try_from(size).ok())
            .and_then(|size| n.data.get(8..8usize.checked_add(size)?));
        if let Some(text) = text {
            let name = budget.text(text)?;
            // AE persists this marker for the default UI name. It is not a rename.
            if name == "-_0_/-" {
                continue;
            }
            return Ok((name, source));
        }
        diag(
            diagnostics,
            budget,
            "invalid-property-name",
            n.offset,
            "embedded name record is malformed; match-name fallback remains available",
        )?;
    }
    Ok((budget.owned(fallback)?, NameSource::MatchName))
}
fn numbers(
    d: &[u8],
    offset: usize,
    dims: usize,
    little: bool,
    budget: &mut Budget<'_>,
) -> Result<Option<Value>, Error> {
    let mut v = Vec::new();
    for i in 0..dims {
        let Some(n) = b::f64(d, offset.saturating_add(i.saturating_mul(8)), little) else {
            return Ok(None);
        };
        budget.push(&mut v, n.into())?;
    }
    Ok(Some(Value::Numbers(v)))
}
fn numeric_property(
    c: &Chunk<'_>,
    ticks: u32,
    little: bool,
    override_dims: Option<u8>,
    budget: &mut Budget<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(Property, DecodeStatus), Error> {
    let desc = c.child(b"tdb4").or_else(|| c.child(b"Tdb4"));
    let code = desc.and_then(|d| d.data.get(3)).copied();
    let dims = override_dims.or(match code {
        Some(1) => Some(1),
        Some(2) => Some(2),
        Some(3) | Some(7) => Some(3),
        Some(4) => Some(4),
        _ => None,
    });
    let mut p = Property {
        storage_dimensions: dims,
        spatial: desc
            .and_then(|d| d.data.get(5))
            .map(|n| n & 8 != 0)
            .unwrap_or(code == Some(7)),
        static_value: None,
        keyframes: Vec::new(),
        expression: None,
        expression_enabled: None,
        layer_reference_id: c.child(b"tdpi").and_then(|n| b::u32(n.data, 0)),
        metadata: crate::metadata::property(c, dims, budget)?,
        effect_definition: None,
        inferred_default: None,
        gradient_snapshots: Vec::new(),
        unmatched_path_snapshots: Vec::new(),
    };
    if let Some(expr) = c.child(b"Utf8") {
        p.expression = Some(budget.text(expr.data)?);
        p.expression_enabled = desc.and_then(|d| d.data.get(0x77)).map(|n| *n == 0);
    }
    let mut status = DecodeStatus::Decoded;
    if let Some(list) = c.list(b"list") {
        let (keys, complete) = keyframes(
            list,
            ticks,
            dims.map(usize::from),
            false,
            budget,
            diagnostics,
        )?;
        p.keyframes = keys;
        if !complete {
            status = DecodeStatus::Partial;
        }
    } else if let (Some(cdat), Some(dims)) = (c.child(b"cdat"), dims) {
        p.static_value = numbers(cdat.data, 0, usize::from(dims), little, budget)?;
        if p.static_value.is_none() {
            status = DecodeStatus::Partial;
        }
    } else if p.metadata.no_value == Some(true) {
        // A descriptor explicitly declaring no value is meaningful metadata.
    } else {
        status = DecodeStatus::Unsupported;
    }
    if dims.is_none() && p.metadata.no_value != Some(true) {
        status = DecodeStatus::Unsupported;
    }
    if status != DecodeStatus::Decoded {
        diag(
            diagnostics,
            budget,
            "incomplete-value",
            c.offset,
            "property value layout is absent, unknown or truncated",
        )?;
    }
    Ok((p, status))
}
pub(crate) fn table<'a>(c: &'a Chunk<'_>) -> Option<(usize, usize, &'a [u8])> {
    let hdr = c.child(b"lhd3")?.data;
    let count = usize::try_from(b::u32(hdr, 8)?).ok()?;
    let stride = usize::try_from(b::u32(hdr, 16)?).ok()?;
    let data = c.child(b"ldat")?.data;
    if stride < 8 || count.checked_mul(stride)? > data.len() {
        return None;
    }
    Some((count, stride, data))
}
fn keyframes(
    c: &Chunk<'_>,
    ticks: u32,
    dims: Option<usize>,
    path_times: bool,
    budget: &mut Budget<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(Vec<Keyframe>, bool), Error> {
    let mut keys = Vec::new();
    let Some((count, stride, data)) = table(c) else {
        diag(
            diagnostics,
            budget,
            "invalid-keyframes",
            c.offset,
            "keyframe table count, stride or bytes are inconsistent",
        )?;
        return Ok((keys, false));
    };
    budget.keyframes = budget
        .keyframes
        .checked_add(count)
        .ok_or(Error::Allocation)?;
    Budget::check(
        "keyframe count",
        budget.keyframes,
        budget.limits.max_keyframes,
    )?;
    let mut complete = dims.is_some() || path_times;
    for i in 0..count {
        let offset = i.checked_mul(stride).ok_or(Error::Allocation)?;
        let blk = data
            .get(offset..offset.checked_add(stride).ok_or(Error::Allocation)?)
            .ok_or(Error::Allocation)?;
        let time = b::u32(blk, 0).ok_or(Error::Allocation)?;
        let style = blk.get(7).copied().unwrap_or(0);
        let mut k = Keyframe {
            time_ticks: time,
            time_seconds: f64::from(time) / f64::from(ticks),
            value: None,
            in_interpolation: blk.get(4).copied().unwrap_or(0),
            out_interpolation: blk.get(5).copied().unwrap_or(0),
            in_ease: Vec::new(),
            out_ease: Vec::new(),
            in_spatial_tangent: Vec::new(),
            out_spatial_tangent: Vec::new(),
            raw_flags: [blk.get(6).copied().unwrap_or(0), style],
        };
        // Native records with flags 15/31 retain the same spatial payload layout
        // as 7. High bits are preserved in raw_flags, not interpreted here.
        let spatial = path_times
            || matches!(style, 7 | 15 | 31)
            || (style == 1 && dims.is_some_and(|n| n >= 2));
        if let Some(n) = dims {
            k.value = numbers(blk, if spatial { 0x38 } else { 8 }, n, false, budget)?;
            if k.value.is_none() {
                complete = false;
            }
        }
        let n = dims.unwrap_or(1);
        let required = if spatial {
            if path_times { 0x38 } else { 0x38 + 3 * n * 8 }
        } else {
            8 + 5 * n * 8
        };
        if blk.len() >= required {
            let ease_dims = if spatial { 1 } else { n };
            for j in 0..ease_dims {
                let ins = if spatial { 0x18 } else { 8 + (n + j) * 8 };
                let inf = if spatial { 0x20 } else { 8 + (2 * n + j) * 8 };
                let outs = if spatial { 0x28 } else { 8 + (3 * n + j) * 8 };
                let outf = if spatial { 0x30 } else { 8 + (4 * n + j) * 8 };
                if let (Some(s), Some(f)) = (b::f64(blk, ins, false), b::f64(blk, inf, false)) {
                    budget.push(
                        &mut k.in_ease,
                        TemporalEase {
                            speed: s.into(),
                            influence: f.into(),
                        },
                    )?;
                }
                if let (Some(s), Some(f)) = (b::f64(blk, outs, false), b::f64(blk, outf, false)) {
                    budget.push(
                        &mut k.out_ease,
                        TemporalEase {
                            speed: s.into(),
                            influence: f.into(),
                        },
                    )?;
                }
            }
            if spatial && !path_times {
                for j in 0..n {
                    if let Some(v) = b::f64(blk, 0x38 + (n + j) * 8, false) {
                        budget.push(&mut k.in_spatial_tangent, v.into())?;
                    }
                    if let Some(v) = b::f64(blk, 0x38 + (2 * n + j) * 8, false) {
                        budget.push(&mut k.out_spatial_tangent, v.into())?;
                    }
                }
            }
        } else {
            complete = false;
        }
        budget.push(&mut keys, k)?;
    }
    if !complete {
        diag(
            diagnostics,
            budget,
            "incomplete-keyframes",
            c.offset,
            "keyframe values or easing are not fully decoded",
        )?;
    }
    Ok((keys, complete))
}
fn path_property(
    c: &Chunk<'_>,
    ticks: u32,
    budget: &mut Budget<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(Property, DecodeStatus), Error> {
    let mut p = Property {
        storage_dimensions: None,
        spatial: false,
        static_value: None,
        keyframes: Vec::new(),
        expression: None,
        expression_enabled: None,
        layer_reference_id: None,
        metadata: c
            .list(b"tdbs")
            .map(|n| crate::metadata::property(n, None, budget))
            .transpose()?
            .unwrap_or_default(),
        effect_definition: None,
        inferred_default: None,
        gradient_snapshots: Vec::new(),
        unmatched_path_snapshots: Vec::new(),
    };
    p.metadata.value_type = PropertyValueType::Custom;
    if let Some(inner) = c.list(b"tdbs") {
        if let Some(expr) = inner.child(b"Utf8") {
            p.expression = Some(budget.text(expr.data)?);
            p.expression_enabled = inner
                .child(b"tdb4")
                .or_else(|| inner.child(b"Tdb4"))
                .and_then(|c| c.data.get(0x77))
                .map(|n| *n == 0);
        }
    }
    let shapes = c
        .list(b"omks")
        .map(|n| n.children.iter().filter(|n| n.form == Some(*b"shap")));
    let mut complete = true;
    let time_table = c.list(b"tdbs").and_then(|n| n.list(b"list"));
    if let Some(t) = time_table {
        let (keys, ok) = keyframes(t, ticks, None, true, budget, diagnostics)?;
        p.keyframes = keys;
        complete = ok;
    }
    let expected_shapes = c.list(b"omks").map_or(0, |n| {
        n.children
            .iter()
            .filter(|n| n.form == Some(*b"shap"))
            .count()
    });
    let associated = if time_table.is_some() {
        expected_shapes == p.keyframes.len() && expected_shapes != 0
    } else {
        expected_shapes == 1
    };
    if !associated {
        complete = false;
        diag(
            diagnostics,
            budget,
            "path-snapshot-count-mismatch",
            c.offset,
            "path snapshots do not match the time table or a single static value; all snapshots retained without invented associations",
        )?;
    }
    let mut shape_count = 0;
    if let Some(shapes) = shapes {
        for shape in shapes {
            let value = path(shape, budget, diagnostics)?;
            if value.is_none() {
                complete = false;
            }
            if !associated {
                budget.push(
                    &mut p.unmatched_path_snapshots,
                    PathSnapshot {
                        path: value,
                        source: span(shape),
                    },
                )?;
            } else if time_table.is_none() && shape_count == 0 {
                p.static_value = value.map(Value::Path);
            } else if let Some(k) = p.keyframes.get_mut(shape_count) {
                k.value = value.map(Value::Path);
            } else {
                complete = false;
            }
            shape_count += 1;
        }
    }
    if shape_count == 0 || (!p.keyframes.is_empty() && shape_count != p.keyframes.len()) {
        complete = false;
    }
    Ok((
        p,
        if complete {
            DecodeStatus::Decoded
        } else {
            DecodeStatus::Partial
        },
    ))
}
fn path(
    c: &Chunk<'_>,
    budget: &mut Budget<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Option<BezierPath>, Error> {
    let header = c.child(b"shph").map(|n| n.data);
    let closed = header.and_then(|h| h.get(3)).and_then(|n| match n {
        1 => Some(true),
        0 | 9 => Some(false),
        _ => None,
    });
    let bounds = header.and_then(|h| {
        Some([
            b::f32(h, 4)?.into(),
            b::f32(h, 8)?.into(),
            b::f32(h, 12)?.into(),
            b::f32(h, 16)?.into(),
        ])
    });
    let mut vertices = Vec::new();
    let Some((count, stride, data)) = c.list(b"list").and_then(table) else {
        diag(
            diagnostics,
            budget,
            "invalid-path",
            c.offset,
            "path vertex table is missing or invalid",
        )?;
        return Ok(None);
    };
    if stride != 8 || count % 3 != 0 {
        diag(
            diagnostics,
            budget,
            "invalid-path",
            c.offset,
            "path needs three float32 pairs per vertex",
        )?;
        return Ok(None);
    }
    for i in 0..count / 3 {
        let off = i.checked_mul(24).ok_or(Error::Allocation)?;
        let xy = |o| -> Option<[Number; 2]> {
            Some([
                b::f32(data, o)?.into(),
                b::f32(data, o.checked_add(4)?)?.into(),
            ])
        };
        // Disk segments are [anchor_i, outgoing_i, incoming_(i+1)], wrapping
        // even on open paths. Reassemble this vertex's incoming control point
        // from the preceding segment; all values stay bbox-normalized.
        let previous = i.checked_sub(1).unwrap_or(count / 3 - 1);
        let incoming = previous
            .checked_mul(24)
            .and_then(|n| n.checked_add(16))
            .ok_or(Error::Allocation)?;
        let (Some(a), Some(input), Some(output)) = (xy(off), xy(incoming), xy(off + 8)) else {
            return Ok(None);
        };
        budget.push(
            &mut vertices,
            Vertex {
                anchor: a,
                in_tangent: input,
                out_tangent: output,
            },
        )?;
    }
    Ok(Some(BezierPath {
        closed,
        bounds,
        vertices,
    }))
}

fn annotate_nodes(nodes: &mut [PropertyNode], depth: usize) {
    for (index, node) in nodes.iter_mut().enumerate() {
        node.index = index;
        node.depth = depth;
        if let Some(p) = &mut node.property {
            p.metadata.separation_leader = node.match_name == "ADBE Position";
            p.metadata.separation_dimension = match node.match_name.as_str() {
                "ADBE Position_0" => Some(0),
                "ADBE Position_1" => Some(1),
                "ADBE Position_2" => Some(2),
                _ => None,
            };
        }
        annotate_nodes(&mut node.children, depth + 1);
    }
}
fn alternate_source(c: &Chunk<'_>) -> Option<u32> {
    let tdgp = c.list(b"tdgp")?;
    for run in tdgp.children.windows(3) {
        let (Some(name), Some(header), Some(overrides)) = (run.first(), run.get(1), run.get(2))
        else {
            continue;
        };
        if name.id != *b"tdmn"
            || name.text() != "ADBE Layer Overrides"
            || header.form != Some(*b"OvG2")
            || overrides.form != Some(*b"tdgp")
        {
            continue;
        }
        for slot in overrides.children.windows(3) {
            let (Some(name), Some(present), Some(value)) = (slot.first(), slot.get(1), slot.get(2))
            else {
                continue;
            };
            if name.id == *b"tdmn"
                && name.text() == "ADBE Layer Source Alternate"
                && present.id == *b"blsv"
                && value.id == *b"blsi"
            {
                return b::u32(value.data, 0);
            }
        }
    }
    None
}
