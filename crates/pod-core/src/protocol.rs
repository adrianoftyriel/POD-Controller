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

/// The 4-byte header prepended to every bulk packet.
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

/// Confirmed-working template for a float parameter set message (tone
/// volume, parameter index 5), captured verbatim from a real device
/// interaction. Bytes at [`FLOAT_PARAM_INDEX_OFFSET`] (index) and the
/// trailing 4 bytes (value) are the only positions known to vary; every
/// other parameter's index and byte layout is still undiscovered — see
/// `docs/PROTOCOL.md` "What's genuinely unknown".
const FLOAT_PARAM_TEMPLATE: [u8; 24] = [
    0x06, 0x00, 0x0A, 0x40, 0x01, 0x03, 0x00, 0x15, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03,
    0x00, 0x01, 0x00, 0x00, 0x00, 0x05, 0x00, 0x10, 0x3F,
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
/// `docs/PROTOCOL.md`.
pub fn encode_float_param(param_index: u8, value: f32) -> Vec<u8> {
    let mut payload = FLOAT_PARAM_TEMPLATE.to_vec();
    payload[FLOAT_PARAM_INDEX_OFFSET] = param_index;
    payload.extend_from_slice(&value.to_le_bytes());

    let mut packet = Vec::with_capacity(4 + payload.len());
    packet.push(payload.len() as u8);
    packet.push(0x00);
    packet.push(FLAG_FIRST);
    packet.push(0x00);
    packet.extend_from_slice(&payload);
    packet
}

/// Reassembles bulk packets (each carrying a [`PacketHeader`]) into
/// complete message payloads.
#[derive(Default)]
pub struct PacketReassembler {
    buffer: Vec<u8>,
    expected_len: Option<usize>,
}

impl PacketReassembler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one raw bulk-IN packet. Returns `Some(payload)` once a full
    /// message has been reassembled.
    pub fn push(&mut self, packet: &[u8]) -> crate::error::Result<Option<Vec<u8>>> {
        let header = PacketHeader::parse(packet).ok_or_else(|| {
            crate::error::PodError::Protocol("bulk packet shorter than 4-byte header".into())
        })?;
        let body = &packet[4..];

        if header.flags & FLAG_FIRST != 0 {
            self.buffer.clear();
            self.expected_len = Some(header.contents_length as usize);
        }

        self.buffer.extend_from_slice(body);

        if let Some(expected) = self.expected_len {
            if self.buffer.len() >= expected {
                self.buffer.truncate(expected);
                self.expected_len = None;
                return Ok(Some(std::mem::take(&mut self.buffer)));
            }
        }
        Ok(None)
    }
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
    fn reassembler_handles_single_packet_message() {
        let mut r = PacketReassembler::new();
        let packet = encode_float_param(5, 1.0);
        let result = r.push(&packet).unwrap();
        assert_eq!(result, Some(packet[4..].to_vec()));
    }
}
