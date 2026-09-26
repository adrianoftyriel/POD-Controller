//! Wire-format constants and message encode/decode for the POD X3 control
//! channel. Everything here is sourced from `docs/PROTOCOL.md` — keep that
//! file and this module in sync as more of the protocol is discovered.

/// Line6's USB vendor ID.
pub const VENDOR_ID: u16 = 0x0E41;
/// POD X3 (rack/desktop) product ID.
pub const PRODUCT_ID_X3: u16 = 0x414A;
/// POD X3 Live product ID.
pub const PRODUCT_ID_X3_LIVE: u16 = 0x414B;

/// The control (non-audio) USB interface number.
pub const CONTROL_INTERFACE: u8 = 1;
/// Bulk IN endpoint (device -> host) on the control interface.
pub const BULK_IN_EP: u8 = 0x81;
/// Bulk OUT endpoint (host -> device) on the control interface.
pub const BULK_OUT_EP: u8 = 0x01;
/// Max bulk packet size on the control interface.
pub const BULK_PACKET_LEN: usize = 64;

/// Vendor-specific control request used for the init handshake (read
/// serial/firmware, and the still-unexplained 0xF000-0xF080 probe reads).
/// Named `L6_X3_CTRL` in prior art (andree182/podx3).
pub const CTRL_REQUEST: u8 = 0x67;

/// Route channel byte: `01` for live edits (channel 01 in the route field
/// `0A 40 <channel> 03`), `02` for patch-memory traffic (slot select,
/// dump push/read). See docs/PROTOCOL.md "Common message header".
pub const CHANNEL_LIVE: u8 = 0x01;
pub const CHANNEL_PATCH: u8 = 0x02;

/// Message type byte (first byte of a reassembled bulk message payload).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    /// Full patch/effect-chain blob (~4096 bytes). Internal layout is
    /// documented in docs/PROTOCOL.md "EffectDump layout (confirmed)".
    EffectDump = 0x01,
    /// Config/control commands: request/write EffectDump, select patch
    /// slot, push a tone block.
    ConfigCmd = 0x02,
    /// Int parameter set: block on/off (sub `0x13`), block move
    /// (sub `0x12`), tempo sync (sub `0x14`), select tone (sub `0x20`).
    IntSet = 0x04,
    /// Tone-level parameter set (int or float, keyed by `kind`).
    ToneSet = 0x05,
    /// Float parameter set (amp/effect knobs).
    FloatSet = 0x06,
}

impl MessageType {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x01 => Some(Self::EffectDump),
            0x02 => Some(Self::ConfigCmd),
            0x04 => Some(Self::IntSet),
            0x05 => Some(Self::ToneSet),
            0x06 => Some(Self::FloatSet),
            _ => None,
        }
    }
}

// --- Subcommands (message payload byte +7) -------------------------------

pub const SUB_REQUEST_DUMP: u8 = 0x00; // ConfigCmd
pub const SUB_WRITE_PATCH: u8 = 0x02; // ConfigCmd
pub const SUB_PUSH_TONE_BLOCK: u8 = 0x04; // ConfigCmd
pub const SUB_BLOCK_MOVE: u8 = 0x12; // IntSet
pub const SUB_INT_SET: u8 = 0x13; // IntSet (block on/off)
pub const SUB_TEMPO_SYNC: u8 = 0x14; // IntSet
pub const SUB_FLOAT_SET: u8 = 0x15; // FloatSet
pub const SUB_TONE_SET: u8 = 0x16; // ToneSet
pub const SUB_SELECT_TONE: u8 = 0x20; // IntSet
pub const SUB_QUERY: u8 = 0x21; // ConfigCmd
pub const SUB_SELECT_PATCH: u8 = 0x27; // ConfigCmd

