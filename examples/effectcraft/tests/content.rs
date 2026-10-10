#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
use effectcraft_keyframe::Value;
use effectcraft_project::LayerSource;
use effectcraft_time::Tick;

fn bytes(name: &str) -> Vec<u8> {
    let root = std::path::PathBuf::from(
        std::env::var_os("AEP_FIXTURE_ROOT").expect("set AEP_FIXTURE_ROOT"),
    );
    std::fs::read(root.join(name)).unwrap()
}

#[test]
#[ignore = "local authored AEP files via AEP_FIXTURE_ROOT"]
fn authored_paths_text_masks_and_units_survive_conversion() {
    let units = bytes("v2_2_shape_transform_full.aep");
    let source = aep_reader::parse(&units).unwrap();
    let source_layer = &source.project.compositions[0].layers[0];
    for name in ["ADBE Scale", "ADBE Opacity"] {
        let p = source_layer.property(name).unwrap();
        assert!(
            p.default_value()
                .unwrap()
                .numbers
                .iter()
                .all(|n| n.value == Some(1.0))
        );
        assert_eq!(p.is_modified(), Some(true));
    }
    let colors = aep_effectcraft::import(&bytes("v2_2_shape_kf_re.aep")).unwrap();
    let color_layer = &colors.project.comps().next().unwrap().1.layers[0];
    let color = color_layer
        .props
        .prop("contents/group/contents/fill/color")
        .unwrap();
    assert_eq!(color.keys[0].value, Value::Color([1.0, 0.0, 0.0, 1.0]));
    assert_eq!(color.keys[1].value, Value::Color([0.0, 0.0, 1.0, 1.0]));
    assert_eq!(
        effectcraft_keyframe::evaluate(&color.keys, Tick::from_seconds_f64(1.0), false),
        Some(Value::Color([0.5, 0.0, 0.5, 1.0]))
    );
    for name in ["re_path_anim_ae2020.aep", "re_path_anim_ae2025.aep"] {
        let result = aep_effectcraft::import(&bytes(name)).unwrap();
        let (_, comp) = result.project.comps().next().unwrap();
        let layer = &comp.layers[0];
        assert!(matches!(layer.source, LayerSource::Shape));
        let path = layer
            .props
            .prop("contents/group/contents/path/path")
            .unwrap();
        assert_eq!(path.keys.len(), 3);
        let expected = [
            vec![[0.0, 0.0], [40.0, 0.0], [40.0, 40.0], [0.0, 40.0]],
            vec![[10.0, 10.0], [150.0, 10.0], [150.0, 150.0], [10.0, 150.0]],
            vec![[0.0, 0.0], [200.0, 0.0], [100.0, 200.0]],
        ];
        for (i, key) in path.keys.iter().enumerate() {
            let Value::Path(p) = &key.value else {
                panic!("expected path")
            };
            assert_eq!(p.vertices, expected[i], "{name}");
            assert!(p.closed);
            assert!(
                p.in_tangents
                    .iter()
                    .chain(&p.out_tangents)
                    .all(|v| *v == [0.0; 2])
            );
            assert_eq!(key.time.seconds(), i as f64);
        }
        let Value::Path(mid) =
            effectcraft_keyframe::evaluate(&path.keys, Tick::from_seconds_f64(0.5), false).unwrap()
        else {
            panic!("path sample")
        };
        assert_eq!(
            mid.vertices,
            [[5.0, 5.0], [95.0, 5.0], [95.0, 95.0], [5.0, 95.0]]
        );
        assert!(
            !result
                .report
                .warnings
                .iter()
                .any(|w| w.context.contains("ADBE Vector Shape")
                    && w.message.contains("Unverified interpolation"))
        );
    }
    let result = aep_effectcraft::import(&bytes("re_text_multirun.aep")).unwrap();
    let layer = result
        .project
        .comps()
        .flat_map(|(_, c)| &c.layers)
        .find(|l| l.name == "multirun_src")
        .unwrap();
    assert!(matches!(layer.source, LayerSource::Text));
    let Value::Text(text) = &layer.props.prop("text/sourceText").unwrap().value else {
        panic!("text")
    };
    assert_eq!(text.text, "Hello");
    assert_eq!(text.runs.iter().map(|r| r.len).collect::<Vec<_>>(), [2, 3]);

    let result = aep_effectcraft::import(&bytes("v2_2_shape_transform_full.aep")).unwrap();
    let layer = &result.project.comps().next().unwrap().1.layers[0];
    assert_eq!(
        layer.props.prop("transform/scale").unwrap().value,
        Value::Vec3([120.0, 130.0, 100.0])
    );
    assert_eq!(
        layer.props.prop("transform/opacity").unwrap().value,
        Value::Scalar(80.0)
    );
    assert_eq!(
        layer
            .props
            .prop("contents/group/contents/fill/color")
            .unwrap()
            .value,
        Value::Color([0.5, 0.5, 0.5, 1.0])
    );
    assert_eq!(
        layer
            .props
            .prop("contents/group/contents/rect/size")
            .unwrap()
            .value,
        Value::Vec2([200.0, 100.0])
    );

    let result = aep_effectcraft::import(&bytes("re_mask_open.aep")).unwrap();
    let layer = result
        .project
        .comps()
        .flat_map(|(_, c)| &c.layers)
        .find(|l| l.props.group("masks/OpenLine").is_some())
        .unwrap();
    let Value::Path(path) = &layer.props.prop("masks/mask/path").unwrap().value else {
        panic!("mask path")
    };
    assert!(!path.closed);
    for (actual, expected) in
        path.vertices
            .iter()
            .zip([[20.0, 30.0], [100.0, 150.0], [180.0, 30.0]])
    {
        for (a, b) in actual.iter().zip(expected) {
            assert!((a - b).abs() < 1e-4);
        }
    }
}

