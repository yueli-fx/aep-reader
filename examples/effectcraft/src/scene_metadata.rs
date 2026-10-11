//! Saved scene metadata, with uncertain layout-derived values kept out of behavior.
use crate::{Report, Result, map_property, ratio, seconds};
use aep_reader as ae;
use effectcraft_project::{
    AutoOrient, Comp, FrameBlend, Layer, LayerSource, LightKind, Marker, Sampling,
};
use std::collections::BTreeSet;

pub(crate) fn project(
    source: &ae::Project,
    target: &mut effectcraft_project::Project,
    report: &mut Report,
) {
    use effectcraft_project::BitDepth;
    if let Some(depth) = source.bits_per_channel {
        match depth {
            8 => target.settings.bit_depth = BitDepth::Bpc8,
            16 => target.settings.bit_depth = BitDepth::Bpc16,
            32 => target.settings.bit_depth = BitDepth::Bpc32,
            _ => report.warn(
                "project",
                "Unknown source bit depth; target default retained.",
            ),
        }
    }
    if let Some(rate) = source.settings.audio_sample_rate {
        match rate.value {
            Some(v) if v.fract() == 0.0 && (8000.0..=192000.0).contains(&v) => {
                target.settings.audio_sample_rate = v as u32
            }
            _ => report.warn(
                "project",
                "Invalid/unsupported source audio sample rate; target default retained.",
            ),
        }
    }
    if source.settings.linear_blending
        || source.settings.linearize_working_space
        || source.settings.color_management.is_some()
    {
        report.warn("project", "Saved working-space/linear-light settings need color-pipeline mapping; bit depth is preserved but does not establish color equivalence.");
    }
}

pub(crate) fn light(code: Option<u32>) -> Option<LightKind> {
    match code {
        Some(0) => Some(LightKind::Parallel),
        Some(1) => Some(LightKind::Spot),
        Some(2) => Some(LightKind::Point),
        Some(3) => Some(LightKind::Ambient),
        _ => None,
    }
}

fn markers(input: &[ae::Marker], context: &str, report: &mut Report) -> Result<Vec<Marker>> {
    input.iter().map(|m|{
        let duration=if m.duration_layout_verified {seconds(m.duration_seconds.unwrap_or(0.0))?}else{
            if m.duration_seconds.is_some_and(|d|d!=0.0){report.warn(context,format!("Marker duration {:?} has an unverified source layout; retained as a point marker at {} seconds.",m.duration_seconds,m.time_seconds));}
            effectcraft_time::Tick::ZERO
        };
        if !m.cue_point_name.is_empty() || m.raw_flags.is_some_and(|f|f!=0){report.warn(context,"Marker cue/protection flags are unverified; text/link fields retained, behavior flags omitted.");}
        Ok(Marker{time:seconds(m.time_seconds)?,duration,comment:m.comment.clone(),chapter:m.chapter.clone(),url:m.url.clone(),frame_target:m.frame_target.clone(),..Marker::default()})
    }).collect()
}

pub(crate) fn composition(
    source: &ae::Composition,
    target: &mut Comp,
    report: &mut Report,
) -> Result<()> {
    target.markers = markers(&source.markers, &source.name, report)?;
    if let Some(start) = source.work_area_start.filter(|r| r.denominator != 0) {
        let start = seconds(ratio(Some(start), "work area start")?)?;
        let end = if source.work_area_uses_duration {
            target.duration
        } else if let Some(end) = source.work_area_end.filter(|r| r.denominator != 0) {
            seconds(ratio(Some(end), "work area end")?)?
        } else {
            target.duration
        };
        if start >= effectcraft_time::Tick::ZERO && end >= start && end <= target.duration {
            target.work_area = (start, end);
        } else {
            report.warn(&source.name,"Saved work area is outside composition bounds; target full-duration work area retained.");
        }
    } else if source.work_area_start.is_some() {
        report.warn(
            &source.name,
            "Invalid saved work-area denominator; full-duration work area retained.",
        );
    }
    if let Some(angle) = source.shutter_angle {
        target.shutter_angle = f64::from(angle);
    }
    if let Some(phase) = source.shutter_phase {
        target.shutter_phase = f64::from(phase);
    }
    for (value, destination) in [
        (
            source.motion_blur_samples_per_frame,
            &mut target.motion_blur_samples,
        ),
        (
            source.motion_blur_adaptive_sample_limit,
            &mut target.motion_blur_adaptive_limit,
        ),
    ] {
        if let Some(value) = value {
            if (1..=256).contains(&value) {
                *destination = value as u32;
            } else {
                report.warn(
                    &source.name,
                    "Motion blur sample setting outside importer budget; target default retained.",
                );
            }
        }
    }
    Ok(())
}

