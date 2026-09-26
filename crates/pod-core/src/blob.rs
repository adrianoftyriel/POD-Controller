//! Reading and editing fields inside an EffectDump blob (4096 bytes: Tone 1's
//! 2048-byte block, then Tone 2's). Layout per docs/PROTOCOL.md "EffectDump
//! layout": a 0xE4-byte tone header, then 12 block records of 0x8C bytes.

use crate::error::{PodError, Result};

/// Byte offset of Tone 1's name field within a decoded EffectDump.
pub const TONE1_NAME_OFFSET: usize = 0x00;
/// Byte offset of Tone 2's name field (each tone block is 0x800 bytes).
pub const TONE2_NAME_OFFSET: usize = 0x800;
/// Length of the (ASCII, space-padded) tone name field.
pub const TONE_NAME_LEN: usize = 16;

/// Size of one tone's block.
pub const TONE_LEN: usize = 0x800;
/// Size of the tone header that precedes the block records.
pub const TONE_HEADER_LEN: usize = 0xE4;
/// Size of one block record.
pub const RECORD_LEN: usize = 0x8C;
/// Block records per tone.
pub const RECORD_COUNT: usize = 12;
/// Parameter records that fit in a block record (8 bytes each after +0x0C).
pub const MAX_PARAMS: usize = (RECORD_LEN - 0x0C) / 8;

/// The fixed record order within a tone block.
pub const RECORD_NAMES: [&str; RECORD_COUNT] = [
    "amp", "cab", "stomp", "mod", "delay", "reverb", "gate", "comp", "eq", "wah", "volume", "loop",
];

/// Read a tone name field (ASCII, space-padded) at `offset` within a
/// decoded EffectDump. Returns an empty string if `patch` is too short.
pub fn tone_name(patch: &[u8], offset: usize) -> String {
    let Some(bytes) = patch.get(offset..offset + TONE_NAME_LEN) else {
        return String::new();
    };
    String::from_utf8_lossy(bytes)
        .trim_end_matches([' ', '\0'])
        .to_string()
}

/// Set tone `tone`'s name: printable ASCII, at most 16 characters,
/// space-padded.
pub fn set_tone_name(patch: &mut [u8], tone: usize, name: &str) -> Result<()> {
    if !name.bytes().all(|b| (0x20..0x7F).contains(&b)) {
        return Err(PodError::Protocol(
            "tone names are limited to printable ASCII".into(),
        ));
    }
    if name.len() > TONE_NAME_LEN {
        return Err(PodError::Protocol(format!(
            "tone names are at most {TONE_NAME_LEN} characters"
        )));
    }
    let off = tone_offset(tone)?;
    check_len(patch)?;
    let field = &mut patch[off..off + TONE_NAME_LEN];
    field.fill(b' ');
    field[..name.len()].copy_from_slice(name.as_bytes());
    Ok(())
}

/// One parameter record: `<idx u16><namespace u16>` read together as
/// Gearbox's 32-bit parameter ID, and an f32 value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Param {
    pub id: u32,
    pub value: f32,
}

impl Param {
    pub fn idx(&self) -> u16 {
        self.id as u16
    }
    pub fn namespace(&self) -> u16 {
        (self.id >> 16) as u16
    }
}

/// A decoded block record.
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub model: u16,
    pub table: u8,
    pub category: u8,
    pub slot: u16,
    pub group: u16,
    pub enabled: bool,
    pub sync: u8,
    pub params: Vec<Param>,
}

fn check_len(patch: &[u8]) -> Result<()> {
    if patch.len() != 2 * TONE_LEN {
        return Err(PodError::Protocol(format!(
            "patch must be {} bytes, got {}",
            2 * TONE_LEN,
            patch.len()
        )));
    }
    Ok(())
}

fn tone_offset(tone: usize) -> Result<usize> {
    if tone > 1 {
        return Err(PodError::Protocol(format!("no tone {tone}")));
    }
    Ok(tone * TONE_LEN)
}

fn record_offset(tone: usize, index: usize) -> Result<usize> {
    if index >= RECORD_COUNT {
        return Err(PodError::Protocol(format!("no block record {index}")));
    }
    Ok(tone_offset(tone)? + TONE_HEADER_LEN + index * RECORD_LEN)
}

