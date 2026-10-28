//! Differential tests of `logic::sim_entity::bonds` (bond buybacks, calls, conversions, bond and stock
//! issues) against the original routines.
//!
//! Player orders with a cash shortfall go through BorrowToCoverPurchase (0x5246b6, hooked for the
//! coverage count; answered YES or NO).
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use wsr_core::logic::sim_entity::bonds::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

const BOND_BUYBACK: u32 = 0x4e4128;
const CALL_BONDS: u32 = 0x4f7c76;
const CONVERT: u32 = 0x686c0f;
const COUNT_ISSUERS: u32 = 0x687f52;
const COUNT_CONV: u32 = 0x687e68;
const ISSUE_BONDS: u32 = 0x5231bc;
const ISSUE_STOCK: u32 = 0x5223a3;
const NORMALIZE: u32 = 0x5212b0;
const SCALE_EARNINGS: u32 = 0x54e63b;

const SKIPS: &[(u32, u32)] = &[];
/// BorrowToCoverPurchase (hooked).
const BORROW: u32 = 0x5246b6;

/// Counts the runs in which the original called BorrowToCoverPurchase.
fn count_borrow(o: &Original, n: &std::sync::atomic::AtomicUsize) {
    if o.log.iter().any(|l| l.starts_with("CALL BorrowToCoverPurchase(")) {
        n.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// A player order short of cash (mode 0 only): the issuer is the acting entity with little cash.
fn short_of_cash(g: &mut Globals, r: &mut Rand, id: i32) -> &'static [&'static str] {
    g.active_entity = id;
    g.active_entity_name = g.ent_name[id].clone();
    g.cash[id] = f64r(r, Ext::from_f64(0.0), Ext::from_f64(2.0));
    g.t_bills[id] = Ext::from_f64(0.0);
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { &["IDNO"] } else { &["IDYES"] }
}

/// The common world with bond issue info in the original's formats (`YYYY@cc.cc`, convertibles
/// `YYYYcv|cap|@cc.cc` with a conversion cap around the market cap) and a lender.
fn bond_world(seed: u64) -> Globals {
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
        if r.chance(cast::<f64, _>(Ext::from_f64(0.35))) {
            let cap = g.market_cap[e] * f64r(&mut r, Ext::from_f64(0.05), Ext::from_f64(1.6));
            g.bond_issue_info[e] = st(&format!("{mat}{__wsr_text_13555}{cap:.2}|@{rate}",__wsr_text_13555=__content.string(13555)));
        } else {
            g.bond_issue_info[e] = st(&format!("{mat}@{rate}"));
        }
    }
    g.lender_id = cast::<i32, _>(r.range(11, 85));
    g.tmp_symbol = g.ent_symbol[g.sim_co_count.clamp(11, 1600)].clone();
    g
}

fn issuers(g: &Globals, pred: impl Fn(i32) -> bool) -> Vec<i32> {
    (11..=1600).filter(|&e| g.entity_status[e] == 1 && g.bonds_issued[e] > 0 && pred(e)).collect()
}

static CHANGED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static RUNS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Coverage note (`--nocapture`): how many runs changed the state at all.
fn note(g: &Globals, o: &Original, what: &str) {
    use std::sync::atomic::Ordering::Relaxed;
    let n = RUNS.fetch_add(1, Relaxed) + 1;
    let d = cmp().diffs(g, &o.globals);
    let mut names: Vec<&str> = d.iter().map(|l| l.split(['(', ':']).next().unwrap_or("")).collect();
    names.dedup();
    let interesting = names.iter().any(|n| ["BondsIssued", "Cash", "HoldStockPct", "SharesOutstanding", "NewsHeadline"].contains(n));
    let c = if interesting { CHANGED.fetch_add(1, Relaxed) + 1 } else { CHANGED.load(Relaxed) };
    if std::env::var_os("BONDS_COVERAGE").is_some() {
        eprintln!("{what}: {:?} ({c}/{n} runs acted)", &names[..names.len().min(12)]);
    }
}

#[track_caller]
fn check(port: &Globals, rng: &Rng, ui: &Rec, o: &Original, what: &str) {
    assert_eq!(rng.state(), o.rng, "{what}: RND state");
    assert_eq!(ui.msgs.len(), orig_msgs(o), "{what}: message boxes {:?} vs {:?}", ui.msgs, o.log);
    cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-9)).assert(port, &o.globals);
}

