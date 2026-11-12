//! Программный рендерер 3D-превью для экспорта миниатюр каталога.
use crate::{dff, preview, txd};
use province_archive::Archive;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

const OUT_W: usize = 160;
const OUT_H: usize = 112;
const SS: usize = 2;
const W: usize = OUT_W * SS;
const H: usize = OUT_H * SS;

struct CatalogItem {
    id: u32,
    name: String,
    texture: Option<String>,
}

fn load_catalog_items(path: &Path) -> Result<Vec<CatalogItem>, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let mut items = Vec::new();
    for entry in value.as_array().into_iter().flatten() {
        let Some(id) = entry.get("id").and_then(|v| v.as_u64()) else {
            continue;
        };
        let Some(name) = entry.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        items.push(CatalogItem {
            id: id as u32,
            name: name.to_owned(),
            texture: entry
                .get("texture")
                .or_else(|| entry.get("txd"))
                .and_then(|v| v.as_str())
                .map(str::to_owned),
        });
    }
    Ok(items)
}

struct Camera {
    yaw: f32,
    pitch: f32,
    zoom: f32,
    pan_y: f32,
    is_ped: bool,
}

fn find_texture<'a>(wanted: &str, textures: &'a [txd::Texture]) -> Option<&'a txd::Texture> {
    if wanted.is_empty() {
        return None;
    }
    textures
        .iter()
        .find(|t| t.name.eq_ignore_ascii_case(wanted))
        .or_else(|| textures.iter().find(|t| txd::name_matches(wanted, &t.name)))
}

fn sample_bilinear(tex: &txd::Texture, u: f32, v: f32) -> [f32; 4] {
    let w = tex.width as i32;
    let h = tex.height as i32;
    let fx = u.rem_euclid(1.0) * w as f32 - 0.5;
    let fy = v.rem_euclid(1.0) * h as f32 - 0.5;
    let x0 = fx.floor() as i32;
    let y0 = fy.floor() as i32;
    let tx = fx - x0 as f32;
    let ty = fy - y0 as f32;
    let at = |x: i32, y: i32| -> [f32; 4] {
        let px = x.rem_euclid(w) as usize;
        let py = y.rem_euclid(h) as usize;
        let off = (py * tex.width + px) * 4;
        [
            f32::from(tex.rgba[off]) / 255.0,
            f32::from(tex.rgba[off + 1]) / 255.0,
            f32::from(tex.rgba[off + 2]) / 255.0,
            f32::from(tex.rgba[off + 3]) / 255.0,
        ]
    };
    let c00 = at(x0, y0);
    let c10 = at(x0 + 1, y0);
    let c01 = at(x0, y0 + 1);
    let c11 = at(x0 + 1, y0 + 1);
    std::array::from_fn(|i| {
        let a = c00[i] * (1.0 - tx) + c10[i] * tx;
        let b = c01[i] * (1.0 - tx) + c11[i] * tx;
        a * (1.0 - ty) + b * ty
    })
}

struct PedOrient {
    height_axis: usize,
    swap_xy: bool,
    flip_front: bool,
}

