//! Consumer-only static text conversion. Font identifiers are copied, never loaded.
use std::collections::BTreeMap;

use aep_reader::{DecodeStatus, Number, TextDocument, TextParagraph, TextRange, TextStyleRun};
use effectcraft_keyframe::{
    BaselineOption, CharStyle, Justify, Kerning, ParaStyle, StyleRun, TextDoc,
};

pub(crate) fn convert(source: &TextDocument) -> Result<(TextDoc, Vec<String>), String> {
    let mut warnings = Vec::new();
    if source.decode_status != DecodeStatus::Decoded {
        warnings.push(
            "Text engine decoding is partial; unrepresented fields remain in the AEP source."
                .into(),
        );
    }
    warnings.push("Font PostScript identifiers are retained; font availability/family matching and text layout fidelity are unverified. Fill enablement is not decoded; target fill defaults to enabled.".into());
    let visible = source.text.strip_suffix('\r').unwrap_or(&source.text);
    let mut target = TextDoc::plain(&visible.replace('\r', "\n"));
    let visible_len = visible.chars().count();
    let mut boundaries = BTreeMap::new();
    let mut units = 0;
    boundaries.insert(0, 0);
    for (index, character) in source.text.chars().enumerate() {
        units += character.len_utf16();
        boundaries.insert(units, (index + 1).min(visible_len));
    }
    let mut runs = Vec::new();
    let mut cursor = 0;
    let mut stroke_order = None;
    for run in &source.style_runs {
        let range = scalar_range(run.range, &boundaries)?;
        if range.start != cursor {
            return Err("Text style runs overlap or leave uncovered characters".into());
        }
        cursor = range.end;
        let style = convert_style(run, source, &mut warnings)?;
        runs.push(StyleRun {
            len: range.len(),
            style,
        });
        if let Some(order) = run.stroke_over_fill {
            if stroke_order.is_some_and(|previous| previous != order) {
                warnings.push("Mixed stroke-over-fill order cannot be represented per run; using first run's order.".into());
            } else if stroke_order.is_none() {
                stroke_order = Some(order);
            }
        }
    }
    if !runs.is_empty() {
        if cursor != visible_len {
            return Err("Text style runs do not cover the visible text".into());
        }
        target.set_runs(runs);
    } else {
        warnings.push("No decoded character styles; using EffectCraft text defaults.".into());
    }
    if let Some(order) = stroke_order {
        target.stroke_over_fill = order;
    }
    for paragraph in &source.paragraphs {
        let range = scalar_range(paragraph.range, &boundaries)?;
        let style = convert_paragraph(paragraph, &mut warnings)?;
        // Source ranges include paragraph separators. Apply the range so a stored
        // style covering multiple paragraphs keeps the same paragraph formatting.
        if range.start <= visible_len {
            target.apply_para(range, |value| *value = style.clone());
        }
    }
    if source.paragraphs.is_empty()
        && let Some(value) = source.justification
    {
        target.justify = justification(value, &mut warnings)?;
    }
    if let Some([left, top, right, bottom]) = source.box_bounds {
        let [left, top, right, bottom] =
            [number(left)?, number(top)?, number(right)?, number(bottom)?];
        let width = right - left;
        let height = bottom - top;
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return Err("Text box has invalid bounds".into());
        }
        target.box_pos = [left, top];
        target.box_size = Some([width, height]);
    }
    if source.font_axes.iter().any(|axes| !axes.is_empty()) {
        warnings.push("Variable font axes lack decoded axis tags and were not mapped.".into());
    }
    if !source.manual_kerning.is_empty() || source.kerning.is_some() {
        warnings
            .push("Manual kerning indices/semantics are unverified and were not mapped.".into());
    }
    warnings.sort();
    warnings.dedup();
    Ok((target, warnings))
}

fn scalar_range(
    range: Option<TextRange>,
    boundaries: &BTreeMap<usize, usize>,
) -> Result<std::ops::Range<usize>, String> {
    let range = range.ok_or("Text range is missing or invalid")?;
    let end = range
        .utf16_start
        .checked_add(range.utf16_length)
        .ok_or("Text range overflow")?;
    let start = boundaries
        .get(&range.utf16_start)
        .copied()
        .ok_or("Text range starts outside a Unicode boundary")?;
    let end = boundaries
        .get(&end)
        .copied()
        .ok_or("Text range ends outside a Unicode boundary")?;
    Ok(start..end)
}

fn number(value: Number) -> Result<f64, String> {
    value
        .value
        .filter(|v| v.is_finite())
        .ok_or_else(|| "Non-finite text numeric value".into())
}

