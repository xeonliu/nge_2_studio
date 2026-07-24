use crate::{u32_le, FormatError};
use flate2::read::DeflateDecoder;
use std::io::Read;

/// Decode the standalone ZPT wrapper used by files under `USRDIR/im`.
///
/// ZPT consists of a little-endian decompressed size followed by a raw
/// DEFLATE stream whose payload is an HGPT image.
pub fn decompress(data: &[u8]) -> Result<Vec<u8>, FormatError> {
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
    if output.get(..4) != Some(b"HGPT") {
        return Err(FormatError::Invalid {
            format: "ZPT",
            message: "decompressed payload is not HGPT".into(),
        });
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{write::DeflateEncoder, Compression};
    use std::io::Write;

    #[test]
    fn decodes_size_prefixed_raw_deflate() {
        let original = b"HGPT fixture";
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(original).unwrap();
        let mut zpt = (original.len() as u32).to_le_bytes().to_vec();
        zpt.extend_from_slice(&encoder.finish().unwrap());

        assert_eq!(decompress(&zpt).unwrap(), original);
    }

    #[test]
    fn rejects_non_hgpt_payload() {
        let original = b"not an image";
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(original).unwrap();
        let mut zpt = (original.len() as u32).to_le_bytes().to_vec();
        zpt.extend_from_slice(&encoder.finish().unwrap());

        assert!(matches!(decompress(&zpt), Err(FormatError::Invalid { format: "ZPT", .. })));
    }
}
