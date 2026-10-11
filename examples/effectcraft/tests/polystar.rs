#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
use aep_effectcraft::import;
use effectcraft_keyframe::Value;
use effectcraft_project::{Project, PropGroup};

fn chunk(id: &[u8; 4], bytes: &[u8]) -> Vec<u8> {
    let mut out = id.to_vec();
    out.extend((bytes.len() as u32).to_be_bytes());
    out.extend(bytes);
    if bytes.len() % 2 == 1 {
        out.push(0);
    }
    out
}
fn list(id: &[u8; 4], children: Vec<Vec<u8>>) -> Vec<u8> {
    let mut body = id.to_vec();
    body.extend(children.into_iter().flatten());
    chunk(b"LIST", &body)
}
fn u32_at(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_be_bytes());
}
fn field(name: &str, values: &[f64]) -> Vec<u8> {
    field_with(name, values, Vec::new())
}
fn field_with(name: &str, values: &[f64], extra: Vec<Vec<u8>>) -> Vec<u8> {
    let mut desc = vec![0; 124];
    desc[3] = values.len() as u8;
    let body: Vec<_> = values.iter().flat_map(|n| n.to_be_bytes()).collect();
    let mut out = chunk(b"tdmn", name.as_bytes());
    let mut children = vec![
        chunk(b"tdsb", &1u32.to_be_bytes()),
        chunk(b"tdb4", &desc),
        chunk(b"cdat", &body),
    ];
    children.extend(extra);
    out.extend(list(b"tdbs", children));
    out
}
fn group(name: &str, children: Vec<Vec<u8>>, enabled: bool) -> Vec<u8> {
    let mut out = chunk(b"tdmn", name.as_bytes());
    let mut body = vec![chunk(b"tdsb", &u32::from(enabled).to_be_bytes())];
    body.extend(children);
    out.extend(list(b"tdgp", body));
    out
}
fn fixture(kind: f64, points: f64, enabled: bool, extra: Vec<Vec<u8>>) -> Vec<u8> {
    let mut item = vec![0; 64];
    item[..2].copy_from_slice(&4u16.to_be_bytes());
    u32_at(&mut item, 16, 7);
    let mut comp = vec![0; 204];
    comp[6..8].copy_from_slice(&1024u16.to_be_bytes());
    u32_at(&mut comp, 8, 30720);
    comp[0x8c..0x8e].copy_from_slice(&256u16.to_be_bytes());
    comp[0x8e..0x90].copy_from_slice(&256u16.to_be_bytes());
    for (at, n) in [
        (0x9c, 30 * 65536),
        (0x90, 1),
        (0x94, 1),
        (0xb0, 150),
        (0xa8, 30720),
    ] {
        u32_at(&mut comp, at, n);
    }
    let mut layer = vec![0; 164];
    for (at, n) in [
        (0, 12),
        (8, 1),
        (0x10, 30720),
        (0x14, 30720),
        (0x18, 30720),
        (0x1c, 153600),
        (0x20, 30720),
        (0x6c, 1),
    ] {
        u32_at(&mut layer, at, n);
    }
    let mut fields = vec![
        field("ADBE Vector Star Type", &[kind]),
        field("ADBE Vector Star Points", &[points]),
        field("ADBE Vector Star Outer Radius", &[80.0]),
    ];
    if kind == 1.0 {
        fields.push(field("ADBE Vector Star Inner Radius", &[30.0]));
    }
    fields.extend(extra);
    let root = group(
        "ADBE Root Vectors Group",
        vec![
            group("ADBE Vector Shape - Star", fields, enabled),
            group(
                "ADBE Vector Graphic - Fill",
                vec![field("ADBE Vector Fill Color", &[255.0, 255.0, 0.0, 0.0])],
                true,
            ),
        ],
        true,
    );
    let item = list(
        b"Item",
        vec![
            chunk(b"idta", &item),
            chunk(b"Utf8", b"Polystar"),
            chunk(b"cdta", &comp),
            list(
                b"Layr",
                vec![chunk(b"ldta", &layer), list(b"tdgp", vec![root])],
            ),
        ],
    );
    let mut body = b"Egg!".to_vec();
    body.extend(list(b"Fold", vec![item]));
    chunk(b"RIFX", &body)
}
fn star(project: &Project) -> Option<&PropGroup> {
    project.comps().next().unwrap().1.layers[0]
        .props
        .group("contents/star")
}

#[test]
fn static_star_polygon_and_reverse_direction_survive_public_import() {
    for (kind, expected) in [(1.0, 0), (2.0, 1)] {
        let result = import(&fixture(
            kind,
            5.0,
            true,
            vec![
                field("ADBE Vector Shape Direction", &[3.0]),
                field("ADBE Vector Star Rotation", &[22.0]),
            ],
        ))
        .unwrap();
        let reopened = Project::from_json(&result.project.to_file_json().unwrap()).unwrap();
        let shape = star(&reopened).unwrap();
        assert_eq!(shape.prop("type").unwrap().value, Value::Enum(expected));
        assert_eq!(shape.prop("direction").unwrap().value, Value::Enum(1));
        assert_eq!(shape.prop("points").unwrap().value, Value::Scalar(5.0));
        assert_eq!(shape.prop("rotation").unwrap().value, Value::Scalar(22.0));
        assert_eq!(shape.prop("innerRadius").is_some(), kind == 1.0);
    }
    let disabled = import(&fixture(1.0, 5.0, false, vec![])).unwrap();
    assert!(!star(&disabled.project).unwrap().enabled);
}

