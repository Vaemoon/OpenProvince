use super::{
    Face, InteractablePart,
    chunks::{Frame, transform},
    components::base_frame_name,
};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
enum DoorKind {
    Lid,
    LeftDoor,
    RightDoor,
    SideAuto,
    GearLeft,
    GearRight,
}

fn classify_part_name(name: &str) -> Option<(String, String, DoorKind, u8)> {
    let base = base_frame_name(name.trim());
    if base.is_empty()
        || base.starts_with("ug_")
        || base.contains("doorsill")
        || base.contains("doorcard")
        || base.contains("door_card")
        || base.contains("doorstep")
        || base.contains("hoodseat")
    {
        return None;
    }

    if base == "bonnet"
        || base.starts_with("bonnet_")
        || base.ends_with("_bonnet_ok")
        || base == "hood"
        || base.starts_with("hood_")
        || base.contains("kapot")
    {
        return Some(("bonnet".into(), "Капот".into(), DoorKind::Lid, 0));
    }
    if base == "boot"
        || base.starts_with("boot_")
        || base == "trunk"
        || base.starts_with("trunk_")
        || base == "tailgate"
        || base.contains("bagaj")
    {
        return Some(("boot".into(), "Багажник".into(), DoorKind::Lid, 1));
    }
    if base.contains("door_lf") || base.contains("door_fl") {
        return Some(("door_lf".into(), "ПЛ дверь".into(), DoorKind::LeftDoor, 2));
    }
    if base.contains("door_rf") || base.contains("door_fr") {
        return Some(("door_rf".into(), "ПП дверь".into(), DoorKind::RightDoor, 3));
    }
    if base.contains("door_lr") || base.contains("door_rl") {
        return Some(("door_lr".into(), "ЗЛ дверь".into(), DoorKind::LeftDoor, 4));
    }
    if base.contains("door_rr") {
        return Some(("door_rr".into(), "ЗП дверь".into(), DoorKind::RightDoor, 5));
    }
    if base == "door_m" || base.starts_with("door_m_") || base.starts_with("door_c_") {
        return Some((
            base.to_owned(),
            "Боковая дверь".into(),
            DoorKind::SideAuto,
            6,
        ));
    }
    if base == "gear_l" {
        return Some((
            "gear_l".into(),
            "Шасси (лев.)".into(),
            DoorKind::GearLeft,
            7,
        ));
    }
    if base == "gear_r" {
        return Some((
            "gear_r".into(),
            "Шасси (прав.)".into(),
            DoorKind::GearRight,
            8,
        ));
    }
    None
}

pub(super) fn interactable_ancestor(mut frame: i32, frames: &[Frame]) -> Option<usize> {
    for _ in 0..=frames.len() {
        if frame < 0 {
            return None;
        }
        let idx = frame as usize;
        let node = frames.get(idx)?;
        if classify_part_name(&node.name).is_some() {
            return Some(idx);
        }
        frame = node.parent;
    }
    None
}

pub(super) fn finalize_interactables(
    faces: &mut [Face],
    face_frames: &[Option<usize>],
    frames: &[Frame],
) -> Vec<InteractablePart> {
    struct Accum {
        label: String,
        kind: DoorKind,
        order: u8,
        pivot_frame: Option<usize>,
        sum: [f64; 3],
        count: usize,
        face_indices: Vec<usize>,
    }

    let mut groups: BTreeMap<String, Accum> = BTreeMap::new();
    for (face_idx, (face, &frame_opt)) in faces.iter().zip(face_frames.iter()).enumerate() {
        let Some(frame_idx) = frame_opt else {
            continue;
        };
        let Some((key, label, kind, order)) = frames
            .get(frame_idx)
            .and_then(|f| classify_part_name(&f.name))
        else {
            continue;
        };
        let is_dummy = frames[frame_idx].name.ends_with("_dummy");
        let entry = groups.entry(key).or_insert_with(|| Accum {
            label,
            kind,
            order,
            pivot_frame: Some(frame_idx),
            sum: [0.0; 3],
            count: 0,
            face_indices: Vec::new(),
        });
        if is_dummy {
            entry.pivot_frame = Some(frame_idx);
        }
        entry.face_indices.push(face_idx);
        for p in &face.points {
            if p.iter().all(|v| v.is_finite()) {
                entry.sum[0] += f64::from(p[0]);
                entry.sum[1] += f64::from(p[1]);
                entry.sum[2] += f64::from(p[2]);
                entry.count += 1;
            }
        }
    }

    let mut ordered: Vec<_> = groups.into_iter().filter(|(_, g)| g.count > 0).collect();
    ordered.sort_by(|(ka, ga), (kb, gb)| ga.order.cmp(&gb.order).then_with(|| ka.cmp(kb)));

    let mut result = Vec::with_capacity(ordered.len());
    for (_, acc) in ordered {
        let Some(pf) = acc.pivot_frame else {
            continue;
        };
        let Ok(pivot) = transform([0.0; 3], pf as i32, frames) else {
            continue;
        };
        let mean = [
            (acc.sum[0] / acc.count as f64) as f32,
            (acc.sum[1] / acc.count as f64) as f32,
            (acc.sum[2] / acc.count as f64) as f32,
        ];
        let rel = [mean[0] - pivot[0], mean[1] - pivot[1], mean[2] - pivot[2]];

        let kind = match acc.kind {
            DoorKind::SideAuto => {
                if pivot[0] < 0.0 {
                    DoorKind::LeftDoor
                } else {
                    DoorKind::RightDoor
                }
            }
            other => other,
        };
        let (axis, max_angle_rad) = match kind {
            DoorKind::Lid => {
                let deg = if rel[1].abs() < 0.12 && rel[2] < -0.08 {
                    if pivot[1] > 0.0 { 75.0f32 } else { -75.0f32 }
                } else if rel[1] > 0.1 {
                    55.0f32
                } else {
                    -70.0f32
                };
                ([1.0, 0.0, 0.0], deg.to_radians())
            }
            DoorKind::LeftDoor => {
                if pivot[0] > 0.1 && rel[0] < -0.1 {
                    ([0.0, 1.0, 0.0], 70.0f32.to_radians())
                } else {
                    let deg = if rel[0].abs() > rel[1].abs() && rel[0] > 0.1 {
                        -105.0f32
                    } else if rel[1] > 0.1 {
                        65.0f32
                    } else {
                        -65.0f32
                    };
                    ([0.0, 0.0, 1.0], deg.to_radians())
                }
            }
            DoorKind::RightDoor => {
                if pivot[0] < -0.1 && rel[0] > 0.1 {
                    ([0.0, 1.0, 0.0], (-70.0f32).to_radians())
                } else {
                    let deg = if rel[0].abs() > rel[1].abs() && rel[0] < -0.1 {
                        105.0f32
                    } else if rel[1] > 0.1 {
                        -65.0f32
                    } else {
                        65.0f32
                    };
                    ([0.0, 0.0, 1.0], deg.to_radians())
                }
            }
            DoorKind::GearLeft => ([0.0, 1.0, 0.0], (-85.0f32).to_radians()),
            DoorKind::GearRight => ([0.0, 1.0, 0.0], 85.0f32.to_radians()),
            DoorKind::SideAuto => unreachable!(),
        };

        let part_idx = result.len();
        for fi in acc.face_indices {
            faces[fi].interactable = Some(part_idx);
        }
        result.push(InteractablePart {
            label: acc.label,
            pivot,
            axis,
            max_angle_rad,
        });
    }
    result
}
