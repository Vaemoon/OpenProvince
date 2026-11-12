use super::Material;

pub(super) struct Geometry {
    pub(super) vertices: Vec<[f32; 3]>,
    pub(super) uv: Vec<[f32; 2]>,
    pub(super) triangles: Vec<([usize; 3], usize)>,
    pub(super) materials: Vec<Material>,
    pub(super) normals: Vec<[f32; 3]>,
    pub(super) colors: Vec<[u8; 4]>,
}

pub(super) struct Frame {
    pub(super) matrix: [f32; 12],
    pub(super) parent: i32,
    pub(super) name: String,
    pub(super) hanim_id: Option<u32>,
}

pub(super) fn u32_at(b: &[u8], p: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        b.get(p..p + 4)
            .ok_or("Truncated DFF field")?
            .try_into()
            .map_err(|_| "DFF field")?,
    ))
}

pub(super) fn f32_at(b: &[u8], p: usize) -> Result<f32, String> {
    let value = f32::from_bits(u32_at(b, p)?);
    if !value.is_finite() {
        return Err("Non-finite DFF coordinate".into());
    }
    Ok(value)
}

pub(super) fn chunks(mut b: &[u8]) -> Result<Vec<(u32, &[u8])>, String> {
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

pub(super) fn child(mut b: &[u8], id: u32) -> Result<&[u8], String> {
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

pub(super) fn read_chunk<'a>(stream: &mut &'a [u8], expected: u32) -> Result<&'a [u8], String> {
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

pub(super) fn materials(b: &[u8]) -> Result<Vec<Material>, String> {
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
        // Часть моделей занижает внешнюю длину RpMaterial; читаем вложенные чанки последовательно.
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
        // Пропускаем дублирующие чанки текстур, если экспортер их оставил.
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

pub(super) fn geometry(b: &[u8]) -> Result<Geometry, String> {
    let data = child(b, 1)?;
    let flags = u32_at(data, 0)?;
    if flags & 0x0100_0000 != 0 {
        return Err("Нативная геометрия не поддерживается".into());
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

pub(super) fn transform_until(
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
        // Для дочерних узлов колеса отбрасывается смещение экспортера, но сохраняются поворот и масштаб.
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

pub(super) fn transform(point: [f32; 3], frame: i32, frames: &[Frame]) -> Result<[f32; 3], String> {
    transform_until(point, frame, None, frames)
}

pub(super) fn transform_normal_until(
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

pub(super) fn transform_normal(
    n: [f32; 3],
    frame: i32,
    frames: &[Frame],
) -> Result<[f32; 3], String> {
    transform_normal_until(n, frame, None, frames)
}

pub(super) fn wheel_ancestor(mut frame: i32, frames: &[Frame]) -> Result<Option<i32>, String> {
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

pub(super) fn left_wheel_part_ancestor(
    mut frame: i32,
    frames: &[Frame],
) -> Result<Option<i32>, String> {
    for _ in 0..=frames.len() {
        if frame < 0 {
            return Ok(None);
        }
        let node = frames
            .get(frame as usize)
            .ok_or("Invalid wheel hierarchy")?;
        if node.name.starts_with("wheel_l") && !node.name.ends_with("_dummy") {
            return Ok(Some(frame));
        }
        frame = node.parent;
    }
    Err("Cyclic wheel hierarchy".into())
}
