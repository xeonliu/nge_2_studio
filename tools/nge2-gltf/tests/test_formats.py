from __future__ import annotations

import numpy as np
import pytest

from nge2_gltf.ge import VertexFormat, quantize_weights
from nge2_gltf.hgar import HgarArchive
from nge2_gltf.hgms import Hgms
from nge2_gltf.hgob import Hgob
from nge2_gltf.hgpt import HgptImage
from nge2_gltf.transforms import inverse_bind_matrices, local_matrix

from .fixtures import make_hgar, make_hgms, make_hgob, make_hgpt


@pytest.mark.parametrize("version", [1, 3])
@pytest.mark.parametrize("compressed", [False, True])
def test_hgar_versions_decompression_and_typed_keys(version: int, compressed: bool) -> None:
    archive = HgarArchive.parse(make_hgar(version=version, compressed_hms=compressed))
    assert archive.version == version
    assert [entry.resource_key for entry in archive.entries] == [
        0x11000001,
        0x15000002,
        0x10000003,
    ]
    assert archive.entries[1].signature == b"HGMS"
    assert archive.entries[1].compressed is compressed


def test_hgob_properties_hierarchy_and_typed_hms_reference() -> None:
    hob = Hgob.parse(make_hgob())
    assert len(hob.nodes) == 2
    assert hob.nodes[1].parent_index == 0
    assert hob.nodes[0].children == [1]
    assert hob.nodes[1].hms_resource_key == 0x15000002
    assert hob.nodes[1].translation == (0.0, 0.0, 0.0)
    assert hob.nodes[1].unknown_properties[0]["dataHex"] == "aabb"


def test_hgob_null_shadow_model_sentinel_is_not_a_resource_reference() -> None:
    # Build a focused fixture because the regular fixture has an unknown property after 0x28.
    from .fixtures import _object

    node = _object(b"SHDW", b"MO", [(0x28, b"\xff\x0f\x00\x00")])
    hob = Hgob.parse(b"HGOB\x01\x00\x08\x00" + node)
    assert hob.nodes[0].hms_resource_key is None


def test_hgms_tables_material_bones_and_palette() -> None:
    hms = Hgms.parse(make_hgms())
    assert hms.texture_resource_keys == (0x10000003,)
    assert hms.bone_names() == ("ROOT",)
    assert hms.meshes[0].bone_palette == (0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF)
    assert hms.materials[0].raw == b"\0" * 8


@pytest.mark.parametrize("weight_count", range(1, 9))
def test_all_observed_weight_vtypes(weight_count: int) -> None:
    raw = 0x336 | ((weight_count - 1) << 14)
    decoded = VertexFormat.decode(raw, resource="fixture", offset=0)
    assert decoded.weight_count == weight_count
    assert decoded.attributes[0].kind == "weights"
    assert decoded.stride % 2 == 0


@pytest.mark.parametrize("format_code", [0x14, 0x13, 0x8800])
def test_hgpt_pixel_formats(format_code: int) -> None:
    image = HgptImage.parse(make_hgpt(format_code))
    assert image.format_code == format_code
    assert len(image.rgba) == image.width * image.height * 4
    assert image.rgba[3] == 255
    assert image.encode_png().startswith(b"\x89PNG\r\n\x1a\n")


def test_eight_weights_quantize_to_exact_byte_sum() -> None:
    source = np.array([[1, 2, 3, 4, 5, 6, 7, 8], [0, 0, 0, 0, 0, 0, 0, 0]], dtype=np.float32)
    result = quantize_weights(source)
    assert result.dtype == np.uint8
    assert result.sum(axis=1).tolist() == [255, 255]
    assert result[1].tolist() == [255, 0, 0, 0, 0, 0, 0, 0]


def test_inverse_bind_keeps_mesh_bind_transform() -> None:
    joint = local_matrix((3, 0, 0), (0, 0, 0, 1), (1, 1, 1))
    mesh = local_matrix((5, 0, 0), (0, 0, 0, 1), (1, 1, 1))
    inverse_bind = inverse_bind_matrices([joint], mesh, native=True)[0]
    np.testing.assert_allclose(joint @ inverse_bind, mesh, atol=1e-6)
