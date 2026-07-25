from __future__ import annotations

import json
import os
import re
import tempfile
import warnings as python_warnings
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import numpy as np
from pygltflib import GLTF2

from .binary import align
from .errors import ConversionError, ParseError
from .ge import DecodedPrimitive, decode_display_list, quantize_weights
from .hgar import HgarArchive, HgarEntry
from .hgms import Hgms, HgmsMaterial, HgmsMesh
from .hgob import Hgob
from .hgpt import HgptImage
from .transforms import (
    convert_matrix,
    inverse_bind_matrices,
    transform_geometry,
    world_matrices,
)

ARRAY_BUFFER = 34962
ELEMENT_ARRAY_BUFFER = 34963
BYTE = 5120
UNSIGNED_BYTE = 5121
SHORT = 5122
UNSIGNED_SHORT = 5123
UNSIGNED_INT = 5125
FLOAT = 5126


@dataclass
class ModelStats:
    nodes: int = 0
    meshes: int = 0
    primitives: int = 0
    vertices: int = 0
    triangles: int = 0
    bones: int = 0
    textures: int = 0

    def as_dict(self) -> dict[str, int]:
        return {
            "nodes": self.nodes,
            "meshes": self.meshes,
            "primitives": self.primitives,
            "vertices": self.vertices,
            "triangles": self.triangles,
            "bones": self.bones,
            "textures": self.textures,
        }


@dataclass
class ExportResult:
    output_files: list[str]
    stats: ModelStats
    warnings: list[dict[str, Any]]


@dataclass
class _Builder:
    binary: bytearray = field(default_factory=bytearray)
    buffer_views: list[dict[str, Any]] = field(default_factory=list)
    accessors: list[dict[str, Any]] = field(default_factory=list)
    nodes: list[dict[str, Any]] = field(default_factory=list)
    meshes: list[dict[str, Any]] = field(default_factory=list)
    skins: list[dict[str, Any]] = field(default_factory=list)
    materials: list[dict[str, Any]] = field(default_factory=list)
    images: list[dict[str, Any]] = field(default_factory=list)
    textures: list[dict[str, Any]] = field(default_factory=list)
    image_payloads: list[tuple[str, bytes]] = field(default_factory=list)

    def add_bytes(self, payload: bytes, *, target: int | None = None) -> int:
        padding = align(len(self.binary), 4) - len(self.binary)
        self.binary.extend(b"\0" * padding)
        view: dict[str, Any] = {
            "buffer": 0,
            "byteOffset": len(self.binary),
            "byteLength": len(payload),
        }
        if target is not None:
            view["target"] = target
        index = len(self.buffer_views)
        self.buffer_views.append(view)
        self.binary.extend(payload)
        return index

    def add_accessor(
        self,
        array: np.ndarray,
        *,
        component_type: int,
        accessor_type: str,
        target: int,
        normalized: bool = False,
        include_bounds: bool = False,
    ) -> int:
        contiguous = np.ascontiguousarray(array)
        if contiguous.size and not np.all(np.isfinite(contiguous)):
            raise ParseError("generated accessor contains NaN or Infinity")
        view = self.add_bytes(contiguous.tobytes(), target=target)
        accessor: dict[str, Any] = {
            "bufferView": view,
            "byteOffset": 0,
            "componentType": component_type,
            "count": len(contiguous),
            "type": accessor_type,
        }
        if normalized:
            accessor["normalized"] = True
        if include_bounds and len(contiguous):
            values = contiguous if contiguous.ndim > 1 else contiguous[:, None]
            accessor["min"] = values.min(axis=0).tolist()
            accessor["max"] = values.max(axis=0).tolist()
        index = len(self.accessors)
        self.accessors.append(accessor)
        return index

    def document(self, scene_roots: list[int], *, glb: bool) -> dict[str, Any]:
        binary_length = align(len(self.binary), 4)
        document: dict[str, Any] = {
            "asset": {"version": "2.0", "generator": "nge2-gltf 0.1.0"},
            "extensionsUsed": ["KHR_materials_unlit"],
            "extensionsRequired": ["KHR_materials_unlit"],
            "scene": 0,
            "scenes": [{"nodes": scene_roots}],
            "nodes": self.nodes,
            "bufferViews": self.buffer_views,
            "accessors": self.accessors,
            "buffers": [{"byteLength": binary_length}],
            "materials": self.materials,
        }
        for key, value in (
            ("meshes", self.meshes),
            ("skins", self.skins),
            ("images", self.images),
            ("textures", self.textures),
        ):
            if value:
                document[key] = value
        if not glb:
            document["buffers"][0]["uri"] = "model.bin"
        return document


