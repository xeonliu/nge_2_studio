from __future__ import annotations

import json
import warnings

import numpy as np
from pygltflib import GLTF2

from nge2_gltf.cli import main

from .fixtures import make_archive, make_hgar, make_hgms, make_hgob, make_hgpt


def test_minimal_archive_exports_embedded_skinned_glb(tmp_path) -> None:
    source = tmp_path / "fixture.har"
    output = tmp_path / "out"
    source.write_bytes(make_hgar(compressed_hms=True))
    assert main([str(source), "--output", str(output)]) == 0

    report = json.loads((output / "conversion-report.json").read_text())
    assert report["summary"] == {"succeeded": 1, "failed": 0}
    item = report["models"][0]
    assert item["stats"] == {
        "nodes": 2,
        "meshes": 1,
        "primitives": 1,
        "vertices": 3,
        "triangles": 1,
        "bones": 1,
        "textures": 1,
    }
    glb = output / item["output"]
    model = GLTF2().load_binary(glb)
    assert len(model.scenes) == 1
    assert len(model.nodes) == 2
    assert len(model.meshes) == 1
    assert len(model.skins) == 1
    primitive = model.meshes[0].primitives[0]
    assert primitive.attributes.POSITION is not None
    assert primitive.attributes.JOINTS_0 is not None
    assert primitive.attributes.WEIGHTS_0 is not None
    assert model.images[0].bufferView is not None
    assert model.materials[0].extensions["KHR_materials_unlit"] == {}
    assert model.extensionsUsed == ["KHR_materials_unlit"]
    assert len(model.binary_blob()) % 4 == 0

    weights_accessor = model.accessors[primitive.attributes.WEIGHTS_0]
    view = model.bufferViews[weights_accessor.bufferView]
    blob = model.binary_blob()
    start = (view.byteOffset or 0) + (weights_accessor.byteOffset or 0)
    weights = np.frombuffer(blob, dtype=np.uint8, count=weights_accessor.count * 4, offset=start)
    assert weights.reshape(-1, 4).sum(axis=1).tolist() == [255, 255, 255]

    uv_accessor = model.accessors[primitive.attributes.TEXCOORD_0]
    uv_view = model.bufferViews[uv_accessor.bufferView]
    uv_start = (uv_view.byteOffset or 0) + (uv_accessor.byteOffset or 0)
    uv = np.frombuffer(
        blob,
        dtype=np.float32,
        count=uv_accessor.count * 2,
        offset=uv_start,
    ).reshape(-1, 2)
    np.testing.assert_allclose(uv.min(axis=0), [0.0, 0.0])
    np.testing.assert_allclose(uv.max(axis=0), [1.0, 1.0])


def test_model_failure_does_not_prevent_other_hob_and_report_is_written(tmp_path) -> None:
    source = tmp_path / "fixture.har"
    output = tmp_path / "out"
    source.write_bytes(
        make_archive(
            [
                ("good.hob", 0x11000001, make_hgob(), False),
                ("bad.hob", 0x11000004, b"HGOB\x01\x00\xff\xff", False),
                ("model.hms", 0x15000002, make_hgms(), False),
                ("tex.hpt", 0x10000003, make_hgpt(), False),
            ]
        )
    )
    assert main([str(source), "--output", str(output)]) == 1
    report = json.loads((output / "conversion-report.json").read_text())
    assert report["summary"] == {"succeeded": 1, "failed": 1}
    assert len(list(output.glob("*.glb"))) == 1


def test_directory_gltf_writes_external_bin_and_png(tmp_path) -> None:
    source = tmp_path / "fixture.har"
    output = tmp_path / "out"
    source.write_bytes(make_hgar())
    assert main([str(source), "--output", str(output), "--format", "gltf"]) == 0
    report = json.loads((output / "conversion-report.json").read_text())
    gltf = output / report["models"][0]["output"]
    assert gltf.exists()
    assert (gltf.parent / "model.bin").exists()
    assert len(list(gltf.parent.glob("*.png"))) == 1
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", RuntimeWarning)
        GLTF2().load(gltf)
