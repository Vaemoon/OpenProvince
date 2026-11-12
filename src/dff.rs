//! Bounded RenderWare clump decoding and an orbitable, textured static-mesh preview.
//! Skin bind poses are shown as stored; animation and native geometry are not invented.
use eframe::egui;

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

#[derive(Clone)]
pub struct Face {
    pub points: [[f32; 3]; 3],
    pub uv: [[f32; 2]; 3],
    pub material: usize,
    pub normals: [[f32; 3]; 3],
    pub colors: [[u8; 4]; 3],
    pub component: Option<usize>,
}
pub struct Model {
    pub faces: Vec<Face>,
    pub materials: Vec<Material>,
    pub components: Vec<ComponentGroup>,
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
            center: [0.0; 3],
            radius: 1.0,
            vertices: 0,
            discarded_triangles: 0,
        };
        let mut low = [f32::INFINITY; 3];
        let mut high = [f32::NEG_INFINITY; 3];
        for part in parts {
            let mat_base = combined.materials.len();
            let comp_base = combined.components.len();
            combined.materials.extend(part.materials);
            combined.components.extend(part.components);
            combined.vertices += part.vertices;
            combined.discarded_triangles += part.discarded_triangles;
            for mut face in part.faces {
                face.material += mat_base;
                face.component = face.component.map(|c| c + comp_base);
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
struct Geometry {
    vertices: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    triangles: Vec<([usize; 3], usize)>,
    materials: Vec<Material>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[u8; 4]>,
}
struct Frame {
    matrix: [f32; 12],
    parent: i32,
    name: String,
}
fn u32_at(b: &[u8], p: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        b.get(p..p + 4)
            .ok_or("Truncated DFF field")?
            .try_into()
            .map_err(|_| "DFF field")?,
    ))
}
fn f32_at(b: &[u8], p: usize) -> Result<f32, String> {
    let value = f32::from_bits(u32_at(b, p)?);
    if !value.is_finite() {
        return Err("Non-finite DFF coordinate".into());
    }
    Ok(value)
}
fn chunks(mut b: &[u8]) -> Result<Vec<(u32, &[u8])>, String> {
    let mut result = Vec::new();
    while !b.is_empty() {
        if b.len() < 12 {
            return Err("Truncated RenderWare chunk header".into());
        }
        let kind = u32_at(b, 0)?;
        let size = u32_at(b, 4)? as usize;
        let end = 12usize.checked_add(size).ok_or("Chunk overflow")?;
        result.push((
            kind,
            b.get(12..end)
                .ok_or("RenderWare chunk exceeds its parent")?,
        ));
        b = b.get(end..).ok_or("Chunk extent")?;
        if result.len() > 100_000 {
            return Err("Too many RenderWare chunks".into());
        }
    }
    Ok(result)
}
fn child(mut b: &[u8], id: u32) -> Result<&[u8], String> {
    while b.len() >= 12 {
        let kind = u32_at(b, 0)?;
        let end = 12usize
            .checked_add(u32_at(b, 4)? as usize)
            .ok_or("Chunk overflow")?;
        let payload = b
            .get(12..end)
            .ok_or("RenderWare chunk exceeds its parent")?;
        if kind == id {
            return Ok(payload);
        }
        b = b.get(end..).ok_or("Chunk extent")?;
    }
    Err(format!("Missing RenderWare chunk {id:#x}"))
}
fn read_chunk<'a>(stream: &mut &'a [u8], expected: u32) -> Result<&'a [u8], String> {
    if u32_at(stream, 0)? != expected {
        return Err(format!("Expected RenderWare chunk {expected:#x}"));
    }
    let end = 12usize
        .checked_add(u32_at(stream, 4)? as usize)
        .ok_or("Chunk overflow")?;
    let data = stream
        .get(12..end)
        .ok_or("RenderWare chunk exceeds its parent")?;
    *stream = stream.get(end..).ok_or("Chunk extent")?;
    Ok(data)
}
fn materials(b: &[u8]) -> Result<Vec<Material>, String> {
    let mut stream = child(b, 8)?;
    let header = read_chunk(&mut stream, 1)?;
    let count = u32_at(header, 0)? as usize;
    if count > 4096 || header.len() < 4 + count * 4 {
        return Err("Invalid material count".into());
    }
    let mut result: Vec<Material> = Vec::new();
    for i in 0..count {
        let index = u32_at(header, 4 + i * 4)? as i32;
        if index >= 0 {
            result.push(
                result
                    .get(index as usize)
                    .ok_or("Invalid material reference")?
                    .clone(),
            );
            continue;
        }
        // RpMaterialStreamRead consumes the known fields sequentially. Several
        // supplied DFFs understate the outer material length and put the texture
        // outside that envelope; child chunks still have bounded, valid lengths.
        if u32_at(stream, 0)? != 7 {
            return Err("Missing material".into());
        }
        stream = stream.get(12..).ok_or("Truncated material header")?;
        let data = read_chunk(&mut stream, 1)?;
        let color = data
            .get(4..8)
            .ok_or("Missing material color")?
            .try_into()
            .map_err(|_| "Material color")?;
        let texture = if u32_at(data, 12)? != 0 {
            let texture = read_chunk(&mut stream, 6)?;
            child(texture, 2).map(|name| {
                String::from_utf8_lossy(name.split(|b| *b == 0).next().unwrap_or_default())
                    .to_ascii_lowercase()
            })?
        } else {
            String::new()
        };
        // Some supplied materials retain duplicate texture chunks. The native
        // plugin reader searches for the extension after consuming the first.
        for _ in 0..16 {
            if u32_at(stream, 0)? != 6 {
                break;
            }
            read_chunk(&mut stream, 6)?;
        }
        read_chunk(&mut stream, 3)?;
        result.push(Material { texture, color });
    }
    Ok(result)
}
fn geometry(b: &[u8]) -> Result<Geometry, String> {
    let data = child(b, 1)?;
    let flags = u32_at(data, 0)?;
    if flags & 0x0100_0000 != 0 {
        return Err("Native geometry is not supported by this viewer".into());
    }
    let tris = u32_at(data, 4)? as usize;
    let verts = u32_at(data, 8)? as usize;
    let morphs = u32_at(data, 12)? as usize;
    if verts > 250_000 || tris > 300_000 || morphs == 0 || morphs > 64 {
        return Err("Invalid geometry limits".into());
    }
    let sets = ((flags >> 16) & 0xff) as usize;
    let sets = if sets > 0 {
        sets
    } else if flags & 0x80 != 0 {
        2
    } else {
        usize::from(flags & 4 != 0)
    };
    if sets > 8 {
        return Err("Too many UV sets".into());
    }
    let mut colors = vec![[255; 4]; verts];
    if flags & 8 != 0 {
        for (i, color) in colors.iter_mut().enumerate() {
            *color = data
                .get(16 + i * 4..20 + i * 4)
                .ok_or("Truncated vertex color")?
                .try_into()
                .map_err(|_| "Vertex color")?;
        }
    }
    let mut offset = 16 + if flags & 8 != 0 { verts * 4 } else { 0 };
    let mut uv = vec![[0.0; 2]; verts];
    if sets > 0 {
        for (i, value) in uv.iter_mut().enumerate() {
            *value = [
                f32::from_bits(u32_at(data, offset + i * 8)?),
                f32::from_bits(u32_at(data, offset + i * 8 + 4)?),
            ];
            // Some exporters leave unused UV slots as NaN. Never upload them to the GPU.
            for coordinate in value {
                if !coordinate.is_finite() {
                    *coordinate = 0.0;
                }
            }
        }
    }
    offset += verts * sets * 8;
    let mut triangles = Vec::with_capacity(tris);
    for i in 0..tris {
        let row = data
            .get(offset + i * 8..offset + (i + 1) * 8)
            .ok_or("Truncated triangle")?;
        let word = |p| u16::from_le_bytes([row[p], row[p + 1]]) as usize;
        let indices = [word(2), word(0), word(6)];
        if indices.iter().any(|v| *v >= verts) {
            return Err("Triangle vertex outside geometry".into());
        }
        triangles.push((indices, word(4)));
    }
    offset += tris * 8;
    let has_vertices = u32_at(data, offset + 16)? != 0;
    let has_normals = u32_at(data, offset + 20)? != 0;
    if !has_vertices {
        return Err("Morph target has no vertices".into());
    }
    offset += 24;
    let mut vertices = Vec::with_capacity(verts);
    for i in 0..verts {
        vertices.push([
            f32::from_bits(u32_at(data, offset + i * 12)?),
            f32::from_bits(u32_at(data, offset + i * 12 + 4)?),
            f32::from_bits(u32_at(data, offset + i * 12 + 8)?),
        ]);
    }
    offset += verts * 12;
    let mut normals = vec![[0.0; 3]; verts];
    if has_normals {
        for (i, normal) in normals.iter_mut().enumerate() {
            *normal = [
                f32::from_bits(u32_at(data, offset + i * 12)?),
                f32::from_bits(u32_at(data, offset + i * 12 + 4)?),
                f32::from_bits(u32_at(data, offset + i * 12 + 8)?),
            ];
            if normal.iter().any(|n| !n.is_finite()) {
                *normal = [0.0; 3];
            }
        }
    }
    let materials = materials(b)?;
    let triangles = bin_mesh_triangles(b, verts, materials.len()).unwrap_or(triangles);
    Ok(Geometry {
        vertices,
        uv,
        triangles,
        materials,
        normals,
        colors,
    })
}
fn bin_mesh_triangles(
    geometry_chunk: &[u8],
    verts: usize,
    mat_count: usize,
) -> Option<Vec<([usize; 3], usize)>> {
    let ext = child(geometry_chunk, 3).ok()?;
    let bin = chunks(ext)
        .ok()?
        .into_iter()
        .find(|(id, _)| *id == 0x050e)
        .map(|(_, data)| data)?;
    if bin.len() < 12 || mat_count == 0 {
        return None;
    }
    let flags = u32_at(bin, 0).ok()?;
    let splits = u32_at(bin, 4).ok()? as usize;
    let total_indices = u32_at(bin, 8).ok()? as usize;
    if splits == 0 || splits > 4096 || total_indices > 1_200_000 {
        return None;
    }
    let is_strip = flags & 1 != 0;
    let mut pos = 12usize;
    let mut result = Vec::with_capacity(if is_strip {
        total_indices
    } else {
        total_indices / 3
    });
    for _ in 0..splits {
        let count = u32_at(bin, pos).ok()? as usize;
        let mat_id = u32_at(bin, pos + 4).ok()? as usize;
        pos = pos.checked_add(8)?;
        if mat_id >= mat_count || count > 1_200_000 {
            return None;
        }
        let end = pos.checked_add(count.checked_mul(4)?)?;
        if end > bin.len() {
            return None;
        }
        let idx = |k: usize| -> Result<usize, ()> {
            let v = u32_at(bin, pos + k * 4).map_err(|_| ())? as usize;
            (v < verts).then_some(v).ok_or(())
        };
        if is_strip {
            for k in 0..count.saturating_sub(2) {
                let (Ok(a), Ok(b), Ok(c)) = (idx(k), idx(k + 1), idx(k + 2)) else {
                    return None;
                };
                if a == b || b == c || a == c {
                    continue;
                }
                let tri = if k % 2 == 0 { [a, b, c] } else { [a, c, b] };
                result.push((tri, mat_id));
            }
        } else {
            for k in (0..count).step_by(3) {
                if k + 2 >= count {
                    break;
                }
                let (Ok(a), Ok(b), Ok(c)) = (idx(k), idx(k + 1), idx(k + 2)) else {
                    return None;
                };
                if a != b && b != c && a != c {
                    result.push(([a, b, c], mat_id));
                }
            }
        }
        pos = end;
    }
    (!result.is_empty()).then_some(result)
}
fn transform_until(
    mut point: [f32; 3],
    mut frame: i32,
    stop: Option<i32>,
    frames: &[Frame],
) -> Result<[f32; 3], String> {
    for _ in 0..=frames.len() {
        if frame < 0 || Some(frame) == stop {
            return Ok(point);
        }
        let f = frames.get(frame as usize).ok_or("Invalid atomic frame")?;
        let m = &f.matrix;
        // Collapsed wheel child frames discard exporter offsets but keep local 3x3 scale/rotation.
        let (tx, ty, tz) = if stop.is_some() {
            (0.0, 0.0, 0.0)
        } else {
            (m[9], m[10], m[11])
        };
        point = [
            m[0] * point[0] + m[3] * point[1] + m[6] * point[2] + tx,
            m[1] * point[0] + m[4] * point[1] + m[7] * point[2] + ty,
            m[2] * point[0] + m[5] * point[1] + m[8] * point[2] + tz,
        ];
        frame = f.parent;
    }
    Err("Cyclic DFF frame hierarchy".into())
}
fn transform(point: [f32; 3], frame: i32, frames: &[Frame]) -> Result<[f32; 3], String> {
    transform_until(point, frame, None, frames)
}
fn transform_normal_until(
    mut n: [f32; 3],
    mut frame: i32,
    stop: Option<i32>,
    frames: &[Frame],
) -> Result<[f32; 3], String> {
    for _ in 0..=frames.len() {
        if frame < 0 || Some(frame) == stop {
            return Ok(n);
        }
        let f = frames.get(frame as usize).ok_or("Invalid normal frame")?;
        let m = &f.matrix;
        // Inverse transpose, including nonuniform scaling and mirrored frame transforms.
        let c0 = [
            m[4] * m[8] - m[5] * m[7],
            m[5] * m[6] - m[3] * m[8],
            m[3] * m[7] - m[4] * m[6],
        ];
        let c1 = [
            m[7] * m[2] - m[8] * m[1],
            m[8] * m[0] - m[6] * m[2],
            m[6] * m[1] - m[7] * m[0],
        ];
        let c2 = [
            m[1] * m[5] - m[2] * m[4],
            m[2] * m[3] - m[0] * m[5],
            m[0] * m[4] - m[1] * m[3],
        ];
        let det = m[0] * c0[0] + m[1] * c0[1] + m[2] * c0[2];
        if det.abs() < 1e-12 {
            return Err("Singular DFF normal transform".into());
        }
        n = std::array::from_fn(|i| (c0[i] * n[0] + c1[i] * n[1] + c2[i] * n[2]) / det);
        frame = f.parent;
    }
    Err("Cyclic normal hierarchy".into())
}
fn transform_normal(n: [f32; 3], frame: i32, frames: &[Frame]) -> Result<[f32; 3], String> {
    transform_normal_until(n, frame, None, frames)
}
pub fn decode(bytes: &[u8]) -> Result<Model, String> {
    decode_with_wheels(bytes, [1.0; 2])
}