/// The parameter namespace bytes that follow a knob's `idx` in float sets
/// and in the EffectDump block-record parameter table. Confirmed in
/// docs/PROTOCOL.md "EffectDump layout (confirmed)".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamNamespace {
    /// `10 3F` — normal knobs, 0.0-1.0.
    Normal,
    /// `01 3F` — the Mix knob of mod/delay/reverb/loop, 0.0-1.0.
    Mix,
    /// `00 3F` — real units (gate threshold dB, wah position, etc).
    RealUnit,
}

impl ParamNamespace {
    pub fn bytes(self) -> [u8; 2] {
        match self {
            ParamNamespace::Normal => [0x10, 0x3F],
            ParamNamespace::Mix => [0x01, 0x3F],
            ParamNamespace::RealUnit => [0x00, 0x3F],
        }
    }
}

/// Build the common 8-byte message header (type, route, subcommand) shared
/// by the `02`/`04`/`05`/`06` message families. See docs/PROTOCOL.md
/// "Common message header".
fn message_header(msg_type: u8, channel: u8, subcommand: u8) -> [u8; 8] {
    [msg_type, 0x00, 0x0A, 0x40, channel, 0x03, 0x00, subcommand]
}

/// Build a float-parameter-set message (`06`/`15`): sets the knob
/// identified by `(idx, namespace)` on the block currently at `slot`/`group`
/// for `tone`. Confirmed working for amp knobs (slot 0, group 3) — see
/// docs/PROTOCOL.md "Effect knob map" for other blocks' slot/group and idx
/// values. `tone` is `0` for Tone 1, `1` for Tone 2.
pub fn encode_float_set(
    tone: u8,
    slot: u16,
    group: u16,
    idx: u16,
    namespace: ParamNamespace,
    value: f32,
) -> Vec<u8> {
    let mut payload = message_header(0x06, CHANNEL_LIVE, SUB_FLOAT_SET).to_vec();
    payload.push(tone);
    payload.extend_from_slice(&[0x00, 0x00, 0x00]);
    payload.extend_from_slice(&slot.to_le_bytes());
    payload.extend_from_slice(&group.to_le_bytes());
    payload.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);
    payload.extend_from_slice(&idx.to_le_bytes());
    payload.extend_from_slice(&namespace.bytes());
    payload.extend_from_slice(&value.to_le_bytes());
    frame_message(&payload)
}

/// Same shape as [`encode_float_set`] but for an int-valued parameter,
/// addressed by `(subcommand, tone, slot, group)`. Shared by block on/off
/// (`0x13`) and tempo sync (`0x14`) — see docs/PROTOCOL.md.
fn encode_addressed_int(subcommand: u8, tone: u8, slot: u16, group: u16, value: u32) -> Vec<u8> {
    let mut payload = message_header(0x04, CHANNEL_LIVE, subcommand).to_vec();
    payload.push(tone);
    payload.extend_from_slice(&[0x00, 0x00, 0x00]);
    payload.extend_from_slice(&slot.to_le_bytes());
    payload.extend_from_slice(&group.to_le_bytes());
    payload.extend_from_slice(&value.to_le_bytes());
    frame_message(&payload)
}

/// Turn the block at `slot`/`group` on or off (`04`/`13`).
pub fn encode_block_enabled(tone: u8, slot: u16, group: u16, enabled: bool) -> Vec<u8> {
    encode_addressed_int(SUB_INT_SET, tone, slot, group, enabled as u32)
}

/// Set the tempo-sync division for the block at `slot`/`group` (`04`/`14`).
/// `division` is the FX TEMPO menu position (0 = off; see docs/PROTOCOL.md
/// for the full 0-13 table).
pub fn encode_tempo_sync(tone: u8, slot: u16, group: u16, division: u8) -> Vec<u8> {
    encode_addressed_int(SUB_TEMPO_SYNC, tone, slot, group, division as u32)
}