#[test]
#[ignore = "local native AEP spatial-layout regression via AEP_FIXTURE_ROOT"]
fn native_flagged_spatial_records_retain_their_actual_values() {
    let source = bytes("re_batch.aep");
    let document = aep_reader::parse(&source).unwrap();
    let layer = document
        .project
        .compositions
        .iter()
        .flat_map(|c| &c.layers)
        .find(|l| l.id == 45)
        .unwrap();
    let prop = layer.property("ADBE Anchor Point").unwrap();
    let flagged: Vec<_> = prop
        .keyframes
        .iter()
        .filter(|k| matches!(k.raw_flags[1], 15 | 31))
        .collect();
    assert_eq!(flagged.len(), 3);
    for key in flagged {
        assert_eq!(
            key.value
                .as_ref()
                .unwrap()
                .numbers()
                .unwrap()
                .iter()
                .map(|n| n.value.unwrap())
                .collect::<Vec<_>>(),
            [0.5, 0.5, 0.0]
        );
        assert_eq!(key.in_spatial_tangent.len(), 3);
        assert_eq!(key.out_spatial_tangent.len(), 3);
    }
}

#[cfg(feature = "render-check")]
#[test]
#[ignore = "actual EffectCraft CPU rendering of local AEP via AEP_FIXTURE_ROOT"]
fn imported_shape_renders_expected_coverage_color_and_opacity() {
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
    let result = aep_effectcraft::import(&bytes("v2_2_shape_transform_full.aep")).unwrap();
    let id = *result.project.comps().next().unwrap().0;
    let render = effectcraft_render::Renderer::new(
        &result.project,
        &NoMedia,
        effectcraft_render::RenderOpts {
            motion_blur: false,
            ..Default::default()
        },
    );
    let image = render.comp_frame(id, Tick::ZERO);
    // Source rect is 200x100; layer scale 120x130%, rotation 25 degrees,
    // anchor (30,40), position (500,300). Its transformed center is (489,238).
    let center = image.get(489, 238);
    for (actual, expected) in center.into_iter().zip([0.4, 0.4, 0.4, 0.8]) {
        assert!((actual - expected).abs() < 0.02, "center={center:?}");
    }
    assert_eq!(image.get(0, 0), [0.0; 4]);
    let coverage = image.data.iter().filter(|p| p[3] > 0.1).count();
    assert!((30_000..33_000).contains(&coverage), "coverage={coverage}");
}

