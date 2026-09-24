# POD X3 USB Protocol — known facts, sources, and open questions

This document tracks everything currently known about the POD X3 / X3 Live
USB protocol. It exists so reverse-engineering effort isn't duplicated or
lost. **Every claim here is sourced** — update this file as new facts are
confirmed against real hardware, and mark speculation clearly as such.

## Device identification

| Field | Value | Source |
|---|---|---|
| USB Vendor ID | `0x0E41` (Line6) | andree182/podx3 |
| Product ID — POD X3 (rack/desktop) | `0x414A` | andree182/podx3 |
| Product ID — POD X3 Live | `0x414B` | andree182/podx3, confirmed via `lsusb` |
| Device/interface class | `0xFF` (vendor-specific) | andree182/podx3 — **not** USB Audio class, **not** USB MIDI class |

Line6's own KB lists POD X3 as USB 2.0 but explicitly *not* class-compliant.
This means the OS will not surface it as a standard audio or MIDI device —
no off-the-shelf MIDI/audio library can talk to it. It requires either
Line6's proprietary driver (for audio, via ASIO/CoreAudio integration) or
raw USB access (libusb/WinUSB/IOKit-style) for the control protocol we care
about here.

## USB interfaces

- **Interface 0** (alt setting 1): isochronous audio streaming.
  8 channels in, 2 channels out, S24_3LE. Not needed for patch/effect
  control — out of scope for this project unless we later add audio
  routing features.
- **Interface 1**: the control channel we care about.
  - Bulk IN endpoint: `0x81`
  - Bulk OUT endpoint: `0x01`
  - Max packet size: 64 bytes (`0x40`)

## Control handshake (before bulk I/O starts)

Uses standard USB **control transfers**, not the bulk endpoints:

- `bRequest = 0x67` (named `L6_X3_CTRL` in prior art)
- `bmRequestType = 0x40` (host→device) or `0xC0` (device→host)
- Used to read serial number and firmware version, and to send an
  "INIT" sequence of 8-byte reads at `wValue` `0xF000`–`0xF080`. **Purpose
  of the INIT sequence is unconfirmed** — device responds, but the
  significance of the returned bytes is unknown. Treat as "send it because
  the only known-working implementation does," not as understood protocol.

After this handshake, normal operation moves to the bulk endpoints.

## Bulk transfer framing

Every bulk packet (max 64 bytes) starts with a 4-byte header:

```
byte 0: ContentsLength
byte 1: ?? (unconfirmed)
byte 2: Flags — 0x01 = first packet of a message, 0x04 = continuation
byte 3: ?? (unconfirmed)
```

Multi-packet messages are reassembled by concatenating payloads across
packets until `ContentsLength` worth of data has been collected. (Reference
implementation: `PacketCompleter` in andree182/podx3.)

## Message types (payload byte 0, after the 4-byte framing header)

| Type | Meaning | Confirmed working? |
|---|---|---|
| `0x01` | **EffectDump** — full patch/effect-chain blob, ~4096 bytes | Partially — dump can be requested/received; internal byte layout is **not decoded** |
| `0x02` | **ConfigCmd** — includes "Request EffectDump(i)" / "Send EffectDump(i, data)", i.e. read/write a patch by slot index | Request/send framing known; full semantics unconfirmed |
| `0x04` / `0x05` | Int parameter get/set (12/16-byte int tuples) | Some cases confirmed |
| `0x06` | **Float parameter set** — 20-byte struct, trailing 4 bytes = little-endian IEEE-754 float, parameter index near byte 0x18 | **Confirmed working** (example below) |

### Confirmed example: float parameter set (e.g. tone volume)

```
1C 00 01 00 06 00 0A 40 01 03 00 15 01 00 00 00 00 00 03 00 01 00 00 00 05 00 10 3F <4-byte float LE>
```

Value range for this parameter is `0x00000000`–`0x3F800000` = `0.0`–`1.0`.
**Other parameters' index values and ranges are not yet catalogued** — this
is a concrete, tractable reverse-engineering task: enumerate parameter
indices by trial against real hardware and record them here as found.

## Confirmed from real Gearbox traffic (2026-09-24)

Captured with `tools/vm-capture/` (Gearbox on a Windows 7 VM, POD X3 Live
passed through by device, host-side `usbmon`). Each finding comes from a
single scripted UI action with its own capture in `captures/`. Offsets
below are into the **message** (after the 4-byte bulk framing header)
unless marked "packet".

### Common message header

