//! An in-memory [`PodBackend`] with fake but realistic patches, for
//! developing and demoing the GUI without a connected POD X3.

use std::collections::BTreeMap;

use crate::amp::{AmpParams, GUITAR_AMPS};
use crate::backend::{BackendError, PodBackend, Result, ToneSelect};
use crate::cab::GUITAR_CABS;
use crate::effect::{
    EffectInstance, EffectPosition, DELAY_EFFECTS, MODULATION_EFFECTS, REVERB_EFFECTS,
    STOMP_EFFECTS,
};
use crate::patch::{Patch, PatchLocation, PatchSlot};
use crate::tone::Tone;

fn location(bank: u8, slot: PatchSlot) -> PatchLocation {
    PatchLocation { bank, slot }
}

fn find_amp(name: &str) -> crate::amp::AmpModel {
    GUITAR_AMPS
        .iter()
        .find(|a| a.name.as_ref() == name)
        .cloned()
        .expect("seed amp name must exist in catalog")
}

fn find_cab(name: &str) -> crate::cab::CabModel {
    GUITAR_CABS
        .iter()
        .find(|c| c.name.as_ref() == name)
        .cloned()
        .expect("seed cab name must exist in catalog")
}

fn seed_patches() -> BTreeMap<PatchLocation, Patch> {
    let mut patches = BTreeMap::new();

    let clean = Patch {
        location: location(1, PatchSlot::A),
        name: "Surf's Up".into(),
        tone_a: Tone {
            amp: find_amp("Surfer Clean"),
            amp_params: AmpParams {
                reverb: 0.4,
                ..Default::default()
            },
            cab: find_cab("2x12 '65 Blackface"),
            effects: vec![EffectInstance {
                model: REVERB_EFFECTS[0].clone(),
                position: EffectPosition::PostAmp,
                enabled: true,
                params: vec![0.3, 0.5],
            }],
        },
        tone_b: None,
        tone_blend: 0.0,
    };
    patches.insert(clean.location, clean);

    let crunch = Patch {
        location: location(1, PatchSlot::B),
        name: "Garage Crunch".into(),
        tone_a: Tone {
            amp: find_amp("Crunch"),
            amp_params: AmpParams {
                drive: 0.65,
                ..Default::default()
            },
            cab: find_cab("4x12 '78 Brit Celest T-75s"),
            effects: vec![EffectInstance {
                model: STOMP_EFFECTS[2].clone(),
                position: EffectPosition::PreAmp,
                enabled: true,
                params: vec![0.5, 0.5, 0.4],
            }],
        },
        tone_b: None,
        tone_blend: 0.0,
    };
    patches.insert(crunch.location, crunch);

    let dual = Patch {
        location: location(1, PatchSlot::C),
        name: "Clean/Lead Split".into(),
        tone_a: Tone {
            amp: find_amp("Clean"),
            amp_params: AmpParams::default(),
            cab: find_cab("2x12 '65 Blackface"),
            effects: vec![EffectInstance {
                model: MODULATION_EFFECTS[0].clone(),
                position: EffectPosition::PostAmp,
                enabled: true,
                params: vec![0.3, 0.4],
            }],
        },
        tone_b: Some(Tone {
            amp: find_amp("Insane"),
            amp_params: AmpParams {
                drive: 0.85,
                mid: 0.6,
                ..Default::default()
            },
            cab: find_cab("4x12 '96 Brit Celest V30s"),
            effects: vec![EffectInstance {
                model: DELAY_EFFECTS[0].clone(),
                position: EffectPosition::PostAmp,
                enabled: true,
                params: vec![0.4, 0.35, 0.3],
            }],
        }),
        tone_blend: 0.5,
    };
    patches.insert(dual.location, dual);

    patches
}

pub struct MockBackend {
    patches: BTreeMap<PatchLocation, Patch>,
    current: PatchLocation,
}

impl Default for MockBackend {
    fn default() -> Self {
        let patches = seed_patches();
        let current = *patches.keys().next().expect("seed_patches is non-empty");
        Self { patches, current }
    }
}

impl PodBackend for MockBackend {
    fn list_patches(&self) -> Result<Vec<PatchLocation>> {
        Ok(self.patches.keys().copied().collect())
    }

    fn load_patch(&self, location: PatchLocation) -> Result<Patch> {
        self.patches
            .get(&location)
            .cloned()
            .ok_or(BackendError::PatchNotFound(location))
    }

    fn current_patch(&self) -> Result<Patch> {
        self.load_patch(self.current)
    }

    fn select_patch(&mut self, location: PatchLocation) -> Result<()> {
        if !self.patches.contains_key(&location) {
            return Err(BackendError::PatchNotFound(location));
        }
        self.current = location;
        Ok(())
    }

    fn save_patch(&mut self, patch: &Patch) -> Result<()> {
        self.patches.insert(patch.location, patch.clone());
        Ok(())
    }

    fn set_amp_params(&mut self, tone: ToneSelect, params: AmpParams) -> Result<()> {
        let patch = self
            .patches
            .get_mut(&self.current)
            .ok_or(BackendError::PatchNotFound(self.current))?;
        match tone {
            ToneSelect::A => patch.tone_a.amp_params = params,
            ToneSelect::B => {
                patch
                    .tone_b
                    .as_mut()
                    .ok_or(BackendError::NoToneB)?
                    .amp_params = params;
            }
        }
        Ok(())
    }

    fn set_tone_blend(&mut self, blend: f32) -> Result<()> {
        let patch = self
            .patches
            .get_mut(&self.current)
            .ok_or(BackendError::PatchNotFound(self.current))?;
        patch.tone_blend = blend.clamp(0.0, 1.0);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_at_least_one_dual_tone_and_one_single_tone_patch() {
        let backend = MockBackend::default();
        let locations = backend.list_patches().unwrap();
        assert!(locations.len() >= 3);

        let patches: Vec<_> = locations
            .iter()
            .map(|&loc| backend.load_patch(loc).unwrap())
            .collect();
        assert!(patches.iter().any(|p| p.is_dual_tone()));
        assert!(patches.iter().any(|p| !p.is_dual_tone()));
    }

    #[test]
    fn select_and_edit_current_patch() {
        let mut backend = MockBackend::default();
        let second = backend.list_patches().unwrap()[1];
        backend.select_patch(second).unwrap();
        assert_eq!(backend.current_patch().unwrap().location, second);

        let mut params = backend.current_patch().unwrap().tone_a.amp_params;
        params.drive = 0.9;
        backend.set_amp_params(ToneSelect::A, params).unwrap();
        assert_eq!(backend.current_patch().unwrap().tone_a.amp_params.drive, 0.9);
    }

    #[test]
    fn set_amp_params_on_missing_tone_b_errors() {
        let mut backend = MockBackend::default();
        let single_tone = backend
            .list_patches()
            .unwrap()
            .into_iter()
            .find(|&loc| !backend.load_patch(loc).unwrap().is_dual_tone())
            .unwrap();
        backend.select_patch(single_tone).unwrap();

        let result = backend.set_amp_params(ToneSelect::B, AmpParams::default());
        assert!(matches!(result, Err(BackendError::NoToneB)));
    }
}
