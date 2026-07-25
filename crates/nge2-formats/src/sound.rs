use crate::audio::{encode_pcm_wave, DecodedAudio};
use crate::sound_mappings::*;
use crate::FormatError;

const MAX_TONES_PER_PROGRAM: usize = 128;
const MAX_DECODED_SAMPLES: usize = 48_000 * 60 * 10;
const PSX_FILTERS: [[i32; 2]; 5] = [[0, 0], [60, 0], [115, -52], [98, -55], [122, -60]];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventSoundKind {
    Free,
    Battle,
    Battle01,
    Battle57,
    Angel,
    End,
    Tv,
    Chara,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoundEffectMapping {
    pub sound_id: u32,
    pub packed_id: u32,
    pub logical_bank: u8,
    pub slot: u8,
    pub program: u8,
    pub note: u8,
    pub tracked: bool,
}

pub fn event_sound_kind(archive_name: &str) -> Option<EventSoundKind> {
    let stem = archive_name
        .rsplit('/')
        .next()
        .unwrap_or(archive_name)
        .strip_suffix(".har")
        .unwrap_or(archive_name)
        .to_ascii_lowercase();
    if stem.starts_with("tev") {
        Some(EventSoundKind::Tv)
    } else if stem.starts_with("cev") {
        Some(EventSoundKind::Chara)
    } else if stem.starts_with("b2s") || stem.starts_with("bs") {
        Some(EventSoundKind::Battle)
    } else if stem.starts_with("bk") {
        Some(EventSoundKind::Battle57)
    } else if stem.starts_with('n') {
        Some(EventSoundKind::Battle01)
    } else if stem.starts_with('a') {
        Some(EventSoundKind::Angel)
    } else if stem.starts_with('e') {
        Some(EventSoundKind::End)
    } else if stem.starts_with('f') {
        Some(EventSoundKind::Free)
    } else {
        None
    }
}

pub fn resolve_sound_effect(sound_id: u32, archive_name: &str) -> Option<SoundEffectMapping> {
    let packed_id = sound_id
        .checked_sub(1000)
        .and_then(|index| SYSTEM_SOUND_MAP.get(index as usize).copied())
        .filter(|value| *value != u32::MAX)
        .or_else(|| event_sound_mapping(sound_id, archive_name))?;
    if packed_id == 0 || packed_id == u32::MAX || packed_id & 0x80 != 0 {
        return None;
    }
    let logical_bank = ((packed_id >> 16) & 0xff) as u8;
    Some(SoundEffectMapping {
        sound_id,
        packed_id,
        logical_bank,
        slot: logical_bank_slot(logical_bank)?,
        program: ((packed_id >> 8) & 0x7f) as u8,
        note: (packed_id & 0x7f) as u8,
        tracked: packed_id & 0x8000 != 0,
    })
}

fn event_sound_mapping(sound_id: u32, archive_name: &str) -> Option<u32> {
    let kind = event_sound_kind(archive_name)?;
    supports_generic_event_sounds(kind)
        .then(|| sound_id.checked_sub(2000))
        .flatten()
        .and_then(|index| GENERIC_EVENT_SOUND_MAP.get(index as usize).copied())
        .or_else(|| event_map(kind).get(sound_id as usize).copied())
}

fn supports_generic_event_sounds(kind: EventSoundKind) -> bool {
    matches!(
        kind,
        EventSoundKind::Battle
            | EventSoundKind::Battle01
            | EventSoundKind::Battle57
            | EventSoundKind::Angel
    )
}

fn event_map(kind: EventSoundKind) -> &'static [u32] {
    match kind {
        EventSoundKind::Free => &FREE_EVENT_SOUND_MAP,
        EventSoundKind::Battle => &BATTLE_EVENT_SOUND_MAP,
        EventSoundKind::Battle01 => &BATTLE01_SOUND_MAP,
        EventSoundKind::Battle57 => &BATTLE57_SOUND_MAP,
        EventSoundKind::Angel => &ANGEL_SOUND_MAP,
        EventSoundKind::End => &END_EVENT_SOUND_MAP,
        EventSoundKind::Tv => &TV_EVENT_SOUND_MAP,
        EventSoundKind::Chara => &CHARA_EVENT_SOUND_MAP,
    }
}

fn logical_bank_slot(bank: u8) -> Option<u8> {
    match bank {
        0 => Some(0),
        1 | 12 => Some(2),
        2 | 24..=32 | 102..=107 => Some(3),
        23
        | 3..=11
        | 13..=17
        | 33..=92
        | 111..=190
        | 192..=204
        | 206..=214
        | 217..=219
        | 221..=246 => Some(1),
        _ => None,
    }
}

