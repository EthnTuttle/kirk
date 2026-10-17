//! Domain type contract. Orchestrator-owned; workers must not change
//! signatures without a review pass. All types are serde + Clone + Eq.

use serde::{Deserialize, Serialize};

// ---------- Typing ----------

/// The Seven Heavenly Virtues: the game's "types" for Champions and Rites.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Virtue {
    Humility,
    Charity,
    Chastity,
    Diligence,
    Patience,
    Temperance,
    Kindness,
}

impl Virtue {
    pub const ALL: [Virtue; 7] = [
        Virtue::Humility,
        Virtue::Charity,
        Virtue::Chastity,
        Virtue::Diligence,
        Virtue::Patience,
        Virtue::Temperance,
        Virtue::Kindness,
    ];
    /// The sin this virtue conquers (2x effectiveness).
    pub fn opposed_sin(self) -> Sin {
        match self {
            Virtue::Humility => Sin::Pride,
            Virtue::Charity => Sin::Envy,
            Virtue::Chastity => Sin::Lust,
            Virtue::Diligence => Sin::Sloth,
            Virtue::Patience => Sin::Wrath,
            Virtue::Temperance => Sin::Gluttony,
            Virtue::Kindness => Sin::Greed,
        }
    }
}

/// The Seven Deadly Sins: typing of hostile sin-spawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Sin {
    Pride,
    Envy,
    Lust,
    Sloth,
    Wrath,
    Gluttony,
    Greed,
}

/// Creature typing: Champions bear virtues (up to two), sin-spawn bear one
/// Sin, historical adversaries bear Valor ("armed honor", off the chart at
/// flat 1x both ways; docs/HISTORICAL-FOES.md section 4).
///
/// Serde constraint: `Valor` is forwards-only — a build predating this arm
/// fails deserialization on `"Valor"`; old builds never meet it because
/// instances serialize `species_id` only (docs/HISTORICAL-FOES.md section 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Typing {
    Virtues([Option<Virtue>; 2]),
    Sin(Sin),
    Valor,
}

/// Type of a Rite. `Typeless` = never STAB, chart 1x.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiteType {
    Typeless,
    Virtue(Virtue),
    Sin(Sin),
}

// ---------- Stats ----------

/// The five combat stats other than Resolve (used for Callings/boosts).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BattleStat {
    Valour,
    Piety,
    Guard,
    Aegis,
    Alacrity,
}

/// Six base stats. Resolve = staying power (HP analogue).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stats {
    pub resolve: u16,
    pub valour: u16,
    pub piety: u16,
    pub guard: u16,
    pub aegis: u16,
    pub alacrity: u16,
}

/// Per-instance "Grace" (IV analogue), 0..=15 each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraceStats {
    pub resolve: u8,
    pub valour: u8,
    pub piety: u8,
    pub guard: u8,
    pub aegis: u8,
    pub alacrity: u8,
}

/// Per-instance "Devotion" (EV analogue), 0..=63 each, sum <= 130.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevotionStats {
    pub resolve: u8,
    pub valour: u8,
    pub piety: u8,
    pub guard: u8,
    pub aegis: u8,
    pub alacrity: u8,
}

pub const MAX_LEVEL: u8 = 50;
pub const GRACE_MAX: u8 = 15;
pub const DEVOTION_MAX: u8 = 63;
pub const DEVOTION_TOTAL_MAX: u16 = 130;

// ---------- Rites (moves) ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RiteCategory {
    /// Uses attacker's Valour vs defender's Guard.
    Martial,
    /// Uses attacker's Piety vs defender's Aegis.
    Spiritual,
    /// No damage; effects only.
    Status,
}

/// Persistent ailments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ailment {
    /// burn: chips 1/16 max RES per turn, halves Martial damage dealt.
    Brand,
    /// paralysis: 25% full-stop, Alacrity halved.
    Shackled,
    /// poison: chips 1/8 max RES per turn.
    Venom,
    /// sleep: 1-3 turns unable to act.
    Slumber,
    /// freeze: 20% thaw chance per turn, unable to act.
    Frost,
}

/// Volatile (non-persistent) conditions; cleared on switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Volatile {
    Confusion, // 2-4 action attempts, 33% self-hit at power 40 typeless
    Flinch,    // this turn only
    SeedLeech, // drains 1/16 to opponent per turn
}

/// Secondary effect attached to a Rite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Secondary {
    Ailment {
        ailment: Ailment,
        chance_pct: u8,
    },
    Volatile {
        volatile_: Volatile,
        chance_pct: u8,
    },
    /// +/- stages (-6..=6) on a stat of the *target*.
    StageOnTarget {
        stat: BattleStat,
        delta: i8,
        chance_pct: u8,
    },
    /// +/- stages on the *user*.
    StageOnSelf {
        stat: BattleStat,
        delta: i8,
        chance_pct: u8,
    },
    /// Heal user for pct of max Resolve.
    HealSelf {
        pct: u8,
    },
    /// Drain pct of damage dealt.
    Drain {
        pct: u8,
    },
    /// Recoil pct of damage dealt.
    Recoil {
        pct: u8,
    },
    /// Multi-hit 2..=5 times.
    MultiHit,
    /// Increased crit stage (+1).
    HighCrit,
}

// NOTE: Rite/SpeciesBase/Calling are static table entries; they are never
// serialized (transcripts store ids only), so no serde derives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rite {
    pub id: u16,
    pub name: &'static str,
    pub rite_type: RiteType,
    pub category: RiteCategory,
    /// 0 for status rites.
    pub power: u8,
    /// 101 = never misses.
    pub accuracy: u8,
    /// Zeal points (PP analogue).
    pub zeal: u8,
    pub priority: i8,
    pub secondaries: &'static [Secondary],
}

