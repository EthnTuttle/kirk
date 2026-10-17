# THE MINISTRY OF THE ROAD — recovery design (M8 follow-on)

Status: design proposal, 2026-10-17. Prompted by docs/STORAGE-SOAK.md's
measured finding: as shipped, the Pilgrim Road is structurally
unwinnable — the single-file mantle faces 2–3 peer-level foes per battle,
resolve carries over between battles with no recovery, and every
soaked pilgrimage went extinct around day 10 with 0 battle wins.
Succession-marching, not victory, is the de facto core loop.

Owner direction: add recovery in some form — a camp action, a healing
unit, healing items, "something to that effect" — **without offsetting
the game's balance**. The road must stay costly. This is a pilgrimage,
not a power fantasy.

This document: (1) what comparable games do and why it works, (2)
candidate designs with exact numbers, (3) one recommendation with an
implementable spec, (4) what is explicitly NOT recommended.

Register: chronicle-firm. Healing on the road is **ministration,
medicine, balms, prayer, shriving** — never "potion," never "HP."
The Book of the Fallen is not diminished by any of this: men still die,
and the road still takes its tithe.

## 0. The systems as built (what recovery must fit)

Relevant facts established by reading the code, cited so the numbers
below are checkable:

- **Resolve persists across road battles.** `on_trail_battle_end`
  (crusade-app/src/client.rs:1415-1428) clones survivors unrested;
  only zeal is refilled (the field-fix so the next `Battle::new`
  passes its full-zeal gate). The campaign lane, by contrast, rests its
  survivors: `current_resolve += max/2` capped, ailment cleared, zeal
  restored (client.rs:1628-1659). That campaign rest is the in-repo
  precedent for a partial, capped heal — **the gauntlet already heals
  50% between nodes and is still the harder lane.**
- **Battle odds.** Wild/ambush mixes (`trail_foe_mix`) field 2–3
  adversary archetypes at the mantle's level; gates field two escorts
  at mantle−2 plus a named commander at mantle+2, capped 55 — no typing
  edge exists (`Typing::Valor` is 1.0× both ways). A won gate therefore
  costs, at best play, most of a full resolve bar.
- **Day economy.** Upkeep is `living/5` supplies/day, floor 1
  (trail.rs:110) — 4/day for a full company of 20. The trail-event
  table's forage income averages ~+3/7 supplies/day against ~−4/day
  spoilage/theft (weights in TRAIL_EVENTS, overworld.rs:164-182): the
  road's supply ledger is roughly break-even on the march and strongly
  negative for a stationary company. **Days themselves are already the
  resource** — the ROADMAP's crossing budget is ~40 days.
- **Morale** is bounded ±100 (MORALE_MAX); the free Rest event gives
  +10 (REST_MORALE_GAIN); starvation −5/day; victory +5
  (TRAIL_VICTORY_MORALE). Morale gates the MANTLE's reorder bleed
  (`50 + morale/2` percent stay) — it is a survival resource, not a
  comfort stat.
- **Props exist.** `PropRef::Camp` is already in data, placed on
  screen 3 (the Marches), with a bevy-shell prompt "make camp" and a
  stub notice (`day += 1`, morale +2) in the demo lane only — the
  engine and client have no camp verb at all. Shrines stand on screens
  2, 10, 21; Caravans on 5, 15 (shell stubs: shrine +4 morale,
  caravan +10 supplies). `TrailView` carries `choice_encounters` only;
  shrine/camp/caravan are scenery in v1 per the TrailView doc comment.
- **A pure-heal rite already exists** in the battle engine (status,
  never misses, heals half — rites.rs `required_special_rites_exist`),
  so in-battle ministration is a solved, tuned problem. The gap is
  strictly **between** battles.
- **The trail battle party is ONE instance.** `TrailCampaign.party` is
  a `Vec` that v1 fills with exactly one champion instance bearing the
  mantle; succession rebuilds it with `practice_champion(former_species,
  banded_level)`. There is no roster slot for a companion body — this
  decides the "healing unit" question below.
- **Hospitaller Ysolt (species 004)** is a champion archetype, not a
  calling; callings (data/callings.rs) are stat-band pairs, all 25
  filled, none medical. The adversary roster 108–125 has no healer —
  the chronicle's symmetry point (HF §6: redeemable men, both sides)
  matters for canon if healing enters the battle lane.

## 1. Comparative matrix: how the genre gates recovery