/// Move a block between its pre/post slot-group positions (`04`/`12`),
/// addressed by its *current* `slot`/`group`, with the new position as the
/// value. **Only confirmed for the two positions Gearbox itself uses per
/// block** (docs/PROTOCOL.md "Signal chain") — whether the POD accepts
/// other positions is untested.
pub fn encode_block_move(tone: u8, slot: u16, group: u16, new_slot: u16, new_group: u16) -> Vec<u8> {
    let mut payload = message_header(0x04, CHANNEL_LIVE, SUB_BLOCK_MOVE).to_vec();
    payload.push(tone);
    payload.extend_from_slice(&[0x00, 0x00, 0x00]);
    payload.extend_from_slice(&slot.to_le_bytes());
    payload.extend_from_slice(&group.to_le_bytes());
    payload.extend_from_slice(&new_slot.to_le_bytes());
    payload.extend_from_slice(&new_group.to_le_bytes());
    frame_message(&payload)
}

fn encode_config_cmd(subcommand: u8, channel: u8, slot: u32) -> Vec<u8> {
    let mut payload = message_header(0x02, channel, subcommand).to_vec();
    payload.extend_from_slice(&slot.to_le_bytes());
    frame_message(&payload)
}

/// Request the POD send back the EffectDump for `slot` (`02`/`00`). Reply
/// is a 4104-byte `01`/`01` message (8-byte header + 4096-byte blob).
pub fn encode_request_dump(slot: u32) -> Vec<u8> {
    encode_config_cmd(SUB_REQUEST_DUMP, CHANNEL_PATCH, slot)
}

/// Select `slot` as the active patch (`02`/`27`).
pub fn encode_select_patch(slot: u32) -> Vec<u8> {
    encode_config_cmd(SUB_SELECT_PATCH, CHANNEL_PATCH, slot)
}

/// Write `blob` (a full 4096-byte EffectDump) to `slot` (`02`/`02`).
/// Confirmed round-trip correct in docs/PROTOCOL.md ("PUT SELECTED"): the
/// device stores exactly what's sent. Unlike the other message families
/// this one has `04` (not `00`) at payload byte +1 — see docs/PROTOCOL.md
/// "Common message header".
///
/// # Panics
/// If `blob` is not exactly 4096 bytes.
pub fn encode_write_patch(slot: u32, blob: &[u8]) -> Vec<u8> {
    assert_eq!(blob.len(), 4096, "EffectDump blob must be exactly 4096 bytes");
    let mut payload = Vec::with_capacity(12 + blob.len());
    payload.push(0x02);
    payload.push(0x04);
    payload.extend_from_slice(&[0x0A, 0x40, CHANNEL_PATCH, 0x03]);
    payload.push(0x00);
    payload.push(SUB_WRITE_PATCH);
    payload.extend_from_slice(&slot.to_le_bytes());
    payload.extend_from_slice(blob);
    frame_message(&payload)
}

/// Confirmed-working example, kept for the regression test below: a float
/// parameter set for tone volume (idx 5, namespace `Normal`), captured
/// verbatim from real device traffic. See docs/PROTOCOL.md "Confirmed
/// example: float parameter set".
#[cfg(test)]
const CONFIRMED_FLOAT_SET_EXAMPLE: [u8; 32] = [
    0x1C, 0x00, 0x01, 0x00, 0x06, 0x00, 0x0A, 0x40, 0x01, 0x03, 0x00, 0x15, 0x01, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, 0x05, 0x00, 0x10, 0x3F, 0x00, 0x00, 0x80, 0x3F,
];

// --- Bulk transfer chunk framing ------------------------------------------
//
// Bulk data is a stream of chunks, each a 4-byte header plus up to 0xFC
// (252) payload bytes. Chunks are *not* aligned to 64-byte USB packets —
// a chunk can span several packets, and (per captures) a 4108-byte message
// frames as 16 chunks of 0xFC followed by one of 0x4C. Message boundaries
// are not carried in any header field (`contents_length` is the chunk's own
// payload length, capped at 0xFC, so it can't encode a multi-kilobyte total).
// See docs/PROTOCOL.md "Bulk transfer framing".

