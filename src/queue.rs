//! Fixed-width saved render queue records. Unknown payloads remain in the raw tree.
use crate::{
    Error, binary as b,
    decode::{diag, span},
    error::Budget,
    model::*,
    rifx::Chunk,
};
const RENDER_SIZE: usize = 2246;
const OUTPUT_SIZE: usize = 128;

fn blocks<'a>(
    c: &'a Chunk<'_>,
    stride: usize,
    budget: &mut Budget<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Option<&'a Chunk<'a>>, Error> {
    // Lifetime of borrowed payload can be shortened to the tree borrow.
    let Some(data) = c.child(b"ldat") else {
        return Ok(None);
    };
    if data.data.len() % stride != 0 {
        diag(
            diagnostics,
            budget,
            "incomplete-render-settings-table",
            data.offset,
            "saved settings table has a truncated record; complete preceding records remain readable",
        )?;
    }
    if let Some(count) = c.child(b"lhd3").and_then(|n| b::u32(n.data, 8)) {
        if usize::try_from(count).ok() != Some(data.data.len() / stride) {
            diag(
                diagnostics,
                budget,
                "render-settings-count-mismatch",
                data.offset,
                "settings table count disagrees with its payload length",
            )?;
        }
    }
    Ok(Some(data))
}
fn record<'a>(c: Option<&'a Chunk<'_>>, index: usize, size: usize) -> Option<(&'a [u8], RawSpan)> {
    let c = c?;
    let offset = index.checked_mul(size)?;
    Some((
        c.data.get(offset..offset.checked_add(size)?)?,
        RawSpan {
            offset: c.offset.checked_add(8)?.checked_add(offset)?,
            length: size,
        },
    ))
}
pub(crate) fn decode(
    root: &Chunk<'_>,
    budget: &mut Budget<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Option<RenderQueue>, Error> {
    let Some(queue) = root.find_list(b"LRdr") else {
        return Ok(None);
    };
    let mut out = RenderQueue {
        items: Vec::new(),
        source: span(queue),
    };
    let settings = queue
        .list(b"list")
        .map(|n| blocks(n, RENDER_SIZE, budget, diagnostics))
        .transpose()?
        .flatten();
    let Some(items) = queue.list(b"LItm") else {
        return Ok(Some(out));
    };
    let mut pending = None;
    let mut comment = None;
    for child in &items.children {
        if child.id == *b"RCom" {
            comment = None;
            if child.data.get(..4) == Some(b"Utf8") {
                if let Some(size) = b::u32(child.data, 4).and_then(|n| usize::try_from(n).ok()) {
                    if let Some(text) = child.data.get(8..8usize.saturating_add(size)) {
                        comment = Some(budget.text(text)?);
                    }
                }
            }
            if comment.is_none() {
                diag(
                    diagnostics,
                    budget,
                    "invalid-render-comment",
                    child.offset,
                    "embedded render comment is malformed",
                )?;
            }
        } else if child.form == Some(*b"list") {
            if pending.is_some() {
                diag(
                    diagnostics,
                    budget,
                    "orphan-render-item",
                    child.offset,
                    "render item lacks its output module container",
                )?;
            }
            pending = Some(child);
        } else if child.form == Some(*b"LOm ") {
            let Some(item) = pending.take() else {
                diag(
                    diagnostics,
                    budget,
                    "orphan-output-modules",
                    child.offset,
                    "output modules have no preceding render item",
                )?;
                continue;
            };
            let rs = record(settings, out.items.len(), RENDER_SIZE)
                .map(|(d, s)| render_settings(d, s, budget))
                .transpose()?;
            if rs.is_none() {
                diag(
                    diagnostics,
                    budget,
                    "missing-render-settings",
                    item.offset,
                    "render item has no complete settings record",
                )?;
            }
            let om_settings = blocks(item, OUTPUT_SIZE, budget, diagnostics)?;
            let mut modules = Vec::new();
            let mut begin = 0;
            for (i, ch) in child.children.iter().enumerate() {
                if i > begin && ch.id == *b"Roou" {
                    let module = module(
                        child.children.get(begin..i).ok_or(Error::Allocation)?,
                        om_settings,
                        modules.len(),
                        budget,
                        diagnostics,
                    )?;
                    budget.push(&mut modules, module)?;
                    begin = i;
                }
            }
            if begin < child.children.len() {
                let module = module(
                    child.children.get(begin..).ok_or(Error::Allocation)?,
                    om_settings,
                    modules.len(),
                    budget,
                    diagnostics,
                )?;
                budget.push(&mut modules, module)?;
            }
            budget.push(
                &mut out.items,
                RenderQueueItem {
                    comment: comment.take(),
                    settings: rs,
                    output_modules: modules,
                    source: span(item),
                },
            )?;
        }
    }
    if let Some(item) = pending {
        diag(
            diagnostics,
            budget,
            "orphan-render-item",
            item.offset,
            "render item lacks its output module container",
        )?;
    }
    if settings.is_some_and(|n| n.data.len() / RENDER_SIZE != out.items.len()) {
        diag(
            diagnostics,
            budget,
            "render-item-count-mismatch",
            queue.offset,
            "render settings and item containers have different counts",
        )?;
    }
    Ok(Some(out))
}
fn render_settings(
    d: &[u8],
    source: RawSpan,
    budget: &mut Budget<'_>,
) -> Result<RenderSettings, Error> {
    let end = d.get(0x5a..0x9a).ok_or(Error::Allocation)?;
    let end = end.iter().rposition(|n| *n != 0).map_or(0, |n| n + 1);
    budget.charge(end.saturating_mul(2))?;
    let mut template_name = String::new();
    template_name
        .try_reserve_exact(end.saturating_mul(2))
        .map_err(|_| Error::Allocation)?;
    for byte in d.get(0x5a..0x5a + end).ok_or(Error::Allocation)? {
        template_name.push(char::from(*byte));
    }
    Ok(RenderSettings {
        composition_id: b::u32(d, 8).ok_or(Error::Allocation)?,
        status: b::u32(d, 12).ok_or(Error::Allocation)?,
        template_name,
        time_span_source: b::u16(d, 2148).ok_or(Error::Allocation)?,
        time_span_start: Ratio {
            numerator: i64::from(b::u32(d, 20).ok_or(Error::Allocation)?),
            denominator: b::u32(d, 24).ok_or(Error::Allocation)?,
        },
        time_span_duration: Ratio {
            numerator: i64::from(b::u32(d, 28).ok_or(Error::Allocation)?),
            denominator: b::u32(d, 32).ok_or(Error::Allocation)?,
        },
        queue_item_notify: d.get(7).is_some_and(|n| n & 4 != 0),
        elapsed_seconds: b::u32(d, 2202).ok_or(Error::Allocation)?,
        resolution: [
            b::u16(d, 58).ok_or(Error::Allocation)?,
            b::u16(d, 60).ok_or(Error::Allocation)?,
        ],
        skip_existing_files: b::u16(d, 84) != Some(0),
        field_render: b::u16(d, 50).ok_or(Error::Allocation)?,
        pulldown: b::u16(d, 54).ok_or(Error::Allocation)?,
        quality: b::u16(d, 56).ok_or(Error::Allocation)?,
        effects: b::u16(d, 64).ok_or(Error::Allocation)?,
        proxy_use: b::u16(d, 68).ok_or(Error::Allocation)?,
        motion_blur: b::u16(d, 72).ok_or(Error::Allocation)?,
        frame_blending: b::u16(d, 76).ok_or(Error::Allocation)?,
        log_type: b::u16(d, 80).ok_or(Error::Allocation)?,
        frame_rate: b::u16(d, 2144).ok_or(Error::Allocation)?,
        solo_switches: b::u16(d, 2164).ok_or(Error::Allocation)?,
        disk_cache: b::u16(d, 2168).ok_or(Error::Allocation)?,
        guide_layers: b::u16(d, 2172).ok_or(Error::Allocation)?,
        color_depth: b::u16(d, 2180).ok_or(Error::Allocation)?,
        source,
    })
}
fn module(
    group: &[Chunk<'_>],
    table: Option<&Chunk<'_>>,
    index: usize,
    budget: &mut Budget<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<OutputModule, Error> {
    let first = group.first().ok_or(Error::Allocation)?;
    let last = group.last().ok_or(Error::Allocation)?;
    let mut out = OutputModule {
        template_name: None,
        file_template: None,
        full_path: None,
        settings: record(table, index, OUTPUT_SIZE)
            .map(|(d, s)| output_settings(d, s))
            .transpose()?,
        options: None,
        format_options: None,
        source: RawSpan {
            offset: first.offset,
            length: last
                .offset
                .checked_add(span(last).length)
                .and_then(|end| end.checked_sub(first.offset))
                .ok_or(Error::Allocation)?,
        },
    };
    let mut after_alias = false;
    for c in group {
        if c.id == *b"Roou" {
            out.options = output_options(c, budget)?;
            if out.options.is_none() {
                diag(
                    diagnostics,
                    budget,
                    "incomplete-output-options",
                    c.offset,
                    "output options are shorter than the known layout",
                )?;
            }
        } else if c.id == *b"Ropt" {
            out.format_options = format(c.data);
            if out.format_options.is_none() {
                diag(
                    diagnostics,
                    budget,
                    "unsupported-output-format-options",
                    c.offset,
                    "format options remain in the raw tree",
                )?;
            }
        } else if c.form == Some(*b"Als2") {
            after_alias = true;
            if let Some(alias) = c.child(b"alas") {
                match crate::json::parse(alias.data, budget) {
                    Ok(json) => {
                        out.full_path = json
                            .get("fullpath")
                            .and_then(crate::json::Json::string)
                            .map(|s| budget.owned(s))
                            .transpose()?
                    }
                    Err(Error::InvalidFormat { .. }) => diag(
                        diagnostics,
                        budget,
                        "invalid-output-path-alias",
                        alias.offset,
                        "output path alias is malformed",
                    )?,
                    Err(e) => return Err(e),
                }
            }
        } else if after_alias && c.id == *b"Utf8" {
            if out.template_name.is_none() {
                out.template_name = Some(budget.text(c.data)?);
            } else if out.file_template.is_none() {
                out.file_template = Some(budget.text(c.data)?);
            }
        }
    }
    Ok(out)
}
fn output_settings(d: &[u8], source: RawSpan) -> Result<OutputModuleSettings, Error> {
    let f = d.get(7).copied().ok_or(Error::Allocation)?;
    Ok(OutputModuleSettings {
        use_comp_frame_number: f & 8 != 0,
        use_region_of_interest: f & 16 != 0,
        include_source_xmp: f & 64 != 0,
        preserve_rgb: f & 128 != 0,
        crop: d.get(31).is_some_and(|n| n & 1 != 0),
        post_render_composition_id: b::u32(d, 8).ok_or(Error::Allocation)?,
        post_render_action: b::u32(d, 48).ok_or(Error::Allocation)?,
        channels: d.get(19).copied().ok_or(Error::Allocation)?,
        resize_quality: d.get(23).copied().ok_or(Error::Allocation)?,
        output_audio: d.get(42).copied().ok_or(Error::Allocation)?,
        convert_to_linear: d.get(91).copied().ok_or(Error::Allocation)?,
        color_space_working: d.get(93).copied().ok_or(Error::Allocation)?,
        resize: d.get(27).is_some_and(|n| *n != 0),
        lock_aspect_ratio: d.get(29).is_some_and(|n| *n != 0),
        include_project_link: d.get(47).is_some_and(|n| *n != 0),
        crop_top: b::u16(d, 32).ok_or(Error::Allocation)?,
        crop_left: b::u16(d, 34).ok_or(Error::Allocation)?,
        crop_bottom: b::u16(d, 36).ok_or(Error::Allocation)?,
        crop_right: b::u16(d, 38).ok_or(Error::Allocation)?,
        source,
    })
}
fn output_options(c: &Chunk<'_>, budget: &mut Budget<'_>) -> Result<Option<OutputOptions>, Error> {
    let d = c.data;
    if d.len() < 114 {
        return Ok(None);
    }
    Ok(Some(OutputOptions {
        video_codec: budget.text(d.get(4..8).ok_or(Error::Allocation)?)?,
        format_id: budget.text(d.get(26..30).ok_or(Error::Allocation)?)?,
        starting_number: b::u32(d, 16).ok_or(Error::Allocation)?,
        width: b::u16(d, 36).ok_or(Error::Allocation)?,
        height: b::u16(d, 40).ok_or(Error::Allocation)?,
        depth: d.get(71).copied().ok_or(Error::Allocation)?,
        color_premultiplied: d.get(77).copied().ok_or(Error::Allocation)?,
        audio_sample_rate: b::f64(d, 100, false).ok_or(Error::Allocation)?.into(),
        audio_format: d.get(109).copied().ok_or(Error::Allocation)?,
        audio_bit_depth: d.get(111).copied().ok_or(Error::Allocation)?,
        audio_channels: d.get(113).copied().ok_or(Error::Allocation)?,
        audio_enabled: d.get(108) != Some(&255),
        source: span(c),
    }))
}
fn format(d: &[u8]) -> Option<FormatOptions> {
    Some(match d.get(..4)? {
        b"sDPX" if d.len() >= 47 => FormatOptions::Cineon {
            black_point: b::u16(d, 14)?,
            white_point: b::u16(d, 16)?,
            converted_black_point: b::f64(d, 18, false)?.into(),
            converted_white_point: b::f64(d, 26, false)?.into(),
            gamma: b::f64(d, 34, false)?.into(),
            highlight_expansion: b::u16(d, 42)?,
            logarithmic_conversion: *d.get(44)? != 0,
            file_format: *d.get(45)?,
            bit_depth: *d.get(46)?,
        },
        b"JPEG" if d.len() >= 58 => FormatOptions::Jpeg {
            quality: b::u16(d, 52)?,
        },
        b"oEXR" if d.len() >= 17 => FormatOptions::OpenExr {
            compression: *d.get(14)?,
            float_32: *d.get(15)? != 0,
            luminance_chroma: *d.get(16)? != 0,
        },
        b"TPIC" if d.len() >= 83 => FormatOptions::Targa {
            bits_per_pixel: *d.get(77)?,
            rle_compression: *d.get(82)? != 0,
        },
        b"TIF " if d.len() >= 602 => FormatOptions::Tiff {
            ibm_pc_byte_order: *d.get(600)? != 0,
            lzw_compression: *d.get(601)? != 0,
        },
        b"png!" if d.len() >= 34 => FormatOptions::Png {
            width: b::u32(d, 18)?,
            height: b::u32(d, 22)?,
            bit_depth: b::u16(d, 28)?,
            compression: b::u32(d, 30)?,
        },
        _ => return None,
    })
}
