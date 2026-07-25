from __future__ import annotations

import struct
import zlib


def command(opcode: int, argument: int = 0) -> bytes:
    return ((opcode << 24) | argument).to_bytes(4, "little")


def make_hgob() -> bytes:
    root = _object(b"ROOT", b"  ", [])
    mesh = _object(
        b"MESH",
        b"MO",
        [
            (0x09, struct.pack("<I", int.from_bytes(b"ROOT", "little"))),
            (0x28, struct.pack("<I", 2)),
            (0x77, b"\xaa\xbb"),
        ],
    )
    first = 10
    return b"HGOB" + struct.pack("<H2H", 2, first, first + len(root)) + root + mesh


def make_hgms(*, vtype: int = 0x336) -> bytes:
    data = bytearray(0x80)
    data[:4] = b"HGMS"
    data[4:8] = bytes((1, 1, 1, 1))
    struct.pack_into("<HHI4f", data, 8, 0, 0, 0, 0.0, 0.0, 0.0, 1.0)
    struct.pack_into("<III", data, 0x20, 0x40, 0, 0x50)
    struct.pack_into("<I", data, 0x2C, 3)
    data[0x30:0x38] = bytes((0, 0, 0, 0, 0, 0, 0, 0))
    data[0x40:0x50] = struct.pack("<BBHI8B", 0, 0, 0xFFFF, 0x60, 0, *([0xFF] * 7))
    data[0x50:0x54] = b"ROOT"
    data[0x60:0x70] = (
        command(0x12, vtype) + command(0x01, 0x80) + command(0x04, (3 << 16) | 3) + command(0x0B)
    )
    data.extend(_vertex(128, 0, 0, 32768, 32768))
    data.extend(_vertex(128, 32767, 0, 36864, 32768))
    data.extend(_vertex(128, 0, 32767, 32768, 36864))
    return bytes(data)


def make_hgpt(format_code: int = 0x8800) -> bytes:
    width = 4 if format_code == 0x8800 else (16 if format_code == 0x13 else 32)
    height = 8
    tile_width = {0x8800: 4, 0x13: 16, 0x14: 32}[format_code]
    pixel_count = tile_width * 8
    if format_code == 0x8800:
        pixels = b"".join(bytes((index & 0xFF, 64, 255, 0x80)) for index in range(pixel_count))
    elif format_code == 0x13:
        pixels = bytes(index & 0xFF for index in range(pixel_count))
    else:
        pixels = bytes(
            ((index * 2) & 0x0F) | (((index * 2 + 1) & 0x0F) << 4)
            for index in range(pixel_count // 2)
        )
    data = bytearray()
    data.extend(b"HGPT")
    data.extend(struct.pack("<HHHHI", 16, 0, 0, 1, 0))
    data.extend(struct.pack("<IHH8x", (format_code << 16) | 0x7070, width, height))
    data.extend(
        struct.pack(
            "<IHH4xHHI12x",
            ((format_code & 0xFF) << 24) | 0x647070,
            width,
            height,
            tile_width,
            8,
            len(pixels),
        )
    )
    data.extend(pixels)
    if format_code != 0x8800:
        count = 16 if format_code == 0x14 else 256
        data.extend(struct.pack("<I2xH8x", 0x00637070, count // 8))
        for index in range(count):
            data.extend(bytes((index, 255 - index, 32, 0x80)))
    return bytes(data)


def make_hgar(*, compressed_hms: bool = False, version: int = 1) -> bytes:
    return make_archive(
        [
            ("model.hob", 0x11000001, make_hgob(), False),
            ("model.hms", 0x15000002, make_hgms(), compressed_hms),
            ("tex.hpt", 0x10000003, make_hgpt(), False),
        ],
        version=version,
    )


def make_archive(members: list[tuple[str, int, bytes, bool]], *, version: int = 1) -> bytes:
    count = len(members)
    prefix_size = 8 + count * 4
    extra = bytearray()
    if version == 3:
        extra.extend(b"\0" * (count * 8))
        for index, (name, _, _, _) in enumerate(members):
            encoded_name = name.encode() + b"\0"
            padded = encoded_name + b"\0" * ((4 - len(encoded_name) % 4) % 4)
            extra.extend(struct.pack("<I", index))
            extra.extend(padded)
    cursor = prefix_size + len(extra)
    offsets: list[int] = []
    bodies = bytearray()
    for name, key, content, compressed in members:
        offsets.append(cursor + len(bodies))
        stem, extension = name.rsplit(".", 1)
        short_name = stem[:8].ljust(8).encode() + ("." + extension[:3]).ljust(4).encode()
        stored = content
        encoded_key = key
        if compressed:
            compressor = zlib.compressobj(wbits=-zlib.MAX_WBITS)
            packed = compressor.compress(content) + compressor.flush()
            stored = struct.pack("<I", len(content)) + packed
            encoded_key |= 0x80000000
        bodies.extend(short_name)
        bodies.extend(struct.pack("<II", encoded_key, len(stored)))
        bodies.extend(stored)
    return (
        b"HGAR"
        + struct.pack("<HH", version, count)
        + struct.pack(f"<{count}I", *offsets)
        + extra
        + bodies
    )


def _object(object_id: bytes, class_id: bytes, properties: list[tuple[int, bytes]]) -> bytes:
    descriptors = b"".join(bytes((opcode, len(payload))) for opcode, payload in properties)
    payloads = b"".join(payload for _, payload in properties)
    return object_id + class_id + struct.pack("<H", len(properties)) + descriptors + payloads


def _vertex(weight: int, x: int, y: int, u: int, v: int) -> bytes:
    # VTYPE 0x336: u8 weight, aligned u16 UV, 5551 color, i8 normal, aligned i16 position.
    return struct.pack("<Bx2H H 3b x 3h", weight, u, v, 0xFFFF, 0, 0, 127, x, y, 0)
