use crate::{
    Error, binary as b,
    decode::{diag, span},
    error::Budget,
    model::*,
    rifx::Chunk,
};

fn localized(c: &Chunk<'_>, budget: &mut Budget<'_>) -> Result<Option<String>, Error> {
    c.list(b"CpS2")
        .and_then(|c| c.child(b"Utf8"))
        .map(|n| budget.text(n.data))
        .transpose()
}
pub(crate) fn decode(
    c: &Chunk<'_>,
    budget: &mut Budget<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Option<EssentialGraphics>, Error> {
    let Some(panel) = c.list(b"CIF3") else {
        if c.list(b"CIF1").is_some() || c.list(b"CIF2").is_some() {
            diag(
                diagnostics,
                budget,
                "unsupported-essential-graphics-version",
                c.offset,
                "older Essential Graphics panel remains in the raw tree",
            )?;
        }
        return Ok(None);
    };
    let mut out = EssentialGraphics {
        template_name: localized(panel, budget)?,
        controllers: Vec::new(),
        source: span(panel),
    };
    for ctrl in panel.children.iter().filter(|c| c.form == Some(*b"CCtl")) {
        let type_code = ctrl.child(b"CTyp").and_then(|n| b::u32(n.data, 0));
        if type_code.is_none() {
            diag(
                diagnostics,
                budget,
                "incomplete-essential-graphics-controller",
                ctrl.offset,
                "controller type is absent or truncated",
            )?;
        }
        let value = EssentialGraphicsController {
            uuid: ctrl
                .child(b"Utf8")
                .map(|n| budget.text(n.data))
                .transpose()?,
            name: localized(ctrl, budget)?,
            type_code,
            source: span(ctrl),
        };
        budget.push(&mut out.controllers, value)?;
    }
    Ok(Some(out))
}