def export_hob(
    archive: HgarArchive,
    hob_entry: HgarEntry,
    output_path: Path,
    *,
    output_format: str,
    skip_unsupported: bool,
    native_coordinates: bool,
) -> ExportResult:
    if hob_entry.signature != b"HGOB":
        raise ParseError("selected member is not HGOB", resource=hob_entry.name, offset=0)
    resources = archive.resources_by_key()
    hob = Hgob.parse(hob_entry.data, resource=hob_entry.name)
    builder = _Builder()
    stats = ModelStats(nodes=len(hob.nodes))
    warnings: list[dict[str, Any]] = []
    source_world = world_matrices(hob.nodes)

    for node in hob.nodes:
        gltf_node: dict[str, Any] = {
            "name": node.name,
            "matrix": _matrix_values(convert_matrix(_local_for_node(node), native_coordinates)),
            "extras": {
                "nge2": {
                    "objectId": f"0x{node.object_id:08X}",
                    "class": node.class_id,
                    "sourceOffset": node.offset,
                    "unknownProperties": node.unknown_properties,
                }
            },
        }
        if node.children:
            gltf_node["children"] = list(node.children)
        builder.nodes.append(gltf_node)
        for prop in node.unknown_properties:
            warnings.append(
                {
                    "message": f"preserved unknown HOB property 0x{prop['opcode']:02X}",
                    "resource": hob_entry.name,
                    "offset": prop["offset"],
                    "offsetHex": f"0x{prop['offset']:X}",
                }
            )

    for node_index, node in enumerate(hob.nodes):
        if node.hms_resource_key is None:
            continue
        hms_entry = _resolve_resource(resources, node.hms_resource_key, b"HGMS", hob_entry.name)
        hgms = Hgms.parse(hms_entry.data, resource=hms_entry.name)
        _attach_hgms(
            builder,
            archive,
            resources,
            hob,
            source_world,
            node_index,
            hms_entry,
            hgms,
            stats,
            warnings,
            skip_unsupported=skip_unsupported,
            native_coordinates=native_coordinates,
            glb=output_format == "glb",
        )

    roots = [index for index, node in enumerate(hob.nodes) if node.parent_index is None]
    if not roots:
        raise ParseError("HGOB has no scene root", resource=hob_entry.name)
    _write_document(builder, roots, output_path, output_format)
    return ExportResult([str(output_path)], stats, warnings)


