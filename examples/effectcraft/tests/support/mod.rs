#![allow(dead_code, clippy::indexing_slicing)]
pub fn chunk(id: &[u8; 4], bytes: &[u8]) -> Vec<u8> {
    let mut out = id.to_vec();
    out.extend((bytes.len() as u32).to_be_bytes());
    out.extend(bytes);
    if bytes.len() % 2 == 1 {
        out.push(0);
    }
    out
}
pub fn list(id: &[u8; 4], children: Vec<Vec<u8>>) -> Vec<u8> {
    let mut body = id.to_vec();
    body.extend(children.into_iter().flatten());
    chunk(b"LIST", &body)
}
pub fn u32_at(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_be_bytes());
}
pub fn field(name: &str, values: &[f64]) -> Vec<u8> {
    field_with(name, values, Vec::new())
}
pub fn field_with(name: &str, values: &[f64], extra: Vec<Vec<u8>>) -> Vec<u8> {
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
pub fn group(name: &str, children: Vec<Vec<u8>>, enabled: bool) -> Vec<u8> {
    let mut out = chunk(b"tdmn", name.as_bytes());
    let mut body = vec![chunk(b"tdsb", &u32::from(enabled).to_be_bytes())];
    body.extend(children);
    out.extend(list(b"tdgp", body));
    out
}

pub fn project(nodes: Vec<Vec<u8>>) -> Vec<u8> {
    project_properties(vec![group("ADBE Root Vectors Group", nodes, true)])
}
pub fn project_properties(properties: Vec<Vec<u8>>) -> Vec<u8> {
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
    let item = list(
        b"Item",
        vec![
            chunk(b"idta", &item),
            chunk(b"Utf8", b"Polystar"),
            chunk(b"cdta", &comp),
            list(
                b"Layr",
                vec![chunk(b"ldta", &layer), list(b"tdgp", properties)],
            ),
        ],
    );
    let mut body = b"Egg!".to_vec();
    body.extend(list(b"Fold", vec![item]));
    chunk(b"RIFX", &body)
}

pub fn keyed_field(name: &str, keys: &[(u32, f64)]) -> Vec<u8> {
    let mut h = vec![0; 52];
    u32_at(&mut h, 8, keys.len() as u32);
    u32_at(&mut h, 16, 48);
    let data: Vec<u8> = keys
        .iter()
        .flat_map(|(t, v)| {
            let mut k = vec![0; 48];
            u32_at(&mut k, 0, *t);
            k[4] = 1;
            k[5] = 1;
            k[8..16].copy_from_slice(&v.to_be_bytes());
            k
        })
        .collect();
    field_with(
        name,
        &[0.0],
        vec![list(
            b"list",
            vec![chunk(b"lhd3", &h), chunk(b"ldat", &data)],
        )],
    )
}

pub fn replace_chunk(root: &aep_reader::rifx::Chunk<'_>, at: usize, replacement: &[u8]) -> Vec<u8> {
    if root.offset == at {
        return replacement.to_vec();
    }

    if let Some(form) = root.form {
        let mut body = form.to_vec();
        if root.opaque {
            body.extend(root.data);
            return chunk(&root.id, &body);
        }
        for child in &root.children {
            body.extend(replace_chunk(child, at, replacement));
        }
        chunk(&root.id, &body)
    } else {
        chunk(&root.id, root.data)
    }
}
