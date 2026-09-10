use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::amp::Instrument;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CabModel {
    pub name: Cow<'static, str>,
    pub instrument: Instrument,
}

/// **Partial** catalog of guitar speaker cabinet models (27 total per Line6).
pub const GUITAR_CABS: &[CabModel] = &[
    cab("No Cabinet", Instrument::Guitar),
    cab("1x6 60s Super O", Instrument::Guitar),
    cab("1x8 Tiny Tweed", Instrument::Guitar),
    cab("1x10 '59 Gibtone", Instrument::Guitar),
    cab("2x12 '65 Blackface", Instrument::Guitar),
    cab("4x12 '78 Brit Celest T-75s", Instrument::Guitar),
    cab("4x12 '96 Brit Celest V30s", Instrument::Guitar),
];

/// **Partial** catalog of bass cabinet models (23 total per Line6).
pub const BASS_CABS: &[CabModel] = &[
    cab("BASS-No Cabinet", Instrument::Bass),
    cab("BASS-1x12 Boutique", Instrument::Bass),
    cab("BASS-4x10 Line 6", Instrument::Bass),
    cab("BASS-4x15 Big Boy", Instrument::Bass),
    cab("BASS-8x10 Classic", Instrument::Bass),
];

const fn cab(name: &'static str, instrument: Instrument) -> CabModel {
    CabModel {
        name: Cow::Borrowed(name),
        instrument,
    }
}