pub(crate) fn layer(
    source: &ae::Layer,
    target: &mut Layer,
    ticks: u32,
    mapped: &mut BTreeSet<usize>,
    context: &str,
    report: &mut Report,
) -> Result<()> {
    if let Some(s) = &source.switches {
        target.switches.motion_blur = s.motion_blur;
        target.switches.collapse = s.collapse_transformations;
        target.switches.sampling = if s.bicubic {
            Sampling::Bicubic
        } else {
            Sampling::Bilinear
        };
        target.switches.frame_blend = if !s.frame_blending {
            FrameBlend::Off
        } else if s.pixel_motion {
            FrameBlend::PixelMotion
        } else {
            FrameBlend::FrameMix
        };
        target.markers_locked = s.markers_locked;
    }
    target.markers = markers(&source.markers, context, report)?;
    if let Some(orient) = source.auto_orient {
        target.auto_orient = match orient {
            ae::AutoOrient::None => AutoOrient::Off,
            ae::AutoOrient::AlongPath => AutoOrient::AlongPath,
            ae::AutoOrient::CameraOrPointOfInterest
                if matches!(
                    target.source,
                    LayerSource::Camera | LayerSource::Light { .. }
                ) =>
            {
                AutoOrient::TowardsPointOfInterest
            }
            ae::AutoOrient::CameraOrPointOfInterest => AutoOrient::TowardsCamera,
            ae::AutoOrient::CharactersTowardCamera => {
                report.warn(context, "Per-character auto-orientation is not mapped.");
                AutoOrient::Off
            }
        };
    }
    let mapping: &[(&str, &str)] = match target.source {
        LayerSource::Camera => &[
            ("ADBE Anchor Point", "transform/poi"),
            ("ADBE Camera Zoom", "cameraOptions/zoom"),
            ("ADBE Camera Depth of Field", "cameraOptions/dof"),
            ("ADBE Camera Focus Distance", "cameraOptions/focusDistance"),
            ("ADBE Camera Aperture", "cameraOptions/aperture"),
            ("ADBE Camera Blur Level", "cameraOptions/blurLevel"),
        ],
        LayerSource::Light { .. } => &[
            ("ADBE Anchor Point", "transform/poi"),
            ("ADBE Light Intensity", "lightOptions/intensity"),
            ("ADBE Light Color", "lightOptions/color"),
            ("ADBE Light Cone Angle", "lightOptions/coneAngle"),
            ("ADBE Light Cone Feather 2", "lightOptions/coneFeather"),
            ("ADBE Light Falloff Start", "lightOptions/radius"),
            (
                "ADBE Light Falloff Distance",
                "lightOptions/falloffDistance",
            ),
            ("ADBE Casts Shadows", "lightOptions/castsShadows"),
            ("ADBE Light Shadow Darkness", "lightOptions/shadowDarkness"),
            (
                "ADBE Light Shadow Diffusion",
                "lightOptions/shadowDiffusion",
            ),
        ],
        _ => &[],
    };
    for (name, path) in mapping {
        if let Some(node) = source.node(name)
            && let Some(p) = &node.property
            && let Some(out) = target.props.prop_mut(path)
        {
            map_property(p, out, name, ticks, context, report)?;
            mapped.insert(node.source.offset);
        }
    }
    if !mapping.is_empty() {
        report.warn(context,"Camera/light saved parameters imported; film-size conventions, iris settings, falloff and AE lighting equivalence are not fully verified.");
    }
    if let Some(group) = source.node("ADBE Material Options Group") {
        for (name, path, max, boolean) in [
            (
                "ADBE Casts Shadows",
                "materialOptions/castsShadows",
                2.0,
                false,
            ),
            (
                "ADBE Accepts Shadows",
                "materialOptions/acceptsShadows",
                2.0,
                false,
            ),
            (
                "ADBE Accepts Lights",
                "materialOptions/acceptsLights",
                1.0,
                true,
            ),
        ] {
            if !group.children.iter().any(|n| n.match_name == name) {
                continue;
            }
            let Some(out) = target.props.prop_mut(path) else {
                continue;
            };
            let mut f = crate::shape_fields::Fields {
                node: group,
                offsets: Vec::new(),
                defaults: Vec::new(),
            };
            match f.scalar(name, None) {
                Ok(v) if v.fract() == 0.0 && (0.0..=max).contains(&v) => {
                    out.value = if boolean {
                        effectcraft_keyframe::Value::Bool(v == 1.0)
                    } else {
                        effectcraft_keyframe::Value::Enum(v as u32)
                    };
                    report.properties += f.offsets.len();
                    mapped.extend(f.offsets);
                }
                _ => report.warn(
                    context,
                    format!(
                        "Material {name} is invalid/animated/incomplete; target default retained."
                    ),
                ),
            }
        }
        report.warn(context, "Material shadow/light switches mapped where explicit; material coefficient units and renderer equivalence remain unverified.");
    }
    Ok(())
}