| Game | Mechanic | Cost | Scope | Pacing brake | Why tension survives |
|---|---|---|---|---|---|
| **Darkest Dungeon** (camp) | Firewood-gated camp in cleared rooms: food phase (starve 0 / half 2 / full 4 / feast 8 food) then 12 Respite points spent on camp skills (heal 15% + cleanse one hero; stress −15; buffs) | One firewood per medium quest (two per long); food spend up to 8; skills fight each other for the 12-point budget | Partial: 15% heal per skill buy; feast caps at +25% health | **33% night-ambush chance after camp** unless a skill is spent to ward it | Recovery is bought from the same purse as food and is never free of risk; skipping the ambush-ward spends recovery budget on safety |
| **Slay the Spire** (rest site) | Rest: heal 30% of max — OR — Smith: upgrade a card | Fixed sites on the map; mutually exclusive choice | 30% of max, once per site | Scarcity via **choice**: every heal is an upgrade foregone | Healing is priced in power; a player who always rests falls behind the curve; one who always smiths dies |
| **The Oregon Trail** (rest) | "Rest for N days": health recovers slowly while days pass | Time (the season window closes; the game ends in winter) plus daily food draw | Slow, whole-party, day-scaled | The calendar is the brake: rest is paid for in arrival risk | Recovery and the endgame clock are the same currency |
| **Banner Saga** (rest) | Camp rest restores morale/mends injured fighters over days | Supplies per day of rest, while the caravan still eats | Injury mends are day-scaled, not instant | Supplies spent resting don't buy renown or feed the march | Rest is an investment carved from the same supplies that keep the caravan alive |
| **Final Fantasy** (Tent/Cottage) | Consumable item: full party heal, **overworld/save-point only** | Inventory slot + gil; unusable mid-dungeon | Total (100%) — but location-gated | Works only because dungeons are one-push gauntlets; the gate is *where*, not *how much* | Total recovery is safe only when the hard content never occurs at a legal use site |
| **Dragon Quest** (inn/church) | Inn heals fully in town; church revives/purifies | Gil; return trip to town | Total, town-only | Distance-to-town is the meter | Same location-gate as FF; meaningless for a one-way pilgrimage with no towns |

**Extracted transferable mechanics:**

