# HGOB/HGMS model format notes

This document records the current reverse-engineering results for the NGE2
`.hob` and `.hms` model resources. The results are based on files observed in
the `ULJS00061` and `ULJS00064` releases and on the corresponding loader and
renderer paths in the game executable.

The names below distinguish confirmed behavior from provisional field names:

- **Confirmed** means that both sample data and executable behavior agree.
- **Inferred** means that the layout is stable across the samples, but the
  exact gameplay or rendering meaning is not yet fully identified.
- Fields named `unknown_*` must not be treated as padding. They may contain
  meaningful flags or counts.

## Asset composition

The game does not store a complete animated model in one file. A model is
assembled from several resource types:

| Extension | Signature | Responsibility | Closest glTF concept |
| --- | --- | --- | --- |
| `.hob` | `HGOB` | Object hierarchy, local transforms, model bindings | Scene and nodes |
| `.hms` | `HGMS` | Meshes, materials, texture references and skin palettes | Mesh, primitive and skin |
| `.hpt` | `HGPT` | PSP tiled/swizzled image and palette data | Image and texture |
| `.hmn` | `HGMN` | Animation data | Animation |

In particular, a `.hob` is not a vertex model. It owns the scene graph and
binds one or more `.hms` resources to model nodes. An animated character can
therefore be summarized as:

```text
HGOB object tree
  -> HGMS mesh resources
       -> HGPT textures
  -> HGMN animation resources
```

HGAR members may be compressed. A compressed member starts with a little-
endian `u32` decompressed size followed by a raw DEFLATE stream. The inner
`HGOB` or `HGMS` signature is visible only after decompression.

## HGOB object graph

### File header

The top-level layout is confirmed:

```c
struct HgobHeader {
    char magic[4];                    // "HGOB"
    uint16_t object_count;
    uint16_t object_offsets[object_count];
};
```

Offsets are relative to the start of the HGOB data. Each offset identifies an
object record. The use of 16-bit offsets also constrains one HGOB blob to less
than 64 KiB.

### Object records

The current object-record interpretation is:

```c
struct HgobObject {
    uint32_t object_id;               // FourCC, e.g. "PELV" or "HEAD"
    uint16_t class_id;                // two-character class tag
    uint16_t property_count;
    PropertyDesc properties[property_count];
    uint8_t property_payloads[];
};

struct PropertyDesc {
    uint8_t opcode;
    uint8_t size_flags;               // payload size is size_flags & 0x7f
};
```

Property payloads follow the descriptor table in descriptor order. The high
bit of `size_flags` is observed but its semantic meaning remains unknown.

Observed object classes include:

| Class | Current interpretation |
| --- | --- |
| `"  "` | Generic transform or bone node |
| `"MO"` | Model node which can bind an HGMS resource |
| `"DL"` | Draw/model link node |
| `"SP"` | Special-purpose node |

Representative samples contained 851 generic nodes, 141 `MO` nodes, 40 `DL`
nodes and one `SP` node. These counts describe the inspected sample set, not a
format limit.

### Confirmed properties

| Opcode | Payload | Meaning |
| --- | --- | --- |
| `0x04` | `int16_t[4]` | Local quaternion, components divided by `32000.0` |
| `0x09` | `uint32_t` | Parent object FourCC |
| `0x0c` | `float[3]` | Local translation |
| `0x0d` | `float[3]` | Local scale |
| `0x28` | resource reference | Model-class binding to an HGMS resource |
| `0x2e` | object/resource reference | `DL` link |

Opcodes `0x12` and `0x19` are also common and affect object or resource state,
but their exact names are not yet confirmed.

The parent property builds the same hierarchy used for named character bones.
For example, FourCC values such as `PELV`, `SPIN` and `HEAD` appear both as
HGOB object IDs and in the HGMS bone table.

## HGMS mesh data

### Header and variable tables

The confirmed and provisionally named header layout is:

```c
struct HgmsHeader {
    char magic[4];                    // "HGMS"
    uint8_t material_count;           // +0x04
    uint8_t texture_count;            // +0x05
    uint8_t mesh_count;               // +0x06
    uint8_t bone_count;               // +0x07
    uint16_t unknown_08;              // +0x08
    uint16_t flags;                   // +0x0a
    uint32_t unknown_0c;              // +0x0c
    float center_x;                   // +0x10
    float center_y;                   // +0x14
    float center_z;                   // +0x18
    float position_scale;             // +0x1c
    uint32_t mesh_offsets[mesh_count];// +0x20
    uint32_t extra_offset;
    uint32_t bone_ids_offset;
    uint32_t texture_refs[texture_count];
    Material materials[material_count];
};
```

`mesh_offsets` and both following offsets are relative to the beginning of the
HGMS data. `bone_ids_offset` points to an array of `bone_count` FourCC values:

```c
uint32_t bone_ids[bone_count];
```

At load time, the engine resolves each FourCC against the owning HGOB graph and
allocates both a bone-node list and one 4x4 original/bind matrix per bone.

`flags & 1` enables a model behavior path; `flags & 2` selects between two
related object states. The externally visible meanings of those bits remain
inferred rather than named.

### Mesh descriptor

Each mesh offset points to a 16-byte descriptor:

```c
struct HgmsMesh {
    uint8_t material_index;
    uint8_t unknown_01;
    uint16_t enabled_marker;           // observed as 0xffff
    uint32_t display_list_offset;
    uint8_t bone_palette[8];
};
```

The renderer skips descriptors whose `enabled_marker` is not `0xffff`.
`display_list_offset` is relative to the start of the HGMS resource.

`bone_palette` maps the eight local hardware-weight slots used by this mesh to
entries in the file-level `bone_ids` table. A value of `0xff` requests the
identity/model matrix rather than a resolved bone matrix. Before drawing, the
renderer submits all eight slots through `sceGuBoneMatrix`.

### Materials and texture references

Texture references are 32-bit HGAR resource identifiers fixed up by the
resource loader. Materials are eight bytes each:

```c
struct Material {
    uint8_t unknown_00[4];
    uint8_t texture_index;             // 0xff means no bound texture
    uint8_t unknown_05;
    uint8_t render_state_06;
    uint8_t unknown_07;
};
```

The names other than `texture_index` are provisional. The loader binds the HPT
selected by byte `+0x04` and copies byte `+0x06` into runtime material/texture
state. The remaining fixed-function lighting, alpha, blend and texture state
semantics still need to be mapped.

### Display lists and primitives

Geometry is stored as a PSP GE display list rather than as a conventional
standalone vertex and index table. The relevant command stream contains:

| GE opcode | Command | Purpose |
| --- | --- | --- |
| `0x01` | `VADDR` | Select vertex data address |
| `0x04` | `PRIM` | Draw a primitive and vertex count |
| `0x0b` | `END` | End the display list |
| `0x12` | `VTYPE` | Describe packed vertex attributes |

All primitives inspected so far use PSP primitive type `4`,
`GU_TRIANGLE_STRIP`. This is an observation, not a format restriction.

The common rigid vertex type is `VTYPE 0x00136`, containing:

- 16-bit texture coordinates
- 16-bit 5:5:5:1 vertex color
- 8-bit normals
- 16-bit positions

Observed skinned vertex types include:

| VTYPE | Weight count | Weight storage |
| --- | ---: | --- |
| `0x00336` | 1 | `uint8_t` |
| `0x04336` | 2 | `uint8_t[2]` |
| `0x08336` | 3 | `uint8_t[3]` |
| ... | ... | ... |
| `0x1c336` | 8 | `uint8_t[8]` |

The exact normalization divisor for weight bytes still requires validation.
An exporter should normalize the decoded weights again before writing glTF.

When bones are present, the runtime constructs a scale/translation matrix from
the HGMS header. The diagonal scale is `position_scale * 32768.0`, and the
translation is the header center. This matrix is combined with each matrix in
the mesh bone palette before the display list is executed. The final simplified
formula for exported positions must be validated against PSP fixed-point vertex
conversion rather than assuming that the `32768` multiplier is applied directly
to an integer position.

## Executable evidence

The following functions in the analyzed PSP executable anchor the current
interpretation. Addresses refer to the loaded IDA database:

| Address | Function role |
| --- | --- |
| `0x0886e198` | Complete HGOB loader and property dispatch |
| `0x0886cf5c` | Loads HOB resources in the `0x11xxxxxx` resource namespace |
| `0x088827a8` | Handles Model property opcode `0x28` |
| `0x088814a4` | Converts the reference to the `0x15xxxxxx` HMS namespace and loads it |
| `0x08881ed8` | Registers the `MO` Model object class |
| `0x08882d60` | Initializes HGMS counts, texture references and materials |
| `0x0887e468` | Resolves HGMS bone IDs to HGOB nodes and builds bind matrices |
| `0x08883ae4` | Iterates HGMS mesh descriptors |
| `0x08883cc8` | Builds the eight-slot bone palette and executes the display list |
| `0x08883a64` | Fixes up HGMS texture resource references |
| `0x0887db18` | Loads HGMN animation data |

The resource namespaces above are runtime identifier tags. They are not file
magic values; the actual inner signatures remain `HGOB` and `HGMS`.

## Comparison with interchange formats

| NGE2 representation | glTF/FBX representation |
| --- | --- |
| HGOB object record | Node |
| HGOB parent ID | Node hierarchy |
| HGOB translation/quaternion/scale | Node TRS |
| HGMS mesh descriptor/display list | Mesh primitive |
| HGMS material | Material |
| HGPT | Image and texture |
| HGMS bone ID table | Skin joints |
| Mesh `bone_palette` | Local-to-global joint mapping |
| Packed vertex weights | `WEIGHTS_n` |
| HGOB bind transforms | Source for inverse bind matrices |
| HGMN channels | Animation samplers and channels |

Unlike glTF or FBX, these files are optimized for direct execution on PSP
hardware:

- mesh data has already been compiled into GE commands;
- positions, normals, UVs and weights use packed fixed-point representations;
- materials describe PSP fixed-function state rather than a PBR model;
- triangle strips are used instead of a portable triangle index list;
- scene hierarchy and mesh data live in separate resources;
- references use HGAR identifiers and FourCC object IDs.

OBJ is only suitable as a debug target for static geometry. It cannot retain
the hierarchy, skin weights, bind matrices or animation represented by the
asset set.

## Proposed glTF 2.0 conversion

glTF 2.0/GLB is the preferred conversion target because it can retain scene
nodes, skins, textures and animation without requiring the proprietary FBX
SDK. A staged converter can be implemented as:

```text
HGAR extraction and DEFLATE
  -> parse HGOB node hierarchy and local TRS
  -> parse HGMS tables and GE display lists
  -> decode packed vertex attributes
  -> expand triangle strips into triangle lists
  -> decode HGPT to RGBA/PNG
  -> map bone palette slots to glTF JOINTS and WEIGHTS
  -> construct skin joints and inverseBindMatrices from HGOB bind transforms
  -> optionally decode HGMN into glTF animation channels
  -> emit GLB
```

Triangle-strip expansion must alternate winding and discard degenerate
triangles. Coordinate-system conversion must update positions, normals, node
transforms, inverse-bind matrices and triangle winding together.

Expected preservation levels are:

| Component | Expected result |
| --- | --- |
| Static positions and topology | High, after fixed-point validation |
| UVs, normals and vertex colors | High |
| HPT image content | High; decoding already exists |
| Hierarchy and skin weights | High, after bind-pose validation |
| HGMN animation | Likely feasible; format work remains |
| PSP material/effect parity | Partial; fixed-function effects need approximation |

## Open questions

The following items should be resolved before claiming a production-quality
converter:

1. Name and validate HGMS fields at `+0x08`, `+0x0a` and `+0x0c`.
2. Identify all eight material bytes and their fixed-function state semantics.
3. Validate position, normal, UV and weight normalization rules against the GE.
4. Determine source handedness, up axis, triangle winding and model units.
5. Check for indexed, non-strip or less common VTYPE variants in more archives.
6. Confirm bind-pose and inverse-bind-matrix conventions with a skinned export.
7. Reverse all HGMN track encodings and interpolation modes.
8. Identify the exact meanings of HGOB opcodes `0x12` and `0x19`.

Until these questions are resolved, readers and parsers should preserve unknown
fields and unsupported display-list commands rather than silently discarding
them.
