//! G16 storage-growth soak (docs/ROADMAP.md): does the Pilgrim Road's
//! append-only memorial — the named-men file + the Book of the Fallen —
//! plus the automerge document's write history threaten the ~5 MB
//! localStorage budget some browsers enforce?
//!
//! MEASUREMENT LANE ONLY: every test here is `#[ignore]`d so CI never runs
//! them. Run by hand with:
//!
//! ```text
//! cargo test -p crusade-app --test storage_soak -- --ignored --nocapture
//! ```
//!
//! The soaks drive the REAL client API — `ClientGame::handle` inputs only,
//! no private-field fabrication. Battles are fought through the public
//! input surface with the same best-attack/pick-switch recipe
//! `tests/campaign_tests.rs` and `tests/client_tests.rs` use (plus an
//! opening Valour stack, a rite any player knows to open with); a
//! scheduled champion fall is a plain `Concede`, which routes through the
//! real `on_trail_battle_end` mantle succession. What is measured:
//!
//! - `trail_json_bytes`: the serialized `TrailSave` JSON — the exact string
//!   `persist::save_trail` writes under the `trail.save` settings key.
//! - `doc_bytes` / `hex_bytes`: the automerge document save bytes
//!   (`RosterStore::save_bytes`) and their hex encoding — the literal
//!   localStorage footprint on wasm (persist.rs stores the doc hex-encoded,
//!   so the browser sees ~2x the doc bytes).
//! - `book_pages` / `named_men` / `store_champions`: the memorial counters.
//!
//! Findings, the growth-rate fit, and the budget projection live in
//! docs/STORAGE-SOAK.md. Headline (measured, deterministic): a v1
//! pilgrimage is a death march — the single-file mantle cannot beat a
//! 2-3 deep peer-level foe party with wound carryover, so every journey
//! grinds the company to extinction within ~2 weeks. The Book of the
//! Fallen is therefore bounded at the landing company size (20 in v1),
//! and the only unbounded axes are TIME (days marched before extinction)
//! and REPETITION (pilgrimage after pilgrimage on one document, whose
//! automerge op history never shrinks).

use crusade_app::persist::{self, TRAIL_SAVE_KEY};
use crusade_app::{AppError, ClientGame, ClientInput, Phase, ViewSnapshot};
use crusade_core::data;
use crusade_core::engine::BattleError;
use crusade_core::types::{
    BattleStat, ChampionInstance, DevotionStats, GraceStats, RiteSlot, Secondary,
};
use crusade_data::RosterStore;

// ---------- Scenario: "the zealous worst-case road" ----------
//
// Every parameter below is a worst-case-but-plausible bound, justified in
// docs/STORAGE-SOAK.md section "Scenario":
//
// - JOURNEY_DAY_CEILING = 40: the ROADMAP's worst-case crossing figure
//   (the v1 spine is 25 screens, ~20 marches from the landing to the Jaffa
//   terminus; 40 days adds a loitering tail). In practice the company is
//   extinct long before this — the ceiling only ever terminates a run the
//   road failed to end.
// - CHECKPOINT_EVERY = 2: pilgrimages measure ~9-15 days, so a 10-day
//   cadence would capture only the embark and the grave.
// - BATTLES_PER_DAY_MAX = 3: every raised encounter fought plus elective
//   choice-encounter props engaged up to the cap — a company that seeks
//   out every fight the road offers (maximizes save writes per day).
// - FALL_EVERY_N_BATTLES = 8: the mantle is conceded (falls, succession
//   runs, a Book page is written) every 8th battle, on top of the battles
//   the road wins off it anyway.
// - charter 255 / provisioning 1000: the BEST crossing — every soul
//   ashore (20 living), the worst case for the Book, since every man is a
//   future page and the largest file survives reorders longest.
// - The mantle template is a fully-invested level-50 Anchetil-pattern
//   veteran (grace at the engine cap, devotion at its total cap): the
//   plausible strongest leader, so the journey lives as long as v1 allows.
// - Every queued Call to Repentance is issued (the zealous player): each
//   success baptizes a convert into the store roster — the other memorial
//   lane that can grow across pilgrimages.