#[test]
#[ignore = "local authored effect AEP via AEP_FIXTURE_ROOT"]
fn gaussian_parameters_use_explicit_values_and_report_definition_sources() {
    let result = aep_effectcraft::import(&bytes("re_effect_param_elision_2025.aep")).unwrap();
    let layers = &result.project.comps().next().unwrap().1.layers;
    let mut values = Vec::new();
    for layer in layers {
        let effects = layer.props.group("effects").unwrap();
        let group = effects.sub("ec.blur.gaussian").unwrap();
        assert!(effects.enabled && group.enabled);
        let Value::Scalar(blur) = group.prop("blurriness").unwrap().value else {
            panic!("blur")
        };
        assert_eq!(group.prop("repeatEdge").unwrap().value, Value::Bool(true));
        assert_eq!(
            group.prop("dimensions").unwrap().value,
            Value::Enum(if blur == 25.0 { 1 } else { 0 })
        );
        values.push(blur as i32);
    }
    values.sort();
    assert_eq!(values, [0, 0, 25]);
    assert!(
        result
            .report
            .warnings
            .iter()
            .any(|w| w.message.contains("definition-derived (project"))
    );
    assert!(
        result
            .report
            .warnings
            .iter()
            .any(|w| w.message.contains("definition-derived (instance"))
    );
    assert!(
        !result
            .report
            .warnings
            .iter()
            .any(|w| w.message.contains("Gaussian omitted"))
    );
    result.project.to_file_json().unwrap();
}

#[test]
#[ignore = "local authored AEP modified only in memory"]
fn gaussian_definition_conflicts_and_disabled_stacks_are_visible() {
    let mut input = bytes("re_effect_param_elision_2025.aep");
    let (repeat_offset, flags) = {
        let doc = aep_reader::parse(&input).unwrap();
        let repeat = doc
            .project
            .effect_definition("ADBE Gaussian Blur 2")
            .unwrap()
            .parameters
            .iter()
            .find(|p| p.match_name.ends_with("-0003"))
            .unwrap();
        (
            repeat.source.offset,
            doc.root
                .find_all(b"ldta")
                .map(|c| c.offset + 8 + 0x27)
                .collect::<Vec<_>>(),
        )
    };
    // Alter the project definition only: instance-local true now conflicts with catalog false.
    input[repeat_offset + 64..repeat_offset + 69].fill(0);
    let result = aep_effectcraft::import(&input).unwrap();
    assert!(
        result
            .report
            .warnings
            .iter()
            .any(|w| w.message.contains("instance/project definition conflict"))
    );
    let layers = &result.project.comps().next().unwrap().1.layers;
    assert_eq!(
        layers
            .iter()
            .filter(|l| l
                .props
                .group("effects")
                .and_then(|g| g.sub("ec.blur.gaussian"))
                .is_some())
            .count(),
        2
    );
    for offset in flags {
        input[offset] &= !4;
    }
    let result = aep_effectcraft::import(&input).unwrap();
    assert!(
        result
            .project
            .comps()
            .next()
            .unwrap()
            .1
            .layers
            .iter()
            .all(|l| !l.switches.effects)
    );
}

