use aes::Aes128;
use aes::cipher::{Array, BlockCipherDecrypt, BlockCipherEncrypt, KeyInit};
use serde::Serialize;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

const SECTOR: u64 = 2048;
const NAME_MASK: [u8; 4] = [0xdd, 0x7a, 0x20, 0x70];

#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("Allocation: {0}")]
    Allocation(#[from] std::collections::TryReserveError),
    #[error("Invalid IMG: {0}")]
    InvalidArchive(String),
    #[error("Unsafe or unsupported archive filename: {0}")]
    InvalidName(String),
    #[error("Entry index {0} is out of range")]
    EntryIndex(usize),
    #[error("Invalid length-prefixed text at byte {0}")]
    InvalidText(usize),
}

#[derive(Clone)]
pub struct Profile {
    world_key: [u8; 16],
    boot_key: [u8; 16],
}

impl Profile {
    pub fn inspected_client() -> Self {
        Self {
            world_key: [
                0xd6, 0x99, 0xc2, 0x17, 0xce, 0x83, 0xff, 0xc0, 0x51, 0x0e, 0x82, 0xf0, 0x55, 0xcd,
                0x6d, 0x8c,
            ],
            boot_key: [
                0xee, 0x74, 0x87, 0x2b, 0xaf, 0xb4, 0x41, 0xc0, 0x90, 0x56, 0x7b, 0x2f, 0xcc, 0xb8,
                0x78, 0x2b,
            ],
        }
    }

    pub fn decode_file(&self, path: &Path, bytes: &[u8]) -> Vec<u8> {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let boot = ["surface.dat", "surfaud.dat", "surfinfo.dat", "veh_data.dat"]
            .iter()
            .any(|s| name.eq_ignore_ascii_case(s));
        decode_payload(
            bytes,
            if boot {
                &self.boot_key
            } else {
                &self.world_key
            },
        )
    }
}

pub fn decode_name(raw: &[u8; 24]) -> Result<(String, bool), Error> {
    let encrypted = !raw[0].is_ascii();
    let mut decoded = *raw;
    if encrypted {
        for (i, byte) in decoded.iter_mut().enumerate() {
            *byte ^= NAME_MASK.get(i).copied().unwrap_or(0xf0);
        }
    }
    let end = decoded
        .iter()
        .position(|b| *b == 0)
        .unwrap_or(decoded.len());
    let bytes = &decoded[..end];
    if !bytes.is_ascii() {
        return Err(Error::InvalidName(
            "non-ASCII name or incompatible profile".into(),
        ));
    }
    let name = String::from_utf8_lossy(bytes).into_owned();
    validate_name(&name)?;
    Ok((name, encrypted))
}

fn validate_name(name: &str) -> Result<(), Error> {
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'));
    if name.is_empty()
        || name == "."
        || name == ".."
        || reserved
        || name.ends_with(['.', ' '])
        || name.bytes().any(|b| b < 32 || b"/\\:*?\"<>|".contains(&b))
    {
        return Err(Error::InvalidName(name.to_owned()));
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize)]
pub struct Entry {
    pub index: usize,
    pub name: String,
    pub offset: u64,
    pub size: u64,
    pub name_obfuscated: bool,
}

#[derive(Debug)]
pub struct Archive {
    path: PathBuf,
    file_len: u64,
    entries: Vec<Entry>,
}

#[derive(Clone, Debug)]
pub enum Edit {
    Replace { name: String, source: PathBuf },
    Remove { name: String },
    Add { source: PathBuf },
}

struct WriteEntry {
    name: String,
    source: WriteSource,
}

enum WriteSource {
    Original {
        path: PathBuf,
        offset: u64,
        size: u64,
    },
    Decoded(PathBuf),
}

fn source_name(path: &Path) -> Result<String, Error> {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| Error::InvalidName(path.display().to_string()))?;
    validate_name(name)?;
    if !name.is_ascii() || name.len() > 23 {
        return Err(Error::InvalidName(name.into()));
    }
    Ok(name.to_owned())
}

pub fn pack_directory(input: &Path, output: &Path, profile: &Profile) -> Result<(), Error> {
    let mut entries = Vec::new();
    for item in fs::read_dir(input)? {
        let item = item?;
        if !item.file_type()?.is_file() {
            return Err(Error::InvalidArchive(
                "packing expects only regular files in a flat directory".into(),
            ));
        }
        let path = item.path();
        if path.file_name().is_some_and(|s| s == "_directory.json") {
            continue;
        }
        entries.push(WriteEntry {
            name: source_name(&path)?,
            source: WriteSource::Decoded(path),
        });
    }
    entries.sort_by_key(|e| e.name.to_ascii_lowercase());
    write_archive(&entries, output, profile)
}

