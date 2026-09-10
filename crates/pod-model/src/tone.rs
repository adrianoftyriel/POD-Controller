use serde::{Deserialize, Serialize};

use crate::amp::{AmpModel, AmpParams};
use crate::cab::CabModel;
use crate::effect::EffectInstance;

/// One of the two independently-configurable signal chains ("Tone A" /
/// "Tone B") that make up a Dual Tone patch. A single-tone patch just uses
/// `tone_a` and leaves `tone_b` unset on the parent [`crate::patch::Patch`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tone {
    pub amp: AmpModel,
    pub amp_params: AmpParams,
    pub cab: CabModel,
    pub effects: Vec<EffectInstance>,
}
