use crate::{align, u16_le, u32_le, FormatError};
use serde::Serialize;
use specta::Type;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HgptImage {
    pub width: u32,
    pub height: u32,
    pub pixel_format: HgptPixelFormat,
    pub divisions: Vec<HgptDivision>,
    #[serde(skip)]
    #[specta(skip)]
    pub rgba: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HgptDivision {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Type)]
pub enum HgptPixelFormat {
    Indexed4,
    Indexed8,
    Rgba8888,
}

impl HgptImage {
    pub fn parse(data: &[u8]) -> Result<Self, FormatError> {
        if data.get(..4) != Some(b"HGPT") {
            return Err(invalid("missing HGPT signature"));
        }
        let pp_offset = usize::from(u16_le(data, 4)?);
        if pp_offset < 16 || pp_offset + 48 > data.len() {
            return Err(invalid("PP offset is outside the file"));
        }
        let has_extended_header = u16_le(data, 6)?;
        if has_extended_header > 1 {
            return Err(invalid("extended header flag must be zero or one"));
        }
        let division_count = usize::from(u16_le(data, 8)?);
        let mut divisions = Vec::with_capacity(division_count);
        if has_extended_header == 1 {
            if u16_le(data, 16)? as usize != division_count {
                return Err(invalid("division count does not match its repeat"));
            }
            let mut cursor = 28;
            for _ in 0..division_count {
                divisions.push(HgptDivision {
                    x: u16_le(data, cursor)?,
                    y: u16_le(data, cursor + 2)?,
                    width: u16_le(data, cursor + 4)?,
                    height: u16_le(data, cursor + 6)?,
                });
                cursor += 8;
            }
            if cursor > pp_offset {
                return Err(invalid("division table overlaps PP data"));
            }
        }

        let pp_header = u32_le(data, pp_offset)?;
        if pp_header & 0xffff != 0x7070 {
            return Err(invalid("missing PP header"));
        }
        let format_code = (pp_header >> 16) as u16;
        let (pixel_format, tile_width, bytes_per_pixel) = match format_code {
            0x14 => (HgptPixelFormat::Indexed4, 32usize, 0.5f32),
            0x13 => (HgptPixelFormat::Indexed8, 16usize, 1.0f32),
            0x8800 => (HgptPixelFormat::Rgba8888, 4usize, 4.0f32),
            _ => {
                return Err(FormatError::Unsupported {
                    format: "HGPT",
                    message: format!("pixel format 0x{format_code:04X}"),
                })
            }
        };
        let width = usize::from(u16_le(data, pp_offset + 4)?);
        let height = usize::from(u16_le(data, pp_offset + 6)?);
        if width == 0 || height == 0 || width.saturating_mul(height) > 32_000_000 {
            return Err(invalid("invalid image dimensions"));
        }
        let ppd_offset = pp_offset + 16;
        let ppd_header = u32_le(data, ppd_offset)?;
        if ppd_header & 0x00ff_ffff != 0x0064_7070
            || (ppd_header >> 24) as u8 != format_code as u8
        {
            return Err(invalid("missing or mismatched PPD header"));
        }
        if usize::from(u16_le(data, ppd_offset + 4)?) != width
            || usize::from(u16_le(data, ppd_offset + 6)?) != height
        {
            return Err(invalid("PP and PPD dimensions do not match"));
        }
        let storage_width = align(width, tile_width);
        let storage_height = align(height, 8);
        let pixel_count = storage_width
            .checked_mul(storage_height)
            .ok_or_else(|| invalid("storage dimensions overflow"))?;
        let data_bytes = (pixel_count as f32 * bytes_per_pixel) as usize;
        let pixel_offset = ppd_offset + 32;
        let tiled = data
            .get(pixel_offset..pixel_offset + data_bytes)
            .ok_or(FormatError::UnexpectedEof(pixel_offset))?;

        let palette = if pixel_format == HgptPixelFormat::Rgba8888 {
            Vec::new()
        } else {
            parse_palette(data, pixel_offset + data_bytes)?
        };
        let expected_palette = match pixel_format {
            HgptPixelFormat::Indexed4 => 16,
            HgptPixelFormat::Indexed8 => 256,
            HgptPixelFormat::Rgba8888 => 0,
        };
        if palette.len() != expected_palette {
            return Err(invalid(&format!(
                "expected {expected_palette} palette colors, found {}",
                palette.len()
            )));
        }

        let mut rgba = vec![0u8; width * height * 4];
        for y in 0..height {
            for x in 0..width {
                let tiled_index = tile_index(x, y, storage_width, tile_width);
                let color = match pixel_format {
                    HgptPixelFormat::Indexed4 => {
                        let byte = tiled[tiled_index / 2];
                        let index = if tiled_index % 2 == 0 { byte & 0x0f } else { byte >> 4 };
                        palette[index as usize]
                    }
                    HgptPixelFormat::Indexed8 => palette[tiled[tiled_index] as usize],
                    HgptPixelFormat::Rgba8888 => {
                        let source = tiled_index * 4;
                        [
                            tiled[source],
                            tiled[source + 1],
                            tiled[source + 2],
                            decode_alpha(tiled[source + 3]),
                        ]
                    }
                };
                let destination = (y * width + x) * 4;
                rgba[destination..destination + 4].copy_from_slice(&color);
            }
        }

        Ok(Self {
            width: width as u32,
            height: height as u32,
            pixel_format,
            divisions,
            rgba,
        })
    }

