# Storage Soak — G16: does the memorial outgrow localStorage?

ROADMAP G16 asks whether the Pilgrim Road's append-only memorial — the
named-men file, the Book of the Fallen, and the world state living in the
automerge document under localStorage — grows without bound against the
~5 MB localStorage budget some browsers enforce. This report answers it
with measurements, not estimates: a soak test drives the **real client
API** (`ClientGame::handle` inputs only, no private-field fabrication)
through worst-case-but-plausible pilgrimages and measures the exact bytes
the persistence lane writes.

Everything here is **measurement only**. No game behavior was changed.

## How to run

```text
cargo test -p crusade-app --test storage_soak -- --ignored --nocapture
```

All four soaks are `#[ignore]`d — CI never runs them; they are a
hand-run measurement lane (like a fuzz target). The run is deterministic
(see "Determinism" below) and finishes in ~12 s.

## What is measured

Three quantities, all read off the real persisted lane:

| Quantity | What it is | Where it lives |
|---|---|---|
| `trail_json_bytes` | The serialized `TrailSave` JSON — the exact string `persist::save_trail` writes under the `trail.save` settings key. | automerge doc (settings map) |
| `doc_bytes` | The automerge document save bytes (`RosterStore::save_bytes`). | the whole save |
| `hex_bytes` | `hex::encode(doc_bytes).len()` — the literal wasm footprint, because `persist.rs` stores the document hex-encoded under `deusvult.roster.v1`. | localStorage |

The hex column is the one to budget against: **the browser sees ~2× the
doc bytes.** A 5 MB localStorage budget therefore means ~2.5 MiB
(2,621,440) of document bytes. The `crusade-data` load path additionally
refuses documents over 8 MiB (`MAX_DOC_BYTES`), a ceiling even further
out.

Memorial counters — Book of the Fallen pages, named-men file size,
living men, store roster size — ride along in every table row.

## Scenario: the zealous worst-case road

The soak models the most storage-hungry plausible player, not an average
one. Each parameter and why:

| Parameter | Value | Justification |
|---|---|---|
| Mantle template | Fully-invested level-50 Anchetil-pattern veteran; grace at the engine cap (15/stat), devotion at its total cap (130) | The plausible strongest leader — keeps the journey (and its daily save writes) alive as long as v1 allows |
| Crossing | charter 255, provisioning 1000 (the best) | Every soul ashore: 20 living men is the worst case for the Book, since every man is a future page |
| March policy | Always the easternmost exit; at the Jaffa terminus the map bounces 24↔25, fighting a scripted gate daily | Maximizes days marched and gate battles |
| Battle cap | 3/day — every raised encounter plus elective `EngageChoice` props up to the cap | A company that seeks out every fight the road offers |
| Champion falls | Conceded every 8th battle (`FALL_EVERY_N_BATTLES = 8`) | Mantle succession + a Book page on a fixed cadence, routed through the real `on_trail_battle_end` defeat path |
| Calls | Every queued Call to Repentance issued | The zealous player; baptisms grow the store roster — the other memorial lane |
| Day ceiling | 40 days (`JOURNEY_DAY_CEILING`) | ROADMAP's worst-case crossing figure; in practice extinction ends runs far sooner |
| Checkpoints | Every 2 days, each with a save/load round-trip verification | Pilgrimages measure ~10 days; a 10-day cadence would capture only the embark and the grave |
| Long horizon | 25 pilgrimages back-to-back on one document | The op history never shrinks; repetition is the unbounded axis |

Battles are fought through the public input surface with the same
best-attack / pick-switch recipe `tests/campaign_tests.rs` and
`tests/client_tests.rs` use, plus an opening two-turn Valour stack (any
player knows to open with their Zealous-Cry-style stage rite).

### Determinism

`add_champion` mints random UUIDs, and the pilgrim seed is derived from
the roster ids — a random fixture would walk a different road every run.
The soak instead pins the veteran under the lexicographic minimum UUID
via `set_champion` (the same public upsert path roster sync uses), so he
is always `roster[0]` and every journey is byte-reproducible run over
run. All numbers below are exact, not averages.

## Result 1: the worst-case single pilgrimage