def _attach_hgms(
    builder: _Builder,
    archive: HgarArchive,
    resources: dict[int, tuple[HgarEntry, ...]],
    hob: Hgob,
    source_world: list[np.ndarray],
    node_index: int,
    hms_entry: HgarEntry,
    hgms: Hgms,
    stats: ModelStats,
    warnings: list[dict[str, Any]],
    *,
    skip_unsupported: bool,
    native_coordinates: bool,
    glb: bool,
) -> None:
    del archive
    texture_indices: list[int] = []
    texture_alpha: list[bool] = []
    for resource_key in hgms.texture_resource_keys:
        hpt_entry = _resolve_resource(resources, resource_key, b"HGPT", hms_entry.name)
        image = HgptImage.parse(hpt_entry.data, resource=hpt_entry.name)
        texture_indices.append(_add_image(builder, image, hpt_entry, glb=glb))
        texture_alpha.append(image.has_alpha)
        stats.textures += 1

    bone_node_indices: list[int] = []
    object_by_id = {node.object_id: index for index, node in enumerate(hob.nodes)}
    for bone_id in hgms.bone_ids:
        resolved = object_by_id.get(bone_id)
        if resolved is None:
            raise ParseError(
                f"HGMS bone 0x{bone_id:08X} is absent from owning HGOB", resource=hms_entry.name
            )
        bone_node_indices.append(resolved)

    primitive_documents: list[dict[str, Any]] = []
    has_skin = bool(hgms.bone_ids)
    identity_joint_index: int | None = None
    skin_index: int | None = None
    if has_skin:
        joints = list(bone_node_indices)
        if any(0xFF in mesh.bone_palette for mesh in hgms.meshes):
            joints.append(node_index)
            identity_joint_index = len(joints) - 1
        joint_world = [source_world[index] for index in joints]
        inverse_binds = inverse_bind_matrices(
            joint_world, source_world[node_index], native_coordinates
        )
        accessor = builder.add_accessor(
            inverse_binds.transpose(0, 2, 1),
            component_type=FLOAT,
            accessor_type="MAT4",
            target=ARRAY_BUFFER,
        )
        skin_index = len(builder.skins)
        skin: dict[str, Any] = {
            "name": f"{hms_entry.name} skin",
            "joints": joints,
            "inverseBindMatrices": accessor,
            "extras": {"nge2": {"boneIds": [f"0x{value:08X}" for value in hgms.bone_ids]}},
        }
        if bone_node_indices:
            skin["skeleton"] = bone_node_indices[0]
        builder.skins.append(skin)
        stats.bones += len(hgms.bone_ids)

    for mesh in hgms.meshes:
        if mesh.enabled_marker != 0xFFFF:
            warnings.append(
                {
                    "message": f"skipped disabled mesh marker 0x{mesh.enabled_marker:04X}",
                    "resource": hms_entry.name,
                    "offset": mesh.offset,
                    "offsetHex": f"0x{mesh.offset:X}",
                }
            )
            continue
        decoded, ge_warnings = decode_display_list(
            hgms.data,
            mesh.display_list_offset,
            resource=hms_entry.name,
            skip_unsupported=skip_unsupported,
        )
        warnings.extend({"message": message, "resource": hms_entry.name} for message in ge_warnings)
        for primitive in decoded:
            primitive_documents.append(
                _add_primitive(
                    builder,
                    primitive,
                    mesh,
                    hgms,
                    texture_indices,
                    texture_alpha,
                    identity_joint_index,
                    has_skin,
                    native_coordinates,
                )
            )
            stats.primitives += 1
            stats.vertices += len(primitive.positions)
            stats.triangles += primitive.triangle_count

    if primitive_documents:
        mesh_index = len(builder.meshes)
        builder.meshes.append(
            {
                "name": hms_entry.name,
                "primitives": primitive_documents,
                "extras": {
                    "nge2": {
                        "resourceKey": f"0x{hms_entry.resource_key:08X}",
                        "center": list(hgms.center),
                        "positionScale": hgms.position_scale,
                        "flags": hgms.flags,
                        "unknown08": hgms.unknown_08,
                        "unknown0c": hgms.unknown_0c,
                    }
                },
            }
        )
        builder.nodes[node_index]["mesh"] = mesh_index
        if skin_index is not None:
            builder.nodes[node_index]["skin"] = skin_index
        stats.meshes += 1


