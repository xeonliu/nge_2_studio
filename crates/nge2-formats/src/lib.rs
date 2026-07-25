pub mod audio;
pub mod evs;
pub mod hgar;
pub mod hgpt;
pub mod sound;
mod sound_mappings;
#[path = "voice_ranges.rs"]
pub mod voice;
pub mod zpt;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FormatError {
    #[error("unexpected end of file at 0x{0:X}")]
    UnexpectedEof(usize),
    #[error("invalid {format}: {message}")]
    Invalid {
        format: &'static str,
        message: String,
    },
    #[error("unsupported {format}: {message}")]
    Unsupported {
        format: &'static str,
        message: String,
    },
    #[error("decompression failed: {0}")]
    Decompression(String),
    #[error("PNG encoding failed: {0}")]
    Png(String),
    #[error("audio decode failed: {0}")]
    Audio(String),
}

pub(crate) fn bytes<const N: usize>(data: &[u8], offset: usize) -> Result<[u8; N], FormatError> {
    data.get(offset..offset + N)
        .ok_or(FormatError::UnexpectedEof(offset))?
        .try_into()
        .map_err(|_| FormatError::UnexpectedEof(offset))
}

pub(crate) fn u16_le(data: &[u8], offset: usize) -> Result<u16, FormatError> {
    Ok(u16::from_le_bytes(bytes(data, offset)?))
}

pub(crate) fn u32_le(data: &[u8], offset: usize) -> Result<u32, FormatError> {
    Ok(u32::from_le_bytes(bytes(data, offset)?))
}

pub(crate) fn align(value: usize, alignment: usize) -> usize {
    value.div_ceil(alignment) * alignment
}