fn color([alpha, red, green, blue]: [Number; 4]) -> Result<[f32; 4], String> {
    let values = [number(red)?, number(green)?, number(blue)?, number(alpha)?];
    if values.iter().any(|v| v.abs() > f64::from(f32::MAX)) {
        return Err("Text color exceeds target numeric range".into());
    }
    Ok(values.map(|v| v as f32))
}

fn convert_style(
    run: &TextStyleRun,
    source: &TextDocument,
    warnings: &mut Vec<String>,
) -> Result<CharStyle, String> {
    let mut style = CharStyle::default();
    if let Some(font) = run.font_name(source).filter(|font| !font.is_empty()) {
        style.font = font.to_owned();
        style.style.clear();
    } else {
        warnings.push("Missing/invalid font identifier; using target default font.".into());
    }
    for (source, target) in [
        (run.font_size, &mut style.size),
        (run.tracking, &mut style.tracking),
        (run.baseline_shift, &mut style.baseline_shift),
        (run.stroke_width, &mut style.stroke_width),
        (run.tsume, &mut style.tsume),
    ] {
        if let Some(value) = source {
            *target = number(value)?;
        }
    }
    if style.size <= 0.0 || style.stroke_width < 0.0 || !(0.0..=100.0).contains(&style.tsume) {
        return Err("Invalid text size, stroke width, or tsume".into());
    }
    if run.auto_leading == Some(false) {
        style.leading = run.leading.map(number).transpose()?;
    } else if run.auto_leading.is_none() && run.leading.is_some() {
        warnings.push("Text auto-leading flag missing; target auto leading used.".into());
    }
    if let Some(fill) = run.fill_argb {
        style.fill = color(fill)?;
    }
    if let Some(stroke) = run.stroke_argb {
        style.stroke = color(stroke)?;
    }
    if let Some(flag) = run.apply_stroke {
        style.apply_stroke = flag;
    }
    if let Some(flag) = run.faux_bold {
        style.faux_bold = flag;
    }
    if let Some(flag) = run.faux_italic {
        style.faux_italic = flag;
    }
    if let Some(value) = run.caps_option {
        match number(value)? {
            0.0 => {}
            1.0 => style.small_caps = true,
            2.0 => style.all_caps = true,
            3.0 => style.opentype.all_small_caps = true,
            _ => warnings.push("Unknown text caps enum; omitted.".into()),
        }
    }
    if let Some(value) = run.baseline_option {
        style.baseline = match number(value)? {
            0.0 => BaselineOption::Normal,
            1.0 => BaselineOption::Superscript,
            2.0 => BaselineOption::Subscript,
            _ => {
                warnings.push("Unknown text baseline enum; omitted.".into());
                BaselineOption::Normal
            }
        };
    }
    if let Some(value) = run.auto_kern_type {
        style.kerning = match number(value)? {
            1.0 => Kerning::Metrics,
            2.0 => Kerning::Optical,
            _ => {
                warnings.push("Manual/unknown text auto-kerning mode; using metrics.".into());
                Kerning::Metrics
            }
        };
    }
    for scale in [run.horizontal_scale, run.vertical_scale]
        .into_iter()
        .flatten()
    {
        if number(scale)? != 1.0 {
            warnings
                .push("Non-default text scale has unverified raw units; using 100% scale.".into());
        }
    }
    if run.no_break == Some(true) || run.line_join_type.is_some() || run.digit_set.is_some() {
        warnings.push("Text no-break, stroke joins, and digit-set settings are not mapped.".into());
    }
    Ok(style)
}

fn justification(value: Number, warnings: &mut Vec<String>) -> Result<Justify, String> {
    Ok(match number(value)? {
        0.0 => Justify::Left,
        1.0 => Justify::Right,
        2.0 => Justify::Center,
        _ => {
            warnings.push("Unverified paragraph justification enum; using left alignment.".into());
            Justify::Left
        }
    })
}