def _add_primitive(
    builder: _Builder,
    primitive: DecodedPrimitive,
    mesh: HgmsMesh,
    hgms: Hgms,
    texture_indices: list[int],
    texture_alpha: list[bool],
    identity_joint_index: int | None,
    has_skin: bool,
    native_coordinates: bool,
) -> dict[str, Any]:
    positions, normals = transform_geometry(
        primitive.positions,
        primitive.normals,
        hgms.center,
        hgms.position_scale,
        native_coordinates,
    )
    indices = primitive.indices.copy()
    if not native_coordinates and primitive.mode == 4:
        indices = indices.reshape(-1, 3)[:, [0, 2, 1]].reshape(-1)
    if len(indices) and int(indices.max()) >= len(positions):
        raise ParseError("generated primitive index is outside POSITION accessor")
    if primitive.mode == 4:
        triangles = indices.reshape(-1, 3)
        if np.any(
            (triangles[:, 0] == triangles[:, 1])
            | (triangles[:, 1] == triangles[:, 2])
            | (triangles[:, 0] == triangles[:, 2])
        ):
            raise ParseError("generated primitive contains a degenerate triangle")

    attributes: dict[str, int] = {
        "POSITION": builder.add_accessor(
            positions,
            component_type=FLOAT,
            accessor_type="VEC3",
            target=ARRAY_BUFFER,
            include_bounds=True,
        )
    }
    if normals is not None:
        lengths = np.linalg.norm(normals, axis=1)
        if np.any((lengths < 0.99) | (lengths > 1.01)):
            raise ParseError("generated normals are not near unit length")
        attributes["NORMAL"] = builder.add_accessor(
            normals, component_type=FLOAT, accessor_type="VEC3", target=ARRAY_BUFFER
        )
    if primitive.texcoords is not None:
        attributes["TEXCOORD_0"] = builder.add_accessor(
            primitive.texcoords.astype(np.float32),
            component_type=FLOAT,
            accessor_type="VEC2",
            target=ARRAY_BUFFER,
        )
    vertex_alpha = False
    if primitive.colors is not None:
        vertex_alpha = bool(np.any(primitive.colors[:, 3] != 255))
        attributes["COLOR_0"] = builder.add_accessor(
            primitive.colors,
            component_type=UNSIGNED_BYTE,
            accessor_type="VEC4",
            target=ARRAY_BUFFER,
            normalized=True,
        )
    if primitive.weights is not None:
        if not has_skin:
            raise ParseError("vertex weights exist but HGMS has no bones")
        quantized = quantize_weights(primitive.weights)
        mapped_joints = np.zeros_like(quantized, dtype=np.uint8)
        for slot in range(quantized.shape[1]):
            palette_value = mesh.bone_palette[slot]
            if palette_value == 0xFF:
                if identity_joint_index is None:
                    raise ParseError("identity bone palette slot has no skin joint")
                palette_value = identity_joint_index
            mapped_joints[:, slot] = palette_value
        if int(mapped_joints.max(initial=0)) > 255:
            raise ParseError("skin joint index exceeds UNSIGNED_BYTE")
        for group, start in enumerate((0, 4)):
            if start >= quantized.shape[1]:
                break
            end = min(start + 4, quantized.shape[1])
            weights = np.zeros((len(quantized), 4), dtype=np.uint8)
            joints = np.zeros((len(quantized), 4), dtype=np.uint8)
            weights[:, : end - start] = quantized[:, start:end]
            joints[:, : end - start] = mapped_joints[:, start:end]
            attributes[f"WEIGHTS_{group}"] = builder.add_accessor(
                weights,
                component_type=UNSIGNED_BYTE,
                accessor_type="VEC4",
                target=ARRAY_BUFFER,
                normalized=True,
            )
            attributes[f"JOINTS_{group}"] = builder.add_accessor(
                joints,
                component_type=UNSIGNED_BYTE,
                accessor_type="VEC4",
                target=ARRAY_BUFFER,
            )

    index_component = UNSIGNED_SHORT if len(positions) <= 65535 else UNSIGNED_INT
    index_array = indices.astype(np.uint16 if index_component == UNSIGNED_SHORT else np.uint32)
    index_accessor = builder.add_accessor(
        index_array,
        component_type=index_component,
        accessor_type="SCALAR",
        target=ELEMENT_ARRAY_BUFFER,
        include_bounds=True,
    )
    material = hgms.materials[mesh.material_index]
    texture_alpha_value = material.texture_index != 0xFF and texture_alpha[material.texture_index]
    material_index = _material(
        builder,
        material,
        texture_indices,
        blend=vertex_alpha or texture_alpha_value,
    )
    return {
        "attributes": attributes,
        "indices": index_accessor,
        "material": material_index,
        "mode": primitive.mode,
        "extras": {
            "nge2": {
                "meshOffset": mesh.offset,
                "displayListOffset": mesh.display_list_offset,
                "commandOffset": primitive.source_offset,
                "vtype": f"0x{primitive.vtype:06X}",
                "sourcePrimitive": primitive.source_primitive,
                "bonePalette": list(mesh.bone_palette),
                "materialRawHex": material.raw.hex(),
            }
        },
    }


def _material(
    builder: _Builder,
    source: HgmsMaterial,
    texture_indices: list[int],
    *,
    blend: bool,
) -> int:
    texture_index = source.texture_index
    key = (source.raw, blend)
    for index, material in enumerate(builder.materials):
        if material.get("extras", {}).get("_cacheKey") == repr(key):
            return index
    pbr: dict[str, Any] = {
        "baseColorFactor": [1.0, 1.0, 1.0, 1.0],
        "metallicFactor": 0.0,
        "roughnessFactor": 1.0,
    }
    if texture_index != 0xFF:
        pbr["baseColorTexture"] = {"index": texture_indices[texture_index]}
    document = {
        "name": f"material_{len(builder.materials)}",
        "pbrMetallicRoughness": pbr,
        "alphaMode": "BLEND" if blend else "OPAQUE",
        "doubleSided": True,
        "extensions": {"KHR_materials_unlit": {}},
        "extras": {"nge2": {"rawHex": source.raw.hex()}, "_cacheKey": repr(key)},
    }
    builder.materials.append(document)
    return len(builder.materials) - 1


