from __future__ import annotations

import zlib
from dataclasses import dataclass
from pathlib import Path

from .binary import Reader
from .errors import ParseError

COMPRESSED_FLAG = 0x8000_0000
RESOURCE_KEY_MASK = 0x7FFF_FFFF


@dataclass(frozen=True)
class HgarEntry:
    index: int
    encoded_identifier: int
    decoded_identifier: int
    short_name: str
    long_name: str | None
    content_offset: int
    stored_size: int
    compressed: bool
    data: bytes

    @property
    def resource_key(self) -> int:
        """Runtime references use this typed key, not the decoded hash-table ID."""
        return self.encoded_identifier & RESOURCE_KEY_MASK

    @property
    def name(self) -> str:
        if self.long_name and self.long_name.casefold() != self.short_name.casefold():
            return self.long_name
        return self.short_name

    @property
    def signature(self) -> bytes:
        return self.data[:4]


@dataclass(frozen=True)
class HgarArchive:
    version: int
    entries: tuple[HgarEntry, ...]

    @classmethod
    def from_file(cls, path: Path) -> HgarArchive:
        try:
            return cls.parse(path.read_bytes(), resource=str(path))
        except OSError as error:
            raise ParseError(str(error), resource=str(path)) from error

    @classmethod
    def parse(cls, data: bytes, *, resource: str = "archive.har") -> HgarArchive:
        reader = Reader(data, resource)
        if bytes(reader.require(0, 4, "HGAR signature")) != b"HGAR":
            raise ParseError("missing HGAR signature", resource=resource, offset=0)
        version = reader.u16(4, "HGAR version")
        if version not in (1, 3):
            raise ParseError(f"unsupported HGAR version {version}", resource=resource, offset=4)
        count = reader.u16(6, "member count")
        if count > 32768:
            raise ParseError("member count exceeds 32768", resource=resource, offset=6)

        table_end = 8 + count * 4
        reader.require(8, count * 4, "member offset table")
        header_offsets = [reader.u32(8 + index * 4) for index in range(count)]
        cursor = table_end
        long_names: list[str | None] = [None] * count
        if version == 3:
            reader.require(cursor, count * 8, "v3 hash table")
            cursor += count * 8
            for expected_index in range(count):
                stored_index = reader.u32(cursor, "long-name member index")
                cursor += 4
                start = cursor
                while True:
                    chunk = reader.require(cursor, 4, "aligned long name")
                    cursor += 4
                    if chunk[3] == 0:
                        break
                    if cursor - start > 4096:
                        raise ParseError(
                            "long member name exceeds 4096 bytes",
                            resource=resource,
                            offset=start,
                        )
                raw = data[start:cursor].split(b"\0", 1)[0]
                name = raw.decode("utf-8", errors="replace").strip()
                destination = stored_index if stored_index < count else expected_index
                long_names[destination] = name or None

        identifier_limit = _identifier_limit(count)
        entries: list[HgarEntry] = []
        for index, offset in enumerate(header_offsets):
            if offset < cursor:
                raise ParseError(
                    "member header overlaps archive tables", resource=resource, offset=offset
                )
            header = reader.require(offset, 20, f"member {index} header")
            stem = bytes(header[:8]).decode("ascii", errors="replace").rstrip()
            extension = bytes(header[8:12]).decode("ascii", errors="replace").strip()
            short_name = f"{stem}{extension}".rstrip(".")
            encoded = reader.u32(offset + 12, "encoded identifier")
            stored_size = reader.u32(offset + 16, "stored member size")
            content_offset = offset + 20
            raw = bytes(reader.require(content_offset, stored_size, f"member {index} content"))
            content = (
                _decompress(raw, resource, content_offset) if encoded & COMPRESSED_FLAG else raw
            )
            entries.append(
                HgarEntry(
                    index=index,
                    encoded_identifier=encoded,
                    decoded_identifier=_decode_identifier(encoded, identifier_limit),
                    short_name=short_name,
                    long_name=long_names[index],
                    content_offset=content_offset,
                    stored_size=stored_size,
                    compressed=bool(encoded & COMPRESSED_FLAG),
                    data=content,
                )
            )
        return cls(version=version, entries=tuple(entries))

    def by_resource_key(self) -> dict[int, HgarEntry]:
        result: dict[int, HgarEntry] = {}
        for entry in self.entries:
            result.setdefault(entry.resource_key, entry)
        return result

    def resources_by_key(self) -> dict[int, tuple[HgarEntry, ...]]:
        result: dict[int, list[HgarEntry]] = {}
        for entry in self.entries:
            result.setdefault(entry.resource_key, []).append(entry)
        return {key: tuple(entries) for key, entries in result.items()}


def _decompress(data: bytes, resource: str, offset: int) -> bytes:
    if len(data) < 4:
        raise ParseError("compressed member has no size prefix", resource=resource, offset=offset)
    expected = int.from_bytes(data[:4], "little")
    try:
        output = zlib.decompress(data[4:], wbits=-zlib.MAX_WBITS)
    except zlib.error as error:
        raise ParseError(
            f"raw DEFLATE decompression failed: {error}", resource=resource, offset=offset + 4
        ) from error
    if len(output) != expected:
        raise ParseError(
            f"decompressed size is {len(output)}, expected {expected}",
            resource=resource,
            offset=offset,
        )
    return output


def _identifier_limit(count: int) -> int:
    limit = 16
    while count > limit and limit < 32768:
        limit *= 2
    return 2 * min(limit, 32768)


def _decode_identifier(encoded: int, limit: int) -> int:
    xor_mask = encoded & RESOURCE_KEY_MASK
    result = 0
    for _ in range(6):
        result = ((result ^ xor_mask) * 0x3D09) & 0xFFFF_FFFF
        xor_mask >>= 5
    return result & (limit - 1)
