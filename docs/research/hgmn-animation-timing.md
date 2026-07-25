# HGMN animation timing in the game runtime

This note separates three values which are easy to confuse when exporting an
HGMN animation:

1. the source-frame number stored in a channel key;
2. the per-target `time_scale` stored in HGMN;
3. the rate at which the game executes scene-object updates.

The short conclusion is that HGMN does not contain an FPS value. It says how
far animation time advances on each logic update. The executable normally runs
one logic update per rendered frame and selects an approximately 30 Hz or 60 Hz
render cadence with a VBlank-based mode.

For the common `time_scale == 2048` and playback multiplier `1.0`, one logic
update advances exactly one HGMN source frame. This is about 30 source frames
per second in the main 30 Hz game mode, but about 60 source frames per second if
the same motion remains active in a 60 Hz scene.

## HGMN timing fields

Each HGMN target block begins with:

```c
struct HgmnTarget {
    uint32_t object_id;
    uint16_t duration;       // duration in source-frame units
    int16_t time_scale;      // base advance rate, not FPS
    uint8_t primary_count;
    uint8_t event_count;
};
```

The loader at `0x0887db18` copies `duration` and `time_scale` into the loaded
target block. Channel key times are signed 16-bit source-frame numbers. The
runtime evaluates them against a signed 16.16 fixed-point motion clock.

`time_scale` is per target. Real character motions commonly use `2048`, whose
meaning follows from the update arithmetic below; it must not be interpreted
as 2048 FPS or milliseconds.

## Runtime state

The motion state used by `0x0887b418` contains these confirmed fields:

| Offset | Type | Meaning |
| --- | --- | --- |
| `+0x80` | `uint16_t` | transition accumulator |
| `+0x82` | `uint16_t` | transition step |
| `+0x84` | `int16_t` | effective playback scale |
| `+0x88` | `int32_t` | current motion time, signed 16.16 |
| `+0x8a` | `int16_t` | integer-frame high half of the same 16.16 clock |
| `+0x90` | pointer | loaded target block |

The `+0x8a` access overlaps the 32-bit field at `+0x88`; it is not a second
independent clock. Within the target block, `+0x14` is the duration and `+0x16`
is the original file `time_scale`.

During ordinary playback the update function executes the following operation:

```c
motion_time_16_16 += ((int32_t)effective_scale << 5);
```

The corresponding MIPS sequence loads the signed halfword at `+0x84`, shifts
it left by five, and adds it to the 32-bit clock at `+0x88`. Therefore:

```text
source_frames_per_logic_update = effective_scale / 2048
```

For example:

| Effective scale | Advance per logic update |
| ---: | ---: |
| `2048` | `+1.0` source frame |
| `1024` | `+0.5` source frame |
| `0` | no advance |
| `-2048` | `-1.0` source frame |

The function has no elapsed-time or `deltaTime` parameter. Animation time is
therefore tied to the number of logic updates, not directly to a microsecond
clock.

## File speed and runtime multiplier

`0x0887c31c` applies a floating-point multiplier to the original HGMN value:

```c
effective_scale = (int16_t)((float)file_time_scale * multiplier);
```

The conversion truncates to a signed 16-bit integer. The same multiplier is
propagated to linked child objects. `0x0887c2e4` implements the inverse query:

```c
multiplier = (float)effective_scale / (float)file_time_scale;
```

Observed callers establish the intended range of behavior:

- `1.0` restores the file speed;
- `0.5` is used for half-speed playback (`0x0885f898`, `0x0885f9bc`);
- `0.0` pauses the motion clock without disabling the object update
  (`0x088c44dc` also seeks to zero);
- a negative value plays backward (`0x088c4284` passes a negated stored speed).

This per-motion pause is separate from the debug facility that can suppress
all scene logic updates.

## From the main loop to HGMN

The normal dispatch chain is:

```text
GameLoop (0x08819b88)
  -> GameLoop_GetUpdateCount (0x08819c70)
  -> SceneObjects_Update (0x0886cf1c)
  -> object callback dispatch (0x0886dc68)
  -> HGMN object walker (0x0886dea4)
  -> motion evaluator (0x0887b418)
```

The HGMN object walker is installed as a scene-object callback by
`0x0886c108`. Its scheduling byte is set to `128`; this value determines its
position in the scene-object callback list. It is not a frequency or an FPS.

In ordinary operation, `GameLoop_GetUpdateCount` returns `1`, so the scene
object list and HGMN evaluator run once per rendered frame. It does not perform
automatic catch-up updates after a slow frame.

There is also an executable debug mode. While its global pause is active the
update count can be `0`, `1`, or `20`, selected by controller masks. The `20`
path deliberately executes twenty complete logic updates before one render and
therefore advances a default HGMN motion by twenty source frames. These paths
are debugging controls, not HGMN timing metadata.

## 30 Hz and 60 Hz frame-rate modes

`SetFrameRateMode` at `0x08819b54` writes the global frame-rate mode:

| Mode | VBlank behavior | Nominal render/logic rate | `g_TimeStep` |
| ---: | --- | ---: | ---: |
| `0` | one VBlank wait | about 60 Hz | `4096.0` |
| nonzero | normally two VBlank waits | about 30 Hz | `2048.0` |

