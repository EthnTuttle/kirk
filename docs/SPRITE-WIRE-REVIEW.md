---
title: "Sprite-wiring review: adversary staging + heraldry re-derivation + codex icon seams (SHIP/FIX verdict)"
summary: >
  Review of the uncommitted sprite-wiring lane: stage_sprites.py gained the
  14-row ADVERSARIES table (108..=117 field archetypes + 120..=123
  commanders) with a heraldry-q128-preferred source rule; battle.rs routes
  adversary names to sprites/adversaries/; screens.rs threads 32px codex
  icons into the Roll/Testimonies rows; the 24 champion sprites/icons were
  re-derived from the owner-approved heraldry cells. I re-verified every
  staged pixel MYSELF: all 14 adversary sprites are byte-content-identical
  to the keyed staging-lane q128s, all 33 icons are NEAREST downscales of
  those same keyed sprites (palette subset-checked), all 12 champions now
  carry the heraldry content (HEAD was pre-heraldry; diff is a
  concentrated emblem-region swap). Test pins land exactly, workspace
  421/0 green, wasm32 gate clean, register-lint 0, stage rerun is
  byte-idempotent. Verdict: SHIP.
tags: [review, sprites, adversaries, heraldry, codex, artbake, m7]
status: review
date: 2026-10-19
---

# Sprite-wiring review (2026-10-19)

Scope as assigned: `tools/artbake/stage_sprites.py` (advertary table +
heraldry-preferred source rule), `crusade-app/src/bevy_shell/battle.rs` and
`screens.rs` sprite/codex seams, `crusade-app/assets/sprites/{adversaries,icons}/*`
new files, the 24 re-derived champion sprite/icon files,
`docs/ART-ADVERSARIES.md` + `docs/ART-STATUS.md` edits.

## Verification performed (self-ran, not trusted)

1. **Pixel identity — full table, not a sample.** Wrote an in-tree
   script reimplementing `key_edge_connected_ink` and the staging rules,
   then asserted equality for every row:
   - All **14 adversary sprites** (108..=117, 120..=123) == keyed copies
     of `art/adversaries-staging/nano-banana-2-1/<cell>/q128.png`.
     Deep-compared id 108 the slow way too: 12 755 edge-connected ink
     pixels transparent, **0 content mismatches** against the staging
     q128; adversary staging cells carry NO `heraldry-q128.png`, so the
     new heraldry-preference branch cannot touch them (verified by
     listing the cells).
   - All **33 icons** (champions 001–012, bosses 101–107, adversaries
     108–123) == `NEAREST` 128→32 downscales of the same keyed sprites;
     every icon's RGBA palette is a **strict subset** of its sprite's
     palette — no color/lightness drift anywhere (108: 13 colors in src,
     13 in icon).
   - All **12 champion sprites** == keyed `heraldry-q128.png` ==
     keyed `q128.png` (the cells' two files are byte-identical; the
     heraldry pass already overwrote q128 in staging on 2026-10-16 per
     ART-STATUS §4.18). HEAD's champions compared as
     `keyed(q128-pre-heraldry.png)` — so the 24 modified files are a
     REAL re-derivation: the diff vs HEAD is a concentrated emblem-region
     swap (2 278 px on 001, alpha-key accounting), not a rewrite artifact.
     `art/gallery-notes.json` is not read by stage_sprites (grepped) and
     no gallery/ file moved in git status — it sourced no pixel change.
   - `sprites/adversaries/` contains **exactly** the 14 ADVERSARIES rows,
     nothing extra; `icons/{124,125}.png` correctly absent.
2. **Coverage pinning.** `advserary_sprite_paths_cover_exactly_ids_with_art`
   asserts `ADVERSARIES.len() == staged 14 + 2 pending`, that each of the
   14 resolves to its exact `sprites/adversaries/{id:03}-{slug}.png`, that
   every OTHER row must have id 124|125 and resolve `None`, and that all
   adversaries keep `None` on the anim/arc/verdict resolvers. Test run by
   me against the desktop bin: `battle::tests` 13/13 ok, including
   `adversary_sprite_paths_cover_exactly_ids_with_art`,
   `every_species_resolves_to_its_staged_sprite_path`,
   `anim_paths_resolve_for_staged_sets_only`, `slugify_matches_staged_slugs`.