impl PedOrient {
    fn from_model(model: &dff::Model) -> Option<Self> {
        let mut pts = Vec::new();
        for face in &model.faces {
            for &p in &face.points {
                if p.iter().all(|v| v.is_finite()) {
                    pts.push(p);
                }
            }
        }
        if pts.len() < 12 {
            return None;
        }
        let mut iqrs = [0.0f32; 3];
        for (axis, slot) in iqrs.iter_mut().enumerate() {
            let mut vals: Vec<f32> = pts.iter().map(|p| p[axis]).collect();
            vals.sort_by(f32::total_cmp);
            let n = vals.len();
            *slot = vals[(n * 3) / 4] - vals[n / 4];
        }
        let height_axis = (0..3)
            .max_by(|&a, &b| iqrs[a].total_cmp(&iqrs[b]))
            .unwrap_or(2);
        let to_z_up = |p: [f32; 3]| -> [f32; 3] {
            match height_axis {
                0 => [-p[2], p[1], p[0]],
                1 => [p[0], -p[2], p[1]],
                _ => p,
            }
        };
        let mut up_pts: Vec<[f32; 3]> = pts.iter().copied().map(to_z_up).collect();
        let (mut min_x, mut max_x) = (f32::INFINITY, f32::NEG_INFINITY);
        let (mut min_y, mut max_y) = (f32::INFINITY, f32::NEG_INFINITY);
        let (mut min_z, mut max_z) = (f32::INFINITY, f32::NEG_INFINITY);
        for p in &up_pts {
            min_x = min_x.min(p[0]);
            max_x = max_x.max(p[0]);
            min_y = min_y.min(p[1]);
            max_y = max_y.max(p[1]);
            min_z = min_z.min(p[2]);
            max_z = max_z.max(p[2]);
        }
        let swap_xy = (max_x - min_x) < (max_y - min_y);
        if swap_xy {
            for p in &mut up_pts {
                *p = [-p[1], p[0], p[2]];
            }
        }
        let z_span = (max_z - min_z).max(1e-5);
        let (mut foot_sum, mut foot_cnt) = (0.0f32, 0usize);
        let (mut shin_sum, mut shin_cnt) = (0.0f32, 0usize);
        for p in &up_pts {
            let t = (p[2] - min_z) / z_span;
            if t <= 0.08 {
                foot_sum += p[1];
                foot_cnt += 1;
            } else if (0.14..=0.28).contains(&t) {
                shin_sum += p[1];
                shin_cnt += 1;
            }
        }
        let flip_front = if foot_cnt > 0 && shin_cnt > 0 {
            let foot_mean = foot_sum / foot_cnt as f32;
            let shin_mean = shin_sum / shin_cnt as f32;
            (foot_mean - shin_mean) < -0.005
        } else {
            false
        };
        Some(Self {
            height_axis,
            swap_xy,
            flip_front,
        })
    }

    fn apply(&self, mut v: [f32; 3]) -> [f32; 3] {
        v = match self.height_axis {
            0 => [-v[2], v[1], v[0]],
            1 => [v[0], -v[2], v[1]],
            _ => v,
        };
        if self.swap_xy {
            v = [-v[1], v[0], v[2]];
        }
        if self.flip_front {
            v = [-v[0], -v[1], v[2]];
        }
        v
    }
}

