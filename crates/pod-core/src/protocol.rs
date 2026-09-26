//! Wire-format constants and message encode/decode for the POD X3 control
//! channel. Everything here is sourced from `docs/PROTOCOL.md` — keep that
//! file and this module in sync as more of the protocol is discovered.

use crate::error::{PodError, Result};

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

/// Size of a decoded EffectDump (patch) blob, in bytes.
pub const EFFECT_DUMP_LEN: usize = 4096;

/// Length of the POD's `02`/`03` ack message (8-byte header + u32 0).
pub const ACK_LEN: usize = 12;
/// One tone's block inside an EffectDump; Tone 2 follows Tone 1.
pub const TONE_BLOCK_LEN: usize = 2048;

/// Subcommand of the POD's `04`/`22` reply to a `02`/`21` query.
pub const QUERY_REPLY_SUB: u8 = 0x22;
/// Length of a `04`/`22` reply: header, u32 0, u32 id, u32 value.
pub const QUERY_REPLY_LEN: usize = 20;

/// Device-wide setting IDs, used by `04`/`20` sets and `02`/`21` queries
/// (docs/PROTOCOL.md "Tone-level and global controls"). Only IDs `00`-`08`
/// answer a query.
pub mod setting {
    /// Selected tone for editing (0/1).
    pub const SELECTED_TONE: u32 = 0x03;
    /// 1/4" outputs mode.
    pub const OUTPUTS: u32 = 0x07;
}

/// Channel byte used in the 4-byte "route" field of the common message
/// header for live parameter edits (the currently-loaded patch).
pub const CHANNEL_LIVE: u8 = 0x01;
/// Channel byte used for patch-memory traffic: slot select, dump push/read.
pub const CHANNEL_PATCH: u8 = 0x02;

/// Message type byte (first byte of a reassembled bulk message payload).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    /// Full patch/effect-chain blob (~4096 bytes). Internal layout not
    /// decoded yet — treat as opaque for backup/restore/swap.
    EffectDump = 0x01,
    /// Config/control commands, including "request/send EffectDump(i)".
    ConfigCmd = 0x02,
    /// Integer parameter, 12-byte encoding.
    IntParam12 = 0x04,
    /// Integer parameter, 16-byte encoding.
    IntParam16 = 0x05,
    /// Float parameter (confirmed working, see `encode_float_param`).
    FloatParam = 0x06,
}

impl MessageType {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x01 => Some(Self::EffectDump),
            0x02 => Some(Self::ConfigCmd),
            0x04 => Some(Self::IntParam12),
            0x05 => Some(Self::IntParam16),
            0x06 => Some(Self::FloatParam),
            _ => None,
        }
    }
}

/// ConfigCmd (`0x02`) subcommand bytes — see docs/PROTOCOL.md "Confirmed
/// from real Gearbox traffic".
pub mod config_cmd {
    /// host->POD: request EffectDump, u32 arg = slot.
    pub const REQUEST_DUMP: u8 = 0x00;
    /// host->POD: write patch to memory, u32 slot + 4096-byte EffectDump.
    pub const WRITE_DUMP: u8 = 0x02;
    /// POD->host: ack for `WRITE_DUMP` writes and per-tone pushes.
    pub const ACK: u8 = 0x03;
    /// host->POD: select patch slot, u32 arg = slot. Only meaningful
    /// followed by two `PUSH_TONE`s.
    pub const SELECT_SLOT: u8 = 0x27;
    /// host->POD: push one tone's 2048-byte block into the edit buffer.
    pub const PUSH_TONE: u8 = 0x04;
    /// host->POD: query a device-wide setting, u32 arg = id.
    pub const QUERY: u8 = 0x21;
    /// POD->host: EffectDump reply subcommand (message type is `0x01`, not
    /// `0x02`, but it shares the same "sub" header position).
    pub const EFFECT_DUMP_REPLY: u8 = 0x01;
}

/// Int-set (`0x04`) subcommand bytes for messages shaped like
/// `<tone u32> <slot u16> <group u16> <value u32>` — see docs/PROTOCOL.md
/// "Int set (04/13): block on/off" and the sync/move rows below it.
pub mod int_set {
    /// Block on/off. Value: 0 = off, 1 = on.
    pub const BLOCK_ENABLED: u8 = 0x13;
    /// Tempo-sync division for a block (0 = off).
    pub const TEMPO_SYNC: u8 = 0x14;
    /// Move a block between its pre- and post-amp positions; the value is
    /// the new `<slot u16><group u16>` (see `encode_block_move`).
    pub const BLOCK_MOVE: u8 = 0x12;
    /// Device-wide setting (different shape, see `encode_device_setting`).
    pub const DEVICE_SETTING: u8 = 0x20;
}

