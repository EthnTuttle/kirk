# REGISTER-LINT — the G11 register gate

Status: **tool shipped, audit complete** (2026-10-17). Tool:
`tools/register_lint.py` (Python 3 stdlib only, no file mutation, no
auto-fixing). Roadmap: docs/ROADMAP.md gap G11 — "a machine-checkable
banned-terms list in `tools/` run as a CI gate over
`crusade-core/src/data/**` and codex tables".

**Audit verdict for the current tree: the player-facing prose is clean.**
All 12 findings are meta-uses: the canon documents quoting the banned
terms in order to ban them (7), and the existing Rust register test's
own banned-terms constant (5). Zero findings in adversary lore, rite
names, codex pages, trail text, Book of the Fallen lines, or
testimonies. Each finding below needs an owner decision (allow or
reword); once the allows land, the gate runs green in CI.

## Usage

```sh
python3 tools/register_lint.py             # gate: prints findings, exit 1 on any
python3 tools/register_lint.py --report    # audit mode: adds scan stats + per-rule counts
python3 tools/register_lint.py --self-test # embedded corpus (hit / miss / attribution / suppression)
```

Exit codes: `0` clean, `1` findings, `2` self-test failure. Output goes
to stdout only in every mode. Suggested CI/pre-commit wiring:

```yaml
- run: python3 tools/register_lint.py   # fails the build on register drift
```

## Scanned surfaces

| Surface | What is scanned |
|---|---|
| `crusade-core/src/data/*.rs` (8 files) | string literals AND comments — names, lore, rite names, register notes |
| `crusade-app/src/{view,conversion,client,persist}.rs` (4 files) | all string literals (player-facing text incl. tests); keys / identifiers / paths / symbol-only literals are skipped |
| `docs/HISTORICAL-FOES.md`, `docs/CONVERSION.md`, `docs/OVERWORLD.md` | prose lines; fenced code blocks and HTML comments skipped |

String literals are extracted by a small scanner (not a parser): it
handles `\"` escapes, `\<newline>` continuations, raw strings, char
literals and lifetimes. Precision over cleverness — anything it misses
or over-flags is handled by the per-line suppression below.

## Encoded rules (8)

Sources: **HF6** = docs/HISTORICAL-FOES.md §6 (THE REGISTER SECTION,
2026-10-16: chronicle-firm, attested citations, never caricature);
**CV8** = docs/CONVERSION.md §8 (banned register for all conversion
text); **DS22** = docs/DOCTRINAL-SINS.md §2.2 vocabulary table (the G11
grounding volume per ROADMAP G1/G11). Matching is case-insensitive with
word boundaries.