    pub fn encode_png(&self) -> Result<Vec<u8>, FormatError> {
        let mut output = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut output, self.width, self.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder
                .write_header()
                .map_err(|error| FormatError::Png(error.to_string()))?;
            writer
                .write_image_data(&self.rgba)
                .map_err(|error| FormatError::Png(error.to_string()))?;
        }
        Ok(output)
    }
}

fn parse_palette(data: &[u8], offset: usize) -> Result<Vec<[u8; 4]>, FormatError> {
    if u32_le(data, offset)? != 0x0063_7070 {
        return Err(invalid("missing PPC palette header"));
    }
    let count = usize::from(u16_le(data, offset + 6)?) * 8;
    let colors_offset = offset + 16;
    let raw = data
        .get(colors_offset..colors_offset + count * 4)
        .ok_or(FormatError::UnexpectedEof(colors_offset))?;
    Ok(raw
        .chunks_exact(4)
        .map(|color| [color[0], color[1], color[2], decode_alpha(color[3])])
        .collect())
}

fn tile_index(x: usize, y: usize, storage_width: usize, tile_width: usize) -> usize {
    let tile_height = 8;
    let tile_size = tile_width * tile_height;
    let tile_row = tile_size * (storage_width / tile_width);
    (y / tile_height) * tile_row
        + (x / tile_width) * tile_size
        + (y % tile_height) * tile_width
        + (x % tile_width)
}

fn decode_alpha(value: u8) -> u8 {
    value.saturating_mul(2)
}

fn invalid(message: &str) -> FormatError {
    FormatError::Invalid {
        format: "HGPT",
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgba_fixture() -> Vec<u8> {
        let width = 4u16;
        let height = 8u16;
        let mut data = Vec::new();
        data.extend_from_slice(b"HGPT");
        data.extend_from_slice(&16u16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0x8800_7070u32.to_le_bytes());
        data.extend_from_slice(&width.to_le_bytes());
        data.extend_from_slice(&height.to_le_bytes());
        data.extend_from_slice(&[0; 8]);
        data.extend_from_slice(&0x0064_7070u32.to_le_bytes());
        data.extend_from_slice(&width.to_le_bytes());
        data.extend_from_slice(&height.to_le_bytes());
        data.extend_from_slice(&[0; 4]);
        data.extend_from_slice(&16u16.to_le_bytes());
        data.extend_from_slice(&8u16.to_le_bytes());
        data.extend_from_slice(&64u32.to_le_bytes());
        data.extend_from_slice(&[0; 12]);
        for index in 0..32u8 {
            data.extend_from_slice(&[index, 255 - index, 64, 0x80]);
        }
        data
    }

    #[test]
    fn decodes_rgba_tiles_and_png() {
        let image = HgptImage::parse(&rgba_fixture()).unwrap();
        assert_eq!((image.width, image.height), (4, 8));
        assert_eq!(&image.rgba[..4], &[0, 255, 64, 255]);
        assert_eq!(&image.rgba[4..8], &[1, 254, 64, 255]);
        assert_eq!(&image.encode_png().unwrap()[..8], b"\x89PNG\r\n\x1a\n");
    }
}