/// Float-set (`0x06`) subcommand byte for messages shaped like
/// `<tone u32> <slot u16> <group u16> 01 00 00 00 <idx u16> <namespace u16> <f32>`.
pub const FLOAT_SET_SUB: u8 = 0x15;

/// Parameter namespace values (the `<idx u16> <namespace u16>` key that
/// identifies a parameter, stored little-endian as `<lo> <hi>` bytes in
/// captures) — see docs/PROTOCOL.md "Parameter records".
pub mod namespace {
    /// Normal knobs, 0.0-1.0.
    pub const NORMAL: u16 = 0x3F10;
    /// The Mix knob of mod/delay/reverb/loop, 0.0-1.0.
    pub const MIX: u16 = 0x3F01;
    /// Real units (dB, position, etc.) rather than a normalized 0.0-1.0.
    pub const REAL_UNITS: u16 = 0x3F00;
}

/// The 4-byte header prepended to every bulk chunk.
#[derive(Debug, Clone, Copy)]
pub struct PacketHeader {
    pub contents_length: u8,
    pub flags: u8,
}

pub const FLAG_FIRST: u8 = 0x01;
pub const FLAG_CONTINUATION: u8 = 0x04;

impl PacketHeader {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 4 {
            return None;
        }
        Some(Self {
            contents_length: bytes[0],
            flags: bytes[2],
        })
    }
}

/// Max payload bytes carried by one chunk (see docs/PROTOCOL.md "Bulk
/// transfer framing").
pub const CHUNK_MAX_PAYLOAD: usize = 0xFC;
/// Bytes in a chunk header.
pub const CHUNK_HEADER_LEN: usize = 4;

/// Split a complete message into chunk-framed bytes (4-byte header + up to
/// [`CHUNK_MAX_PAYLOAD`] bytes of payload per chunk), concatenated and ready
/// for a single bulk OUT transfer — the USB layer fragments that transfer
/// into 64-byte wire packets on its own, so callers don't need to chunk at
/// that level.
///
/// The exact-multiple-of-[`CHUNK_MAX_PAYLOAD`]-bytes edge case (whether a
/// trailing zero-length chunk is needed to terminate the message) is
/// unconfirmed against real hardware — every capture seen so far ends on a
/// short chunk.
pub fn encode_chunks(message: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(
        message.len() + CHUNK_HEADER_LEN * (message.len() / CHUNK_MAX_PAYLOAD + 1),
    );
    let mut offset = 0;
    let mut first = true;
    loop {
        let take = (message.len() - offset).min(CHUNK_MAX_PAYLOAD);
        out.push(take as u8);
        out.push(0x00);
        out.push(if first { FLAG_FIRST } else { FLAG_CONTINUATION });
        out.push(0x00);
        out.extend_from_slice(&message[offset..offset + take]);
        offset += take;
        first = false;
        if offset >= message.len() {
            break;
        }
    }
    out
}

/// Reassembles a stream of raw bulk-IN reads into message bytes by
/// stripping each chunk's own 4-byte header (contents_length + flags) and
/// concatenating the payloads.
///
/// There is no in-band end-of-message marker: a chunk's `contents_length`
/// describes only that chunk, and a chunk shorter than the sender's usual
/// max does **not** reliably mean "last chunk of the message" — confirmed
/// against real hardware, where the device's own EffectDump reply opens
/// with a 24-byte chunk (well under its own 60-byte-per-packet norm) and
/// then keeps going. Callers must know the expected total message length
/// up front (fixed per message type by protocol design) and keep calling
/// [`Self::push`] until they have that many bytes; there's no way to
/// detect "message complete" from the framing alone.
///
/// One `ChunkReassembler` is meant to reassemble exactly one message —
/// construct a fresh one per `transact()`-style call.
#[derive(Default)]
pub struct ChunkReassembler {
    raw: Vec<u8>,
    pos: usize,
}