pub fn decode_sound_bank(data: &[u8], program: u8, note: u8) -> Result<DecodedAudio, FormatError> {
    let (phd, pbd) = bank_members(data)?;
    if phd.get(..4) != Some(b"PPHD") {
        return Err(sound_error(
            "sound bank .phd member is missing PPHD signature",
        ));
    }
    let pppg = section(phd, 16, b"PPPG")?;
    let pptn = section(phd, 20, b"PPTN")?;
    let ppva = section(phd, 24, b"PPVA")?;
    let program_offset = read_u32(pppg, 32 + usize::from(program) * 4)? as usize;
    let program_data = phd
        .get(program_offset..)
        .ok_or_else(|| sound_error("PPPG program offset is out of PPHD bounds"))?;
    let tone_count = read_u32(program_data, 0)? as usize;
    if tone_count > MAX_TONES_PER_PROGRAM {
        return Err(sound_error("PPPG program has too many tones"));
    }

    let first_tone = read_u32(pptn, 16)?;
    let last_tone = read_u32(pptn, 20)?;
    let first_wave = read_u32(ppva, 16)?;
    let last_wave = read_u32(ppva, 20)?;
    let mut layers = Vec::new();
    for index in 0..tone_count {
        let tone_id = read_u32(program_data, 16 + index * 4)?;
        if !(first_tone..=last_tone).contains(&tone_id) {
            continue;
        }
        let tone = record(pptn, 32, 96, (tone_id - first_tone) as usize)?;
        let low_note = read_u32(tone, 16)?;
        let high_note = read_u32(tone, 20)?;
        if !(low_note..=high_note).contains(&u32::from(note)) {
            continue;
        }
        let wave_id = read_u32(tone, 0)?;
        if wave_id == u32::MAX || !(first_wave..=last_wave).contains(&wave_id) {
            continue;
        }
        let wave = record(ppva, 32, 16, (wave_id - first_wave) as usize)?;
        let offset = read_u32(wave, 0)? as usize;
        let sample_rate = read_u32(wave, 4)?;
        let size = read_u32(wave, 8)? as usize;
        if !(8_000..=96_000).contains(&sample_rate) {
            return Err(sound_error(format!(
                "invalid PSX ADPCM sample rate {sample_rate}"
            )));
        }
        let bytes = pbd
            .get(offset..offset.saturating_add(size))
            .ok_or_else(|| sound_error("PPVA sample range is out of .pbd bounds"))?;
        layers.push((sample_rate, decode_psx_adpcm(bytes)?));
    }
    if layers.is_empty() {
        return Err(sound_error(format!(
            "program {program}, note {note} has no playable PPVA wave"
        )));
    }
    mix_layers(layers)
}

fn bank_members(data: &[u8]) -> Result<(&[u8], &[u8]), FormatError> {
    let mut cursor = 0usize;
    let mut phd = None;
    let mut pbd = None;
    while cursor + 32 <= data.len() && data[cursor] != 0 {
        let extension = trim_ascii(&data[cursor + 16..cursor + 24]);
        let offset = read_u32(data, cursor + 24)? as usize;
        let size = read_u32(data, cursor + 28)? as usize;
        let member = data
            .get(offset..offset.saturating_add(size))
            .ok_or_else(|| sound_error("sound bank member is out of bounds"))?;
        match extension {
            b".phd" => phd = Some(member),
            b".pbd" => pbd = Some(member),
            _ => {}
        }
        cursor += 32;
    }
    Ok((
        phd.ok_or_else(|| sound_error("sound bank has no .phd member"))?,
        pbd.ok_or_else(|| sound_error("sound bank has no .pbd member"))?,
    ))
}

fn section<'a>(
    phd: &'a [u8],
    header_offset: usize,
    magic: &[u8; 4],
) -> Result<&'a [u8], FormatError> {
    let offset = read_u32(phd, header_offset)? as usize;
    let section = phd
        .get(offset..)
        .ok_or_else(|| sound_error("PPHD section offset is out of bounds"))?;
    if section.get(..4) != Some(magic) {
        return Err(sound_error(format!(
            "PPHD section at 0x{offset:X} is not {}",
            String::from_utf8_lossy(magic)
        )));
    }
    Ok(section)
}

fn record(data: &[u8], base: usize, stride: usize, index: usize) -> Result<&[u8], FormatError> {
    let start = base
        .checked_add(
            index
                .checked_mul(stride)
                .ok_or_else(|| sound_error("record offset overflow"))?,
        )
        .ok_or_else(|| sound_error("record offset overflow"))?;
    data.get(start..start + stride)
        .ok_or_else(|| sound_error("sound bank record is out of bounds"))
}

