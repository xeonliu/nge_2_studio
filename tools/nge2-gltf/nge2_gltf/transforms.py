from __future__ import annotations

import numpy as np


def local_matrix(
    translation: tuple[float, float, float],
    rotation: tuple[float, float, float, float],
    scale: tuple[float, float, float],
) -> np.ndarray:
    x, y, z, w = rotation
    rotation_matrix = np.array(
        [
            [1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w), 0],
            [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w), 0],
            [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y), 0],
            [0, 0, 0, 1],
        ],
        dtype=np.float64,
    )
    result = rotation_matrix @ np.diag([*scale, 1.0])
    result[:3, 3] = translation
    return result


def world_matrices(nodes: tuple[object, ...]) -> list[np.ndarray]:
    output: list[np.ndarray | None] = [None] * len(nodes)

    def resolve(index: int) -> np.ndarray:
        existing = output[index]
        if existing is not None:
            return existing
        node = nodes[index]
        local = local_matrix(node.translation, node.rotation, node.scale)
        parent = node.parent_index
        result = resolve(parent) @ local if parent is not None else local
        output[index] = result
        return result

    for node_index in range(len(nodes)):
        resolve(node_index)
    return [matrix for matrix in output if matrix is not None]


def coordinate_matrix(native: bool) -> np.ndarray:
    # PSP fixed-function assets use a left-handed Y-up convention in the observed models.
    return np.eye(4, dtype=np.float64) if native else np.diag([1.0, 1.0, -1.0, 1.0])


def convert_matrix(matrix: np.ndarray, native: bool) -> np.ndarray:
    conversion = coordinate_matrix(native)
    return conversion @ matrix @ conversion


def transform_geometry(
    positions: np.ndarray,
    normals: np.ndarray | None,
    center: tuple[float, float, float],
    position_scale: float,
    native: bool,
) -> tuple[np.ndarray, np.ndarray | None]:
    source = positions.astype(np.float64) * (position_scale * 32768.0)
    source += np.asarray(center, dtype=np.float64)
    conversion = coordinate_matrix(native)[:3, :3]
    converted_positions = (source @ conversion.T).astype(np.float32)
    converted_normals = None
    if normals is not None:
        converted_normals = normals.astype(np.float64) @ conversion.T
        lengths = np.linalg.norm(converted_normals, axis=1, keepdims=True)
        converted_normals = np.divide(
            converted_normals,
            lengths,
            out=np.zeros_like(converted_normals),
            where=lengths > 1e-12,
        ).astype(np.float32)
    return converted_positions, converted_normals


def inverse_bind_matrices(
    joint_world: list[np.ndarray], mesh_world: np.ndarray, native: bool
) -> np.ndarray:
    matrices = []
    for joint in joint_world:
        inverse_bind = np.linalg.inv(joint) @ mesh_world
        matrices.append(convert_matrix(inverse_bind, native))
    return np.asarray(matrices, dtype=np.float32)