/// Journey-day ceiling; extinction ends runs far sooner (see header).
const JOURNEY_DAY_CEILING: u16 = 40;
/// Checkpoint cadence (a table row + a save/load verification every N days).
const CHECKPOINT_EVERY: u16 = 2;
/// Daily battle cap: raised encounters + elective engagements.
const BATTLES_PER_DAY_MAX: usize = 3;
/// Concede (champion falls -> mantle succession) every Nth battle.
const FALL_EVERY_N_BATTLES: u64 = 8;
/// How many back-to-back pilgrimages the long-horizon soak runs.
const PILGRIMAGES: usize = 25;

// ---------- Fixtures (mkchamp pattern from tests/client_tests.rs) ----------

fn mkchamp(species_id: u16, level: u8, rite_ids: &[u16]) -> ChampionInstance {
    let sp = data::species_by_id(species_id).expect("fixture species");
    let mut rites: [Option<RiteSlot>; 4] = [None; 4];
    for (i, id) in rite_ids.iter().enumerate() {
        let r = data::rite_by_id(*id).expect("fixture rite");
        assert!(
            sp.learnset
                .iter()
                .any(|e| e.rite_id == *id && e.level <= level),
            "fixture rite {id} not learnable by species {species_id} at {level}"
        );
        rites[i] = Some(RiteSlot {
            rite_id: *id,
            zeal_left: r.zeal,
        });
    }
    let mut c = ChampionInstance {
        species_id,
        level,
        xp: 0,
        grace: GraceStats {
            resolve: 0,
            valour: 0,
            piety: 0,
            guard: 0,
            aegis: 0,
            alacrity: 0,
        },
        devotion: DevotionStats {
            resolve: 0,
            valour: 0,
            piety: 0,
            guard: 0,
            aegis: 0,
            alacrity: 0,
        },
        calling_id: 0,
        rites,
        current_resolve: 0,
        ailment: None,
        converted: false,
        converted_from: None,
        typing_override: None,
    };
    c.current_resolve = data::compute_stats(&c, sp).max_resolve;
    c
}

/// A fully-invested veteran: the mkchamp fixture with every grace stat at
/// the engine cap (GRACE_MAX = 15) and devotion spread to its legal total
/// cap (DEVOTION_TOTAL_MAX = 130) — a long-trained roster lead, the
/// plausible strongest mantle the road can field (see the scenario note).
fn mkveteran(species_id: u16, level: u8, rite_ids: &[u16]) -> ChampionInstance {
    let mut c = mkchamp(species_id, level, rite_ids);
    c.grace = GraceStats {
        resolve: 15,
        valour: 15,
        piety: 15,
        guard: 15,
        aegis: 15,
        alacrity: 15,
    };
    // 22*4 + 21*2 = 130: the devotion total cap, spread evenly.
    c.devotion = DevotionStats {
        resolve: 22,
        valour: 22,
        piety: 22,
        guard: 22,
        aegis: 21,
        alacrity: 21,
    };
    let sp = data::species_by_id(c.species_id).expect("veteran species");
    c.current_resolve = data::compute_stats(&c, sp).max_resolve;
    c
}

// ---------- Battle driver (public inputs only) ----------

fn switch_idx(target: &str) -> u8 {
    target
        .split(':')
        .next()
        .expect("can_switch_to entries look like '<idx>:<name>'")
        .parse()
        .expect("party index prefix parses")
}

/// Highest-power usable damaging rite; ties resolve to the lowest slot.
fn best_attack(snap: &ViewSnapshot) -> Option<u8> {
    let mut best: Option<(u8, u8)> = None; // (slot, power), slots ascend
    for r in snap.my_rites.iter().filter(|r| r.usable && r.power > 0) {
        if best.is_none_or(|(_, p)| r.power > p) {
            best = Some((r.slot, r.power));
        }
    }
    best.map(|(slot, _)| slot)
}

/// Soak tallies, for the scenario summary printed under the table.
#[derive(Default)]
struct SoakCtx {
    battles: u64,
    conceded_falls: u64,
    calls_issued: u64,
    days_marched: u16,
    /// Road battles the client refused to start: after a WON battle the
    /// mantle's spent zeal fails `Battle::new` validation at the next
    /// battle entry (v1 bug — the victory path persists survivors unrested
    /// in zeal, unlike the campaign lane's rest; see STORAGE-SOAK.md). The
    /// day still marched and saved; only the battle fails to build.
    refused_battles: u64,
}

