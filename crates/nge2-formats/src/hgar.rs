use crate::{u16_le, u32_le, FormatError};
use flate2::read::DeflateDecoder;
use serde::Serialize;
use specta::Type;
use std::io::Read;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HgarArchive {
    pub version: u16,
    pub entries: Vec<HgarEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HgarEntry {
    pub index: u32,
    pub identifier: u32,
    pub encoded_identifier: u32,
    pub short_name: String,
    pub long_name: Option<String>,
    pub display_name: String,
    pub compressed: bool,
    pub size: u32,
    pub content_offset: u32,
    pub unknown_first: Option<u32>,
    pub unknown_last: Option<u32>,
}

impl HgarArchive {
    pub fn parse(data: &[u8]) -> Result<Self, FormatError> {
        if data.get(..4) != Some(b"HGAR") {
            return Err(invalid("missing HGAR signature"));
        }
        let version = u16_le(data, 4)?;
        if !matches!(version, 1 | 3) {
            return Err(FormatError::Unsupported {
                format: "HGAR",
                message: format!("version {version}"),
            });
        }
        let count = usize::from(u16_le(data, 6)?);
        if count > 32768 {
            return Err(invalid("member count exceeds 32768"));
        }
        let mut header_offsets = Vec::with_capacity(count);
        for index in 0..count {
            header_offsets.push(u32_le(data, 8 + index * 4)? as usize);
        }

        let mut cursor = 8 + count * 4;
        let mut unknowns = vec![(None, None); count];
        let mut long_names = vec![None; count];
        if version == 3 {
            for value in &mut unknowns {
                *value = (Some(u32_le(data, cursor)?), Some(u32_le(data, cursor + 4)?));
                cursor += 8;
            }
            for expected_index in 0..count {
                let stored_index = u32_le(data, cursor)? as usize;
                cursor += 4;
                let start = cursor;
                loop {
                    let chunk = data
                        .get(cursor..cursor + 4)
                        .ok_or(FormatError::UnexpectedEof(cursor))?;
                    cursor += 4;
                    if chunk[3] == 0 {
                        break;
                    }
                    if cursor - start > 4096 {
                        return Err(invalid("long member name exceeds 4096 bytes"));
                    }
                }
                let end = data[start..cursor]
                    .iter()
                    .position(|byte| *byte == 0)
                    .map(|offset| start + offset)
                    .unwrap_or(cursor);
                let name = String::from_utf8_lossy(&data[start..end]).trim().to_owned();
                let destination = if stored_index < count {
                    stored_index
                } else {
                    expected_index
                };
                long_names[destination] = (!name.is_empty()).then_some(name);
            }
        }

        let identifier_limit = identifier_limit(count);
        let mut entries = Vec::with_capacity(count);
        for index in 0..count {
            let offset = header_offsets[index];
            if offset < cursor || offset + 20 > data.len() {
                return Err(invalid(&format!("member {index} has an invalid header offset")));
            }
            let short_name = parse_short_name(&data[offset..offset + 12]);
            let encoded_identifier = u32_le(data, offset + 12)?;
            let size = u32_le(data, offset + 16)? as usize;
            let content_offset = offset + 20;
            if content_offset + size > data.len() {
                return Err(invalid(&format!("member {index} extends beyond the archive")));
            }
            let long_name = long_names[index].clone();
            let display_name = viable_name(long_name.as_deref(), &short_name);
            entries.push(HgarEntry {
                index: index as u32,
                identifier: decode_identifier(encoded_identifier, identifier_limit),
                encoded_identifier,
                short_name,
                long_name,
                display_name,
                compressed: encoded_identifier & 0x8000_0000 != 0,
                size: size as u32,
                content_offset: content_offset as u32,
                unknown_first: unknowns[index].0,
                unknown_last: unknowns[index].1,
            });
        }
        Ok(Self { version, entries })
    }

    pub fn entry_data(&self, data: &[u8], index: usize) -> Result<Vec<u8>, FormatError> {
        let entry = self
            .entries
            .get(index)
            .ok_or_else(|| invalid("member index is out of range"))?;
        let start = entry.content_offset as usize;
        let end = start + entry.size as usize;
        let raw = data
            .get(start..end)
            .ok_or(FormatError::UnexpectedEof(start))?;
        if !entry.compressed {
            return Ok(raw.to_vec());
        }
        decompress_member(raw)
    }
}

pub fn decompress_member(data: &[u8]) -> Result<Vec<u8>, FormatError> {
    let expected = u32_le(data, 0)? as usize;
    let mut decoder = DeflateDecoder::new(&data[4..]);
    let mut output = Vec::with_capacity(expected);
    decoder
        .read_to_end(&mut output)
        .map_err(|error| FormatError::Decompression(error.to_string()))?;
    if output.len() != expected {
        return Err(FormatError::Decompression(format!(
            "expected {expected} bytes, decoded {}",
            output.len()
        )));
    }
    Ok(output)
}

fn parse_short_name(raw: &[u8]) -> String {
    let stem = String::from_utf8_lossy(&raw[..8]).trim_end().to_owned();
    let extension = String::from_utf8_lossy(&raw[8..]).trim().to_owned();
    format!("{stem}{extension}").trim_end_matches('.').to_owned()
}

fn viable_name(long_name: Option<&str>, short_name: &str) -> String {
    match long_name {
        Some(long) if !long.eq_ignore_ascii_case(short_name) => long.to_owned(),
        _ => short_name.to_owned(),
    }
}

fn identifier_limit(count: usize) -> u32 {
    let mut limit = 16usize;
    while count > limit && limit < 32768 {
        limit *= 2;
    }
    (2 * limit.min(32768)) as u32
}

fn decode_identifier(encoded: u32, limit: u32) -> u32 {
    let mut xor_mask = encoded & 0x7fff_ffff;
    let mut result = 0u32;
    for _ in 0..6 {
        result = (result ^ xor_mask).wrapping_mul(0x3d09);
        xor_mask >>= 5;
    }
    result & (limit - 1)
}

fn invalid(message: &str) -> FormatError {
    FormatError::Invalid {
        format: "HGAR",
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{write::DeflateEncoder, Compression};
    use std::io::Write;

    fn fixture(compressed: bool) -> Vec<u8> {
        let original = b".EVS fixture";
        let content = if compressed {
            let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(original).unwrap();
            let compressed_data = encoder.finish().unwrap();
            [
                (original.len() as u32).to_le_bytes().as_slice(),
                compressed_data.as_slice(),
            ]
            .concat()
        } else {
            original.to_vec()
        };
        let mut archive = Vec::new();
        archive.extend_from_slice(b"HGAR");
        archive.extend_from_slice(&1u16.to_le_bytes());
        archive.extend_from_slice(&1u16.to_le_bytes());
        archive.extend_from_slice(&12u32.to_le_bytes());
        archive.extend_from_slice(b"a000    .evs");
        archive.extend_from_slice(&(if compressed { 0x8000_0001u32 } else { 1 }).to_le_bytes());
        archive.extend_from_slice(&(content.len() as u32).to_le_bytes());
        archive.extend_from_slice(&content);
        archive
    }

    #[test]
    fn parses_member_metadata_and_content() {
        let data = fixture(false);
        let archive = HgarArchive::parse(&data).unwrap();
        assert_eq!(archive.entries[0].short_name, "a000.evs");
        assert_eq!(archive.entry_data(&data, 0).unwrap(), b".EVS fixture");
    }

    #[test]
    fn decompresses_raw_deflate_members() {
        let data = fixture(true);
        let archive = HgarArchive::parse(&data).unwrap();
        assert!(archive.entries[0].compressed);
        assert_eq!(archive.entry_data(&data, 0).unwrap(), b".EVS fixture");
    }
}