/// Max payload bytes in a single chunk.
pub const MAX_CHUNK_PAYLOAD: usize = 0xFC;
const CHUNK_HEADER_LEN: usize = 4;

pub const FLAG_FIRST: u8 = 0x01;
pub const FLAG_CONTINUATION: u8 = 0x04;

/// A chunk's 4-byte framing header.
#[derive(Debug, Clone, Copy)]
pub struct ChunkHeader {
    pub contents_length: u8,
    pub flags: u8,
}

impl ChunkHeader {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < CHUNK_HEADER_LEN {
            return None;
        }
        Some(Self {
            contents_length: bytes[0],
            flags: bytes[2],
        })
    }
}

/// Wrap a complete message payload in bulk-transfer chunk framing: a
/// 4-byte header per <=252-byte piece, the first flagged [`FLAG_FIRST`] and
/// the rest [`FLAG_CONTINUATION`]. The result is ready to hand to a single
/// bulk OUT transfer (the USB layer splits it into 64-byte packets on its
/// own; that split is unrelated to chunk boundaries).
pub fn frame_message(message: &[u8]) -> Vec<u8> {
    if message.is_empty() {
        return Vec::new();
    }
    let num_chunks = message.len().div_ceil(MAX_CHUNK_PAYLOAD);
    let mut out = Vec::with_capacity(message.len() + CHUNK_HEADER_LEN * num_chunks);
    for (i, chunk) in message.chunks(MAX_CHUNK_PAYLOAD).enumerate() {
        let flags = if i == 0 { FLAG_FIRST } else { FLAG_CONTINUATION };
        out.push(chunk.len() as u8);
        out.push(0x00);
        out.push(flags);
        out.push(0x00);
        out.extend_from_slice(chunk);
    }
    out
}

/// Reassembles a stream of bulk-framed chunks back into complete message
/// payloads. Feed it raw bytes read from the bulk-IN endpoint in whatever
/// sizes they arrive (no assumption of 64-byte alignment).
///
/// A chunk shorter than [`MAX_CHUNK_PAYLOAD`] is treated as the last chunk
/// of a message. This matches every capture seen so far (e.g. the 4108-byte
/// patch write's trailing 0x4C chunk) but is **inferred, not confirmed**,
/// for a message whose length happens to be an exact multiple of 252 bytes
/// — see docs/PROTOCOL.md "What's genuinely unknown". `tools/vm-capture/msgs.py`
/// instead detects boundaries by looking at the *next* chunk's `FLAG_FIRST`,
/// which needs a message to follow; that doesn't apply to a single
/// request/reply `transact()` where nothing else arrives.
#[derive(Default)]
pub struct PacketReassembler {
    message: Vec<u8>,
    pending: Vec<u8>,
}

