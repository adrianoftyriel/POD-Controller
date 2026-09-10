use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// The four effect footswitch categories on the X3 Live front panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectCategory {
    Stomp,
    Modulation,
    Delay,
    Reverb,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectModel {
    pub name: Cow<'static, str>,
    pub category: EffectCategory,
}

/// Where in the tone's signal chain an effect is inserted, relative to the
/// amp block. Confirmed by Line6 docs that pre- and post-amp placement is
/// possible; whether placement is freely reorderable per-slot or
/// constrained to fixed pre/post groups is not yet confirmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectPosition {
    PreAmp,
    PostAmp,
}

/// One effect instance placed in a tone's signal chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectInstance {
    pub model: EffectModel,
    pub position: EffectPosition,
    pub enabled: bool,
    /// Normalized 0.0-1.0 parameter values. Real per-effect parameter
    /// names/counts (e.g. a delay's Time/Repeats/Mix) are not yet
    /// confirmed against the official manual — see docs/PROTOCOL.md.
    pub params: Vec<f32>,
}

/// Up to 9 simultaneous effects per tone (confirmed by Line6 docs), split
/// across pre-amp and post-amp slots.
pub const MAX_EFFECTS_PER_TONE: usize = 9;

/// **Partial** catalog of Stomp-category effect models (33 total per Line6).
pub const STOMP_EFFECTS: &[EffectModel] = &[
    fx("Facial Fuzz", EffectCategory::Stomp),
    fx("Screamer", EffectCategory::Stomp),
    fx("Classic Distortion", EffectCategory::Stomp),
    fx("Red Comp", EffectCategory::Stomp),
    fx("Blue Comp", EffectCategory::Stomp),
    fx("Auto Wah", EffectCategory::Stomp),
    fx("Dingo Tron", EffectCategory::Stomp),
    fx("Seismik Synth", EffectCategory::Stomp),
];

/// **Partial** catalog of Modulation-category effect models (24 total per Line6).
pub const MODULATION_EFFECTS: &[EffectModel] = &[
    fx("Sine Chorus", EffectCategory::Modulation),
    fx("Line 6 Flanger", EffectCategory::Modulation),
    fx("Phaser", EffectCategory::Modulation),
    fx("U-Vibe", EffectCategory::Modulation),
    fx("Rotary Drum + Horn", EffectCategory::Modulation),
    fx("Auto Pan", EffectCategory::Modulation),
];

/// **Partial** catalog of Delay-category effect models (13 total per Line6).
pub const DELAY_EFFECTS: &[EffectModel] = &[
    fx("Analog Delay", EffectCategory::Delay),
    fx("Tube Echo", EffectCategory::Delay),
    fx("Multi-Head Delay", EffectCategory::Delay),
    fx("Digital Delay", EffectCategory::Delay),
    fx("Reverse Delay", EffectCategory::Delay),
];

/// **Partial** catalog of Reverb-category effect models (15 total per Line6).
pub const REVERB_EFFECTS: &[EffectModel] = &[
    fx("'Lux Spring", EffectCategory::Reverb),
    fx("Small Room", EffectCategory::Reverb),
    fx("Dark Hall", EffectCategory::Reverb),
    fx("Vintage Plate", EffectCategory::Reverb),
];

const fn fx(name: &'static str, category: EffectCategory) -> EffectModel {
    EffectModel {
        name: Cow::Borrowed(name),
        category,
    }
}