#[cfg(feature = "render-check")]
#[test]
#[ignore = "actual EffectCraft Gaussian CPU render of imported native parameters"]
fn imported_gaussian_renders_repeat_edge_and_horizontal_dimension() {
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
    let mut result = aep_effectcraft::import(&bytes("re_effect_param_elision_2025.aep")).unwrap();
    let id = *result.project.comps().next().unwrap().0;
    let comp = result.project.comp_mut(id).unwrap();
    comp.width = 192;
    comp.height = 192;
    comp.layers.retain(|l| {
        l.props
            .group("effects")
            .and_then(|g| g.sub("ec.blur.gaussian"))
            .and_then(|g| g.prop("blurriness"))
            .is_some_and(|p| p.value == Value::Scalar(25.0))
    });
    assert_eq!(comp.layers.len(), 1);
    let layer = &mut comp.layers[0];
    // Isolate the imported effect using a 100x100 solid in a compact composition.
    for (name, value) in [
        ("anchor", Value::Vec3([50.0, 50.0, 0.0])),
        ("position", Value::Vec3([96.0, 96.0, 0.0])),
        ("scale", Value::Vec3([100.0; 3])),
        ("rotation", Value::Scalar(0.0)),
        ("opacity", Value::Scalar(100.0)),
    ] {
        layer
            .props
            .prop_mut(&format!("transform/{name}"))
            .unwrap()
            .value = value;
    }
    let render = |p: &effectcraft_project::Project| {
        effectcraft_render::Renderer::new(
            p,
            &NoMedia,
            effectcraft_render::RenderOpts {
                motion_blur: false,
                ..Default::default()
            },
        )
        .comp_frame(id, Tick::ZERO)
    };
    let repeated = render(&result.project);
    assert!(repeated.get(48, 96)[3] > 0.99);
    assert_eq!(repeated.get(40, 96)[3], 0.0);
    result.project.comp_mut(id).unwrap().layers[0]
        .props
        .group_mut("effects")
        .and_then(|g| g.sub_mut("ec.blur.gaussian"))
        .and_then(|g| g.prop_mut("repeatEdge"))
        .unwrap()
        .value = Value::Bool(false);
    let transparent = render(&result.project);
    let edge = transparent.get(40, 96)[3];
    assert!(edge > 0.05 && edge < 0.5, "horizontal blurred edge={edge}");
    assert_eq!(transparent.get(96, 40)[3], 0.0); // Horizontal-only, not vertical.
    assert!(transparent.get(96, 96)[3] > 0.99);
    result.project.comp_mut(id).unwrap().layers[0]
        .switches
        .effects = false;
    let disabled = render(&result.project);
    assert_eq!(disabled.get(40, 96)[3], 0.0);
}

#[test]
#[ignore = "native Gaussian animation via AEP_FIXTURE_ROOT"]
fn gaussian_blurriness_animation_preserves_saved_values_and_times() {
    let input = bytes("re_batch.aep");
    let source = aep_reader::parse(&input).unwrap();
    let imported = aep_effectcraft::import(&input).unwrap();
    let mut keys = 0;
    for comp in &source.project.compositions {
        let target = imported
            .project
            .comps()
            .find(|(id, _)| imported.project.item(**id).unwrap().name == comp.name)
            .unwrap()
            .1;
        for (sl, tl) in comp.layers.iter().zip(&target.layers) {
            for effect in sl
                .effects()
                .iter()
                .filter(|n| n.match_name == "ADBE Gaussian Blur 2")
            {
                let p = effect
                    .children
                    .iter()
                    .find(|n| n.match_name == "ADBE Gaussian Blur 2-0001")
                    .unwrap()
                    .property
                    .as_ref()
                    .unwrap();
                let mapped = tl
                    .props
                    .group("effects")
                    .unwrap()
                    .sub("ec.blur.gaussian")
                    .unwrap()
                    .prop("blurriness")
                    .unwrap();
                assert_eq!(mapped.keys.len(), p.keyframes.len());
                for (a, b) in p.keyframes.iter().zip(&mapped.keys) {
                    let expected = a.value.as_ref().unwrap().numbers().unwrap()[0]
                        .value
                        .unwrap();
                    assert_eq!(b.value, Value::Scalar(expected));
                    assert!(
                        (b.time.seconds()
                            - f64::from(a.time_ticks) / f64::from(comp.keyframe_ticks_per_second))
                        .abs()
                            < 1e-6
                    );
                    keys += 1;
                }
            }
        }
    }
    assert!(keys > 0);
}

