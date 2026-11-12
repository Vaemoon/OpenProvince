mod chunks;
mod components;
mod doors;
mod skin;

use self::{
    chunks::{
        Frame, atomic_norm_basis, atomic_pos_matrix, child, chunks, f32_at, geometry,
        left_wheel_part_ancestor, transform, u32_at, wheel_ancestor,
    },
    components::{base_frame_name, classify_component, is_permanent_damage_or_lod, is_restyle_name},
    doors::{finalize_interactables, interactable_ancestor},
    skin::{accumulate_skin_centroids, orient_ped_upright},
};
use eframe::egui;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
pub struct Material {
    pub texture: String,
    pub color: [u8; 4],
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ComponentKind {
    StockVariant,
    Restyle,
    Tuning,
    Extra,
}

#[derive(Clone, Debug)]
pub struct ComponentGroup {
    pub key: String,
    pub label: String,
    pub kind: ComponentKind,
    pub active_by_default: bool,
}

#[derive(Clone, Debug)]
pub struct InteractablePart {
    pub label: String,
    pub pivot: [f32; 3],
    pub axis: [f32; 3],
    pub max_angle_rad: f32,
}

#[derive(Clone)]
pub struct Face {
    pub points: [[f32; 3]; 3],
    pub uv: [[f32; 2]; 3],
    pub material: usize,
    pub normals: [[f32; 3]; 3],
    pub colors: [[u8; 4]; 3],
    pub component: Option<usize>,
    pub interactable: Option<usize>,
}

pub struct Model {
    pub faces: Vec<Face>,
    pub materials: Vec<Material>,
    pub components: Vec<ComponentGroup>,
    pub interactables: Vec<InteractablePart>,
    pub is_vehicle: bool,
    pub center: [f32; 3],
    pub radius: f32,
    pub vertices: usize,
    pub discarded_triangles: usize,
}

impl Model {
    pub fn combine(parts: Vec<Model>) -> Self {
        let mut combined = Model {
            faces: Vec::new(),
            materials: Vec::new(),
            components: Vec::new(),
            interactables: Vec::new(),
            is_vehicle: false,
            center: [0.0; 3],
            radius: 1.0,
            vertices: 0,
            discarded_triangles: 0,
        };
        let mut low = [f32::INFINITY; 3];
        let mut high = [f32::NEG_INFINITY; 3];
        for part in parts {
            combined.is_vehicle |= part.is_vehicle;
            let mat_base = combined.materials.len();
            let comp_base = combined.components.len();
            let part_base = combined.interactables.len();
            combined.materials.extend(part.materials);
            combined.components.extend(part.components);
            combined.interactables.extend(part.interactables);
            combined.vertices += part.vertices;
            combined.discarded_triangles += part.discarded_triangles;
            for mut face in part.faces {
                face.material += mat_base;
                face.component = face.component.map(|c| c + comp_base);
                face.interactable = face.interactable.map(|i| i + part_base);
                for point in &face.points {
                    for i in 0..3 {
                        if point[i].is_finite() {
                            low[i] = low[i].min(point[i]);
                            high[i] = high[i].max(point[i]);
                        }
                    }
                }
                combined.faces.push(face);
            }
        }
        if low[0].is_finite() {
            for i in 0..3 {
                combined.center[i] = (low[i] + high[i]) * 0.5;
            }
            combined.radius = (0..3)
                .map(|i| (high[i] - low[i]).powi(2))
                .sum::<f32>()
                .sqrt()
                .max(0.01)
                * 0.5;
        }
        combined
    }
}

pub fn is_remap_texture(name: &str) -> bool {
    let lower = name.trim().to_ascii_lowercase();
    !lower.contains("nocolor")
        && (lower.starts_with('#') || lower.starts_with("remap") || lower.ends_with("_remap"))
}

fn has_vehicle_frames(frames: &[Frame]) -> bool {
    frames.iter().any(|f| {
        let n = f.name.trim();
        (n.starts_with("wheel_") && n.ends_with("_dummy"))
            || matches!(
                n,
                "chassis"
                    | "chassis_dummy"
                    | "wheel_front"
                    | "wheel_rear"
                    | "handlebar"
                    | "forks_front"
                    | "door_lf_dummy"
                    | "door_rf_dummy"
                    | "bonnet_dummy"
                    | "boot_dummy"
                    | "ped_frontseat"
                    | "headlights"
                    | "taillights"
                    | "moving_rotor"
                    | "static_rotor"
                    | "moving_prop"
                    | "gear_l"
                    | "gear_r"
            )
    })
}

pub fn decode(bytes: &[u8]) -> Result<Model, String> {
    decode_with_wheels(bytes, [1.0; 2])
}

pub fn decode_with_wheels(mut bytes: &[u8], wheel_sizes: [f32; 2]) -> Result<Model, String> {
    if u32_at(bytes, 0)? == 0x2b {
        let end = 12usize
            .checked_add(u32_at(bytes, 4)? as usize)
            .ok_or("UV dictionary overflow")?;
        bytes = bytes
            .get(end..)
            .ok_or("Truncated UV animation dictionary")?;
    }
    if !wheel_sizes
        .iter()
        .all(|size| size.is_finite() && *size > 0.0)
    {
        return Err("Invalid vehicle wheel dimensions".into());
    }
    if u32_at(bytes, 0)? != 0x10 {
        return Err("Expected a RenderWare clump".into());
    }
    let size = u32_at(bytes, 4)? as usize;
    let clump = bytes
        .get(12..12usize.checked_add(size).ok_or("Clump overflow")?)
        .ok_or("Truncated clump")?;
    let normalized;
    let clump = if u32_at(clump, 0)? == 1
        && u32_at(clump, 4)? == 0x0100_000c
        && u32_at(clump, 24)? == 0xe
    {
        normalized = {
            let mut data = clump.to_vec();
            data[4..8].copy_from_slice(&12u32.to_le_bytes());
            data
        };
        normalized.as_slice()
    } else {
        clump
    };
    let list = child(clump, 0xe)?;
    let data = child(list, 1)?;
    let count = u32_at(data, 0)? as usize;
    if count > 4096 {
        return Err("Too many DFF frames".into());
    }
    let mut frames = Vec::new();
    for i in 0..count {
        let p = 4 + i * 56;
        let mut matrix = [0.0; 12];
        for (j, value) in matrix.iter_mut().enumerate() {
            *value = f32_at(data, p + j * 4)?;
        }
        frames.push(Frame {
            matrix,
            parent: u32_at(data, p + 48)? as i32,
            name: String::new(),
            hanim_id: None,
        });
    }
    let mut hanim_bones: Vec<(u32, u8)> = Vec::new();
    for (frame, (_, extension)) in frames
        .iter_mut()
        .zip(chunks(list)?.into_iter().filter(|(id, _)| *id == 3))
    {
        for (ext_id, ext_data) in chunks(extension)? {
            if ext_id == 0x0253_f2fe {
                frame.name = String::from_utf8_lossy(
                    ext_data.split(|b| *b == 0).next().unwrap_or_default(),
                )
                .to_ascii_lowercase();
            } else if ext_id == 0x011e && ext_data.len() >= 12 {
                let node_id = u32_at(ext_data, 4)?;
                let num_nodes = u32_at(ext_data, 8)? as usize;
                frame.hanim_id = Some(node_id);
                if hanim_bones.is_empty()
                    && num_nodes > 0
                    && num_nodes <= 512
                    && ext_data.len() >= 20 + num_nodes * 12
                {
                    for b in 0..num_nodes {
                        let off = 20 + b * 12;
                        let bone_id = u32_at(ext_data, off)?;
                        let bone_index = u32_at(ext_data, off + 4)?;
                        if let Ok(idx) = u8::try_from(bone_index) {
                            hanim_bones.push((bone_id, idx));
                        }
                    }
                }
            }
        }
    }
    let max_bone = hanim_bones.iter().map(|&(_, i)| i as usize).max().unwrap_or(0);
    let mut bone_to_frame: Vec<Option<usize>> = if hanim_bones.is_empty() {
        Vec::new()
    } else {
        vec![None; max_bone + 1]
    };
    for (bone_id, bone_idx) in hanim_bones {
        if let Some(frame_idx) = frames.iter().position(|f| f.hanim_id == Some(bone_id)) {
            bone_to_frame[bone_idx as usize] = Some(frame_idx);
        }
    }

    let geometries = chunks(child(clump, 0x1a)?)?
        .into_iter()
        .filter(|(id, _)| *id == 0xf)
        .map(|(_, b)| b)
        .collect::<Vec<_>>();
    let has_wheel_dummies = frames
        .iter()
        .any(|f| f.name.starts_with("wheel_") && f.name.ends_with("_dummy"));
    let is_vehicle = has_wheel_dummies || has_vehicle_frames(&frames);
    let is_aircraft = frames.iter().any(|f| {
        matches!(
            f.name.as_str(),
            "gear_l"
                | "gear_r"
                | "aileron_l"
                | "aileron_r"
                | "elevator_l"
                | "elevator_r"
                | "rudder"
                | "static_rotor"
                | "moving_rotor"
                | "static_rotor2"
                | "moving_rotor2"
        )
    });
    let has_restyle = frames
        .iter()
        .any(|f| is_restyle_name(base_frame_name(&f.name)));
    let mut model = Model {
        faces: Vec::new(),
        materials: Vec::new(),
        components: Vec::new(),
        interactables: Vec::new(),
        is_vehicle,
        center: [0.0; 3],
        radius: 1.0,
        vertices: 0,
        discarded_triangles: 0,
    };
    let mut component_map = BTreeMap::<String, usize>::new();
    let mut low = [f32::INFINITY; 3];
    let mut high = [f32::NEG_INFINITY; 3];
    let mut fallback_low = [f32::INFINITY; 3];
    let mut fallback_high = [f32::NEG_INFINITY; 3];
    let mut wheel_components: BTreeMap<i32, Vec<(usize, Face)>> = Default::default();
    let mut standalone_left_wheels: BTreeMap<i32, Vec<usize>> = Default::default();
    let mut occupied_wheels = BTreeSet::new();
    let mut skin_centroids = BTreeMap::<String, ([f32; 3], f32)>::new();
    let mut face_interact_frames: Vec<Option<usize>> = Vec::new();

    for (_, atomic) in chunks(clump)?.into_iter().filter(|(id, _)| *id == 0x14) {
        let data = child(atomic, 1)?;
        let frame = u32_at(data, 0)? as i32;
        if u32_at(data, 8)? & 4 == 0 {
            continue;
        }
        let mut ancestor = frame;
        let mut hidden = false;
        for _ in 0..frames.len() {
            if ancestor < 0 {
                break;
            }
            let f = frames
                .get(ancestor as usize)
                .ok_or("Invalid frame hierarchy")?;
            if is_permanent_damage_or_lod(&f.name, is_vehicle) {
                hidden = true;
                break;
            }
            ancestor = f.parent;
        }
        if hidden {
            continue;
        }
        let component = if is_vehicle {
            classify_component(frame, &frames, has_restyle).map(|spec| {
                if let Some(&idx) = component_map.get(&spec.key) {
                    idx
                } else {
                    let idx = model.components.len();
                    component_map.insert(spec.key.clone(), idx);
                    model.components.push(spec);
                    idx
                }
            })
        } else {
            None
        };
        let default_visible = component.is_none_or(|idx| model.components[idx].active_by_default);
        let wheel = wheel_ancestor(frame, &frames)?;
        let interact_frame = if is_vehicle {
            interactable_ancestor(frame, &frames)
        } else {
            None
        };
        let scale = wheel.map_or(1.0, |index| {
            if frames[index as usize].name.contains("f_dummy") {
                wheel_sizes[0]
            } else {
                wheel_sizes[1]
            }
        });
        let geo_chunk = *geometries
            .get(u32_at(data, 4)? as usize)
            .ok_or("Invalid atomic geometry")?;
        let geometry = geometry(geo_chunk)?;
        let base = model.materials.len();
        let face_start = model.faces.len();
        model.materials.extend(geometry.materials.iter().cloned());
        let vertices = if geometry.vertices.is_empty() {
            Vec::new()
        } else {
            let [origin, ax, ay, az] = atomic_pos_matrix(frame, wheel, scale, &frames)?;
            geometry
                .vertices
                .iter()
                .map(|p| {
                    [
                        origin[0] + ax[0] * p[0] + ay[0] * p[1] + az[0] * p[2],
                        origin[1] + ax[1] * p[0] + ay[1] * p[1] + az[1] * p[2],
                        origin[2] + ax[2] * p[0] + ay[2] * p[1] + az[2] * p[2],
                    ]
                })
                .collect()
        };
        let norm_basis = (!geometry.triangles.is_empty())
            .then(|| atomic_norm_basis(frame, wheel, &frames));
        let mut norm_cache: Vec<Option<[f32; 3]>> = vec![None; geometry.normals.len()];
        if !is_vehicle && !bone_to_frame.is_empty() {
            accumulate_skin_centroids(
                geo_chunk,
                &vertices,
                &bone_to_frame,
                &frames,
                &mut skin_centroids,
            );
        }
        model.vertices += vertices.len();
        if model.vertices > 1_000_000 || model.faces.len() + geometry.triangles.len() > 500_000 {
            return Err("Clump exceeds preview budget".into());
        }
        for point in &vertices {
            if point.iter().any(|v| !v.is_finite()) {
                continue;
            }
            for i in 0..3 {
                fallback_low[i] = fallback_low[i].min(point[i]);
                fallback_high[i] = fallback_high[i].max(point[i]);
                if default_visible {
                    low[i] = low[i].min(point[i]);
                    high[i] = high[i].max(point[i]);
                }
            }
        }
        for (indices, material) in &geometry.triangles {
            if indices
                .iter()
                .any(|i| vertices[*i].iter().any(|v| !v.is_finite()))
            {
                model.discarded_triangles += 1;
                continue;
            }
            if *material >= geometry.materials.len() {
                return Err("Invalid triangle material".into());
            }
            let points = indices.map(|i| vertices[i]);
            let a = std::array::from_fn::<_, 3, _>(|i| points[1][i] - points[0][i]);
            let b = std::array::from_fn::<_, 3, _>(|i| points[2][i] - points[0][i]);
            let fallback = [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ];
            let mut transformed_normal = |i: usize| -> Result<[f32; 3], String> {
                if let Some(cached) = norm_cache[i] {
                    return Ok(cached);
                }
                if geometry.normals[i].iter().all(|n| n.abs() < 0.00001) {
                    return Ok(fallback);
                }
                let nb = norm_basis
                    .as_ref()
                    .expect("norm_basis computed for non-empty triangles")
                    .as_ref()
                    .map_err(Clone::clone)?;
                let n = geometry.normals[i];
                let out = [
                    nb[0][0] * n[0] + nb[1][0] * n[1] + nb[2][0] * n[2],
                    nb[0][1] * n[0] + nb[1][1] * n[1] + nb[2][1] * n[2],
                    nb[0][2] * n[0] + nb[1][2] * n[1] + nb[2][2] * n[2],
                ];
                norm_cache[i] = Some(out);
                Ok(out)
            };
            let normals = [
                transformed_normal(indices[0])?,
                transformed_normal(indices[1])?,
                transformed_normal(indices[2])?,
            ];
            model.faces.push(Face {
                points,
                uv: indices.map(|i| geometry.uv[i]),
                material: base + material,
                normals,
                colors: indices.map(|i| geometry.colors[i]),
                component,
                interactable: None,
            });
            face_interact_frames.push(interact_frame);
        }
        if let Some(dummy) = wheel {
            occupied_wheels.insert(dummy);
            let entry = wheel_components.entry(dummy).or_default();
            for (idx, face) in model.faces[face_start..].iter().cloned().enumerate() {
                entry.push((face_start + idx, face));
            }
        } else if has_wheel_dummies
            && let Some(part) = left_wheel_part_ancestor(frame, &frames)?
        {
            standalone_left_wheels
                .entry(part)
                .or_default()
                .extend(face_start..model.faces.len());
        }
    }

    for (&dummy, wheel_faces) in &mut wheel_components {
        if !frames[dummy as usize].name.starts_with("wheel_l") {
            continue;
        }
        let origin = transform([0.0; 3], dummy, &frames)?;
        let indices: Vec<usize> = wheel_faces.iter().map(|(i, _)| *i).collect();
        if turn_left_wheel_around_origin(
            &mut model.faces,
            &indices,
            origin,
            &mut fallback_low,
            &mut fallback_high,
            &mut low,
            &mut high,
        ) {
            for (face_idx, cached_face) in wheel_faces.iter_mut() {
                *cached_face = model.faces[*face_idx].clone();
            }
        }
    }
    for (part_frame, indices) in &standalone_left_wheels {
        let origin = transform([0.0; 3], *part_frame, &frames)?;
        turn_left_wheel_around_origin(
            &mut model.faces,
            indices,
            origin,
            &mut fallback_low,
            &mut fallback_high,
            &mut low,
            &mut high,
        );
    }

    if !is_aircraft
        && let Some((&source, faces)) = wheel_components
            .iter()
            .find(|(index, _)| frames[**index as usize].name == "wheel_rf_dummy")
            .or_else(|| wheel_components.first_key_value())
    {
        let faces: Vec<Face> = faces.iter().map(|(_, f)| f.clone()).collect();
        let origin = transform([0.0; 3], source, &frames)?;
        let basis: [[f32; 3]; 3] = [0, 1, 2]
            .map(|axis| {
                let mut p = [0.0; 3];
                p[axis] = 1.0;
                transform(p, source, &frames).map(|p| std::array::from_fn(|i| p[i] - origin[i]))
            })
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?
            .try_into()
            .map_err(|_| "Wheel basis")?;
        let cross = |a: [f32; 3], b: [f32; 3]| {
            [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]
        };
        let dot = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).map(|(a, b)| a * b).sum::<f32>();
        let mut hand = dot(cross(basis[0], basis[1]), basis[2]);
        if hand.abs() < 1e-6 {
            hand = 1.0;
        }
        let target_axes = [
            "wheel_rf_dummy",
            "wheel_lf_dummy",
            "wheel_rb_dummy",
            "wheel_lb_dummy",
        ];
        for (index, target) in frames.iter().enumerate() {
            if !target_axes.contains(&target.name.as_str())
                || occupied_wheels.contains(&(index as i32))
            {
                continue;
            }
            let target_origin = transform([0.0; 3], index as i32, &frames)?;
            let mut target_basis: [[f32; 3]; 3] = [0, 1, 2]
                .map(|axis| {
                    let mut p = [0.0; 3];
                    p[axis] = 1.0;
                    transform(p, index as i32, &frames)
                        .map(|p| std::array::from_fn(|i| p[i] - target_origin[i]))
                })
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?
                .try_into()
                .map_err(|_| "Target wheel basis")?;
            let target_hand = dot(cross(target_basis[0], target_basis[1]), target_basis[2]);
            let left = target.name.starts_with("wheel_l");
            if left && target_hand * hand > 0.0 && target_basis[0][0] > 0.0 {
                target_basis[0] = target_basis[0].map(|v| -v);
                target_basis[1] = target_basis[1].map(|v| -v);
            }
            let ratio = if target.name.contains("f_dummy") {
                wheel_sizes[0]
            } else {
                wheel_sizes[1]
            } / if frames[source as usize].name.contains("f_dummy") {
                wheel_sizes[0]
            } else {
                wheel_sizes[1]
            };
            let wheel_component = classify_component(index as i32, &frames, has_restyle)
                .map(|spec| {
                    if let Some(&idx) = component_map.get(&spec.key) {
                        idx
                    } else {
                        let idx = model.components.len();
                        component_map.insert(spec.key.clone(), idx);
                        model.components.push(spec);
                        idx
                    }
                });
            let default_visible =
                wheel_component.is_none_or(|idx| model.components[idx].active_by_default);
            for face in &faces {
                let mut copy = face.clone();
                copy.component = wheel_component.or(copy.component);
                for i in 0..3 {
                    let relative = std::array::from_fn(|k| (face.points[i][k] - origin[k]) * ratio);
                    let coeffs = basis.map(|axis| {
                        let len = dot(axis, axis).max(1e-6);
                        dot(relative, axis) / len
                    });
                    copy.points[i] = std::array::from_fn(|k| {
                        target_origin[k]
                            + coeffs[0] * target_basis[0][k]
                            + coeffs[1] * target_basis[1][k]
                            + coeffs[2] * target_basis[2][k]
                    });
                    let normal = face.normals[i];
                    let n_coeffs = basis.map(|axis| {
                        let len = dot(axis, axis).max(1e-6);
                        dot(normal, axis) / len
                    });
                    copy.normals[i] = std::array::from_fn(|k| {
                        n_coeffs[0] * target_basis[0][k]
                            + n_coeffs[1] * target_basis[1][k]
                            + n_coeffs[2] * target_basis[2][k]
                    });
                    for axis in 0..3 {
                        fallback_low[axis] = fallback_low[axis].min(copy.points[i][axis]);
                        fallback_high[axis] = fallback_high[axis].max(copy.points[i][axis]);
                        if default_visible {
                            low[axis] = low[axis].min(copy.points[i][axis]);
                            high[axis] = high[axis].max(copy.points[i][axis]);
                        }
                    }
                }
                model.faces.push(copy);
                face_interact_frames.push(None);
                if model.faces.len() > 500_000 {
                    return Err("Assembled wheels exceed preview budget".into());
                }
            }
        }
    }

