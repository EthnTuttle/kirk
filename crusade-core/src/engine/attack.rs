//! Damage calculation, secondaries, end-of-turn. Flow contract in `mod.rs`.

use super::*;

impl Battle {
    pub(crate) fn rite_of(&self, s: usize, slot: u8) -> Option<&'static Rite> {
        let r = self.sides[s].active().rites.get(slot as usize)?.as_ref()?;
        data::rite_by_id(r.rite_id)
    }

    /// staged stat; `halve_raw` applies Brand's martial halving before stages.
    pub(crate) fn staged_stat(&self, s: usize, stat: BattleStat, halve_raw: bool) -> u32 {
        let sd = &self.sides[s];
        let cs = sd.computed();
        let mut raw = match stat {
            BattleStat::Valour => cs.valour,
            BattleStat::Piety => cs.piety,
            BattleStat::Guard => cs.guard,
            BattleStat::Aegis => cs.aegis,
            BattleStat::Alacrity => cs.alacrity,
        };
        if halve_raw {
            raw /= 2;
        }
        let staged = raw as u32 * data::stage_pct(sd.active_state.stages[stat_idx(stat)]) / 100;
        staged.max(1)
    }

    /// Battle typing of side `s`'s active instance: the instance's
    /// `typing_override` (a baptized convert fights as Humility,
    /// docs/CONVERSION.md section 4), falling back to the species' static
    /// typing. Species-level reads (encounter tables, UI type badges for
    /// the unconverted) do not come through here.
    fn active_typing(&self, s: usize) -> Option<Typing> {
        let c = self.sides[s].active();
        c.typing_override
            .or_else(|| data::species_by_id(c.species_id).map(|b| b.typing))
    }

    pub(crate) fn stab_pct(&self, s: usize, rt: RiteType) -> u32 {
        match (self.active_typing(s), rt) {
            (Some(Typing::Virtues(vs)), RiteType::Virtue(v)) if vs.contains(&Some(v)) => 150,
            (Some(Typing::Sin(sd)), RiteType::Sin(sr)) if sd == sr => 150,
            _ => 100,
        }
    }

    pub(crate) fn exec_rite(
        &mut self,
        s: usize,
        slot: u8,
        rng: &mut BattleRng,
    ) -> Result<(), BattleError> {
        if self.winner.is_some() || self.pending_switch[s] {
            return Ok(());
        }
        // gate prevalidation errors before ailment gates: malformed actions
        // are caller bugs and must error deterministically.
        let (rite_id, zeal) = {
            let c = self.sides[s].active();
            match c.rites.get(slot as usize) {
                Some(Some(r)) => (r.rite_id, r.zeal_left),
                _ => return Err(BattleError::BadSlot(slot)),
            }
        };
        let rite = data::rite_by_id(rite_id).ok_or(BattleError::BadSlot(slot))?;
        if self.gates_block(s, rng) {
            return Ok(());
        }
        if zeal == 0 {
            return Err(BattleError::NoZeal);
        }
        {
            let c = self.sides[s].active_mut();
            let slot_ref = c.rites[slot as usize].as_mut().expect("validated");
            slot_ref.zeal_left -= 1;
        }
        self.emit(BattleEvent::RiteUsed {
            side: s as u8,
            rite_id,
        });
        let t = 1 - s;
        let hits = if rite.secondaries.contains(&Secondary::MultiHit) {
            rng.range_u8(2, 5)
        } else {
            1
        };
        for _ in 0..hits {
            if self.winner.is_some() || !self.sides[t].alive(self.sides[t].active_idx) {
                break;
            }
            if rng.pct() >= rite.accuracy {
                self.emit(BattleEvent::Missed { side: s as u8 });
                continue;
            }
            if rite.category == RiteCategory::Status {
                self.apply_secondaries(s, t, rite, 0, rng);
                break;
            }
            let def_typing = self.active_typing(t).expect("validated");
            let eff = data::effectiveness(rite.rite_type, def_typing);
            // reserved for future immunities; the current chart never yields 0
            if eff == 0 {
                self.emit(BattleEvent::Immune { side: s as u8 });
                continue;
            }
            let high_crit = rite.secondaries.contains(&Secondary::HighCrit);
            let crit = rng.range_u32(0, 23) < if high_crit { 2 } else { 1 };
            let lvl = self.sides[s].active().level as u32;
            let (atk, def) = match rite.category {
                RiteCategory::Martial => (BattleStat::Valour, BattleStat::Guard),
                _ => (BattleStat::Piety, BattleStat::Aegis),
            };
            let branded = rite.category == RiteCategory::Martial
                && self.sides[s].active().ailment == Some(Ailment::Brand);
            let a = self.staged_stat(s, atk, branded);
            let d = self.staged_stat(t, def, false);
            let power = rite.power as u32;
            let mut dmg = (((2 * lvl / 5 + 2) * power * a) / d) / 50 + 2;
            dmg = dmg * self.stab_pct(s, rite.rite_type) / 100;
            dmg = dmg * eff as u32 / 100;
            dmg = dmg * rng.range_u32(85, 100) / 100;
            dmg = dmg * if crit { 150 } else { 100 } / 100;
            if dmg == 0 {
                dmg = 1;
            }
            let amt = dmg.min(u16::MAX as u32) as u16;
            self.apply_damage(t, amt);
            self.emit(BattleEvent::Damage {
                side: t as u8,
                amount: amt,
                crit,
                effectiveness_pct: eff,
            });
            self.apply_secondaries(s, t, rite, amt, rng);
        }
        Ok(())
    }

    pub(crate) fn apply_secondaries(
        &mut self,
        s: usize,
        t: usize,
        rite: &Rite,
        dmg: u16,
        rng: &mut BattleRng,
    ) {
        // battle decided by the hit itself: no recoil/drain/ailments after
        // BattleEnded (recoil double-KO must not flip the winner).
        if self.winner.is_some() {
            return;
        }
        for sec in rite.secondaries {
            match *sec {
                Secondary::Ailment {
                    ailment,
                    chance_pct,
                } => {
                    if !rng.chance(chance_pct)
                        || !self.sides[t].alive(self.sides[t].active_idx)
                        || self.sides[t].active().ailment.is_some()
                    {
                        continue;
                    }
                    self.sides[t].active_mut().ailment = Some(ailment);
                    if ailment == Ailment::Slumber {
                        self.sides[t].active_state.slumber_turns = rng.range_u8(1, 3);
                    }
                    self.emit(BattleEvent::AilmentApplied {
                        side: t as u8,
                        ailment,
                    });
                }
                Secondary::Volatile {
                    volatile_,
                    chance_pct,
                } => {
                    if !rng.chance(chance_pct)
                        || !self.sides[t].alive(self.sides[t].active_idx)
                        || self.sides[t].has_volatile(volatile_)
                    {
                        continue;
                    }
                    let turns = match volatile_ {
                        Volatile::Confusion => rng.range_u8(2, 4),
                        // cleared at EOT / on switch
                        Volatile::Flinch | Volatile::SeedLeech => u8::MAX,
                    };
                    self.sides[t]
                        .active_state
                        .volatiles
                        .push((volatile_, turns));
                    self.emit(BattleEvent::VolatileApplied {
                        side: t as u8,
                        volatile_,
                    });
                }
                Secondary::StageOnTarget {
                    stat,
                    delta,
                    chance_pct,
                } => {
                    if !rng.chance(chance_pct) || !self.sides[t].alive(self.sides[t].active_idx) {
                        continue;
                    }
                    let st = &mut self.sides[t].active_state.stages[stat_idx(stat)];
                    *st = (*st + delta).clamp(-6, 6);
                    let new_stage = *st;
                    self.emit(BattleEvent::StageChanged {
                        side: t as u8,
                        stat,
                        new_stage,
                    });
                }
                Secondary::StageOnSelf {
                    stat,
                    delta,
                    chance_pct,
                } => {
                    if !rng.chance(chance_pct) || !self.sides[s].alive(self.sides[s].active_idx) {
                        continue;
                    }
                    let st = &mut self.sides[s].active_state.stages[stat_idx(stat)];
                    *st = (*st + delta).clamp(-6, 6);
                    let new_stage = *st;
                    self.emit(BattleEvent::StageChanged {
                        side: s as u8,
                        stat,
                        new_stage,
                    });
                }
                Secondary::HealSelf { pct } => {
                    if !self.sides[s].alive(self.sides[s].active_idx) {
                        continue;
                    }
                    let max = self.sides[s].computed().max_resolve as u32;
                    self.heal_active(s, (max * pct as u32 / 100) as u16);
                }
                Secondary::Drain { pct } => {
                    if dmg == 0 || !self.sides[s].alive(self.sides[s].active_idx) {
                        continue;
                    }
                    // u32 widening: dmg * pct can exceed u16::MAX.
                    self.heal_active(s, ((dmg as u32 * pct as u32) / 100) as u16);
                }
                Secondary::Recoil { pct } => {
                    if dmg == 0 || !self.sides[s].alive(self.sides[s].active_idx) {
                        continue;
                    }
                    let r = ((dmg as u32 * pct as u32 / 100).max(1)) as u16;
                    self.emit(BattleEvent::Damage {
                        side: s as u8,
                        amount: r,
                        crit: false,
                        effectiveness_pct: 100,
                    });
                    self.apply_damage(s, r);
                }
                Secondary::MultiHit | Secondary::HighCrit => {}
            }
        }
    }

    pub(crate) fn end_of_turn(&mut self, s: usize) {
        // flinch expires regardless of anything else
        self.sides[s]
            .active_state
            .volatiles
            .retain(|(k, _)| *k != Volatile::Flinch);
        if self.winner.is_some() || !self.sides[s].alive(self.sides[s].active_idx) {
            return;
        }
        let max = self.sides[s].computed().max_resolve;
        let t = 1 - s;
        match self.sides[s].active().ailment {
            Some(Ailment::Brand) => {
                let chip = (max / 16).max(1);
                self.emit(BattleEvent::AilmentChip {
                    side: s as u8,
                    amount: chip,
                });
                self.apply_damage(s, chip);
            }
            Some(Ailment::Venom) => {
                let chip = (max / 8).max(1);
                self.emit(BattleEvent::AilmentChip {
                    side: s as u8,
                    amount: chip,
                });
                self.apply_damage(s, chip);
            }
            _ => {}
        }
        if self.winner.is_some() || !self.sides[s].alive(self.sides[s].active_idx) {
            return;
        }
        if self.sides[s].has_volatile(Volatile::SeedLeech) {
            let drain = (max / 16).max(1);
            self.emit(BattleEvent::AilmentChip {
                side: s as u8,
                amount: drain,
            });
            self.apply_damage(s, drain);
            if self.winner.is_none() && self.sides[t].alive(self.sides[t].active_idx) {
                self.heal_active(t, drain);
            }
        }
    }

    // `s` legitimately indexes parallel arrays in the switch/yield loops.
    #[allow(clippy::needless_range_loop)]
    pub fn apply_turn(&mut self, a: Action, b: Action) -> Result<Vec<BattleEvent>, BattleError> {
        if self.winner.is_some() {
            return Err(BattleError::BattleOver);
        }
        if self.pending_switch[0] || self.pending_switch[1] {
            return Err(BattleError::BadSwitch);
        }
        let start = self.log.len();
        let mut rng = self.turn_rng();
        self.emit(BattleEvent::TurnStart { turn: self.turn });
        let actions = [a, b];
        for s in 0..2 {
            if actions[s] == Action::Yield {
                self.finish(1 - s as u8);
                return Ok(self.suffix(start));
            }
        }
        for s in 0..2 {
            if let Action::Switch { party_idx } = actions[s] {
                self.do_switch(s, party_idx as usize)?;
            }
        }
        let mut order: Vec<usize> = (0..2)
            .filter(|&s| matches!(actions[s], Action::Rite { .. }))
            .collect();
        if order.len() == 2 {
            let prio = |s: usize| -> i8 {
                match actions[s] {
                    Action::Rite { slot } => self.rite_of(s, slot).map(|r| r.priority).unwrap_or(0),
                    _ => 0,
                }
            };
            let (p0, p1) = (prio(0), prio(1));
            let al = |s: usize| {
                let raw = self.staged_stat(s, BattleStat::Alacrity, false);
                if self.sides[s].active().ailment == Some(Ailment::Shackled) {
                    raw / 2
                } else {
                    raw
                }
            };
            let side0_first = if p0 != p1 {
                p0 > p1
            } else {
                let (a0, a1) = (al(0), al(1));
                if a0 != a1 {
                    a0 > a1
                } else {
                    rng.chance(50)
                }
            };
            if !side0_first {
                order.swap(0, 1);
            }
        }
        for s in order {
            if self.winner.is_some() || self.pending_switch[0] || self.pending_switch[1] {
                break;
            }
            if let Action::Rite { slot } = actions[s] {
                self.exec_rite(s, slot, &mut rng)?;
            }
        }
        for s in 0..2 {
            if self.winner.is_some() {
                break;
            }
            self.end_of_turn(s);
        }
        if self.winner.is_none() {
            self.turn += 1;
        }
        Ok(self.suffix(start))
    }
}
