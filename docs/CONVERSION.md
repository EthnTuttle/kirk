# CALL TO REPENTANCE - the conversion mechanic (collection grammar)

Status: design contract, 2026-10-16. Owner direction: this REPLACES
"Rally" as the collection loop. Slogan register: "call them all to
repentance." Binds crusade-core (M7) and crusade-app UI (M8 windowing
comes later; the engine seam lands with the adversary tables).

## 1. Theology of the loop

All men can be saved through repentance and faith in Christ Jesus
(1 Tim 2:4; 2 Pet 3:9; Rom 10:9-13). The loop IS that doctrine: every
common adversary is convertible in principle; the contest is against
hardness of heart (Heb 3:13), not against his worth as a man. Saul the
persecutor is the archetype of the converted foe. Register rules:
docs/HISTORICAL-FOES.md section 6 (revised) and docs/DOCTRINAL-SINS.md.

## 2. When the Call may be issued

- Only against a DEFEATED human adversary (HP -> Yield) of an archetype
  flagged `convertible` (ids 108-117 yes; commanders 120-123 NEVER;
  sin-spawn NEVER - demons are fought, not evangelized).
- The Call is a post-battle action, once per defeated foe, issued by the
  Champion who struck the final blow (or the squad leader if that
  Champion yielded). It does not occur during battle; no rite slot is
  spent. Refusal is final for that instance.
- Campaign nodes may SCRIPT the outcome (e.g., a tutorial convert, a
  scripted refusal) - the engine supports a forced result.

## 3. The contested roll

New species field: `obstinacy: u8` (1-100). Higher = deeper entrenchment.
Commanders are not rolled (see section 2) so carry no value.

    conviction = piety_of_caller                        (stat, post-level)
               + 20 if caller's virtue rites felled the foe
                 (the defeat itself preached)
               + fervor_bonus                           (0..15, from litany
                 items / chapter-house blessings, OPTIONAL lane)
    hardness   = obstinacy
               + 15 if the foe's level > caller's level
                 (seniority in his error)
               + sin_depth_bonus                        (0..30, per
                 archetype's narrative tier - see section 4)
    chance = clamp( conviction / (conviction + hardness), 0.03, 0.85 )

Success => BAPTISM (section 4). Failure => RELEASE (section 5).
The floor of 3% honors Romans 10:13 even against the hardest common foe;
the 85% ceiling keeps zeal meaningful - no champion converts at will.

Determinism: the single-player draw uses a store-derived salt
(splitmix64; see crusade-app/src/conversion.rs), deterministic per save —
the kirk commit-reveal seam is NOT consumed here since PvP battles never
convert and the single-player roll needs no opponent-facing proof. If a
post-M8 mode ever makes conversion adversarial, re-route through the
per-battle sub-seed discipline.
PvP battles never convert (your opponent's roster is his own).

## 4. Baptism (success)

- The adversary repents and is baptized. He joins the roster as a NEW
  CHRISTIAN instance: species kept (his kit and arms are his trade), but:
  - typing shifts from `Typing::Valor` to the virtue that felled him
    (or Humility if the final blow was neutral) - grace, not erasure;
  - nickname defaults to a Christian name form of his office
    (data table: `baptismal_names` per archetype); renamable;
  - a `converted: true` flag + `converted_from: species_id` fields on the
    instance (additive serde fields, default false/absent - no migration).
- Hagiography unlock: the Roll of Worthy Adversaries page PLUS a
  **Testimonies page** (the man's account of repentance, first-person,
  chronicle voice - the emotional payload of the loop).
- Reward: "Firstfruits Shards" (new item). Bigger shard grant than
  Contrition; the economy pays for mercy, not slaughter.

## 5. Release (refusal)

- The foe refuses. The party commends his soul unto God (short liturgical
  beat - text, no animation budget). He departs the field permanently.
- Reward: "Mercy Shards" (smaller than Firstfruits, nonzero - releasing
  is not failure; "as many as received him" is the whole verse).
- Refusal is logged in Hagiography statistics (calls issued / converts /
  souls released) but not as a dead-letter ghost roster.

## 6. Obstinacy & sin-depth tiers (data obligations)

Each archetype in `crusade-core/src/data/adversaries.rs` carries
`obstinacy` and `sin_depth: u8 (0..=3)`:

| tier | meaning | visual register (HF section 6 rule 4) | sample obstinacy |
|---|---|---|---|
| 0 | conscript / tribal auxiliary, loosely held | plain, dignified man | 20-35 |
| 1 | professional soldier of the askar | plain, harder bearing | 35-50 |
| 2 | zealot elite; ghazi culture | shadowed eyes, ashen drift | 50-70 |
| 3 | sin-boss-adjacent commander cadre | visible corruption | 70-90 |

The visual gradient uses existing palette slots where possible and must
read as spiritual condition, never kit or ethnicity (HF 6.4).

## 7. Engine seams (M7 contract)

- `species.base`: add `obstinacy: Option<u8>` and `sin_depth: Option<u8>`
  (serde-default Option => old saves/tables untouched).
- `instance`: add `converted: bool` (default false), `converted_from:
  Option<u16>`. Persistence-note: instances already serialize species_id
  u16; the two new fields are additive with serde defaults.
- New pure fn in crusade-core (LANDED M7, data::conversion_chance):
  `conversion_chance(caller_piety: u16, caller_level: u8,
  caller_used_virtue_kill: bool, fervor_bonus: u8, foe: &SpeciesBase,
  foe_level: u8) -> Option<u8>` (percent 3..=85; None for
  non-convertible), unit-tested at boundary tiers; the actual draw path
  reuses the RngFacade. No I/O, wasm-safe.
- Scope note: crusade-core seams above are M7 (landed). The crusade-app
  seams below - the settings keys `campaign.foes_defeated` /
  `campaign.converts_baptized` (u32) and the post-victory dialog - are
  DEFERRED to M7b (ROADMAP), matching HF section 9 HF-3/HF-4.
- M7 balance debt (documented, M1-Sabina precedent): the Sudani
  Shieldbearer's no-STAB Valor lane puts him below the 40-60% fuzz band;
  fixing needs a doc-level kit change in HISTORICAL-FOES section 4 or a
  RiteType::Valor STAB lane - owner decision. Also: `crusade-sim fuzz'
  pool is champions-only today; widening it for adversaries needs the
  id-indexed stats array fixed first (crusade-sim main.rs).
- Client flow (crusade-app): post-victory dialog when a convertible foe
  yielded: [Call to Repentance] [Release his soul] - choosing Release
  directly grants Mercy Shards with no roll.
- Settings bitmask `campaign.foes_defeated` (u32; 14 foe ids exceed u8)
  per HF 6.5 unchanged; add `campaign.converts_baptized` (u32) for
  Testimonies pages. (The "Testimonies" name avoids collision with the
  existing `witnesses.rs` module.)

## 8. Banned register for all conversion text

Never: coercion verbs ("converted him"), gore-thrills at refusal,
mockery of the man's former faith beyond the attested doctrinal points
(DOCTRINAL-SINS.md), or suggesting the soul released is damned by the
GAME rather than by his own refusal - "his soul unto God" is the exact
frame; judgment belongs to the Judge.
