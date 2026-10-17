---
title: "Coherence Audit — DEUS VULT: Champions of the Cross"
summary: >
  Read-only cross-check of PLAN.md, README.md, docs/*.md, and the Cargo
  workspace source trees for contradictions, unfleshed mechanics, ID/table
  collisions, theological/register inconsistencies, and build/truth risks.
  7 MAJOR and 14 MINOR findings; no CRITICAL. Headline items: a three-way
  rite-range conflict (43..=54 vs 43..=57), a stale "nothing exists yet"
  bullet in ROADMAP.md, the u8 campaign-mask width conflict, stale
  "PvP not wired in UI" claims in BUILDING/SHIPPING, and a transiently-red
  golden-digest test that exposed the absence of any test CI lane.
tags: [audit, coherence, roadmap, m7, conversion, adversaries, pvp, docs]
date: 2026-10-16
status: audit
---

# Coherence Audit — DEUS VULT: Champions of the Cross

**Date:** 2026-10-16 (late evening snapshot) · **Mode:** read-only ·
**Scope:** PLAN.md, README.md, all of `docs/*.md`, workspace Cargo files,
`crusade-core`, `crusade-app`, `crusade-bridge`, `crusade-net`, `crusade-data`,
`crusade-sim`, `kirk` (`src/`), `.github/`.

## 0. Snapshot caveat — this tree is a moving target

The audit ran **while M7 was being landed concurrently** by the parent
session. During the audit window itself:

- `docs/ROADMAP.md` grew from 170 lines (truncated) to 413 lines (complete
  with the promised gap register G1–G16, critical path, risk table, and
  punch list). The truncation observed mid-audit is therefore **resolved**
  and is not reported as a finding.
- `crusade-core/src/types.rs` gained `Typing::Valor` plus
  `obstinacy`/`sin_depth`/`converted`/`converted_from`;
  `crusade-core/src/data/adversaries.rs` (ids 108–117, 120–123; rites
  43..=57) appeared untracked; `docs/DLC-ASSESSMENT.md` and
  `docs/PRODUCTION-ARCHITECTURE.md` were created.

Findings below are cited against the **state at report time** (~23:00).
Committed HEAD is `ede0db4` (fictional heresy-cult pivot); the working tree
reinstates the historical lane per owner direction 2026-10-16 — tracked as
ROADMAP Gap G1.

**Test snapshot:** `cargo test -p crusade-core` was observed **FAILED**
mid-audit (~22:30: 72 passed, 1 failed — `engine::tests::golden_digest`,
after the `Typing::Valor` chart arm landed) and **green again by ~23:10**
(full `cargo test --workspace --exclude kirk-cli-card-game`: all crates
pass, core now 73/73, with the `GOLDEN_DIGEST` constant unchanged at
`tests.rs:478` — the in-flight battle path was evidently reconciled). The
failure was transient churn, not a permanent finding; see F1 for the
residue. An earlier full workspace run in the audit window was likewise
green (133 kirk / 55 core / 26+21 app / 11+9 bridge / 7 data / 16+9 net).

Severity scale: **CRITICAL** = breaks build, consensus, or core truth
claims. **MAJOR** = doc-vs-doc or doc-vs-code conflict that will mislead a
worker or block a card. **MINOR** = stale wording, typography, tracked gap,
or polish.

---

## 1. MAJOR findings

### F1 — Transient red golden-digest test; stale test counts; no CI safety net · MAJOR (residual)

At ~22:30 the tree was red: `engine::tests::golden_digest`
(`crusade-core/src/engine/tests.rs:478`) failed after `chart.rs` added the
`Typing::Valor` flat-100 arm (`chart.rs:58,75`). By ~23:10 the same test
passed with the same pinned constant — the parent session reconciled the
battle path mid-flight. The test itself is healthy; what remains:

- Stale counts: `PLAN.md:110` "deterministic engine; **53 tests**" and
  ROADMAP M7-1 done-test (`docs/ROADMAP.md:68`) "existing **53 core
  tests** green unchanged" — the core lib now has **73** tests.
- For roughly 40 minutes the working tree was red with nothing but local
  runs to notice, while `docs/ROADMAP.md:21` asserts "Workspace tests
  green" and the orchestration rule is "orchestrator commits after each
  green card" (ROADMAP:41–43). The only CI workflow
  (`.github/workflows/web-pages.yml`) builds wasm; no lane runs
  `cargo test` (see f8).

**Fix:** update the "53" counts; add a test CI lane so a red M7 landing
is caught by automation, not by whoever runs tests next.

### F2 — Rite id range: three-way conflict (43..=54 vs 43..=57 vs "43+") · MAJOR

- `docs/HISTORICAL-FOES.md:210` — "new rites 43..=54 likewise"
- `docs/ROADMAP.md:85` — heading "**M7-3 — Rites 43..=54**"
- `docs/ROADMAP.md:88–89` — the same card lists **15** [NEW] rites
  (Wheeling Shots … Wheel the Line), which is 43..=57, not 43..=54
- `crusade-core/src/data/adversaries.rs:3` — "add-only adversary rites
  **43..=57**" (matches the code)
- `docs/ROADMAP.md:390` (punch list) — "M7-3 rites 43+" (vague third form)

The code is self-consistent at 43..=57 with 15 rites; the two doc anchors
that say 43..=54 are wrong (only 12 rites would fit that range).

**Fix:** correct HF §7 (`HISTORICAL-FOES.md:210`) and the M7-3 heading to
`43..=57`; make the punch list say the same.

### F3 — ROADMAP.md contradicts itself and the tree about what exists · MAJOR

`docs/ROADMAP.md:22–24` says "**M7 is IN FLIGHT** … adversary data tables
(ids 108–117 + commanders 120–123), rites 43+, `Obstinacy`, the
Call-to-Repentance conversion engine." Twelve lines later,
`docs/ROADMAP.md:34–36` says "**Nothing in `crusade-core` exists yet for
M7/M8**: `Typing` has no `Valor` arm, no `adversaries.rs`, no
`obstinacy`/`sin_depth` fields…" — every clause of which is now false
against the live tree (`types.rs:57` Valor arm; `adversaries.rs` exists;
`obstinacy`/`sin_depth` fields exist). A worker reading only the caveat
block would re-implement M7-1/M7-2 from scratch.

**Fix:** delete or rewrite the "Nothing … exists yet" bullet to enumerate
what has actually landed vs what remains (e.g. `overworld.rs`, conversion
engine, UI cards).

### F4 — Campaign mask width: docs still say u8-style bitmask, register says u8 overflows · MAJOR

- `docs/HISTORICAL-FOES.md:188` — foes unlocked "via a new additive
  settings key `campaign.foes_defeated`" (bitmask semantics per HF §6.5,
  mirroring `campaign.sins_defeated`, a u8 with 7 bits used)
- `docs/CONVERSION.md:108–109` — "Settings bitmask `campaign.foes_defeated`
  per HF 6.5 **unchanged**; add `campaign.converts_baptized` bitmask"
- `docs/ROADMAP.md:322` (Gap G5) — "needs 14 bits … **u8 masks overflow**.
  Use u32 masks in new keys … Decide at M7b-2 contract time"

The conflict is live: CONVERSION/HF still specify mask-unchanged semantics
while ROADMAP G5 (and the punch list, line 402: "u32 masks per G5") say the
width must change. M7b-2 blocks on this and the source docs have not been
updated to the resolution.

**Fix:** land the G5 decision (u32 masks or per-id keys) and edit HF §6.5
and CONVERSION §7 to state it; until then M7b-2's contract is ambiguous.

### F5 — PLAN.md §7 contradicts itself about milestone completion · MAJOR

- `PLAN.md:138` — "ALL milestones M0..M6 complete."
- `PLAN.md:152` — "Remaining roadmap: M5b (Bevy visual shell, wasm32/mobile
  targets — needs …), M6 (…)"

The stale "Remaining roadmap" sentence survives below the "all complete"
declaration. M5b and M6 are demonstrably done (shell, lobby, Elo,
attestation, alms CTA all exist in `crusade-app`).

**Fix:** delete the stale remaining-roadmap sentence (and fold M7/M8 lines
into §6/§7 per the ROADMAP punch-list Day-0 item).

### F6 — BUILDING.md / SHIPPING.md claim PvP is not wired into the UI; it is · MAJOR

- `docs/BUILDING.md:111` — "**PvP is NOT wired into the UI yet** (the net
  driver exists in …)"
- `docs/SHIPPING.md:32` — "1. **PvP not exposed in the UI.** Transports,
  duel protocol, and attestation …"

The code disagrees: `crusade-app/src/bevy_shell/screens.rs` has the lobby
screen, `main.rs` runs the net bridge, `client.rs`/`lobby.rs` drive
`Phase::Lobby`/`BattleSetup`, and pvp.rs implements Elo + attestation +
alms. `docs/ART-STATUS.md` §5 and `docs/ROADMAP.md:19–21` both describe the
v1.1 PvP surface as feature-complete. BUILDING's menu description likewise
omits the lobby/Hagiography/gallery surfaces that exist.

**Fix:** update both docs' limitation lists to current reality (keep the
*true* limitation — no live Cashu mint, `SHIPPING.md:37` — which is
accurately stated).

### F7 — Shard economy has three names across docs and code · MAJOR

- `docs/CONVERSION.md` §4–§5 mint **"Firstfruits Shards"** and **"Mercy
  Shards"** (baptism / release rewards)
- `docs/HISTORICAL-FOES.md:213` — rewards mirror contrition shards as
  **"Commendation Shards"** (q4)
- The existing code economy is **Contrition** shards
  (`crusade-core/src/data/rites.rs:174` "Frost of Contrition";
  `docs/CONVERSION.md:68` "Contrition; the economy pays for mercy")

ROADMAP G7 (line 324) tracks this as an unresolved owner decision with a
proposal (two shards: Firstfruits + Mercy), but the three docs still
disagree and M7-4 will implement item rewards against one of them.

**Fix:** owner ruling (already Day-0 punch-list item 2); then sweep HF §7's
"Commendation" mention and CONVERSION §4–§5 to the chosen canon.

---

## 2. MINOR findings

### f1 — "Witness" naming collision (tracked as G3) · MINOR

`crusade-core/src/data/witnesses.rs:3` is the Kirk memorial roll
("non-combatant"); CONVERSION.md §4's "Witness pages" are convert
testimonies — a different thing. ROADMAP G3 proposes renaming the CONVERSION
side to "Testimonies of the Baptized"; CONVERSION.md is not yet edited.
**Fix:** apply the G3 rename in CONVERSION-driven cards.

### f2 — Pokemon term "HP" in CONVERSION.md · MINOR

`docs/CONVERSION.md:19` — "a DEFEATED human adversary (HP -> Yield)". The
codebase stat is **Resolve**. **Fix:** s/HP/Resolve/.

### f3 — Garbled Shema quotation in DOCTRINAL-SINS.md §1.7 · MINOR

`docs/DOCTRINAL-SINS.md:181` — "The first commandment is this: **Hour
self the Lord**…" should read "Hear, O Israel: The Lord our God…"
(Mark 12:29 quoting Deut 6:4–5). A mangled Scripture quote is a register
offense in a doc whose whole job is doctrinal precision.
**Fix:** correct the quotation.

### f4 — Two garbled phrases in DOCTRINAL-SINS-2.md · MINOR

- Line 140: "East — **salves of conquest** made into the realm's elite"
  (should be "slaves")
- Line 440: "**pennies-to-shoulds**: a rich war-gear laid down at an altar"
  (garbled idiom; likely "pennies-to-pounds" or a coined phrase needing a
  gloss)

**Fix:** correct or gloss both.

### f5 — Stale "rally" terminology in witnesses.rs doc-comment · MINOR

`crusade-core/src/data/witnesses.rs:3` — "no typing, no stats, no learnset,
no rally or roster presence." "Rally" is retired PLAN-vocabulary; HF §10-q5
meanwhile reuses "Rally" for the trial-join mechanic, so the word now means
two things. **Fix:** drop "rally" from the doc-comment; prefer "battle
presence".

### f6 — Stale crate description and README bullet for crusade-app · MINOR

`crusade-app/Cargo.toml:5` — "Bevy shell + wasm/mobile targets **land in
M5b**" (M5b shipped). The README's crusade-app bullet carries the same
stale future tense. **Fix:** update both to descriptive present.

### f7 — ART-PIPELINE specs superseded but unmarked · MINOR

`docs/ART-PIPELINE.md` still specifies a ~24-colour master palette and
64×64 sprites (lines 19, 58, 118, 142–144, 180–183, 222–223) while
ART-STATUS locks the shipped reality at 16 colours / 128×128. Nothing in
ART-PIPELINE points to the supersession. **Fix:** add a "superseded by
ART-STATUS §…" banner to §5/§6.

### f8 — No CI lane runs `cargo test` · MINOR (truth risk)

`.github/workflows/web-pages.yml` is the only workflow (trunk wasm build
for Pages). Every "workspace tests green" claim in the docs rests on local
runs — demonstrated as fragile by F1. **Fix:** add a
`cargo test --workspace --exclude kirk-cli-card-game` workflow; ideally the
G11 banned-terms gate rides in the same lane.

### f9 — Unbalanced `**` bold markers in OVERWORLD.md · MINOR

`docs/OVERWORLD.md:49` and `:97–98` have odd numbers of `**` delimiters,
breaking rendering of the named-men principles list.
**Fix:** close the bold spans.

### f10 — `fervor_bonus` sources undefined (tracked as G12) · MINOR

CONVERSION §3's formula takes `fervor_bonus (0..15)` from "litany items /
chapter-house blessings" that have no item table, source, or economy.
ROADMAP G12 proposes plumbing `fervor: u8` always 0. Consistent plan, but
CONVERSION.md still reads as if the sources exist.
**Fix:** annotate CONVERSION §3 with the G12 deferral.

### f11 — THE MANTLE "sufficient Devotion" threshold unspecified · MINOR

OVERWORLD.md's succession mechanic triggers on "sufficient Devotion"
without a number, band, or cost curve — and this one is **not** in the
ROADMAP gap register (G1–G16). **Fix:** pin the threshold (or a
"determined in M8b fuzz" note) and consider adding it to the register.

### f12 — Book of the Fallen shape only sketched · MINOR

OVERWORLD.md and the M8b-2 card describe intent (memorial of fallen named
men) but no data shape, cap, or compaction rule. Partially covered by G16
(storage growth). **Fix:** fold a concrete shape into the M8b-2 contract.

### f13 — HF §10 shows all 7 questions open; ROADMAP G13 says three are answered "in-file" · MINOR

ROADMAP G13 (line 330) claims q1/q5/q6 were "answered by owner direction
(in-file)", but `docs/HISTORICAL-FOES.md` §10 carries no answered-markers.
Either the markers were never added or the answers live elsewhere.
**Fix:** mark the answered questions in HF §10 (or correct G13).

### f14 — "Chains of Conscience" realized as a rite name while the ailment is `Shackled` · MINOR

PLAN §3's concept appears in code as rite 25 "Chains of Conscience"
inflicting the `Shackled` ailment. Harmless, but the concept-name and
mechanic-name split will confuse prose cards.
**Fix:** one clarifying line in PLAN §3 or the codex table.

---

## 3. Verified clean (checked, no finding)

- **Nostr kind map:** kirk 9259–9263 (`src/events/mod.rs:18–22`), duel
  9264 and attest 9265 in crusade-net/bridge, 9734/9735 are the standard
  NIP-57 zap kinds. No collisions inside the project or with registered
  NIPs in use.
- **Species id ranges disjoint:** champions 1–12, sins 101–107, adversary
  archetypes 108–117, commanders 120–123; `species_by_id` scans SPECIES
  then ADVERSARIES; ids < 108 unchanged.
- **Rite ids:** 1–42 existing, 43–57 append-only adversary rites; no
  overlap, serde-forwards-only discipline stated and tested at the table
  level.
- **Settings keys:** `campaign.sins_defeated`, planned
  `campaign.foes_defeated`/`campaign.converts_baptized`, `ladder.rating`,
  relay-list override — all distinct; no bit-position reuse across keys.
- **Palette slots:** adversary accents (Rum red `#A62639`, palegold
  `#F4D35E`, Zengid ink `#0B0E1B`) all exist in the locked palette.
- **Stat bands:** archetype totals 300–380, commander band 390–420; id
  117's 405 is a documented mini-boss exception riding the commander band
  (`adversaries.rs:14,390`), and the table test encodes it.
- **Register/theology in code:** no coercion verbs, no mocking text;
  conversion is gated on a *defeated, surrendering* adversary (CONVERSION
  §2), consistent with PLAN §1 dignity rules and CONVERSION §8's banned
  register. The release option ("Release his soul") is framed as mercy,
  not conquest.
- **No CRITICAL findings.** The build is green repo-wide at snapshot time;
  the mid-audit red was a transient in-flight miscalibration (F1), since
  reconciled, not a logic regression.

---

## 4. Prioritized fix queue

1. **Truth-proof the green claims** (F1): update the stale "53 tests"
   counts (PLAN.md:110, ROADMAP.md:68) and add a
   `cargo test --workspace` CI lane so a red M7 landing is caught by
   automation, not luck.
2. **Rite range sweep** (F2): `43..=57` in HISTORICAL-FOES.md:210 and the
   ROADMAP M7-3 heading; punch list "43+" → "43..=57".
3. **Delete the stale "Nothing exists yet" bullet** (F3,
   ROADMAP.md:34–36) and re-enumerate actual M7 landing state.
4. **Land G5 width decision** (F4): u32 masks (or per-id keys) written
   into HF §6.5 + CONVERSION §7 before M7b-2's contract freezes.
5. **PLAN.md §7 cleanup** (F5): remove the stale "Remaining roadmap:
   M5b/M6" sentence; add M7/M8 lines per punch-list Day-0.
6. **PvP-wiring doc updates** (F6): BUILDING.md:111, SHIPPING.md:32, and
   BUILDING's menu description → current v1.1 surface.
7. **Owner decisions already queued** (F7, f13): G1 canon commit, G7
   shard naming, G13 q3/q7 — per ROADMAP §5 Day 0.
8. **Text sweep** (f2–f7, f9, f14): HP→Resolve, Shema quote, "salves",
   "pennies-to-shoulds", witnesses.rs "rally", Cargo.toml/README bullets,
   OVERWORLD bold, Chains-of-Conscience note.
9. **ART-PIPELINE supersession banner** (f7) and CONVERSION §3 G12
   annotation (f10).
10. **CI lane** — folded into fix 1 above (the cheapest truth-risk fix in
    the repo; consider riding the G11 banned-terms gate on it).
11. **Register the Mantle threshold gap** (f11) in ROADMAP §2.