```
+0  type        01 EffectDump, 02 ConfigCmd, 04 int, 05 ?, 06 float
+1  ??          00, except 02 on per-tone dump pushes and 04 on the EffectDump reply
+2  route (4)   host->POD: 0A 40 <ch> 03    POD->host: 0A 03 <ch> 40
+6  00
+7  subcommand  (table below)
+8  tone        00 = Tone 1, 01 = Tone 2 (for per-tone messages)
```

`<ch>` is `01` for live edits and `02` for patch-memory traffic (slot
select, dump push/read). Replies swap the two halves of the route, so these
bytes look like source/destination addresses.

| Type | Sub | Direction | Meaning |
|---|---|---|---|
| `04` | `13` | host->POD | int parameter set |
| `06` | `15` | host->POD | float parameter set |
| `05` | `16` | host->POD | sent in pairs on tone switch, value 0/1 per tone (focus flags?) |
| `04` | `20` | host->POD | select tone for editing (value = tone index) |
| `02` | `21` | host->POD | query (arg `03`, `07` seen, meaning unknown) |
| `04` | `22` | POD->host | answer to `21` |
| `02` | `00` | host->POD | **request EffectDump**, u32 arg = slot |
| `02` | `02` | host->POD | **write patch to memory**: u32 slot + 4096-byte EffectDump (4108 bytes; byte +1 = `04`) |
| `01` | `01` | POD->host | **EffectDump** reply (4104 bytes) |
| `02` | `04` | host->POD | push one tone's 2048-byte block into the edit buffer |
| `02` | `27` | host->POD | select patch slot, u32 arg = slot |
| `02` | `03` | POD->host | ack for `02` writes and `04` pushes |

Slot numbering is `(bank-1)*4 + channel`, with A=0..D=3 (5A = `0x10`).

Setting a parameter is **fire-and-forget**: one bulk OUT per value change,
with no reply. Knob drags stream one float set per mouse step.

### Int set (`04`/`13`): block on/off

```
04 00 0A 40 01 03 00 13 <tone> 00 00 00 <slot u16> <group u16> <u32 LE value>
```

| Block | slot | group |
|---|---|---|
| Gate | `00` | `02` |
| Wah | `02` | `02` |
| Stomp | `03` | `02` |
| Amp (sends slot `00` and `01`, amp+cab?) | `00`,`01` | `03` |
| EQ | `04` | `03` |
| Comp | `00` | `05` |
| Mod | `03` | `05` |
| Delay | `04` | `05` |
| Reverb | `05` | `05` |

Values: `0` = off, `1` = on. VOL has no on/off switch in the UI.

### Float set (`06`/`15`): amp knobs

```
06 00 0A 40 01 03 00 15 <tone> 00 00 00 <slot u16> <group u16> 01 00 00 00 <idx u16> 10 3F <f32 LE>
```

The amp knobs are slot `00`, group `03`:

| Knob | `idx` |
|---|---|
| Bass | `00` |
| Middle | `01` |
| Treble | `02` |
| Drive | `03` |
| Presence | `04` |
| Volume | `05` (matches the example above) |

Values are 0.0-1.0. `<tone>` is confirmed: the same Drive drag on Tone 2
sends `01` there (so the earlier example with `01` was a Tone 2 edit). The
`01 00 00 00` and `10 3F` fields are still unknown. Inside the EffectDump,
parameters are stored as `<idx u16> 10 3F <f32>` too (see below).

### Effect knob map (float sets, 8D tone 1)

Captured by nudging every knob on each effect panel
(`tools/vm-capture/actions/fx/`). Keys are `idx`/namespace (`k` = `10 3F`,
`m` = `01 3F`, `u` = `00 3F`), and the block is addressed by its current
chain slot/group. Model-specific knobs will differ for other models.

| Block (model on 8D) | slot/group | Knobs |
|---|---|---|
| Gate | 0/2 | Threshold `0u` (dB), Decay `3u` |
| Wah (Vetta Wah) | 2/2 | Position `1u` |
| Stomp (Fuzz Pi) | 3/2 | Drive `1k`, Gain `2k`, Tone `3k` |
| FX loop | 12/2 | Send `1u`, Return `2u`, Mix `1m` |
| Comp | 0/5 | Threshold `0k`, Gain `1k` |
| EQ (4 Band Semi-Param) | 4/3 | band *n* (0-3): Freq `(2n)k`, Gain `(2n+1)k` (checked: band 1 freq `0k`, gain `1k`; band 4 gain `7k`) |
| Volume pedal | 2/5 | Min `4u`, Max `5u` |
| Mod (Bias Tremolo) | 3/5 | Speed `0k`, Wave `1k`, Mix `1m` |
| Delay (Tube Echo) | 4/5 | Time `0k`, Feedback `1k`, Flutter `2k`, Drive `3k`, Mix `1m` |
| Reverb (Brite Room) | 5/5 | Decay `0k`, Pre-delay `1k`, Tone `2k`, Mix `2m` |
| Amp | 0/3 | Bass `0k`, Middle `1k`, Treble `2k`, Drive `3k`, Presence `4k`, Volume `5k` |

