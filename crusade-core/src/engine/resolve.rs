//! Turn resolution. See `engine/mod.rs` for the flow contract.

use super::*;

impl Battle {
    pub(crate) fn emit(&mut self, e: BattleEvent) {
        self.log.push(e);
    }

    pub(crate) fn finish(&mut self, winner: u8) {
        // first faint decides the battle; later KOs (e.g. recoil in the same
        // rite) must not flip the winner or emit a second BattleEnded.
        if self.winner.is_some() {
            return;
        }
        self.winner = Some(winner);
        self.emit(BattleEvent::BattleEnded { winner });
    }

    pub(crate) fn suffix(&self, from: usize) -> Vec<BattleEvent> {
        self.log[from..].to_vec()
    }

    /// Subtract resolve from side `t`'s active; faint/victory chain.
    ///
    /// Once the battle is decided the hp state is still updated, but no
    /// events are emitted and no faint chain runs (double-KO guard).
    pub(crate) fn apply_damage(&mut self, t: usize, amount: u16) {
        let side = &mut self.sides[t];
        let c = side.active_mut();
        c.current_resolve = c.current_resolve.saturating_sub(amount);
        if self.winner.is_some() {
            return;
        }
        if c.current_resolve == 0 {
            let idx = side.active_idx as u8;
            self.emit(BattleEvent::ChampionYielded {
                side: t as u8,
                party_idx: idx,
            });
            if self.sides[t].any_alive() {
                self.pending_switch[t] = true;
                self.emit(BattleEvent::ForcedSwitchNeeded { side: t as u8 });
            } else {
                self.finish(1 - t as u8);
            }
        }
    }

    pub(crate) fn heal_active(&mut self, s: usize, amount: u16) {
        let max = self.sides[s].computed().max_resolve;
        let cur = self.sides[s].active().current_resolve;
        let can = max.saturating_sub(cur).min(amount);
        if can > 0 {
            self.sides[s].active_mut().current_resolve += can;
        }
        self.emit(BattleEvent::Healed {
            side: s as u8,
            amount: can,
        });
    }

    pub(crate) fn do_switch(&mut self, s: usize, idx: usize) -> Result<(), BattleError> {
        if idx >= self.sides[s].party.len()
            || !self.sides[s].alive(idx)
            || idx == self.sides[s].active_idx
        {
            return Err(BattleError::BadSwitch);
        }
        // Sleep wears off when a champion leaves the field (its counter lives
        // on ActiveState, which is reset; leaving the ailment behind would
        // keep it Slumbering forever on switch-back).
        let out = self.sides[s].active_idx;
        if self.sides[s].party[out].ailment == Some(Ailment::Slumber) {
            self.sides[s].party[out].ailment = None;
            self.emit(BattleEvent::AilmentEnded {
                side: s as u8,
                ailment: Ailment::Slumber,
            });
        }
        self.sides[s].active_idx = idx;
        self.sides[s].active_state = ActiveState::default();
        self.emit(BattleEvent::Switched {
            side: s as u8,
            to_party_idx: idx as u8,
        });
        Ok(())
    }

    pub fn apply_forced_switch(&mut self, side: u8, party_idx: u8) -> Result<(), BattleError> {
        let s = side as usize;
        if self.winner.is_some() || s > 1 || !self.pending_switch[s] {
            return Err(BattleError::BadSwitch);
        }
        self.do_switch(s, party_idx as usize)?;
        self.pending_switch[s] = false;
        Ok(())
    }

    pub fn legal_actions(&self, side: u8) -> Vec<Action> {
        let s = side as usize;
        if self.winner.is_some() || s > 1 {
            return vec![];
        }
        let sd = &self.sides[s];
        let switches = (0..sd.party.len())
            .filter(|&i| sd.alive(i) && i != sd.active_idx)
            .map(|i| Action::Switch { party_idx: i as u8 });
        if self.pending_switch[s] {
            return switches.collect();
        }
        let rites = sd.active().rites.iter().enumerate().filter_map(|(i, r)| {
            r.filter(|r| r.zeal_left > 0)
                .map(|_| Action::Rite { slot: i as u8 })
        });
        rites.chain(switches).chain([Action::Yield]).collect()
    }

    /// Pre-action gates. Returns true when the attempt is fully blocked.
    pub(crate) fn gates_block(&mut self, s: usize, rng: &mut BattleRng) -> bool {
        {
            let st = &mut self.sides[s].active_state;
            if st.slumber_turns > 0 {
                st.slumber_turns -= 1;
                if st.slumber_turns == 0 {
                    self.sides[s].active_mut().ailment = None;
                    self.emit(BattleEvent::AilmentEnded {
                        side: s as u8,
                        ailment: Ailment::Slumber,
                    });
                }
                // waking champion loses this turn (older-school rule: simpler
                // to reason about, easier to test)
                return true;
            }
        }
        match self.sides[s].active().ailment {
            Some(Ailment::Frost) => {
                if rng.chance(20) {
                    self.sides[s].active_mut().ailment = None;
                    self.emit(BattleEvent::AilmentEnded {
                        side: s as u8,
                        ailment: Ailment::Frost,
                    });
                } else {
                    return true;
                }
            }
            Some(Ailment::Shackled) => {
                if rng.chance(25) {
                    return true;
                }
            }
            _ => {}
        }
        if self.sides[s].has_volatile(Volatile::Confusion) {
            let pos = self.sides[s]
                .active_state
                .volatiles
                .iter()
                .position(|(k, _)| *k == Volatile::Confusion)
                .expect("checked");
            let v = &mut self.sides[s].active_state.volatiles[pos];
            v.1 = v.1.saturating_sub(1);
            if v.1 == 0 {
                self.sides[s].active_state.volatiles.remove(pos);
            }
            if rng.chance(33) {
                let a = self.staged_stat(s, BattleStat::Valour, false);
                let d = self.staged_stat(s, BattleStat::Guard, false);
                let lvl = self.sides[s].active().level as u32;
                let base = (((2 * lvl / 5 + 2) * 40 * a) / d) / 50 + 2;
                let amt = base.clamp(1, u16::MAX as u32) as u16;
                self.emit(BattleEvent::Damage {
                    side: s as u8,
                    amount: amt,
                    crit: false,
                    effectiveness_pct: 100,
                });
                self.apply_damage(s, amt);
                return true;
            }
        }
        self.sides[s].has_volatile(Volatile::Flinch)
    }
}