fn write_archive(entries: &[WriteEntry], output: &Path, profile: &Profile) -> Result<(), Error> {
    let mut names = std::collections::HashSet::new();
    for entry in entries {
        validate_name(&entry.name)?;
        if entry.name.len() > 23
            || !entry.name.is_ascii()
            || !names.insert(entry.name.to_ascii_lowercase())
        {
            return Err(Error::InvalidArchive(format!(
                "duplicate or unsupported filename: {}",
                entry.name
            )));
        }
    }
    let count = u32::try_from(entries.len())
        .map_err(|_| Error::InvalidArchive("too many entries".into()))?;
    let mut target = OpenOptions::new()
        .write(true)
        .read(true)
        .create_new(true)
        .open(output)?;
    let result = (|| {
        target.write_all(b"VER2")?;
        target.write_all(&count.to_le_bytes())?;
        let first_data = (8 + u64::from(count) * 32).div_ceil(SECTOR) * SECTOR;
        target.set_len(first_data)?;
        target.seek(SeekFrom::Start(first_data))?;
        let mut directory = Vec::new();
        directory.try_reserve_exact(entries.len() * 32)?;
        for entry in entries {
            let offset = target.stream_position()?;
            let size = match &entry.source {
                WriteSource::Original {
                    path,
                    offset: source_offset,
                    size,
                } => {
                    let mut source = File::open(path)?;
                    source.seek(SeekFrom::Start(*source_offset))?;
                    if std::io::copy(&mut source.take(*size), &mut target)? != *size {
                        return Err(Error::InvalidArchive(
                            "source changed or was truncated while saving".into(),
                        ));
                    }
                    *size
                }
                WriteSource::Decoded(path) => {
                    let metadata = fs::metadata(path)?;
                    if metadata.len() > 65535 * SECTOR - 16 {
                        return Err(Error::InvalidArchive(format!(
                            "{} exceeds IMG sector-count limit",
                            path.display()
                        )));
                    }
                    let bytes = fs::read(path)?;
                    let encoded = encode_payload(&bytes, &profile.world_key)?;
                    target.write_all(&encoded)?;
                    encoded.len() as u64
                }
            };
            let sector = u32::try_from(offset / SECTOR)
                .map_err(|_| Error::InvalidArchive("archive exceeds IMG offset limit".into()))?;
            let sectors = u16::try_from(size / SECTOR).map_err(|_| {
                Error::InvalidArchive("entry exceeds IMG sector-count limit".into())
            })?;
            directory.extend_from_slice(&sector.to_le_bytes());
            directory.extend_from_slice(&sectors.to_le_bytes());
            directory.extend_from_slice(&[0, 0]);
            let mut name = [0u8; 24];
            name[..entry.name.len()].copy_from_slice(entry.name.as_bytes());
            for (i, byte) in name.iter_mut().enumerate() {
                *byte ^= NAME_MASK.get(i).copied().unwrap_or(0xf0);
            }
            directory.extend_from_slice(&name);
        }
        target.seek(SeekFrom::Start(8))?;
        target.write_all(&directory)?;
        target.sync_all()?;
        Ok(())
    })();
    drop(target);
    if result.is_err() {
        let _ = fs::remove_file(output);
    }
    result
}

pub fn encode_payload(bytes: &[u8], key: &[u8; 16]) -> Result<Vec<u8>, Error> {
    let length = bytes
        .len()
        .checked_add(16)
        .ok_or_else(|| Error::InvalidArchive("payload length overflow".into()))?
        .div_ceil(SECTOR as usize)
        * SECTOR as usize;
    if length > 65535 * SECTOR as usize {
        return Err(Error::InvalidArchive(
            "payload exceeds IMG sector-count limit".into(),
        ));
    }
    let mut encoded = Vec::new();
    encoded.try_reserve_exact(length)?;
    encoded.resize(length, 0);
    encoded[..4].copy_from_slice(b"dWV+");
    encoded[4..4 + bytes.len()].copy_from_slice(bytes);
    let cipher = Aes128::new(&Array::from(*key));
    for batch in encoded[4..length - 12].chunks_mut(16 * 32) {
        let count = batch.len() / 16;
        let mut blocks = [Array::from([0u8; 16]); 32];
        for (block, raw) in blocks.iter_mut().zip(batch.as_chunks::<16>().0) {
            block.copy_from_slice(raw);
        }
        cipher.encrypt_blocks(&mut blocks[..count]);
        for (raw, block) in batch
            .as_chunks_mut::<16>()
            .0
            .iter_mut()
            .zip(&blocks[..count])
        {
            raw.copy_from_slice(block);
        }
    }
    Ok(encoded)
}

