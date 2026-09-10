use serde::{Deserialize, Serialize};

use crate::tone::Tone;

/// A/B/C/D patch slot within a bank, matching the X3 Live's footswitches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PatchSlot {
    A,
    B,
    C,
    D,
}

/// A location in the device's patch storage: 32 banks x 4 slots (A-D) = 128
/// total patches (confirmed, kb.line6.com/pod-x3-series-presets-list).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PatchLocation {
    pub bank: u8, // 1..=32
    pub slot: PatchSlot,
}

/// A complete patch: one or two tones (Dual Tone), plus patch-level
/// metadata. Patch name character limit is not yet confirmed against the
/// official manual.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Patch {
    pub location: PatchLocation,
    pub name: String,
    pub tone_a: Tone,
    /// `Some` only for Dual Tone patches.
    pub tone_b: Option<Tone>,
    /// Tone A/B blend, controlled by the expression pedal or a footswitch.
    /// 0.0 = all Tone A, 1.0 = all Tone B.
    pub tone_blend: f32,
}

impl Patch {
    pub fn is_dual_tone(&self) -> bool {
        self.tone_b.is_some()
    }
}
