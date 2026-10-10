use crate::{
    Error, binary as b,
    decode::{diag, span},
    error::Budget,
    json,
    model::*,
    rifx::Chunk,
};

pub(crate) fn project(
    root: &Chunk<'_>,
    budget: &mut Budget<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<ProjectSettings, Error> {
    let head = root.child(b"head").map(|c| c.data).unwrap_or_default();
    let n = root.child(b"nnhd").map(|c| c.data).unwrap_or_default();
    let old = root.child(b"nhed").map(|c| c.data).unwrap_or_default();
    let byte = |new, legacy| n.get(new).or_else(|| old.get(legacy)).copied();
    let mut s = ProjectSettings {
        version_word: b::u32(head, 4),
        revision: b::u16(head, 18),
        compensate_for_scene_referred_profiles: root
            .child(b"acer")
            .and_then(|c| c.data.first())
            .map(|n| *n != 0),
        audio_sample_rate: root
            .child(b"adfr")
            .and_then(|c| b::f64(c.data, 0, false))
            .map(Number::from),
        working_gamma: root
            .child(b"dwga")
            .and_then(|c| c.data.first())
            .map(|n| Number::from(if *n == 0 { 2.2 } else { 2.4 })),
        time_display_type: byte(8, 8).map(|n| n & 0x7f),
        footage_timecode_display_start_type: byte(9, 9),
        frames_use_feet_frames: byte(11, 11).map(|n| n & 1 != 0),
        timecode_default_base: b::u16(n, 14).or_else(|| old.get(12).map(|n| u16::from(*n))),
        feet_frames_per_foot: b::u32(n, 16).or_else(|| old.get(13).map(|n| u32::from(*n))),
        frames_count_type: byte(20, 14),
        transparency_grid_thumbnails: n.get(25).map(|n| *n != 0),
        linear_blending: root.child(b"lnrb").is_some(),
        linearize_working_space: root.child(b"lnrp").is_some(),
        ..ProjectSettings::default()
    };
    for (form, output) in [
        (b"gpuG", &mut s.gpu_accel_type),
        (b"ExEn", &mut s.expression_engine),
    ] {
        if let Some(c) = root.list(form).and_then(|c| c.child(b"Utf8")) {
            *output = Some(budget.text(c.data)?);
        }
    }
    for c in root.children.iter().filter(|c| c.id == *b"Utf8") {
        if ![
            b"colorManagementSystem".as_slice(),
            b"lutInterpolationMethod".as_slice(),
        ]
        .iter()
        .any(|k| c.data.windows(k.len()).any(|w| w == *k))
        {
            continue;
        }
        match json::parse(c.data, budget) {
            Ok(j) => {
                let text = |v: Option<&json::Json>,
                            budget: &mut Budget<'_>|
                 -> Result<Option<String>, Error> {
                    v.and_then(json::Json::string)
                        .map(|s| budget.owned(s))
                        .transpose()
                };
                s.color_management = Some(ColorManagement {
                    system: j
                        .get("colorManagementSystem")
                        .and_then(json::Json::number)
                        .map(Number::from),
                    lut_interpolation_method: j
                        .get("lutInterpolationMethod")
                        .and_then(json::Json::number)
                        .map(Number::from),
                    ocio_configuration_file: text(j.get("ocioConfigurationFile"), budget)?,
                    working_space: text(
                        j.get("baseColorProfile")
                            .and_then(|v| v.get("colorProfileName")),
                        budget,
                    )?,
                });
            }
            Err(Error::InvalidFormat { .. }) => diag(
                diagnostics,
                budget,
                "invalid-color-settings",
                c.offset,
                "malformed CMS JSON remains raw",
            )?,
            Err(e) => return Err(e),
        }
        break;
    }
    // Missing optional settings have no fabricated default value. Named fields
    // from short scalar chunks likewise remain None and are diagnostic.
    for (id, need) in [(b"head", 20), (b"adfr", 8), (b"acer", 1), (b"dwga", 1)] {
        if let Some(c) = root.child(id).filter(|c| c.data.len() < need) {
            diag(
                diagnostics,
                budget,
                "incomplete-project-setting",
                c.offset,
                "project setting payload is truncated",
            )?;
        }
    }
    Ok(s)
}

pub(crate) fn markers(
    c: &Chunk<'_>,
    ticks: u32,
    budget: &mut Budget<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<Marker>, Error> {
    let mut result = Vec::new();
    let list = c.list(b"tdbs").and_then(|c| c.list(b"list"));
    let Some((header, data)) =
        list.and_then(|c| Some((c.child(b"lhd3")?.data, c.child(b"ldat")?.data)))
    else {
        diag(
            diagnostics,
            budget,
            "invalid-markers",
            c.offset,
            "marker table is missing",
        )?;
        return Ok(result);
    };
    let (Some(count), Some(stride)) = (b::u32(header, 8), b::u32(header, 16)) else {
        diag(
            diagnostics,
            budget,
            "invalid-markers",
            c.offset,
            "marker table header is truncated",
        )?;
        return Ok(result);
    };
    let count = usize::try_from(count).map_err(|_| Error::Allocation)?;
    let stride = usize::try_from(stride).map_err(|_| Error::Allocation)?;
    if stride < 4 || count.checked_mul(stride).is_none_or(|n| n > data.len()) {
        diag(
            diagnostics,
            budget,
            "invalid-markers",
            c.offset,
            "marker count, stride and data length disagree",
        )?;
        return Ok(result);
    }
    budget.keyframes = budget
        .keyframes
        .checked_add(count)
        .ok_or(Error::Allocation)?;
    Budget::check(
        "keyframe count",
        budget.keyframes,
        budget.limits.max_keyframes,
    )?;
    let mut records = c
        .list(b"mrky")
        .into_iter()
        .flat_map(|c| &c.children)
        .filter(|c| c.form == Some(*b"Nmrd"));
    for i in 0..count {
        let time_ticks = b::u32(data, i.checked_mul(stride).ok_or(Error::Allocation)?)
            .ok_or(Error::Allocation)?;
        let record = records.next();
        let header = record.and_then(|c| c.child(b"NmHd"));
        let h = header.map(|c| c.data).unwrap_or_default();
        let mut texts = record
            .into_iter()
            .flat_map(|c| &c.children)
            .filter(|c| c.id == *b"Utf8");
        let mut next = || -> Result<String, Error> {
            texts
                .next()
                .map(|c| budget.text(c.data))
                .transpose()
                .map(|v| v.unwrap_or_default())
        };
        let m = Marker {
            time_ticks,
            time_seconds: f64::from(time_ticks) / f64::from(ticks),
            duration_seconds: b::u32(h, 8).map(|n| f64::from(n) / 600.0),
            duration_layout_verified: false,
            label: h.get(16).copied(),
            raw_flags: h.get(3).copied(),
            comment: next()?,
            chapter: next()?,
            url: next()?,
            frame_target: next()?,
            cue_point_name: next()?,
            source: record.map(span).unwrap_or_else(|| span(c)),
        };
        if header.is_some() {
            diag(
                diagnostics,
                budget,
                "marker-duration-layout-unverified",
                header.map(|c| c.offset).unwrap_or(c.offset),
                "NmHd duration follows Go @0x08; newer layout requires independent verification",
            )?;
        }
        budget.push(&mut result, m)?;
    }
    Ok(result)
}
pub(crate) fn comp_markers(
    item: &Chunk<'_>,
    ticks: u32,
    budget: &mut Budget<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<Marker>, Error> {
    for c in item.children.iter().filter(|c| c.form == Some(*b"SecL")) {
        if c.child(b"Utf8").is_some_and(|c| c.text() == "Markers") {
            if let Some(m) = c.find_list(b"mrst") {
                return markers(m, ticks, budget, diagnostics);
            }
        }
    }
    Ok(Vec::new())
}
pub(crate) fn guides(
    item: &Chunk<'_>,
    budget: &mut Budget<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<Guide>, Error> {
    let mut result = Vec::new();
    let Some(c) = item
        .list(b"Gide")
        .and_then(|c| c.list(b"list"))
        .and_then(|c| c.child(b"ldat"))
    else {
        return Ok(result);
    };
    if c.data.len() % 16 != 0 {
        diag(
            diagnostics,
            budget,
            "incomplete-guides",
            c.offset,
            "guide array ends in an incomplete record",
        )?;
    }
    for (i, d) in c.data.chunks_exact(16).enumerate() {
        budget.push(
            &mut result,
            Guide {
                orientation: b::u32(d, 0).ok_or(Error::Allocation)?,
                position_type: b::u32(d, 4).ok_or(Error::Allocation)?,
                position: b::f64(d, 8, false).ok_or(Error::Allocation)?.into(),
                source: RawSpan {
                    offset: c.offset + 8 + i * 16,
                    length: 16,
                },
            },
        )?;
    }
    Ok(result)
}
pub(crate) fn property(
    c: &Chunk<'_>,
    dims: Option<u8>,
    budget: &mut Budget<'_>,
) -> Result<PropertyMetadata, Error> {
    let d = c
        .child(b"tdb4")
        .or_else(|| c.child(b"Tdb4"))
        .map(|c| c.data)
        .unwrap_or_default();
    let s = c.child(b"tdsb").map(|c| c.data).unwrap_or_default();
    let bit = |o, mask| d.get(o).map(|n| n & mask != 0);
    let mut m = PropertyMetadata {
        storage_type_code: d.get(3).copied(),
        spatial: bit(5, 8),
        static_flag: bit(5, 1),
        animated: d.get(0x44).map(|n| *n != 0),
        color: bit(0x3b, 1),
        integer: bit(0x3b, 4),
        vector: bit(0x3b, 8),
        no_value: bit(0x39, 1),
        can_vary_over_time: bit(0xb, 2),
        locked_ratio: s.get(2).map(|n| n & 16 != 0),
        dimensions_separated: s.get(3).map(|n| n & 2 != 0),
        enabled: s.get(3).map(|n| n & 1 != 0),
        ..PropertyMetadata::default()
    };
    if c.child(b"tdpi").is_some_and(|c| c.data.len() >= 4) {
        m.control_type = Some(0);
        m.value_type = PropertyValueType::LayerIndex;
    } else if m.no_value == Some(true) {
        m.value_type = PropertyValueType::NoValue;
    }
    if m.control_type.is_none() {
        if m.color == Some(true) {
            m.control_type = Some(5);
            m.value_type = PropertyValueType::Color;
        } else if m.integer == Some(true) && dims == Some(1) {
            m.control_type = Some(4);
            m.value_type = PropertyValueType::OneD;
        } else if m.vector == Some(true) || m.integer == Some(true) {
            let spatial = m.spatial == Some(true);
            let (control, value) = match dims {
                Some(1) => (2, PropertyValueType::OneD),
                Some(2) => (
                    6,
                    if spatial {
                        PropertyValueType::TwoDSpatial
                    } else {
                        PropertyValueType::TwoD
                    },
                ),
                Some(3) => (
                    18,
                    if spatial {
                        PropertyValueType::ThreeDSpatial
                    } else {
                        PropertyValueType::ThreeD
                    },
                ),
                _ => (15, PropertyValueType::Unknown),
            };
            m.control_type = Some(control);
            m.value_type = value;
        }
    }
    m.min_value = range(c.child(b"tdum"), &m, budget)?;
    m.max_value = range(c.child(b"tduM"), &m, budget)?;
    Ok(m)
}
fn range(
    c: Option<&Chunk<'_>>,
    m: &PropertyMetadata,
    budget: &mut Budget<'_>,
) -> Result<Option<Value>, Error> {
    let Some(c) = c else {
        return Ok(None);
    };
    let mut values = Vec::new();
    if m.color == Some(true) {
        if c.data.len() < 16 {
            return Ok(None);
        }
        for i in 0..4 {
            budget.push(
                &mut values,
                b::f32(c.data, i * 4).ok_or(Error::Allocation)?.into(),
            )?;
        }
    } else if m.integer == Some(true) {
        let Some(n) = b::u32(c.data, 0) else {
            return Ok(None);
        };
        budget.push(&mut values, f64::from(n).into())?;
    } else {
        if c.data.is_empty() || c.data.len() % 8 != 0 {
            return Ok(None);
        }
        for d in c.data.chunks_exact(8) {
            budget.push(
                &mut values,
                b::f64(d, 0, false).ok_or(Error::Allocation)?.into(),
            )?;
        }
    }
    Ok(Some(Value::Numbers(values)))
}