fn disable_saved_node(mut input: Vec<u8>, name: &str) -> Vec<u8> {
    let at = {
        let doc = aep_reader::parse(&input).unwrap();
        let node = doc
            .project
            .compositions
            .iter()
            .flat_map(|c| &c.layers)
            .find_map(|l| l.node(name))
            .unwrap();
        assert_eq!(node.enabled, Some(true), "native baseline for {name}");
        let chunk = doc
            .root
            .walk()
            .find(|c| c.offset == node.source.offset)
            .unwrap();
        chunk.child(b"tdsb").unwrap().offset + 11
    };
    input[at] &= !1;
    input
}

#[test]
#[ignore = "native fixtures with enable bit changed only in memory"]
fn shape_and_mask_enable_states_survive_import() {
    for (name, path) in [
        ("ADBE Root Vectors Group", "contents/group"),
        ("ADBE Vector Group", "contents/group"),
        ("ADBE Vectors Group", "contents/group"),
        ("ADBE Vector Shape - Rect", "contents/group/contents/rect"),
        ("ADBE Vector Graphic - Fill", "contents/group/contents/fill"),
    ] {
        let input = disable_saved_node(bytes("v2_2_shape_transform_full.aep"), name);
        let imported = aep_effectcraft::import(&input).unwrap();
        let layer = &imported.project.comps().next().unwrap().1.layers[0];
        assert!(!layer.props.group(path).unwrap().enabled, "{name}");
        assert_eq!(
            layer
                .props
                .prop("contents/group/contents/rect/size")
                .unwrap()
                .value,
            Value::Vec2([200.0, 100.0])
        );
    }
    let all_masks = |p: &effectcraft_project::Project| {
        p.comps()
            .flat_map(|(_, c)| &c.layers)
            .flat_map(|l| l.props.group("masks"))
            .flat_map(|g| g.groups())
            .map(|g| g.enabled)
            .collect::<Vec<_>>()
    };
    let baseline = aep_effectcraft::import(&bytes("re_mask_open.aep")).unwrap();
    let before = all_masks(&baseline.project);
    for name in ["ADBE Mask Atom", "ADBE Mask Parade"] {
        let input = disable_saved_node(bytes("re_mask_open.aep"), name);
        let result = aep_effectcraft::import(&input).unwrap();
        let after = all_masks(&result.project);
        assert_eq!(before.len(), after.len());
        assert!(before.iter().zip(&after).any(|(a, b)| *a && !*b), "{name}");
    }
}

#[cfg(feature = "render-check")]
#[test]
#[ignore = "CPU pixel checks with native shape flags changed only in memory"]
fn disabled_shape_nodes_do_not_render() {
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
    for name in [
        "ADBE Root Vectors Group",
        "ADBE Vector Group",
        "ADBE Vectors Group",
        "ADBE Vector Shape - Rect",
        "ADBE Vector Graphic - Fill",
    ] {
        let input = disable_saved_node(bytes("v2_2_shape_transform_full.aep"), name);
        let imported = aep_effectcraft::import(&input).unwrap();
        let id = *imported.project.comps().next().unwrap().0;
        let image = effectcraft_render::Renderer::new(
            &imported.project,
            &NoMedia,
            effectcraft_render::RenderOpts {
                motion_blur: false,
                ..Default::default()
            },
        )
        .comp_frame(id, Tick::ZERO);
        assert!(
            image.data.iter().all(|p| p[3] == 0.0),
            "{name} still rendered"
        );
    }
}