The Time (delay) and Speed (mod) knobs also send an int message with
sub `0x14` for the same slot/group, value `0`, once per step. It is
probably "tempo sync off" (the panels have an FX TEMPO on/off button).
Unconfirmed.

### EffectDump layout (confirmed)

`02`/`00` with slot `0x10` got a 4104-byte `01`/`01` reply: an 8-byte
header, then 4096 bytes. **Those 4096 bytes are exactly Tone 1's 2048-byte
block followed by Tone 2's 2048-byte block.** They match, byte for byte,
the two `02`/`04` pushes Gearbox sends when it loads the patch (12-byte
header + 2048). The same 4096 bytes are what `02`/`02` writes.

**A tone block is a 0xE4-byte header followed by 12 block records of 0x8C
(140) bytes each.** 0xE4 + 12 × 0x8C = 0x800. This holds for all 10 tones
decoded so far (5A and all of bank 8), and `tools/vm-capture/parse_tone.py`
parses them.

#### Block record (0x8C bytes)

```
+0x00  model      u16   index into the table picked by +0x02 (docs/MODELS.md)
+0x02  table      u8    02 delay, 03 mod, 04 reverb, 05 stomp (dist), 06 wah,
                        07 volume, 0A stomp (filter/synth), 0B gate/comp/loop,
                        0C EQ. Amp and cab records have 02 here too.
+0x03  category   u8    00 amp, 01 cab, 02 effect
+0x04  slot       u16   chain position, same values as the live int-set address
+0x06  group      u16   02 = pre-amp, 03 = amp section, 05 = post-amp
+0x08  enabled    u8    0/1 (the live block on/off int set writes this)
+0x09  00 00
+0x0B  count      u8    number of parameter records that follow
+0x0C  params     count × 8 bytes: <idx u16> <type u16> <value 4 bytes>
```

Record order in the tone block is fixed (amp, cab, stomp, mod, delay,
reverb, gate, comp, EQ, wah, volume, FX loop), but **slot/group move** when
a block is switched between pre and post. Mod shows up as slot 3/group 5
(post) or slot 4/group 2 (pre), delay as 4/5 or 5/2, and the loop as 12/2
or 9/5. The live int/float sets address a block by its current slot and
group, not by record index.

Parameter records: `<idx u16> <namespace u16> <f32 LE>`. **A parameter is
identified by (idx, namespace), not idx alone.** The delay block, for
example, has Feedback at `1`/`10 3F` and Mix at `1`/`01 3F`. Every value
seen so far is a float:

| namespace (LE bytes) | Meaning |
|---|---|
| `10 3F` | normal knobs, 0.0-1.0 |
| `01 3F` | the Mix knob of mod/delay/reverb/loop, 0.0-1.0 |
| `00 3F` | real units: gate threshold in dB (-58.0 shows as "-58 dB"), gate decay, wah position, volume pedal min/max, loop send/return |

Live float sets (`06`/`15`) carry the same `<idx> <namespace> <f32>` at
message offset `0x14`, so the knob map below applies to both. See the next
section.

Some records have stale non-zero bytes after their last parameter,
probably left over from a previous model. The unit accepts them.

#### Tone header (0x00-0xE3)

| Offset | Field | Evidence |
|---|---|---|
| `0x00` | Tone name, ASCII, space-padded, 16 bytes | all tones |
| `0x28`-`0x29` | two values 0-127 (`43 7F`, `47 64`, `0E 7F`...) | meaning unknown |
| `0x38` | f32, **likely tempo in BPM**: 120.0 on untouched tones, 118.6 on 8D, and the mod/delay panels show "FX TEMPO 118.6" | 8 tones + screen |
| `0x40` | a `10 3F` float record (idx 0) | meaning unknown |
| `0xD4`-`0xD5` | copy of `0x28`-`0x29`, tone 1 only. **Gearbox zeroes it when writing** | 8D before/after PUT |

