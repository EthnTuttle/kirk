---
title: "Register-lint review: G11 banned-terms gate (tools/register_lint.py)"
summary: >
  Review of the uncommitted G11 addition (tools/register_lint.py +
  docs/REGISTER-LINT.md). The gate is faithful to its card: stdlib-only,
  no mutation, per-rule suppression with no allow-all, attribution
  exception confined to the exonym rule and requiring quotes + marker,
  exit 1 on findings. All 12 reported audit findings reproduce exactly
  and all proposed resolutions are sound. Findings are all LOW/NIT:
  small regex under-coverage (plural "jihadists", them/her coercion
  forms), an attribution-marker alternation that is looser than HF §6.2's
  "named character or source" requirement, and one mis-cited rule source.
tags: [review, tools, register-lint, G11, historical-foes, conversion]
status: review
date: 2026-10-17
---

# Register-lint review (2026-10-17)

## Scope

- `git status`: exactly two new files — `tools/register_lint.py` (658
  lines) and `docs/REGISTER-LINT.md` (159 lines). Nothing else is in the
  diff under review (`crusade-app/tests/storage_soak.rs` is untracked but
  outside this card).
- Card constraints: stdlib-only; scan `crusade-core/src/data/*.rs`
  literals+comments, `crusade-app/src/{view,conversion,client,persist}.rs`
  player-facing literals, prose of HISTORICAL-FOES/CONVERSION/OVERWORLD;
  banned-terms table derived from and sourced to HF §6 + CV §8;
  attribution exception only for exonym; per-rule `reglint:allow`, no
  allow-all; exit non-zero on findings; no mutation; self-test.

## Verification performed

- **Source re-read:** HF §6 (lines 162-205), CV §8 (lines 133-139), and
  DS §2.1/§2.2 (the cited grounding volume) read in full and mapped
  rule-by-rule against the encoded table. Sections 6.1 "never gibber" /
  joy-not-conquest-candy, 6.2 exonym-in-attributed-quotation, 6.3
  banned-without-exception list, CV8's four Never categories, and the
  DS §2.2 table all line up with what the tool encodes.
- **Live run (self):** `python3 tools/register_lint.py --report` —
  scanned 15 files / 1531 prose units, exit 1, and the printed 12
  findings match the audit table in REGISTER-LINT.md cell-for-cell
  (conversion.rs:234 ×5, CONVERSION.md:135, HF:172, HF:179, HF:185 ×3,
  HF:285). The doc's claim that HF §10 item 3 ("Saracen" may ship ONLY
  in clearly attributed quotation) is auto-waived also checks out under
  my run, and that is the exception mechanism working as designed on
  real text.
- **Extractor spot-checks (adversarial corpus, run against the module
  directly):** raw string `r#"...the paynim host..."#` flags; multi-line
  literal with `\`-continuation flags on the correct interior line;
  apostrophes in comments (`// don't ... hordes`) don't derail the state
  machine; multi-line block comments scan with correct line numbers;
  single-word literal `"Saracen"` flags (not skipped as keylike);
  escaped quotes in a literal do not fool the quote heuristic when
  attribution is absent; `according to ...` without quotes still flags.
  Byte-string/raw-string/concat!/include_str! sweep of all scanned
  files: none present in shapes that could skip prose (the two
  `r#"{...}"#` hits are JSON test fixtures in mod.rs/view.rs/persist.rs).
- **Cross-check grep:** an independent regex sweep of all scanned
  surfaces for the full banned vocabulary found only the 12 tool
  findings plus two doc comments in conversion.rs (:20, :229) that quote
  "Saracen" — app files are literals-only by card, and both instances
  would be auto-waived anyway (quoted + "attributed"). No silent skips
  of live prose.
- **Suppression:** unknown rule names warn-and-deny (`reglint:allow hord`
  → warning, finding stands); bare marker and `all` rejected (self-test
  cases 7/8); no allow-all codepath exists; `# reglint:allow` honored as
  documented. Confirmed the documented limitation that a marker must sit
  on the same physical line as the match — for multi-line literals a
  marker on the closing line does not suppress (matches the doc text).
- **Proposed resolutions reviewed at the source lines:** the nine
  justified-allows are all meta-uses on the rule/test lines themselves.
  The HF:179 alternative rewording (`what the age called "Saracen"`) is
  the *secondary* option and does not weaken the §6.2 rule sentence —
  the primary proposal (allow marker) is the right one anyway. The
  HF:285 rewording (`... as the costumers called it`) is honest
  attribution inside §11's [R] rejected-romance bullet and leaves the
  rejection force intact. No canon weakening in either.