#[test]
fn counts_match_original() {
    par_seeds(1..=3, |seed| {
        let g = bond_world(seed);
        let Some(o) = run(COUNT_ISSUERS, &[], &g, 1, &[], &[]) else { return };
        assert_eq!(count_bond_issuers(&g), cast::<i32, _>(o.return_value));
        let Some(o) = run(COUNT_CONV, &[], &g, 1, &[], &[]) else { return };
        assert_eq!(count_convertible_issuers(&g), cast::<i32, _>(o.return_value));
    });
}

#[test]
fn diff_bond_buyback() {
    let __content = wsr_core::content::current();
    static BORROWED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    par_seeds(1..=24, |seed| {
        let g0 = bond_world(seed);
        let mut r = Rand(seed);
        let ids = issuers(&g0, |_| true);
        for k in 0..3 {
            let id = ids[cast::<usize, _>(r.range(0, cast::<i64, _>(ids.len()) - 1))];
            let mut g = g0.clone();
            let mode = [0, 1, 2, 2][(cast::<usize, _>(seed) + k) % 4];
            g.tmp_ext2 = cast::<Ext, _>(mode);
            g.tmp_ext1 = if mode == 0 { (cast::<Ext, _>(g.bonds_issued[id]) * Ext::from_f64(0.3)).floor() } else { Ext::from_f64(0.0) };
            g.tmp_ext3 = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { -Ext::from_f64(1.0) } else { Ext::from_f64(0.0) };
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                g.cash[id] += Ext::from_f64(5000.0);
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                g.bond_price[id] = wsr_core::pb::numeric::Cur::from_f64(f64r(&mut r, Ext::from_f64(40.0), Ext::from_f64(80.0)));
            }
            let answers: &[&str] = if mode == 0 && k == 1 { short_of_cash(&mut g, &mut r, id) } else { &[] };
            let rng0 = cast::<u32, _>(seed * 7919 + cast::<u64, _>(k));
            let Some(o) = run_hooked(BOND_BUYBACK, &[&id.to_string()], &g, rng0, answers, SKIPS, &[BORROW]) else { return };
            count_borrow(&o, &BORROWED);
            let mut port = g.clone();
            let mut rng = Rng::from_state(rng0);
            let mut ui = Rec::default();
            ui.answers = answers.iter().map(|a| if *a == "IDNO" { 7 } else { 6 }).collect();
            bond_buyback(&mut port, &mut ui, &mut rng, id);
            note(&g, &o, &format!("seed {seed} id {id}{__wsr_text_13548}{mode}",__wsr_text_13548=__content.string(13548)));
            check(&port, &rng, &ui, &o, &format!("seed {seed} id {id}{__wsr_text_13548}{mode}",__wsr_text_13548=__content.string(13548)));
        }
    });
    let n = BORROWED.load(std::sync::atomic::Ordering::Relaxed);
    eprintln!("{__wsr_text_14152}{n}",__wsr_text_14152=__content.string(14152));
    if unavailable().is_none() {
        assert!(n > 0, "BorrowToCoverPurchase not reached");
    }
}

#[test]
fn diff_call_bonds() {
    let __content = wsr_core::content::current();
    static BORROWED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    par_seeds(1..=24, |seed| {
        let g0 = bond_world(seed);
        let mut r = Rand(seed + 100);
        let ids = issuers(&g0, |_| true);
        for k in 0..3 {
            let id = ids[cast::<usize, _>(r.range(0, cast::<i64, _>(ids.len()) - 1))];
            let mut g = g0.clone();
            let mode = [0, 1, 2][(cast::<usize, _>(seed) + k) % 3];
            g.tmp_ext1 = [Ext::from_f64(100.0), Ext::from_f64(50.0), Ext::from_f64(25.0), Ext::from_f64(100.0)][cast::<usize, _>(r.range(0, 3))];
            g.tmp_ext2 = cast::<Ext, _>(mode);
            if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
                g.cash[id] += Ext::from_f64(3000.0);
            }
            let answers: &[&str] = if mode == 0 && r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { short_of_cash(&mut g, &mut r, id) } else { &[] };
            let rng0 = cast::<u32, _>(seed * 104_729 + cast::<u64, _>(k));
            let Some(o) = run_hooked(CALL_BONDS, &[&id.to_string()], &g, rng0, answers, SKIPS, &[BORROW]) else { return };
            count_borrow(&o, &BORROWED);
            let mut port = g.clone();
            let mut rng = Rng::from_state(rng0);
            let mut ui = Rec::default();
            ui.answers = answers.iter().map(|a| if *a == "IDNO" { 7 } else { 6 }).collect();
            call_bonds(&mut port, &mut ui, &mut rng, id);
            note(&g, &o, &format!("seed {seed} id {id}{__wsr_text_13548}{mode}",__wsr_text_13548=__content.string(13548)));
            check(&port, &rng, &ui, &o, &format!("seed {seed} id {id}{__wsr_text_13548}{mode}",__wsr_text_13548=__content.string(13548)));
        }
    });
    let n = BORROWED.load(std::sync::atomic::Ordering::Relaxed);
    eprintln!("{__wsr_text_14152}{n}",__wsr_text_14152=__content.string(14152));
    if unavailable().is_none() {
        assert!(n > 0, "BorrowToCoverPurchase not reached");
    }
}