/// Fight the live battle through the public input surface, except every
/// FALL_EVERY_N_BATTLES-th battle, which is conceded at once so the mantle
/// falls and succession runs through the real defeat router. Opening
/// policy: stack the Valour stage for two turns (any Zealous-Cry-style
/// status rite, read off the static rite table the way a player knows
/// their own kit), then strike with the best damaging rite.
fn fight_or_concede(game: &mut ClientGame, ctx: &mut SoakCtx) {
    ctx.battles += 1;
    if ctx.battles % FALL_EVERY_N_BATTLES == 0 {
        ctx.conceded_falls += 1;
        game.handle(ClientInput::Concede)
            .expect("yielding the field is legal in battle");
        return;
    }
    let mut turns = 0u32;
    while game.phase() == Phase::InBattle {
        let snap = game.snapshot();
        let boost = if turns < 2 {
            snap.my_rites.iter().find(|r| {
                r.usable
                    && data::rite_by_id(r.rite_id).is_some_and(|rite| {
                        rite.power == 0
                            && rite.secondaries.iter().any(|s| {
                                matches!(
                                    s,
                                    Secondary::StageOnSelf {
                                        stat: BattleStat::Valour,
                                        ..
                                    }
                                )
                            })
                    })
            })
        } else {
            None
        };
        let input = if let Some(r) = boost {
            ClientInput::PickRite { slot: r.slot }
        } else if let Some(slot) = best_attack(&snap) {
            ClientInput::PickRite { slot }
        } else if let Some(t) = snap.can_switch_to.first() {
            ClientInput::PickSwitch {
                party_idx: switch_idx(t),
            }
        } else {
            // Zeal exhausted with no bench: the farce ends here.
            ClientInput::Concede
        };
        game.handle(input).expect("battle turn is legal");
        turns += 1;
        assert!(turns < 2000, "a road battle did not terminate");
    }
}

/// Settle everything the current moment raised: the live battle (one
/// SoakCtx cadence step), any forced switch, and every queued Call to
/// Repentance (the zealous player always issues the Call — baptisms grow
/// the store roster, the other memorial lane under measurement).
fn settle(game: &mut ClientGame, ctx: &mut SoakCtx, battles_today: &mut usize) {
    loop {
        match game.phase() {
            Phase::InBattle => {
                fight_or_concede(game, ctx);
                *battles_today += 1;
            }
            Phase::ForcedSwitchSelect => {
                let snap = game.snapshot();
                let target = snap
                    .can_switch_to
                    .first()
                    .expect("a forced switch always has a target")
                    .clone();
                game.handle(ClientInput::PickSwitch {
                    party_idx: switch_idx(&target),
                })
                .expect("forced switch is legal");
            }
            Phase::ConversionChoice => {
                ctx.calls_issued += 1;
                game.handle(ClientInput::CallToRepentance)
                    .expect("the call is legal while a hearer waits");
            }
            _ => return,
        }
    }
}

// ---------- Measurement ----------

/// One checkpoint row of the storage table.
struct Row {
    day: u16,
    book_pages: usize,
    named_men: usize,
    living: usize,
    store_champions: usize,
    trail_json_bytes: usize,
    doc_bytes: usize,
    hex_bytes: usize,
}

/// Snapshot every quantity G16 cares about, off the real persisted lane:
/// the `trail.save` JSON exactly as `persist::save_trail` writes it, the
/// automerge document bytes, and the hex form persist.rs actually stores
/// in localStorage on wasm.
fn measure(game: &mut ClientGame) -> Row {
    let save = persist::load_trail(game.store())
        .expect("store read")
        .expect("the trail lane is saved");
    let json = serde_json::to_string(&save).expect("trail save serializes");
    let doc = game.store().save_bytes().expect("document saves");
    let store_champions = game.store().list_champions().expect("roster lists").len();
    Row {
        day: save.day,
        book_pages: save.fallen.len(),
        named_men: save.men.len(),
        living: save.men.iter().filter(|m| m.alive).count(),
        store_champions,
        trail_json_bytes: json.len(),
        doc_bytes: doc.len(),
        hex_bytes: hex::encode(&doc).len(),
    }
}

