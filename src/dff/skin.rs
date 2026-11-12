use super::{
    Model,
    chunks::{Frame, child, chunks, transform},
};
use std::collections::BTreeMap;

fn norm3(v: [f32; 3]) -> Option<[f32; 3]> {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    (len > 1e-4).then(|| [v[0] / len, v[1] / len, v[2] / len])
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Накапливает средние мировые координаты костей прямо по весам вершин из SkinPLG.
/// Это надёжнее FrameList, где корень скелета часто повернут независимо от меша.
pub(super) fn accumulate_skin_centroids(
    geometry_chunk: &[u8],
    world_vertices: &[[f32; 3]],
    bone_to_frame: &[Option<usize>],
    frames: &[Frame],
    centroids: &mut BTreeMap<String, ([f32; 3], f32)>,
) {
    let Ok(ext) = child(geometry_chunk, 3) else {
        return;
    };
    let Ok(ext_chunks) = chunks(ext) else {
        return;
    };
    let Some((_, skin)) = ext_chunks.into_iter().find(|(id, _)| *id == 0x0116) else {
        return;
    };
    if skin.len() < 4 {
        return;
    }
    let num_used = skin[1] as usize;
    let nverts = world_vertices.len();
    let Some(idx_off) = 4usize.checked_add(num_used) else {
        return;
    };
    let Some(w_off) = nverts.checked_mul(4).and_then(|v| idx_off.checked_add(v)) else {
        return;
    };
    let Some(end) = nverts.checked_mul(16).and_then(|v| w_off.checked_add(v)) else {
        return;
    };
    if end > skin.len() {
        return;
    }
    for (vi, &p) in world_vertices.iter().enumerate() {
        if !p.iter().all(|v| v.is_finite()) {
            continue;
        }
        let ibase = idx_off + vi * 4;
        let wbase = w_off + vi * 16;
        for k in 0..4 {
            let w = f32::from_le_bytes(skin[wbase + k * 4..wbase + k * 4 + 4].try_into().unwrap());
            if !w.is_finite() || w <= 0.01 {
                continue;
            }
            let bone = skin[ibase + k] as usize;
            let Some(fi) = bone_to_frame.get(bone).copied().flatten() else {
                continue;
            };
            let bname = frames[fi].name.trim();
            if bname.is_empty() {
                continue;
            }
            let entry = centroids.entry(bname.to_owned()).or_insert(([0.0; 3], 0.0));
            for (acc, coord) in entry.0.iter_mut().zip(p) {
                *acc += coord * w;
            }
            entry.1 += w;
        }
    }
}

/// Выравнивает лежачие и наклонённые скины педов вертикально вверх (+Z).
pub(super) fn orient_ped_upright(
    model: &mut Model,
    frames: &[Frame],
    skin_centroids: &BTreeMap<String, ([f32; 3], f32)>,
    low: &mut [f32; 3],
    high: &mut [f32; 3],
) {
    let has_skeleton = frames.iter().any(|f| {
        let t = f.name.trim();
        t == "pelvis" || t == "spine" || t.starts_with("bip01")
    });
    if !has_skeleton {
        return;
    }
    let span = [high[0] - low[0], high[1] - low[1], high[2] - low[2]];
    let max_span = span[0].max(span[1]).max(span[2]);
    if !max_span.is_finite() || max_span < 0.55 {
        return;
    }

    let use_skin = !skin_centroids.is_empty();
    let find_bone = |names: &[&str], suffix: Option<&str>| -> Option<[f32; 3]> {
        if use_skin {
            let mut sum = [0.0f32; 3];
            let mut weight = 0.0f32;
            for (bname, &(acc, w)) in skin_centroids {
                let matched =
                    names.contains(&bname.as_str()) || suffix.is_some_and(|s| bname.ends_with(s));
                if matched && w > 0.0 {
                    for axis in 0..3 {
                        sum[axis] += acc[axis];
                    }
                    weight += w;
                }
            }
            if weight > 0.0 {
                return Some([sum[0] / weight, sum[1] / weight, sum[2] / weight]);
            }
        } else {
            for (i, f) in frames.iter().enumerate() {
                let t = f.name.trim();
                let matched = names.contains(&t) || suffix.is_some_and(|s| t.ends_with(s));
                if matched && let Ok(p) = transform([0.0; 3], i as i32, frames) {
                    return Some(p);
                }
            }
        }
        None
    };

    struct SymPair {
        mid: Option<[f32; 3]>,
        left: Option<[f32; 3]>,
        right: Option<[f32; 3]>,
    }
    let sym_pair = |left: &[&str], right: &[&str]| -> SymPair {
        let l = find_bone(left, None);
        let r = find_bone(right, None);
        let mid = match (l, r) {
            (Some(a), Some(b)) => {
                Some([(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5, (a[2] + b[2]) * 0.5])
            }
            (Some(a), None) | (None, Some(a)) => Some(a),
            (None, None) => None,
        };
        SymPair {
            mid,
            left: l,
            right: r,
        }
    };

    let head = find_bone(&["head", "bip01 head"], Some(" head"));
    let feet = sym_pair(
        &["l foot", "bip01 l foot", "l toe0", "bip01 l toe0"],
        &["r foot", "bip01 r foot", "r toe0", "bip01 r toe0"],
    )
    .mid
    .or_else(|| sym_pair(&["l calf", "bip01 l calf"], &["r calf", "bip01 r calf"]).mid)
    .or_else(|| sym_pair(&["l thigh", "bip01 l thigh"], &["r thigh", "bip01 r thigh"]).mid)
    .or_else(|| (!use_skin).then(|| find_bone(&["pelvis"], None)).flatten());

    let basis: [[f32; 3]; 3] = if let (Some(h), Some(f)) = (head, feet)
        && let Some(u) = norm3([h[0] - f[0], h[1] - f[1], h[2] - f[2]])
    {
        let arms = sym_pair(
            &[
                "bip01 l clavicle",
                "l clavicle",
                "l upperarm",
                "bip01 l upperarm",
                "l hand",
            ],
            &[
                "bip01 r clavicle",
                "r clavicle",
                "r upperarm",
                "bip01 r upperarm",
                "r hand",
            ],
        );
        let (l_side, r_side) = if arms.left.is_some() && arms.right.is_some() {
            (arms.left, arms.right)
        } else {
            let legs = sym_pair(
                &["l thigh", "l calf", "l foot"],
                &["r thigh", "r calf", "r foot"],
            );
            (legs.left, legs.right)
        };
        let lr_vec = l_side.zip(r_side).and_then(|(l, r)| {
            let raw = [l[0] - r[0], l[1] - r[1], l[2] - r[2]];
            norm3([
                raw[0] - u[0] * dot3(raw, u),
                raw[1] - u[1] * dot3(raw, u),
                raw[2] - u[2] * dot3(raw, u),
            ])
        });
        if u[2] > 0.96 && lr_vec.is_none_or(|l| l[0] > 0.96) {
            return;
        }
        let l_ortho = lr_vec.unwrap_or_else(|| {
            let alt = if u[0].abs() < 0.8 {
                [1.0, 0.0, 0.0]
            } else {
                [0.0, 1.0, 0.0]
            };
            norm3(cross3(alt, u)).unwrap_or([1.0, 0.0, 0.0])
        });
        let y_axis = cross3(u, l_ortho);
        [l_ortho, y_axis, u]
    } else if !use_skin {
        let tall_axis = if span[0] >= span[1] && span[0] >= span[2] {
            0
        } else if span[1] >= span[0] && span[1] >= span[2] {
            1
        } else {
            return;
        };
        match tall_axis {
            0 => [[0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]],
            _ => [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
        }
    } else {
        return;
    };

    let rotate = |v: [f32; 3]| -> [f32; 3] {
        [
            dot3(v, basis[0]),
            dot3(v, basis[1]),
            dot3(v, basis[2]),
        ]
    };

    *low = [f32::INFINITY; 3];
    *high = [f32::NEG_INFINITY; 3];
    for face in &mut model.faces {
        for i in 0..3 {
            face.points[i] = rotate(face.points[i]);
            face.normals[i] = rotate(face.normals[i]);
            for axis in 0..3 {
                low[axis] = low[axis].min(face.points[i][axis]);
                high[axis] = high[axis].max(face.points[i][axis]);
            }
        }
    }
}
