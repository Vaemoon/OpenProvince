use crate::txd;
use std::sync::Arc;

#[derive(Clone)]
struct Chunk {
    kind: u32,
    version: u32,
    payload: Arc<Vec<u8>>,
}

#[derive(Clone)]
pub struct Document {
    version: u32,
    chunks: Vec<Chunk>,
}

pub struct Entry {
    pub name: String,
    pub width: usize,
    pub height: usize,
    pub format: String,
    pub mipmaps: u8,
    pub supported: bool,
}

fn word(bytes: &[u8], offset: usize) -> Result<u32, String> {
    bytes
        .get(offset..offset + 4)
        .ok_or_else(|| "Обрезанный заголовок TXD".to_owned())
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}
fn chunks(mut bytes: &[u8]) -> Result<Vec<Chunk>, String> {
    let mut result = Vec::new();
    while !bytes.is_empty() {
        let kind = word(bytes, 0)?;
        let size = word(bytes, 4)? as usize;
        let version = word(bytes, 8)?;
        let end = 12usize
            .checked_add(size)
            .ok_or("Переполнение размера TXD")?;
        let payload = bytes
            .get(12..end)
            .ok_or("Блок TXD выходит за границы файла")?
            .to_vec();
        result.push(Chunk {
            kind,
            version,
            payload: Arc::new(payload),
        });
        if result.len() > 4096 {
            return Err("Слишком много блоков TXD".into());
        }
        bytes = bytes.get(end..).ok_or("Границы TXD")?;
    }
    Ok(result)
}
fn encode_chunk(chunk: &Chunk, out: &mut Vec<u8>) {
    out.extend(chunk.kind.to_le_bytes());
    out.extend((chunk.payload.len() as u32).to_le_bytes());
    out.extend(chunk.version.to_le_bytes());
    out.extend_from_slice(&chunk.payload);
}
fn encode_chunks(chunks: &[Chunk]) -> Vec<u8> {
    let mut result = Vec::new();
    for chunk in chunks {
        encode_chunk(chunk, &mut result);
    }
    result
}
fn texture_data(chunk: &Chunk) -> Result<&[u8], String> {
    let bytes = chunk.payload.as_slice();
    if word(bytes, 0)? != 1 {
        return Err("Нет данных текстуры".into());
    }
    let end = 12usize
        .checked_add(word(bytes, 4)? as usize)
        .ok_or("Размер текстуры")?;
    bytes
        .get(12..end)
        .ok_or_else(|| "Обрезанные данные текстуры".into())
}
fn field_name(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes.split(|v| *v == 0).next().unwrap_or_default()).into_owned()
}
fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > 31
        || !name.is_ascii()
        || name.bytes().any(|b| b < 32 || b == 127)
    {
        return Err("Имя текстуры: от 1 до 31 символа ASCII без управляющих символов".into());
    }
    Ok(())
}

