//! Minimal, read-only ISO9660 access for PSP disc images.
//!
//! Directory records and file extents are read lazily. Opening an image reads
//! only volume descriptors, so large ISOs do not need to be scanned or unpacked.

use serde::Serialize;
use specta::Type;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use thiserror::Error;

const SECTOR_SIZE: u64 = 2048;
const VOLUME_DESCRIPTOR_START: u64 = 16;
const MAX_VOLUME_DESCRIPTORS: u64 = 64;
const MAX_DIRECTORY_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum IsoError {
    #[error("could not read ISO: {0}")]
    Io(#[from] std::io::Error),
    #[error("not an ISO9660 image: {0}")]
    InvalidImage(String),
    #[error("path not found in ISO: {0}")]
    NotFound(String),
    #[error("path is not a directory: {0}")]
    NotDirectory(String),
    #[error("requested range is outside the resource")]
    Range,
    #[error("ISO path contains an invalid segment")]
    InvalidPath,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct IsoMetadata {
    pub source_path: String,
    pub volume_id: String,
    pub logical_block_size: u16,
    pub volume_size: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct IsoEntry {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub size: u32,
    pub extent: u32,
    pub modified: Option<String>,
}

#[derive(Clone, Debug)]
struct Extent {
    lba: u32,
    size: u32,
    is_directory: bool,
}

#[derive(Debug)]
struct Volume {
    id: String,
    block_size: u16,
    blocks: u32,
    root: Extent,
}

pub struct IsoImage {
    path: PathBuf,
    file: Mutex<File>,
    file_len: u64,
    volume: Volume,
}

impl std::fmt::Debug for IsoImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IsoImage")
            .field("path", &self.path)
            .field("file_len", &self.file_len)
            .field("volume", &self.volume)
            .finish()
    }
}

impl IsoImage {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, IsoError> {
        let path = path.as_ref().to_path_buf();
        let mut file = File::open(&path)?;
        let file_len = file.metadata()?.len();
        let volume = read_primary_volume(&mut file, file_len)?;
        Ok(Self {
            path,
            file: Mutex::new(file),
            file_len,
            volume,
        })
    }

    pub fn metadata(&self) -> IsoMetadata {
        IsoMetadata {
            source_path: self.path.to_string_lossy().into_owned(),
            volume_id: self.volume.id.clone(),
            logical_block_size: self.volume.block_size,
            volume_size: self.volume.blocks.saturating_mul(u32::from(self.volume.block_size)),
        }
    }

    pub fn list_directory(&self, path: &str) -> Result<Vec<IsoEntry>, IsoError> {
        let normalized = normalize_path(path)?;
        let extent = self.resolve_extent(&normalized)?;
        if !extent.is_directory {
            return Err(IsoError::NotDirectory(normalized));
        }
        let data = self.read_extent(&extent)?;
        let mut entries = parse_directory(&data, &normalized, self.volume.block_size)?;
        entries.sort_by(|a, b| {
            b.is_directory
                .cmp(&a.is_directory)
                .then_with(|| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()))
        });
        Ok(entries)
    }

    pub fn entry(&self, path: &str) -> Result<IsoEntry, IsoError> {
        let normalized = normalize_path(path)?;
        if normalized == "/" {
            return Ok(IsoEntry {
                name: self.volume.id.clone(),
                path: "/".into(),
                is_directory: true,
                size: self.volume.root.size,
                extent: self.volume.root.lba.saturating_mul(u32::from(self.volume.block_size)),
                modified: None,
            });
        }
        let (parent, name) = split_parent(&normalized);
        self.list_directory(parent)?
            .into_iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| IsoError::NotFound(normalized))
    }

    pub fn read_file(&self, path: &str) -> Result<Vec<u8>, IsoError> {
        let normalized = normalize_path(path)?;
        let extent = self.resolve_extent(&normalized)?;
        if extent.is_directory {
            return Err(IsoError::NotDirectory(normalized));
        }
        self.read_extent(&extent)
    }

    pub fn read_file_range(
        &self,
        path: &str,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>, IsoError> {
        let normalized = normalize_path(path)?;
        let extent = self.resolve_extent(&normalized)?;
        let size = u64::from(extent.size);
        if extent.is_directory || offset > size || length as u64 > size.saturating_sub(offset) {
            return Err(IsoError::Range);
        }
        let absolute = u64::from(extent.lba) * u64::from(self.volume.block_size) + offset;
        self.read_at(absolute, length)
    }

    fn resolve_extent(&self, normalized_path: &str) -> Result<Extent, IsoError> {
        if normalized_path == "/" {
            return Ok(self.volume.root.clone());
        }
        let mut current = self.volume.root.clone();
        let mut current_path = String::new();
        for segment in normalized_path.trim_start_matches('/').split('/') {
            if !current.is_directory {
                return Err(IsoError::NotDirectory(current_path));
            }
            let bytes = self.read_extent(&current)?;
            current_path.push('/');
            current_path.push_str(segment);
            current = parse_directory_extents(&bytes)?
                .into_iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(segment))
                .map(|(_, extent)| extent)
                .ok_or_else(|| IsoError::NotFound(current_path.clone()))?;
        }
        Ok(current)
    }

    fn read_extent(&self, extent: &Extent) -> Result<Vec<u8>, IsoError> {
        let size = u64::from(extent.size);
        if extent.is_directory && size > MAX_DIRECTORY_BYTES {
            return Err(IsoError::InvalidImage(format!(
                "directory extent is unexpectedly large ({size} bytes)"
            )));
        }
        let absolute = u64::from(extent.lba) * u64::from(self.volume.block_size);
        self.read_at(absolute, extent.size as usize)
    }

    fn read_at(&self, offset: u64, length: usize) -> Result<Vec<u8>, IsoError> {
        if offset > self.file_len || length as u64 > self.file_len.saturating_sub(offset) {
            return Err(IsoError::Range);
        }
        let mut data = vec![0; length];
        let mut file = self
            .file
            .lock()
            .map_err(|_| IsoError::InvalidImage("ISO file lock was poisoned".into()))?;
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(&mut data)?;
        Ok(data)
    }
}