#[test]
fn diff_convert_or_redeem() {
    par_seeds(1..=24, |seed| {
        let g0 = bond_world(seed);
        let mut r = Rand(seed + 200);
        let cv = issuers(&g0, |e| g0.bond_issue_info[e].to_display().contains("cv"));
        let all = issuers(&g0, |_| true);
        for k in 0..3 {
            let ids = if k < 2 && !cv.is_empty() { &cv } else { &all };
            let id = ids[cast::<usize, _>(r.range(0, cast::<i64, _>(ids.len()) - 1))];
            let mut g = g0.clone();
            let convert_only = cast::<i32, _>(r.range(0, 1));
            let call_mode = [1, 2][cast::<usize, _>(r.range(0, 1))];
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                g.cash[id] += Ext::from_f64(3000.0);
            }
            let rng0 = cast::<u32, _>(seed * 15_485_863 + cast::<u64, _>(k));
            let args = [id.to_string(), convert_only.to_string(), call_mode.to_string()];
            let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            let Some(o) = run(CONVERT, &args, &g, rng0, &[], SKIPS) else { return };
            let mut port = g.clone();
            let mut rng = Rng::from_state(rng0);
            let mut ui = Rec::default();
            let res = convert_or_redeem_convertibles(&mut port, &mut ui, &mut rng, id, convert_only, call_mode);
            let what = format!("seed {seed} id {id} co {convert_only} cm {call_mode}");
            assert_eq!(res, cast::<i32, _>(o.return_value), "{what}: result");
            note(&g, &o, &what);
            check(&port, &rng, &ui, &o, &what);
        }
    });
}

#[test]
fn diff_issue_bonds() {
    let __content = wsr_core::content::current();
    par_seeds(1..=24, |seed| {
        let g0 = bond_world(seed);
        let mut r = Rand(seed + 300);
        let ids: Vec<i32> = (131..=1580).filter(|&e| g0.entity_status[e] == 1 && g0.bonds_issued[e] == 0).collect();
        for k in 0..3 {
            let id = ids[cast::<usize, _>(r.range(0, cast::<i64, _>(ids.len()) - 1))];
            let mut g = g0.clone();
            g.tmp_ext1 = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { Ext::from_f64(0.0) } else { cast::<Ext, _>(r.range(1, 40) * 50) };
            g.borrow_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(3.0), Ext::from_f64(12.0)));
            let (rate, maturity) = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                (cast::<Ext, _>(r.range(60, 240)) / Ext::from_f64(20.0), g.cur_year + cast::<i32, _>(r.range(3, 20)))
            } else {
                (Ext::from_f64(0.0), 0)
            };
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                g.ent_name[id] = st(["ACME", "Bolt Inc", "WIDGETCO'S", "Ab"][cast::<usize, _>(r.range(0, 3))]);
            }
            let rng0 = cast::<u32, _>(seed * 32_452_843 + cast::<u64, _>(k));
            let args = [id.to_string(), format!("{__wsr_text_13665}{rate}",__wsr_text_13665=__content.string(13665)), maturity.to_string()];
            let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            let Some(o) = run(ISSUE_BONDS, &args, &g, rng0, &[], SKIPS) else { return };
            let mut port = g.clone();
            let mut rng = Rng::from_state(rng0);
            let mut ui = Rec::default();
            let res = issue_bonds(&mut port, &mut ui, &mut rng, id, rate, maturity);
            let what = format!("seed {seed} id {id}{__wsr_text_14153}{rate}",__wsr_text_14153=__content.string(14153));
            assert_eq!(res, cast::<i32, _>(o.return_value), "{what}: result");
            note(&g, &o, &what);
            check(&port, &rng, &ui, &o, &what);
        }
    });
}

