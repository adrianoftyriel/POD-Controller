use std::borrow::Cow;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Instrument {
    Guitar,
    Bass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AmpOrigin {
    /// A Line 6 original amp design (not modeled after a specific real amp).
    Line6Original,
    /// A model of a specific historical/vintage amplifier.
    Vintage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AmpModel {
    pub name: Cow<'static, str>,
    pub instrument: Instrument,
    pub origin: AmpOrigin,
}

/// Knob values common to every amp model on the X3 front panel.
///
/// Confirmed front-panel "Amp Control" knob set (Line6 legacy POD X3 page).
/// All values are normalized 0.0-1.0 here; real device parameter indices
/// and ranges are not yet mapped (see `docs/PROTOCOL.md`) — only Drive
/// (assumed to correspond to the confirmed "tone volume"-style float
/// parameter at index 5) has a tentative real-hardware mapping, and even
/// that is unconfirmed beyond the one example currently in `pod-core`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AmpParams {
    pub drive: f32,
    pub bass: f32,
    pub mid: f32,
    pub treble: f32,
    pub presence: f32,
    pub reverb: f32,
    pub channel_volume: f32,
}

impl Default for AmpParams {
    fn default() -> Self {
        Self {
            drive: 0.5,
            bass: 0.5,
            mid: 0.5,
            treble: 0.5,
            presence: 0.5,
            reverb: 0.0,
            channel_volume: 0.7,
        }
    }
}

/// **Partial** catalog of guitar amp models.
///
/// The X3 ships 79 guitar amp models total (per Line6's official models
/// list, kb.line6.com/pod-x3-series-models-list). Only the Line 6 original
/// designs are fully named here; the ~50 vintage-modeled amps are
/// represented by a handful of confirmed examples. Complete this list from
/// the official Advanced User Guide before treating it as exhaustive.
pub const GUITAR_AMPS: &[AmpModel] = &[
    amp("No Amp", Instrument::Guitar, AmpOrigin::Line6Original),
    amp("Agro", Instrument::Guitar, AmpOrigin::Line6Original),
    amp("Bayou", Instrument::Guitar, AmpOrigin::Line6Original),
    amp("Big Bottom", Instrument::Guitar, AmpOrigin::Line6Original),
    amp("Boutique #1", Instrument::Guitar, AmpOrigin::Line6Original),
    amp("Chemical X", Instrument::Guitar, AmpOrigin::Line6Original),
    amp("Chunk Chunk", Instrument::Guitar, AmpOrigin::Line6Original),
    amp("Class A", Instrument::Guitar, AmpOrigin::Line6Original),
    amp("Clean", Instrument::Guitar, AmpOrigin::Line6Original),
    amp("Crunch", Instrument::Guitar, AmpOrigin::Line6Original),
    amp("Fuzz", Instrument::Guitar, AmpOrigin::Line6Original),
    amp("Insane", Instrument::Guitar, AmpOrigin::Line6Original),
    amp("JTS-45", Instrument::Guitar, AmpOrigin::Line6Original),
    amp("Lunatic", Instrument::Guitar, AmpOrigin::Line6Original),
    amp("Modern Hi Gain", Instrument::Guitar, AmpOrigin::Line6Original),
    amp("Mood", Instrument::Guitar, AmpOrigin::Line6Original),
    amp("Octave", Instrument::Guitar, AmpOrigin::Line6Original),
    amp("Piezacoustic 2", Instrument::Guitar, AmpOrigin::Line6Original),
    amp("Purge", Instrument::Guitar, AmpOrigin::Line6Original),
    amp("Smash", Instrument::Guitar, AmpOrigin::Line6Original),
    amp("Sparkle", Instrument::Guitar, AmpOrigin::Line6Original),
    amp("Sparkle Clean", Instrument::Guitar, AmpOrigin::Line6Original),
    amp("Spinal Puppet", Instrument::Guitar, AmpOrigin::Line6Original),
    amp("Surfer Clean", Instrument::Guitar, AmpOrigin::Line6Original),
    amp("Surfer Sparkle", Instrument::Guitar, AmpOrigin::Line6Original),
    amp("Throttle", Instrument::Guitar, AmpOrigin::Line6Original),
    amp("Treadplate", Instrument::Guitar, AmpOrigin::Line6Original),
    amp("Tube Preamp", Instrument::Guitar, AmpOrigin::Line6Original),
    amp("Twain", Instrument::Guitar, AmpOrigin::Line6Original),
    amp("Variax Acoustic", Instrument::Guitar, AmpOrigin::Line6Original),
    // Confirmed vintage examples only — NOT an exhaustive list.
    amp("1964 Blackface 'Lux", Instrument::Guitar, AmpOrigin::Vintage),
    amp("1969 Brit Plexi Lead 200", Instrument::Guitar, AmpOrigin::Vintage),
    amp("1953 Small Tweed", Instrument::Guitar, AmpOrigin::Vintage),
];

/// **Partial** catalog of bass amp models (28 total per Line6).
pub const BASS_AMPS: &[AmpModel] = &[
    amp("Brit Invader", Instrument::Bass, AmpOrigin::Line6Original),
    amp("Classic Jazz", Instrument::Bass, AmpOrigin::Line6Original),
    amp("Doppleganger", Instrument::Bass, AmpOrigin::Line6Original),
    amp("Ebony Lux", Instrument::Bass, AmpOrigin::Line6Original),
    amp("Frankenstein", Instrument::Bass, AmpOrigin::Line6Original),
    amp("Sub Dub", Instrument::Bass, AmpOrigin::Line6Original),
    amp("Super Thor", Instrument::Bass, AmpOrigin::Line6Original),
    amp("1998 Adam & Eve", Instrument::Bass, AmpOrigin::Vintage),
    amp("1958 Tweed B-Man", Instrument::Bass, AmpOrigin::Vintage),
];

const fn amp(name: &'static str, instrument: Instrument, origin: AmpOrigin) -> AmpModel {
    AmpModel {
        name: Cow::Borrowed(name),
        instrument,
        origin,
    }
}
