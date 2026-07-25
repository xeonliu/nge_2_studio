from __future__ import annotations

import math
import struct
from dataclasses import dataclass

import numpy as np

from .binary import Reader, align
from .errors import ParseError, UnsupportedFeature

CMD_VADDR = 0x01
CMD_IADDR = 0x02
CMD_PRIM = 0x04
CMD_BEZIER = 0x05
CMD_SPLINE = 0x06
CMD_JUMP = 0x08
CMD_BJUMP = 0x09
CMD_CALL = 0x0A
CMD_RET = 0x0B
CMD_END = 0x0C
CMD_BASE = 0x10
CMD_VTYPE = 0x12
CMD_OFFSET_ADDR = 0x13
CMD_ORIGIN = 0x14

PRIMITIVE_NAMES = {
    0: "points",
    1: "lines",
    2: "line_strip",
    3: "triangles",
    4: "triangle_strip",
    5: "triangle_fan",
    6: "sprites",
}

GLTF_MODES = {"points": 0, "lines": 1, "line_strip": 3, "triangles": 4}


@dataclass(frozen=True)
class AttributeSpec:
    kind: str
    storage: str
    components: int
    offset: int
    size: int
    alignment: int


@dataclass(frozen=True)
class VertexFormat:
    raw: int
    attributes: tuple[AttributeSpec, ...]
    stride: int
    index_type: int
    weight_count: int
    transformed_2d: bool

    @classmethod
    def decode(cls, raw: int, *, resource: str, offset: int) -> VertexFormat:
        texture_type = raw & 0x3
        color_type = (raw >> 2) & 0x7
        normal_type = (raw >> 5) & 0x3
        position_type = (raw >> 7) & 0x3
        weight_type = (raw >> 9) & 0x3
        index_type = (raw >> 11) & 0x3
        weight_count = ((raw >> 14) & 0x7) + 1 if weight_type else 0
        morph_count = ((raw >> 18) & 0x7) + 1
        transformed_2d = bool(raw & (1 << 23))
        known_mask = 0x009D_DFFF
        unknown = raw & ~known_mask
        if unknown:
            raise UnsupportedFeature(
                f"VTYPE 0x{raw:06X} has unknown bits 0x{unknown:X}",
                resource=resource,
                offset=offset,
            )
        if index_type == 3:
            raise UnsupportedFeature(
                f"VTYPE 0x{raw:06X} uses reserved index type 3",
                resource=resource,
                offset=offset,
            )
        if morph_count != 1:
            raise UnsupportedFeature(
                f"VTYPE 0x{raw:06X} uses {morph_count} morph targets",
                resource=resource,
                offset=offset,
            )
        if position_type == 0:
            raise ParseError("VTYPE has no position", resource=resource, offset=offset)

        fields: list[tuple[str, int, int]] = []
        if weight_type:
            fields.append(("weights", weight_type, weight_count))
        if texture_type:
            fields.append(("texcoord", texture_type, 2))
        if color_type:
            fields.append(("color", color_type, 1))
        if normal_type:
            fields.append(("normal", normal_type, 3))
        fields.append(("position", position_type, 3))

        specs: list[AttributeSpec] = []
        cursor = 0
        maximum_alignment = 1
        for kind, storage_code, components in fields:
            if kind == "color":
                if storage_code not in (4, 5, 6, 7):
                    raise UnsupportedFeature(
                        f"VTYPE 0x{raw:06X} uses reserved color type {storage_code}",
                        resource=resource,
                        offset=offset,
                    )
                storage = {4: "5650", 5: "5551", 6: "4444", 7: "8888"}[storage_code]
                size = 4 if storage_code == 7 else 2
                field_alignment = size
            else:
                storage = {1: "u8", 2: "u16", 3: "f32"}[storage_code]
                component_size = {1: 1, 2: 2, 3: 4}[storage_code]
                size = components * component_size
                field_alignment = component_size
            cursor = align(cursor, field_alignment)
            specs.append(AttributeSpec(kind, storage, components, cursor, size, field_alignment))
            cursor += size
            maximum_alignment = max(maximum_alignment, field_alignment)
        return cls(
            raw,
            tuple(specs),
            align(cursor, maximum_alignment),
            index_type,
            weight_count,
            transformed_2d,
        )


@dataclass(frozen=True)
class DecodedPrimitive:
    mode: int
    positions: np.ndarray
    indices: np.ndarray
    normals: np.ndarray | None
    texcoords: np.ndarray | None
    colors: np.ndarray | None
    weights: np.ndarray | None
    source_offset: int
    vtype: int
    source_primitive: str

    @property
    def triangle_count(self) -> int:
        return len(self.indices) // 3 if self.mode == 4 else 0


