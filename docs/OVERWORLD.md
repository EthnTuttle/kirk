# THE PILGRIM ROAD - overworld & pilgrimage design (M8)

Status: design skeleton, 2026-10-16. Owner direction: the game's final
form leads a company of crusaders to the Holy Land - Oregon Trail
logistics wrapped around top-down, screen-by-screen exploration in the
Zelda register (fixed tile screens, transitions at edges; NOT continuous
scrolling, NOT Pokemon's tile-scrolling overworld). Top-down pixel art
aesthetic is a hard requirement. Encounters are Pokemon-shaped: random in
the wild, scripted at funnels, elective by player choice.

## 1. The two layers

**World layer (Zelda).** The map is a graph of discrete SCREENS, each a
single fixed top-down tile screen (e.g. 160x144-ish internal res, our
LOCKED 16-color palette; exact screen metrics set with the art pass).
Movement is tile-stepped within a screen; stepping off a screen edge
transitions to the adjacent screen (NES-Zelda discipline: brief wipe,
full-screen commitment). Screens carry terrain, props, and encounter
zones marked in data, never in code.

**Journey layer (Oregon Trail).** The campaign is one directed pilgrimage:
rally at the Order's chapter house, cross to the East, survive the road,
reach the Holy Land. The party consumes SUPPLIES per screen traveled;
MORALE and ATTRITION are journey resources (not battle stats). Trail
events fire from tables keyed by region and day. The Oregon Trail
analogy governs pacing economics: starvation, sickness, theft, weather,
river crossings, wrong turns.

## 2. Encounter model (Pokemon-shaped, three species)

| kind | where | trigger | notes |
|---|---|---|---|
| Wild (random) | screens flagged `encounter_zone` | per-step weighted roll, table per zone | adversary archetypes 108-117 by region (Anatolian screens: Rum Seljuk band; Levant: Zengid/Ayyubid; etc.) - historical geography per HISTORICAL-FOES section 2 |
| Scripted (funnel) | gate screens on the single road | entering the screen fires the battle | battles of record (Dorylaeum, Harim, Hattin...); commanders live here; no skipping |
| Choice (elective) | flagged props (warband camp, shrine, wrecked caravan) | player walks up and engages, or walks away | scouting may reveal odds; mercy and conversion options unchanged |

Sin-bosses: the Seven manifest at corruption sites - fixed scripted nodes
the trail passes near; engaging some is elective but ignored corruption
raises per-region journey pressure (morale drain) until cleansed.

## 3. Journey resources

- **Supplies**: spent per screen moved; bought at friendly holds, foraged
  on marked screens (risk roll), looted from choice encounters.
- **Morale** (party): rises on victories, converts baptized, shrines
  visited; falls on defeats, souls released in numbers, starvation,
  trail tragedies. Low morale = battle stat penalties band, not instant
  loss; rock bottom = company disperses (run over).
- **Named men, abstracted numbers never. Attrition removes named men from the
roster permanently - the road to the crusades took many, and the game
remembers them (a Book of the Fallen page in the Hagiography: name, where
he fell, how). Villages/shrines/recruited converts replenish the ranks.
- **Day count**: trail events and some screens are day-gated; the journey
  has a season window (arrive before winter) - the Oregon Trail clock.

## 4. Regions (v1 sketch, tracks the historical arc)

1. The Marches (home; tutorial; low wild density)
2. The Crossing (sea/river gauntlet screens; supply pressure)
3. Anatolia (Rum Seljuk table; Kilij Arslan scripted gate)
4. The Coasts (mixed tables; the Harim gate)
5. The Roads of Egypt (Shirkuh gate; heaviest supply drain)
6. The Horns of Hattin (scripted gauntlet; Gokbori / Taki al-Din wings)
7. The Holy Land (Arsuf, Jaffa; Saladin finale; the City's gate)

Each region = a screen graph + an encounter table + a trail-event table +
art tileset accent (existing palette slots per region faction accents).

## 5. Data seams (M8 contract, add-only)

- `crusade-core/src/data/overworld.rs`: `ScreenDef` (id, region, tile
  layout ref, edges [N/E/S/W -> screen ids], encounter_zone table ref,
  props), `RegionDef`, `TrailEvent` (condition, weight, effect closure id
  - effects are data-selected from a fixed engine verb set: lose_supply,
  lose_men, morale_delta, battle, convert_opportunity, nothing).
- Engine: a deterministic `TrailRng` fed by the kirk seed discipline;
  trail events and wild rolls are seeded per-travel-day, so replays of a
  day are stable and PvP/coop-syncable later.
- New engine state vector (additive serde): world: current screen, named-men
  roster + file assignments + per-man Devotion seed, supplies, morale, day,
  region flags, Book of the Fallen. Battle engine untouched;
  battles are entered/exited through a `start_battle(context)` gate.
- Rendering (crusade-app Bevy): a screen is one tilemap chunk drawn from
  `ScreenDef`; no camera scroll within a screen beyond sub-tile centring;
  edge step -> transition anim -> load adjacent. This is the constraint
  that keeps the aesthetic Zelda rather than scrolling-Pokemon.

## 6. Decisions (owner, 2026-10-16) + THE SUCCESSION MECHANIC

1. **Named men.** The party is a roster of NAMED men, not a headcount
   ("the ranks"). Attrition kills named men permanently; the Book of the
   Fallen records every loss.
2. **Champions lead; the ranks rise.** Champions truly fight and each
   leads a file of named men. When a Champion FALLS on the road (trail
   event, scripted funnel defeat, or sack), SUCCESSION resolves:
   - If any man in his file has sufficient Devotion (a hidden per-man
     righteousness seed, rolled at recruitment), one **rises in
     righteousness**: he becomes a new Champion with RANDOMLY rolled
     skills/level (seeded by the trail RNG; band = former champion's
     level -5..+2) and takes the file. This is the David-from-the-sheep
     beat; it should be rare enough to feel like grace.
   - If no man qualifies, the file **reorders** under another Champion:
     a percentage of its men are lost in the confusion (death in rout /
     desertion, rolled per man against morale) and the survivors are
     distributed. Taking heavy losses to demoralization here is the
     intended pressure - losing a Champion late-region should wound.
   - Spec detail (champion_succession() pure fn, testing bands, the
     Devotion seed field, Book of the Fallen entries) is a sub-card of
     M8; the mechanic name is THE MANTLE.
3. **Modes.** Scenario/campaign (the pilgrimage) is single-player. The
   MMO-style shared world is a SEPARATE lane; protocol assessment
   (arkin0x/cyberspace et al) in docs/MMO-PROTOCOL-ASSESSMENT.md.
   VERDICT (2026-10-16): cyberspace REJECTED as backbone (spec-only,
   Python/leftpad PoW, no shared world state by design, wasm32-blocked).
   If the lane is ever greenlit: a nostr-native `crusade-world` crate for
   presence/encounter rendezvous over the existing nostr_transport seam -
   battle truth, cashu, identity, and personal data stay kirk-native.
4. **The Crossing is a compressed event sequence at journey start**, and
   its outcomes DETERMINE the party's starting strength (supplies carried
   over, men lost at sea, morale scar). Early choices (charter quality,
   provisioning spend) map to starting bending - the Oregon Trail
   Independence-purchase phase.
5. **Map budget: 40 screens x 7 regions** to start. Screens are data
   (section 5); add/densify later without code churn.