| Rule | Flags | Source | Attribution exception |
|---|---|---|---|
| `slur` | raghead, towelhead, camel jockey, muzzie, mooselim, musloid, sand monkey (modern slur class) | HF6.3 | never |
| `mock-arabic` | mahound, mahomet/mohamet/muhammet (mock-spellings of the Prophet's name), ali baba, open sesame, abulbul, "achmed the" (Orientalist-fantasy name fragments) | HF6.3, DS22 | never |
| `anachronism` | turkey shoot, terrorist, jihadi(st), suicide bomber/vest/attack, roadside bomb, shock and awe, collateral damage, friendly fire, headshot (modern war/gamer jargon) | HF6.1 | never |
| `gore-thrill` | splatter, gibber(ish/ing/ed), giblet, pulped, fountains/sprays of blood, gore-drenched | CV8, HF6.1 | never |
| `horde-framing` | horde, grovel(ling), swarthy, sweaty, bestial, bloodthirsty, treachery/treacherous by birth ("heathen horde" framing, physical mockery — banned without exception per HF6.3) | HF6.3, DS22 | never |
| `exonym` | saracen, infidel, paynim, heathen, pagan (period exonyms in narrator voice) | HF6.2, DS22 | **yes** (see below) |
| `coercion` | forced convert, converted him, forced baptism, baptized him by force, compelled his baptism | CV8 | never |
| `game-damnation` | damned to hell, soul is/was damned, soul condemned, burn/roast in hell (the game must not pronounce damnation) | CV8 | never |

The list is deliberately explicit and reviewable — it lives in the
script header (`tools/register_lint.py` docstring) and encodes the
docs' categories, not an exhaustive vocabulary. Extending it is a
one-line table edit plus a self-test case. Per G11 it complements, not
replaces, the kimi-k3#high register review on every prose card.

### Attribution-context exception (`exonym` only)

HF6.2: "Period exonyms (e.g. 'Saracen') live only inside quotation
marks, attributed to a named character or source." A finding is waived
when BOTH hold within the same scanned text unit:

1. the term sits inside quotation marks (heuristic: at least one quote
   character — `"`, `\"`, or typographic — before the match, at least
   one after, and the count before is odd);
2. an attribution marker matches the unit: "says/writes the
   chronicler", "as the age called", "they called him", "according
   to", "in his chronicle", "quoted", "attributed", or a named source
   (William of Tyre, Peter the Venerable, Bernard of Clairvaux, Robert
   of Ketton). Full regex: `ATTRIBUTION_RE` in the script.

Unmarked quoted speech still flags — precision first; a human confirms
via suppression. All other rules have no exception ("banned without
exception", HF6.3).

### Suppression

On the same physical line as the finding:

- Rust: `// reglint:allow <rule>[, <rule> ...]` (a `# reglint:allow`
  form is honored too — the marker text is what matters)
- Markdown: `<!-- reglint:allow <rule>[, <rule> ...] -->`

There is **no allow-all form**: every waiver must name its rule(s).
Markers naming unknown rules (including `all`) are printed as warnings
and suppress nothing.

## Audit of the current tree (2026-10-17)

```
register-lint: scanned 15 files, 1531 prose units

Scanned surfaces:
  crusade-core/src/data/*.rs (literals + comments): 8 files, 561 units, 22 skipped as keys/paths
  crusade-app/src/* (player-facing literals): 4 files, 496 units, 57 skipped as keys/paths
  docs/*.md (prose): 3 files, 474 units
```

### Findings (12) and proposed resolutions

All findings are legitimate meta-uses. Proposed resolution for each is
either **justified-allow** (add the suppression marker with the reason
recorded here) or a small rewording. No content semantics change either
way.

| # | Finding | Rule | Proposed resolution |
|---|---|---|---|
| 1 | `crusade-app/src/conversion.rs:234` — `"saracen"` | exonym | Justified-allow. This is the `register_gate_banned_terms_absent` test's own `BANNED` const (the Rust-side gate, CONVERSION §8 / HF §6 enforcement). Add `// reglint:allow exonym, gore-thrill, coercion` to the line — one marker covers all five literals on it. |
| 2 | `crusade-app/src/conversion.rs:234` — `"infidel"` | exonym | Same marker as #1 (same line). |
| 3 | `crusade-app/src/conversion.rs:234` — `"heathen"` | exonym | Same marker as #1 (same line). |
| 4 | `crusade-app/src/conversion.rs:234` — `"gibber"` | gore-thrill | Same marker as #1 (same line). |
| 5 | `crusade-app/src/conversion.rs:234` — `"forced convert"` | coercion | Same marker as #1 (same line). |
| 6 | `docs/CONVERSION.md:135` — `("converted him")` | coercion | Justified-allow: `<!-- reglint:allow coercion -->`. §8 names the banned coercion verb in order to ban it. Alternative: reword to "object-directed conversion verbs" without the quote — keep the quote, it is the clearer rule text. |
| 7 | `docs/HISTORICAL-FOES.md:172` — `never "gibber"` | gore-thrill | Justified-allow: `<!-- reglint:allow gore-thrill -->`. §6.1 names the banned verb to ban it. |
| 8 | `docs/HISTORICAL-FOES.md:179` — `(e.g. "Saracen")` | exonym | Justified-allow: `<!-- reglint:allow exonym -->`. §6.2 rule text; the term is quoted but the line carries no attribution marker, so the exception correctly does not fire. Alternative: reword to `what the age called "Saracen"`, which would satisfy the attribution exception naturally. |
| 9 | `docs/HISTORICAL-FOES.md:185` — `"heathen horde"` | exonym | Justified-allow: `<!-- reglint:allow exonym, horde-framing -->` covering #9-#11 (same line). §6.3 lists the banned framings in order to ban them "without exception". |
| 10 | `docs/HISTORICAL-FOES.md:185` — `groveling` | horde-framing | Same marker as #9. |
| 11 | `docs/HISTORICAL-FOES.md:185` — `horde` | horde-framing | Same marker as #9. |
| 12 | `docs/HISTORICAL-FOES.md:285` — `"noble Saracen" costume tradition` | exonym | Preferred: add attribution phrasing, e.g. `the later romances' "noble Saracen" tradition, as the costumers called it` — the line already sits in §11 under the [R] rejected-romance marker, so attribution is honest. Fallback: `<!-- reglint:allow exonym -->` with reason "names the rejected-romance tradition by its own label ([R], §11)". |

Note: `docs/HISTORICAL-FOES.md:262` (`"Saracen" may ship ONLY in clearly
attributed quotation`) is **auto-waived** by the attribution exception —
the term is quoted and the line carries an attribution marker. That is
the exception mechanism working as designed on real text.

### What was verified clean

- `crusade-core/src/data/*.rs`: all 561 scanned units (names, lore,
  rite names, comments across adversaries/species/rites/overworld/
  witnesses/callings/chart/mod) — zero findings. The §4 sample-lore
  register (chronicle voice: "yield, stand, hold") holds.
- `crusade-app/src/*`: 496 scanned literals — zero findings outside the
  test gate's own `BANNED` const. Codex/trail/Book of the Fallen text,
  testimonies, and baptismal names pass.
- `docs/OVERWORLD.md`: clean.

### Follow-ups (owner)

1. Apply the justified-allow markers from the table above (or the two
   suggested rewordings for #8/#12) so the gate runs green.
2. Wire `python3 tools/register_lint.py` into CI and/or pre-commit
   (ROADMAP G11).
3. Consider extending surfaces to `crusade-core/src/data/*.rs` test
   strings already covered, plus any new prose table that lands with
   M7/M8 content cards.

## Resolutions applied (2026-10-17, orchestrator; tool review SHIP docs/REGISTER-LINT-REVIEW.md)

- Findings 1-11 resolved with the proposed justified-allow markers
  (`reglint:allow` with named rules) in conversion.rs:234, CONVERSION.md
  §8, HF §6.1/§6.2/§6.3. Finding 8 additionally reworded to "what the age
  called \"Saracen\""; finding 12 reworded to attribute the rejected
  romance tradition ("as the costumers called it"). Both reworded lines
  still trip the quote-count heuristic (other quotes on the same line),
  so they carry the allow marker too — noted here for honesty.
- Reviewer LOW items fixed in the tool: `jihadis?t?` now also matches
  plural "jihadists"; coercion patterns cover her/them forms; the
  attribution regex no longer waives on generic "in the chronicle"
  (possessive only); anachronism source re-cited to the HF §6 head
  clause. Three self-test cases added (17/17 pass).
- The gate runs green at exit 0 over the current tree.

Remaining follow-ups stand: CI wiring, and extending surfaces as new
prose cards land.