| day | book_pages | named_men | living | roster | trail_json | doc_bytes | hex_bytes (localStorage) |
|----:|-----------:|----------:|-------:|-------:|-----------:|----------:|-------------------------:|
| 2 | 1 | 20 | 19 | 1 | 2,067 | 951 | 1,902 |
| 3 | 1 | 20 | 19 | 1 | 2,066 | 1,009 | 2,018 |
| 4 | 4 | 20 | 16 | 1 | 2,282 | 1,275 | 2,550 |
| 6 | 5 | 20 | 15 | 1 | 2,357 | 1,394 | 2,788 |
| 8 | 7 | 20 | 13 | 1 | 2,509 | 1,524 | 3,048 |
| 10 | 20 | 20 | 0 | 1 | 3,152 | 1,937 | 3,874 |

Scenario tallies: 8 days marched, 8 battles (1 conceded fall), 0 calls
issued, 0 battle entries refused.

Two structural facts fall out of this table, and they shape everything
else:

1. **A v1 pilgrimage is a death march.** The single-file mantle cannot
   beat the road's 2–3 deep peer-level foe parties with wound carryover
   and no rest (historical foes are `Typing::Valor`, so there is no
   effectiveness edge either). Every pilgrimage grinds the company to
   extinction by ~day 10 — 0 battle wins across the entire soak.
2. **The Book of the Fallen is bounded.** With no recruitment mechanic
   in v1, the named-men file is fixed at the landing company size (20),
   and the Book can never exceed the file. The memorial's per-journey
   footprint is capped by construction. Note the Book also resets per
   embark: each new pilgrimage starts a fresh chapter, so pages do not
   accumulate across pilgrimages within one save document.

The unbounded axes are therefore **time** (days marched — bounded in
practice by extinction) and **repetition** (pilgrimage after pilgrimage
on one document, whose automerge op history never shrinks).

## Result 2: 25 pilgrimages on one document

Each run re-embarks the same fixed company on the same document
(graves acknowledged, survivors abandoned), measuring after each:

| run | book_pages | roster | battles | doc_bytes | hex_bytes (localStorage) |
|----:|-----------:|-------:|--------:|----------:|-------------------------:|
| 1 | 20 | 1 | 8 | 1,937 | 3,874 |
| 2 | 20 | 1 | 8 | 2,883 | 5,766 |
| 3 | 20 | 1 | 8 | 3,835 | 7,670 |
| 5 | 20 | 1 | 8 | 5,716 | 11,432 |
| 10 | 20 | 1 | 8 | 9,946 | 19,892 |
| 15 | 20 | 1 | 8 | 14,333 | 28,666 |
| 20 | 20 | 1 | 8 | 18,704 | 37,408 |
| 25 | 20 | 1 | 8 | 23,068 | 46,136 |

Post-embark document after 25 pilgrimages: **23,217 doc bytes (46,434
hex bytes)**, roster still 1. Growth is cleanly linear at **~880
doc-bytes per pilgrimage** (23,068 − 1,937 over 24 runs = 880.5 B/run)
— pure op-history accumulation; the journey state itself is identical
run over run.

## Result 3: write amplification per trail save

Every road-mutating transition rewrites the `trail.save` settings key —
a last-writer-wins put that appends to the op history even when the
value stays the same size. This soak isolates that per-write cost from
state growth: a real captured save pair (two different days of the same
real journey) is written alternatingly into a real embarked document
for 2,000 writes:

| writes | doc_bytes | hex_bytes (localStorage) |
|-------:|----------:|-------------------------:|
| 250 | 9,572 | 19,144 |
| 500 | 18,348 | 36,696 |
| 1,000 | 35,925 | 71,850 |
| 1,500 | 53,005 | 106,010 |
| 2,000 | 70,687 | 141,374 |

Observed: **~34.9 doc-bytes per save write**, linear across the whole
range, with the journey state held constant. (One pilgrimage performs
roughly 25 save writes — a day's march, its battles, succession events —
which is the ~880 B/pilgrimage figure from Result 2.)

## Result 4: roster growth via baptisms

The Call-to-Repentance flow is the one trail lane that grows the store
roster without bound — each successful Call performs four writes (the
convert instance, his baptismal-name setting, the firstfruits bounty,
the converts mask). The real v1 road never reaches it (the mantle does
not survive its battles, and willing hearers are rare), so this soak
fabricates exactly the four writes `ClientGame::baptize` performs,
using a legal level-45 Bedouin Outrider convert (species 113's own
learnset) through the same public store calls:

| baptisms | doc_bytes | hex_bytes (localStorage) |
|---------:|----------:|-------------------------:|
| 10 | 1,471 | 2,944 |
| 50 | 2,880 | 5,762 |
| 100 | 4,566 | 9,132 |

(Rows 10/50 re-measured at review: 1,471/2,880; the fixture approximates
the baptism writes without the `converted`/`converted_from` fields a real
Call also sets — an underestimate of a few bytes per baptism, immaterial
to the slope or conclusions.)

Observed: **~34 doc-bytes per baptism** (roster state + op history),
linear. 100 baptisms — a document that would take even a devoted player
a v2's worth of play to build — costs under 10 KB in localStorage.

## Growth-rate fit and projection

Summarizing the slopes (all linear, all deterministic):

| Axis | Growth rate | Bound in v1 |
|---|---|---|
| Book of the Fallen pages (trail JSON) | ~57 B/page | 20 pages (fixed company) → ~3.2 KB ceiling |
| Pilgrimages (doc op history) | ~880 B/pilgrimage | none |
| `trail.save` rewrites | ~34.9 B/write | none |
| Baptisms (roster + settings) | ~34 B/baptism | none in code; hearer rarity in practice |
| localStorage overhead | hex = 2 × doc bytes | budget ~5 MB hex ≈ 2.5 MiB doc |

Projected against a **5 MB localStorage budget** (2,621,440 doc bytes):

- **~2,980 pilgrimages** at 880 B/run — roughly 30,000 in-game days of
  marching on a single document. At one pilgrimage per playing session,
  that is eight years of daily play.
- Equivalently ~75,000 individual save writes at 34.9 B/write.
- The `crusade-data` 8 MiB load cap sits ~3× further out (~9,500
  pilgrimages).
- The Book/JSON lane never participates: it caps at ~3.2 KB by
  construction.

## Recommendation

**Within budget — no cap or compaction needed for v1.** The measured
worst-case growth clears the ~5 MB localStorage budget only after
thousands of pilgrimages / tens of thousands of in-game days, far past
any plausible v1 play pattern, and 3× below the hard 8 MiB load cap.

Specifically for ROADMAP G16's proposed mitigations:

- **"Cap named-men roster at a fixed max company size (e.g. 40)" is
  moot in v1.** There is no recruitment mechanic: the named-men file is
  fixed at the landing company (20) and the Book cannot exceed it. The
  only unbounded roster lane is baptisms, which grow at ~34 B each —
  re-evaluate the cap when recruitment or hearer-rich content lands
  (v2's M7/M8 lanes).
- **"Doc compaction (automerge)" is not warranted by the numbers.** If
  it ever becomes warranted, it must compact the op history only —
  re-base the document (load the state into a fresh doc, drop the
  history) — and must never touch the Book of the Fallen pages or the
  roster entries. The memorial is an append-only record; a page is
  never silently dropped. Compaction would be a settings/telemetry
  decision, not a memorial one.

## Findings for other cards

Two real behaviors surfaced by the soak, documented here because the
soak had to tolerate them (both are v1 balance/bug territory, not
storage territory — nothing was changed by this work):

1. **Zeal carryover after a won road battle** (bug). The engine spends
   zeal per rite use (`zeal_left -= 1`), and the victory path of
   `on_trail_battle_end` does not restore it for survivors — unlike the
   campaign lane, which rests its survivors. A mantle that wins a road
   battle then fails `Battle::new` with
   `IllegalChampion(_, IllegalRite(..))` at its next battle entry.
   Note: established by code read (`attack.rs` zeal spend ->
   client.rs:1411-1418 victory clone unrested -> `validate_instance`
   full-zeal gate at types.rs:358-360); the soak observed 0 such
   refusals because the mantle never won. *Suggested fix
   card: field-reset zeal for road survivors on victory, mirroring the
   campaign lane.*
2. **Road battles are structurally unwinnable in v1** (balance). The
   single-file mantle faces 2–3 peer-level foes with wound carryover
   and no rest, with no typing edge available. Every pilgrimage ends
   extinct at ~day 10 with 0 wins — which makes succession, not
   victory, the core trail loop as shipped. Fine for v1's tone, but
   worth stating in the design docs rather than leaving to discovery.