fn read_primary_volume(file: &mut File, file_len: u64) -> Result<Volume, IsoError> {
    let mut sector = vec![0; SECTOR_SIZE as usize];
    for index in VOLUME_DESCRIPTOR_START..VOLUME_DESCRIPTOR_START + MAX_VOLUME_DESCRIPTORS {
        let offset = index * SECTOR_SIZE;
        if offset + SECTOR_SIZE > file_len {
            break;
        }
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(&mut sector)?;
        if &sector[1..6] != b"CD001" || sector[6] != 1 {
            continue;
        }
        match sector[0] {
            1 => {
                let block_size = le_u16(&sector, 128)?;
                if block_size == 0 {
                    return Err(IsoError::InvalidImage("logical block size is zero".into()));
                }
                let root = parse_record(&sector[156..])?.1;
                if !root.is_directory {
                    return Err(IsoError::InvalidImage("root record is not a directory".into()));
                }
                let id = String::from_utf8_lossy(&sector[40..72])
                    .trim_end_matches(|value| value == ' ' || value == '\0')
                    .to_owned();
                return Ok(Volume {
                    id,
                    block_size,
                    blocks: le_u32(&sector, 80)?,
                    root,
                });
            }
            255 => break,
            _ => {}
        }
    }
    Err(IsoError::InvalidImage(
        "primary volume descriptor was not found".into(),
    ))
}

fn parse_directory(
    data: &[u8],
    parent: &str,
    block_size: u16,
) -> Result<Vec<IsoEntry>, IsoError> {
    let mut output: Vec<IsoEntry> = Vec::new();
    walk_records(data, block_size, |name, extent, modified| {
        if name == "." || name == ".." {
            return;
        }
        let path = if parent == "/" {
            format!("/{name}")
        } else {
            format!("{parent}/{name}")
        };
        if let Some(previous) = output.last_mut().filter(|entry| {
            entry.name.eq_ignore_ascii_case(&name) && entry.is_directory == extent.is_directory
        }) {
            previous.size = previous.size.saturating_add(extent.size);
            return;
        }
        output.push(IsoEntry {
            name,
            path,
            is_directory: extent.is_directory,
            size: extent.size,
            extent: extent.lba.saturating_mul(u32::from(block_size)),
            modified,
        });
    })?;
    Ok(output)
}

