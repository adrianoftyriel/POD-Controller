//! Minimal decoding of fields inside an opaque EffectDump blob. Deliberately
//! narrow — full blob decoding (effect chain, block parameters, model
//! tables) is future work (README phase 4); this only pulls out the couple
//! of fields confirmed in docs/PROTOCOL.md that are useful without it.

/// Byte offset of Tone 1's name field within a decoded EffectDump.
pub const TONE1_NAME_OFFSET: usize = 0x00;
/// Byte offset of Tone 2's name field (each tone block is 0x800 bytes).
pub const TONE2_NAME_OFFSET: usize = 0x800;
/// Length of the (ASCII, space-padded) tone name field.
pub const TONE_NAME_LEN: usize = 16;

/// Read a tone name field (ASCII, space-padded) at `offset` within a
/// decoded EffectDump. Returns an empty string if `patch` is too short.
pub fn tone_name(patch: &[u8], offset: usize) -> String {
    let Some(bytes) = patch.get(offset..offset + TONE_NAME_LEN) else {
        return String::new();
    };
    String::from_utf8_lossy(bytes).trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tone_name_trims_trailing_padding() {
        // Real captured shape (docs/PROTOCOL.md): ASCII name, zero-padded.
        let mut patch = vec![0u8; 4096];
        let name_field = b"Direct San Diego"; // exactly TONE_NAME_LEN bytes
        patch[TONE1_NAME_OFFSET..TONE1_NAME_OFFSET + TONE_NAME_LEN].copy_from_slice(name_field);
        assert_eq!(tone_name(&patch, TONE1_NAME_OFFSET), "Direct San Diego");
    }

    #[test]
    fn tone_name_handles_short_input() {
        assert_eq!(tone_name(&[0u8; 4], TONE1_NAME_OFFSET), "");
    }
}
