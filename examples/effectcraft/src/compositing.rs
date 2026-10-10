//! Application compositing mappings; stored reader codes remain untouched.
use crate::{Report, Result};
use effectcraft_color::BlendMode;
use effectcraft_project::{Comp, LayerId, MatteKind, TrackMatte};
use std::collections::BTreeMap;

pub(crate) fn blend(code: Option<u8>, context: &str, report: &mut Report) -> BlendMode {
    use BlendMode::*;
    match code {
        Some(0 | 2) => Normal,
        Some(3) => Dissolve,
        Some(4) => Add,
        Some(5) => Multiply,
        Some(6) => Screen,
        Some(7) => Overlay,
        Some(8) => SoftLight,
        Some(9) => HardLight,
        Some(10) => Darken,
        Some(11) => Lighten,
        Some(12) => ClassicDifference,
        Some(13) => Hue,
        Some(14) => Saturation,
        Some(15) => Color,
        Some(16) => Luminosity,
        Some(17) => StencilAlpha,
        Some(18) => StencilLuma,
        Some(19) => SilhouetteAlpha,
        Some(20) => SilhouetteLuma,
        Some(21) => LuminescentPremul,
        Some(22) => AlphaAdd,
        Some(23) => ClassicColorDodge,
        Some(24) => ClassicColorBurn,
        Some(25) => Exclusion,
        Some(26) => Difference,
        Some(27) => ColorDodge,
        Some(28) => ColorBurn,
        Some(29) => LinearDodge,
        Some(30) => LinearBurn,
        Some(31) => LinearLight,
        Some(32) => VividLight,
        Some(33) => PinLight,
        Some(34) => HardMix,
        Some(35) => LighterColor,
        Some(36) => DarkerColor,
        Some(37) => Subtract,
        Some(38) => Divide,
        _ => {
            report.warn(
                context,
                format!("Unknown blend code {code:?}; normal blending used."),
            );
            Normal
        }
    }
}

pub(crate) fn mattes(
    source: &aep_reader::Composition,
    target: &mut Comp,
    ids: &BTreeMap<u32, LayerId>,
    report: &mut Report,
) -> Result<()> {
    for (layer, out) in source.layers.iter().zip(&mut target.layers) {
        let context = format!("{}/{}", source.name, layer.name);
        let kind = match layer.track_matte_mode {
            Some(0) => continue,
            Some(1) => MatteKind::Alpha,
            Some(2) => MatteKind::AlphaInverted,
            Some(3) => MatteKind::Luma,
            Some(4) => MatteKind::LumaInverted,
            code => {
                report.warn(
                    &context,
                    format!("Unknown matte code {code:?}; matte omitted."),
                );
                continue;
            }
        };
        let Some(matte) = source.matte_source_of(layer) else {
            if layer
                .track_matte_layer_id
                .is_some_and(|id| id != 0 && id != u32::MAX)
            {
                return Err(format!("{context}: dangling matte source"));
            }
            report.warn(
                &context,
                "Matte mode has no resolved source; matte omitted.",
            );
            continue;
        };
        if matte.track_matte_mode.is_some_and(|code| code != 0) {
            report.warn(
                &context,
                "Nested matte source: target rendering equivalence is not verified.",
            );
        }
        let id = *ids.get(&matte.id).ok_or("missing target matte layer")?;
        out.track_matte = Some(TrackMatte { layer: id, kind });
    }
    crate::validate_chains(
        target
            .layers
            .iter()
            .map(|l| (l.id, l.track_matte.map(|m| m.layer)))
            .collect(),
    )
    .map_err(|e| format!("invalid matte graph: {e}"))
}