@dataclass
class _GeState:
    base_address: int = 0
    offset_address: int = 0
    vertex_address: int | None = None
    index_address: int | None = None
    vertex_format: VertexFormat | None = None

    def address(self, argument: int) -> int:
        return ((self.base_address | argument) + self.offset_address) & 0xFFFF_FFFF


def decode_display_list(
    data: bytes,
    offset: int,
    *,
    resource: str,
    skip_unsupported: bool = False,
) -> tuple[list[DecodedPrimitive], list[str]]:
    reader = Reader(data, resource)
    state = _GeState()
    primitives: list[DecodedPrimitive] = []
    warnings: list[str] = []
    pc = offset
    command_limit = max(1, len(data) // 4)
    for _ in range(command_limit):
        word = reader.u32(pc, "GE command")
        opcode = word >> 24
        argument = word & 0x00FF_FFFF
        command_offset = pc
        pc += 4
        if opcode == CMD_VADDR:
            state.vertex_address = state.address(argument)
        elif opcode == CMD_IADDR:
            state.index_address = state.address(argument)
        elif opcode == CMD_BASE:
            state.base_address = (argument << 8) & 0xFF00_0000
        elif opcode == CMD_OFFSET_ADDR:
            raw = (argument << 8) & 0xFFFF_FFFF
            state.offset_address = raw - 0x1_0000_0000 if raw & 0x8000_0000 else raw
        elif opcode == CMD_ORIGIN:
            # GE ORIGIN makes subsequent relative addresses use the command stream origin.
            state.offset_address = command_offset - (state.base_address | argument)
        elif opcode == CMD_VTYPE:
            try:
                state.vertex_format = VertexFormat.decode(
                    argument, resource=resource, offset=command_offset
                )
            except UnsupportedFeature as error:
                if not skip_unsupported:
                    raise
                state.vertex_format = None
                warnings.append(str(error))
        elif opcode == CMD_PRIM:
            primitive_type = (argument >> 16) & 0x7
            count = argument & 0xFFFF
            name = PRIMITIVE_NAMES.get(primitive_type)
            if name is None or name == "sprites":
                error = UnsupportedFeature(
                    f"GE primitive type {primitive_type} ({name or 'reserved'})",
                    resource=resource,
                    offset=command_offset,
                )
                if not skip_unsupported:
                    raise error
                warnings.append(str(error))
                continue
            if count == 0:
                continue
            if state.vertex_format is None:
                error = ParseError(
                    "PRIM has no supported preceding VTYPE",
                    resource=resource,
                    offset=command_offset,
                )
                if not skip_unsupported:
                    raise error
                warnings.append(str(error))
                continue
            if state.vertex_address is None:
                raise ParseError(
                    "PRIM has no preceding VADDR", resource=resource, offset=command_offset
                )
            primitive, vertex_bytes, index_bytes = _decode_primitive(
                reader,
                state.vertex_address,
                state.index_address,
                state.vertex_format,
                count,
                name,
                command_offset,
            )
            primitives.append(primitive)
            state.vertex_address += vertex_bytes
            if state.index_address is not None:
                state.index_address += index_bytes
        elif opcode in (CMD_BEZIER, CMD_SPLINE, CMD_JUMP, CMD_BJUMP, CMD_CALL):
            error = UnsupportedFeature(
                f"geometry/control-flow GE command 0x{opcode:02X}",
                resource=resource,
                offset=command_offset,
            )
            if not skip_unsupported:
                raise error
            warnings.append(str(error))
        elif opcode in (CMD_RET, CMD_END):
            return primitives, warnings
        else:
            # All remaining commands configure fixed-function render state and do not
            # change vertex addressing or topology used by this exporter.
            continue
    raise ParseError("display list has no RET/END", resource=resource, offset=offset)


def _decode_primitive(
    reader: Reader,
    vertex_address: int,
    index_address: int | None,
    vertex_format: VertexFormat,
    count: int,
    primitive_name: str,
    command_offset: int,
) -> tuple[DecodedPrimitive, int, int]:
    source_indices, index_bytes = _read_source_indices(
        reader, index_address, vertex_format.index_type, count
    )
    if source_indices:
        highest = max(source_indices)
        vertex_count = highest + 1
    else:
        source_indices = list(range(count))
        vertex_count = count
    reader.require(
        vertex_address,
        vertex_count * vertex_format.stride,
        f"{vertex_count} packed vertices",
    )
    values: dict[str, list[object]] = {spec.kind: [] for spec in vertex_format.attributes}
    for vertex in range(vertex_count):
        base = vertex_address + vertex * vertex_format.stride
        for spec in vertex_format.attributes:
            values[spec.kind].append(_decode_attribute(reader, base + spec.offset, spec))

    positions = np.asarray(values["position"], dtype=np.float32)
    normals = _optional_array(values, "normal", np.float32)
    texcoords = _optional_array(values, "texcoord", np.float32)
    colors = _optional_array(values, "color", np.uint8)
    weights = _optional_array(values, "weights", np.float32)
    output_indices, output_name = _portable_indices(source_indices, primitive_name)
    return (
        DecodedPrimitive(
            GLTF_MODES[output_name],
            positions,
            np.asarray(output_indices, dtype=np.uint32),
            normals,
            texcoords,
            colors,
            weights,
            command_offset,
            vertex_format.raw,
            primitive_name,
        ),
        vertex_count * vertex_format.stride,
        index_bytes,
    )


def _read_source_indices(
    reader: Reader, address: int | None, index_type: int, count: int
) -> tuple[list[int], int]:
    if index_type == 0:
        return [], 0
    if address is None:
        raise ParseError("indexed VTYPE has no preceding IADDR", resource=reader.resource)
    size = 1 if index_type == 1 else 2
    raw = reader.require(address, count * size, "primitive indices")
    if size == 1:
        return list(raw), count
    return list(struct.unpack(f"<{count}H", raw)), count * 2


def _decode_attribute(reader: Reader, offset: int, spec: AttributeSpec) -> object:
    raw = reader.require(offset, spec.size, spec.kind)
    if spec.kind == "color":
        value = int.from_bytes(raw, "little")
        if spec.storage == "5650":
            return (_bits(value, 0, 5), _bits(value, 5, 6), _bits(value, 11, 5), 255)
        if spec.storage == "5551":
            return (
                _bits(value, 0, 5),
                _bits(value, 5, 5),
                _bits(value, 10, 5),
                255 if value >> 15 else 0,
            )
        if spec.storage == "4444":
            return tuple(_bits(value, shift, 4) for shift in (0, 4, 8, 12))
        return tuple(raw)

    if spec.storage == "f32":
        values = struct.unpack(f"<{spec.components}f", raw)
        if not all(math.isfinite(value) for value in values):
            raise ParseError(
                f"{spec.kind} contains NaN or Infinity", resource=reader.resource, offset=offset
            )
        return _texture_matrix(values) if spec.kind == "texcoord" else values
    signed = spec.kind in ("normal", "position")
    code = "b" if signed and spec.storage == "u8" else "B"
    if spec.storage == "u16":
        code = "h" if signed else "H"
    values = struct.unpack(f"<{spec.components}{code}", raw)
    divisor = 128.0 if spec.storage == "u8" else 32768.0
    if spec.kind == "color":
        return values
    normalized = tuple(value / divisor for value in values)
    return _texture_matrix(normalized) if spec.kind == "texcoord" else normalized


def _texture_matrix(values: tuple[float, ...]) -> tuple[float, ...]:
    # NGE2 model UVs use a fixed GE scale/offset. For u16 this simplifies to
    # (raw - 32768) / 4096, mapping the observed atlas range onto 0..1.
    return tuple(value * 8.0 - 8.0 for value in values)


def _bits(value: int, shift: int, width: int) -> int:
    maximum = (1 << width) - 1
    return ((value >> shift) & maximum) * 255 // maximum


def _optional_array(values: dict[str, list[object]], key: str, dtype: object) -> np.ndarray | None:
    return np.asarray(values[key], dtype=dtype) if key in values else None


def _portable_indices(indices: list[int], primitive: str) -> tuple[list[int], str]:
    if primitive == "triangle_strip":
        triangles: list[int] = []
        for index in range(len(indices) - 2):
            a, b, c = indices[index : index + 3]
            if index & 1:
                a, b = b, a
            if a != b and b != c and a != c:
                triangles.extend((a, b, c))
        return triangles, "triangles"
    if primitive == "triangle_fan":
        triangles = []
        for index in range(1, len(indices) - 1):
            a, b, c = indices[0], indices[index], indices[index + 1]
            if a != b and b != c and a != c:
                triangles.extend((a, b, c))
        return triangles, "triangles"
    return indices, primitive


def quantize_weights(weights: np.ndarray) -> np.ndarray:
    """Normalize each vertex and use largest remainders so every row totals 255."""
    if weights.ndim != 2 or not 1 <= weights.shape[1] <= 8:
        raise ValueError("weights must have shape (vertices, 1..8)")
    if not np.all(np.isfinite(weights)) or np.any(weights < 0):
        raise ValueError("weights must be finite and non-negative")
    totals = weights.sum(axis=1, keepdims=True)
    normalized = np.divide(weights, totals, out=np.zeros_like(weights), where=totals > 0)
    normalized[totals[:, 0] == 0, 0] = 1.0
    scaled = normalized * 255.0
    output = np.floor(scaled).astype(np.uint8)
    remainders = scaled - output
    missing = 255 - output.astype(np.uint16).sum(axis=1)
    for row, amount in enumerate(missing):
        order = np.argsort(-remainders[row], kind="stable")
        output[row, order[: int(amount)]] += 1
    return output