1. **Recover a fraction of max, not to full.** Every durable system
   (DD 15–25%, StS 30%, campaign lane's own 50%) heals a *share of the
   bar*, so a damaged company can be made *serviceable*, never *fresh*.
2. **Pay for it in the same resource that paces the journey.** DD food,
   OT days, Banner Saga supplies — recovery is a reallocation of the
   survival budget, so healing never adds net safety, it only shifts
   where the risk lives (later hunger, later calendar, later odds).
3. **Attach a distinct risk to the vulnerable act.** DD's 33%
   night-ambush is the cleanest example: camping is when a marching
   column is weakest; the mechanic prices the military truth.
4. **Choice-as-scarcity beats cooldown-as-scarcity.** StS's
   heal-or-upgrade derives tension from mutual exclusion, not timers;
   here the analogue is camp-day vs march-day (the Oregon clock).
5. **Location-gated total heals (FF/DQ) do not transfer.** Recovery
   that is total-but-place-locked works for hub-and-spoke dungeons;
   the Pilgrim Road is a one-way line with no towns. A shrine that
   fully heals would become a mandatory toll booth and a bounce-loop
   exploit (the soak's 24↔25 march pattern proves players bounce).
6. **Healer companions in small bodies-of-one.** The genre's answer is
   consistent: in a single-fighting-body party, "the healer" cannot be
   a separate unit — it collapses into either a self-heal skill (which
   this engine already has: the 50% status rite) or a mandatory-pick
   archetype the whole run orbits. For a 20-man named roster where only
   the mantle fights, a healer *unit* would concentrate all play on
   protecting one archetype — and per succession rules the mantle's
   species is inherited, so the perk would be a permanent birthright or
   a permanent loss. Neither fits. (Full argument in §4.)

## 2. The balance ledger the numbers must answer

From the soak (docs/STORAGE-SOAK.md, Result 1): the measured baseline is
**0 battle wins, extinction ≈ day 10**, 8 battles forced/fought in that
span. Foe mixes are 2–3 peers per battle. So the average battle
consumes ≈ a full resolve bar: one body versus three peers at equal
level, no typing edge.

What recovery can change, and what it must not:

- A battle that costs ~100% of the bar is unwinnable twice; one that
  costs ~60% of the bar (perfect rite play, Valour stack openers, foe
  AI greed) is winnable twice **if** the mantle re-enters at ≥75%.
- Therefore the design target: recovery that can bring a prudent
  player's mantle back to a **two-thirds-to-three-quarters floor**
  between battles, at a cost of roughly **one day and a heavy feed per
  mend** — enough that a two-battle day still costs men and resolve,
  and a gate day still opens from a wounded stance.
- Days cost supplies (4/day at full company) and forage roughly
  breaks even while marching; a stationary mend day is strictly
  net-negative in supplies. ~30 travel days separate embark from the
  Jaffa terminus on the v1 spine; every camp day is a day the season
  clock and the supply ledger both run.

## 3. Candidate designs

### Candidate A — MAKE CAMP (a road day action) — RECOMMENDED

A new trail verb at `PropRef::Camp` screens: the company halts, posts
watches, sings the office, mends what can be mended.

- **Effect on the mantle:** `current_resolve += max_resolve / 4`
  (25% of max), **capped at 3/4 of max** (`min(new, (max*3)/4)`);
  clears `ailment` (mirrors the campaign rest's ailment rule); zeal
  untouched (the victory field-fix already keeps entry legal).
- **Effect on the company:** morale +REST_MORALE_GAIN's half (+5);
  no men return, no supplies appear.
- **Cost:** one full day (`day += 1`), upkeep charged **twice**
  (the men eat double rations and the wounded are shriven —
  `2 × daily_supply_cost(living)`), and the day's provisioning is
  consumed with **no trail-event roll** (a camped company neither
  forages nor suffers spoilage — one ledger, honestly told).
- **Risk (the DD brake):** a night-watch roll on camped days —
  10% flat (vs the 8% road wild roll), foes drawn from the screen's
  encounter-zone mix (or the local brigand mix on zone-less screens).
  A camped battle is fought from the *unmended* resolve: the watch
  cries before the balms are applied. This is the military truth of
  the thing and kills the "camp after every scratch" density.
- **Availability:** Camp props only. v1 spine has one (screen 3); the
  data pass should add one per region on the screen *before* each
  gate cluster (screens 8, 12, 15, 18, 21→ before their gates; exact
  ids set with the densification pass), matching the shrine cadence
  (2, 10, 21). Each prop reusable freely — the brakes are the day,
  the doubled feed, the ambush risk, and the 3/4 ceiling, not a
  per-site cooldown (cooldowns invite bounce-camping at 24↔25).
- **Why it preserves tension (ledger argument):** a camped mantle
  re-enters at ≤75% against a battle costing ≥60% under best play —
  so camping converts *death* into *a hard fight you might lose*,
  once, and the second consecutive battle still opens wounded. Each
  mend costs a day (~4–8 supplies net after upkeep doubling) and a
  1-in-10 chance of fighting from scratch anyway. Against the soak
  baseline: the never-camping player sees **zero change** (day ~10
  extinction stands); the prudent camper trades ~6–10 days of the 40-day
  season for 3–4 mends across the whole road — enough to make one or
  two gates winnable, never enough to camp through the gauntlet
  (Hattin is back-to-back gates at 19/20 with no camp between in data).

### Candidate B — BALMS & RELICS (rare consumable items)

A "Balms of the Order" item: one use between battles, heal 25% of max,
cap 3/4 (identical curve to the camp). Found: gate-victory spoils
(one per battle of record) and a low chance when searching a Caravan
prop. No shop, no drop from wilds.

- Verdict on fit: the **search-the-caravan** and **spoils** hooks are
  chronicle-true and cheap to wire on existing sanctioned write
  points. But item scaffolding (inventory UI on the trail, a new
  store item, drop tables) is real surface area, and — worse —
  hoardable recovery becomes a *planning* resource rather than a
  *tension* resource: the optimal play is to stockpile balms and
  spend them all at Hattin, which is exactly the power-fantasy beat
  the owner ruled out.
- Verdict: **defer to a later card**, possibly as the camp's upgrade
  path (balms extend the camp's cap, never freelances). Not v1.

### Candidate C — HOSPITALLER MINISTRATION (healing companion/calling)

A companion body or a calling that mends the mantle between battles.

- Verdict: **rejected** — see §4. Structurally cannot exist as a unit
  (the trail party is one instance); as a species-locked perk it
  concentrates the run on one archetype and breaks the mantle's
  succession symmetry; as a calling it collides with the locked,
  filled callings table.

## 4. What is NOT recommended, and why

1. **A healer unit (companion body).** There is no slot for it:
   `TrailCampaign.party` is the mantle, full stop, and widening it
   re-opens the multi-file card M8A deliberately deferred. Even in a
   multi-file future, genre precedent is that a dedicated healer in a
   1–2 body fighting line turns every battle into protect-the-medic
   and every death spiral into lose-the-medic-lose-the-run —
   un-saga-shaped. Ysolt's ministry to the wounded is already honored
   where the engine can bear it: her in-battle drain/heal kit.
2. **A healing calling or learned skill.** The callings table is full
   (25/25, stat-pair discipline) and callings are rolled at mint, not
   chosen — a mend-calling becomes either a reroll tax or a lucky
   birthright. The mantle's species carries through succession
   (`practice_champion(former_species, ...)`), so a species-locked
   mend would make one archetype the only viable pilgrim.
3. **Full heals at shrines (the FF/DQ location-gate pattern).** Total
   recovery at fixed points converts shrines into mandatory tolls and
   rewards bounce-marching back to them; it also erases the partial-
   heal discipline every durable system in §1 shares.
4. **Heal-over-days while marching (passive regen).** Removes the
   camp *decision* — the Oregon-Trail tradeoff that makes recovery a
   choice against the season clock. Passive regen also cannot be
   priced per-use; it either out-races attrition (trivial) or doesn't
   (dead mechanic).
5. **Healing keyed to morale or devotion.** Morale is already the
   survival stat of the MANTLE's reorder rule; stacking a second
   payout on it double-Prices the same dial and rewards the rich.

## 5. Recommended spec (Candidate A), implementable

### Engine (crusade-core/src/engine/trail.rs)

- New consts:
  - `CAMP_RESOLVE_PCT_NUM: u16 = 1, CAMP_RESOLVE_PCT_DEN: u16 = 4` (heal 25%);
  - `CAMP_RESOLVE_CAP_NUM: u16 = 3, CAMP_RESOLVE_CAP_DEN: u16 = 4` (cap 3/4);
  - `CAMP_MORALE_GAIN: i8 = 5`; `CAMP_AMBUSH_PCT: u8 = 10`.
- New pure fn beside `travel_day`:

  ```text
  make_camp(state, rng, mantle_max_resolve, mantle_resolve, mantle_ailment)
      -> CampOutcome { day, supplies, morale, resolve_after, ailment_cleared, ambush_ref }
  ```

  Semantics: `day += 1`; supplies charged `2 × daily_supply_cost(living)`;
  morale `+ CAMP_MORALE_GAIN` (clamped); resolve healed by `max/4`,
  capped at `3max/4` (integer math, no float); ailment cleared; ambush
  resolved from `rng.pct() < CAMP_AMBUSH_PCT`, ambush payload = the
  current screen's encounter-zone id or `None` (caller maps `None` to
  the brigand mix 201). If an ambush fires, resolve gain is NOT
  refunded — the balms go on after the fight, at victory routing (the
  caller applies the mend only if the battle is won; a lost camp
  battle is an ordinary road defeat → THE MANTLE).
  No `TrailEvent` roll on a camped day; no men lost, none gained.
- Guard tests: cap math at boundaries (resolve 0, max−1, exactly 3/4);
  upkeep doubling; determinism per seed; ambush bit stable per
  (seed, day).

### Data (crusade-core/src/data/overworld.rs)

- No new `TrailVerb` needed — camp is a player verb on a prop, like
  `EngageChoice`, not a rolled event.
- Place `PropRef::Camp` on the screens listed in §3A (data-only diffs,
  matching the existing guard tests' discipline: props are plain data).

### Client (crusade-app/src/client.rs)

- `ClientInput::MakeCamp` (legal in `Phase::Trail` when the screen
  carries `PropRef::Camp` and the pilgrimage is not over).
- Handler: runs `trail_day_rng(seed, day)` on the *pre-increment* day
  (mirroring `travel`'s stream discipline so a camp and a march on
  "day N" never alias), calls `make_camp`, then: if `ambush_ref`,
  route through `enter_trail_battle` with the region mix **before**
  applying the mend; apply resolve/ailment changes to
  `t.party[0]` only on the quiet path or post-victory. `save_trail()`
  at all exits (one sanctioned write, same as `travel`).
- `TrailView` gains `can_camp: bool` (screen has Camp prop; append-only
  serde tail discipline) and `mantle_resolve: (u16, u16)` so the trail
  pane can show the mantle's wounds — a player choosing to camp must
  see why. (Names-never-numbers applies to the *men*; the mantle's
  resolve is already a shown battle stat.)

### UI / log text (register-firm samples)

- Prompt (bevy prop line, already partially present):
  "Your own camp could stand here. [C] make camp — a day, double
  rations, and the watches posted."
- Quiet camp:
  "The company makes camp. The wounds are washed and bound; the office
  is sung. {Name} rises steadier than he lay down (resolve {a} -> {b}/{max})."
- Mend capped:
  "The balms have done what field-dressing can; the rest must knit of
  itself. ({b}/{max} — no more may be mended on the road.)"
- Night ambush:
  "A cry from the watch — the camp is fallen upon in the dark!"
- Victory after night ambush:
  "The night is kept. By dawn the wounded are tended."
- Starved camp attempt (supplies < cost): refuse with
  "The stores are too lean for a halt; the company must march or starve."

### Test plan

1. Engine unit tests: cap/cost/ailment/determinism guards above; a
   30-day golden sim extended with camp days at fixed cadence, hash
   stable, supplies never underflow.
2. Client tests mirroring campaign_tests.rs' harness: MakeCamp
   rejected off-prop / when `over`; resolve curve respected across a
   camp→battle→camp sequence; ambush path fights unmended then mends
   on victory; Book of the Fallen untouched by camps.
3. Register lint on the new strings (docs/REGISTER-LINT.md lane).
4. Fuzz/soak: extend the storage-soak harness with a `CAMP_PRUDENTLY`
   policy (camp when mantle resolve < 50%, supplies ≥ 3× cost, days
   remaining ≥ 10) vs the existing never-camp policy.

### Winnability acceptance criteria (fuzz/soak targets)

Measured against the STORAGE-SOAK baseline (extinction ~day 10, 0 wins):

- **Never-camp policy:** unchanged within noise — median extinction
  day 8–12 (regression guard: recovery must not leak into the
  non-camper's road).
- **Prudent-camp policy, best charter/provisioning:**
  - ≥ 50% of pilgrimages record **≥ 1 road battle win** (the road is
    now beatable in the small, measurably);
  - ≥ 25% reach the screen-25 terminus by day 40 (Jerusalem is now a
    destination some seeds can see, not a myth);
  - **but** median living-at-arrival ≤ 12 of 20 (the road still takes
    its tithe; arrival is a remnant, not a host), and ≥ 80% of
    arriving pilgrimages carry ≥ 5 Book pages;
  - 100% of pilgrimages that never camp still end extinct by day 15.
- If the targets overshoot (too easy): first turn the cap from 3/4 to
  2/3; then the heal from 25% to 20%. Do not raise the ambush rate
  past 10% — a punished prayer reads as cruelty, not cost.

## 6. Canon note (HF §6)

The adversary roster fields no healers, and none is added: ministration
on this road belongs to the company's own women and men behind the
line — the camp, the balms, the psalm — not to a body in the fighting
line. When the foe yields, the Call to Repentance is unchanged; the
camp mends only ours, as the chronicles' hospital tents stood behind
their own banners.

## 7. Sources

- Darkest Dungeon camping (food tiers, 12-respite skill budget,
  33% night-ambush): [Darkest Dungeon Wiki — Camping](https://darkestdungeon.wiki.gg/wiki/Camping)
- Slay the Spire rest sites (30% heal OR smith): [Slay the Spire Wiki — Rest Sites](https://slaythespire.wiki.gg/wiki/Rest_Sites)
- Oregon Trail rest-vs-calendar tradeoff: [Format Magazine, "You Have Died of Dysentery"](https://www.format.com/magazine/you-have-died-of-dysentery-oregon-trail-design); [Philip Bouchard on OT design](https://www.philipbouchard.com/oregon-trail/imagining-design.html)
- Banner Saga rest/supplies/morale interplay: [Banner Saga Wiki — Events](https://bannersaga.fandom.com/wiki/Events) (rest = morale+ / supplies− / day+); [Game Narrative Review GDC paper](https://media.gdcvault.com/gdc2020/GameNarrativeReview/Benjamin%20Mora-Davison%20-%20The%20Banner%20Saga-paper.pdf)
- In-repo: docs/STORAGE-SOAK.md (measured baseline), docs/OVERWORLD.md §3/§6 (journey resources, THE MANTLE), crusade-app/src/client.rs:1628-1659 (campaign-lane 50%-rest precedent).
