use crate::amp::AmpParams;
use crate::patch::{Patch, PatchLocation};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToneSelect {
    A,
    B,
}

#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("no device connected")]
    NotConnected,
    #[error("patch not found: {0:?}")]
    PatchNotFound(PatchLocation),
    #[error("patch has no Tone B (not a Dual Tone patch)")]
    NoToneB,
    #[error("backend error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, BackendError>;

/// The interface a GUI programs against, regardless of whether it's talking
/// to real hardware (`pod-core`'s `UsbBackend`, not yet implemented) or a
/// `MockBackend` for development without hardware. Keeping this trait
/// device-agnostic is what lets frontend work proceed before the USB
/// protocol is fully reverse-engineered.
pub trait PodBackend {
    /// All patch locations the device reports as populated.
    fn list_patches(&self) -> Result<Vec<PatchLocation>>;

    /// Load a patch from a given location into memory (does not necessarily
    /// make it the "current" live patch on the device).
    fn load_patch(&self, location: PatchLocation) -> Result<Patch>;

    /// The patch currently active/live on the device.
    fn current_patch(&self) -> Result<Patch>;

    /// Make the patch at `location` the current live patch.
    fn select_patch(&mut self, location: PatchLocation) -> Result<()>;

    /// Write `patch` to its own `location` in device storage.
    fn save_patch(&mut self, patch: &Patch) -> Result<()>;

    /// Live-update the amp knobs for one tone of the current patch.
    fn set_amp_params(&mut self, tone: ToneSelect, params: AmpParams) -> Result<()>;

    /// Live-update the Tone A/Tone B blend of the current patch.
    fn set_tone_blend(&mut self, blend: f32) -> Result<()>;
}