/// Vehicle wheels use unit-size geometry and dimensions supplied by vehicles.ide.
pub fn decode_with_wheels(mut bytes: &[u8], wheel_sizes: [f32; 2]) -> Result<Model, String> {
    // Exporters may place a UV-animation dictionary before the model clump.
    // IMG sector padding is not a chunk and must never be scanned as one.
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
    // Native RpClumpStreamRead (0x74B48B) reads exactly 12 bytes for this
    // structure, independent of its declared length. Eight supplied models set
    // an extra high bit in that length. Normalize only this observed pattern.
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
        });
    }
    for (frame, (_, extension)) in frames
        .iter_mut()
        .zip(chunks(list)?.into_iter().filter(|(id, _)| *id == 3))
    {
        if let Ok(name) = child(extension, 0x0253_f2fe) {
            frame.name =
                String::from_utf8_lossy(name.split(|b| *b == 0).next().unwrap_or_default())
                    .to_ascii_lowercase();
        }
    }
    let geometries = chunks(child(clump, 0x1a)?)?
        .into_iter()
        .filter(|(id, _)| *id == 0xf)
        .map(|(_, b)| b)
        .collect::<Vec<_>>();
    let vehicle = frames
        .iter()
        .any(|f| f.name.starts_with("wheel_") && f.name.ends_with("_dummy"));
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
        center: [0.0; 3],
        radius: 1.0,
        vertices: 0,
        discarded_triangles: 0,
    };
    let mut component_map = std::collections::BTreeMap::<String, usize>::new();
    let mut low = [f32::INFINITY; 3];
    let mut high = [f32::NEG_INFINITY; 3];
    let mut fallback_low = [f32::INFINITY; 3];
    let mut fallback_high = [f32::NEG_INFINITY; 3];
    let mut wheel_components: std::collections::BTreeMap<i32, Vec<(usize, Face)>> =
        Default::default();
    let mut occupied_wheels = std::collections::BTreeSet::new();
    for (_, atomic) in chunks(clump)?.into_iter().filter(|(id, _)| *id == 0x14) {
        let data = child(atomic, 1)?;
        let frame = u32_at(data, 0)? as i32;
        // RenderWare atomics with render flag cleared are collision/damage helpers.
        if u32_at(data, 8)? & 4 == 0 {
            continue;
        }
        // Always skip damaged meshes and low-detail LOD dummies.
        let mut ancestor = frame;
        let mut hidden = false;
        for _ in 0..frames.len() {
            if ancestor < 0 {
                break;
            }
            let f = frames
                .get(ancestor as usize)
                .ok_or("Invalid frame hierarchy")?;
            if is_permanent_damage_or_lod(&f.name, vehicle) {
                hidden = true;
                break;
            }
            ancestor = f.parent;
        }
        if hidden {
            continue;
        }
        let component = if vehicle {
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
        let scale = wheel.map_or(1.0, |index| {
            if frames[index as usize].name.contains("f_dummy") {
                wheel_sizes[0]
            } else {
                wheel_sizes[1]
            }
        });
        let geometry = geometry(
            geometries
                .get(u32_at(data, 4)? as usize)
                .ok_or("Invalid atomic geometry")?,
        )?;
        let base = model.materials.len();
        let face_start = model.faces.len();
        model.materials.extend(geometry.materials.iter().cloned());
        let transform_point = |p: [f32; 3]| -> Result<[f32; 3], String> {
            let scaled = p.map(|v| v * scale);
            if let Some(dummy) = wheel {
                let local = transform_until(scaled, frame, Some(dummy), &frames)?;
                transform(local, dummy, &frames)
            } else {
                transform(scaled, frame, &frames)
            }
        };
        let transform_vec = |n: [f32; 3]| -> Result<[f32; 3], String> {
            if let Some(dummy) = wheel {
                let local = transform_normal_until(n, frame, Some(dummy), &frames)?;
                transform_normal(local, dummy, &frames)
            } else {
                transform_normal(n, frame, &frames)
            }
        };
        let vertices = geometry
            .vertices
            .iter()
            .copied()
            .map(transform_point)
            .collect::<Result<Vec<_>, _>>()?;
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
            // Native GPU rendering discards triangles whose positions contain NaN.
            // Some source meshes intentionally retain such vertices. Preserve the
            // valid geometry and report omissions; never fabricate positions.
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
            let normals = indices.map(|i| {
                if geometry.normals[i].iter().all(|n| n.abs() < 0.00001) {
                    return Ok(fallback);
                }
                transform_vec(geometry.normals[i])
            });
            let [n0, n1, n2] = normals;
            model.faces.push(Face {
                points,
                uv: indices.map(|i| geometry.uv[i]),
                material: base + material,
                normals: [n0?, n1?, n2?],
                colors: indices.map(|i| geometry.colors[i]),
                component,
            });
        }
        if let Some(dummy) = wheel {
            occupied_wheels.insert(dummy);
            let entry = wheel_components.entry(dummy).or_default();
            for (idx, face) in model.faces[face_start..].iter().cloned().enumerate() {
                entry.push((face_start + idx, face));
            }
        }
    }
    // MTA/Province bus and truck models frequently leave occupied wheel_l*_dummy frames
    // unrotated (Rxx > 0), placing left wheels inward inside the body. Turn them 180° around Z.
    for (&dummy, wheel_faces) in &mut wheel_components {
        let name = frames[dummy as usize].name.as_str();
        if !name.starts_with("wheel_l") {
            continue;
        }
        let origin = transform([0.0; 3], dummy, &frames)?;
        if origin[0] >= -0.1 {
            continue;
        }
        let (mut min_rel_x, mut max_rel_x) = (f32::INFINITY, f32::NEG_INFINITY);
        for (_, face) in wheel_faces.iter() {
            for p in &face.points {
                let rel_x = p[0] - origin[0];
                min_rel_x = min_rel_x.min(rel_x);
                max_rel_x = max_rel_x.max(rel_x);
            }
        }
        if min_rel_x.is_finite() && (min_rel_x + max_rel_x) > 0.02 {
            for (face_idx, cached_face) in wheel_faces.iter_mut() {
                let target = &mut model.faces[*face_idx];
                for i in 0..3 {
                    let dx = target.points[i][0] - origin[0];
                    let dy = target.points[i][1] - origin[1];
                    target.points[i][0] = origin[0] - dx;
                    target.points[i][1] = origin[1] - dy;
                    target.normals[i][0] = -target.normals[i][0];
                    target.normals[i][1] = -target.normals[i][1];
                    cached_face.points[i] = target.points[i];
                    cached_face.normals[i] = target.normals[i];
                    for axis in 0..3 {
                        fallback_low[axis] = fallback_low[axis].min(target.points[i][axis]);
                        fallback_high[axis] = fallback_high[axis].max(target.points[i][axis]);
                        low[axis] = low[axis].min(target.points[i][axis]);
                        high[axis] = high[axis].max(target.points[i][axis]);
                    }
                }
            }
        }
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
        let inverse = [
            cross(basis[1], basis[2]),
            cross(basis[2], basis[0]),
            cross(basis[0], basis[1]),
        ];
        let det = dot(basis[0], inverse[0]);
        if det.abs() < 1e-12 {
            return Err("Singular wheel dummy transform".into());
        }
        for (index, target) in frames.iter().enumerate() {
            if !target.name.starts_with("wheel_")
                || !target.name.ends_with("_dummy")
                || occupied_wheels.contains(&(index as i32))
            {
                continue;
            }
            if matches!(target.name.as_str(), "wheel_lm_dummy" | "wheel_rm_dummy")
                && target.matrix[9].abs() <= 0.1
            {
                continue;
            }
            let left = target.name.starts_with("wheel_l");
            let source_size = if frames[source as usize].name.contains("f_dummy") {
                wheel_sizes[0]
            } else {
                wheel_sizes[1]
            };
            let target_size = if target.name.contains("f_dummy") {
                wheel_sizes[0]
            } else {
                wheel_sizes[1]
            };
            let scale = target_size / source_size;
            for face in &faces {
                let mut copy = face.clone();
                for i in 0..3 {
                    let relative = std::array::from_fn(|axis| face.points[i][axis] - origin[axis]);
                    let mut local = inverse.map(|row| dot(relative, row) / det * scale);
                    let mut normal = basis.map(|column| dot(column, face.normals[i]));
                    // Native automobiles turn left wheel frames through 180 degrees about Z.
                    if left {
                        local[0] = -local[0];
                        local[1] = -local[1];
                        normal[0] = -normal[0];
                        normal[1] = -normal[1];
                    }
                    copy.points[i] = transform(local, index as i32, &frames)?;
                    copy.normals[i] = transform_normal(normal, index as i32, &frames)?;
                    for axis in 0..3 {
                        fallback_low[axis] = fallback_low[axis].min(copy.points[i][axis]);
                        fallback_high[axis] = fallback_high[axis].max(copy.points[i][axis]);
                        if copy
                            .component
                            .is_none_or(|idx| model.components[idx].active_by_default)
                        {
                            low[axis] = low[axis].min(copy.points[i][axis]);
                            high[axis] = high[axis].max(copy.points[i][axis]);
                        }
                    }
                }
                model.faces.push(copy);
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

fn wheel_ancestor(mut frame: i32, frames: &[Frame]) -> Result<Option<i32>, String> {
    for _ in 0..=frames.len() {
        if frame < 0 {
            return Ok(None);
        }
        let node = frames
            .get(frame as usize)
            .ok_or("Invalid wheel hierarchy")?;
        if node.name.starts_with("wheel_") && node.name.ends_with("_dummy") {
            return Ok(Some(frame));
        }
        frame = node.parent;
    }
    Err("Cyclic wheel hierarchy".into())
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
        extra_controls: impl FnOnce(&mut egui::Ui),
    ) -> Option<egui::Response> {
        self.renderer
            .show(ui, textures, paintjob, vinyl, extra_controls)
    }
}

fn base_frame_name(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((prefix, suffix))
            if !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()) =>
        {
            prefix
        }
        _ => name,
    }
}