fn rasterize(model: &dff::Model, textures: &[txd::Texture], cam: &Camera) -> Vec<u8> {
    let mut color = vec![[0.0f32; 4]; W * H];
    let mut depth = vec![f32::INFINITY; W * H];
    let cy = cam.yaw.cos();
    let sy = cam.yaw.sin();
    let cp = cam.pitch.cos();
    let sp = cam.pitch.sin();
    let aspect = OUT_W as f32 / OUT_H as f32;
    let scale = 2.4 * cam.zoom;
    let lx = -0.35f32;
    let ly = 0.6f32;
    let lz = -0.75f32;
    let llen = (lx * lx + ly * ly + lz * lz).sqrt();
    let light = [lx / llen, ly / llen, lz / llen];

    let ped_orient = cam.is_ped.then(|| PedOrient::from_model(model)).flatten();

    let project = |pos: [f32; 3]| -> (f32, f32, f32, f32) {
        let mut norm = [
            (pos[0] - model.center[0]) / model.radius,
            (pos[1] - model.center[1]) / model.radius,
            (pos[2] - model.center[2]) / model.radius,
        ];
        if let Some(orient) = &ped_orient {
            norm = orient.apply(norm);
        }
        let [nx, ny, nz] = norm;
        let rx = cy * nx - sy * ny;
        let ry = -sy * sp * nx - cy * sp * ny + cp * nz;
        let rz = sy * cp * nx + cy * cp * ny + sp * nz;
        let d = (3.2 + rz).max(0.2);
        let ndc_x = (rx * scale / aspect) / d;
        let ndc_y = (ry * scale + cam.pan_y * d) / d;
        let sx = (ndc_x * 0.5 + 0.5) * W as f32;
        let sy_px = (0.5 - ndc_y * 0.5) * H as f32;
        (sx, sy_px, d, 1.0 / d)
    };

    let rot_vec = |mut v: [f32; 3]| -> [f32; 3] {
        if let Some(orient) = &ped_orient {
            v = orient.apply(v);
        }
        [
            cy * v[0] - sy * v[1],
            -sy * sp * v[0] - cy * sp * v[1] + cp * v[2],
            sy * cp * v[0] + cy * cp * v[1] + sp * v[2],
        ]
    };

    let resolved: Vec<(Option<&txd::Texture>, [f32; 4])> = model
        .materials
        .iter()
        .map(|m| {
            let tex = find_texture(&m.texture, textures);
            let rgb = [m.color[0], m.color[1], m.color[2]];
            let primary = matches!(rgb, [60, 255, 0] | [0, 255, 255]);
            let secondary = matches!(rgb, [255, 0, 175] | [255, 0, 255]);
            let c = if primary {
                if tex.is_some() {
                    [0.95, 0.96, 0.98]
                } else {
                    [0.58, 0.65, 0.74]
                }
            } else if secondary {
                if tex.is_some() {
                    [0.95, 0.96, 0.98]
                } else {
                    [0.24, 0.27, 0.32]
                }
            } else if tex.is_some()
                && rgb[0] == rgb[1]
                && rgb[1] == rgb[2]
                && (120..=210).contains(&rgb[0])
            {
                rgb.map(|v| (f32::from(v) / 255.0 * 1.45).min(1.0))
            } else {
                rgb.map(|v| f32::from(v) / 255.0)
            };
            (tex, [c[0], c[1], c[2], f32::from(m.color[3]) / 255.0])
        })
        .collect();

    let mut tri_pixels = 0u64;
    for pass in 0..2 {
        for face in &model.faces {
            if face
                .component
                .is_some_and(|idx| !model.components[idx].active_by_default)
            {
                continue;
            }
            let (tex, mat) = resolved[face.material];
            if pass == 0 && mat[3] < 0.98 && tex.is_none() {
                continue;
            }
            if pass == 1 && mat[3] >= 0.98 && tex.is_none() {
                continue;
            }
            let p0 = project(face.points[0]);
            let p1 = project(face.points[1]);
            let p2 = project(face.points[2]);
            let area = (p1.0 - p0.0) * (p2.1 - p0.1) - (p2.0 - p0.0) * (p1.1 - p0.1);
            if area.abs() < 1e-4 {
                continue;
            }
            let inv_area = 1.0 / area;
            let min_x =
                (p0.0.min(p1.0).min(p2.0).floor() as isize).clamp(0, W as isize - 1) as usize;
            let max_x =
                (p0.0.max(p1.0).max(p2.0).ceil() as isize).clamp(0, W as isize - 1) as usize;
            let min_y =
                (p0.1.min(p1.1).min(p2.1).floor() as isize).clamp(0, H as isize - 1) as usize;
            let max_y =
                (p0.1.max(p1.1).max(p2.1).ceil() as isize).clamp(0, H as isize - 1) as usize;
            if min_x > max_x || min_y > max_y {
                continue;
            }
            tri_pixels += ((max_x - min_x + 1) * (max_y - min_y + 1)) as u64;
            let rn = face.normals.map(rot_vec);
            for py in min_y..=max_y {
                let y = py as f32 + 0.5;
                for px in min_x..=max_x {
                    let x = px as f32 + 0.5;
                    let w0 = ((p1.0 - x) * (p2.1 - y) - (p2.0 - x) * (p1.1 - y)) * inv_area;
                    let w1 = ((p2.0 - x) * (p0.1 - y) - (p0.0 - x) * (p2.1 - y)) * inv_area;
                    let w2 = 1.0 - w0 - w1;
                    if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 {
                        continue;
                    }
                    let inv_w = w0 * p0.3 + w1 * p1.3 + w2 * p2.3;
                    if inv_w <= 1e-5 {
                        continue;
                    }
                    let z_lin = 1.0 / inv_w;
                    let idx = py * W + px;
                    if z_lin >= depth[idx] {
                        continue;
                    }
                    let u = (w0 * face.uv[0][0] * p0.3
                        + w1 * face.uv[1][0] * p1.3
                        + w2 * face.uv[2][0] * p2.3)
                        * z_lin;
                    let v = (w0 * face.uv[0][1] * p0.3
                        + w1 * face.uv[1][1] * p1.3
                        + w2 * face.uv[2][1] * p2.3)
                        * z_lin;
                    let t = tex.map_or([1.0; 4], |tx| sample_bilinear(tx, u, v));
                    let vc: [f32; 4] = std::array::from_fn(|k| {
                        (w0 * f32::from(face.colors[0][k]) * p0.3
                            + w1 * f32::from(face.colors[1][k]) * p1.3
                            + w2 * f32::from(face.colors[2][k]) * p2.3)
                            * z_lin
                            / 255.0
                    });
                    let a = t[3] * mat[3] * vc[3];
                    if a < 0.025 || (pass == 0 && a < 0.98) || (pass == 1 && a >= 0.98) {
                        continue;
                    }
                    let n = [
                        (w0 * rn[0][0] * p0.3 + w1 * rn[1][0] * p1.3 + w2 * rn[2][0] * p2.3)
                            * z_lin,
                        (w0 * rn[0][1] * p0.3 + w1 * rn[1][1] * p1.3 + w2 * rn[2][1] * p2.3)
                            * z_lin,
                        (w0 * rn[0][2] * p0.3 + w1 * rn[1][2] * p1.3 + w2 * rn[2][2] * p2.3)
                            * z_lin,
                    ];
                    let nlen = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-5);
                    let dot = (n[0] * light[0] + n[1] * light[1] + n[2] * light[2]) / nlen;
                    let diffuse = 0.72 + 0.28 * dot.abs();
                    let rgb = std::array::from_fn::<f32, 3, _>(|k| {
                        let prelit = 1.0 - 0.45 + 0.45 * vc[k];
                        (t[k] * mat[k] * prelit * diffuse).clamp(0.0, 1.0)
                    });
                    if pass == 0 {
                        depth[idx] = z_lin;
                        color[idx] = [rgb[0], rgb[1], rgb[2], 1.0];
                    } else {
                        let dst = color[idx];
                        let out_a = a + dst[3] * (1.0 - a);
                        if out_a > 1e-5 {
                            color[idx] = [
                                (rgb[0] * a + dst[0] * dst[3] * (1.0 - a)) / out_a,
                                (rgb[1] * a + dst[1] * dst[3] * (1.0 - a)) / out_a,
                                (rgb[2] * a + dst[2] * dst[3] * (1.0 - a)) / out_a,
                                out_a,
                            ];
                        }
                    }
                }
            }
        }
    }

    let _ = tri_pixels;
    let mut out = vec![0u8; OUT_W * OUT_H * 4];
    for y in 0..OUT_H {
        for x in 0..OUT_W {
            let mut sum = [0.0f32; 4];
            let mut premul = [0.0f32; 3];
            for dy in 0..SS {
                for dx in 0..SS {
                    let px = color[(y * SS + dy) * W + (x * SS + dx)];
                    sum[3] += px[3];
                    for k in 0..3 {
                        premul[k] += px[k] * px[3];
                    }
                }
            }
            let a = (sum[3] / (SS * SS) as f32).clamp(0.0, 1.0);
            let dst = (y * OUT_W + x) * 4;
            if a > 1e-4 {
                for k in 0..3 {
                    let c = (premul[k] / sum[3]).clamp(0.0, 1.0);
                    out[dst + k] = (c * 255.0).round() as u8;
                }
                out[dst + 3] = (a * 255.0).round() as u8;
            }
        }
    }
    out
}