#[test]
fn unsupported_and_hostile_star_parameters_omit_only_the_shape() {
    for input in [
        fixture(1.0, 5.5, true, vec![]),
        fixture(1.0, f64::INFINITY, true, vec![]),
        fixture(1.0, 1e12, true, vec![]),
        fixture(1.0, 2.0, true, vec![]),
        fixture(7.0, 5.0, true, vec![]),
        fixture(
            1.0,
            5.0,
            true,
            vec![field("ADBE Vector Star Points", &[6.0])],
        ),
        fixture(
            1.0,
            5.0,
            true,
            vec![field("ADBE Vector Shape Direction", &[9.0])],
        ),
        fixture(
            1.0,
            5.0,
            true,
            vec![field("ADBE Vector Star Position", &[1.0])],
        ),
    ] {
        let result = import(&input).unwrap();
        assert!(star(&result.project).is_none());
        assert!(
            result.project.comps().next().unwrap().1.layers[0]
                .props
                .group("contents/fill")
                .is_some()
        );
        assert!(
            result
                .report
                .warnings
                .iter()
                .any(|w| w.message.contains("Polystar omitted"))
        );
    }
}

#[test]
fn approximations_and_elided_defaults_are_reported() {
    let result = import(&fixture(
        1.0,
        6.0,
        true,
        vec![field("ADBE Vector Star Outer Roundess", &[8.0])],
    ))
    .unwrap();
    assert_eq!(
        star(&result.project)
            .unwrap()
            .prop("outerRoundness")
            .unwrap()
            .value,
        Value::Scalar(8.0)
    );
    assert!(
        result
            .report
            .warnings
            .iter()
            .any(|w| w.message.contains("curve approximation"))
    );
    assert!(
        result
            .report
            .warnings
            .iter()
            .any(|w| w.message.contains("importer conventions"))
    );
}

#[test]
fn animated_or_expression_geometry_is_not_silently_frozen() {
    let mut header = vec![0; 52];
    u32_at(&mut header, 8, 1);
    u32_at(&mut header, 16, 48);
    let mut key = vec![0; 48];
    u32_at(&mut key, 0, 15360);
    key[8..16].copy_from_slice(&30.0f64.to_be_bytes());
    for extra in [
        vec![chunk(b"Utf8", b"time * 30")],
        vec![list(
            b"list",
            vec![chunk(b"lhd3", &header), chunk(b"ldat", &key)],
        )],
    ] {
        let input = fixture(
            1.0,
            5.0,
            true,
            vec![field_with("ADBE Vector Star Rotation", &[0.0], extra)],
        );
        let result = import(&input).unwrap();
        assert!(star(&result.project).is_none());
        assert!(
            result
                .report
                .warnings
                .iter()
                .any(|w| w.message.contains("Polystar omitted") && w.message.contains("static"))
        );
    }
}

#[test]
#[ignore = "local native star via AEP_SHAPE_FIXTURE_ROOT"]
fn native_star_parameters_are_read_and_mapped() {
    let path = std::path::PathBuf::from(std::env::var_os("AEP_SHAPE_FIXTURE_ROOT").unwrap())
        .join("v2_2_star.aep");
    let result = import(&std::fs::read(path).unwrap()).unwrap();
    fn find(group: &PropGroup) -> Option<&PropGroup> {
        if group.match_id == "star" {
            return Some(group);
        }
        group.children.iter().find_map(|n| match n {
            effectcraft_project::Node::Group(g) => find(g),
            _ => None,
        })
    }
    let shape = result
        .project
        .comps()
        .flat_map(|(_, c)| &c.layers)
        .find_map(|l| find(&l.props))
        .unwrap();
    for (name, n) in [
        ("points", 6.0),
        ("rotation", 15.0),
        ("innerRadius", 70.0),
        ("outerRadius", 180.0),
        ("innerRoundness", 12.0),
        ("outerRoundness", 8.0),
    ] {
        assert_eq!(shape.prop(name).unwrap().value, Value::Scalar(n));
    }
    assert_eq!(
        shape.prop("position").unwrap().value,
        Value::Vec2([10.0, 20.0])
    );
    Project::from_json(&result.project.to_file_json().unwrap()).unwrap();
}

#[cfg(feature = "render-check")]
#[test]
fn star_has_tips_not_a_disk_and_disable_removes_pixels() {
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
    let mut result = import(&fixture(1.0, 5.0, true, vec![])).unwrap();
    let id = *result.project.comps().next().unwrap().0;
    let layer = &mut result.project.comp_mut(id).unwrap().layers[0];
    layer.switches.video = true;
    layer.in_point = Tick::ZERO;
    for (name, value) in [
        ("position", Value::Vec3([128.0, 128.0, 0.0])),
        ("anchor", Value::Vec3([0.0; 3])),
        ("scale", Value::Vec3([100.0; 3])),
        ("opacity", Value::Scalar(100.0)),
    ] {
        layer
            .props
            .prop_mut(&format!("transform/{name}"))
            .unwrap()
            .value = value;
    }
    let render = |p: &Project| {
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
    let image = render(&result.project);
    assert!(image.get(128, 60)[3] > 0.9); // Top tip, outer radius 80.
    assert!(image.get(128, 128)[0] > 0.9); // Red center.
    assert_eq!(image.get(128, 190)[3], 0.0); // Between lower tips, unlike a disk.
    result.project.comp_mut(id).unwrap().layers[0]
        .props
        .group_mut("contents/star")
        .unwrap()
        .enabled = false;
    assert_eq!(render(&result.project).get(128, 128)[3], 0.0);
}