    if model.faces.is_empty() {
        return Err("DFF contains no visible triangle atomics".into());
    }
    if !model.components.is_empty() {
        let mut order: Vec<usize> = (0..model.components.len()).collect();
        order.sort_by_key(|&idx| match model.components[idx].kind {
            ComponentKind::StockVariant => (0, model.components[idx].label.clone()),
            ComponentKind::Restyle => (1, model.components[idx].label.clone()),
            ComponentKind::Tuning => (2, model.components[idx].label.clone()),
            ComponentKind::Extra => (3, model.components[idx].label.clone()),
        });
        let mut inverse = vec![0usize; order.len()];
        for (new_idx, &old_idx) in order.iter().enumerate() {
            inverse[old_idx] = new_idx;
        }
        model.components = order
            .into_iter()
            .map(|idx| model.components[idx].clone())
            .collect();
        for face in &mut model.faces {
            if let Some(comp) = &mut face.component {
                *comp = inverse[*comp];
            }
        }
    }
    if !low[0].is_finite() {
        low = fallback_low;
        high = fallback_high;
    }
    if !is_vehicle {
        orient_ped_upright(&mut model, &frames, &skin_centroids, &mut low, &mut high);
    } else {
        model.interactables =
            finalize_interactables(&mut model.faces, &face_interact_frames, &frames);
    }
    for i in 0..3 {
        model.center[i] = (low[i] + high[i]) * 0.5;
    }
    model.radius = (0..3)
        .map(|i| (high[i] - low[i]).powi(2))
        .sum::<f32>()
        .sqrt()
        .max(0.01)
        * 0.5;
    Ok(model)
}