`Update_WaitVblank_FPSLimit` at `0x088044b8` always calls
`sceDisplayWaitVblankStartCB()` once. In nonzero mode it waits for an additional
VBlank when the display counter has not advanced since the preceding present.
If rendering has already crossed a VBlank, it omits that extra wait. This is a
frame limiter, not a fixed-step catch-up loop.

The main scene/overlay entry at `0x08884af4` explicitly selects mode `1`, which
is the evidence for treating approximately 30 updates per second as the normal
character-gameplay rate. Other code explicitly changes modes: for example, the
title-screen path at `0x0881c24c` saves the old value, selects mode `0`, and
restores the old value on exit. Several special-scene wrappers similarly force
one mode temporarily.

Consequently there is no single executable-wide HGMN FPS. Wall-clock motion
speed is:

```text
source_frames_per_second
    = updates_per_second * file_time_scale * multiplier / 2048
```

The separate `g_TimeStep` value is changed with the frame-rate mode, but the
confirmed HGMN advance at `0x0887b418` does not read it. Do not multiply HGMN
key times by `g_TimeStep` in an exporter.

## Seeking, boundaries, and reverse playback

The current time API also uses 16.16 source-frame units:

- `0x0887c168` returns the current 16.16 time;
- `0x0887c2c8` returns the target duration;
- `0x0887b794` sets an absolute 16.16 time and repairs the cached-frame state;
- `0x0887bc68` resolves an opcode-`0x13` marker index to `frame << 16`;
- `0x0887bc24` seeks to that marker time.

`0x0887b930` sets runtime flag bit 0 to enable playback, while `0x0887ba00`
clears it. At the end boundaries, `0x0887b418` branches on runtime flags and
marker state to loop, clamp, stop, or queue an object state change. Reverse
looping is confirmed by adding `duration << 16` after the time becomes
negative.

Bit 3 participates in the loop-versus-clamp decision, and bit 16 is tested in
the same boundary path. Their complete semantic names are not yet established,
so they should remain described by observed behavior rather than speculative
enum names.

## Transition timing

Pose transitions initialized by `0x0887b274`, `0x0887bd84`, and `0x0887bf80`
use a second update-count clock:

```c
transition_step = 0x10000 / transition_updates;
transition_accumulator =
    -transition_step * (transition_updates - 1);
```

While `transition_step != 0`, the normal motion-time advance is not taken.
Instead, each logic update advances the accumulator and derives blend alpha as
`transition_accumulator / 65536`. Translation is blended linearly and rotation
uses quaternion blending. The transition ends after the accumulator passes
`0xffff`.

Thus `transition_updates` is a count of scene logic updates, not milliseconds.
A 15-update transition is about 0.5 seconds in the 30 Hz mode and about 0.25
seconds in the 60 Hz mode.

## glTF export conversion

glTF key times are seconds. Given an explicitly selected logic update rate:

```text
effective_scale = file_time_scale * multiplier
source_frames_per_update = effective_scale / 2048
seconds = source_frame / source_frames_per_second
```

For an exported track at the default runtime multiplier `1.0`:

```text
seconds = source_frame * 2048
        / (updates_per_second * file_time_scale)
```

This is the formula used by `nge2-gltf` through `--animation-fps`. Its default
of `30` matches the confirmed main 30 Hz scene mode and common character-motion
usage. Use `--animation-fps 60` when reproducing a motion observed in one of the
60 Hz paths. A capture of the scene or its caller's `SetFrameRateMode` is the
deciding evidence; HGMN alone cannot decide it.

## Address index

| Address | Confirmed role |
| --- | --- |
| `0x088044b8` | VBlank wait and 30/60 frame limiter |
| `0x08819b54` | set frame-rate mode and `g_TimeStep` |
| `0x08819b88` | main render/logic loop |
| `0x08819c70` | choose logic-update count (`1` normally) |
| `0x0886c108` | install HGMN scene-object callback at priority `128` |
| `0x0886cf1c` | update scene-object callbacks |
| `0x0886dc68` | invoke registered object callbacks |
| `0x0886dea4` | visit active animated objects |
| `0x0887b418` | evaluate transition or advance/evaluate motion |
| `0x0887b794` | seek to absolute 16.16 motion time |
| `0x0887bc24` | seek to an opcode-`0x13` marker |
| `0x0887c2e4` | get playback multiplier |
| `0x0887c31c` | set playback multiplier |
| `0x0887db18` | load HGMN targets and channels |
| `0x08884af4` | select 30 Hz mode for the main scene/overlay path |

## Confidence and remaining questions

The fixed-point advance, multiplier, callback chain, normal update count, and
VBlank mode behavior are direct executable observations. The approximate 30/60
rates additionally rely on the PSP display's VBlank cadence; the code itself
counts VBlanks and never stores a literal `30.0` or `60.0` beside HGMN.

Still unresolved are the full names of the motion boundary flag bits and a
complete map of which game states force each global frame-rate mode. Neither
unknown changes the exporter formula or the conclusion that wall-clock FPS is
external to HGMN.