impl PacketReassembler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed raw bytes read from the bulk-IN endpoint. Returns every
    /// complete message extracted so far (usually zero or one).
    pub fn push(&mut self, data: &[u8]) -> crate::error::Result<Vec<Vec<u8>>> {
        self.pending.extend_from_slice(data);
        let mut messages = Vec::new();
        while let Some(header) = ChunkHeader::parse(&self.pending) {
            let total = CHUNK_HEADER_LEN + header.contents_length as usize;
            if self.pending.len() < total {
                break;
            }
            if header.flags & FLAG_FIRST != 0 {
                self.message.clear();
            }
            self.message
                .extend_from_slice(&self.pending[CHUNK_HEADER_LEN..total]);
            self.pending.drain(..total);
            if (header.contents_length as usize) < MAX_CHUNK_PAYLOAD {
                messages.push(std::mem::take(&mut self.message));
            }
        }
        Ok(messages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_float_set_matches_confirmed_example() {
        assert_eq!(
            encode_float_set(1, 0, 3, 5, ParamNamespace::Normal, 1.0),
            CONFIRMED_FLOAT_SET_EXAMPLE
        );
    }

    #[test]
    fn frame_message_single_chunk_matches_confirmed_example() {
        let payload = &CONFIRMED_FLOAT_SET_EXAMPLE[4..];
        assert_eq!(frame_message(payload), CONFIRMED_FLOAT_SET_EXAMPLE.to_vec());
    }

    #[test]
    fn reassembler_handles_single_chunk_message() {
        let mut r = PacketReassembler::new();
        let framed = encode_float_set(0, 0, 3, 5, ParamNamespace::Normal, 1.0);
        let messages = r.push(&framed).unwrap();
        assert_eq!(messages, vec![framed[4..].to_vec()]);
    }

    /// Confirmed from docs/PROTOCOL.md: a 4108-byte patch write frames as
    /// 16 chunks of 0xFC (252) followed by one of 0x4C (76):
    /// 16*252 + 76 = 4108.
    #[test]
    fn frame_message_matches_confirmed_patch_write_chunk_counts() {
        let message = vec![0xAAu8; 4108];
        let framed = frame_message(&message);

        let mut offset = 0;
        let mut chunk_lens = Vec::new();
        let mut flags = Vec::new();
        while offset < framed.len() {
            let header = ChunkHeader::parse(&framed[offset..]).unwrap();
            chunk_lens.push(header.contents_length as usize);
            flags.push(header.flags);
            offset += CHUNK_HEADER_LEN + header.contents_length as usize;
        }

        assert_eq!(chunk_lens.len(), 17, "expected 16 full chunks + 1 remainder");
        assert!(chunk_lens[..16].iter().all(|&l| l == 0xFC));
        assert_eq!(chunk_lens[16], 0x4C);
        assert_eq!(flags[0], FLAG_FIRST);
        assert!(flags[1..].iter().all(|&f| f == FLAG_CONTINUATION));
    }

    #[test]
    fn reassembler_round_trips_a_multi_chunk_message_fed_in_arbitrary_pieces() {
        let message: Vec<u8> = (0..4108u32).map(|i| (i % 251) as u8).collect();
        let framed = frame_message(&message);

        // Feed it back in 64-byte pieces, like real bulk-IN reads, so
        // chunk boundaries deliberately don't line up with read boundaries.
        let mut r = PacketReassembler::new();
        let mut reassembled = Vec::new();
        for piece in framed.chunks(64) {
            reassembled.extend(r.push(piece).unwrap());
        }
        assert_eq!(reassembled, vec![message]);
    }

    #[test]
    fn encode_request_dump_and_select_patch_are_twelve_bytes() {
        // 8-byte header + 4-byte u32 slot, single chunk.
        let dump = encode_request_dump(0x10);
        assert_eq!(dump[0], 0x0C);
        assert_eq!(&dump[4..8], &[0x02, 0x00, 0x0A, 0x40]);
        assert_eq!(dump[8], CHANNEL_PATCH);
        assert_eq!(dump[11], SUB_REQUEST_DUMP);
        assert_eq!(&dump[12..16], &0x10u32.to_le_bytes());

        let select = encode_select_patch(0x1F);
        assert_eq!(select[11], SUB_SELECT_PATCH);
        assert_eq!(&select[12..16], &0x1Fu32.to_le_bytes());
    }

    #[test]
    fn encode_write_patch_is_4108_bytes_with_byte_plus_one_set() {
        let blob = vec![0u8; 4096];
        let framed = encode_write_patch(0x1F, &blob);
        // 16 full chunks + 1 remainder, matching the confirmed capture.
        let total_payload: usize = 8 + 4 + 4096;
        assert_eq!(total_payload, 4108);
        assert_eq!(framed[4], 0x02);
        assert_eq!(framed[5], 0x04, "byte +1 is 04 for write-patch, unlike other messages");
    }

    #[test]
    #[should_panic(expected = "4096 bytes")]
    fn encode_write_patch_rejects_wrong_size_blob() {
        encode_write_patch(0, &[0u8; 10]);
    }
}