impl ChunkReassembler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one raw bulk-IN read. Returns whatever newly-available message
    /// bytes it contained (zero or more complete chunks' worth — never a
    /// partial chunk).
    pub fn push(&mut self, packet: &[u8]) -> Result<Vec<u8>> {
        self.raw.extend_from_slice(packet);
        let mut out = Vec::new();
        loop {
            if self.raw.len() - self.pos < CHUNK_HEADER_LEN {
                break;
            }
            let header = PacketHeader::parse(&self.raw[self.pos..]).expect("length checked above");
            let contents_length = header.contents_length as usize;
            let body_start = self.pos + CHUNK_HEADER_LEN;
            let body_end = body_start + contents_length;
            if self.raw.len() < body_end {
                break;
            }
            out.extend_from_slice(&self.raw[body_start..body_end]);
            self.pos = body_end;
        }
        Ok(out)
    }
}

/// Builds the 8-byte common message header shared by ConfigCmd/int/float
/// messages (see docs/PROTOCOL.md "Common message header").
fn common_header(msg_type: u8, byte1: u8, channel: u8, subcommand: u8) -> [u8; 8] {
    [msg_type, byte1, 0x0A, 0x40, channel, 0x03, 0x00, subcommand]
}

/// Confirmed-working template for a float parameter set message (tone
/// volume, parameter index 5), captured verbatim from a real device
/// interaction. Bytes at [`FLOAT_PARAM_INDEX_OFFSET`] (index) and the
/// trailing 4 bytes (value) are the only positions known to vary; every
/// other parameter's index and byte layout is still undiscovered — see
/// `docs/PROTOCOL.md` "What's genuinely unknown".
const FLOAT_PARAM_TEMPLATE: [u8; 24] = [
    0x06, 0x00, 0x0A, 0x40, 0x01, 0x03, 0x00, 0x15, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00,
    0x01, 0x00, 0x00, 0x00, 0x05, 0x00, 0x10, 0x3F,
];

/// Offset within the float-param payload (after the 4-byte bulk header)
/// where the parameter index lives. Only confirmed for index 5 (tone
/// volume) — see `docs/PROTOCOL.md`.
pub const FLOAT_PARAM_INDEX_OFFSET: usize = 20;

/// Build a bulk-framed float-parameter-set message.
///
/// `param_index` and `value` are only confirmed correct for the one known
/// case (index 5 = tone volume, value range 0.0-1.0). Using other indices
/// is speculative until confirmed against real hardware and recorded in
/// `docs/PROTOCOL.md`. Prefer [`encode_float_set`] for parameters whose
/// full `(tone, slot, group, idx, namespace)` address is known.
pub fn encode_float_param(param_index: u8, value: f32) -> Vec<u8> {
    let mut payload = FLOAT_PARAM_TEMPLATE.to_vec();
    payload[FLOAT_PARAM_INDEX_OFFSET] = param_index;
    payload.extend_from_slice(&value.to_le_bytes());
    encode_chunks(&payload)
}

/// Build a live float-parameter-set message (type `0x06`, sub `0x15`) — the
/// "knob turned" message for float-valued parameters (amp knobs, effect
/// knobs, etc.). See docs/PROTOCOL.md "Float set (06/15)".
///
/// `slot`/`group` address the target block at its *current* chain
/// position (this can move when a block is switched pre/post-amp — it is
/// not a fixed per-model constant). `idx`/`namespace` identify the
/// parameter within that block (see docs/KNOBS.md and the effect knob map
/// in docs/PROTOCOL.md).
pub fn encode_float_set(
    tone: u8,
    slot: u16,
    group: u16,
    idx: u16,
    namespace: u16,
    value: f32,
) -> Vec<u8> {
    let mut message = common_header(
        MessageType::FloatParam as u8,
        0x00,
        CHANNEL_LIVE,
        FLOAT_SET_SUB,
    )
    .to_vec();
    message.extend_from_slice(&(tone as u32).to_le_bytes());
    message.extend_from_slice(&slot.to_le_bytes());
    message.extend_from_slice(&group.to_le_bytes());
    message.extend_from_slice(&1u32.to_le_bytes()); // constant; meaning unknown
    message.extend_from_slice(&idx.to_le_bytes());
    message.extend_from_slice(&namespace.to_le_bytes());
    message.extend_from_slice(&value.to_le_bytes());
    encode_chunks(&message)
}

