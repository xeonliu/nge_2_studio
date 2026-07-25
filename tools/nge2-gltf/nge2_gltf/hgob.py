from __future__ import annotations

import math
import struct
from dataclasses import dataclass, field

from .binary import Reader, fourcc
from .errors import ParseError

HMS_RESOURCE_TYPE = 0x1500_0000


@dataclass(frozen=True)
class HgobProperty:
    opcode: int
    size_flags: int
    payload: bytes
    offset: int


@dataclass
class HgobNode:
    object_id: int
    class_id: str
    properties: tuple[HgobProperty, ...]
    offset: int
    translation: tuple[float, float, float] = (0.0, 0.0, 0.0)
    rotation: tuple[float, float, float, float] = (0.0, 0.0, 0.0, 1.0)
    scale: tuple[float, float, float] = (1.0, 1.0, 1.0)
    parent_id: int | None = None
    hms_resource_key: int | None = None
    parent_index: int | None = None
    children: list[int] = field(default_factory=list)

    @property
    def name(self) -> str:
        return fourcc(self.object_id)

    @property
    def unknown_properties(self) -> list[dict[str, str | int]]:
        known = {0x04, 0x09, 0x0C, 0x0D, 0x28}
        return [
            {
                "opcode": prop.opcode,
                "sizeFlags": prop.size_flags,
                "offset": prop.offset,
                "dataHex": prop.payload.hex(),
            }
            for prop in self.properties
            if prop.opcode not in known
        ]


@dataclass(frozen=True)
class Hgob:
    nodes: tuple[HgobNode, ...]

    @classmethod
    def parse(cls, data: bytes, *, resource: str = "model.hob") -> Hgob:
        reader = Reader(data, resource)
        if bytes(reader.require(0, 4, "HGOB signature")) != b"HGOB":
            raise ParseError("missing HGOB signature", resource=resource, offset=0)
        count = reader.u16(4, "object count")
        offsets = [reader.u16(6 + index * 2, "object offset") for index in range(count)]
        minimum = 6 + count * 2
        nodes: list[HgobNode] = []
        seen_ids: dict[int, int] = {}
        for index, offset in enumerate(offsets):
            if offset < minimum:
                raise ParseError("object overlaps offset table", resource=resource, offset=offset)
            object_id = reader.u32(offset, "object FourCC")
            if object_id in seen_ids:
                raise ParseError(
                    f"duplicate object ID {fourcc(object_id)!r}", resource=resource, offset=offset
                )
            seen_ids[object_id] = index
            raw_class = bytes(reader.require(offset + 4, 2, "object class"))
            class_id = raw_class.decode("ascii", errors="replace")
            property_count = reader.u16(offset + 6, "property count")
            descriptors_end = offset + 8 + property_count * 2
            reader.require(offset + 8, property_count * 2, "property descriptors")
            payload_cursor = descriptors_end
            properties: list[HgobProperty] = []
            for prop_index in range(property_count):
                descriptor = offset + 8 + prop_index * 2
                opcode = reader.u8(descriptor)
                size_flags = reader.u8(descriptor + 1)
                size = size_flags & 0x7F
                payload = bytes(reader.require(payload_cursor, size, f"property 0x{opcode:02X}"))
                properties.append(HgobProperty(opcode, size_flags, payload, payload_cursor))
                payload_cursor += size
            node = HgobNode(object_id, class_id, tuple(properties), offset)
            _decode_properties(node, resource)
            nodes.append(node)

        for index, node in enumerate(nodes):
            if node.parent_id is None or node.parent_id == 0:
                continue
            parent_index = seen_ids.get(node.parent_id)
            if parent_index is None:
                raise ParseError(
                    f"parent {fourcc(node.parent_id)!r} does not exist",
                    resource=resource,
                    offset=node.offset,
                )
            node.parent_index = parent_index
            nodes[parent_index].children.append(index)
        _validate_acyclic(nodes, resource)
        return cls(tuple(nodes))


def _decode_properties(node: HgobNode, resource: str) -> None:
    for prop in node.properties:
        if prop.opcode == 0x04:
            _require_size(prop, 8, resource)
            values = struct.unpack("<4h", prop.payload)
            quaternion = tuple(value / 32000.0 for value in values)
            length = math.sqrt(sum(value * value for value in quaternion))
            if not math.isfinite(length) or length < 1e-8:
                raise ParseError("invalid zero quaternion", resource=resource, offset=prop.offset)
            node.rotation = tuple(value / length for value in quaternion)  # type: ignore[assignment]
        elif prop.opcode == 0x09:
            _require_size(prop, 4, resource)
            node.parent_id = int.from_bytes(prop.payload, "little")
        elif prop.opcode == 0x0C:
            _require_size(prop, 12, resource)
            node.translation = struct.unpack("<3f", prop.payload)
        elif prop.opcode == 0x0D:
            _require_size(prop, 12, resource)
            node.scale = struct.unpack("<3f", prop.payload)
        elif prop.opcode == 0x28:
            _require_size(prop, 4, resource)
            if node.class_id != "MO":
                raise ParseError(
                    "HGMS binding appears on a non-MO object",
                    resource=resource,
                    offset=prop.offset,
                )
            raw = int.from_bytes(prop.payload, "little")
            identifier = raw & 0x00FF_FFFF
            # Observed SHDW nodes use 0x000FFF as the engine's null model sentinel.
            if identifier != 0x000FFF:
                node.hms_resource_key = HMS_RESOURCE_TYPE | identifier
    values = (*node.translation, *node.rotation, *node.scale)
    if not all(math.isfinite(value) for value in values):
        raise ParseError(
            "node transform contains NaN or Infinity", resource=resource, offset=node.offset
        )


def _require_size(prop: HgobProperty, expected: int, resource: str) -> None:
    if len(prop.payload) != expected:
        raise ParseError(
            f"property 0x{prop.opcode:02X} has size {len(prop.payload)}, expected {expected}",
            resource=resource,
            offset=prop.offset,
        )


def _validate_acyclic(nodes: list[HgobNode], resource: str) -> None:
    state = [0] * len(nodes)

    def visit(index: int) -> None:
        if state[index] == 1:
            raise ParseError(
                "object hierarchy contains a cycle", resource=resource, offset=nodes[index].offset
            )
        if state[index] == 2:
            return
        state[index] = 1
        if nodes[index].parent_index is not None:
            visit(nodes[index].parent_index)
        state[index] = 2

    for index in range(len(nodes)):
        visit(index)