fn optional_extra_number(name: &str) -> Option<u8> {
    let base = base_frame_name(name);
    let suffix = base
        .strip_prefix("extra_")
        .or_else(|| base.strip_prefix("extra"))?;
    let n = suffix.parse::<u8>().ok()?;
    (1..=6).contains(&n).then_some(n)
}

fn is_permanent_damage_or_lod(name: &str, vehicle: bool) -> bool {
    let lower = name.to_ascii_lowercase();
    let base = base_frame_name(&lower);
    if lower.contains("_dam")
        || (base.len() > 3 && base.ends_with("dam"))
        || (base.len() > 2 && base.ends_with("_d") && base != "misc_d")
    {
        return true;
    }
    vehicle && (base.ends_with("_vlo") || base.ends_with("_lod"))
}

fn is_restyle_name(base: &str) -> bool {
    base.contains("_rest")
        || ["_2021", "_2022", "_2023", "_2024", "_2025"]
            .iter()
            .any(|tag| base.contains(tag))
}

fn normalize_cm_label(base: &str) -> (String, String) {
    let trimmed = base
        .strip_prefix("cm_")
        .or_else(|| base.strip_prefix("ug_"))
        .unwrap_or(base)
        .trim_end_matches("_ok")
        .trim_end_matches("_e")
        .trim_end_matches("_a")
        .trim_end_matches("_g");
    let merged = trimmed
        .replace("_lf_", "_")
        .replace("_rf_", "_")
        .replace("_lr_", "_")
        .replace("_rr_", "_");
    let pretty = merged.replace('_', " ");
    (format!("cm:{merged}"), format!("Тюнинг: {pretty}"))
}