/// Build a live int-parameter-set message (type `0x04`) — used for block
/// on/off (`sub = `[`int_set::BLOCK_ENABLED`]), tempo sync
/// (`sub = `[`int_set::TEMPO_SYNC`]), and other messages shaped like
/// `<tone u32> <slot u16> <group u16> <value u32>`. See docs/PROTOCOL.md
/// "Int set (04/13): block on/off".
pub fn encode_int_set(tone: u8, sub: u8, slot: u16, group: u16, value: u32) -> Vec<u8> {
    let mut message =
        common_header(MessageType::IntParam12 as u8, 0x00, CHANNEL_LIVE, sub).to_vec();
    message.extend_from_slice(&(tone as u32).to_le_bytes());
    message.extend_from_slice(&slot.to_le_bytes());
    message.extend_from_slice(&group.to_le_bytes());
    message.extend_from_slice(&value.to_le_bytes());
    encode_chunks(&message)
}

/// Build a block move (int set, sub `0x12`): the block currently at
/// `slot`/`group` goes to `new_slot`/`new_group`. Each movable block has
/// exactly two positions (docs/PROTOCOL.md "Signal chain").
pub fn encode_block_move(
    tone: u8,
    slot: u16,
    group: u16,
    new_slot: u16,
    new_group: u16,
) -> Vec<u8> {
    let value = new_slot as u32 | (new_group as u32) << 16;
    encode_int_set(tone, int_set::BLOCK_MOVE, slot, group, value)
}

/// Subcommand of the tone-level set (message type `0x05`).
pub const TONE_SETTING_SUB: u8 = 0x16;

/// Build a tone-level set (`05`/`16`): `<tone u32> <kind u32> <param u32>
/// <value>`, kind 1 for an f32 value and 0 for an integer. Used for the
/// tone header settings (input, mic, room, Variax, ...), see
/// docs/PROTOCOL.md "Tone-level and global controls".
pub fn encode_tone_setting(tone: u8, param: u32, value: ToneValue) -> Vec<u8> {
    let mut message = common_header(
        MessageType::IntParam16 as u8,
        0x00,
        CHANNEL_LIVE,
        TONE_SETTING_SUB,
    )
    .to_vec();
    message.extend_from_slice(&(tone as u32).to_le_bytes());
    let (kind, raw) = match value {
        ToneValue::Float(v) => (1u32, v.to_le_bytes()),
        ToneValue::Int(v) => (0u32, v.to_le_bytes()),
    };
    message.extend_from_slice(&kind.to_le_bytes());
    message.extend_from_slice(&param.to_le_bytes());
    message.extend_from_slice(&raw);
    encode_chunks(&message)
}

/// Value of a tone-level set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ToneValue {
    Float(f32),
    Int(u32),
}

/// Build a ConfigCmd message requesting the EffectDump for `slot` — see
/// docs/PROTOCOL.md "02/00: request EffectDump". The reply is decoded with
/// [`decode_effect_dump`].
pub fn encode_request_dump(slot: u8) -> Vec<u8> {
    let mut message = common_header(
        MessageType::ConfigCmd as u8,
        0x00,
        CHANNEL_PATCH,
        config_cmd::REQUEST_DUMP,
    )
    .to_vec();
    message.extend_from_slice(&(slot as u32).to_le_bytes());
    encode_chunks(&message)
}

/// Build a ConfigCmd message making `slot` the active/live patch — see
/// docs/PROTOCOL.md "02/27: select patch slot". Unlike parameter sets, no
/// reply is documented for this message; treat it as fire-and-forget like
/// block on/off.
pub fn encode_select_slot(slot: u8) -> Vec<u8> {
    let mut message = common_header(
        MessageType::ConfigCmd as u8,
        0x00,
        CHANNEL_PATCH,
        config_cmd::SELECT_SLOT,
    )
    .to_vec();
    message.extend_from_slice(&(slot as u32).to_le_bytes());
    encode_chunks(&message)
}

/// Build a ConfigCmd message writing `patch` (a raw, opaque EffectDump
/// blob) to `slot` — see docs/PROTOCOL.md "02/02: write patch to memory".
/// `patch` must be exactly [`EFFECT_DUMP_LEN`] bytes.
pub fn encode_write_dump(slot: u8, patch: &[u8]) -> Result<Vec<u8>> {
    if patch.len() != EFFECT_DUMP_LEN {
        return Err(PodError::Protocol(format!(
            "patch data must be exactly {EFFECT_DUMP_LEN} bytes, got {}",
            patch.len()
        )));
    }
    // Byte +1 is 0x04 for this message specifically (documented exception
    // to the usual 0x00) — see docs/PROTOCOL.md.
    let mut message = common_header(
        MessageType::ConfigCmd as u8,
        0x04,
        CHANNEL_PATCH,
        config_cmd::WRITE_DUMP,
    )
    .to_vec();
    message.extend_from_slice(&(slot as u32).to_le_bytes());
    message.extend_from_slice(patch);
    Ok(encode_chunks(&message))
}

