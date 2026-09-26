//! Known parameter addresses for live editing — the `(slot, group, idx)`
//! tuples confirmed by capture in docs/PROTOCOL.md. Kept separate from
//! `protocol.rs` (pure wire framing) so new confirmed mappings can be added
//! here without touching the framing code.

/// The six amp knobs (docs/PROTOCOL.md "Float set (06/15): amp knobs").
/// All live at slot `0x00`, group `0x03` — the amp block's fixed address
/// (unlike effect blocks, the amp doesn't move between pre/post).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmpKnob {
    Bass,
    Middle,
    Treble,
    Drive,
    Presence,
    Volume,
}

impl AmpKnob {
    pub const SLOT: u16 = 0x00;
    pub const GROUP: u16 = 0x03;

    pub fn idx(self) -> u16 {
        match self {
            Self::Bass => 0,
            Self::Middle => 1,
            Self::Treble => 2,
            Self::Drive => 3,
            Self::Presence => 4,
            Self::Volume => 5,
        }
    }
}

impl std::str::FromStr for AmpKnob {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "bass" => Ok(Self::Bass),
            "middle" => Ok(Self::Middle),
            "treble" => Ok(Self::Treble),
            "drive" => Ok(Self::Drive),
            "presence" => Ok(Self::Presence),
            "volume" => Ok(Self::Volume),
            other => Err(format!("unknown amp knob: {other}")),
        }
    }
}

/// Blocks that can be toggled on/off with a live int-set
/// (docs/PROTOCOL.md "Int set (04/13): block on/off"). `slot`/`group` are
/// the block's address at its *default* chain position — a block moved
/// pre/post-amp (or reordered) will have a different current address, which
/// this table does not track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Block {
    Gate,
    Wah,
    Stomp,
    /// Sends the int-set to both slot `0x00` and `0x01` (amp + cab).
    Amp,
    Eq,
    Comp,
    Mod,
    Delay,
    Reverb,
}

impl Block {
    /// The `(slot, group)` pairs to address for this block. Usually one
    /// pair; [`Block::Amp`] is two.
    pub fn slot_group(self) -> &'static [(u16, u16)] {
        match self {
            Self::Gate => &[(0x00, 0x02)],
            Self::Wah => &[(0x02, 0x02)],
            Self::Stomp => &[(0x03, 0x02)],
            Self::Amp => &[(0x00, 0x03), (0x01, 0x03)],
            Self::Eq => &[(0x04, 0x03)],
            Self::Comp => &[(0x00, 0x05)],
            Self::Mod => &[(0x03, 0x05)],
            Self::Delay => &[(0x04, 0x05)],
            Self::Reverb => &[(0x05, 0x05)],
        }
    }
}

impl std::str::FromStr for Block {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "gate" => Ok(Self::Gate),
            "wah" => Ok(Self::Wah),
            "stomp" => Ok(Self::Stomp),
            "amp" => Ok(Self::Amp),
            "eq" => Ok(Self::Eq),
            "comp" => Ok(Self::Comp),
            "mod" => Ok(Self::Mod),
            "delay" => Ok(Self::Delay),
            "reverb" => Ok(Self::Reverb),
            other => Err(format!("unknown block: {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amp_knob_indices_match_documented_table() {
        assert_eq!(AmpKnob::Bass.idx(), 0);
        assert_eq!(AmpKnob::Middle.idx(), 1);
        assert_eq!(AmpKnob::Treble.idx(), 2);
        assert_eq!(AmpKnob::Drive.idx(), 3);
        assert_eq!(AmpKnob::Presence.idx(), 4);
        assert_eq!(AmpKnob::Volume.idx(), 5);
    }

    #[test]
    fn amp_knob_parses_case_insensitively() {
        assert_eq!("Volume".parse::<AmpKnob>().unwrap(), AmpKnob::Volume);
    }

    #[test]
    fn amp_knob_rejects_unknown_name() {
        assert!("wobble".parse::<AmpKnob>().is_err());
    }

    #[test]
    fn block_addresses_match_documented_table() {
        assert_eq!(Block::Gate.slot_group(), &[(0x00, 0x02)]);
        assert_eq!(Block::Amp.slot_group(), &[(0x00, 0x03), (0x01, 0x03)]);
        assert_eq!(Block::Reverb.slot_group(), &[(0x05, 0x05)]);
    }

    #[test]
    fn block_parses_case_insensitively() {
        assert_eq!("EQ".parse::<Block>().unwrap(), Block::Eq);
    }
}
