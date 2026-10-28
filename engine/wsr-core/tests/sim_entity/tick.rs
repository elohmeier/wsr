//! Differential tests of SimProcessNextEntity (0x4c6bc1) over consecutive ticks: the port and the
//! original run the same tick on the same state (full Globals + RND state); the original's result is the
//! next tick's input. Callees that are still PORT-TODOs are skipped in the original (`CALLEE_TODO`).
//! The Ui is the scripted one of the trading tests with no answers (like the harness: MSGBOX defaults,
//! empty INPUTBOX texts, no pick-list clicks).
//!
//! `TICK_SEEDS=a,b,..` runs only those seeds; `TICK_DBG=1` prints the original's log on a mismatch.
#![allow(unused_imports)]
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use wsr_core::logic::sim_entity::sim_process_next_entity_skipping;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::pb::numeric::Cur;
use wsr_core::state::Globals;

const SIM: u32 = 0x4c6bc1;

/// File-producing callees tested separately, skipped on both sides of these consecutive-tick tests.
const CALLEE_TODO: &[(u32, u32)] = &[
    (0x40c01b, 1), // SaveGamePrompt (auto-save; tested below with its files)
];

/// Ported callees that are skipped as well (both sides) in these tests.
const ALSO_SKIPPED: &[(u32, u32)] = &[
    (0x458a7a, 0), // UpdateEconomy (economy agent; tested there)
    (0x4af1d2, 0), // UpdateInterestRates (economy agent; tested there)
];

fn skips() -> Vec<(u32, u32)> {
    CALLEE_TODO.iter().chain(ALSO_SKIPPED).copied().collect()
}

fn seeds(default: std::ops::RangeInclusive<u64>) -> Vec<u64> {
    let seeds: Vec<_> = match std::env::var("TICK_SEEDS") {
        Ok(s) => s.split(',').filter_map(|x| x.trim().parse().ok()).collect(),
        Err(_) => default.collect(),
    };
    assert!(!seeds.is_empty(), "TICK_SEEDS selected no valid cases");
    seeds
}

/// `common::world` with BondIssueInfo in the game's formats (`YYYY@cc.cc`, `YYYYcv|cap|@cc.cc`), a lender,
/// player CEO jobs, strikes and other per-tick state.
fn sim_world(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let mut g = world(seed);
    let mut r = Rand(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) | 1);
    for e in 11..=1600 {
        if g.bonds_issued[e] == 0 {
            continue;
        }
        let mat = g.cur_year + cast::<i32, _>(r.range(0, 14));
        let coupon = cast::<Ext, _>(r.range(40, 300)) / Ext::from_f64(20.0);
        let rate = format!("{coupon:05.2}");
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            let cap = g.market_cap[e] * f64r(&mut r, Ext::from_f64(0.05), Ext::from_f64(1.6));
            g.bond_issue_info[e] = st(&format!("{mat}{__wsr_text_13555}{cap:.2}|@{rate}",__wsr_text_13555=__content.string(13555)));
        } else {
            g.bond_issue_info[e] = st(&format!("{mat}@{rate}"));
        }
    }
    g.lender_id = cast::<i32, _>(r.range(11, 85));
    g.bonds_public_held = r.range(0, 3000);
    for p in 1..=g.num_players {
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            g.player_ceo_of[p] = cast::<i32, _>(r.range(131, 1580));
        }
        g.player_cap_gains[p] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(200.0));
        g.player_prepaid_tax[p] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(50.0));
    }
    g.ceo_bonus_multiple = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(2.0));
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        g.strike_corp_code = cast::<i16, _>(r.range(131, 1580)) + if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { 10000 } else { 0 };
    }
    for e in 11..=1600 {
        g.fund_fee_pct[e] = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.1), Ext::from_f64(1.2)));
        g.fund_adviser[e] = cast::<i32, _>(r.range(0, 1600));
    }
    g.ind_total_assets = f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(50000.0));
    g
}

#[test]
fn diff_tick_auto_save_persists_game_and_lastgame() {
    let mut g = sim_world(7);
    g.version_str = st("9.85");
    g.game_number = 7;
    g.sim_co_count = 1600;
    g.cur_auto_save = 1;
    g.auto_save_state = 0;
    g.game_in_progress = 1;
    g.game_end_flag = st("N");
    g.game_end_year = g.cur_year + 10;
    // Keep the persisted world small; the separate save/holdings suites cover dense tables.
    g.num_players = 2;
    g.cur_player = 1;
    g.entity_status.reset();
    for e in [1, 2, 11, 131] { g.entity_status[e] = 1; }
    g.hold_stock_id.reset();
    g.hold_stock_pct.reset();
    g.hold_stock_basis.reset();
    g.hold_bond_id.reset();
    g.hold_bond_face.reset();
    g.bond_hold_cost.reset();
    for p in 1..=g.num_players {
        g.cash[p] = Ext::from_f64(100_000.0);
        g.bank_debt[p] = Ext::from_f64(0.0);
    }
    let rng = 123456;
    let mut port = g.clone();
    let mut r = Rng::from_state(rng);
    let mut ui = Rec::default();
    sim_process_next_entity_skipping(&mut port, &mut ui, &mut r,
        &wsr_core::logic::newgame::LocalTime::HARNESS, &[]);
    let opts = Opts { rng: Some(rng), budget: Some(300_000_000), hooks: vec![0x40c01b], ..Opts::default() };
    let Some(o) = run_original_with(SIM, &opts, &g) else { return };
    cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-9)).assert(&port, &o.globals);
    assert_eq!(r.state(), o.rng);
    assert_eq!(ui.files.len(), 2);
    for name in ["GAME07.DAT", "LASTGAME.DAT"] {
        assert_eq!(ui.files.get(name), o.files.get(name).and_then(Option::as_ref), "{name}");
    }
}