fn decode_psx_adpcm(data: &[u8]) -> Result<Vec<f32>, FormatError> {
    let mut samples = Vec::with_capacity(data.len() / 16 * 28);
    let mut previous = 0i32;
    let mut previous_previous = 0i32;
    for block in data.chunks_exact(16) {
        let predictor = usize::from(block[0] >> 4);
        let shift = u32::from(block[0] & 0x0f);
        let coefficients = PSX_FILTERS
            .get(predictor)
            .ok_or_else(|| sound_error(format!("invalid PSX ADPCM predictor {predictor}")))?;
        for byte in &block[2..] {
            for nibble in [byte & 0x0f, byte >> 4] {
                let signed = if nibble >= 8 {
                    i32::from(nibble) - 16
                } else {
                    i32::from(nibble)
                };
                let residual = (signed << 12) >> shift;
                let sample = (residual
                    + ((previous * coefficients[0] + previous_previous * coefficients[1] + 32)
                        >> 6))
                    .clamp(i32::from(i16::MIN), i32::from(i16::MAX));
                previous_previous = previous;
                previous = sample;
                samples.push(sample as f32 / 32768.0);
                if samples.len() > MAX_DECODED_SAMPLES {
                    return Err(sound_error(
                        "decoded sound effect exceeds the ten-minute safety limit",
                    ));
                }
            }
        }
        if block[1] & 1 != 0 {
            break;
        }
    }
    Ok(samples)
}

fn mix_layers(layers: Vec<(u32, Vec<f32>)>) -> Result<DecodedAudio, FormatError> {
    let sample_rate = layers.iter().map(|(rate, _)| *rate).max().unwrap_or(44_100);
    let output_len = layers
        .iter()
        .map(|(rate, samples)| samples.len().saturating_mul(sample_rate as usize) / *rate as usize)
        .max()
        .unwrap_or_default();
    let mut mixed = vec![0.0f32; output_len];
    for (rate, samples) in &layers {
        for (index, output) in mixed.iter_mut().enumerate() {
            let source = index.saturating_mul(*rate as usize) / sample_rate as usize;
            if let Some(sample) = samples.get(source) {
                *output += *sample / layers.len() as f32;
            }
        }
    }
    let duration_millis =
        ((mixed.len() as u64 * 1000) / u64::from(sample_rate)).min(u64::from(u32::MAX)) as u32;
    Ok(DecodedAudio {
        wav: encode_pcm_wave(&mixed, 1, sample_rate)?,
        sample_rate,
        channels: 1,
        duration_millis,
    })
}

fn trim_ascii(value: &[u8]) -> &[u8] {
    let end = value
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(value.len());
    &value[..end]
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, FormatError> {
    data.get(offset..offset + 4)
        .and_then(|value| value.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| sound_error(format!("u32 at 0x{offset:X} is out of bounds")))
}

fn sound_error(message: impl Into<String>) -> FormatError {
    FormatError::Audio(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_system_and_event_sound_ids_from_the_runtime_tables() {
        let system = resolve_sound_effect(1005, "f052.har").unwrap();
        assert_eq!((system.slot, system.program, system.note), (0, 0, 0x40));
        assert_eq!(resolve_sound_effect(1005, "unknown.har"), Some(system));

        let event = resolve_sound_effect(109, "f052.har").unwrap();
        assert_eq!(
            (event.slot, event.logical_bank, event.note),
            (1, 0x3b, 0x4f)
        );

        assert_eq!(resolve_sound_effect(2008, "b2s04.har").unwrap().slot, 3);
        assert_eq!(resolve_sound_effect(2008, "f052.har"), None);
    }

    #[test]
    fn decodes_zero_psx_adpcm_frame() {
        let mut block = [0u8; 16];
        block[1] = 1;
        let decoded = decode_psx_adpcm(&block).unwrap();
        assert_eq!(decoded, vec![0.0; 28]);
    }

    #[test]
    fn decodes_configured_sound_bank_fixture() {
        let Ok(path) = std::env::var("NGE2_SOUND_BANK_FIXTURE") else {
            return;
        };
        let data = std::fs::read(path).unwrap();
        let decoded = decode_sound_bank(&data, 0, 0x40).unwrap();
        assert_eq!((decoded.channels, decoded.sample_rate), (1, 11_025));
        assert!(decoded.duration_millis > 100);
        assert_eq!(&decoded.wav[..4], b"RIFF");
    }
}