fn turn_left_wheel_around_origin(
    faces: &mut [Face],
    indices: &[usize],
    origin: [f32; 3],
    fallback_low: &mut [f32; 3],
    fallback_high: &mut [f32; 3],
    low: &mut [f32; 3],
    high: &mut [f32; 3],
) -> bool {
    if origin[0] >= -0.1 {
        return false;
    }
    let (mut min_rel_x, mut max_rel_x) = (f32::INFINITY, f32::NEG_INFINITY);
    for &face_idx in indices {
        for p in &faces[face_idx].points {
            let rel_x = p[0] - origin[0];
            min_rel_x = min_rel_x.min(rel_x);
            max_rel_x = max_rel_x.max(rel_x);
        }
    }
    if !min_rel_x.is_finite() || (min_rel_x + max_rel_x) <= 0.02 {
        return false;
    }
    for &face_idx in indices {
        let target = &mut faces[face_idx];
        for i in 0..3 {
            let dx = target.points[i][0] - origin[0];
            let dy = target.points[i][1] - origin[1];
            target.points[i][0] = origin[0] - dx;
            target.points[i][1] = origin[1] - dy;
            target.normals[i][0] = -target.normals[i][0];
            target.normals[i][1] = -target.normals[i][1];
            for axis in 0..3 {
                fallback_low[axis] = fallback_low[axis].min(target.points[i][axis]);
                fallback_high[axis] = fallback_high[axis].max(target.points[i][axis]);
                low[axis] = low[axis].min(target.points[i][axis]);
                high[axis] = high[axis].max(target.points[i][axis]);
            }
        }
    }
    true
}

pub struct View {
    renderer: crate::gpu::View,
}

impl View {
    pub fn new(model: Model) -> Self {
        Self {
            renderer: crate::gpu::View::new(model),
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        textures: &[(String, egui::TextureHandle)],
        paintjob: bool,
        vinyl: Option<&egui::TextureHandle>,
        vinyl_on_primary: bool,
        extra_controls: impl FnOnce(&mut egui::Ui),
    ) -> Option<egui::Response> {
        self.renderer
            .show(ui, textures, paintjob, vinyl, vinyl_on_primary, extra_controls)
    }
}
