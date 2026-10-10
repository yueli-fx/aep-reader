#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
use aep_reader::{Value, parse};
fn scalar(value: &Option<Value>) -> Option<f64> {
    value.as_ref()?.numbers()?.first()?.value
}
#[test]
#[ignore = "local authored AEP via AEP_FIXTURE_ROOT"]
fn native_effect_catalog_keeps_elided_instances_and_explicit_values_separate() {
    let root = std::path::PathBuf::from(
        std::env::var_os("AEP_FIXTURE_ROOT").expect("set AEP_FIXTURE_ROOT"),
    );
    let bytes = std::fs::read(root.join("re_effect_param_elision_2025.aep")).unwrap();
    let doc = parse(&bytes).unwrap();
    let name = "ADBE Gaussian Blur 2";
    let catalog = doc.project.effect_definition(name).unwrap();
    assert_eq!(catalog.source.offset, 0x130);
    let repeat = catalog
        .parameters
        .iter()
        .find(|d| d.match_name.ends_with("-0003"))
        .unwrap();
    assert_eq!(repeat.control_type, 4);
    assert_eq!(scalar(&repeat.default_value), Some(1.0));
    let dimensions = catalog
        .parameters
        .iter()
        .find(|d| d.match_name.ends_with("-0002"))
        .unwrap();
    assert_eq!(scalar(&dimensions.last_value), Some(1.0));
    assert_eq!(scalar(&dimensions.default_value), Some(0.0)); // Raw fields, not an effective enum value.
    assert_eq!(dimensions.option_count, Some(3));
    let instances: Vec<_> = doc
        .project
        .compositions
        .iter()
        .flat_map(|c| &c.layers)
        .flat_map(|l| l.effects())
        .filter(|n| n.match_name == name)
        .collect();
    assert_eq!(instances.len(), 3);
    assert_eq!(
        instances
            .iter()
            .filter(|n| n.effect_parameters.is_empty())
            .count(),
        2
    );
    let touched = instances
        .iter()
        .find(|n| n.children.iter().any(|c| c.match_name.ends_with("-0001")))
        .unwrap();
    assert_eq!(touched.source.offset, 0x7aa0);
    let blur = touched
        .children
        .iter()
        .find(|c| c.match_name.ends_with("-0001"))
        .unwrap()
        .property
        .as_ref()
        .unwrap();
    assert_eq!(scalar(&blur.static_value), Some(25.0));
    assert!(blur.effect_definition.is_none()); // No automatic cross-scope merge.
    for instance in &instances {
        assert!(
            !instance
                .children
                .iter()
                .any(|n| n.match_name.ends_with("-0003"))
        );
    }
    let local = instances
        .iter()
        .find(|n| !n.effect_parameters.is_empty())
        .unwrap();
    let local_repeat = local
        .effect_parameters
        .iter()
        .find(|d| d.match_name.ends_with("-0003"))
        .unwrap();
    assert_ne!(repeat.source, local_repeat.source);
    assert_eq!(scalar(&local_repeat.default_value), Some(1.0));
    assert_eq!(
        doc.bytes(repeat.source).unwrap().get(..4),
        Some(b"pard".as_slice())
    );
}