fn u16_at(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}

/// Decode block record `index` of tone `tone`.
pub fn record(patch: &[u8], tone: usize, index: usize) -> Result<Record> {
    check_len(patch)?;
    let r = &patch[record_offset(tone, index)?..][..RECORD_LEN];
    let count = (r[0x0B] as usize).min(MAX_PARAMS);
    let params = (0..count)
        .map(|p| {
            let off = 0x0C + p * 8;
            Param {
                id: u32::from_le_bytes(r[off..off + 4].try_into().expect("4 bytes")),
                value: f32::from_le_bytes(r[off + 4..off + 8].try_into().expect("4 bytes")),
            }
        })
        .collect();
    Ok(Record {
        model: u16_at(r, 0),
        table: r[2],
        category: r[3],
        slot: u16_at(r, 4),
        group: u16_at(r, 6),
        enabled: r[8] != 0,
        sync: r[9],
        params,
    })
}

/// Write `rec` back as block record `index` of tone `tone`. Bytes after the
/// last parameter are zeroed (the POD accepts stale bytes there, but they
/// only confuse diffs).
pub fn set_record(patch: &mut [u8], tone: usize, index: usize, rec: &Record) -> Result<()> {
    check_len(patch)?;
    if rec.params.len() > MAX_PARAMS {
        return Err(PodError::Protocol(format!(
            "a block holds at most {MAX_PARAMS} parameters"
        )));
    }
    let off = record_offset(tone, index)?;
    let r = &mut patch[off..off + RECORD_LEN];
    r[0..2].copy_from_slice(&rec.model.to_le_bytes());
    r[2] = rec.table;
    r[3] = rec.category;
    r[4..6].copy_from_slice(&rec.slot.to_le_bytes());
    r[6..8].copy_from_slice(&rec.group.to_le_bytes());
    r[8] = rec.enabled as u8;
    r[9] = rec.sync;
    r[0x0B] = rec.params.len() as u8;
    r[0x0C..].fill(0);
    for (p, param) in rec.params.iter().enumerate() {
        let o = 0x0C + p * 8;
        r[o..o + 4].copy_from_slice(&param.id.to_le_bytes());
        r[o + 4..o + 8].copy_from_slice(&param.value.to_le_bytes());
    }
    Ok(())
}

/// Set one parameter's value in a record, adding it if the record doesn't
/// have it yet.
pub fn set_param(patch: &mut [u8], tone: usize, index: usize, id: u32, value: f32) -> Result<()> {
    let mut rec = record(patch, tone, index)?;
    match rec.params.iter_mut().find(|p| p.id == id) {
        Some(p) => p.value = value,
        None => rec.params.push(Param { id, value }),
    }
    set_record(patch, tone, index, &rec)
}

/// Where a tone-level setting lives in the tone header, and how the live
/// `05`/`16` set addresses it (docs/PROTOCOL.md "Tone-level and global
/// controls").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToneSetting {
    pub key: &'static str,
    /// `05`/`16` parameter number.
    pub param: u32,
    /// Offset in the tone header.
    pub offset: usize,
    /// f32 at `offset` if true, else a u8.
    pub is_float: bool,
}

/// The tone-level settings the editor exposes.
pub const TONE_SETTINGS: &[ToneSetting] = &[
    ToneSetting {
        key: "variax_model",
        param: 0x00,
        offset: 0x28,
        is_float: false,
    },
    ToneSetting {
        key: "variax_tone",
        param: 0x01,
        offset: 0x29,
        is_float: false,
    },
    ToneSetting {
        key: "room",
        param: 0x19,
        offset: 0x44,
        is_float: true,
    },
    ToneSetting {
        key: "mic",
        param: 0x1A,
        offset: 0x52,
        is_float: false,
    },
    ToneSetting {
        key: "input",
        param: 0x16,
        offset: 0x51,
        is_float: false,
    },
];

pub fn tone_setting(key: &str) -> Option<&'static ToneSetting> {
    TONE_SETTINGS.iter().find(|s| s.key == key)
}

