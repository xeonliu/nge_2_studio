from __future__ import annotations

import struct
from dataclasses import dataclass

from .errors import ParseError


@dataclass(frozen=True)
class Reader:
    data: bytes
    resource: str

    def require(self, offset: int, size: int, label: str = "data") -> memoryview:
        if offset < 0 or size < 0 or offset > len(self.data) - size:
            raise ParseError(
                f"{label} range 0x{offset:X}..0x{offset + size:X} is outside "
                f"the 0x{len(self.data):X}-byte resource",
                resource=self.resource,
                offset=max(offset, 0),
            )
        return memoryview(self.data)[offset : offset + size]

    def unpack(self, fmt: str, offset: int, label: str = "field") -> tuple[object, ...]:
        size = struct.calcsize(fmt)
        return struct.unpack_from(fmt, self.require(offset, size, label))

    def u8(self, offset: int, label: str = "u8") -> int:
        return self.require(offset, 1, label)[0]

    def u16(self, offset: int, label: str = "u16") -> int:
        return int.from_bytes(self.require(offset, 2, label), "little")

    def i16(self, offset: int, label: str = "i16") -> int:
        return int.from_bytes(self.require(offset, 2, label), "little", signed=True)

    def u32(self, offset: int, label: str = "u32") -> int:
        return int.from_bytes(self.require(offset, 4, label), "little")

    def f32(self, offset: int, label: str = "f32") -> float:
        return struct.unpack_from("<f", self.require(offset, 4, label))[0]


def align(value: int, alignment: int) -> int:
    return (value + alignment - 1) & -alignment


def fourcc(value: int) -> str:
    raw = value.to_bytes(4, "little")
    return "".join(chr(byte) if 0x20 <= byte < 0x7F else f"\\x{byte:02x}" for byte in raw)
