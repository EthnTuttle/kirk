//! Callings table. Worker A fills all 25 (5 stats choose +,- incl. 5 neutral).

use crate::types::Calling;

pub const NEUTRAL: Calling = Calling {
    id: 0,
    name: "Steadfast",
    inc: None,
    dec: None,
};

pub static CALLINGS: &[Calling] = &[NEUTRAL];