3. **Resolver collision audit.** Champion and adversary static tables
   share **zero names** (programmatic check over `species.rs` /
   `adversaries.rs`). Name-keyed lookup is therefore one-hit by
   construction; adversary ids outside 108..=117|120..=123 short-circuit
   `None` before any path is built, so 124/125 can never 404-guess a
   battle sprite. The practice trio`s 116 (`PRACTICE_FOE_SPECIES
   [3, 8, 116]`, committed earlier in b75bd7f) resolves "Askari Alamdar"
   → `adversaries/116-askari-alamdar.png`, which exists and was pixel-
   verified above.
4. **The battle.rs:1734 initializer is a genuine, minimal compile fix.**
   HEAD's `net_replay_snapshots_suppress_all_arc_beats` literal omits
   `foes`, `testimonies`, `conversion`, `trail` — all four exist on
   `view::ViewSnapshot` (view.rs:342-358, committed) and carry no
   defaults, so HEAD's crusade-desktop bin does not compile as-is. The
   diff adds exactly those four empty/None initializers and nothing else
   to the test.
5. **Codex seams.** `adversary_row`/`testimony_row` draw the 32px icon via
   the shared `sin_icon` seam: static-table id + AssetServer gated, sealed
   rows keep the icon at `srgb(0.30, 0.30, 0.38)` dim; revealed rows are
   full-tint gold-headed. `assets` is threaded `hagiography_screen` →
   `adversary_roll_panel`/`testimony_panel` → rows (all call sites in the
   diff; no other callers exist). Sealed rows still leak only name +
   "- Unmet"/"- Not yet called", lore/testimony gating untouched.
6. **Tooling.** `python3 tools/artbake/stage_sprites.py` rerun twice:
   `0 files written, 162 unchanged` — byte-idempotent, mtimes unchurned.
   MANIFEST.json: 221 keys; the 19 `*/gallery` entries all map to existing
   `sprites/gallery/{id}-{slug}.png`; adversary keys are the generation
   cost ledger (no asset paths), nothing dangling. PENDING_ART rows print
   the warned skip; adversary-lane-missing skips wholesale.
7. **Gates.** `cargo test --workspace`: **421 passed, 0 failed**.
   `cargo check -p crusade-app --target wasm32-unknown-unknown --features
   bevy-ui`: clean. `register_lint.py`: **0 findings**.

## Findings

- **Nit 1 (comment-only, pre-existing shape):** `FoeCard.species_id`'s doc
  comment reads "(108..=117, 120..=123)" while `conv::COMMANDER_IDS =
  120..=125` means sealed Roll rows for 124/125 DO carry ids whose icon
  file doesn't exist. `sin_icon` spawns an ImageNode for them → one 404'd
  fetch per sealed row that draws nothing; the documented invariant
  ("a 404ing fetch draws nothing over the row's unconditional text")
  makes this behaviorally text-only as intended. Optional tidy-up: clamp
  `COMMANDER_IDS` in the codex card builder or narrow `sin_icon`'s gate;
  not a blocker.
- **Nit 2 (message-only):** on a masters-less box the idempotence summary
  prints "162 files expected" while the 19 gallery plates legitimately
  exist on disk from a masters run (gallery pass correctly reports its
  skip separately). Cosmetic; pre-existing gallery-pass behavior, not
  introduced by this lane.

Both are non-load-bearing; neither alters a pixel, a path, or a test.

## Verdict

**SHIP.**

- Coverage is exactly 108..=117 + 120..=123 adversaries + the two-None
  pending-art rule, and the knobs test pins that exact set (with a table-
  length assertion so adding a 16th adversary row forces a test update).
- Content identity verified pixel-for-pixel across all 47 touched/created
  art files; icons are drift-free subset-palette downscales.
- Champion re-derivation genuinely lands the owner-approved heraldry
  content (HEAD carried the pre-heraldry render); gallery notes sourced
  nothing.
- The resolver cannot collide (name sets are disjoint) or misroute
  (id-range gate before path construction); practice-trio 116 renders.
- The view-snapshot initializer hunk is a real, minimal compile fix.
- Codex icons respect the sealed-dim reveal discipline; wasm32 compiles;
  workspace 421/0; lint 0; stage script byte-idempotent with a sane
  ledger.
