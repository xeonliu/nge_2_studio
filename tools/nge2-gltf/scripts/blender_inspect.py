from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import bpy
from mathutils import Vector


def parse_args() -> argparse.Namespace:
    arguments = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    parser = argparse.ArgumentParser()
    parser.add_argument("input", type=Path)
    parser.add_argument("output", type=Path)
    return parser.parse_args(arguments)


def world_bounds(objects: list[bpy.types.Object]) -> tuple[Vector, Vector]:
    points = [obj.matrix_world @ Vector(corner) for obj in objects for corner in obj.bound_box]
    return (
        Vector(tuple(min(point[axis] for point in points) for axis in range(3))),
        Vector(tuple(max(point[axis] for point in points) for axis in range(3))),
    )


def look_at(camera: bpy.types.Object, target: Vector) -> None:
    camera.rotation_euler = (target - camera.location).to_track_quat("-Z", "Y").to_euler()


def render_view(
    output: Path,
    camera: bpy.types.Object,
    center: Vector,
    size: Vector,
    *,
    axis: str,
    target_z: float,
    scale: float,
    suffix: str,
) -> None:
    distance = max(size) * 3.0 + 1.0
    if axis == "front":
        camera.location = (center.x, center.y + distance, target_z)
    elif axis == "back":
        camera.location = (center.x, center.y - distance, target_z)
    elif axis == "left":
        camera.location = (center.x - distance, center.y, target_z)
    else:
        camera.location = (center.x + distance, center.y, target_z)
    look_at(camera, Vector((center.x, center.y, target_z)))
    camera.data.ortho_scale = max(scale, 0.05)
    bpy.context.scene.render.filepath = str(output / f"{suffix}.png")
    bpy.ops.render.render(write_still=True)


def material_report(material: bpy.types.Material) -> dict[str, object]:
    images: list[str] = []
    nodes: list[dict[str, object]] = []
    if material.use_nodes and material.node_tree:
        for node in material.node_tree.nodes:
            item: dict[str, object] = {"name": node.name, "type": node.bl_idname}
            if node.bl_idname == "ShaderNodeTexImage" and node.image:
                item["image"] = node.image.name
                item["size"] = list(node.image.size)
                images.append(node.image.name)
            nodes.append(item)
    return {
        "name": material.name,
        "blendMethod": material.surface_render_method,
        "images": images,
        "nodes": nodes,
    }


def main() -> None:
    args = parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    bpy.ops.import_scene.gltf(filepath=str(args.input))
    meshes = [obj for obj in bpy.context.scene.objects if obj.type == "MESH"]
    if not meshes:
        raise RuntimeError("GLB imported without mesh objects")

    minimum, maximum = world_bounds(meshes)
    center = (minimum + maximum) * 0.5
    size = maximum - minimum
    objects: list[dict[str, object]] = []
    for obj in meshes:
        obj_min, obj_max = world_bounds([obj])
        uv = obj.data.uv_layers.active
        uv_min = uv_max = None
        if uv and uv.data:
            coordinates = [item.uv for item in uv.data]
            uv_min = [min(value[axis] for value in coordinates) for axis in range(2)]
            uv_max = [max(value[axis] for value in coordinates) for axis in range(2)]
        objects.append(
            {
                "name": obj.name,
                "vertices": len(obj.data.vertices),
                "polygons": len(obj.data.polygons),
                "boundsMin": list(obj_min),
                "boundsMax": list(obj_max),
                "uvMin": uv_min,
                "uvMax": uv_max,
                "materials": [
                    slot.material.name if slot.material else None for slot in obj.material_slots
                ],
                "modifiers": [modifier.type for modifier in obj.modifiers],
            }
        )

    report = {
        "input": str(args.input),
        "boundsMin": list(minimum),
        "boundsMax": list(maximum),
        "size": list(size),
        "objects": objects,
        "materials": [material_report(material) for material in bpy.data.materials],
        "images": [
            {
                "name": image.name,
                "size": list(image.size),
                "colorspace": image.colorspace_settings.name,
            }
            for image in bpy.data.images
        ],
    }
    (args.output / "report.json").write_text(json.dumps(report, indent=2), encoding="utf-8")

    scene = bpy.context.scene
    scene.render.engine = "BLENDER_EEVEE"
    scene.render.resolution_x = 720
    scene.render.resolution_y = 960
    scene.render.resolution_percentage = 100
    scene.render.image_settings.file_format = "PNG"
    scene.render.film_transparent = False
    if scene.world is None:
        scene.world = bpy.data.worlds.new("InspectionWorld")
    scene.world.color = (0.055, 0.055, 0.055)
    scene.view_settings.look = "AgX - Medium High Contrast"

    camera_data = bpy.data.cameras.new("InspectionCamera")
    camera_data.type = "ORTHO"
    camera = bpy.data.objects.new("InspectionCamera", camera_data)
    scene.collection.objects.link(camera)
    scene.camera = camera

    full_scale = max(size.z * 1.12, size.x * 960 / 720 * 1.12)
    render_view(
        args.output,
        camera,
        center,
        size,
        axis="front",
        target_z=center.z,
        scale=full_scale,
        suffix="front",
    )
    render_view(
        args.output,
        camera,
        center,
        size,
        axis="back",
        target_z=center.z,
        scale=full_scale,
        suffix="back",
    )
    render_view(
        args.output,
        camera,
        center,
        size,
        axis="left",
        target_z=center.z,
        scale=full_scale,
        suffix="left",
    )

    head_z = maximum.z - size.z * 0.12
    head_scale = size.z * 0.33
    render_view(
        args.output,
        camera,
        center,
        size,
        axis="front",
        target_z=head_z,
        scale=head_scale,
        suffix="face-front",
    )
    render_view(
        args.output,
        camera,
        center,
        size,
        axis="back",
        target_z=head_z,
        scale=head_scale,
        suffix="face-back",
    )

    torso_z = minimum.z + size.z * 0.58
    render_view(
        args.output,
        camera,
        center,
        size,
        axis="front",
        target_z=torso_z,
        scale=size.z * 0.52,
        suffix="torso-front",
    )


if __name__ == "__main__":
    main()