fn classify_name(name: &str, has_restyle: bool) -> Option<ComponentGroup> {
    let base = base_frame_name(name);
    if let Some(n) = optional_extra_number(base) {
        return Some(ComponentGroup {
            key: format!("extra{n}"),
            label: format!("Экстра {n}"),
            kind: ComponentKind::Extra,
            active_by_default: false,
        });
    }
    if is_restyle_name(base) {
        let label = if base.contains("_2021") {
            "Рестайлинг 2021"
        } else {
            "Рестайлинг"
        };
        return Some(ComponentGroup {
            key: format!("restyle:{label}"),
            label: label.to_owned(),
            kind: ComponentKind::Restyle,
            active_by_default: false,
        });
    }
    if base.contains("_dorest")
        || (has_restyle
            && ["_2018", "_2019", "_2020"]
                .iter()
                .any(|tag| base.contains(tag)))
    {
        return Some(ComponentGroup {
            key: "dorest".to_owned(),
            label: "Дорестайл / сток".to_owned(),
            kind: ComponentKind::StockVariant,
            active_by_default: true,
        });
    }
    if base.contains("_stock") || base.starts_with("stock_") || base == "bodykit_stock" {
        return Some(ComponentGroup {
            key: "stock".to_owned(),
            label: "Сток-детали".to_owned(),
            kind: ComponentKind::StockVariant,
            active_by_default: true,
        });
    }
    if base.starts_with("cm_")
        && (base.contains("_0_")
            || base.ends_with("_0")
            || base.contains("_pnt_")
            || base == "cm_diffusor_plastic_ok")
    {
        return Some(ComponentGroup {
            key: "stock_cm".to_owned(),
            label: "Базовый обвес".to_owned(),
            kind: ComponentKind::StockVariant,
            active_by_default: true,
        });
    }
    if base.contains("_sport") || base.starts_with("sport_") {
        return Some(ComponentGroup {
            key: "sport".to_owned(),
            label: "Пакет Sport".to_owned(),
            kind: ComponentKind::Tuning,
            active_by_default: false,
        });
    }
    if base.contains("_carb") {
        return Some(ComponentGroup {
            key: "carbon".to_owned(),
            label: "Карбон".to_owned(),
            kind: ComponentKind::Tuning,
            active_by_default: false,
        });
    }
    if base.contains("acrap") || base.contains("akrap") {
        return Some(ComponentGroup {
            key: "akrapovic".to_owned(),
            label: "Выхлоп Akrapovic".to_owned(),
            kind: ComponentKind::Tuning,
            active_by_default: false,
        });
    }
    if base.starts_with("tuning")
        || base.starts_with("tun_")
        || base.contains("_tuning")
        || base.contains("_tun")
        || base.starts_with("bodykit_")
        || base == "rs6_abt_bodykit"
        || base.contains("_kit")
        || base.contains("pandem")
        || base.contains("heritage")
        || base.starts_with("prov_neon_")
        || base == "taxish"
        || base == "windscr_sticker"
        || base.starts_with("window_sticker")
        || base == "momo_rule"
        || base.starts_with("bride_")
        || (!base.ends_with("_0")
            && [
                "fbmp_", "rbmp_", "fbump_", "wg_", "bnt_", "spl_", "exh_", "lgt_", "nto_", "rf_b_",
            ]
            .iter()
            .any(|p| base.starts_with(p)))
    {
        let label = format!("Тюнинг: {}", base.trim_end_matches("_ok").replace('_', " "));
        return Some(ComponentGroup {
            key: format!("tun:{base}"),
            label,
            kind: ComponentKind::Tuning,
            active_by_default: false,
        });
    }
    if base.starts_with("cm_") || base.starts_with("ug_") {
        let (key, label) = normalize_cm_label(base);
        return Some(ComponentGroup {
            key,
            label,
            kind: ComponentKind::Tuning,
            active_by_default: false,
        });
    }
    None
}