/// Mid-journey save/load: the document round-trips and the trail DTO comes
/// back byte-faithful (the same reload discipline the M8b lib tests pin,
/// exercised at every checkpoint of the soak).
fn verify_reload_round_trip(game: &mut ClientGame) {
    let before = persist::load_trail(game.store())
        .expect("read")
        .expect("the lane is saved");
    let bytes = game.store().save_bytes().expect("document saves");
    let mut reloaded = ClientGame::new(RosterStore::load_bytes(&bytes).expect("document loads"));
    assert_eq!(reloaded.phase(), Phase::Trail, "the road resumes on load");
    let after = persist::load_trail(reloaded.store())
        .expect("read")
        .expect("the lane is saved");
    assert_eq!(
        after, before,
        "the trail DTO is byte-faithful through the document"
    );
    assert_eq!(
        reloaded.snapshot().trail,
        game.snapshot().trail,
        "the journey view agrees after the reload"
    );
}

fn print_table(rows: &[Row]) {
    println!(
        "{:>5} {:>10} {:>9} {:>6} {:>8} {:>15} {:>9} {:>24}",
        "day", "book_pages", "named_men", "living", "roster", "trail_json", "doc_bytes", "hex_bytes(localStorage)"
    );
    for r in rows {
        println!(
            "{:>5} {:>10} {:>9} {:>6} {:>8} {:>15} {:>9} {:>24}",
            r.day,
            r.book_pages,
            r.named_men,
            r.living,
            r.store_champions,
            r.trail_json_bytes,
            r.doc_bytes,
            r.hex_bytes
        );
    }
}

// ---------- The soaks ----------

/// Embark: one fully-invested level-50 Anchetil-pattern veteran leads the
/// file, the best possible crossing lands every soul.
fn embark(game: &mut ClientGame) {
    game.handle(ClientInput::StartTrail {
        charter_quality: 255,
        provisioning_spend: 1000,
    })
    .expect("the crossing is legal from the menu");
}

/// The soak document: one fully-invested level-50 Anchetil-pattern veteran
/// under a FIXED id. The id matters twice over: the pilgrim seed is minted
/// from the roster ids (client.rs `pilgrim_seed`), and the embark template
/// is `roster[0]` by id sort — a random `add_champion` uuid would make
/// every run of this soak walk a different road. `set_champion`'s upsert
/// (the same public path roster sync uses) pins the id to the
/// lexicographic minimum so the veteran is always the file's lead and the
/// journey is byte-reproducible run over run.
fn soak_store() -> RosterStore {
    let mut store = RosterStore::new();
    store
        .set_champion(
            "00000000-0000-4000-8000-000000000000",
            &mkveteran(7, 50, &[16, 17, 15, 7]),
        )
        .expect("fixture champion stores");
    store
}

/// Drive one pilgrimage: march every day (always the easternmost exit — at
/// the Jaffa terminus the map bounces 24<->25, fighting a scripted gate
/// every day), settle everything raised, top up to the daily battle cap
/// with elective choice-encounter props, and measure every
/// CHECKPOINT_EVERY days. Stops at the day ceiling or when the company is
/// gone, whichever the road decides first. Returns the checkpoint rows.
fn run_journey(game: &mut ClientGame, ctx: &mut SoakCtx) -> Vec<Row> {
    let mut rows = vec![measure(game)];
    verify_reload_round_trip(game);
    let mut next_checkpoint = CHECKPOINT_EVERY;
    let mut inputs: u32 = 0;
    loop {
        let snap = game.snapshot();
        let t = snap.trail.clone().expect("the trail view rides the road");
        if t.over {
            break;
        }
        if t.day >= JOURNEY_DAY_CEILING {
            break;
        }
        // March: the easternmost exit; at the terminus this is the 24<->25
        // bounce (a scripted gate fires on every entry of both screens).
        let next = t
            .exits
            .iter()
            .copied()
            .max()
            .expect("the spine always offers an exit");
        let mut battles_today = 0usize;
        match game.handle(ClientInput::Travel { to_screen: next }) {
            Ok(()) => {}
            Err(AppError::Engine(BattleError::IllegalChampion(..))) => {
                // v1 zeal-carryover bug (see SoakCtx::refused_battles):
                // the day marched and saved; the road just declines to
                // fight a mantle carrying spent zeal. Tolerated, counted.
                ctx.refused_battles += 1;
            }
            Err(e) => panic!("travel failed unexpectedly: {e:?}"),
        }
        ctx.days_marched += 1;
        if game.phase() != Phase::Trail {
            settle(game, ctx, &mut battles_today);
            if game.phase() == Phase::BattleOver {
                break; // the company is gone; the terminal row still measures
            }
        }
        // Elective top-up: seek out the choice-encounter props until the
        // daily battle cap (worst case: a company that takes every fight).
        while game.phase() == Phase::Trail && battles_today < BATTLES_PER_DAY_MAX {
            let snap = game.snapshot();
            let Some(t) = snap.trail.clone() else { break };
            if t.over {
                break;
            }
            let Some(enc) = t.choice_encounters.first().copied() else {
                break; // no prop on this screen to engage
            };
            match game.handle(ClientInput::EngageChoice { encounter_ref: enc }) {
                Ok(()) => settle(game, ctx, &mut battles_today),
                Err(AppError::Engine(BattleError::IllegalChampion(..))) => {
                    ctx.refused_battles += 1;
                    break; // same zeal bug; no elective battle can build
                }
                Err(e) => panic!("engage failed unexpectedly: {e:?}"),
            }
            if game.phase() == Phase::BattleOver {
                break;
            }
        }
        if game.phase() == Phase::BattleOver {
            break;
        }
        let day = game.snapshot().trail.as_ref().expect("trail view").day;
        if day >= next_checkpoint {
            rows.push(measure(game));
            verify_reload_round_trip(game);
            next_checkpoint += CHECKPOINT_EVERY;
        }
        inputs += 1;
        assert!(inputs < 10_000, "the journey did not terminate");
    }
    rows.push(measure(game));
    rows
}

