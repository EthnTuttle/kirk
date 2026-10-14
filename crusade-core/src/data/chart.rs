//! Effectiveness wheel. Worker A implements per PLAN.md section 3:
//! virtue vs opposed sin = 200; each virtue resists its wheel neighbours (50)
//! and is weak to none among virtues except its two nemesis virtues (200);
//! Sin rites vs Champions: sin vs opposing virtue = 200, else 100; typeless = 100.

use crate::types::*;

pub fn effectiveness(att: RiteType, _def: DefenderTypes) -> u8 {
    match att {
        None => 100,
        Some(_v) => 100, // placeholder until wheel table lands
    }
}