fn convert_paragraph(
    source: &TextParagraph,
    warnings: &mut Vec<String>,
) -> Result<ParaStyle, String> {
    let mut target = ParaStyle::default();
    if let Some(value) = source.justification {
        target.justify = justification(value, warnings)?;
    }
    for (source, destination) in [
        (source.first_line_indent, &mut target.indent_first),
        (source.start_indent, &mut target.indent_left),
        (source.end_indent, &mut target.indent_right),
        (source.space_before, &mut target.space_before),
        (source.space_after, &mut target.space_after),
    ] {
        if let Some(value) = source {
            *destination = number(value)?;
        }
    }
    if let Some(flag) = source.hanging_roman {
        target.hanging_punctuation = flag;
    }
    if source.direction.is_some()
        || source.leading_type.is_some()
        || source.auto_hyphenate.is_some()
    {
        warnings.push("Paragraph direction, leading type, and hyphenation are not mapped; target defaults apply.".into());
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(text: &str) -> TextDocument {
        TextDocument {
            text: text.into(),
            fonts: Vec::new(),
            font_axes: Vec::new(),
            style_runs: Vec::new(),
            paragraphs: Vec::new(),
            box_bounds: None,
            manual_kerning: Vec::new(),
            kerning: None,
            justification: None,
            decode_status: DecodeStatus::Partial,
        }
    }

    fn run(start: usize, length: usize, size: f64) -> TextStyleRun {
        TextStyleRun {
            range: Some(TextRange {
                utf16_start: start,
                utf16_length: length,
            }),
            font_index: Some(0.0.into()),
            font_size: Some(size.into()),
            tracking: None,
            leading: None,
            fill_argb: None,
            stroke_argb: None,
            faux_bold: None,
            faux_italic: None,
            auto_leading: None,
            horizontal_scale: None,
            vertical_scale: None,
            baseline_shift: None,
            tsume: None,
            apply_stroke: None,
            stroke_width: None,
            auto_kern_type: None,
            caps_option: None,
            baseline_option: None,
            no_break: None,
            stroke_over_fill: None,
            line_join_type: None,
            digit_set: None,
        }
    }

    #[test]
    fn mixed_styles_keep_unicode_ranges_and_static_formatting() -> Result<(), String> {
        let mut source = plain("A😀B\r");
        source.fonts.push("Example-Bold".into());
        source.style_runs = vec![run(0, 1, 24.0), run(1, 2, 48.0), run(3, 2, 30.0)];
        if let Some(emoji) = source.style_runs.get_mut(1) {
            emoji.fill_argb = Some([0.5, 0.0, 1.0, 0.0].map(Number::from));
            emoji.tracking = Some(150.0.into());
            emoji.caps_option = Some(3.0.into());
        }
        let (target, _) = convert(&source)?;
        assert_eq!(target.text, "A😀B");
        assert_eq!(
            target.runs().iter().map(|r| r.len).collect::<Vec<_>>(),
            [1, 1, 1]
        );
        assert_eq!(target.style_at(1).size, 48.0);
        assert_eq!(target.style_at(1).fill, [0.0, 1.0, 0.0, 0.5]);
        assert_eq!(target.style_at(1).tracking, 150.0);
        assert!(target.style_at(1).opentype.all_small_caps);
        assert_eq!(target.style_at(2).font, "Example-Bold");
        source.style_runs = vec![run(0, 2, 24.0)];
        assert!(convert(&source).is_err());
        source.style_runs = vec![run(1, 4, 24.0)];
        assert!(convert(&source).is_err());
        Ok(())
    }

    #[test]
    fn strips_one_terminator_and_preserves_user_line_breaks() -> Result<(), String> {
        assert_eq!(convert(&plain("A\rB\r\r"))?.0.text, "A\nB\n");
        assert_eq!(convert(&plain("\r"))?.0.text, "");
        Ok(())
    }

    #[test]
    fn unicode_boundaries_reject_split_surrogates_and_overflow() -> Result<(), String> {
        let boundaries = BTreeMap::from([(0, 0), (1, 1), (3, 2), (4, 3)]);
        assert_eq!(
            scalar_range(
                Some(TextRange {
                    utf16_start: 1,
                    utf16_length: 2
                }),
                &boundaries
            )?,
            1..2
        );
        assert!(
            scalar_range(
                Some(TextRange {
                    utf16_start: 1,
                    utf16_length: 1
                }),
                &boundaries
            )
            .is_err()
        );
        assert!(
            scalar_range(
                Some(TextRange {
                    utf16_start: usize::MAX,
                    utf16_length: 1
                }),
                &boundaries
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn argb_conversion_keeps_alpha_and_rejects_nonfinite() -> Result<(), String> {
        assert_eq!(
            color([0.5.into(), 1.0.into(), 0.0.into(), 0.25.into()])?,
            [1.0, 0.0, 0.25, 0.5]
        );
        assert!(color([f64::NAN.into(); 4]).is_err());
        Ok(())
    }

    #[test]
    fn box_bounds_and_alignment_are_converted() -> Result<(), String> {
        let mut source = plain("text\r");
        source.box_bounds = Some([-10.0, -20.0, 390.0, 180.0].map(Number::from));
        source.justification = Some(2.0.into());
        let (target, _) = convert(&source)?;
        assert_eq!(target.box_pos, [-10.0, -20.0]);
        assert_eq!(target.box_size, Some([400.0, 200.0]));
        assert_eq!(target.justify, Justify::Center);
        source.box_bounds = Some([10.0, 10.0, 0.0, 0.0].map(Number::from));
        assert!(convert(&source).is_err());
        Ok(())
    }
}