fn classify_component(
    mut frame: i32,
    frames: &[Frame],
    has_restyle: bool,
) -> Option<ComponentGroup> {
    for _ in 0..=frames.len() {
        if frame < 0 {
            return None;
        }
        let node = frames.get(frame as usize)?;
        if let Some(group) = classify_name(&node.name, has_restyle) {
            return Some(group);
        }
        frame = node.parent;
    }
    None
}

#[cfg(test)]
fn hidden_component(name: &str) -> bool {
    is_permanent_damage_or_lod(name, true) || optional_extra_number(name).is_some()
}
#[cfg(test)]
mod tests {
    use super::*;
    fn chunk(kind: u32, data: &[u8]) -> Vec<u8> {
        [
            kind.to_le_bytes().as_slice(),
            &(data.len() as u32).to_le_bytes(),
            &0x1803_ffffu32.to_le_bytes(),
            data,
        ]
        .concat()
    }

    // A whole clump fixture, with one wheel atomic and four wheel dummies. The
    // wheel child deliberately contains an exporter transform GTA discards.
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
            0.0f32, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.5, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0,
            0.0,
        ] {
            geometry.extend(value.to_le_bytes());
        }
        let mut material_data = vec![0; 28];
        material_data[4..8].fill(255);
        let material = chunk(7, &[chunk(1, &material_data), chunk(3, &[])].concat());
        let material_list = [chunk(1, &[1, 0, 0, 0, 255, 255, 255, 255]), material].concat();
        let geometry = chunk(
            0xf,
            &[chunk(1, &geometry), chunk(8, &material_list)].concat(),
        );
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
        assert!(
            model.radius < 4.0,
            "discarded exporter transform must not affect framing"
        );
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
        assert_eq!(
            decode(&bytes)
                .expect("native clump size compatibility")
                .faces
                .len(),
            4
        );
        bytes[16..20].copy_from_slice(&0x0100_000du32.to_le_bytes());
        assert!(
            decode(&bytes).is_err(),
            "other invalid lengths remain rejected"
        );
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
                    name: String::new()
                }]
            )
            .is_err()
        );
        assert!(chunks(&[0; 11]).is_err());
    }

    #[test]
    fn preserves_wheel_child_scale_and_skips_aircraft_cloning() {
        let clump = || {
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
                    0.0f32, 0.0, 0.0, 0.0, r, 0.0, 0.0, 0.0, r, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0,
                    0.0, 0.0,
                ] {
                    g.extend(v.to_le_bytes());
                }
                let mut md = vec![0; 28];
                md[4..8].fill(255);
                let mat = chunk(7, &[chunk(1, &md), chunk(3, &[])].concat());
                let ml = [chunk(1, &[1, 0, 0, 0, 255, 255, 255, 255]), mat].concat();
                chunk(0xf, &[chunk(1, &g), chunk(8, &ml)].concat())
            };
            let g0 = make_geo(0.3);
            let g1 = make_geo(10.0);
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
            chunk(
                0x10,
                &[
                    chunk(1, &[2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
                    chunk(0xe, &frame_list),
                    chunk(0x1a, &[g0, g1].concat()),
                    a0,
                    a1,
                ]
                .concat(),
            )
        };
        let model = decode(&clump()).expect("aircraft clump");
        assert_eq!(
            model.faces.len(),
            2,
            "must not clone onto empty aircraft dummies"
        );
        let rear_y = model.faces[1].points[1][1];
        assert!(
            (rear_y - (-1.0)).abs() < 0.001,
            "expected 0.1 child scale preserved, got y={rear_y}"
        );
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
        let mut geometry = [0u32, 1, 3, 1]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        geometry.extend([1u16, 0, 0, 2].into_iter().flat_map(u16::to_le_bytes));
        geometry.extend([0u32, 0, 0, 0, 1, 1].into_iter().flat_map(u32::to_le_bytes));
        for v in [
            0.15f32, 0.0, 0.0, 0.45, 0.3, 0.0, 0.45, 0.0, 0.3, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0,
            0.0, 0.0,
        ] {
            geometry.extend(v.to_le_bytes());
        }
        let mut md = vec![0; 28];
        md[4..8].fill(255);
        let mat = chunk(7, &[chunk(1, &md), chunk(3, &[])].concat());
        let ml = [chunk(1, &[1, 0, 0, 0, 255, 255, 255, 255]), mat].concat();
        let geo = chunk(0xf, &[chunk(1, &geometry), chunk(8, &ml)].concat());
        let a0 = chunk(
            0x14,
            &chunk(
                1,
                &[2u32, 0, 5, 0]
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            ),
        );
        let a1 = chunk(
            0x14,
            &chunk(
                1,
                &[4u32, 0, 5, 0]
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
                chunk(0x1a, &geo),
                a0,
                a1,
            ]
            .concat(),
        );
        let model = decode(&bytes).expect("bus clump");
        let lb_face = &model.faces[1];
        assert!(
            lb_face.points.iter().all(|p| p[0] <= -0.9),
            "expected left rear wheel outward, got {:?}",
            lb_face.points
        );
    }

    #[test]
    fn separates_damage_and_groups_stock_vs_tuning_components() {
        assert!(is_permanent_damage_or_lod("bonnet_carb_d", true));
        assert!(is_permanent_damage_or_lod("bump_rear_rest_d", true));
        assert!(is_permanent_damage_or_lod("bonnet_kit1dam", true));
        assert!(is_permanent_damage_or_lod("fbmp_350z_tun1dam", true));
        assert!(!is_permanent_damage_or_lod("misc_d", true));
        assert!(!is_permanent_damage_or_lod("windshield", true));
        let rest = classify_name("head_lights_2021", true).expect("restyle");
        assert!(!rest.active_by_default);
        let old = classify_name("head_lights_2018", true).expect("dorest");
        assert!(old.active_by_default);
        let stock = classify_name("bonnet_stock_ok", false).expect("stock");
        assert!(stock.active_by_default);
        let stock_kit = classify_name("bodykit_stock", false).expect("stock bodykit");
        assert!(stock_kit.active_by_default);
        let carb = classify_name("bonnet_carb_ok", false).expect("carb");
        assert!(!carb.active_by_default);
        let fbmp = classify_name("fbmp_350z_tun1ok", false).expect("front bumper kit");
        assert!(!fbmp.active_by_default);
        let spl = classify_name("cm_m5_splitter1_ok", false).expect("splitter");
        assert!(!spl.active_by_default);
    }

    #[test]
    fn bin_mesh_splits_override_zeroed_legacy_triangle_materials() {
        let mut geom_struct = Vec::new();
        // flags=0 (unlit/minimal), 2 tris, 4 verts, 1 morph
        geom_struct.extend(0u32.to_le_bytes());
        geom_struct.extend(2u32.to_le_bytes());
        geom_struct.extend(4u32.to_le_bytes());
        geom_struct.extend(1u32.to_le_bytes());
        // Legacy triangles both claim mat_id = 0
        for (v0, v1, v2) in [(0u16, 1u16, 2u16), (0u16, 2u16, 3u16)] {
            geom_struct.extend(v1.to_le_bytes());
            geom_struct.extend(v0.to_le_bytes());
            geom_struct.extend(0u16.to_le_bytes());
            geom_struct.extend(v2.to_le_bytes());
        }
        // Morph target header (bounding sphere + has_verts=1 + has_norms=0)
        geom_struct.extend([0u8; 16]);
        geom_struct.extend(1u32.to_le_bytes());
        geom_struct.extend(0u32.to_le_bytes());
        for v in [
            [0.0f32, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ] {
            for c in v {
                geom_struct.extend(c.to_le_bytes());
            }
        }
        // Two materials
        let mat_data = [0u32, 0xffff_ffffu32, 0, 0, 0, 0]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        let make_mat = |r: u8| {
            let mut d = mat_data.clone();
            d[4] = r;
            chunk(7, &[chunk(1, &d), chunk(3, &[])].concat())
        };
        let mut mat_header = 2u32.to_le_bytes().to_vec();
        mat_header.extend((-1i32).to_le_bytes());
        mat_header.extend((-1i32).to_le_bytes());
        let mat_list = chunk(
            8,
            &[chunk(1, &mat_header), make_mat(10), make_mat(20)].concat(),
        );

        // BinMeshPLG: flags=0 (tri list), 2 splits, 6 total indices -> tri 0 on mat 0, tri 1 on mat 1
        let mut bin = Vec::new();
        for w in [0u32, 2, 6, 3, 0, 0, 1, 2, 3, 1, 0, 2, 3] {
            bin.extend(w.to_le_bytes());
        }
        let ext = chunk(3, &chunk(0x050e, &bin));
        let geo = chunk(0x0f, &[chunk(1, &geom_struct), mat_list, ext].concat());

        let mut fl = 1u32.to_le_bytes().to_vec();
        for f in [
            1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0,
        ] {
            fl.extend(f.to_le_bytes());
        }
        fl.extend((-1i32).to_le_bytes());
        fl.extend(0u32.to_le_bytes());
        let frame_list = chunk(0xe, &[chunk(1, &fl), chunk(3, &[])].concat());
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
    }
}
