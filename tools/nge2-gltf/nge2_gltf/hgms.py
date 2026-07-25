from __future__ import annotations

import math
from dataclasses import dataclass

from .binary import Reader, fourcc
from .errors import ParseError

HPT_RESOURCE_TYPE = 0x1000_0000


@dataclass(frozen=True)
class HgmsMaterial:
    raw: bytes
    texture_index: int


@dataclass(frozen=True)
class HgmsMesh:
    material_index: int
    unknown_01: int
    enabled_marker: int
    display_list_offset: int
    bone_palette: tuple[int, ...]
    offset: int


@dataclass(frozen=True)
class Hgms:
    material_count: int
    texture_count: int
    mesh_count: int
    bone_count: int
    unknown_08: int
    flags: int
    unknown_0c: int
    center: tuple[float, float, float]
    position_scale: float
    extra_offset: int
    bone_ids_offset: int
    texture_resource_keys: tuple[int, ...]
    materials: tuple[HgmsMaterial, ...]
    meshes: tuple[HgmsMesh, ...]
    bone_ids: tuple[int, ...]
    data: bytes

    @classmethod
    def parse(cls, data: bytes, *, resource: str = "model.hms") -> Hgms:
        reader = Reader(data, resource)
        if bytes(reader.require(0, 4, "HGMS signature")) != b"HGMS":
            raise ParseError("missing HGMS signature", resource=resource, offset=0)
        reader.require(0, 0x20, "HGMS header")
        material_count = reader.u8(4)
        texture_count = reader.u8(5)
        mesh_count = reader.u8(6)
        bone_count = reader.u8(7)
        center = (reader.f32(0x10), reader.f32(0x14), reader.f32(0x18))
        position_scale = reader.f32(0x1C)
        if not all(math.isfinite(value) for value in (*center, position_scale)):
            raise ParseError(
                "HGMS transform contains NaN or Infinity", resource=resource, offset=0x10
            )

        cursor = 0x20
        mesh_offsets = [
            reader.u32(cursor + index * 4, "mesh offset") for index in range(mesh_count)
        ]
        cursor += mesh_count * 4
        extra_offset = reader.u32(cursor, "extra offset")
        bone_ids_offset = reader.u32(cursor + 4, "bone IDs offset")
        cursor += 8
        texture_refs: list[int] = []
        for index in range(texture_count):
            raw = reader.u32(cursor + index * 4, "texture reference")
            texture_refs.append(HPT_RESOURCE_TYPE | (raw & 0x00FF_FFFF))
        cursor += texture_count * 4
        materials: list[HgmsMaterial] = []
        for index in range(material_count):
            raw = bytes(reader.require(cursor + index * 8, 8, "material"))
            texture_index = raw[4]
            if texture_index != 0xFF and texture_index >= texture_count:
                raise ParseError(
                    f"material {index} texture index {texture_index} is out of range",
                    resource=resource,
                    offset=cursor + index * 8 + 4,
                )
            materials.append(HgmsMaterial(raw, texture_index))

        bone_ids = tuple(
            reader.u32(bone_ids_offset + index * 4, "bone ID") for index in range(bone_count)
        )
        meshes: list[HgmsMesh] = []
        for index, offset in enumerate(mesh_offsets):
            reader.require(offset, 16, f"mesh {index} descriptor")
            material_index = reader.u8(offset)
            if material_index >= material_count:
                raise ParseError(
                    f"mesh material index {material_index} is out of range",
                    resource=resource,
                    offset=offset,
                )
            palette = tuple(reader.u8(offset + 8 + slot) for slot in range(8))
            for slot, bone_index in enumerate(palette):
                # Rigid HGMS resources have no bone table but leave palette slots zeroed.
                if bone_count and bone_index != 0xFF and bone_index >= bone_count:
                    raise ParseError(
                        f"bone palette slot {slot} refers to bone {bone_index}, "
                        f"but only {bone_count} exist",
                        resource=resource,
                        offset=offset + 8 + slot,
                    )
            display_list_offset = reader.u32(offset + 4, "display-list offset")
            reader.require(display_list_offset, 4, "display-list start")
            meshes.append(
                HgmsMesh(
                    material_index,
                    reader.u8(offset + 1),
                    reader.u16(offset + 2),
                    display_list_offset,
                    palette,
                    offset,
                )
            )
        return cls(
            material_count,
            texture_count,
            mesh_count,
            bone_count,
            reader.u16(8),
            reader.u16(0x0A),
            reader.u32(0x0C),
            center,
            position_scale,
            extra_offset,
            bone_ids_offset,
            tuple(texture_refs),
            tuple(materials),
            tuple(meshes),
            bone_ids,
            data,
        )

    def bone_names(self) -> tuple[str, ...]:
        return tuple(fourcc(value) for value in self.bone_ids)