#[test]
#[ignore = "authored tracking reference via AEP_DATA_ROOT"]
fn static_tracking_animator_preserves_saved_range_and_amount() {
    let path = std::path::PathBuf::from(std::env::var_os("AEP_DATA_ROOT").unwrap())
        .join("reference/text-animator/re_text_animator_tracking.aep");
    let bytes = std::fs::read(path).unwrap();
    let result = aep_effectcraft::import(&bytes).unwrap();
    let layer = &result.project.comps().next().unwrap().1.layers[0];
    let anim = layer.props.group("text/animators/animator").unwrap();
    for (path, value) in [
        ("selectors/rangeSelector/start", 20.0),
        ("selectors/rangeSelector/end", 80.0),
        ("selectors/rangeSelector/offset", 10.0),
        ("properties/tracking", 50.0),
    ] {
        let p = anim.prop(path).unwrap();
        assert_eq!(p.value, Value::Scalar(value));
        assert!(p.keys.is_empty());
    }
    assert_eq!(
        anim.prop("properties/trackingType").unwrap().value,
        Value::Enum(0)
    );
    assert!(anim.enabled);
    // A different, unverified companion enum must not silently become the supported mode.
    let doc = aep_reader::parse(&bytes).unwrap();
    let n = doc.project.compositions[0].layers[0]
        .node("ADBE Text Track Type")
        .unwrap();
    let chunk = doc
        .root
        .find_all(b"LIST")
        .find(|c| c.offset == n.source.offset)
        .unwrap();
    let at = chunk.child(b"cdat").unwrap().offset + 8;
    let mut changed = bytes.clone();
    changed[at..at + 8].copy_from_slice(&2.0f64.to_be_bytes());
    let result = aep_effectcraft::import(&changed).unwrap();
    assert!(
        result.project.comps().next().unwrap().1.layers[0]
            .props
            .group("text/animators")
            .unwrap()
            .children
            .is_empty()
    );
    assert!(
        result
            .report
            .warnings
            .iter()
            .any(|w| w.message.contains("unverified tracking type"))
    );
}

#[test]
#[ignore = "real corpus via AEP_DATA_ROOT"]
fn corpus_duration_and_bad_ease_do_not_destroy_import() {
    let root = std::path::PathBuf::from(std::env::var_os("AEP_DATA_ROOT").unwrap());
    for (path, seconds) in [
        (
            "samples/dribbble-freebies-2/oleksandr-countdown/raw/Countdown_Freebie.aep",
            20.0,
        ),
        (
            "samples/dribbble-freebies-2/oleksandr-loader/raw/loader.aep",
            4.0,
        ),
    ] {
        let bytes = std::fs::read(root.join(path)).unwrap();
        let result = aep_effectcraft::import(&bytes).unwrap();
        assert_eq!(
            result.project.comps().next().unwrap().1.duration.seconds(),
            seconds
        );
    }
    for path in [
        "samples/dribbble-freebies-10/eddy-camera-flip/raw/28_-_selfie_camera.aep",
        "samples/dribbble-freebies-20/tony-ducky-run-walk/raw/ducky_walkruncycle_tonybabel.aep",
    ] {
        let bytes = std::fs::read(root.join(path)).unwrap();
        let result = aep_effectcraft::import(&bytes).unwrap();
        assert!(result.report.keyframes > 0);
        assert!(
            result
                .report
                .warnings
                .iter()
                .any(|w| w.message.contains("Invalid temporal ease"))
        );
        result.project.to_file_json().unwrap();
    }
    let bytes = std::fs::read(root.join("samples/github-licensed/_extracted/bryful_AE_SpiritBall-master/AE_SpiritBall-master/Icons/Icons.aep")).unwrap();
    let result = aep_effectcraft::import(&bytes).unwrap();
    assert!(
        result
            .project
            .comps()
            .any(|(_, c)| c.display_start < effectcraft_time::Tick::ZERO)
    );
}