struct ArchiveIndex {
    archives: Vec<Arc<Archive>>,
    lookup: BTreeMap<String, (usize, usize)>,
}

fn index_archives(models_dir: &Path) -> ArchiveIndex {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(models_dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("img")))
        .collect();
    // Архивы Province должны переопределять базовые архивы GTA.
    paths.sort_by_key(|p| {
        let s = p
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase();
        let rank = if s.starts_with("province_vehs") || s.starts_with("province_mods") {
            0
        } else if s.starts_with("province_") {
            1
        } else if s == "gta3.img" {
            2
        } else {
            3
        };
        (rank, s)
    });
    let mut archives = Vec::new();
    let mut lookup = BTreeMap::new();
    for path in paths {
        if let Ok(arch) = Archive::open(&path) {
            let idx = archives.len();
            for entry in arch.entries() {
                let lower = entry.name.to_ascii_lowercase();
                if let Some(stem) = lower.strip_suffix(".dff") {
                    lookup.entry(stem.to_owned()).or_insert((idx, entry.index));
                }
            }
            archives.push(Arc::new(arch));
        }
    }
    ArchiveIndex { archives, lookup }
}

pub fn export_catalog(
    models_dir: &Path,
    vehicles_json: &Path,
    skins_json: &Path,
    out_dir: &Path,
) -> Result<(), String> {
    let ArchiveIndex { archives, lookup } = index_archives(models_dir);
    if archives.is_empty() {
        return Err(format!("No IMG archives found in {}", models_dir.display()));
    }
    let veh_dir = out_dir.join("vehicles");
    let skin_dir = out_dir.join("skins");
    std::fs::create_dir_all(&veh_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&skin_dir).map_err(|e| e.to_string())?;

    let veh_items = load_catalog_items(vehicles_json)?;
    let skin_items = load_catalog_items(skins_json)?;

    let export_group = |items: &[CatalogItem], dir: &Path, cam: &Camera| -> (usize, usize) {
        let (mut ok, mut miss) = (0usize, 0usize);
        for item in items {
            let out_path = dir.join(format!("{}.png", item.id));
            let stem = item.name.trim().to_ascii_lowercase();
            let key = if stem == "null" || stem.is_empty() {
                item.texture
                    .as_deref()
                    .unwrap_or("")
                    .trim()
                    .to_ascii_lowercase()
            } else {
                stem
            };
            let Some(&(ai, ei)) = lookup.get(&key) else {
                miss += 1;
                continue;
            };
            let Ok((model, textures)) = preview::load_with_archives(&archives[ai], ei, &archives)
            else {
                miss += 1;
                continue;
            };
            let rgba = rasterize(&model, &textures, cam);
            if image::save_buffer_with_format(
                &out_path,
                &rgba,
                OUT_W as u32,
                OUT_H as u32,
                image::ColorType::Rgba8,
                image::ImageFormat::Png,
            )
            .is_ok()
            {
                ok += 1;
            }
        }
        (ok, miss)
    };

    let (v_ok, v_miss) = export_group(
        &veh_items,
        &veh_dir,
        &Camera {
            yaw: 2.45,
            pitch: -0.28,
            zoom: 1.68,
            pan_y: -0.02,
            is_ped: false,
        },
    );
    let (s_ok, s_miss) = export_group(
        &skin_items,
        &skin_dir,
        &Camera {
            yaw: 2.75,
            pitch: -0.10,
            zoom: 1.38,
            pan_y: 0.0,
            is_ped: true,
        },
    );
    println!("Exported thumbs: vehicles={v_ok} (miss {v_miss}), skins={s_ok} (miss {s_miss})");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_ped(map_point: impl Fn([f32; 3]) -> [f32; 3]) -> dff::Model {
        let mut pts = Vec::new();
        for &(x, y, z) in &[
            (-0.1, 0.18, -0.95),
            (0.1, 0.18, -0.95),
            (-0.1, -0.05, -0.95),
            (0.1, -0.05, -0.95),
            (-0.1, 0.0, -0.60),
            (0.1, 0.0, -0.60),
            (-0.15, 0.0, 0.0),
            (0.15, 0.0, 0.0),
            (-0.90, 0.0, 0.45),
            (0.90, 0.0, 0.45),
            (-0.08, 0.08, 0.80),
            (0.08, 0.08, 0.80),
        ] {
            pts.push(map_point([x, y, z]));
        }
        let faces = pts
            .as_chunks::<3>()
            .0
            .iter()
            .map(|&points| dff::Face {
                points,
                uv: [[0.0; 2]; 3],
                material: 0,
                normals: [[0.0, 1.0, 0.0]; 3],
                colors: [[255; 4]; 3],
                component: None,
                interactable: None,
            })
            .collect();
        dff::Model {
            faces,
            materials: Vec::new(),
            components: Vec::new(),
            interactables: Vec::new(),
            is_vehicle: false,
            center: [0.0; 3],
            radius: 1.0,
            vertices: 12,
            discarded_triangles: 0,
        }
    }

    #[test]
    fn ped_orient_uprights_horizontal_and_turns_backward_skins_to_front() {
        let horiz_y = synthetic_ped(|[x, y, z]| [x, z, y]);
        let orient_y = PedOrient::from_model(&horiz_y).expect("orient y");
        assert_eq!(orient_y.height_axis, 1);
        assert!(orient_y.flip_front);
        let foot = orient_y.apply([0.0, -0.95, 0.18]);
        assert!(foot[2] < -0.5, "feet must end at -Z bottom: {foot:?}");
        assert!(foot[1] > 0.1, "toes must face +Y camera: {foot:?}");

        let back_z = synthetic_ped(|[x, y, z]| [-x, -y, z]);
        let orient_b = PedOrient::from_model(&back_z).expect("orient back");
        assert_eq!(orient_b.height_axis, 2);
        assert!(orient_b.flip_front);
        let front_toe = orient_b.apply([0.0, -0.18, -0.95]);
        assert!(
            front_toe[1] > 0.1,
            "backward toes must flip to +Y: {front_toe:?}"
        );
    }
}
