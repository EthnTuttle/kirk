---
title: "Adversary Art Subjects — The Road East (ids 108-117, 120-123)"
summary: "SUBJECTS prompt table for the historical-foes art lane (milestone M7a): 10 archetypes, 4 commander boss portraits, 1 road-east scene header. Descriptors built verbatim from docs/HISTORICAL-FOES.md section 4 wardrobe column; palette locked; sin-depth visual register per tier."
tags: [art, adversaries, road-east, prompts, m7a]
date: 2026-10-16
status: art-subjects
---

# Adversary Art Subjects — The Road East

Binders: `docs/HISTORICAL-FOES.md` sections 3 (arms/armor canon), 4 (wardrobe
column — the source of every descriptor below), 6 rule 4 (sin-depth visual
register), 8 (art lane + the 108 descriptor sample used here as the style
template), and `docs/CONVERSION.md` section 6 (tier -> visual register table).
sin_depth / obstinacy values below are read from the landed M7 data in
`crusade-core/src/data/adversaries.rs`.

## Shared style wrapper and negative prompt

Every subject prepends the standard pipeline STYLE wrapper (see
`tools/artbake/ppq_roster.py::STYLE`): chunky 16-bit JRPG pixel-art game
sprite, limited retro palette, full-body readable silhouette, centered, plain
solid dark-navy background (#0B0E1B), no text/letters/watermark/border/frame.
All adversaries are pixel sprites **facing right** at 128x128 production canvas,
quantized by `quant.py` to the LOCKED 16-color palette. Faction accents use
EXISTING palette slots only (HF 8): red `#A62639` (Rum), ink `#0B0E1B`
(Zengid standarts), palegold `#F4D35E` (attested Ayyubid yellow). No palette
mutation. Fatimid foot and auxiliaries carry no faction accent (untouched
plain kit is itself the Fatimid read); their accents read as bone/leather
materials already in palette.

Every subject appends this full shared TAIL (honorable-foe register +
banned-cliche negatives, HF 3 + 6 rule 3):

> Dignified medieval Levantine professional soldier of 1095-1192, face calm
> and dignified, honorable man, never caricature. Straight double-edged sword
> only where a sword appears. NO deep-curved scimitar or saber, NO
> forward-curved yataghan, NO bulbous turban-shaped helmet (turbans are wound
> cloth over small caps, never cast metal), NO barbary-pirate robes or sashes,
> NO combat camels, NO exaggerated or sneering faces, no ethnic caricature,
> no words or banners with writing.

Commanders (120-123) are static boss portraits per the sin-boss rule (HF 8):
single dominant pose, no animation family.

Sin-depth visual directives (CONVERSION 6 / HF 6.4) are rendered as
SPIRITUAL CONDITION readable on the face and bearing only — never ethnicity
or kit slander:
- **tier 0** ("plain, dignified man"): open calm face, even light, no
  shading on the eyes, upright relaxed bearing.
- **tier 1** ("plain, harder bearing"): plain dignified man with a harder,
  drilled bearing; set jaw, level gaze; no darkening.
- **tier 2** ("shadowed eyes, ashen drift"): shadowed eyes (brow shadow
  using existing steeldk/bruise slots), a faint ashen drift on steel-grey
  surfaces (dimbone/steeldk), dignified still — zealot elite, not monster.
- **tier 3** ("sin-boss-adjacent commander cadre / visible corruption"):
  heavier shadowed eyes, ashen drift across armor and face light, cold
  midnight rim; reads as spiritual weight clear of any ethnic feature.

## The table

NEG below = the full shared TAIL quoted verbatim above (appended in full to
every prompt at generation time).

| id | slug | tier (sin_depth / obstinacy) | faction accent | subject descriptor (kit terms verbatim from HF 4) |
|---|---|---|---|---|
| 108 | turcoman-bowrider | 0 (28) — plain, dignified man | Rum red `#A62639` (saddle-cloth band only) | a lean horseman at the gallop on a small pony, fur-trimmed kalpak cap over a wrapped turban, belted khaftan, riding boots, composite bow drawn, small round hide shield slung, dust rising; dark-navy background; face calm and dignified; pixel sprite facing right |
| 109 | ghulam-lancer | 1 (42) — harder bearing | Zengid ink `#0B0E1B` (shield rim) | a household heavy cavalryman on an armored horse, maille hauberk over a khaftan, conical khud helm with maille aventail, kite shield, couched two-handed lance, straight saif at the belt; dark-navy background; face calm and dignified; pixel sprite facing right |
| 110 | sudani-shieldbearer | 1 (40) — harder bearing | none (plain white kit) | a great-shield spearman behind a towering full-body hide shield, short thrusting spear, plain white tunic, head-cloth; no metal; dark-navy background; face calm and dignified; pixel sprite facing right |
| 111 | armenian-bowman | 0 (25) — plain, dignified man | none (no accent) | a foot archer with a composite bow taller than a horseman's bow, open coat, soft cap, tall quiver at his hip, small knife only; dark-navy background; face calm and dignified; pixel sprite facing right |
| 112 | daylamite-axeman | 1 (45) — harder bearing | none (no accent) | a hill-foot shock infantryman with two tabar axes (one slung), a zhupin javelin in hand, knee-length maille shirt, round shield on his back; dark-navy background; face calm and dignified; pixel sprite facing right |
| 113 | bedouin-outrider | 0 (22) — plain, dignified man | none (no accent) | a light-horse scout on a lean pony at a walk, flowing head-cloth and mantle, no armor, light lance carried loose, small hide buckler; dark-navy background; face calm and dignified; pixel sprite facing right |
| 114 | kurdish-ghazw-rider | 2 (55) — shadowed eyes, ashen drift | palegold `#F4D35E` (shield boss band, Ayyubid-attested yellow) | a spear-armed tribal cavalryman, jawshan lamellar corselet over a khaftan, turban wound over a small cap (cloth, not metal), round shield, spear held over-arm, straight saif at the hip; dark-navy background; shadowed eyes and a faint ashen drift reading as spiritual condition, never ethnicity; pixel sprite facing right |
| 115 | naffatun-sapper | 1 (40) — harder bearing | Zengid/Ayyubid ink `#0B0E1B` (apron ties) | a naphtha-pot thrower crossing the field at a walk, padded hood, wet rawhide apron over plain clothes, a clay grenade cradled in one arm, a torch held low in the other; dark-navy background; face calm and dignified; pixel sprite facing right |
| 116 | askari-alamdar | 2 (58) — shadowed eyes, ashen drift | regimental banner in faction accent per node (palegold shaft cloth shown; banner swaps at qty per HF 8) | the banner-bearer of a lord's askar, maille and lamellar half-suit, a tall regimental banner planted butt-down beside him, straight saif held low; dark-navy background; shadowed eyes and faint ashen drift as spiritual condition, never ethnicity; pixel sprite facing right |
| 117 | muqaddam-of-the-tulb | 3 (75) — visible corruption (mini-boss) | Rum red `#A62639` (horse barding band) | a tulb officer on a barded horse, full lamellar cuirass over maille, conical khud helm with nasal, mace raised in signal, straight saif sheathed; heavy shadowed eyes and ashen drift across the armor as spiritual condition, never cruelty or caricature; dark-navy background; pixel sprite facing right |
| 120 | kilij-arslan-i | 3 (never converts) — visible corruption; static boss portrait | Rum red `#A62639` (saddle-trim and shield face) | a hard, intelligent border prince — the Sultan of Rum at Dorylaeum: fine maille hauberk and lamellar over a rich but soldierly khaftan, conical khud helm with aventail, small rounded baydah helm carried at the saddle, composite bow cased and a straight saif at the hip, mounted and turning to watch the plain; heavy shadowed eyes, ashen drift as spiritual condition; dark-navy background; static boss portrait, single dominant pose; pixel sprite facing right |
| 121 | nur-ad-din | 3 (never converts) — visible corruption; static boss portrait | Zengid ink `#0B0E1B` (standart cloth) | the atabeg of Aleppo and Damascus, austere and just by both sides' telling: plain dark maille and lamellar with no ornament, khud helm with nasal, an ink-dark standart beside him, straight saif drawn low; heavy shadowed eyes, ashen drift as spiritual condition; dark-navy background; static boss portrait, single dominant pose; pixel sprite facing right |
| 122 | asad-al-din-shirkuh | 3 (never converts) — visible corruption; static boss portrait | Zengid ink `#0B0E1B` (cloak clasp) | the old mountain lion of the Egypt marches: a weathered commander in road-worn maille and lamellar over a dusty khaftan, khud helm in hand rather than worn, straight saif at the belt, standing on a wind-blown road; heavy shadowed eyes, ashen drift as spiritual condition; dark-navy background; static boss portrait, single dominant pose; pixel sprite facing right |
| 123 | saladin | 3 (never converts) — visible corruption; static boss portrait | palegold `#F4D35E` (banner cloth, attested Ayyubid yellow) | the Sultan at Hattin: jawshan lamellar over maille with a palegold-bannered standart behind him, khud helm with aventail, straight saif in hand lowered; courteous, commanding, grave; heavy shadowed eyes and ashen drift as spiritual condition, never the "noble Saracen" costume tradition (no kilij curve, no turban-helm, no Scott romance); dark-navy background; static boss portrait, single dominant pose; pixel sprite facing right |
| — | road-east-header | n/a (scene header, not a fighter) | palegold `#F4D35E` sky band, red `#A62639` far dust | a wide "road east" scene header tile: a dusty pilgrim road climbing toward distant Anatolian hills at dawn, far-off horsemen dust on the horizon, no figures close, no text; palegold dawn sky band over deep-navy distance; pixel-art scene tile, 128x128 |

## Generation notes

- Lane: `nano-banana-2-1` via PayPerQ only for the $0.00 pass; `gpt-image-1.5`
  (quality=low, $0.023/img, ~$0.35 total) is the documented contingency
  alternate (HF 8 backlog).
- Staging: `art/adversaries-staging/<model>/<subject>/{raw,q64,q128}.png`,
  mirroring `art/roster-staging/`; q128 is the production candidate; icons
  downscale 32x32 later via stage_sprites.py (NOT yet extended with an
  ADVERSARIES table — a later card after eyeball QA, per HF 8
  "one prompt/eyeball/retry pass").
- `client_request_id` convention: `kirk-adv-<model>-<subject>-vN`.
- NEG (the shared TAIL above) is appended verbatim to every prompt; extra
  per-model caption wording from the 4.5 fixes ("absolutely no captions, no
  labels, no words, no letters"; "every pose faces right") applies too.

## Status

- 2026-10-16: table written (15 rows: 10 archetypes 108-117, 4 commanders
  120-123, 1 scene header). sin_depth tiers read from the landed M7
  `crusade-core/src/data/adversaries.rs` table.
- **Generation COMPLETE 2026-10-16 (15/15, $0.00, nano-banana-2-1 only).**
  Driver `/tmp/opencode/sprite-bakeoff/adv_batch.py` (scratch; STYLE/NEG/CAPFIX
  verbatim from this doc + ppq_roster.py). Every cell staged at
  `art/adversaries-staging/nano-banana-2-1/<subject>/{raw,q64,q128}.png`,
  q128 = 128x128 LOCKED-palette, verified 0 off-palette px and 0
  semi-transparent px across all 15. 15 ledger entries appended to
  `tools/artbake/MANIFEST.json` (`kirk-adv-nano-banana-2-1-<subject>-vN`).
  worker eyeball pass: 14/15 accepted at v1 (historical kits read as kit
  contrast, no scimitar/turban-helm/barbary/camel/caricature motifs, faces
  calm); **road-east-header** was a v1 subject-drift foul (sprite framing
  produced a figure instead of a landscape) and passed at v2 after the driver
  gained a scene-tile style wrapper for that one cell — its ledger key is `-v2`.
  One retry, zero doubles-failures, gpt-image-1.5 lane never touched.
- Nothing wired into crusade-app (correct — the stage_sprites ADVERSARIES
  table + icons are a later card after human eyeball QA per HF 8). The
  worker eyeball above is the generation gate, not that human QA.
