from __future__ import annotations

import struct

from nge2_gltf.ge import decode_display_list

from .fixtures import command


def test_ge_base_offset_origin_and_index_address_state() -> None:
    data = bytearray(0x100)
    data[0:24] = (
        command(0x13, 0)
        + command(0x12, 0x900)  # i16 position + u8 indices
        + command(0x01, 0x80)
        + command(0x02, 0x70)
        + command(0x04, (3 << 16) | 3)
        + command(0x0B)
    )
    data[0x70:0x73] = bytes((0, 1, 2))
    struct.pack_into("<9h", data, 0x80, 0, 0, 0, 32767, 0, 0, 0, 32767, 0)
    primitives, warnings = decode_display_list(bytes(data), 0, resource="fixture")
    assert not warnings
    assert len(primitives) == 1
    assert primitives[0].indices.tolist() == [0, 1, 2]
    assert primitives[0].positions.shape == (3, 3)


def test_triangle_strip_alternates_winding_and_removes_degenerates() -> None:
    data = bytearray(0x100)
    data[0:16] = (
        command(0x12, 0x80) + command(0x01, 0x40) + command(0x04, (4 << 16) | 5) + command(0x0B)
    )
    # Vertex 1 and 2 intentionally share coordinates but are distinct indices; topology
    # degeneracy is index-based, matching the GE strip restart convention.
    struct.pack_into("<15b", data, 0x40, 0, 0, 0, 1, 0, 0, 1, 0, 0, 1, 1, 0, 2, 1, 0)
    primitive = decode_display_list(bytes(data), 0, resource="fixture")[0][0]
    assert primitive.indices.tolist() == [0, 1, 2, 2, 1, 3, 2, 3, 4]
    assert primitive.mode == 4
