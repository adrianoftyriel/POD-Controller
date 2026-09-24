# POD Controller

An open-source, cross-platform replacement for Line6's abandoned **Gearbox**
software: set up patches and edit effects on a **Line6 POD X3 / X3 Live**
over USB, from Windows, macOS, and (eventually) Android.

## Why this is harder than it sounds

The POD X3 is **not** a USB class-compliant MIDI or audio device. It uses a
proprietary, undocumented USB bulk-transfer protocol (vendor class `0xFF`).
Line6 never published this protocol, and the official Gearbox app is the
only software that ever fully spoke it. See [`docs/PROTOCOL.md`](docs/PROTOCOL.md)
for everything currently known about the wire format, its sources, and what's
still unknown.

This project builds on prior reverse-engineering work by
[andree182/podx3](https://github.com/andree182/podx3) (raw USB protocol,
GPLv2) and tracks the parallel community effort in
[arteme/pod-ui#70](https://github.com/arteme/pod-ui/issues/70).

## Status

Reading patches from a real POD X3 Live is confirmed working
(`pod-cli dump`). Writing (`restore`) and live parameter edits
(`set-amp`, `block`) are implemented against the documented protocol but
not yet confirmed on hardware.

## Planned phasing

1. **Core protocol crate** (`crates/pod-core`, Rust, using [`nusb`](https://docs.rs/nusb)):
   device discovery, control handshake, bulk framing, EffectDump
   (patch blob) get/set, and int/float live-parameter get/set — the pieces
   already confirmed working by prior reverse-engineering.
2. **Patch librarian MVP**: back up, restore, and swap patches between slots,
   treating the 4096-byte EffectDump blob as opaque. This does not require
   decoding its internal layout.
3. **Live effect editing MVP**: get/set individual effect parameters
   (volume, gain, etc.) on the currently loaded patch via the confirmed
   float/int parameter messages.
4. **Blob decoding**: reverse-engineer the internal layout of the EffectDump
   blob (which bytes encode which effect model/parameter) by diffing dumps
   taken before/after changes made on the unit's own front panel. This
   unlocks full patch editing (building effect chains from scratch, naming
   patches, etc.) — see `docs/PROTOCOL.md` for the diffing methodology.
5. **Desktop UI** (Windows/macOS) on top of `pod-core`.
6. **Android port**: reimplement the transport layer on Android's native USB
   host API (`android.hardware.usb`), reusing the protocol/patch-model logic.

## License

GPL-3.0, for consistency with the reverse-engineering projects this work
builds on (`pod-ui`, `l6t-rs`, `podx3`). Open to revisiting this.