impl Document {
    pub fn dds(&self, index: usize) -> Result<Vec<u8>, String> {
        let data = texture_data(self.texture(index)?)?;
        if data.len() < 88 || word(data, 0)? != 9 {
            return Err("Неподдерживаемая платформа TXD".into());
        }
        let format = word(data, 76)?;
        let fourcc = format.to_le_bytes();
        let compressed = matches!(&fourcc, b"DXT1" | b"DXT2" | b"DXT3" | b"DXT4" | b"DXT5");
        let (width, height, levels, payload) = if compressed {
            let width = u16::from_le_bytes([data[80], data[81]]) as u32;
            let height = u16::from_le_bytes([data[82], data[83]]) as u32;
            let levels = data[85];
            if width == 0
                || height == 0
                || width > 4096
                || height > 4096
                || levels == 0
                || levels > 13
            {
                return Err("Некорректные уровни DDS".into());
            }
            let mut payload = Vec::new();
            let mut offset = 88;
            for level in 0..levels {
                let size = word(data, offset)? as usize;
                let expected = ((width >> level).max(1) as usize).div_ceil(4)
                    * ((height >> level).max(1) as usize).div_ceil(4)
                    * if format == u32::from_le_bytes(*b"DXT1") {
                        8
                    } else {
                        16
                    };
                if size != expected {
                    return Err("Размер mip-уровня не соответствует формату DDS".into());
                }
                offset += 4;
                payload.extend_from_slice(
                    data.get(offset..offset + size)
                        .ok_or("Обрезанный mip-уровень")?,
                );
                offset += size;
            }
            (width, height, levels, payload)
        } else {
            let texture = self
                .preview(index)
                .ok_or("Неподдерживаемый формат текстуры")?;
            (texture.width as u32, texture.height as u32, 1, texture.rgba)
        };
        let mut bytes = vec![0; 128];
        bytes[..4].copy_from_slice(b"DDS ");
        for (offset, value) in [
            (4, 124u32),
            (
                8,
                0x1007
                    | if compressed { 0x80000 } else { 8 }
                    | if levels > 1 { 0x20000 } else { 0 },
            ),
            (12, height),
            (16, width),
            (
                20,
                if compressed {
                    width.div_ceil(4)
                        * height.div_ceil(4)
                        * if format == u32::from_le_bytes(*b"DXT1") {
                            8
                        } else {
                            16
                        }
                } else {
                    width * 4
                },
            ),
            (28, u32::from(levels)),
            (76, 32),
            (80, if compressed { 4 } else { 0x41 }),
            (84, if compressed { format } else { 0 }),
            (88, if compressed { 0 } else { 32 }),
            (92, if compressed { 0 } else { 0xff }),
            (96, if compressed { 0 } else { 0xff00 }),
            (100, if compressed { 0 } else { 0xff0000 }),
            (104, if compressed { 0 } else { 0xff000000 }),
            (108, 0x1000 | if levels > 1 { 0x400008 } else { 0 }),
        ] {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes.extend(payload);
        Ok(bytes)
    }

    pub fn replace_dds(&mut self, index: usize, bytes: &[u8]) -> Result<(), String> {
        if bytes.get(..4) != Some(b"DDS ") || word(bytes, 4)? != 124 || word(bytes, 76)? != 32 {
            return Err("Неверный заголовок DDS".into());
        }
        let format = word(bytes, 84)?;
        let compressed = matches!(
            &format.to_le_bytes(),
            b"DXT1" | b"DXT2" | b"DXT3" | b"DXT4" | b"DXT5"
        );
        let rgba = word(bytes, 88)? == 32
            && word(bytes, 92)? == 0xff
            && word(bytes, 96)? == 0xff00
            && word(bytes, 100)? == 0xff0000
            && word(bytes, 104)? == 0xff000000;
        let bgra = word(bytes, 88)? == 32
            && word(bytes, 92)? == 0xff0000
            && word(bytes, 96)? == 0xff00
            && word(bytes, 100)? == 0xff
            && word(bytes, 104)? == 0xff000000;
        if !compressed && !(rgba || bgra) {
            return Err("DDS должен содержать DXT1–DXT5 или RGBA/BGRA 8888".into());
        }
        if word(bytes, 112)? != 0 || word(bytes, 24)? > 1 {
            return Err("Поддерживаются двумерные DDS без cubemap и volume".into());
        }
        let width = word(bytes, 16)?;
        let height = word(bytes, 12)?;
        let levels = word(bytes, 28)?.max(1);
        if width == 0
            || height == 0
            || width > 4096
            || height > 4096
            || levels > 13
            || levels > 32 - width.max(height).leading_zeros()
        {
            return Err("Некорректные размеры DDS".into());
        }
        let position = self.position(index)?;
        let mut children = chunks(&self.chunks[position].payload)?;
        let original = children
            .iter_mut()
            .find(|c| c.kind == 1)
            .ok_or("Нет данных текстуры")?;
        if word(&original.payload, 0)? != 9 || original.payload.len() < 88 {
            return Err("Неподдерживаемая платформа TXD".into());
        }
        let mut data = original.payload[..88].to_vec();
        let dxt1 = format == u32::from_le_bytes(*b"DXT1");
        let has_alpha = !dxt1
            || bytes.get(128..).is_some_and(|data| {
                data.as_chunks::<8>().0.iter().any(|block| {
                    u16::from_le_bytes([block[0], block[1]])
                        <= u16::from_le_bytes([block[2], block[3]])
                        && (0..16).any(|i| {
                            (u32::from_le_bytes([block[4], block[5], block[6], block[7]])
                                >> (i * 2))
                                & 3
                                == 3
                        })
                })
            });
        let raster = if !compressed {
            0x500
        } else if dxt1 {
            if has_alpha { 0x100u32 } else { 0x200 }
        } else {
            0x300
        };
        data[72..76].copy_from_slice(&(raster | if levels > 1 { 0x8000 } else { 0 }).to_le_bytes());
        data[76..80].copy_from_slice(&if compressed { format } else { 21u32 }.to_le_bytes());
        data[80..82].copy_from_slice(&(width as u16).to_le_bytes());
        data[82..84].copy_from_slice(&(height as u16).to_le_bytes());
        data[84..88].copy_from_slice(&[
            if !compressed {
                32
            } else if dxt1 {
                4
            } else {
                8
            },
            levels as u8,
            4,
            if compressed {
                8 | u8::from(has_alpha)
            } else {
                1
            },
        ]);
        let mut offset = 128;
        for level in 0..levels {
            let size = if compressed {
                ((width >> level).max(1) as usize).div_ceil(4)
                    * ((height >> level).max(1) as usize).div_ceil(4)
                    * if dxt1 { 8 } else { 16 }
            } else {
                (width >> level).max(1) as usize * (height >> level).max(1) as usize * 4
            };
            let block = bytes
                .get(offset..offset + size)
                .ok_or("Обрезанные уровни DDS")?;
            data.extend((size as u32).to_le_bytes());
            if !compressed && rgba {
                for p in block.as_chunks::<4>().0 {
                    data.extend([p[2], p[1], p[0], p[3]]);
                }
            } else {
                data.extend_from_slice(block);
            }
            offset += size;
        }
        original.payload = Arc::new(data);
        self.chunks[position].payload = Arc::new(encode_chunks(&children));
        Ok(())
    }
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if word(bytes, 0)? != 0x16 {
            return Err("Файл не является словарём TXD".into());
        }
        let size = word(bytes, 4)? as usize;
        let payload = bytes
            .get(12..12usize.checked_add(size).ok_or("Размер TXD")?)
            .ok_or("Обрезанный словарь TXD")?;
        let document = Self {
            version: word(bytes, 8)?,
            chunks: chunks(payload)?,
        };
        let header = document
            .chunks
            .first()
            .filter(|c| c.kind == 1 && c.payload.len() >= 4)
            .ok_or("Нет заголовка словаря TXD")?;
        let expected = u16::from_le_bytes([header.payload[0], header.payload[1]]) as usize;
        if expected != document.len() {
            return Err("Число текстур TXD не совпадает с содержимым".into());
        }
        Ok(document)
    }
    pub fn len(&self) -> usize {
        self.chunks.iter().filter(|c| c.kind == 0x15).count()
    }
    fn texture(&self, index: usize) -> Result<&Chunk, String> {
        self.chunks
            .iter()
            .filter(|c| c.kind == 0x15)
            .nth(index)
            .ok_or_else(|| "Текстура больше не существует".into())
    }
    fn position(&self, index: usize) -> Result<usize, String> {
        self.chunks
            .iter()
            .enumerate()
            .filter(|(_, c)| c.kind == 0x15)
            .nth(index)
            .map(|(i, _)| i)
            .ok_or_else(|| "Текстура больше не существует".into())
    }
    pub fn entries(&self) -> Vec<Entry> {
        (0..self.len())
            .map(|index| {
                let data = self
                    .texture(index)
                    .ok()
                    .and_then(|c| texture_data(c).ok())
                    .unwrap_or_default();
                let supported = word(data, 0).is_ok_and(|v| v == 9) && data.len() >= 88;
                let format = word(data, 76).unwrap_or_default();
                Entry {
                    name: data
                        .get(8..40)
                        .map(field_name)
                        .unwrap_or_else(|| format!("Текстура {}", index + 1)),
                    width: data
                        .get(80..82)
                        .map_or(0, |b| u16::from_le_bytes([b[0], b[1]]) as usize),
                    height: data
                        .get(82..84)
                        .map_or(0, |b| u16::from_le_bytes([b[0], b[1]]) as usize),
                    mipmaps: data.get(85).copied().unwrap_or(0),
                    format: match format {
                        21 => "RGBA 8888".into(),
                        22 => "RGB 8888".into(),
                        23 => "RGB 565".into(),
                        25 => "ARGB 1555".into(),
                        26 => "ARGB 4444".into(),
                        f if f.to_le_bytes()[..3] == *b"DXT" => {
                            String::from_utf8_lossy(&f.to_le_bytes()).into_owned()
                        }
                        _ => format!("D3D {format}"),
                    },
                    supported,
                }
            })
            .collect()
    }
    pub fn preview(&self, index: usize) -> Option<txd::Texture> {
        let texture = self.texture(index).ok()?;
        txd::decode(texture_data(texture).ok()?)
    }
    pub fn bytes(&self) -> Vec<u8> {
        let mut children = self.chunks.clone();
        if let Some(header) = children.first_mut() {
            Arc::make_mut(&mut header.payload)[..2]
                .copy_from_slice(&(self.len() as u16).to_le_bytes());
        }
        encode_chunks(&[Chunk {
            kind: 0x16,
            version: self.version,
            payload: Arc::new(encode_chunks(&children)),
        }])
    }
    pub fn rename(&mut self, index: usize, name: &str) -> Result<(), String> {
        validate_name(name)?;
        if self
            .entries()
            .iter()
            .enumerate()
            .any(|(i, e)| i != index && e.name.eq_ignore_ascii_case(name))
        {
            return Err("Текстура с таким именем уже существует".into());
        }
        let position = self.position(index)?;
        let mut children = chunks(&self.chunks[position].payload)?;
        let data = Arc::make_mut(
            &mut children
                .iter_mut()
                .find(|c| c.kind == 1)
                .ok_or("Нет данных текстуры")?
                .payload,
        );
        if word(data, 0)? != 9 || data.len() < 88 {
            return Err("Изменение этой платформы TXD не поддерживается".into());
        }
        data[8..40].fill(0);
        data[8..8 + name.len()].copy_from_slice(name.as_bytes());
        self.chunks[position].payload = Arc::new(encode_chunks(&children));
        Ok(())
    }
    pub fn remove(&mut self, index: usize) -> Result<(), String> {
        let position = self.position(index)?;
        self.chunks.remove(position);
        Ok(())
    }
    pub fn replace(
        &mut self,
        index: usize,
        image: &image::RgbaImage,
        mipmaps: bool,
    ) -> Result<(), String> {
        let position = self.position(index)?;
        let mut children = chunks(&self.chunks[position].payload)?;
        let data = Arc::make_mut(
            &mut children
                .iter_mut()
                .find(|c| c.kind == 1)
                .ok_or("Нет данных текстуры")?
                .payload,
        );
        if word(data, 0)? != 9 || data.len() < 88 {
            return Err("Изменение этой платформы TXD не поддерживается".into());
        }
        *data = encode_image(data, image, mipmaps)?;
        self.chunks[position].payload = Arc::new(encode_chunks(&children));
        Ok(())
    }
    pub fn add(
        &mut self,
        name: &str,
        image: &image::RgbaImage,
        mipmaps: bool,
    ) -> Result<usize, String> {
        validate_name(name)?;
        if self.len() >= 512 {
            return Err("Не более 512 текстур в редакторе".into());
        }
        if self
            .entries()
            .iter()
            .any(|e| e.name.eq_ignore_ascii_case(name))
        {
            return Err("Текстура с таким именем уже существует".into());
        }
        let mut header = vec![0; 88];
        header[..4].copy_from_slice(&9u32.to_le_bytes());
        header[4..8].copy_from_slice(&0x1102u32.to_le_bytes());
        header[8..8 + name.len()].copy_from_slice(name.as_bytes());
        let payload = encode_chunks(&[
            Chunk {
                kind: 1,
                version: self.version,
                payload: Arc::new(encode_image(&header, image, mipmaps)?),
            },
            Chunk {
                kind: 3,
                version: self.version,
                payload: Arc::new(Vec::new()),
            },
        ]);
        let index = self.len();
        let position = self
            .chunks
            .iter()
            .rposition(|c| c.kind == 0x15)
            .map_or(1, |p| p + 1);
        self.chunks.insert(
            position,
            Chunk {
                kind: 0x15,
                version: self.version,
                payload: Arc::new(payload),
            },
        );
        Ok(index)
    }
}