/// Build a `02`/`04` push of one tone's 2048-byte block into the edit
/// buffer (byte +1 is `02` for this message). The POD acks with `02`/`03`.
pub fn encode_push_tone(tone: u8, block: &[u8]) -> Result<Vec<u8>> {
    if block.len() != TONE_BLOCK_LEN {
        return Err(PodError::Protocol(format!(
            "tone block must be exactly {TONE_BLOCK_LEN} bytes, got {}",
            block.len()
        )));
    }
    let mut message = common_header(
        MessageType::ConfigCmd as u8,
        0x02,
        CHANNEL_PATCH,
        config_cmd::PUSH_TONE,
    )
    .to_vec();
    message.extend_from_slice(&(tone as u32).to_le_bytes());
    message.extend_from_slice(block);
    Ok(encode_chunks(&message))
}

/// Build a `04`/`20` device-wide setting set: u32 0, u32 setting id, u32
/// value. Gearbox sends it on channel `00` for UI changes and on
/// [`CHANNEL_PATCH`] after loading a patch.
pub fn encode_device_setting(channel: u8, id: u32, value: u32) -> Vec<u8> {
    let mut message = common_header(
        MessageType::IntParam12 as u8,
        0x00,
        channel,
        int_set::DEVICE_SETTING,
    )
    .to_vec();
    message.extend_from_slice(&0u32.to_le_bytes());
    message.extend_from_slice(&id.to_le_bytes());
    message.extend_from_slice(&value.to_le_bytes());
    encode_chunks(&message)
}

/// Build a `02`/`21` query for a device-wide setting, on the live channel
/// as Gearbox sends it. The POD answers `04`/`22`.
pub fn encode_query(id: u32) -> Vec<u8> {
    let mut message = common_header(
        MessageType::ConfigCmd as u8,
        0x00,
        CHANNEL_LIVE,
        config_cmd::QUERY,
    )
    .to_vec();
    message.extend_from_slice(&id.to_le_bytes());
    encode_chunks(&message)
}