def _add_image(builder: _Builder, image: HgptImage, entry: HgarEntry, *, glb: bool) -> int:
    png = image.encode_png()
    image_document: dict[str, Any] = {
        "name": entry.name,
        "extras": {
            "nge2": {
                "resourceKey": f"0x{entry.resource_key:08X}",
                "formatCode": f"0x{image.format_code:04X}",
                "divisions": [division.__dict__ for division in image.divisions],
            }
        },
    }
    if glb:
        image_document.update({"bufferView": builder.add_bytes(png), "mimeType": "image/png"})
    else:
        filename = f"texture_{len(builder.images):02d}_{_safe_stem(entry.name)}.png"
        image_document["uri"] = filename
        builder.image_payloads.append((filename, png))
    image_index = len(builder.images)
    builder.images.append(image_document)
    builder.textures.append({"source": image_index})
    return len(builder.textures) - 1


def _write_document(builder: _Builder, roots: list[int], output: Path, output_format: str) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    glb = output_format == "glb"
    document = builder.document(roots, glb=glb)
    # Cache-only implementation detail must not leak into glTF extras.
    for material in document.get("materials", []):
        material.get("extras", {}).pop("_cacheKey", None)
    binary = bytes(builder.binary) + b"\0" * (align(len(builder.binary), 4) - len(builder.binary))
    try:
        with python_warnings.catch_warnings():
            python_warnings.filterwarnings(
                "ignore",
                message=".*non-optional type.*",
                category=RuntimeWarning,
            )
            model = GLTF2.from_dict(document)
        model.set_binary_blob(binary)
    except Exception as error:
        raise ConversionError(f"failed to construct glTF: {error}") from error

    if glb:
        fd, temporary_name = tempfile.mkstemp(
            prefix=f".{output.name}.", suffix=".tmp", dir=output.parent
        )
        os.close(fd)
        temporary = Path(temporary_name)
        try:
            model.save_binary(temporary)
            with python_warnings.catch_warnings():
                python_warnings.filterwarnings(
                    "ignore",
                    message=".*non-optional type uri.*",
                    category=RuntimeWarning,
                )
                GLTF2().load_binary(temporary)
            os.replace(temporary, output)
        except Exception as error:
            temporary.unlink(missing_ok=True)
            raise ConversionError(f"failed to write or validate GLB: {error}") from error
        return

    temporary_dir = Path(tempfile.mkdtemp(prefix=f".{output.stem}.", dir=output.parent))
    try:
        temporary_gltf = temporary_dir / output.name
        (temporary_dir / "model.bin").write_bytes(binary)
        for filename, payload in builder.image_payloads:
            (temporary_dir / filename).write_bytes(payload)
        temporary_gltf.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")
        with python_warnings.catch_warnings():
            python_warnings.filterwarnings(
                "ignore",
                message=".*non-optional type (mimeType|bufferView).*",
                category=RuntimeWarning,
            )
            GLTF2().load(temporary_gltf)
        for source in temporary_dir.iterdir():
            os.replace(source, output.parent / source.name)
        temporary_dir.rmdir()
    except Exception as error:
        for child in temporary_dir.glob("*"):
            child.unlink(missing_ok=True)
        temporary_dir.rmdir()
        raise ConversionError(f"failed to write or validate glTF: {error}") from error


def _resolve_resource(
    resources: dict[int, tuple[HgarEntry, ...]], key: int, signature: bytes, owner: str
) -> HgarEntry:
    candidates = [entry for entry in resources.get(key, ()) if entry.signature == signature]
    if not candidates:
        raise ParseError(f"missing resource key 0x{key:08X} referenced by {owner}", resource=owner)
    first = candidates[0]
    if any(entry.data != first.data for entry in candidates[1:]):
        raise ParseError(
            f"resource key 0x{key:08X} has {len(candidates)} different "
            f"{signature.decode()} candidates",
            resource=owner,
        )
    return first


def _matrix_values(matrix: np.ndarray) -> list[float]:
    return matrix.T.astype(np.float32).reshape(-1).tolist()


def _local_for_node(node: object) -> np.ndarray:
    from .transforms import local_matrix

    return local_matrix(node.translation, node.rotation, node.scale)


def _safe_stem(name: str) -> str:
    stem = Path(name).stem
    return re.sub(r"[^A-Za-z0-9._-]+", "_", stem).strip("._") or "texture"