fn encode_image(header: &[u8], image: &image::RgbaImage, mipmaps: bool) -> Result<Vec<u8>, String> {
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 || width > 4096 || height > 4096 {
        return Err("Размер текстуры: от 1 до 4096 пикселей по каждой стороне".into());
    }
    let mut levels = vec![image.clone()];
    if mipmaps {
        while levels
            .last()
            .is_some_and(|i| i.width() > 1 || i.height() > 1)
        {
            let previous = levels.last().ok_or("Нет изображения")?;
            levels.push(image::imageops::resize(
                previous,
                (previous.width() / 2).max(1),
                (previous.height() / 2).max(1),
                image::imageops::FilterType::Triangle,
            ));
        }
    }
    let mut result = header[..88].to_vec();
    result[72..76].copy_from_slice(&(0x500u32 | if mipmaps { 0x8000 } else { 0 }).to_le_bytes());
    result[76..80].copy_from_slice(&21u32.to_le_bytes());
    result[80..82].copy_from_slice(&(width as u16).to_le_bytes());
    result[82..84].copy_from_slice(&(height as u16).to_le_bytes());
    result[84..88].copy_from_slice(&[
        32,
        levels.len() as u8,
        4,
        u8::from(image.pixels().any(|p| p[3] != 255)),
    ]);
    for level in levels {
        result.extend((level.as_raw().len() as u32).to_le_bytes());
        for pixel in level.as_raw().as_chunks::<4>().0 {
            result.extend([pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
    }
    Ok(result)
}
