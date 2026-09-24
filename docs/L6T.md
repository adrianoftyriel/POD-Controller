# .l6t tone files (GearBox 3.x)

GearBox saves tones (File > Tone 1/Tone 2/Dual Tone > Save As) and its
factory presets as `.l6t`. The format is a labelled version of the same
data the POD stores in an EffectDump, so it doubles as a map of the dump:
every field in a `.l6t` has a parameter ID, and most of those IDs appear in
the dump too. `tools/l6t/l6t.py` dumps the chunk tree.
`tools/l6t/sample-podre-8d.l6t` is 8D saved from GearBox, and it matches
the 8D dump in `PROTOCOL.md` field by field.

## Container

IFF, big-endian sizes, even-byte padding:

```
FORM L6P2
  HEAD                 4 bytes, 0
  LIST BANK
    BINF               bank info: u32, UTF-16BE name ("Dual" for a dual tone)
    FORM L6PA          one per tone (a dual tone has two)
      PINF             u32 ?, u16 ?, UTF-16BE tone name
      LIST PATC
        LIST MODL      one per block (tone settings, then each effect)
          MINF         12 bytes, see below
          PARM ...     12 bytes each: <id u32> <kind u32> <value u32/f32>
      LIST UNFO        metadata: IDAT (date string), IAMP (amp name),
                       IAPP ("GearBox"), IAPV ("3.72"), all UTF-16BE
```

`kind` is 1 for some floats, but many floats are stored with kind 0 and
their raw IEEE bits (e.g. `0x3E428F66` = 0.19), so read the value as raw
32 bits and interpret it by parameter.

## MINF: the block header

```
+0  category u8   00 amp, 01 cab, 02 effect, 3E tone settings
+1  table    u8   model table (02 delay, 03 mod, 04 reverb, ... see MODELS.md)
+2  model    u16
+4  group    u16  chain position: 02 pre-amp, 03 amp section, 05 post-amp
+6  slot     u16
+8  u16      0 so far
+10 enabled  u16
```

That's the dump's block record header (`PROTOCOL.md`) reordered and
big-endian: dump `model, table, category, slot, group, enabled`.

## Parameter IDs

A dump parameter record `<idx u16><namespace u16>` is this ID stored
little-endian: dump `01 00 10 3F` = file `3F100001`.

| ID range | Meaning |
|---|---|
| `3F10 00xx` | normal knobs (0-1) |
| `3F01 00xx` | mix knobs (0-1) |
| `3F00 00xx` | real-unit values (dB, gate, wah, pedal) |
| `3F20 00xx` | integer/tone-level settings |

The tone-settings block (`MINF 3E 70 ...`) holds the tone header fields,
plus some blocks that the dump keeps in the tone header instead of as
records:

| File ID / block | Dump tone header | Setting |
|---|---|---|
| `3F200001` | `0x38` | tempo (BPM) |
| `3F200006` | `0x54` | pedal assign |
| `3F200017` | `0x64` | unknown float (0.56-0.57 everywhere) |
| `3F200018` | `0x57` | footswitch |
| `3F20001E` | `0x55` | tone active (tone 2 = DUAL TONE) |
| `3F20001F` | `0x3C` | Studio/Direct mix |
| `3F200021` | `0x51` | input |
| `3F200022` | `0x56` | tweak block |
| `3F200023` | `0x58` | tweak parameter |
| `3F200024` | `0x36` | pedal control |
| `3F200025` | `0x60` | Tone 1+2 vol trim (dB) |
| block `02 0B` model 2, `3F100000` | `0x44` | room / ER (the dump stores its ID at `0x40`) |
| block `02 0B` model 12, `3F200009` / `3F20000A` | `0x28` / `0x29` | Variax model / tone |
| cab block `3F000000` | `0x52` | mic |
| mod/delay block `3F200000` | block record `+0x09` | tempo sync division |

Unmapped: `3F200016`, `20`, `26` and `27` (zero in every tone checked),
the two small `0B` blocks (models 13 and 14), and `3F200002`-`05`, `07`,
`12`-`15`, `19` and `1A`, which only appear in some factory presets.

The file tone-level IDs are **not** the USB `05`/`16` param numbers
(tempo is `3F200001` here but USB param `0x17`), except for vol trim,
which is `0x25` in both.

## Factory preset corpus

`tools/l6t/preset_models.json` lists, for every (category, table, model)
used by GearBox's 558 factory presets, how often it's used, the chain
positions it appears in, and the min/max of each parameter ID. That gives
the parameter layout of 240 models without having to load each one on
the unit. Knob names still come from the Gearbox panels (see the knob map
in `PROTOCOL.md`).