fn parse_directory_extents(data: &[u8]) -> Result<Vec<(String, Extent)>, IsoError> {
    let mut output = Vec::new();
    walk_records(data, 2048, |name, extent, _| {
        if name != "." && name != ".." {
            output.push((name, extent));
        }
    })?;
    Ok(output)
}

fn walk_records(
    data: &[u8],
    block_size: u16,
    mut visit: impl FnMut(String, Extent, Option<String>),
) -> Result<(), IsoError> {
    let block_size = usize::from(block_size);
    let mut cursor = 0usize;
    while cursor < data.len() {
        let record_len = data[cursor] as usize;
        if record_len == 0 {
            cursor = ((cursor / block_size) + 1) * block_size;
            continue;
        }
        if record_len < 34 || cursor + record_len > data.len() {
            return Err(IsoError::InvalidImage("malformed directory record".into()));
        }
        let (name, extent) = parse_record(&data[cursor..cursor + record_len])?;
        let modified = parse_recording_time(&data[cursor + 18..cursor + 25]);
        visit(name, extent, modified);
        cursor += record_len;
    }
    Ok(())
}

fn parse_record(data: &[u8]) -> Result<(String, Extent), IsoError> {
    let len = *data
        .first()
        .ok_or_else(|| IsoError::InvalidImage("missing directory record".into()))?
        as usize;
    if len < 34 || data.len() < len {
        return Err(IsoError::InvalidImage("short directory record".into()));
    }
    let name_len = data[32] as usize;
    if 33 + name_len > len {
        return Err(IsoError::InvalidImage("invalid file identifier length".into()));
    }
    let raw_name = &data[33..33 + name_len];
    let name = match raw_name {
        [0] => ".".into(),
        [1] => "..".into(),
        _ => String::from_utf8_lossy(raw_name)
            .split(';')
            .next()
            .unwrap_or_default()
            .trim_end_matches('.')
            .to_owned(),
    };
    Ok((
        name,
        Extent {
            lba: le_u32(data, 2)?,
            size: le_u32(data, 10)?,
            is_directory: data[25] & 0x02 != 0,
        },
    ))
}

fn parse_recording_time(raw: &[u8]) -> Option<String> {
    if raw.len() < 7 || raw[..6].iter().all(|value| *value == 0) {
        return None;
    }
    Some(format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        u16::from(raw[0]) + 1900,
        raw[1],
        raw[2],
        raw[3],
        raw[4],
        raw[5]
    ))
}

fn normalize_path(path: &str) -> Result<String, IsoError> {
    let mut output = Vec::new();
    let normalized_separators = path.replace('\\', "/");
    for segment in normalized_separators.split('/') {
        match segment {
            "" | "." => {}
            ".." => return Err(IsoError::InvalidPath),
            value if value.contains('\0') => return Err(IsoError::InvalidPath),
            value => output.push(value),
        }
    }
    Ok(if output.is_empty() {
        "/".into()
    } else {
        format!("/{}", output.join("/"))
    })
}

fn split_parent(path: &str) -> (&str, &str) {
    let index = path.rfind('/').unwrap_or(0);
    let parent = if index == 0 { "/" } else { &path[..index] };
    (parent, &path[index + 1..])
}

fn le_u16(data: &[u8], offset: usize) -> Result<u16, IsoError> {
    let bytes: [u8; 2] = data
        .get(offset..offset + 2)
        .ok_or_else(|| IsoError::InvalidImage("unexpected end of data".into()))?
        .try_into()
        .expect("slice length checked");
    Ok(u16::from_le_bytes(bytes))
}

