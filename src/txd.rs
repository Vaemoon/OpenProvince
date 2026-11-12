pub struct Texture {
    pub name: String,
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

fn word(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn chunks(mut bytes: &[u8]) -> impl Iterator<Item = (u32, &[u8])> {
    std::iter::from_fn(move || {
        let kind = word(bytes, 0)?;
        let size = word(bytes, 4)? as usize;
        let (payload, rest) = bytes.get(12..)?.split_at_checked(size)?;
        bytes = rest;
        Some((kind, payload))
    })
}

pub fn previews(bytes: &[u8]) -> Vec<Texture> {
    matching(bytes, None)
}

pub fn name_matches(wanted: &str, candidate: &str) -> bool {
    let w = wanted.trim();
    let c = candidate.trim();
    if w.is_empty() || c.is_empty() {
        return false;
    }
    if w.eq_ignore_ascii_case(c) {
        return true;
    }
    if w.len() >= 31 && c.len() >= 31 {
        let n = w.len().min(c.len()).min(31);
        if w.is_char_boundary(n) && c.is_char_boundary(n) && w[..n].eq_ignore_ascii_case(&c[..n]) {
            return true;
        }
    }
    w.strip_suffix(['a', 'A'])
        .is_some_and(|base| !base.is_empty() && base.eq_ignore_ascii_case(c))
}

pub fn matching(bytes: &[u8], wanted: Option<&std::collections::BTreeSet<String>>) -> Vec<Texture> {
    let Some((0x16, dictionary)) = chunks(bytes).next() else {
        return Vec::new();
    };
    let mut textures = Vec::new();
    let mut pixels = 0usize;
    for (_, native) in chunks(dictionary).filter(|(kind, _)| *kind == 0x15) {
        let Some((1, data)) = chunks(native).next() else {
            continue;
        };
        if let Some(wanted) = wanted {
            let name = data.get(8..40).map(|name| {
                String::from_utf8_lossy(name.split(|b| *b == 0).next().unwrap_or_default())
                    .trim()
                    .to_ascii_lowercase()
            });
            if !name.is_some_and(|cand| wanted.iter().any(|w| name_matches(w, &cand))) {
                continue;
            }
        }
        if let Some(texture) = decode(data) {
            pixels += texture.width * texture.height;
            if pixels > 67_108_864 || textures.len() == 512 {
                break;
            }
            textures.push(texture);
        }
    }
    textures
}

fn decode_dds_compressed(
    fourcc: u32,
    width: usize,
    height: usize,
    bytes: &[u8],
) -> Option<Vec<u8>> {
    let mut dds = vec![0u8; 128];
    dds[..4].copy_from_slice(b"DDS ");
    for (offset, value) in [
        (4, 124),
        (8, 0x81007),
        (12, height as u32),
        (16, width as u32),
        (20, bytes.len() as u32),
        (76, 32),
        (80, 4),
        (84, fourcc),
        (108, 0x1000),
    ] {
        dds[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    dds.extend_from_slice(bytes);
    Some(
        image::load_from_memory_with_format(&dds, image::ImageFormat::Dds)
            .ok()?
            .to_rgba8()
            .into_raw(),
    )
}

pub(crate) fn decode(data: &[u8]) -> Option<Texture> {
    if word(data, 0)? != 9 || data.len() < 92 {
        return None;
    }
    let width = u16::from_le_bytes(data[80..82].try_into().ok()?) as usize;
    let height = u16::from_le_bytes(data[82..84].try_into().ok()?) as usize;
    if width == 0 || height == 0 || width > 4096 || height > 4096 {
        return None;
    }
    let name_bytes = &data[8..40];
    let end = name_bytes.iter().position(|b| *b == 0).unwrap_or(32);
    let name = String::from_utf8_lossy(&name_bytes[..end])
        .trim()
        .to_owned();
    let format = word(data, 76)?;
    let raster = word(data, 72)?;
    let dxt1 = u32::from_le_bytes(*b"DXT1");
    let dxt2 = u32::from_le_bytes(*b"DXT2");
    let dxt3 = u32::from_le_bytes(*b"DXT3");
    let dxt4 = u32::from_le_bytes(*b"DXT4");
    let dxt5 = u32::from_le_bytes(*b"DXT5");

    let rgba = if raster & 0x2000 != 0 {
        let palette = data.get(88..88 + 1024)?;
        let size = word(data, 88 + 1024)? as usize;
        let indices = data.get(88 + 1028..88usize.checked_add(1028)?.checked_add(size)?)?;
        let count = width * height;
        if indices.len() < count {
            return None;
        }
        let mut out = Vec::with_capacity(count * 4);
        for &idx in &indices[..count] {
            let p = idx as usize * 4;
            out.extend_from_slice(&palette[p..p + 4]);
        }
        out
    } else if raster & 0x4000 != 0 {
        let palette = data.get(88..88 + 64)?;
        let size = word(data, 88 + 64)? as usize;
        let packed = data.get(88 + 68..88usize.checked_add(68)?.checked_add(size)?)?;
        let count = width * height;
        if packed.len() * 2 < count {
            return None;
        }
        let mut out = Vec::with_capacity(count * 4);
        for i in 0..count {
            let byte = packed[i / 2];
            let idx = if i % 2 == 0 { byte & 0x0f } else { byte >> 4 } as usize;
            let p = idx * 4;
            out.extend_from_slice(&palette[p..p + 4]);
        }
        out
    } else {
        let size = word(data, 88)? as usize;
        let bytes = data.get(92..92usize.checked_add(size)?)?;
        if [dxt1, dxt2, dxt3, dxt4, dxt5].contains(&format) {
            let mapped = if format == dxt2 {
                dxt3
            } else if format == dxt4 {
                dxt5
            } else {
                format
            };
            let mut out = decode_dds_compressed(mapped, width, height, bytes)?;
            if format == dxt2 || format == dxt4 {
                for pixel in out.as_chunks_mut::<4>().0 {
                    let a = u16::from(pixel[3]);
                    if a > 0 && a < 255 {
                        for c in &mut pixel[..3] {
                            *c = ((u16::from(*c) * 255) / a).min(255) as u8;
                        }
                    }
                }
            }
            out
        } else if [21, 22].contains(&format) && bytes.len() >= width * height * 4 {
            let mut out = bytes[..width * height * 4].to_vec();
            for pixel in out.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
                if format == 22 {
                    pixel[3] = 255;
                }
            }
            out
        } else if matches!(format, 23 | 25 | 26)
            || matches!(raster & 0x0f00, 0x0100 | 0x0200 | 0x0300)
        {
            let count = width * height;
            if bytes.len() < count * 2 {
                return None;
            }
            let fmt = match format {
                23 | 25 | 26 => format,
                _ => match raster & 0x0f00 {
                    0x0100 => 25,
                    0x0200 => 23,
                    0x0300 => 26,
                    _ => return None,
                },
            };
            let mut out = Vec::with_capacity(count * 4);
            for &[b0, b1] in bytes[..count * 2].as_chunks::<2>().0 {
                let v = u16::from_le_bytes([b0, b1]);
                let rgba_px = match fmt {
                    23 => [
                        (((v >> 11) & 0x1f) * 255 / 31) as u8,
                        (((v >> 5) & 0x3f) * 255 / 63) as u8,
                        ((v & 0x1f) * 255 / 31) as u8,
                        255,
                    ],
                    25 => [
                        (((v >> 10) & 0x1f) * 255 / 31) as u8,
                        (((v >> 5) & 0x1f) * 255 / 31) as u8,
                        ((v & 0x1f) * 255 / 31) as u8,
                        if (v & 0x8000) != 0 { 255 } else { 0 },
                    ],
                    _ => [
                        (((v >> 8) & 0xf) * 17) as u8,
                        (((v >> 4) & 0xf) * 17) as u8,
                        ((v & 0xf) * 17) as u8,
                        (((v >> 12) & 0xf) * 17) as u8,
                    ],
                };
                out.extend_from_slice(&rgba_px);
            }
            out
        } else {
            return None;
        }
    };
    Some(Texture {
        name,
        width,
        height,
        rgba,
    })
}