/// G16: the single worst-case pilgrimage. Run with
/// `cargo test -p crusade-app --test storage_soak -- --ignored --nocapture`.
#[test]
#[ignore = "G16 measurement soak: synthetic campaign run (see docs/STORAGE-SOAK.md)"]
fn worst_case_crossing_soak() {
    let mut game = ClientGame::new(soak_store());
    embark(&mut game);
    let mut ctx = SoakCtx::default();
    let rows = run_journey(&mut game, &mut ctx);

    println!(
        "\n=== G16 storage soak: the worst-case road ({}-day ceiling) ===",
        JOURNEY_DAY_CEILING
    );
    print_table(&rows);
    println!(
        "scenario tallies: {} days marched, {} battles ({} conceded falls), {} calls issued, {} battle entries refused (v1 zeal bug)",
        ctx.days_marched, ctx.battles, ctx.conceded_falls, ctx.calls_issued, ctx.refused_battles
    );
    let last = rows.last().expect("at least the embark row");
    let over = game.snapshot().trail.expect("trail view").over;
    println!(
        "journey end: day {}, book {}/{} pages, {} living{}",
        last.day,
        last.book_pages,
        last.named_men,
        last.living,
        if over { " — the pilgrimage is over" } else { "" }
    );

    // The lane under test stays coherent across the whole soak: the Book
    // never exceeds the named-men file, and the file itself never grows
    // past the fixed v1 landing roster (no recruitment mechanic yet).
    for r in &rows {
        assert!(r.book_pages <= r.named_men, "the Book never outgrows the file");
        assert_eq!(r.named_men, rows[0].named_men, "v1 has no recruitment: the file is fixed");
    }
    // Mid-journey reloads were verified at every checkpoint (inside
    // run_journey); the terminal state must still round-trip.
    if game.phase() == Phase::Trail {
        verify_reload_round_trip(&mut game);
    }
}