fn le_u32(data: &[u8], offset: usize) -> Result<u32, IsoError> {
    let bytes: [u8; 4] = data
        .get(offset..offset + 4)
        .ok_or_else(|| IsoError::InvalidImage("unexpected end of data".into()))?
        .try_into()
        .expect("slice length checked");
    Ok(u32::from_le_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn record(name: &[u8], lba: u32, size: u32, directory: bool) -> Vec<u8> {
        let length = 33 + name.len() + usize::from(name.len() % 2 == 0);
        let mut value = vec![0u8; length];
        value[0] = length as u8;
        value[2..6].copy_from_slice(&lba.to_le_bytes());
        value[6..10].copy_from_slice(&lba.to_be_bytes());
        value[10..14].copy_from_slice(&size.to_le_bytes());
        value[14..18].copy_from_slice(&size.to_be_bytes());
        value[18..25].copy_from_slice(&[126, 7, 24, 12, 30, 0, 0]);
        value[25] = if directory { 2 } else { 0 };
        value[28..30].copy_from_slice(&1u16.to_le_bytes());
        value[30..32].copy_from_slice(&1u16.to_be_bytes());
        value[32] = name.len() as u8;
        value[33..33 + name.len()].copy_from_slice(name);
        value
    }

    fn fixture() -> Vec<u8> {
        let mut iso = vec![0u8; 32 * SECTOR_SIZE as usize];
        let pvd = 16 * SECTOR_SIZE as usize;
        iso[pvd] = 1;
        iso[pvd + 1..pvd + 6].copy_from_slice(b"CD001");
        iso[pvd + 6] = 1;
        iso[pvd + 40..pvd + 48].copy_from_slice(b"NGE2TEST");
        iso[pvd + 80..pvd + 84].copy_from_slice(&32u32.to_le_bytes());
        iso[pvd + 84..pvd + 88].copy_from_slice(&32u32.to_be_bytes());
        iso[pvd + 128..pvd + 130].copy_from_slice(&(SECTOR_SIZE as u16).to_le_bytes());
        iso[pvd + 130..pvd + 132].copy_from_slice(&(SECTOR_SIZE as u16).to_be_bytes());
        let root_record = record(&[0], 20, SECTOR_SIZE as u32, true);
        iso[pvd + 156..pvd + 156 + root_record.len()].copy_from_slice(&root_record);

        let root = 20 * SECTOR_SIZE as usize;
        let records = [
            record(&[0], 20, SECTOR_SIZE as u32, true),
            record(&[1], 20, SECTOR_SIZE as u32, true),
            record(b"README.TXT;1", 21, 5, false),
            record(b"PSP_GAME", 22, SECTOR_SIZE as u32, true),
        ];
        let mut cursor = root;
        for value in records {
            iso[cursor..cursor + value.len()].copy_from_slice(&value);
            cursor += value.len();
        }
        iso[21 * SECTOR_SIZE as usize..21 * SECTOR_SIZE as usize + 5]
            .copy_from_slice(b"hello");
        let child = 22 * SECTOR_SIZE as usize;
        let child_records = [
            record(&[0], 22, SECTOR_SIZE as u32, true),
            record(&[1], 20, SECTOR_SIZE as u32, true),
            record(b"PARAM.SFO;1", 23, 3, false),
        ];
        cursor = child;
        for value in child_records {
            iso[cursor..cursor + value.len()].copy_from_slice(&value);
            cursor += value.len();
        }
        iso[23 * SECTOR_SIZE as usize..23 * SECTOR_SIZE as usize + 3]
            .copy_from_slice(b"SFO");
        iso
    }

    #[test]
    fn opens_and_reads_iso_without_unpacking() {
        let path = std::env::temp_dir().join(format!(
            "nge2-iso-vfs-{}-{}.iso",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        fs::write(&path, fixture()).unwrap();
        let image = IsoImage::open(&path).unwrap();
        assert_eq!(image.metadata().volume_id, "NGE2TEST");
        let root = image.list_directory("/").unwrap();
        assert_eq!(root.len(), 2);
        assert!(root[0].is_directory);
        assert_eq!(image.read_file("/readme.txt").unwrap(), b"hello");
        assert_eq!(image.read_file("PSP_GAME/PARAM.SFO").unwrap(), b"SFO");
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_parent_segments() {
        assert!(matches!(normalize_path("/../secret"), Err(IsoError::InvalidPath)));
    }
}