/// Parse a reassembled EffectDump reply (type `0x01`, sub `0x01`): an
/// 8-byte header followed by [`EFFECT_DUMP_LEN`] bytes of opaque patch data.
/// `message` is a full message as returned by [`ChunkReassembler`] (chunk
/// framing already stripped).
pub fn decode_effect_dump(message: &[u8]) -> Result<&[u8]> {
    const HEADER_LEN: usize = 8;
    if message.len() != HEADER_LEN + EFFECT_DUMP_LEN {
        return Err(PodError::Protocol(format!(
            "expected {}-byte EffectDump reply, got {} bytes",
            HEADER_LEN + EFFECT_DUMP_LEN,
            message.len()
        )));
    }
    if message[0] != MessageType::EffectDump as u8 || message[7] != config_cmd::EFFECT_DUMP_REPLY {
        return Err(PodError::Protocol(format!(
            "expected EffectDump reply (type=0x01 sub=0x01), got type={:#04x} sub={:#04x}",
            message[0], message[7]
        )));
    }
    Ok(&message[HEADER_LEN..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_float_param_matches_confirmed_example() {
        // Confirmed example from docs/PROTOCOL.md: index 5, value 1.0.
        let expected: Vec<u8> = vec![
            0x1C, 0x00, 0x01, 0x00, 0x06, 0x00, 0x0A, 0x40, 0x01, 0x03, 0x00, 0x15, 0x01, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, 0x05, 0x00, 0x10, 0x3F,
            0x00, 0x00, 0x80, 0x3F,
        ];
        assert_eq!(encode_float_param(5, 1.0), expected);
    }

    #[test]
    fn encode_float_set_matches_encode_float_param_for_the_confirmed_case() {
        // Tone 2 (index8 = 1 in the confirmed template), amp volume knob.
        assert_eq!(
            encode_float_set(1, 0x0000, 0x0003, 5, namespace::NORMAL, 1.0),
            encode_float_param(5, 1.0)
        );
    }

    #[test]
    fn reassembler_strips_chunk_header_from_single_chunk_message() {
        let mut r = ChunkReassembler::new();
        let packet = encode_float_param(5, 1.0);
        let result = r.push(&packet).unwrap();
        assert_eq!(result, packet[4..].to_vec());
    }

    #[test]
    fn encode_chunks_matches_confirmed_4108_byte_write_shape() {
        // docs/PROTOCOL.md: "a 4108-byte patch write is 16 chunks of 0xFC
        // plus one of 0x4C (4032 + 76)".
        let message = vec![0xAB; 4108];
        let framed = encode_chunks(&message);

        let mut offset = 0;
        let mut chunk_lengths = Vec::new();
        while offset < framed.len() {
            let header = PacketHeader::parse(&framed[offset..]).unwrap();
            chunk_lengths.push(header.contents_length);
            offset += CHUNK_HEADER_LEN + header.contents_length as usize;
        }

        let mut expected = vec![0xFC; 16];
        expected.push(0x4C);
        assert_eq!(chunk_lengths, expected);
        assert_eq!(offset, framed.len());
    }

    #[test]
    fn chunk_reassembler_roundtrips_multi_chunk_message_fed_as_64_byte_packets() {
        let message: Vec<u8> = (0..4108u32).map(|i| (i % 256) as u8).collect();
        let framed = encode_chunks(&message);

        let mut reassembler = ChunkReassembler::new();
        let mut result = Vec::new();
        for packet in framed.chunks(BULK_PACKET_LEN) {
            result.extend(reassembler.push(packet).unwrap());
        }
        assert_eq!(result, message);
    }

    #[test]
    fn chunk_reassembler_handles_feeds_not_aligned_to_packet_boundaries() {
        let message: Vec<u8> = (0..600u32).map(|i| (i % 256) as u8).collect();
        let framed = encode_chunks(&message);

        let mut reassembler = ChunkReassembler::new();
        let mut result = Vec::new();
        for packet in framed.chunks(7) {
            result.extend(reassembler.push(packet).unwrap());
        }
        assert_eq!(result, message);
    }

    #[test]
    fn chunk_reassembler_does_not_stop_early_on_a_short_non_final_chunk() {
        // Confirmed against real hardware: the device's EffectDump reply
        // opens with a short (24-byte) chunk and then keeps sending.
        let mut message = vec![4u8; 24];
        message.extend(vec![5u8; 60]);
        message.extend(vec![6u8; 60]);

        let mut framed = vec![24, 0x00, FLAG_FIRST, 0x00];
        framed.extend_from_slice(&message[0..24]);
        framed.extend([60, 0x00, FLAG_CONTINUATION, 0x00]);
        framed.extend_from_slice(&message[24..84]);
        framed.extend([60, 0x00, FLAG_CONTINUATION, 0x00]);
        framed.extend_from_slice(&message[84..144]);

        let mut reassembler = ChunkReassembler::new();
        let mut result = Vec::new();
        for packet in framed.chunks(64) {
            result.extend(reassembler.push(packet).unwrap());
        }
        assert_eq!(result, message);
    }

    #[test]
    fn encode_request_dump_matches_expected_bytes() {
        let framed = encode_request_dump(0x10);
        let expected_message: Vec<u8> = vec![
            0x02, 0x00, 0x0A, 0x40, 0x02, 0x03, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00,
        ];
        assert_eq!(framed, encode_chunks(&expected_message));
    }

    #[test]
    fn encode_tone_setting_matches_prior_art_input_set() {
        // andree182/podx3 setguitarmic(): tone 0, int, param 0x16 = 2.
        let msg = encode_tone_setting(0, 0x16, ToneValue::Int(2));
        assert_eq!(
            msg,
            [
                0x18, 0x00, 0x01, 0x00, 0x05, 0x00, 0x0A, 0x40, 0x01, 0x03, 0x00, 0x16, 0, 0, 0, 0,
                0, 0, 0, 0, 0x16, 0, 0, 0, 2, 0, 0, 0
            ]
        );
    }

    #[test]
    fn encode_block_move_matches_delay_post_to_pre() {
        // docs/PROTOCOL.md: Delay post -> pre was 4/5 -> 5/2.
        let msg = encode_block_move(0, 4, 5, 5, 2);
        assert_eq!(
            msg[4..],
            [0x04, 0x00, 0x0A, 0x40, 0x01, 0x03, 0x00, 0x12, 0, 0, 0, 0, 4, 0, 5, 0, 5, 0, 2, 0]
        );
    }

    #[test]
    fn encode_device_setting_matches_gearbox_patch_load() {
        // 041-hw-load-5a: "select Tone 1" sent after the tone pushes.
        let msg = encode_device_setting(CHANNEL_PATCH, setting::SELECTED_TONE, 0);
        assert_eq!(
            msg[4..],
            [0x04, 0x00, 0x0A, 0x40, 0x02, 0x03, 0x00, 0x20, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0]
        );
    }

    #[test]
    fn encode_query_matches_gearbox() {
        let msg = encode_query(setting::OUTPUTS);
        assert_eq!(
            msg[4..],
            [0x02, 0x00, 0x0A, 0x40, 0x01, 0x03, 0x00, 0x21, 7, 0, 0, 0]
        );
    }

    #[test]
    fn encode_push_tone_header_and_length() {
        let msg = encode_push_tone(1, &[0u8; TONE_BLOCK_LEN]).unwrap();
        let mut r = ChunkReassembler::new();
        let body = r.push(&msg).unwrap();
        assert_eq!(body.len(), 12 + TONE_BLOCK_LEN);
        assert_eq!(
            body[..12],
            [0x02, 0x02, 0x0A, 0x40, 0x02, 0x03, 0x00, 0x04, 1, 0, 0, 0]
        );
        assert!(encode_push_tone(0, &[0u8; 10]).is_err());
    }

    #[test]
    fn encode_select_slot_matches_expected_bytes() {
        let framed = encode_select_slot(0x1F);
        let expected_message: Vec<u8> = vec![
            0x02, 0x00, 0x0A, 0x40, 0x02, 0x03, 0x00, 0x27, 0x1F, 0x00, 0x00, 0x00,
        ];
        assert_eq!(framed, encode_chunks(&expected_message));
    }

    #[test]
    fn encode_write_dump_matches_expected_shape() {
        let patch = vec![0x42; EFFECT_DUMP_LEN];
        let framed = encode_write_dump(0x1F, &patch).unwrap();
        let mut expected_message: Vec<u8> = vec![
            0x02, 0x04, 0x0A, 0x40, 0x02, 0x03, 0x00, 0x02, 0x1F, 0x00, 0x00, 0x00,
        ];
        expected_message.extend_from_slice(&patch);
        assert_eq!(expected_message.len(), 4108);
        assert_eq!(framed, encode_chunks(&expected_message));
    }

    #[test]
    fn encode_write_dump_rejects_wrong_length() {
        let result = encode_write_dump(0, &[0u8; 100]);
        assert!(matches!(result, Err(PodError::Protocol(_))));
    }

    #[test]
    fn decode_effect_dump_extracts_patch_bytes() {
        let mut message = vec![0x01, 0x04, 0x0A, 0x03, 0x02, 0x40, 0x00, 0x01];
        let patch = vec![0x7A; EFFECT_DUMP_LEN];
        message.extend_from_slice(&patch);
        assert_eq!(decode_effect_dump(&message).unwrap(), patch.as_slice());
    }

    #[test]
    fn decode_effect_dump_rejects_wrong_length() {
        assert!(decode_effect_dump(&[0x01, 0x04, 0x0A, 0x03, 0x02, 0x40, 0x00, 0x01]).is_err());
    }

    #[test]
    fn decode_effect_dump_rejects_wrong_type() {
        let mut message = vec![0x02, 0x04, 0x0A, 0x03, 0x02, 0x40, 0x00, 0x01];
        message.extend_from_slice(&[0u8; EFFECT_DUMP_LEN]);
        assert!(decode_effect_dump(&message).is_err());
    }

    #[test]
    fn encode_int_set_block_enabled_matches_expected_bytes() {
        // Gate block, tone 1, slot 0x00 group 0x02, turned on.
        let framed = encode_int_set(0, int_set::BLOCK_ENABLED, 0x00, 0x02, 1);
        let expected_message: Vec<u8> = vec![
            0x04, 0x00, 0x0A, 0x40, 0x01, 0x03, 0x00, 0x13, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x02, 0x00, 0x01, 0x00, 0x00, 0x00,
        ];
        assert_eq!(framed, encode_chunks(&expected_message));
    }
}