/// G16 long horizon: pilgrimage after pilgrimage on ONE document. The
/// per-journey memorial is bounded (see worst_case_crossing_soak), but the
/// automerge op history never shrinks — a devoted player's document grows
/// with every day marched, every battle settled, every hearer called, run
/// after run. This soak re-embarks (or abandons a survived journey) until
/// PILGRIMAGES runs have burned through the same fixed company, measuring
/// the document after each.
#[test]
#[ignore = "G16 measurement soak: long-horizon multi-pilgrimage run (see docs/STORAGE-SOAK.md)"]
fn many_pilgrimages_soak() {
    let mut game = ClientGame::new(soak_store());
    embark(&mut game);
    println!(
        "\n=== G16 long horizon: {} pilgrimages on one document ===",
        PILGRIMAGES
    );
    println!(
        "{:>5} {:>10} {:>9} {:>8} {:>16} {:>24}",
        "run", "book_pages", "roster", "battles", "doc_bytes", "hex_bytes(localStorage)"
    );
    for run in 1..=PILGRIMAGES {
        let mut ctx = SoakCtx::default();
        let rows = run_journey(&mut game, &mut ctx);
        let last = rows.last().expect("each journey measures its end");
        let over = game.snapshot().trail.expect("trail view").over;
        // Close the run out: acknowledge a grave, abandon a survivor.
        if game.phase() == Phase::BattleOver {
            game.handle(ClientInput::AcknowledgeResult)
                .expect("the grave is acknowledged");
        } else if !over {
            game.handle(ClientInput::AbandonTrail)
                .expect("a lived-in road can be abandoned");
        }
        println!(
            "{:>5} {:>10} {:>9} {:>8} {:>16} {:>24}",
            run, last.book_pages, last.store_champions, ctx.battles, last.doc_bytes, last.hex_bytes
        );
        embark(&mut game);
    }
    // The document survived the whole devotion intact and reloadable.
    verify_reload_round_trip(&mut game);
    let final_row = measure(&mut game);
    println!(
        "post-embark document after {PILGRIMAGES} pilgrimages: {} doc bytes \
         ({} hex bytes in localStorage), roster of {}",
        final_row.doc_bytes, final_row.hex_bytes, final_row.store_champions
    );
}

/// G16, isolated term: automerge write-amplification per trail save. The
/// journey state itself is bounded (the Book caps at the fixed company
/// size), but every road-mutating transition rewrites the `trail.save`
/// settings key — a last-writer-wins put that appends to the document's op
/// history even when the value is the same size. This soak separates that
/// per-write growth from state growth: a real captured save pair (two
/// different days of the same real journey, so the JSON genuinely changes)
/// is written alternatingly into a real embarked document, and the doc
/// bytes are sampled as the write count grows.
#[test]
#[ignore = "G16 measurement soak: long-horizon write-amplification run (see docs/STORAGE-SOAK.md)"]
fn trail_save_write_amplification_soak() {
    const WRITES: u64 = 2_000; // ~ 50 pilgrimages' worth of save writes
    const SAMPLE_EVERY: u64 = 250;

    // Two real consecutive saves off the real client API (a marched day
    // between them, so day/fallen/supplies genuinely differ).
    let mut game = ClientGame::new(soak_store());
    embark(&mut game);
    let mut ctx = SoakCtx::default();
    let mut battles = 0usize;
    settle(&mut game, &mut ctx, &mut battles); // anything the landing raised
    let save_a = game
        .store()
        .setting(TRAIL_SAVE_KEY)
        .expect("read")
        .expect("the lane is saved after embark");
    game.handle(ClientInput::Travel { to_screen: 6 })
        .expect("travel is legal");
    settle(&mut game, &mut ctx, &mut battles);
    let save_b = game
        .store()
        .setting(TRAIL_SAVE_KEY)
        .expect("read")
        .expect("the lane is saved after the march");
    assert_ne!(save_a, save_b, "the two captured days genuinely differ");
    let json_len = save_a.len();

    // Hammer the SAME real document (champion + pilgrimage seed + trail
    // lane all present) with alternating saves — exactly what the road
    // does per mutating transition, minus the state drift.
    let doc = game.store().save_bytes().expect("document saves");
    let mut store = RosterStore::load_bytes(&doc).expect("document reloads");
    println!(
        "\n=== G16 write amplification: trail.save rewrites (JSON ~{json_len} bytes, alternating two real day-saves) ==="
    );
    println!(
        "{:>12}  {:>16}  {:>22}",
        "writes", "doc_bytes", "hex_bytes(localStorage)"
    );
    let mut first: Option<(u64, usize)> = None;
    let mut last: Option<(u64, usize)> = None;
    let mut final_value = &save_a;
    for i in 1..=WRITES {
        let value = if i % 2 == 0 { &save_a } else { &save_b };
        store
            .set_setting(TRAIL_SAVE_KEY, value)
            .expect("settings write");
        final_value = value;
        if i % SAMPLE_EVERY == 0 {
            let bytes = store.save_bytes().expect("document saves");
            println!(
                "{:>12}  {:>16}  {:>16}",
                i,
                bytes.len(),
                hex::encode(&bytes).len()
            );
            if first.is_none() {
                first = Some((i, bytes.len()));
            }
            last = Some((i, bytes.len()));
        }
    }
    // Linear-fit sanity for the report: report the observed per-write
    // slope rather than asserting a rate (this is a measurement lane).
    if let (Some((w0, b0)), Some((w1, b1))) = (first, last) {
        let per_write = (b1 - b0) as f64 / (w1 - w0) as f64;
        println!(
            "observed doc growth: ~{per_write:.1} bytes/write across writes {w0}..{w1} \
             (state held constant; this is pure op-history growth)"
        );
    }
    // The document must still load (and stay under the 8 MiB crusade-data
    // load cap) after the full hammering, and the last write wins.
    let bytes = store.save_bytes().expect("document saves");
    let reloaded = RosterStore::load_bytes(&bytes).expect("document loads after the soak");
    assert_eq!(
        reloaded.setting(TRAIL_SAVE_KEY).expect("read").as_deref(),
        Some(final_value.as_str()),
        "the last write wins and reads back"
    );
}

