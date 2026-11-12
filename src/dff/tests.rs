use super::{
    chunks::{Frame, child, chunks, materials, transform},
    components::{is_permanent_damage_or_lod, optional_extra_number},
    *,
};

fn hidden_component(name: &str) -> bool {
    is_permanent_damage_or_lod(name, true) || optional_extra_number(name).is_some()
}

fn chunk(kind: u32, data: &[u8]) -> Vec<u8> {
    [
        kind.to_le_bytes().as_slice(),
        &(data.len() as u32).to_le_bytes(),
        &0x1803_ffffu32.to_le_bytes(),
        data,
    ]
    .concat()
}

fn wheel_clump() -> Vec<u8> {
    let nodes = [
        ("root", -1i32, [0.0f32; 3]),
        ("wheel_rf_dummy", 0, [1.0, 2.0, 0.0]),
        ("wheel_mesh", 1, [100.0, 200.0, 300.0]),
        ("wheel_lf_dummy", 0, [-1.0, 2.0, 0.0]),
        ("wheel_rb_dummy", 0, [1.0, -2.0, 0.0]),
        ("wheel_lb_dummy", 0, [-1.0, -2.0, 0.0]),
    ];
    let mut frames = (nodes.len() as u32).to_le_bytes().to_vec();
    for (_, parent, position) in &nodes {
        for value in [1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]
            .into_iter()
            .chain(*position)
        {
            frames.extend(value.to_le_bytes());
        }
        frames.extend(parent.to_le_bytes());
        frames.extend(0u32.to_le_bytes());
    }
    let mut frame_list = chunk(1, &frames);
    for (name, _, _) in &nodes {
        frame_list.extend(chunk(3, &chunk(0x0253_f2fe, name.as_bytes())));
    }
    let mut geometry = [0u32, 1, 3, 1]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    geometry.extend([1u16, 0, 0, 2].into_iter().flat_map(u16::to_le_bytes));
    geometry.extend([0u32, 0, 0, 0, 1, 1].into_iter().flat_map(u32::to_le_bytes));
    for value in [
        0.0f32, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.5, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0,
    ] {
        geometry.extend(value.to_le_bytes());
    }
    let mut material_data = vec![0; 28];
    material_data[4..8].fill(255);
    let material = chunk(7, &[chunk(1, &material_data), chunk(3, &[])].concat());
    let material_list = [chunk(1, &[1, 0, 0, 0, 255, 255, 255, 255]), material].concat();
    let geometry = chunk(0xf, &[chunk(1, &geometry), chunk(8, &material_list)].concat());
    let atomic = chunk(
        0x14,
        &chunk(
            1,
            &[2u32, 0, 5, 0]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        ),
    );
    chunk(
        0x10,
        &[
            chunk(1, &[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            chunk(0xe, &frame_list),
            chunk(0x1a, &geometry),
            atomic,
        ]
        .concat(),
    )
}

#[test]
fn assembles_four_wheels_with_native_reparenting_and_axle_dimensions() {
    let model = decode_with_wheels(&wheel_clump(), [0.8, 1.0]).expect("complete clump fixture");
    assert_eq!(model.faces.len(), 4);
    assert!(model.is_vehicle);
    let right_front = &model.faces[0];
    assert_eq!(
        right_front.points,
        [[1.0, 2.0, 0.0], [1.0, 2.4, 0.0], [1.0, 2.0, 0.4]]
    );
    let left_front = &model.faces[1];
    assert!((left_front.points[1][1] - 1.6).abs() < 0.0001);
    assert_eq!(left_front.normals[0], [-1.0, 0.0, 0.0]);
    assert_eq!(model.faces[2].points[2], [1.0, -2.0, 0.5]);
    assert_eq!(model.faces[3].points[2], [-1.0, -2.0, 0.5]);
    assert!(model.radius < 4.0);
}

#[test]
fn uv_dictionary_prefix_is_skipped_without_scanning_sector_padding() {
    let bytes = [chunk(0x2b, &[0; 8]), wheel_clump(), vec![0; 2048]].concat();
    assert_eq!(decode(&bytes).expect("prefixed clump").faces.len(), 4);
    assert!(decode(&chunk(0x2b, &[0; 8])).is_err());
}

#[test]
fn native_fixed_clump_header_ignores_observed_high_length_bit() {
    let mut bytes = wheel_clump();
    bytes[16..20].copy_from_slice(&0x0100_000cu32.to_le_bytes());
    assert_eq!(decode(&bytes).expect("native clump").faces.len(), 4);
    bytes[16..20].copy_from_slice(&0x0100_000du32.to_le_bytes());
    assert!(decode(&bytes).is_err());
}

#[test]
fn sequential_material_read_accepts_short_outer_envelope_but_checks_children() {
    let mut data = vec![0; 28];
    data[4..8].fill(255);
    data[12..16].copy_from_slice(&1u32.to_le_bytes());
    let texture = chunk(
        6,
        &[
            chunk(1, &[0; 4]),
            chunk(2, b"body\0"),
            chunk(2, &[0; 4]),
            chunk(3, &[]),
        ]
        .concat(),
    );
    let mut material = chunk(7, &[chunk(1, &data), texture, chunk(3, &[])].concat());
    material[4..8].copy_from_slice(&52u32.to_le_bytes());
    let list = chunk(
        8,
        &[chunk(1, &[1, 0, 0, 0, 255, 255, 255, 255]), material].concat(),
    );
    let parsed = materials(&list).expect("native sequential material");
    assert_eq!(parsed[0].texture, "body");
    assert!(materials(&list[..list.len() - 2]).is_err());
}

#[test]
fn invalid_source_normals_use_finite_face_normals() {
    let mut bytes = wheel_clump();
    let root = child(&bytes, 0x10).expect("clump");
    let list = child(root, 0x1a).expect("geometry list");
    let geometry = child(list, 0xf).expect("geometry");
    let data = child(geometry, 1).expect("geometry data");
    let normal_offset = data.as_ptr() as usize - bytes.as_ptr() as usize + 84;
    bytes[normal_offset..normal_offset + 4].copy_from_slice(&f32::NAN.to_le_bytes());
    let model = decode(&bytes).expect("valid positions with one invalid normal");
    assert!(
        model
            .faces
            .iter()
            .flat_map(|f| f.normals)
            .flatten()
            .all(f32::is_finite)
    );
    assert_eq!(model.discarded_triangles, 0);
}

#[test]
fn native_damage_and_optional_components_are_hidden_in_base_preview() {
    for name in [
        "door_lf_dam",
        "door_lf_dam.001",
        "chassis_vlo",
        "extra1",
        "extra6",
    ] {
        assert!(hidden_component(name), "{name}");
    }
    for name in [
        "door_lf_ok",
        "chassis",
        "wheel_rf_dummy",
        "extra_light",
        "extra7",
    ] {
        assert!(!hidden_component(name), "{name}");
    }
}

#[test]
fn malformed_chunks_and_frame_cycles_are_rejected() {
    assert!(decode(&[]).is_err());
    assert!(decode(&[0x10, 0, 0, 0, 255, 255, 255, 127, 0, 0, 0, 0]).is_err());
    let mut matrix = [0.0; 12];
    matrix[0] = 1.0;
    matrix[4] = 1.0;
    matrix[8] = 1.0;
    assert!(
        transform(
            [0.0; 3],
            0,
            &[Frame {
                matrix,
                parent: 0,
                name: String::new(),
                hanim_id: None,
            }]
        )
        .is_err()
    );
    assert!(chunks(&[0; 11]).is_err());
}

#[test]
fn preserves_wheel_child_scale_and_skips_aircraft_cloning() {
    let nodes = [
        (
            "hydra",
            -1i32,
            [1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            [0.0f32; 3],
        ),
        (
            "gear_l",
            0,
            [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            [-1.0, -2.0, 0.0],
        ),
        (
            "wheel_rf_dummy",
            0,
            [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            [0.0, 2.0, -1.0],
        ),
        (
            "wheel_rf",
            2,
            [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0],
        ),
        (
            "wheel_lf_dummy",
            0,
            [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            [0.0, 0.0, -0.5],
        ),
        (
            "wheel_rb_dummy",
            0,
            [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            [1.0, -2.0, -1.0],
        ),
        (
            "wheel_rb",
            5,
            [0.1, 0.0, 0.0, 0.0, 0.1, 0.0, 0.0, 0.0, 0.1],
            [50.0, 50.0, 50.0],
        ),
    ];
    let mut frames = (nodes.len() as u32).to_le_bytes().to_vec();
    for (_, parent, rot, pos) in &nodes {
        for value in rot.iter().chain(pos.iter()) {
            frames.extend(value.to_le_bytes());
        }
        frames.extend(parent.to_le_bytes());
        frames.extend(0u32.to_le_bytes());
    }
    let mut frame_list = chunk(1, &frames);
    for (name, _, _, _) in &nodes {
        frame_list.extend(chunk(3, &chunk(0x0253_f2fe, name.as_bytes())));
    }
    let make_geo = |r: f32| {
        let mut g = [0u32, 1, 3, 1]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        g.extend([1u16, 0, 0, 2].into_iter().flat_map(u16::to_le_bytes));
        g.extend([0u32, 0, 0, 0, 1, 1].into_iter().flat_map(u32::to_le_bytes));
        for v in [
            0.0f32, 0.0, 0.0, 0.0, r, 0.0, 0.0, 0.0, r, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0,
        ] {
            g.extend(v.to_le_bytes());
        }
        let mut md = vec![0; 28];
        md[4..8].fill(255);
        let mat = chunk(7, &[chunk(1, &md), chunk(3, &[])].concat());
        let ml = [chunk(1, &[1, 0, 0, 0, 255, 255, 255, 255]), mat].concat();
        chunk(0xf, &[chunk(1, &g), chunk(8, &ml)].concat())
    };
    let a0 = chunk(
        0x14,
        &chunk(
            1,
            &[3u32, 0, 5, 0]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        ),
    );
    let a1 = chunk(
        0x14,
        &chunk(
            1,
            &[6u32, 1, 5, 0]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        ),
    );
    let clump = chunk(
        0x10,
        &[
            chunk(1, &[2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            chunk(0xe, &frame_list),
            chunk(0x1a, &[make_geo(0.3), make_geo(10.0)].concat()),
            a0,
            a1,
        ]
        .concat(),
    );
    let model = decode(&clump).expect("aircraft clump");
    assert_eq!(model.faces.len(), 2);
    let rear_y = model.faces[1].points[1][1];
    assert!((rear_y - (-1.0)).abs() < 0.001);
}

#[test]
fn turns_inward_unrotated_left_wheel_outward() {
    let nodes = [
        ("root", -1i32, [0.0f32; 3]),
        ("wheel_rf_dummy", 0, [0.8, 2.0, 0.0]),
        ("wheel_rf", 1, [0.0, 0.0, 0.0]),
        ("wheel_lb_dummy", 0, [-0.8, -2.0, 0.0]),
        ("wheel_lb", 3, [0.0, 0.0, 0.0]),
    ];
    let mut frames = (nodes.len() as u32).to_le_bytes().to_vec();
    for (_, parent, position) in &nodes {
        for value in [1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]
            .into_iter()
            .chain(*position)
        {
            frames.extend(value.to_le_bytes());
        }
        frames.extend(parent.to_le_bytes());
        frames.extend(0u32.to_le_bytes());
    }
    let mut frame_list = chunk(1, &frames);
    for (name, _, _) in &nodes {
        frame_list.extend(chunk(3, &chunk(0x0253_f2fe, name.as_bytes())));
    }
    let make_wheel = |shift_x: f32| {
        let mut g = [0u32, 1, 3, 1]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        g.extend([1u16, 0, 0, 2].into_iter().flat_map(u16::to_le_bytes));
        g.extend([0u32, 0, 0, 0, 1, 1].into_iter().flat_map(u32::to_le_bytes));
        for v in [
            shift_x, 0.0, 0.0, shift_x, 0.2, 0.0, shift_x, 0.0, 0.2, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0,
            1.0, 0.0, 0.0,
        ] {
            g.extend(v.to_le_bytes());
        }
        let mut md = vec![0; 28];
        md[4..8].fill(255);
        let ml = chunk(
            8,
            &[
                chunk(1, &[1, 0, 0, 0, 255, 255, 255, 255]),
                chunk(7, &[chunk(1, &md), chunk(3, &[])].concat()),
            ]
            .concat(),
        );
        chunk(0xf, &[chunk(1, &g), ml].concat())
    };
    let a_rf = chunk(
        0x14,
        &chunk(
            1,
            &[2u32, 0, 4, 0]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        ),
    );
    let a_lb = chunk(
        0x14,
        &chunk(
            1,
            &[4u32, 1, 4, 0]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        ),
    );
    let bytes = chunk(
        0x10,
        &[
            chunk(1, &[2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            chunk(0xe, &frame_list),
            chunk(0x1a, &[make_wheel(0.1), make_wheel(0.1)].concat()),
            a_rf,
            a_lb,
        ]
        .concat(),
    );
    let model = decode(&bytes).expect("bus clump");
    let lb_x = model.faces[1].points[0][0];
    assert!((lb_x - (-0.9)).abs() < 1e-4, "got lb_x={lb_x}");
}

#[test]
fn separates_damage_and_groups_stock_vs_tuning_components() {
    let nodes = [
        ("chassis_dummy", -1i32),
        ("door_lf_ok", 0),
        ("door_lf_dam", 0),
        ("bumper_f_dorest", 0),
        ("bumper_f_rest", 0),
        ("extra1", 0),
    ];
    let mut fl = (nodes.len() as u32).to_le_bytes().to_vec();
    for (_, parent) in &nodes {
        for value in [1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0] {
            fl.extend(value.to_le_bytes());
        }
        fl.extend(parent.to_le_bytes());
        fl.extend(0u32.to_le_bytes());
    }
    let mut frame_list = chunk(1, &fl);
    for (name, _) in &nodes {
        frame_list.extend(chunk(3, &chunk(0x0253_f2fe, name.as_bytes())));
    }
    let mut g = [0u32, 1, 3, 1]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    g.extend([1u16, 0, 0, 2].into_iter().flat_map(u16::to_le_bytes));
    g.extend([0u32, 0, 0, 0, 1, 1].into_iter().flat_map(u32::to_le_bytes));
    for v in [
        0.0f32, 0.0, 0.0, 0.2, 0.0, 0.0, 0.0, 0.2, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0,
    ] {
        g.extend(v.to_le_bytes());
    }
    let mut md = vec![0; 28];
    md[4..8].fill(255);
    let ml = chunk(
        8,
        &[
            chunk(1, &[1, 0, 0, 0, 255, 255, 255, 255]),
            chunk(7, &[chunk(1, &md), chunk(3, &[])].concat()),
        ]
        .concat(),
    );
    let geo = chunk(0xf, &[chunk(1, &g), ml].concat());
    let atomics: Vec<u8> = (1..=5u32)
        .flat_map(|f| {
            chunk(
                0x14,
                &chunk(
                    1,
                    &[f, 0, 4, 0]
                        .into_iter()
                        .flat_map(u32::to_le_bytes)
                        .collect::<Vec<_>>(),
                ),
            )
        })
        .collect();
    let bytes = chunk(
        0x10,
        &[
            chunk(1, &[5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            chunk(0xe, &frame_list),
            chunk(0x1a, &geo),
            atomics,
        ]
        .concat(),
    );
    let model = decode(&bytes).expect("vehicle with variants");
    assert_eq!(model.faces.len(), 4);
    assert!(model.interactables.iter().any(|p| p.label == "ПЛ дверь"));
}

#[test]
fn bin_mesh_splits_override_zeroed_legacy_triangle_materials() {
    let mut fl = 1u32.to_le_bytes().to_vec();
    for v in [
        1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0,
    ] {
        fl.extend(v.to_le_bytes());
    }
    fl.extend((-1i32).to_le_bytes());
    fl.extend(0u32.to_le_bytes());
    let frame_list = chunk(0xe, &[chunk(1, &fl), chunk(3, &chunk(0x0253_f2fe, b"chassis\0"))].concat());

    let mut g = [0u32, 2, 4, 1]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    g.extend([1u16, 0, 0, 2, 2, 1, 0, 3].into_iter().flat_map(u16::to_le_bytes));
    g.extend([0u32, 0, 0, 0, 1, 0].into_iter().flat_map(u32::to_le_bytes));
    for v in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0] {
        g.extend(v.to_le_bytes());
    }
    let mut md = vec![0; 28];
    md[4..8].fill(255);
    let mat = |t: &[u8]| {
        let mut d = md.clone();
        d[12..16].copy_from_slice(&1u32.to_le_bytes());
        let tex = chunk(
            6,
            &[chunk(1, &[0; 4]), chunk(2, t), chunk(2, &[0; 4]), chunk(3, &[])].concat(),
        );
        chunk(7, &[chunk(1, &d), tex, chunk(3, &[])].concat())
    };
    let mlist = chunk(
        8,
        &[
            chunk(1, &[2, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255]),
            mat(b"a\0"),
            mat(b"remap_tun\0"),
        ]
        .concat(),
    );
    let bin = [
        0u32, 2, 6, 3, 0, 0, 1, 2, 3, 1, 1, 2, 3,
    ]
    .into_iter()
    .flat_map(u32::to_le_bytes)
    .collect::<Vec<_>>();
    let geo = chunk(0xf, &[chunk(1, &g), mlist, chunk(3, &chunk(0x050e, &bin))].concat());
    let atomic = chunk(
        0x14,
        &chunk(
            1,
            &[0u32, 0, 4, 0]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        ),
    );
    let bytes = chunk(
        0x10,
        &[
            chunk(1, &[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            frame_list,
            chunk(0x1a, &geo),
            atomic,
        ]
        .concat(),
    );
    let model = decode(&bytes).expect("binmesh clump");
    assert_eq!(model.faces.len(), 2);
    assert_eq!(model.faces[0].material, 0);
    assert_eq!(model.faces[1].material, 1);
    assert!(is_remap_texture(&model.materials[1].texture));
    assert!(!is_remap_texture("remap_nocolor"));
}

#[test]
fn separate_left_wheel_cap_turns_outward_around_its_origin() {
    let mut fl = 3u32.to_le_bytes().to_vec();
    for v in [1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0] {
        fl.extend(v.to_le_bytes());
    }
    fl.extend((-1i32).to_le_bytes());
    fl.extend(0u32.to_le_bytes());
    for v in [1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, -0.98, 2.0, -1.0] {
        fl.extend(v.to_le_bytes());
    }
    fl.extend(0i32.to_le_bytes());
    fl.extend(0u32.to_le_bytes());
    for v in [1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.98, 2.0, -1.0] {
        fl.extend(v.to_le_bytes());
    }
    fl.extend(0i32.to_le_bytes());
    fl.extend(0u32.to_le_bytes());

    let mut geo = [0u32, 1, 3, 1]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    geo.extend([1u16, 0, 0, 2].into_iter().flat_map(u16::to_le_bytes));
    geo.extend([0u32, 0, 0, 0, 1, 1].into_iter().flat_map(u32::to_le_bytes));
    for v in [
        0.15f32, 0.0, 0.0, 0.15, 0.2, 0.0, 0.15, 0.0, 0.2, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0,
        0.0,
    ] {
        geo.extend(v.to_le_bytes());
    }
    let mut mat = vec![0u8; 28];
    mat[4..8].fill(255);
    let mlist = chunk(
        8,
        &[
            chunk(1, &[1, 0, 0, 0, 255, 255, 255, 255]),
            chunk(7, &[chunk(1, &mat), chunk(3, &[])].concat()),
        ]
        .concat(),
    );
    let g = chunk(0xf, &[chunk(1, &geo), mlist].concat());
    let name_ext = |s: &[u8]| chunk(3, &chunk(0x0253_f2fe, s));
    let frame_list = chunk(
        0xe,
        &[
            chunk(1, &fl),
            name_ext(b"chassis\0"),
            name_ext(b"wheel_lf_cap\0"),
            name_ext(b"wheel_rf_dummy\0"),
        ]
        .concat(),
    );
    let atomic = chunk(
        0x14,
        &chunk(
            1,
            &[1u32, 0, 4, 0]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        ),
    );
    let bytes = chunk(
        0x10,
        &[
            chunk(1, &[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            frame_list,
            chunk(0x1a, &g),
            atomic,
        ]
        .concat(),
    );
    let model = decode(&bytes).expect("cap model");
    assert!((model.faces[0].points[0][0] - (-1.13)).abs() < 1e-4);
}

#[test]
fn lying_ped_with_skeleton_becomes_upright() {
    let mut fl = 3u32.to_le_bytes().to_vec();
    for v in [1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0] {
        fl.extend(v.to_le_bytes());
    }
    fl.extend((-1i32).to_le_bytes());
    fl.extend(0u32.to_le_bytes());
    for v in [1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.7, 0.0, 0.0] {
        fl.extend(v.to_le_bytes());
    }
    fl.extend(0i32.to_le_bytes());
    fl.extend(0u32.to_le_bytes());
    for v in [1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, -0.9, 0.0, 0.1] {
        fl.extend(v.to_le_bytes());
    }
    fl.extend(0i32.to_le_bytes());
    fl.extend(0u32.to_le_bytes());

    let mut geo = [0u32, 1, 3, 1]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    geo.extend([1u16, 0, 0, 2].into_iter().flat_map(u16::to_le_bytes));
    geo.extend([0u32, 0, 0, 0, 1, 1].into_iter().flat_map(u32::to_le_bytes));
    for v in [
        -0.9f32, -0.1, 0.0, 0.7, 0.1, 0.0, 0.0, 0.0, 0.2, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0,
        1.0,
    ] {
        geo.extend(v.to_le_bytes());
    }
    let mut mat = vec![0u8; 28];
    mat[4..8].fill(255);
    let mlist = chunk(
        8,
        &[
            chunk(1, &[1, 0, 0, 0, 255, 255, 255, 255]),
            chunk(7, &[chunk(1, &mat), chunk(3, &[])].concat()),
        ]
        .concat(),
    );
    let g = chunk(0xf, &[chunk(1, &geo), mlist].concat());
    let name_ext = |s: &[u8]| chunk(3, &chunk(0x0253_f2fe, s));
    let frame_list = chunk(
        0xe,
        &[
            chunk(1, &fl),
            name_ext(b"pelvis\0"),
            name_ext(b"head\0"),
            name_ext(b"l foot\0"),
        ]
        .concat(),
    );
    let atomic = chunk(
        0x14,
        &chunk(
            1,
            &[0u32, 0, 4, 0]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        ),
    );
    let bytes = chunk(
        0x10,
        &[
            chunk(1, &[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            frame_list,
            chunk(0x1a, &g),
            atomic,
        ]
        .concat(),
    );

    let model = decode(&bytes).expect("ped model");
    let z_foot = model.faces[0].points[0][2];
    let z_head = model.faces[0].points[1][2];
    assert!(z_head > z_foot + 1.0);
}

#[test]
fn skinned_ped_with_upright_frame_tree_but_lying_mesh_becomes_upright() {
    let mut fl = 4u32.to_le_bytes().to_vec();
    for v in [1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0] {
        fl.extend(v.to_le_bytes());
    }
    fl.extend((-1i32).to_le_bytes());
    fl.extend(0u32.to_le_bytes());
    for v in [1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0] {
        fl.extend(v.to_le_bytes());
    }
    fl.extend(0i32.to_le_bytes());
    fl.extend(0u32.to_le_bytes());
    for v in [1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.7] {
        fl.extend(v.to_le_bytes());
    }
    fl.extend(1i32.to_le_bytes());
    fl.extend(0u32.to_le_bytes());
    for v in [1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, -0.9] {
        fl.extend(v.to_le_bytes());
    }
    fl.extend(1i32.to_le_bytes());
    fl.extend(0u32.to_le_bytes());

    let hanim_ext = |name: &[u8], node_id: u32, bones: &[(u32, u32)]| -> Vec<u8> {
        let mut h = Vec::new();
        h.extend(0x0100u32.to_le_bytes());
        h.extend(node_id.to_le_bytes());
        h.extend((bones.len() as u32).to_le_bytes());
        if !bones.is_empty() {
            h.extend(0u32.to_le_bytes());
            h.extend(36u32.to_le_bytes());
            for &(bid, bidx) in bones {
                h.extend(bid.to_le_bytes());
                h.extend(bidx.to_le_bytes());
                h.extend(0u32.to_le_bytes());
            }
        }
        chunk(3, &[chunk(0x0253_f2fe, name), chunk(0x011e, &h)].concat())
    };
    let frame_list = chunk(
        0xe,
        &[
            chunk(1, &fl),
            chunk(3, &[]),
            hanim_ext(b"pelvis\0", 1, &[(1, 0), (2, 1), (3, 2)]),
            hanim_ext(b"head\0", 2, &[]),
            hanim_ext(b"l foot\0", 3, &[]),
        ]
        .concat(),
    );

    let mut geo = [0u32, 1, 3, 1]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    geo.extend([1u16, 0, 0, 2].into_iter().flat_map(u16::to_le_bytes));
    geo.extend([0u32, 0, 0, 0, 1, 1].into_iter().flat_map(u32::to_le_bytes));
    for v in [
        -0.9f32, -0.1, 0.0, 0.7, 0.1, 0.0, 0.0, 0.0, 0.2, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0,
        1.0,
    ] {
        geo.extend(v.to_le_bytes());
    }
    let mut skin = vec![3u8, 0, 0, 0];
    skin.extend([2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]);
    for _ in 0..3 {
        skin.extend(1.0f32.to_le_bytes());
        skin.extend([0u8; 12]);
    }
    let mut mat = vec![0u8; 28];
    mat[4..8].fill(255);
    let mlist = chunk(
        8,
        &[
            chunk(1, &[1, 0, 0, 0, 255, 255, 255, 255]),
            chunk(7, &[chunk(1, &mat), chunk(3, &[])].concat()),
        ]
        .concat(),
    );
    let g = chunk(
        0xf,
        &[chunk(1, &geo), mlist, chunk(3, &chunk(0x0116, &skin))].concat(),
    );
    let atomic = chunk(
        0x14,
        &chunk(
            1,
            &[0u32, 0, 4, 0]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        ),
    );
    let bytes = chunk(
        0x10,
        &[
            chunk(1, &[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            frame_list,
            chunk(0x1a, &g),
            atomic,
        ]
        .concat(),
    );

    let model = decode(&bytes).expect("skinned ped model");
    let z_foot = model.faces[0].points[0][2];
    let z_head = model.faces[0].points[1][2];
    assert!(z_head > z_foot + 1.0);
}

#[test]
fn detects_doors_hood_and_trunk_with_universal_hinges() {
    let nodes = [
        ("chassis_dummy", -1i32, [0.0f32, 0.0, 0.0]),
        ("door_lf_dummy", 0, [-0.9, 0.8, 0.0]),
        ("customcar_door_lf_ok", 1, [0.0, 0.0, 0.0]),
        ("bonnet_dummy", 0, [0.0, 1.0, 0.4]),
        ("boot_dummy", 0, [0.0, -1.5, 0.4]),
    ];
    let mut fl = (nodes.len() as u32).to_le_bytes().to_vec();
    for (_, parent, pos) in &nodes {
        for v in [1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]
            .into_iter()
            .chain(*pos)
        {
            fl.extend(v.to_le_bytes());
        }
        fl.extend(parent.to_le_bytes());
        fl.extend(0u32.to_le_bytes());
    }
    let mut frame_list = chunk(1, &fl);
    for (name, _, _) in &nodes {
        frame_list.extend(chunk(3, &chunk(0x0253_f2fe, name.as_bytes())));
    }
    let make_panel = |p0: [f32; 3], p1: [f32; 3], p2: [f32; 3]| {
        let mut g = [0u32, 1, 3, 1]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        g.extend([1u16, 0, 0, 2].into_iter().flat_map(u16::to_le_bytes));
        g.extend([0u32, 0, 0, 0, 1, 1].into_iter().flat_map(u32::to_le_bytes));
        for v in p0
            .into_iter()
            .chain(p1)
            .chain(p2)
            .chain([0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0])
        {
            g.extend(v.to_le_bytes());
        }
        let mut md = vec![0; 28];
        md[4..8].fill(255);
        let ml = chunk(
            8,
            &[
                chunk(1, &[1, 0, 0, 0, 255, 255, 255, 255]),
                chunk(7, &[chunk(1, &md), chunk(3, &[])].concat()),
            ]
            .concat(),
        );
        chunk(0xf, &[chunk(1, &g), ml].concat())
    };
    let geos = [
        make_panel([0.0, -0.5, 0.0], [0.1, -0.5, 0.5], [0.0, -0.8, 0.0]),
        make_panel([-0.4, 0.6, 0.0], [0.4, 0.6, 0.0], [0.0, 0.8, 0.0]),
        make_panel([-0.4, -0.6, 0.0], [0.4, -0.6, 0.0], [0.0, -0.8, 0.0]),
    ]
    .concat();
    let atomics = [(2u32, 0u32), (3, 1), (4, 2)]
        .into_iter()
        .flat_map(|(f, g)| {
            chunk(
                0x14,
                &chunk(
                    1,
                    &[f, g, 4, 0]
                        .into_iter()
                        .flat_map(u32::to_le_bytes)
                        .collect::<Vec<_>>(),
                ),
            )
        })
        .collect::<Vec<_>>();
    let bytes = chunk(
        0x10,
        &[
            chunk(1, &[3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            chunk(0xe, &frame_list),
            chunk(0x1a, &geos),
            atomics,
        ]
        .concat(),
    );
    let model = decode(&bytes).expect("car doors");
    assert!(model.is_vehicle);
    let labels: Vec<_> = model.interactables.iter().map(|p| p.label.as_str()).collect();
    assert_eq!(labels, ["Капот", "Багажник", "ПЛ дверь"]);
    assert!(model.interactables[0].max_angle_rad > 0.0);
    assert!(model.interactables[1].max_angle_rad < 0.0);
    assert!(model.interactables[2].max_angle_rad < 0.0);
}

#[test]
fn bus_front_lid_and_aircraft_canopy_gear_orient_correctly() {
    let nodes = [
        ("chassis_dummy", -1i32, [0.0f32, 0.0, 0.0]),
        ("bonnet_dummy", 0, [0.0, 3.5, 0.1]),
        ("door_lf_dummy", 0, [0.3, 2.3, 0.9]),
        ("gear_l", 0, [-1.4, -1.8, -0.1]),
        ("wheel_lb_dummy", 3, [-1.5, -2.0, -1.3]),
        ("wheel_lb", 4, [0.0, 0.0, 0.0]),
    ];
    let mut fl = (nodes.len() as u32).to_le_bytes().to_vec();
    for (_, parent, pos) in &nodes {
        for v in [1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]
            .into_iter()
            .chain(*pos)
        {
            fl.extend(v.to_le_bytes());
        }
        fl.extend(parent.to_le_bytes());
        fl.extend(0u32.to_le_bytes());
    }
    let mut frame_list = chunk(1, &fl);
    for (name, _, _) in &nodes {
        frame_list.extend(chunk(3, &chunk(0x0253_f2fe, name.as_bytes())));
    }
    let make_panel = |p0: [f32; 3], p1: [f32; 3], p2: [f32; 3]| {
        let mut g = [0u32, 1, 3, 1]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        g.extend([1u16, 0, 0, 2].into_iter().flat_map(u16::to_le_bytes));
        g.extend([0u32, 0, 0, 0, 1, 1].into_iter().flat_map(u32::to_le_bytes));
        for v in p0
            .into_iter()
            .chain(p1)
            .chain(p2)
            .chain([0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0])
        {
            g.extend(v.to_le_bytes());
        }
        let mut md = vec![0; 28];
        md[4..8].fill(255);
        let ml = chunk(
            8,
            &[
                chunk(1, &[1, 0, 0, 0, 255, 255, 255, 255]),
                chunk(7, &[chunk(1, &md), chunk(3, &[])].concat()),
            ]
            .concat(),
        );
        chunk(0xf, &[chunk(1, &g), ml].concat())
    };
    let geos = [
        // Вертикальная крышка автобуса: висит вниз -Z при y = 0 относительно bonnet_dummy
        make_panel([-0.3, 0.0, -0.1], [0.3, 0.0, -0.1], [0.0, 0.0, -0.7]),
        // Фонарь кабины: петля справа, стекло слева (-X)
        make_panel([-0.5, -0.4, 0.1], [-0.5, 0.4, 0.1], [-0.1, 0.0, 0.2]),
        // Колесо шасси под gear_l
        make_panel([0.0, -0.2, 0.0], [0.0, 0.2, 0.0], [0.0, 0.0, 0.2]),
    ]
    .concat();
    let atomics = [(1u32, 0u32), (2, 1), (5, 2)]
        .into_iter()
        .flat_map(|(f, g)| {
            chunk(
                0x14,
                &chunk(
                    1,
                    &[f, g, 4, 0]
                        .into_iter()
                        .flat_map(u32::to_le_bytes)
                        .collect::<Vec<_>>(),
                ),
            )
        })
        .collect::<Vec<_>>();
    let bytes = chunk(
        0x10,
        &[
            chunk(1, &[3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            chunk(0xe, &frame_list),
            chunk(0x1a, &geos),
            atomics,
        ]
        .concat(),
    );
    let model = decode(&bytes).expect("bus/aircraft");
    let hood = &model.interactables[0];
    assert_eq!(hood.label, "Капот");
    assert_eq!(hood.axis, [1.0, 0.0, 0.0]);
    assert!(hood.max_angle_rad > 0.0);

    let canopy = &model.interactables[1];
    assert_eq!(canopy.label, "ПЛ дверь");
    assert_eq!(canopy.axis, [0.0, 1.0, 0.0]);
    assert!(canopy.max_angle_rad > 0.0);

    let gear = &model.interactables[2];
    assert_eq!(gear.label, "Шасси (лев.)");
    assert_eq!(gear.axis, [0.0, 1.0, 0.0]);
    assert!(gear.max_angle_rad < 0.0);
    // Колесо внутри gear_l должно двигаться вместе со стойкой шасси.
    assert_eq!(model.faces[2].interactable, Some(2));
}
