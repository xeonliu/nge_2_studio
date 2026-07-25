# NGE2 glTF research converter

`nge2-gltf` converts the HGOB models in one NGE2 HGAR archive to glTF 2.0. It
is an independent Python 3.13 research tool and is not connected to the desktop
application.

## Usage

```bash
cd tools/nge2-gltf
uv sync
uv run python convert.py INPUT.har --output OUTPUT_DIR
```

The default output is one self-contained GLB for every HGOB:
`<hob-name>#id<decoded-id>.glb`. A conversion of one model is written
atomically; one failed model does not prevent other HGOB members from being
attempted.

Options:

- `--hob NAME_OR_ID` selects one HGOB by member name, decoded HGAR ID, or typed
  resource key such as `0x11201000`.
- `--format glb|gltf` selects self-contained GLB (the default) or a diagnostic
  directory containing `.gltf`, `model.bin`, and external PNG files.
- `--skip-unsupported` skips unsupported GE primitives or geometry commands.
  Without it, the current model fails on unsupported geometry.
- `--native-coordinates` retains source coordinates. The default converts the
  observed left-handed Y-up data to glTF right-handed Y-up by reflecting Z and
  reversing triangle winding.
- `--animation-har MOTION.har --hmn NAME_OR_ID` selects one HGMN from a
  separate archive and attaches its supported node tracks to each selected
  HGOB. A typed resource key is the safest selector when names are duplicated.
- `--animation-fps FPS` selects the engine update rate used to convert HGMN
  frame numbers to seconds; it defaults to 30. HGMN stores a relative time
  scale but no real-world update frequency.

For example, this exports one character with one motion:

```bash
uv run python convert.py misato00.har --output out \
  --hob misato00.hob \
  --animation-har motion.har --hmn 0x120042e0
```

The process exits with `0` when every selected model succeeds, `1` when one or
more models fail, and `2` when the archive or command line cannot be read.

## Report

`conversion-report.json` is always replaced atomically in the output directory.
It records the input and options, a success/failure summary, and one item per
model with:

- HGAR member name, typed resource key, decoded ID, output path, and status;
- node, mesh, primitive, vertex, triangle, bone, texture, animation, and
  animation-channel counts;
- warnings and errors with resource names and hexadecimal source offsets.

Unknown non-rendering HOB properties are preserved in node `extras` and
reported as warnings. Original HMS material bytes, resource keys, mesh/display
list offsets, VTYPE, bone palette, HPT format, and texture divisions are also
preserved in glTF `extras`.

## Supported data

- HGAR v1/v3 and bit-31 raw-DEFLATE members;
- HGOB hierarchy, defaults, TRS, model bindings, and unknown properties;
- HGMS mesh/material tables, bone IDs, and eight-slot palettes;
- GE `VADDR`, `IADDR`, `BASE`, `OFFSET_ADDR`, `ORIGIN`, `VTYPE`, `PRIM`,
  `RET`, and `END` state;
- 8-bit, 16-bit, and float texture coordinates, normals, positions, and
  weights; 5650, 5551, 4444, and 8888 colors; no/U8/U16 indices;
- points, lines, line strips, triangles, triangle strips, and triangle fans;
- one through eight skin influences using `JOINTS_0/WEIGHTS_0` and
  `JOINTS_1/WEIGHTS_1`;
- HGPT indexed-4, indexed-8, and RGBA8888 tiled textures;
- embedded PNG, vertex color, alpha selection, and `KHR_materials_unlit`.
- HGMN absolute/delta target tables, primary/event channel boundaries, float
  and quantized TRS tracks, and cubic translation tracks.

HGMN primary channels are matched to HGOB nodes by the shared FourCC object ID.
Quantized quaternions are normalized and sign-adjusted for shortest-path glTF
interpolation. Opcode `0x10` cubic translations store Bezier control points;
the exporter converts them to glTF CUBICSPLINE derivatives. Event channels and
unknown primary opcodes are preserved by the parser but omitted from glTF with
warnings.

Integer PSP attributes are decoded using the GE normalized fixed-point rule:
8-bit values use a divisor of 128 and 16-bit values use 32768. The HGMS
position transform is then `center + position * position_scale * 32768`.
NGE2's fixed model texture matrix is also applied as `uv = normalized * 8 - 8`
(for U16, `(raw - 32768) / 4096`). Normals are normalized after coordinate
conversion. Skin weights are normalized per vertex and quantized by largest
remainder so their bytes total exactly 255.

## Known limits

HGMN event/script execution, custom object-class motion opcodes, multiple
animations in one output, PSP sprites, Bezier/spline geometry, display-list
jumps/calls, morph targets, PBR material reconstruction, ISO input, and desktop
integration are not implemented. Duplicate typed keys are accepted only when
the requested HGOB/HGMS/HGPT candidates have identical data. PSP fixed-function
render bits are retained but not interpreted beyond texture selection and
transparency.

The default 30 FPS is an export-time assumption, not a field recovered from
HGMN. The executable advances 16.16 motion time by `32 * time_scale` on each
engine update; the common `time_scale == 2048` advances exactly one source
frame per update. Use `--animation-fps` when matching a captured game rate.

The default coordinate convention and UV orientation match the inspected
character and static-door samples, but `--native-coordinates` remains available
for reverse-engineering comparisons.

## Verification

```bash
uv run pytest
uv run ruff check .
```

The tests construct a game-data-free HGAR -> HGOB/HGMS/HGPT fixture and read
the exported GLB back with `pygltflib`. They also cover both HGMN target-offset
modes and an embedded TRS animation, both HGAR versions,
compression, typed references, hierarchy errors, all observed 1-8 weight
VTYPEs, GE addressing/topology, all HPT formats, exact weight quantization, and
inverse bind matrices.

The converter was additionally run in strict mode against the local
`ULJS00061` `shinji00`, `asuka00`, `rei00`, `misato00`, and `gendo00` character
archives, the `gmkdoor1` static model in `mapobj01.har`, and `ULJS00064`
`shinji00`. Generated game-derived files are intentionally not committed.

When Blender is installed, an exported GLB can also be inspected headlessly:

```bash
/Applications/Blender.app/Contents/MacOS/Blender \
  --background --factory-startup \
  --python scripts/blender_inspect.py -- MODEL.glb REVIEW_DIR
```

This writes a Blender-import report plus front, back, side, face, and torso PNG
renders without modifying the current Blender project. When an action is
present, it also renders the first, middle, and last action frames.