/// G16, roster axis: the doc cost of baptisms. The Call-to-Repentance flow
/// is the one trail lane that grows the STORE roster without bound — every
/// successful Call inserts a new champion plus its baptismal setting, the
/// firstfruits bounty, and the converts mask. The real road in v1 never
/// reaches it (the single-file mantle does not survive its battles, and
/// willing hearers are a rare event), so this soak fabricates exactly the
/// four writes `ClientGame::baptize` performs — a legal convert instance
/// (species 113's own learnset, the `practice_champion` recipe) through
/// the same public store calls — and measures the document as the roster
/// grows. This is the "named-men roster growth" term of ROADMAP G16.
#[test]
#[ignore = "G16 measurement soak: roster growth via fabricated baptisms (see docs/STORAGE-SOAK.md)"]
fn baptism_roster_growth_soak() {
    use crusade_app::conversion as conv;

    const BAPTISMS: usize = 100;
    const SAMPLE_EVERY: usize = 10;

    // A real embarked document as the starting point.
    let mut game = ClientGame::new(soak_store());
    embark(&mut game);
    let doc = game.store().save_bytes().expect("document saves");
    let mut store = RosterStore::load_bytes(&doc).expect("document reloads");

    println!("\n=== G16 roster growth: fabricated baptisms (the 4 writes ClientGame::baptize performs) ===");
    println!(
        "{:>12}  {:>16}  {:>22}",
        "baptisms", "doc_bytes", "hex_bytes(localStorage)"
    );
    let mut first: Option<(usize, usize)> = None;
    let mut last: Option<(usize, usize)> = None;
    for i in 1..=BAPTISMS {
        // 1. the convert joins the roster as a new instance of his own
        //    species (kit from his own learnset — practice_champion's
        //    last-4 recipe, all four entries legal at level 45);
        let convert = mkchamp(113, 45, &[1, 51, 52, 61]);
        let id = store.add_champion(&convert).expect("convert joins");
        // 2. his baptismal name is seeded under roster.baptismal.<id>;
        store
            .set_setting(&format!("{}{id}", conv::BAPTISMAL_KEY_PREFIX), "baptized of the road")
            .expect("baptismal name writes");
        // 3. the firstfruits bounty is paid;
        store
            .add_item(conv::FIRSTFRUITS_SHARD_ITEM, conv::FIRSTFRUITS_BOUNTY)
            .expect("firstfruits paid");
        // 4. the converts mask records him.
        let mask = 1u32 << (113 - 108);
        store
            .set_setting(conv::CONVERTS_BAPTIZED_KEY, &mask.to_string())
            .expect("converts mask writes");
        if i % SAMPLE_EVERY == 0 {
            let bytes = store.save_bytes().expect("document saves");
            println!(
                "{:>12}  {:>16}  {:>16}",
                i,
                bytes.len(),
                hex::encode(&bytes).len()
            );
            if first.is_none() {
                first = Some((i, bytes.len()));
            }
            last = Some((i, bytes.len()));
        }
    }
    if let (Some((n0, b0)), Some((n1, b1))) = (first, last) {
        let per_baptism = (b1 - b0) as f64 / (n1 - n0) as f64;
        println!("observed doc growth: ~{per_baptism:.0} bytes/baptism (roster state + op history)");
    }
    // The grown roster still lists and the document still loads.
    assert_eq!(
        store.list_champions().expect("roster lists").len(),
        BAPTISMS + 1,
        "the veteran plus every convert"
    );
    let bytes = store.save_bytes().expect("document saves");
    let _ = RosterStore::load_bytes(&bytes).expect("document loads after the soak");
}
