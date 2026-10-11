//! Bounded static shape fields with explicit default provenance.
use crate::Result;
use aep_reader::{DecodeStatus, PropertyNode};

pub(crate) struct Fields<'a> {
    pub(crate) node: &'a PropertyNode,
    pub(crate) offsets: Vec<usize>,
    pub(crate) defaults: Vec<&'static str>,
}
impl Fields<'_> {
    pub(crate) fn numbers<const N: usize>(
        &mut self,
        name: &'static str,
        default: Option<[f64; N]>,
    ) -> Result<[f64; N]> {
        let mut found = self.node.children.iter().filter(|n| n.match_name == name);
        let Some(node) = found.next() else {
            self.defaults.push(name);
            return default.ok_or_else(|| format!("missing {name}"));
        };
        if found.next().is_some() {
            return Err(format!("duplicate {name}"));
        }
        let p = node
            .property
            .as_ref()
            .ok_or_else(|| format!("missing {name} payload"))?;
        if node.decode_status != DecodeStatus::Decoded
            || p.is_animated()
            || p.expression.as_ref().is_some_and(|s| !s.is_empty())
        {
            return Err(format!(
                "{name} requires complete static data without expressions"
            ));
        }
        let values = p
            .static_value
            .as_ref()
            .and_then(|v| v.numbers())
            .ok_or_else(|| format!("missing numeric {name}"))?;
        if values.len() != N {
            return Err(format!("invalid dimensions for {name}"));
        }
        let mut result = [0.0; N];
        for (out, n) in result.iter_mut().zip(values) {
            *out = crate::finite(n.value, name)?;
            if out.abs() > 1_000_000.0 {
                return Err(format!("{name} exceeds importer geometry budget"));
            }
        }
        self.offsets.push(node.source.offset);
        Ok(result)
    }
    pub(crate) fn scalar(&mut self, name: &'static str, default: Option<f64>) -> Result<f64> {
        let [value] = self.numbers(name, default.map(|n| [n]))?;
        Ok(value)
    }
}
