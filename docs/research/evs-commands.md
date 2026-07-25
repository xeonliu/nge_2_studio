# EVS command research

This note records command semantics recovered from the `ULJS00064` executable
at `PSP_GAME/SYSDIR/BOOT.BIN`. Addresses use image base `0x08804000`. The
executable contains the source-file marker `../game/evseq.c`.

## Runtime structure

- `sub_0883F8A4` loads the event archive and EVS member, verifies the `.EVS`
  signature, stores the command count and offset table in the event context at
  `off_089F177C`, and resets the command index to zero.
- `sub_08840400` returns the current record as
  `evs_base + offset_table[command_index]`.
- `sub_08840150` reads the record's first `u16` as the opcode and invokes the
  dispatcher until a handler yields or the event stops.
- `sub_08842FA0` accepts opcodes below `0xB8` and indexes the table at
  `0x089BAFB0`. Each eight-byte entry is `{execution_mask, handler_address}`.
- EVS records are `{u16 opcode, u16 payload_size, u8 payload[payload_size]}`.
  Handler wrappers read 32-bit parameters from offsets `+4`, `+8`, and so on;
  content strings follow those parameters.

The parser has a parameter layout for every dispatched opcode from `0x01`
through `0xB7`. The layouts added directly from handler reads during this pass
were:

| Opcode | u32 parameters | Opcode | u32 parameters |
| --- | ---: | --- | ---: |
| `0x03` | 1 | `0x0B` | 1 |
| `0x1B` | 2 | `0x1D` | 2 |
| `0x25` | 3 | `0x3C` | 2 |
| `0x66` | 1 | `0x82..0x84` | 1 |
| `0x88` | 7 | `0x89` | 2 |
| `0x8A` | 6 | `0x8B` | 0 |
| `0x96` | 0 | `0xB1` | 2 |
| `0xB4` | 3 | `0xB6` | 2 |

This confirms record boundaries and safe parsing. It does not by itself prove
the game-state meaning of every command, so unresolved opcodes deliberately
retain the neutral `COMMAND` name and `state` category.

## Confirmed semantics

| Opcode | Runtime handler | Meaning and payload |
| --- | --- | --- |
| `0x01` | `sub_0884301C` | Dialogue. Three u32 values control speaker/avatar flags, expression flags, and the first voice ID; Shift-JIS text follows. |
| `0x02..0x0A` | `sub_08843054` onward | Dialogue block filters and sequences. They scan following commands by current/valid character state; `0x09` selects a valid entry randomly. |
| `0x14` | `sub_08843450` | Signed relative jump of the command index. |
| `0x15..0x56` | `sub_08843488` onward | Conditional branches over character, event, or game state. Branch destinations are relative command offsets. |
| `0x7B` | `sub_0884690C` | Stop the event with the default result. |
| `0x7D..0x86` | `sub_08846AF0` onward | Populate the event return structure, set the stop flag, and leave the script. |
| `0x87` | `sub_08846F2C` | Dispatch a game-specific extension ID. Observed cases include tutorial HUD, menus, and save operations. |
| `0x8C` | `sub_088471BC` | Set or clear the background layer. Reads one transition-flags u32 and a resource name. |
| `0x8D` | `sub_088471E8` | Set or clear the picture/CG layer. Reads one transition-flags u32 and a resource name. |
| `0x8E` | `sub_08847214` | Set or clear the telop overlay. Reads one display-flags u32 and a resource name. |
| `0x8F` | `sub_08847240` | Configure an event-specific visual effect from one mode value. |
| `0x90` | `sub_08847264` | Wait in milliseconds. The handler converts with `60 * value / 1000` before scheduling frames. |
| `0x91` | `sub_088472C0` | Stop a sound effect. The downstream function treats `-1` as stop all tracked effects. |
| `0x92` | `sub_088472E4` | Play a sound effect ID and track its playback handle. |
| `0x93` | `sub_08847308` | Play a music ID; non-positive values stop current music. |
| `0x94` | `sub_088473B4` | Set music volume. Downstream code also recognizes preset values `0x8000` and `0x8001`. |
| `0x95` | `sub_088473F0` | Show a choice menu. No numeric parameters; Shift-JIS text starts at record `+4`. `sub_0883EFB8` splits it on `0x815E` (full-width slash `／`) into at most four 32-byte options. |
| `0xA3` | `sub_088474E4` | Resource hint. The current runtime handler only increments the command index and never reads its payload. Samples contain names such as `end_se03.bin`. |

The command model exposes this evidence as a category, description, known
parameter names, and parsed choice options. The UI consumes those fields rather
than maintaining a separate opcode classification table.

## Sound effect lookup and bank format

The `0x92` call chain is `sub_088472E4 -> sub_08842ED0 -> sub_088496A8 ->
sub_08849738 -> sub_0880918C`. `sub_088496A8` maps script sound IDs through a
fixed system table (`1000..1037`), a small generic event table (`2000..2012` in
supported event modes), or one of the `_SidToSe*` event tables. The packed value
has this layout:

- bits `16..23`: logical bank ID;
- bits `8..14`: program;
- bits `0..6`: MIDI note;
- bit `15`: request a tracked playback handle;
- low-byte bit `7`: special BGM-slot path, not a normal sound-effect preview.

`sub_0880996C` maps the logical bank to a loaded physical slot. Slot 0 is
`/PSP_GAME/USRDIR/sound/sys_se01.bin`. Slot 1 first uses a playable `.bin`
member in the current EVS segment, then the nearest playable bank earlier in
the same HGAR. If neither exists, `sub_08971398` confirms the global fallback
`/PSP_GAME/USRDIR/sound/esg_se01.bin` is loaded into slot 1. Slots 2 and 3
depend on live game mode and may refer to EVA, map, or situation banks, so the
studio reports that missing context instead of selecting a file heuristically.
The same applies to base scripts such as `f000.evs` whose slot 1 bank is loaded
by the caller before the EVS starts; the archive alone cannot identify it.

The bank is a 32-byte member table containing `.phd` and `.pbd` files. The
`.phd` `PPHD` header points to `PPPG` program, `PPTN` tone, and `PPVA` wave
tables. Program offsets are relative to the `PPHD` base; tones select waves by
note range; each wave gives a `.pbd` offset, sample rate, and byte size. `.pbd`
payloads use 16-byte PSX ADPCM frames. This structure follows the runtime
lookups at `sub_08994B34`, `sub_08994A94`, and `sub_08994A14`.

The preview decodes PSX ADPCM to mono PCM WAV and mixes matching tone layers.
Tone volume and pan are not yet applied, so layered effects are an approximate
audition rather than a bit-exact emulation of the PSP audio engine.

## Remaining work

- Name individual conditional and state opcodes only after tracing their
  downstream state fields and validating them against multiple scripts.
- Decode the `0x87` extension ID table into stable names and parameter schemas.
- Determine the exact bit layout of dialogue avatar/expression flags and visual
  transition flags.
- Compare other regional executables before treating handler addresses or
  extension IDs as cross-version constants.
- Recover tone volume, pan, and pitch fields for closer sound-effect rendering.