#[test]
fn diff_issue_stock() {
    let __content = wsr_core::content::current();
    par_seeds(1..=24, |seed| {
        let g0 = bond_world(seed);
        let mut r = Rand(seed + 400);
        let ids: Vec<i32> = (11..=1600).filter(|&e| g0.entity_status[e] == 1).collect();
        for k in 0..3 {
            let id = ids[cast::<usize, _>(r.range(0, cast::<i64, _>(ids.len()) - 1))];
            let mut g = g0.clone();
            g.tmp_ext1 = cast::<Ext, _>(r.range(3, 52));
            g.tmp_symbol = g.ent_symbol[id].clone();
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                g.action_code = st("AUTO");
            }
            let offer_type = cast::<i32, _>(r.range(1, 2));
            let rng0 = cast::<u32, _>(seed * 49_979_687 + cast::<u64, _>(k));
            let args = [id.to_string(), offer_type.to_string()];
            let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            let Some(o) = run(ISSUE_STOCK, &args, &g, rng0, &[], SKIPS) else { return };
            let mut port = g.clone();
            let mut rng = Rng::from_state(rng0);
            let mut ui = Rec::default();
            issue_stock(&mut port, &mut ui, &mut rng, id, offer_type);
            note(&g, &o, &format!("seed {seed} id {id}{__wsr_text_13737}{offer_type}",__wsr_text_13737=__content.string(13737)));
            check(&port, &rng, &ui, &o, &format!("seed {seed} id {id}{__wsr_text_13737}{offer_type}",__wsr_text_13737=__content.string(13737)));
        }
    });
}

#[test]
fn diff_normalize_and_scale_earnings() {
    par_seeds(1..=8, |seed| {
        let g0 = bond_world(seed);
        let mut r = Rand(seed + 500);
        let id = loop {
            let e = cast::<i32, _>(r.range(11, 1600));
            if g0.entity_status[e] == 1 {
                break e;
            }
        };
        let force = cast::<i32, _>(r.range(0, 1));
        let Some(o) = run(NORMALIZE, &[&id.to_string(), &force.to_string()], &g0, 5, &[], &[]) else { return };
        let mut port = g0.clone();
        let mut rng = Rng::from_state(5);
        let mut ui = Rec::default();
        normalize_holding_pcts(&mut port, &mut ui, &mut rng, id, force);
        note(&g0, &o, &format!("normalize seed {seed} id {id}"));
        check(&port, &rng, &ui, &o, &format!("normalize seed {seed} id {id}"));

        let pct = cast::<i32, _>(r.range(-120, 120));
        let Some(o) = run(SCALE_EARNINGS, &[&id.to_string(), &pct.to_string()], &g0, 5, &[], &[]) else { return };
        let mut port = g0.clone();
        scale_earnings_history(&mut port, id, pct);
        cmp().rel_tol(Ext::from_f64(1e-9)).assert(&port, &o.globals);
    });
}

#[test]
fn normalize_trims_to_100() {
    let mut g = Globals::default();
    g.entity_status[500] = 1;
    g.hold_stock_id[(1, 200)] = 500;
    g.hold_stock_pct[(1, 200)] = 70;
    g.hold_stock_id[(2, 300)] = 500;
    g.hold_stock_pct[(2, 300)] = 40;
    let mut ui = Rec::default();
    normalize_holding_pcts(&mut g, &mut ui, &mut Rng::default(), 500, 0);
    assert_eq!(g.hold_stock_pct[(1, 200)], 60);
    assert_eq!(g.hold_stock_pct[(2, 300)], 40);
}

#[test]
fn issue_info_formats() {
    let mut g = Globals::default();
    g.bonds_issued[300] = 100;
    g.bond_issue_info[300] = PbStr::from("2031cv|250.00|@07.50");
    adjust_conversion_price(&mut g, 300, Ext::from_f64(0.0), Ext::from_f64(10.0));
    assert_eq!(g.bond_issue_info[300].to_display(), "2031cv|275.00|@07.50");
}