/// Runs one tick in the port and the original on `g` and compares; returns the original's state and RND.
#[track_caller]
fn tick(g: &Globals, rng: u32, what: &str) -> Option<(Globals, u32)> {
    let __content = wsr_core::content::current();
    let mut port = g.clone();
    let mut r = Rng::from_state(rng);
    let mut ui = super::script::Script::default();
    let skip: Vec<u32> = skips().iter().map(|x| x.0).collect();
    sim_process_next_entity_skipping(&mut port, &mut ui, &mut r, &wsr_core::logic::newgame::LocalTime::HARNESS, &skip);
    let o = run(SIM, &[], g, rng, &[], &skips())?;
    let dbg = std::env::var_os("TICK_DBG").is_some();
    let errs: Vec<&String> = o.log.iter().filter(|l| l.starts_with("PBERR")).collect();
    assert!(errs.is_empty(), "{what}: original raised runtime errors {errs:?}{__wsr_text_14168}",__wsr_text_14168=__content.string(14168));
    let cmp = cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-9));
    if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cmp.assert(&port, &o.globals))) {
        if dbg {
            for l in &o.log {
                eprintln!("  {l}");
            }
        }
        panic!("{what}: {}", e.downcast_ref::<String>().cloned().unwrap_or_default());
    }
    if std::env::var_os("TICK_COV").is_some() {
        let d = cmp.diffs(g, &o.globals);
        let mut names: Vec<&str> = d.iter().map(|l| l.split(['(', ':']).next().unwrap_or("")).collect();
        names.dedup();
        eprintln!("{what}: {}{__wsr_text_14169}{:?}{__wsr_text_13611}{}", d.len(), &names[..names.len().min(25)], ui.news.len(),__wsr_text_14169=__content.string(14169), __wsr_text_13611=__content.string(13611));
    }
    assert_eq!(r.state(), o.rng, "{what}: RND state");
    assert_eq!(ui.msgs.len(), orig_msgs(&o), "{what}: message boxes {:?}", ui.msgs);
    Some((o.globals, o.rng))
}

/// Runs `n` consecutive ticks from `g`.
fn ticks(mut g: Globals, mut rng: u32, n: usize, what: &str) {
    let __content = wsr_core::content::current();
    assert!(n > 0, "consecutive-tick comparison needs at least one tick");
    let mut compared = 0;
    for k in 0..n {
        let label = format!("{what} tick {k} (SimCoCount {})", g.sim_co_count + 1);
        let Some((ng, nr)) = tick(&g, rng, &label) else { return };
        g = ng;
        rng = nr;
        compared += 1;
    }
    assert_eq!(compared, n, "{what}: incomplete comparison");
    eprintln!("{what}{__wsr_text_14170}{compared}/{n} ticks",__wsr_text_14170=__content.string(14170));
}

/// The quarter-end tick (SimCoCount = NumPlayers + 1) and the players' ticks around it.
#[test]
fn diff_tick_quarter_end_and_players() {
    par_seeds(seeds(1..=6), |seed| {
        let mut g = sim_world(seed);
        let mut r = Rand(seed * 77 + 5);
        g.sim_co_count = 0;
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            g.rumor_victim_id = cast::<i32, _>(r.range(11, 1600));
            g.libel_suit_defendant = cast::<i16, _>(r.range(1, cast::<i64, _>(g.num_players)));
            g.player_notoriety[cast::<i32, _>(g.libel_suit_defendant)] = cast::<i16, _>(r.range(0, 10));
        }
        let rng = (cast::<u32, _>(seed)).wrapping_mul(2654435761);
        let n = cast::<usize, _>(g.num_players) + 2;
        ticks(g, rng, n, &format!("seed {seed}"));
    });
}

/// Operating corps, banks, insurers and funds at various SimCoCount values (incl. the every-2nd
/// industry repricing, the 290th-tick economy step and the 100th/95th-tick routines).
#[test]
fn diff_tick_entities() {
    par_seeds(seeds(1..=12), |seed| {
        let mut g = sim_world(seed);
        let mut r = Rand(seed * 31 + 7);
        let starts = [14, 40, 94, 99, 115, 199, 289, 299, 450, 579, 774, 899, 1100, 1300, 1578, 1582, 1595];
        g.sim_co_count = starts[cast::<usize, _>(r.range(0, cast::<i64, _>(starts.len()) - 1))];
        let rng = (cast::<u32, _>(seed)).wrapping_mul(2246822519);
        ticks(g, rng, 4, &format!("seed {seed}"));
    });
}