/// Read a tone-level setting as f64 (u8 settings widen losslessly).
pub fn get_setting(patch: &[u8], tone: usize, s: &ToneSetting) -> Result<f64> {
    check_len(patch)?;
    let off = tone_offset(tone)? + s.offset;
    Ok(if s.is_float {
        f32::from_le_bytes(patch[off..off + 4].try_into().expect("4 bytes")) as f64
    } else {
        patch[off] as f64
    })
}

pub fn set_setting(patch: &mut [u8], tone: usize, s: &ToneSetting, value: f64) -> Result<()> {
    check_len(patch)?;
    let off = tone_offset(tone)? + s.offset;
    if s.is_float {
        patch[off..off + 4].copy_from_slice(&(value as f32).to_le_bytes());
    } else {
        if !(0.0..=255.0).contains(&value) {
            return Err(PodError::Protocol(format!("{} out of range", s.key)));
        }
        patch[off] = value as u8;
    }
    Ok(())
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

    #[test]
    fn set_tone_name_pads_and_validates() {
        let mut patch = vec![0u8; 4096];
        set_tone_name(&mut patch, 1, "Lead").unwrap();
        assert_eq!(&patch[0x800..0x810], b"Lead            ");
        assert_eq!(tone_name(&patch, TONE2_NAME_OFFSET), "Lead");
        assert!(set_tone_name(&mut patch, 0, "seventeen chars!!").is_err());
        assert!(set_tone_name(&mut patch, 0, "caf\u{e9}").is_err());
    }

    /// Tone 1 delay record of dumps-all/00.bin ("Direct San Diego"):
    /// Tube Echo-style record at 4/5, sync 9, five params.
    fn sample() -> Vec<u8> {
        let mut patch = vec![0u8; 4096];
        let off = TONE_HEADER_LEN + 4 * RECORD_LEN;
        let r = &mut patch[off..off + RECORD_LEN];
        r[..12].copy_from_slice(&[7, 0, 2, 2, 4, 0, 5, 0, 1, 9, 0, 2]);
        r[12..20].copy_from_slice(&[0, 0, 0x10, 0x3F, 0, 0, 0, 0x3F]); // 3F100000 = 0.5
        r[20..28].copy_from_slice(&[1, 0, 0x01, 0x3F, 0, 0, 0x80, 0x3F]); // 3F010001 = 1.0
        patch
    }

    #[test]
    fn record_round_trips() {
        let mut patch = sample();
        let rec = record(&patch, 0, 4).unwrap();
        assert_eq!((rec.model, rec.table, rec.category), (7, 2, 2));
        assert_eq!(
            (rec.slot, rec.group, rec.enabled, rec.sync),
            (4, 5, true, 9)
        );
        assert_eq!(
            rec.params,
            vec![
                Param {
                    id: 0x3F10_0000,
                    value: 0.5
                },
                Param {
                    id: 0x3F01_0001,
                    value: 1.0
                }
            ]
        );
        assert_eq!(rec.params[1].idx(), 1);
        assert_eq!(rec.params[1].namespace(), 0x3F01);
        let before = patch.clone();
        set_record(&mut patch, 0, 4, &rec).unwrap();
        assert_eq!(patch, before);
    }

    #[test]
    fn set_param_updates_or_appends() {
        let mut patch = sample();
        set_param(&mut patch, 0, 4, 0x3F10_0000, 0.25).unwrap();
        set_param(&mut patch, 0, 4, 0x3F10_0003, 0.75).unwrap();
        let rec = record(&patch, 0, 4).unwrap();
        assert_eq!(rec.params.len(), 3);
        assert_eq!(rec.params[0].value, 0.25);
        assert_eq!(
            rec.params[2],
            Param {
                id: 0x3F10_0003,
                value: 0.75
            }
        );
    }

    #[test]
    fn settings_read_and_write_header_fields() {
        let mut patch = vec![0u8; 4096];
        let variax = tone_setting("variax_model").unwrap();
        set_setting(&mut patch, 1, variax, 12.0).unwrap();
        assert_eq!(patch[0x800 + 0x28], 12);
        let room = tone_setting("room").unwrap();
        set_setting(&mut patch, 0, room, 0.25).unwrap();
        assert_eq!(get_setting(&patch, 0, room).unwrap(), 0.25);
        assert!(set_setting(&mut patch, 0, variax, 300.0).is_err());
    }
}
