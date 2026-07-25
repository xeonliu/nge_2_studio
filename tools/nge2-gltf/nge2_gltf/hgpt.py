from __future__ import annotations

from dataclasses import dataclass
from io import BytesIO

from PIL import Image

from .binary import Reader, align
from .errors import ParseError, UnsupportedFeature


@dataclass(frozen=True)
class HgptDivision:
    x: int
    y: int
    width: int
    height: int


@dataclass(frozen=True)
class HgptImage:
    width: int
    height: int
    format_code: int
    divisions: tuple[HgptDivision, ...]
    rgba: bytes

    @classmethod
    def parse(cls, data: bytes, *, resource: str = "texture.hpt") -> HgptImage:
        reader = Reader(data, resource)
        if bytes(reader.require(0, 4, "HGPT signature")) != b"HGPT":
            raise ParseError("missing HGPT signature", resource=resource, offset=0)
        pp_offset = reader.u16(4, "PP offset")
        extended = reader.u16(6, "extended-header flag")
        division_count = reader.u16(8, "division count")
        if pp_offset < 16:
            raise ParseError("PP offset precedes header", resource=resource, offset=4)
        if extended not in (0, 1):
            raise ParseError("extended-header flag must be 0 or 1", resource=resource, offset=6)
        divisions: list[HgptDivision] = []
        if extended:
            if reader.u16(16, "repeated division count") != division_count:
                raise ParseError("division counts do not match", resource=resource, offset=16)
            cursor = 28
            for _ in range(division_count):
                divisions.append(
                    HgptDivision(
                        reader.u16(cursor),
                        reader.u16(cursor + 2),
                        reader.u16(cursor + 4),
                        reader.u16(cursor + 6),
                    )
                )
                cursor += 8
            if cursor > pp_offset:
                raise ParseError(
                    "division table overlaps PP data", resource=resource, offset=cursor
                )

        pp_header = reader.u32(pp_offset, "PP header")
        if pp_header & 0xFFFF != 0x7070:
            raise ParseError("missing PP header", resource=resource, offset=pp_offset)
        format_code = pp_header >> 16
        formats = {0x14: (32, 1, 2), 0x13: (16, 1, 1), 0x8800: (4, 4, 1)}
        if format_code not in formats:
            raise UnsupportedFeature(
                f"HGPT pixel format 0x{format_code:04X}", resource=resource, offset=pp_offset
            )
        tile_width, bytes_per_unit, units_per_byte = formats[format_code]
        width = reader.u16(pp_offset + 4, "image width")
        height = reader.u16(pp_offset + 6, "image height")
        if width == 0 or height == 0 or width * height > 32_000_000:
            raise ParseError("invalid image dimensions", resource=resource, offset=pp_offset + 4)
        ppd_offset = pp_offset + 16
        ppd = reader.u32(ppd_offset, "PPD header")
        if ppd & 0x00FF_FFFF != 0x0064_7070 or ppd >> 24 != format_code & 0xFF:
            raise ParseError(
                "missing or mismatched PPD header", resource=resource, offset=ppd_offset
            )
        if reader.u16(ppd_offset + 4) != width or reader.u16(ppd_offset + 6) != height:
            raise ParseError(
                "PP and PPD dimensions differ", resource=resource, offset=ppd_offset + 4
            )
        storage_width = align(width, tile_width)
        storage_height = align(height, 8)
        unit_count = storage_width * storage_height
        data_size = unit_count * bytes_per_unit // units_per_byte
        pixel_offset = ppd_offset + 32
        tiled = reader.require(pixel_offset, data_size, "tiled pixels")
        palette: tuple[tuple[int, int, int, int], ...] = ()
        if format_code != 0x8800:
            palette_offset = pixel_offset + data_size
            if reader.u32(palette_offset, "PPC header") != 0x0063_7070:
                raise ParseError(
                    "missing PPC palette header", resource=resource, offset=palette_offset
                )
            count = reader.u16(palette_offset + 6, "palette block count") * 8
            expected = 16 if format_code == 0x14 else 256
            if count != expected:
                raise ParseError(
                    f"palette contains {count} colors, expected {expected}",
                    resource=resource,
                    offset=palette_offset + 6,
                )
            raw_palette = reader.require(palette_offset + 16, count * 4, "palette colors")
            palette = tuple(
                (raw_palette[i], raw_palette[i + 1], raw_palette[i + 2], _alpha(raw_palette[i + 3]))
                for i in range(0, len(raw_palette), 4)
            )

        rgba = bytearray(width * height * 4)
        for y in range(height):
            for x in range(width):
                tiled_index = _tile_index(x, y, storage_width, tile_width)
                if format_code == 0x14:
                    packed = tiled[tiled_index // 2]
                    color = palette[(packed >> 4) if tiled_index & 1 else (packed & 0x0F)]
                elif format_code == 0x13:
                    color = palette[tiled[tiled_index]]
                else:
                    source = tiled_index * 4
                    color = (
                        tiled[source],
                        tiled[source + 1],
                        tiled[source + 2],
                        _alpha(tiled[source + 3]),
                    )
                destination = (y * width + x) * 4
                rgba[destination : destination + 4] = bytes(color)
        return cls(width, height, format_code, tuple(divisions), bytes(rgba))

    @property
    def has_alpha(self) -> bool:
        return any(value != 255 for value in self.rgba[3::4])

    def encode_png(self) -> bytes:
        output = BytesIO()
        Image.frombytes("RGBA", (self.width, self.height), self.rgba).save(output, format="PNG")
        return output.getvalue()


def _tile_index(x: int, y: int, storage_width: int, tile_width: int) -> int:
    tile_size = tile_width * 8
    return (
        (y // 8) * tile_size * (storage_width // tile_width)
        + (x // tile_width) * tile_size
        + (y % 8) * tile_width
        + x % tile_width
    )


def _alpha(value: int) -> int:
    return min(value * 2, 255)