impl Archive {
    pub fn save_as(&self, output: &Path, edits: &[Edit], profile: &Profile) -> Result<(), Error> {
        let mut entries: Vec<_> = self
            .entries
            .iter()
            .map(|e| WriteEntry {
                name: e.name.clone(),
                source: WriteSource::Original {
                    path: self.path.clone(),
                    offset: e.offset,
                    size: e.size,
                },
            })
            .collect();
        for edit in edits {
            match edit {
                Edit::Remove { name } => {
                    let before = entries.len();
                    entries.retain(|e| !e.name.eq_ignore_ascii_case(name));
                    if entries.len() == before {
                        return Err(Error::InvalidArchive(format!("entry not found: {name}")));
                    }
                }
                Edit::Replace { name, source } => {
                    let entry = entries
                        .iter_mut()
                        .find(|e| e.name.eq_ignore_ascii_case(name))
                        .ok_or_else(|| Error::InvalidArchive(format!("entry not found: {name}")))?;
                    entry.source = WriteSource::Decoded(source.clone());
                }
                Edit::Add { source } => entries.push(WriteEntry {
                    name: source_name(source)?,
                    source: WriteSource::Decoded(source.clone()),
                }),
            }
        }
        write_archive(&entries, output, profile)
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        let mut file = File::open(path)?;
        let file_len = file.metadata()?.len();
        let mut header = [0; 8];
        file.read_exact(&mut header)?;
        if &header[..4] != b"VER2" {
            return Err(Error::InvalidArchive("expected VER2".into()));
        }
        let count = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
        let directory_len = 8 + count as u64 * 32;
        if directory_len > file_len {
            return Err(Error::InvalidArchive(
                "directory exceeds file length".into(),
            ));
        }
        let first_data = directory_len.div_ceil(SECTOR) * SECTOR;
        let mut entries = Vec::new();
        entries.try_reserve_exact(count)?;
        for index in 0..count {
            let mut row = [0; 32];
            file.read_exact(&mut row)?;
            let sector = u32::from_le_bytes([row[0], row[1], row[2], row[3]]);
            let streamed = u16::from_le_bytes([row[4], row[5]]);
            let stored = u16::from_le_bytes([row[6], row[7]]);
            let size = u64::from(if stored == 0 { streamed } else { stored }) * SECTOR;
            let offset = u64::from(sector) * SECTOR;
            if size > 0
                && (offset < first_data
                    || offset.checked_add(size).is_none_or(|end| end > file_len))
            {
                return Err(Error::InvalidArchive(format!(
                    "entry {index} is outside payload bounds"
                )));
            }
            let mut raw = [0; 24];
            raw.copy_from_slice(&row[8..]);
            let (name, name_obfuscated) = decode_name(&raw)?;
            entries.push(Entry {
                index,
                name,
                offset,
                size,
                name_obfuscated,
            });
        }
        Ok(Self {
            path: path.to_owned(),
            file_len,
            entries,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn file_len(&self) -> u64 {
        self.file_len
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn read(&self, index: usize, profile: &Profile) -> Result<Vec<u8>, Error> {
        let entry = self.entries.get(index).ok_or(Error::EntryIndex(index))?;
        let size = usize::try_from(entry.size)
            .map_err(|_| Error::InvalidArchive("entry too large for this platform".into()))?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(size)?;
        bytes.resize(size, 0);
        let mut file = File::open(&self.path)?;
        file.seek(SeekFrom::Start(entry.offset))?;
        file.read_exact(&mut bytes)?;
        if bytes.starts_with(b"dWV+") {
            Ok(decode_payload(&bytes, &profile.world_key))
        } else {
            Ok(bytes)
        }
    }

    pub fn extract(
        &self,
        index: usize,
        profile: &Profile,
        output: &Path,
    ) -> Result<Exported, Error> {
        let entry = self.entries.get(index).ok_or(Error::EntryIndex(index))?;
        let bytes = self.read(index, profile)?;
        let inspection = inspect(&entry.name, &bytes);
        let size = inspection.logical_size.unwrap_or(bytes.len());
        fs::create_dir_all(output)?;
        let path = output.join(&entry.name);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        if let Err(error) = file.write_all(&bytes[..size]) {
            drop(file);
            let _ = fs::remove_file(&path);
            return Err(error.into());
        }
        Ok(Exported {
            path,
            bytes: size,
            warning: inspection.warning,
        })
    }
}

#[derive(Debug, Serialize)]
pub struct Exported {
    pub path: PathBuf,
    pub bytes: usize,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Inspection {
    pub format: String,
    pub logical_size: Option<usize>,
    pub warning: Option<String>,
}

pub fn inspect(name: &str, bytes: &[u8]) -> Inspection {
    let extension = Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mut result = Inspection {
        format: extension.to_ascii_uppercase(),
        logical_size: None,
        warning: None,
    };
    let word = |offset| {
        bytes
            .get(offset..offset + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    match extension.as_str() {
        "dff" | "txd" => {
            let mut offset = 0usize;
            if extension == "dff" && word(0) == Some(0x2b) {
                offset = word(4).map_or(bytes.len(), |n| n as usize + 12);
            }
            let expected = if extension == "dff" { 0x10 } else { 0x16 };
            if word(offset) == Some(expected)
                && let Some(end) =
                    word(offset + 4).and_then(|n| offset.checked_add(n as usize + 12))
                && end <= bytes.len()
            {
                result.logical_size = Some(end);
            }
            if result.logical_size.is_none() {
                result.warning = Some(
                    "invalid or truncated RenderWare container; preserving decoded bytes".into(),
                );
            }
        }
        "col" => {
            let mut offset = 0usize;
            while bytes.get(offset..offset + 4).is_some_and(|s| {
                [b"COLL", b"COL2", b"COL3", b"COL4"]
                    .iter()
                    .any(|magic| s == *magic)
            }) {
                let Some(size) = word(offset + 4).map(|n| n as usize + 8) else {
                    break;
                };
                if size < 32 || offset.checked_add(size).is_none_or(|n| n > bytes.len()) {
                    result.warning =
                        Some("truncated collision chunk; preserving decoded bytes".into());
                    return result;
                }
                offset += size;
            }
            if offset > 0 {
                result.logical_size = Some(offset);
            } else {
                result.warning = Some("unrecognized collision header".into());
            }
        }
        "ifp" => {
            if (bytes.starts_with(b"ANPK") || bytes.starts_with(b"ANP3"))
                && let Some(end) = word(4)
                    .map(|n| n as usize + 8)
                    .filter(|n| *n <= bytes.len())
            {
                result.logical_size = Some(end);
            }
            if result.logical_size.is_none() {
                result.warning = Some("invalid animation container".into());
            }
        }
        _ => {}
    }
    result
}

pub fn decode_payload(bytes: &[u8], key: &[u8; 16]) -> Vec<u8> {
    let Some(body) = bytes.strip_prefix(b"dWV+") else {
        return bytes.to_vec();
    };
    let mut decoded = body.to_vec();
    let complete = decoded.len() / 16 * 16;
    let cipher = Aes128::new(&Array::from(*key));
    for batch in decoded[..complete].chunks_mut(16 * 32) {
        let count = batch.len() / 16;
        let mut blocks = [Array::from([0u8; 16]); 32];
        for (block, raw) in blocks.iter_mut().zip(batch.as_chunks::<16>().0.iter()) {
            block.copy_from_slice(raw);
        }
        cipher.decrypt_blocks(&mut blocks[..count]);
        for (raw, block) in batch
            .as_chunks_mut::<16>()
            .0
            .iter_mut()
            .zip(&blocks[..count])
        {
            raw.copy_from_slice(block);
        }
    }
    decoded[complete..].fill(0);
    decoded
}

pub fn decode_text_records(bytes: &[u8]) -> Result<Vec<u8>, Error> {
    let mut offset = 0usize;
    let mut text = Vec::new();
    while let Some(header) = bytes.get(offset..offset + 4) {
        let length = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        offset += 4;
        if length == u32::MAX {
            if bytes[offset..].iter().any(|b| *b != 0) {
                return Err(Error::InvalidText(offset));
            }
            return Ok(text);
        }
        let end = offset
            .checked_add(length as usize)
            .ok_or(Error::InvalidText(offset))?;
        let row = bytes.get(offset..end).ok_or(Error::InvalidText(offset))?;
        text.try_reserve(row.len())?;
        text.extend_from_slice(row);
        offset = end;
    }
    Err(Error::InvalidText(offset))
}