Model IDs are in the amp record (`+0x00` at tone offset `0x0E4`) and the
cab record (tone offset `0x170`), matching the offsets found earlier.

Changing the amp or cab model does **not** use a parameter set: Gearbox
re-pushes the whole tone block (`02`/`04`), and the POD acks it with
`02`/`03`. That is the write path for anything the int/float sets can't
reach.

### Patch load / read sequences

- **Load a patch** (double-click in the Hardware Memory window): `27`
  (select slot), push Tone 1 block, push Tone 2 block, `20` (select
  Tone 1), then `21` queries. Gearbox pushes its **cached** copy; nothing
  is read from the POD.
- **GET SELECTED**: `00` (request dump, slot) -> 4104-byte `01` reply,
  then the same load sequence as above using the fresh data.
- **PUT SELECTED** (write, tested on 8D): `02`/`02` = slot `0x1F` + the
  4096-byte patch -> `02`/`03` ack, then the load sequence. **Verified by
  reading 8D back:** the stored dump equals the written bytes exactly.
  Gearbox re-serializes the patch on the way out, though: besides the
  intended change, it cleared tone header `0xD4`-`0xD5` and added a default
  parameter record to one block. So a write is "Gearbox's model of the
  patch", not "device copy + one change".

Gearbox only enables GET/PUT while its copy of the **active** patch differs
from the hardware (the patch name is shown in italics), and GET ALL / GET
SELECTED then only fetch that one patch. The capture tooling works around
this by loading a slot and toggling the gate twice before a GET
(`tools/vm-capture/get-slot.sh`).

Continuation packets of these multi-packet messages have flags `04` at
packet offset 2, and a byte at packet offset 1 that varies with no
obvious pattern (`42`, `53`, `0F`, `F7`, ...), which is still unknown.

## What's genuinely unknown

1. **Knob maps for other effect models.** The knob map above covers the
   models loaded on 8D. Other models (and their `u` real-unit ranges)
   need the same knob sweep, run with each model loaded.
2. **The rest of the tone header** (0x00-0xE3): inputs, pedal/tweak
   assignments, the `0x28` pair and the `0x40` record are unmapped. Only the
   name and the (likely) tempo are identified.
3. **Whether MIDI CC / SysEx also works over the 5-pin DIN MIDI ports**,
   independent of USB. Line6 publishes an official MIDI CC chart for X3
   Live, but per `pod-ui` maintainer `arteme` (issue #70), full SysEx
   patch-dump/UDI support over MIDI on X3 is unconfirmed — he tested a unit
   whose MIDI port was dead. If DIN MIDI CC works, it's a much simpler path
   for live parameter tweaking (standard MIDI libraries apply), but won't
   give the fuller librarian features.

## Reverse-engineering methodology going forward

Gearbox traffic can now be captured (see above and `tools/vm-capture/`),
which is the fastest way to map live parameters. For the EffectDump layout,
**black-box diffing against real hardware** still applies:

1. Take an EffectDump of a patch.
2. Change exactly one thing on the unit's own front panel (e.g. tweak one
   knob, or rename the patch).
3. Take another EffectDump.
4. Diff the two blobs. The changed byte(s) are very likely the encoding for
   that control.
5. Record the finding in this document, with the exact steps and byte
   offset/encoding.

The full-tone pushes and dumps are now the quickest way to do this: change
one thing in Gearbox, and the tone block it pushes can be diffed against
the previous one (`parse_tone.py` for structure).

This requires the physical POD X3 connected via USB to a machine running the
probe tooling in `crates/pod-core` — no Gearbox or Windows VM needed for
this part. (A capture of real Gearbox traffic, if one ever becomes
available, would still be valuable to cross-check — see
[arteme/pod-ui#70](https://github.com/arteme/pod-ui/issues/70), where a
community member has offered to attempt exactly that.)

## Sources

- [andree182/podx3](https://github.com/andree182/podx3) — GPLv2, primary
  source for everything in this document.
- [arteme/pod-ui#70](https://github.com/arteme/pod-ui/issues/70) — ongoing
  community discussion of X3 support; confirms no prior art exists for the
  EffectDump layout or a working USB capture.
- [Line6 USB compatibility KB](https://kb.line6.com/usb-compatibility-with-line-6-devices)
  — confirms X3 is not class-compliant.
- [Line6 MIDI Continuous Controller Reference](https://line6.com/data/l/0a06000f1344c45ecbcd6e0293/application/pdf/MIDI%20Continuous%20Controller%20Reference%20)
  — official CC chart; relevant only if DIN MIDI path is pursued.
