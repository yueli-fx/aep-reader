#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
use effectcraft_color::BlendMode;
use effectcraft_project::MatteKind;
fn bytes(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::PathBuf::from(std::env::var_os("AEP_FIXTURE_ROOT").unwrap()).join(name),
    )
    .unwrap()
}
fn offset(input: &[u8], id: u32) -> usize {
    let d = aep_reader::parse(input).unwrap();
    d.root
        .find_all(b"ldta")
        .find(|n| input[n.offset + 8..n.offset + 12] == id.to_be_bytes())
        .unwrap()
        .offset
        + 8
}
#[test]
#[ignore = "local authored native AEP fixtures"]
fn native_explicit_and_legacy_mattes_reopen() {
    let result = aep_effectcraft::import(&bytes("re_trackmatte_ae24.aep")).unwrap();
    let c = result
        .project
        .comps()
        .find(|(_, c)| c.layers.len() == 7)
        .unwrap()
        .1;
    // Fixture's native ldta IDs in saved order: 67,65,63,61,59,57,55.
    for (index, source_index, kind) in [
        (6, 2, MatteKind::Alpha),
        (5, 1, MatteKind::Luma),
        (4, 0, MatteKind::AlphaInverted),
    ] {
        let m = c.layers[index].track_matte.unwrap();
        assert_eq!(m.kind, kind);
        assert_eq!(m.layer, c.layers[source_index].id);
    }
    effectcraft_project::Project::from_json(&result.project.to_file_json().unwrap()).unwrap();
    for file in [
        "re_delete_layer_matte_predelete_ae20.aep",
        "re_delete_layer_matte_predelete_ae25.aep",
    ] {
        let input = bytes(file);
        let d = aep_reader::parse(&input).unwrap();
        let out = aep_effectcraft::import(&input).unwrap();
        let mut checked = 0;
        for (source, (_, target)) in d.project.compositions.iter().zip(out.project.comps()) {
            for (l, t) in source.layers.iter().zip(&target.layers) {
                if let Some(m) = source.matte_source_of(l) {
                    let index = source
                        .layers
                        .iter()
                        .position(|l| std::ptr::eq(l, m))
                        .unwrap();
                    assert_eq!(t.track_matte.unwrap().layer, target.layers[index].id);
                    checked += 1;
                }
            }
        }
        assert!(checked > 0, "{file}");
    }
}
#[test]
#[ignore = "native fixture changed only in memory"]
fn compositing_codes_and_invalid_references_are_not_silent() {
    let original = bytes("re_trackmatte_ae24.aep");
    let at = offset(&original, 55);
    for (code, expected) in [
        (2, BlendMode::Normal),
        (4, BlendMode::Add),
        (5, BlendMode::Multiply),
        (6, BlendMode::Screen),
        (26, BlendMode::Difference),
        (38, BlendMode::Divide),
    ] {
        let mut input = original.clone();
        input[at + 0x63] = code;
        let out = aep_effectcraft::import(&input).unwrap();
        let l = out
            .project
            .comps()
            .find(|(_, c)| c.layers.len() == 7)
            .map(|(_, c)| &c.layers[6])
            .unwrap();
        assert_eq!(l.blend_mode, expected);
    }
    let mut input = original.clone();
    input[at + 0x63] = 255;
    input[at + 0x6b] = 255;
    let out = aep_effectcraft::import(&input).unwrap();
    assert!(
        out.report
            .warnings
            .iter()
            .any(|w| w.message.contains("Unknown blend"))
    );
    assert!(
        out.report
            .warnings
            .iter()
            .any(|w| w.message.contains("Unknown matte"))
    );
    for id in [
        u32::from_be_bytes(original[at..at + 4].try_into().unwrap()),
        999999,
    ] {
        let mut input = original.clone();
        input[at + 0xa0..at + 0xa4].copy_from_slice(&id.to_be_bytes());
        assert!(aep_effectcraft::import(&input).is_err());
    }
    let mut input = original.clone();
    input[at + 0xa0..at + 0xa4].fill(0);
    let out = aep_effectcraft::import(&input).unwrap();
    let l = out
        .project
        .comps()
        .find(|(_, c)| c.layers.len() == 7)
        .map(|(_, c)| &c.layers[6])
        .unwrap();
    assert!(l.track_matte.is_none()); // Explicit zero must not select the layer above.
    assert!(
        out.report
            .warnings
            .iter()
            .any(|w| w.message.contains("no resolved source"))
    );
    let b = offset(&original, 63);
    let mut input = original.clone();
    input[b + 0x6b] = 1;
    input[b + 0xa0..b + 0xa4].copy_from_slice(&original[at..at + 4]);
    assert!(aep_effectcraft::import(&input).is_err()); // Two-layer cycle.
}
#[cfg(feature = "render-check")]
#[test]
#[ignore = "isolated CPU render of imported native matte"]
fn four_matte_modes_render_with_hidden_source() {
    use effectcraft_keyframe::Value;
    use effectcraft_time::Tick;
    struct NoMedia;
    impl effectcraft_render::FootageSource for NoMedia {
        fn frame(
            &self,
            _: effectcraft_project::ItemId,
            _: &effectcraft_project::Footage,
            _: Tick,
        ) -> Option<std::sync::Arc<effectcraft_render::Image>> {
            None
        }
    }
    let original = bytes("re_trackmatte_ae24.aep");
    let at = offset(&original, 55);
    for (code, inside, outside) in [
        (1, 1.0, 0.0),
        (2, 0.0, 1.0),
        (3, 0.9278, 0.0),
        (4, 0.0722, 1.0),
    ] {
        let mut input = original.clone();
        input[at + 0x6b] = code;
        let mut out = aep_effectcraft::import(&input).unwrap();
        let cid = *out
            .project
            .comps()
            .find(|(_, c)| c.layers.len() == 7)
            .unwrap()
            .0;
        let c = out.project.comp_mut(cid).unwrap();
        c.width = 200;
        c.height = 100;
        let fill_id = c.layers[6].id;
        let matte_id = c.layers[2].id;
        c.layers.retain(|l| l.id == fill_id || l.id == matte_id);
        for l in &mut c.layers {
            l.switches.video = l.id == fill_id;
            for (name, v) in [
                ("anchor", Value::Vec3([50.0, 50.0, 0.0])),
                (
                    "position",
                    Value::Vec3([if l.switches.video { 100.0 } else { 50.0 }, 50.0, 0.0]),
                ),
                ("scale", Value::Vec3([100.0; 3])),
                ("rotation", Value::Scalar(0.0)),
                ("opacity", Value::Scalar(100.0)),
            ] {
                let p = l.props.prop_mut(&format!("transform/{name}")).unwrap();
                p.keys.clear();
                p.value = v;
            }
        }
        let image = effectcraft_render::Renderer::new(
            &out.project,
            &NoMedia,
            effectcraft_render::RenderOpts {
                motion_blur: false,
                ..Default::default()
            },
        )
        .comp_frame(cid, Tick::ZERO);
        for (x, expected) in [(75, inside), (125, outside)] {
            assert!(
                (image.get(x, 50)[3] - expected).abs() < 0.02,
                "code={code} x={x} pixel={:?}",
                image.get(x, 50)
            );
        }
        assert!(image.get(25, 50)[3] < 0.01); // Hidden matte must not draw by itself.
    }
}
