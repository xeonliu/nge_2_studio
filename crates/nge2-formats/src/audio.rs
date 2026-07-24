use crate::{align, FormatError};
use atrac3p_decoder::Decoder;
use std::io::{Cursor, Read, Seek, SeekFrom};

const WAVE_ALIGNMENT: usize = 0x800;
const MAX_DECODED_SAMPLES: usize = 8 * 48_000 * 60 * 10;
const DECODER_STACK_SIZE: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaveEntry {
    pub offset: u64,
    pub size: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedAudio {
    pub wav: Vec<u8>,
    pub sample_rate: u32,
    pub channels: u16,
    pub duration_millis: u32,
}

pub fn index_wave_archive<R: Read + Seek>(
    reader: &mut R,
    archive_size: u64,
) -> Result<Vec<WaveEntry>, FormatError> {
    let mut entries = Vec::new();
    let mut offset = 0u64;
    let mut header = [0u8; 12];
    while offset < archive_size {
        reader
            .seek(SeekFrom::Start(offset))
            .and_then(|_| reader.read_exact(&mut header))
            .map_err(|error| audio_error(error.to_string()))?;
        if &header[..4] != b"RIFF" || &header[8..] != b"WAVE" {
            return Err(audio_error(format!("missing RIFF/WAVE header at 0x{offset:X}")));
        }
        let size = u32::from_le_bytes(header[4..8].try_into().expect("four-byte slice"))
            .checked_add(8)
            .ok_or_else(|| audio_error("RIFF size overflows u32"))?;
        if size < 12 || u64::from(size) > archive_size - offset {
            return Err(audio_error(format!("invalid RIFF size at 0x{offset:X}")));
        }
        entries.push(WaveEntry { offset, size });
        offset = offset
            .checked_add(align(size as usize, WAVE_ALIGNMENT) as u64)
            .ok_or_else(|| audio_error("WAVE archive offset overflow"))?;
    }
    if offset != archive_size {
        return Err(audio_error("WAVE archive has a partial trailing block"));
    }
    Ok(entries)
}

pub fn decode_atrac3plus(data: &[u8]) -> Result<DecodedAudio, FormatError> {
    let data = data.to_vec();
    std::thread::Builder::new()
        .name("atrac3plus-decoder".into())
        .stack_size(DECODER_STACK_SIZE)
        .spawn(move || decode_atrac3plus_inner(&data))
        .map_err(|error| audio_error(format!("could not start decoder thread: {error}")))?
        .join()
        .map_err(|_| audio_error("ATRAC3+ decoder thread panicked"))?
}

fn decode_atrac3plus_inner(data: &[u8]) -> Result<DecodedAudio, FormatError> {
    let (channels, sample_rate) = wave_spec(data)?;
    let decoder = Decoder::new(Cursor::new(data)).map_err(|error| audio_error(error.to_string()))?;
    let samples = decoder.take(MAX_DECODED_SAMPLES + 1).collect::<Vec<_>>();
    if samples.len() > MAX_DECODED_SAMPLES {
        return Err(audio_error("decoded audio exceeds the ten-minute safety limit"));
    }
    let duration_millis = ((samples.len() as u64 * 1000)
        / (u64::from(sample_rate) * u64::from(channels)))
    .min(u64::from(u32::MAX)) as u32;
    Ok(DecodedAudio {
        wav: encode_pcm_wave(&samples, channels, sample_rate)?,
        sample_rate,
        channels,
        duration_millis,
    })
}

fn wave_spec(data: &[u8]) -> Result<(u16, u32), FormatError> {
    if data.get(..4) != Some(b"RIFF") || data.get(8..12) != Some(b"WAVE") {
        return Err(audio_error("input is not a RIFF/WAVE file"));
    }
    let mut cursor = 12usize;
    while cursor + 8 <= data.len() {
        let size = u32::from_le_bytes(
            data[cursor + 4..cursor + 8]
                .try_into()
                .expect("four-byte slice"),
        ) as usize;
        let body = cursor + 8;
        let end = body
            .checked_add(size)
            .filter(|end| *end <= data.len())
            .ok_or_else(|| audio_error("RIFF chunk extends beyond the file"))?;
        if &data[cursor..cursor + 4] == b"fmt " {
            if size < 16 {
                return Err(audio_error("WAVE fmt chunk is too short"));
            }
            let channels = u16::from_le_bytes(data[body + 2..body + 4].try_into().unwrap());
            let sample_rate = u32::from_le_bytes(data[body + 4..body + 8].try_into().unwrap());
            if channels == 0 || sample_rate == 0 {
                return Err(audio_error("WAVE has an invalid channel count or sample rate"));
            }
            return Ok((channels, sample_rate));
        }
        cursor = end + (size & 1);
    }
    Err(audio_error("WAVE fmt chunk was not found"))
}

fn encode_pcm_wave(samples: &[f32], channels: u16, sample_rate: u32) -> Result<Vec<u8>, FormatError> {
    let data_size = samples
        .len()
        .checked_mul(2)
        .and_then(|size| u32::try_from(size).ok())
        .ok_or_else(|| audio_error("PCM output is too large for RIFF"))?;
    let byte_rate = sample_rate
        .checked_mul(u32::from(channels))
        .and_then(|value| value.checked_mul(2))
        .ok_or_else(|| audio_error("PCM byte rate overflow"))?;
    let block_align = channels
        .checked_mul(2)
        .ok_or_else(|| audio_error("PCM block alignment overflow"))?;
    let mut output = Vec::with_capacity(44 + data_size as usize);
    output.extend_from_slice(b"RIFF");
    output.extend_from_slice(&(36 + data_size).to_le_bytes());
    output.extend_from_slice(b"WAVEfmt ");
    output.extend_from_slice(&16u32.to_le_bytes());
    output.extend_from_slice(&1u16.to_le_bytes());
    output.extend_from_slice(&channels.to_le_bytes());
    output.extend_from_slice(&sample_rate.to_le_bytes());
    output.extend_from_slice(&byte_rate.to_le_bytes());
    output.extend_from_slice(&block_align.to_le_bytes());
    output.extend_from_slice(&16u16.to_le_bytes());
    output.extend_from_slice(b"data");
    output.extend_from_slice(&data_size.to_le_bytes());
    for sample in samples {
        let pcm = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16;
        output.extend_from_slice(&pcm.to_le_bytes());
    }
    Ok(output)
}

fn audio_error(message: impl Into<String>) -> FormatError {
    FormatError::Audio(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn riff(size: usize) -> Vec<u8> {
        let mut value = vec![0u8; size];
        value[..4].copy_from_slice(b"RIFF");
        value[4..8].copy_from_slice(&((size - 8) as u32).to_le_bytes());
        value[8..12].copy_from_slice(b"WAVE");
        value
    }

    #[test]
    fn indexes_aligned_concatenated_wave_files() {
        let first = riff(0x810);
        let second = riff(0x110);
        let mut archive = first.clone();
        archive.resize(0x1000, 0);
        archive.extend_from_slice(&second);
        archive.resize(0x1800, 0);

        let entries = index_wave_archive(&mut Cursor::new(&archive), archive.len() as u64).unwrap();
        assert_eq!(entries, vec![WaveEntry { offset: 0, size: 0x810 }, WaveEntry { offset: 0x1000, size: 0x110 }]);
    }

    #[test]
    fn writes_standard_pcm_wave_header() {
        let wav = encode_pcm_wave(&[-1.0, 0.0, 1.0], 1, 44_100).unwrap();
        assert_eq!(&wav[..12], b"RIFF*\0\0\0WAVE");
        assert_eq!(&wav[36..44], b"data\x06\0\0\0");
        assert_eq!(&wav[44..], b"\x01\x80\0\0\xff\x7f");
    }

    #[test]
    fn decodes_configured_atrac3plus_fixture() {
        let Ok(path) = std::env::var("NGE2_ATRAC_FIXTURE") else {
            return;
        };
        let data = std::fs::read(path).unwrap();
        let decoded = decode_atrac3plus(&data).unwrap();
        assert_eq!((decoded.channels, decoded.sample_rate), (1, 44_100));
        assert!(decoded.duration_millis > 100);
        assert_eq!(&decoded.wav[..4], b"RIFF");
        assert_eq!(&decoded.wav[8..12], b"WAVE");
        assert!(decoded.wav.len() > 44);
        assert!(decoded.wav[44..].iter().any(|byte| *byte != 0));
    }
}