- **Style/ops:** same tool conventions as tools/art_gallery.py
  (shebang, module docstring with usage, `ROOT =
  Path(__file__).resolve().parent.parent`, `from __future__ import
  annotations`); imports are argparse/re/sys/dataclasses/pathlib only;
  filesystem access is `read_text` only — no mutation, stdout-only
  output, exit codes as documented (verified 1 live; 2 path reads
  clean).

## Findings

### LOW — attribution-marker grammar is looser than HF §6.2's "named"

HF §6.2: period exonyms live in quotation "attributed to a **named**
character or source". `ATTRIBUTION_RE` also accepts
`in\s+(?:his|her|their|**the**)\s+(?:chronicle|annals?|...)` — and I
demonstrated both `According to the annals the \"Saracens\" held the
pass` and `In the chronicle, the \"paynim\" fell` auto-waive with no
named character or source anywhere. The exception still requires quotes
+ marker (the card's bar is met), and nothing in the current tree hits
this — but a future lore line like *"In the chronicles of the march,
the 'Saracen' vanguard..."* would be silently waived, under the one
exception on the register's most sensitive rule. Fix is one word: drop
`the` from the possessive alternation (possessives imply a character;
named sources have their own alternation) and rely on human
`reglint:allow` for the generic-source case. Cheap; do it in the same
polish pass as the allows.

### LOW — regex under-coverage against the docs' own classes

- `jihadis?t?` misses **"jihadists"** (the plural of an explicitly
  listed singular): `\b` fails after the consumed `t`, and backtracking
  sheds letters rather than matching. `suicide bomb`/`bombing` also
  evade (only bomber/vest/attack covered).
- `coercion` is him/his-only: "converted them", "forced conversion of
  the garrison", "compelled their baptism" all evade, though CV §8 bans
  coercion verbs as a class.
- `mock-arabic` implements DS §2.2's "any mock-spelling" with four
  specific spellings — "Mahomed"/"Muhammed"-style variants evade.
- `game-damnation` misses "condemned to hell" / bare "damned"
  paraphrases of CV §8's game-must-not-damn rule.

Mitigants: zero such text exists anywhere in-tree (cross-checked by
independent grep); the doc discloses "encodes the docs' categories, not
an exhaustive vocabulary"; and ROADMAP G11 pairs this lint with
kimi-k3#high register review on every prose card. Still, the anachronism
and coercion rows can be tightened without new false positives
(`jihadi(?:sts?|s)?`, `suicide\s+bomb\w*`, `converted\s+(?:him|them|her)`,
`(?:forced|compelled)\s+\w+\s+(?:baptism|conversion)`) — worth one
follow-up edit plus matching self-test cases.

### LOW — rule-source mis-citation and one source conflict

- The `anachronism` rule cites "HF6.1" as its source; §6.1 is the
  never-gibber / redeemable-men rule and contains no anachronism ban.
  The real (and adequate) grounding is §6's head clause — "the register
  is 'chronicle-firm, not cruel' ... binds every downstream card" —
  plus the tool's own disclosed category-encoding. The specific
  war-jargon vocabulary appears in no source document; the citation
  should say so.
- `pagan` sits inside the attribution-exception rule, but DS §2.2 bans
  pagan outright ("❌ banned", no quotation carve-out), while HF §6.2
  (newer, and explicitly binding on follow-on cards) grants the exonym
  class the attributed-quotation lane. The tool's choice is defensible
  under HF's precedence; flag it for the owner when the allow-marker
  pass lands, so the two docs stop disagreeing.

### NIT — multi-line-literal suppression ergonomics

A `reglint:allow` on a multi-line literal's closing line does not
suppress a match earlier in the literal; the marker must sit on the
interior physical line. This is exactly what the docs say ("on the same
physical line as the finding"), and single-line `\`-continuation style
in the data files means a marker on the term's own line works — just
worth remembering when the first multi-line suppression is needed.

## Register check (docs/HISTORICAL-FOES.md section 6)

The tool's own prose (docstrings, REGISTER-LINT.md) is chronicle-firm
and scrupulously attributed; the audit table's verdict "the
player-facing prose is clean" is true under independent re-verification.

## Verdict

**SHIP** — every card requirement is met and verified: surfaces,
rule-table fidelity to HF §6/CV §8/DS §2.2 (with the two LOW
citation/precision notes above), exonym-only attribution exception
requiring quotes AND marker, warn-and-deny per-rule suppression with no
allow-all, exit 1 on findings, no mutation, self-test. All 12 audit
findings and resolutions are accurate as documented, and neither
proposed rewording weakens canon. The LOW items (tighten the
attribution alternation and the two regex rows, fix the HF6.1 citation)
are follow-up polish that should ride the same commit as the
justified-allow markers, not gate the tool's adoption.