// ---------- Species & instances ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LearnEntry {
    pub level: u8,
    pub rite_id: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeciesBase {
    pub id: u16,
    pub name: &'static str,
    /// Champion: Typing::Virtues([Some(v), maybe Second]); sin-spawn: Typing::Sin.
    pub typing: Typing,
    pub base: Stats,
    pub learnset: &'static [LearnEntry],
    /// xp needed to reach level n is `XP_SLOPE * n^3 / XP_DIV`.
    pub xp_slope: u32,
    pub xp_div: u32,
    /// Flavour line for the Hagiography.
    pub lore: &'static str,
    /// Conversion hardness 1..=100 (docs/CONVERSION.md section 3). None =
    /// never convertible (champions, sin-spawn, named commanders).
    pub obstinacy: Option<u8>,
    /// Entrenchment tier 0..=3 (docs/CONVERSION.md section 6). None for
    /// champions and sin-spawn.
    pub sin_depth: Option<u8>,
}

/// A Calling (nature analogue): +10% one stat, -10% another; None==neutral.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Calling {
    pub id: u8,
    pub name: &'static str,
    pub inc: Option<BattleStat>,
    pub dec: Option<BattleStat>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiteSlot {
    pub rite_id: u16,
    pub zeal_left: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChampionInstance {
    pub species_id: u16,
    pub level: u8,
    pub xp: u32,
    pub grace: GraceStats,
    pub devotion: DevotionStats,
    pub calling_id: u8,
    pub rites: [Option<RiteSlot>; 4],
    pub current_resolve: u16,
    pub ailment: Option<Ailment>,
    /// Baptized adversary (docs/CONVERSION.md section 4). Additive with
    /// serde default: old saves deserialize as false.
    #[serde(default)]
    pub converted: bool,
    /// Species the convert was called from; absent pre-M7
    /// (docs/CONVERSION.md section 7).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub converted_from: Option<u16>,
    /// Instance-level battle typing (docs/CONVERSION.md section 4): a
    /// baptized convert fights as Humility while his species table entry
    /// stays Valor. `None` (the serde default) resolves to the species'
    /// static typing; species-level reads (encounter tables, UI type
    /// badges for unconverted) never come through here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typing_override: Option<Typing>,
}

/// Stat mathematics (exact integer formula, no floats):
///
/// resolve = ((2*base + grace + devotion/4) * level) / 50 + level + 10
/// other   = (((2*base + grace + devotion/4) * level) / 50 + 5) * calling_pct / 100
///   where calling_pct = 110 / 100 / 90 for inc/neutral/dec.
/// All divisions are integer floor. Implemented in `data` (static helpers) so
/// the engine and UI share one implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputedStats {
    pub max_resolve: u16,
    pub valour: u16,
    pub piety: u16,
    pub guard: u16,
    pub aegis: u16,
    pub alacrity: u16,
}

/// Validate an instance against species data. This checks BATTLE-ENTRY
/// legality: levels, grace/devotion caps, learnset legality, rites carrying
/// full zeal, and resolve that is full-or-partial-but-nonzero (the
/// `current_resolve > 0` half is enforced by `Battle::new` via
/// [`InstanceError::NotBattleReady`]). Mid-campaign persistence (wounded
/// champions, spent zeal) uses partial validation elsewhere.
pub fn validate_instance(
    _c: &ChampionInstance,
    _species: &SpeciesBase,
) -> Result<(), InstanceError> {
    if _c.level == 0 || _c.level > MAX_LEVEL {
        return Err(InstanceError::BadLevel(_c.level));
    }
    for g in [
        _c.grace.resolve,
        _c.grace.valour,
        _c.grace.piety,
        _c.grace.guard,
        _c.grace.aegis,
        _c.grace.alacrity,
    ] {
        if g > GRACE_MAX {
            return Err(InstanceError::BadGrace(g));
        }
    }
    let devotions = [
        _c.devotion.resolve,
        _c.devotion.valour,
        _c.devotion.piety,
        _c.devotion.guard,
        _c.devotion.aegis,
        _c.devotion.alacrity,
    ];
    for d in devotions {
        if d > DEVOTION_MAX {
            return Err(InstanceError::BadDevotion(d));
        }
    }
    let devotion_total: u16 = devotions.iter().map(|&d| d as u16).sum();
    if devotion_total > DEVOTION_TOTAL_MAX {
        return Err(InstanceError::BadDevotionTotal(devotion_total));
    }
    for slot in _c.rites.iter().flatten() {
        let learnable = _species
            .learnset
            .iter()
            .any(|e| e.rite_id == slot.rite_id && e.level <= _c.level);
        let max_zeal = crate::data::rite_by_id(slot.rite_id).map(|r| r.zeal);
        if !learnable || max_zeal != Some(slot.zeal_left) {
            return Err(InstanceError::IllegalRite(slot.rite_id));
        }
    }
    if _c.current_resolve > crate::data::compute_stats(_c, _species).max_resolve {
        return Err(InstanceError::ResolveOverflow);
    }
    Ok(())
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum InstanceError {
    #[error("level {0} out of range 1..={MAX_LEVEL}")]
    BadLevel(u8),
    #[error("grace value {0} exceeds {GRACE_MAX}")]
    BadGrace(u8),
    #[error("devotion value {0} exceeds {DEVOTION_MAX}")]
    BadDevotion(u8),
    #[error("devotion total {0} exceeds {DEVOTION_TOTAL_MAX}")]
    BadDevotionTotal(u16),
    #[error("rite {0} not in learnset at this level")]
    IllegalRite(u16),
    #[error("resolve exceeds computed maximum")]
    ResolveOverflow,
    #[error("current_resolve is 0")]
    NotBattleReady,
}
