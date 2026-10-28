//! Differential tests of `logic::ai_player::actions`: the AiComputerPlayerTurn steps StartupCompany,
//! InsurerBuyBonds, IssueBonds, FileLawsuit, SpreadRumors and WriteOptions, each run through the turn
//! with all other steps skipped (both sides).
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use std::sync::atomic::{AtomicU32, Ordering};
use wsr_core::pb::numeric::Cur;
use wsr_core::state::Globals;

const STARTUP: u32 = 0x5c9f3f;
const INSURER_BONDS: u32 = 0x5caa87;
const ISSUE_BONDS: u32 = 0x5cb017;
const LAWSUIT: u32 = 0x5cb6ea;
const RUMORS: u32 = 0x5cc20c;
const WRITE_OPTIONS: u32 = 0x5cce66;

/// Makes `corp` the mover's main holding (stock slot 1) with `pct`, controlled by the mover.
fn set_main(g: &mut Globals, r: &mut Rand, corp: i32, pct: i32) {
    let p = g.cur_player;
    if let Some(s) = (1..=15).find(|&s| cast::<i32, _>(g.hold_stock_id[(s, p)]) == corp) {
        g.hold_stock_id[(s, p)] = 0;
        g.hold_stock_pct[(s, p)] = 0;
    }
    if g.hold_stock_id[(1, p)] != 0
        && let Some(free) = (2..=15).find(|&s| g.hold_stock_id[(s, p)] == 0)
    {
        g.hold_stock_id[(free, p)] = g.hold_stock_id[(1, p)];
        g.hold_stock_pct[(free, p)] = g.hold_stock_pct[(1, p)];
        g.hold_stock_basis[(free, p)] = g.hold_stock_basis[(1, p)];
    }
    g.hold_stock_id[(1, p)] = cast::<i16, _>(corp);
    g.hold_stock_pct[(1, p)] = cast::<i16, _>(pct);
    g.hold_stock_basis[(1, p)] = g.market_cap[corp] * cast::<Ext, _>(pct) * Ext::from_f64(0.01) * f64r(r, Ext::from_f64(0.5), Ext::from_f64(1.5));
    recount(g, p);
    g.corp_controller[corp] = p;
    g.entity_status[corp] = 1;
}

/// Runs the step `step` on `g` and compares; counts the seed in `hits` when the original changed `field`.
#[track_caller]
fn step_case(what: &str, g: &Globals, rng: u32, step: u32, field: &str, hits: &AtomicU32) {
    let Some(t) = run_turn(g, rng, Some(&[step]), &[], &[]) else { return };
    if check_turn(what, &t, Ext::from_f64(1e-9)) && t.changed_field(field) {
        hits.fetch_add(1, Ordering::Relaxed);
    }
}

/// `OptionRecs` written by AiCreateOptionPosition carry `STR$` of EXT intermediates (the strike), whose
/// 15th digit may differ between the 80-bit original and the f64 port: records whose fields are equal
/// as numbers (relative 1e-12) are taken as equal.
fn same_option_recs(t: &mut TurnRun) {
    for i in 0..=1000 {
        let (a, b) = (t.port.option_recs[i].to_display(), t.orig.globals.option_recs[i].to_display());
        if a == b {
            continue;
        }
        let (fa, fb): (Vec<&str>, Vec<&str>) = (a.split('|').collect(), b.split('|').collect());
        let same = fa.len() == fb.len()
            && fa.iter().zip(&fb).all(|(x, y)| {
                x == y
                    || match (x.trim().parse::<Ext>(), y.trim().parse::<Ext>()) {
                        (Ok(u), Ok(v)) => (u - v).abs() <= Ext::from_f64(1e-12) * u.abs().max(v.abs()),
                        _ => false,
                    }
            });
        if same {
            t.port.option_recs[i] = t.orig.globals.option_recs[i].clone();
        }
    }
}

#[track_caller]
fn assert_hits(name: &str, hits: &AtomicU32, min: u32) {
    if super::diff::unavailable().is_none() {
        let n = hits.load(Ordering::Relaxed);
        assert!(n >= min, "{name}: only {n} seeds hit the branch (want {min})");
    }
}

// ------------------------------------------------------------------------------------ StartupCompany

#[test]
fn diff_startup_company() {
    let hits = AtomicU32::new(0);
    par_seeds(1..=48, |seed| {
        let mut g = ai_world(seed + 5000);
        let mut r = Rand(seed * 7919 + 1);
        let p = g.cur_player;
        if g.cur_year < g.start_year + 1 {
            g.start_year = g.cur_year - 2;
        }
        g.cpu_max_corps = cast::<i32, _>(r.range(3, 25));
        g.player_corps_controlled[p] = cast::<i32, _>(r.range(0, 6));
        g.player_startup_count[p] = if r.chance(cast::<f64, _>(Ext::from_f64(0.85))) { 0 } else { 1 };
        let main = active_corp(&g, &mut r);
        let pct = cast::<i32, _>(r.range(30, 90));
        set_main(&mut g, &mut r, main, pct);
        // rich [runtime resource wording] the player (some banks / insurers)
        for k in 0..r.range(1, 8) {
            let c = match k % 4 {
                0 => active_in(&g, &mut r, 11, 85),
                1 => active_in(&g, &mut r, 86, 130),
                _ => active_corp(&g, &mut r),
            };
            g.corp_controller[c] = p;
            g.cash[c] = f64r(&mut r, Ext::from_f64(200.0), Ext::from_f64(8000.0)) * g.exchange_rate;
            g.credit_rating_num[c] = cast::<i32, _>(r.range(4, 10));
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                g.hold_stock_id[(15, c)] = cast::<i16, _>(r.range(131, 1580));
            }
        }
        // free bank / insurer slots
        for e in [61 + r.range(0, 24), 121 + r.range(0, 9), 85, 130] {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                g.entity_status[cast::<i32, _>(e)] = 0;
            }
        }
        for ind in 3..=69 {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.25))) {
                g.ind_growth_forecast[ind] = cast::<i16, _>(r.range(10, 60));
                g.ind_growth_rate[ind] = cast::<i16, _>(r.range(-10, 10));
                g.ind_demand_gap[ind] = cast::<f64, _>(f64r(&mut r, Ext::from_f64(10.0), Ext::from_f64(60.0)));
            }
        }
        let rs = (cast::<u32, _>(seed)).wrapping_mul(2654435761) ^ 0x5eed;
        step_case(&format!("StartupCompany seed {seed}"), &g, rs, STARTUP, "PlayerStartupCount", &hits);
    });
    assert_hits("StartupCompany", &hits, 3);
}

// ----------------------------------------------------------------------------------- InsurerBuyBonds

#[test]
fn diff_insurer_buy_bonds() {
    let hits = AtomicU32::new(0);
    let sub = AtomicU32::new(0);
    par_seeds(1..=40, |seed| {
        let mut g = ai_world(seed + 6000);
        let mut r = Rand(seed * 104729 + 3);
        let p = g.cur_player;
        let main = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { active_in(&g, &mut r, 86, 130) } else { active_corp(&g, &mut r) };
        let pct = cast::<i32, _>(r.range(30, 90));
        set_main(&mut g, &mut r, main, pct);
        for _ in 0..r.range(1, 4) {
            let c = active_in(&g, &mut r, 86, 130);
            g.corp_controller[c] = p;
            g.cash[c] = f64r(&mut r, Ext::from_f64(500.0), Ext::from_f64(20000.0)) * g.exchange_rate;
            g.ins_reserves[c] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(5000.0));
        }
        g.base_interest_rate = cast::<f32, _>([4.0f32, 8.0f32, 9.0f32, 11.0f32, 13.0f32][cast::<usize, _>(r.range(0, 4))]);
        g.subprime_crisis_state = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { 1 } else { 0 };
        g.bonds_public_held = r.range(0, 200_000);
        for _ in 0..r.range(5, 40) {
            let e = active_in(&g, &mut r, 86, 400);
            g.bonds_issued[e] = r.range(300, 3000);
            g.bond_price[e] = Cur::from_f64(f64r(&mut r, Ext::from_f64(60.0), Ext::from_f64(90.0)));
            g.credit_rating_num[e] = cast::<i32, _>(r.range(4, 8));
            g.proj_qtr_earnings[e] = f64r(&mut r, -Ext::from_f64(5.0), Ext::from_f64(30.0));
        }
        let rs = (cast::<u32, _>(seed)).wrapping_mul(2246822519) ^ 0xb0d5;
        let Some(t) = run_turn(&g, rs, Some(&[INSURER_BONDS]), &[], &[]) else { return };
        if check_turn(&format!("InsurerBuyBonds seed {seed}"), &t, Ext::from_f64(1e-9)) {
            if t.changed_field("HoldBondFace") {
                hits.fetch_add(1, Ordering::Relaxed);
            }
            if t.changed_field("SubprimeSecs") {
                sub.fetch_add(1, Ordering::Relaxed);
            }
        }
    });
    assert_hits("InsurerBuyBonds (bonds)", &hits, 2);
    assert_hits("InsurerBuyBonds (subprime)", &sub, 2);
}

// ---------------------------------------------------------------------------------------- IssueBonds

#[test]
fn diff_issue_bonds() {
    let issued = AtomicU32::new(0);
    let bought = AtomicU32::new(0);
    par_seeds(1..=48, |seed| {
        let mut g = ai_world(seed + 7000);
        let mut r = Rand(seed * 15485863 + 5);
        let p = g.cur_player;
        let main = active_corp(&g, &mut r);
        let pct = cast::<i32, _>(r.range(30, 90));
        set_main(&mut g, &mut r, main, pct);
        let high = seed % 3 != 0;
        g.base_interest_rate = cast::<f32, _>(if high { f32r(&mut r, Ext::from_f64(9.8), Ext::from_f64(14.0)) } else { f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(5.5)) });
        if seed % 7 == 0 {
            g.base_interest_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(5.6), Ext::from_f64(9.7)));
        }
        if high && seed % 4 != 1 {
            // only the companies set up below have bonds out
            for e in 11..=1600 {
                if g.corp_controller[e] == p {
                    g.bonds_issued[e] = 0;
                }
            }
        }
        for k in 0..r.range(1, 9) {
            let c = if k == 0 { main } else { active_corp(&g, &mut r) };
            g.corp_controller[c] = p;
            if high {
                g.bonds_issued[c] = r.range(10, 2000);
                g.bond_price[c] = Cur::from_f64(f64r(&mut r, Ext::from_f64(50.0), Ext::from_f64(90.0)));
                // low coupons, long maturities: priced well below par at high rates
                let mat = g.cur_year + cast::<i32, _>(r.range(5, 25));
                g.bond_issue_info[c] = st(&format!("{mat}@{:05.2}", cast::<Ext, _>(r.range(20, 120)) / Ext::from_f64(20.0)));
                g.cash[c] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(6000.0)) * g.exchange_rate;
                g.credit_rating_num[c] = cast::<i32, _>(r.range(3, 10));
            } else {
                g.bonds_issued[c] = if r.chance(cast::<f64, _>(Ext::from_f64(0.75))) { 0 } else { r.range(10, 500) };
                g.credit_rating_num[c] = cast::<i32, _>(r.range(7, 10));
                g.earnings_yr3_ago[c] = f64r(&mut r, -Ext::from_f64(10.0), Ext::from_f64(200.0));
                g.business_assets[c] = f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(20000.0));
                g.cash[c] = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(5000.0));
            }
        }
        let rs = (cast::<u32, _>(seed)).wrapping_mul(3266489917) ^ 0x1b;
        let Some(t) = run_turn(&g, rs, Some(&[ISSUE_BONDS]), &[], &[]) else { return };
        if check_turn(&format!("IssueBonds seed {seed}"), &t, Ext::from_f64(1e-9)) {
            if t.changed_field("BondIssueInfo") {
                issued.fetch_add(1, Ordering::Relaxed);
            }
            if t.changed_field("ExtraordinaryPL") && high {
                bought.fetch_add(1, Ordering::Relaxed);
            }
        }
    });
    assert_hits("IssueBonds (issue)", &issued, 2);
    assert_hits("IssueBonds (buyback)", &bought, 2);
}

// -------------------------------------------------------------------------------------- FileLawsuit

#[test]
fn diff_file_lawsuit() {
    let hits = AtomicU32::new(0);
    par_seeds(1..=40, |seed| {
        let mut g = ai_world(seed + 8000);
        let mut r = Rand(seed * 32452843 + 7);
        let p = g.cur_player;
        g.difficulty_level = if seed % 9 == 0 { 2 } else { cast::<i32, _>(r.range(3, 4)) };
        let main = active_corp(&g, &mut r);
        let pct = cast::<i32, _>(r.range(30, 90));
        set_main(&mut g, &mut r, main, pct);
        g.player_notoriety[p] = cast::<i16, _>(r.range(0, 6));
        g.cash[p] = f64r(&mut r, Ext::from_f64(3000.0), Ext::from_f64(80000.0));
        if seed % 11 == 5 {
            g.lawsuit_rec[10] = st("2| 200 | 300 | 4 |");
        }
        // poor rivals with one big holding each
        for q in 2..=g.num_players {
            if q == p {
                continue;
            }
            for s in 1..=15 {
                g.hold_stock_id[(s, q)] = 0;
                g.hold_stock_pct[(s, q)] = 0;
            }
            let c = active_corp(&g, &mut r);
            // (MarketCap x pct is the stake's value)
            g.market_cap[c] = f64r(&mut r, Ext::from_f64(3.0), Ext::from_f64(40.0));
            g.corp_controller[c] = if r.chance(cast::<f64, _>(Ext::from_f64(0.8))) { q } else { p };
            let pct = cast::<i32, _>(r.range(20, 70));
            add_holding(&mut g, &mut r, q, c, pct);
            g.cash[q] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(100.0));
            g.t_bills[q] = Ext::from_f64(0.0);
            g.bank_debt[q] = g.market_cap[c] * cast::<Ext, _>(pct) * f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(1.0));
            g.player_law_firm[q] = cast::<i32, _>(r.range(0, 4));
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                let pending = format!("2| {} | {c} | 4 |", r.range(11, 1600));
                g.lawsuit_rec[cast::<i32, _>(r.range(1, 9))] = st(&pending);
            }
        }
        // rich plaintiffs
        for _ in 0..r.range(1, 5) {
            let c = active_corp(&g, &mut r);
            g.corp_controller[c] = p;
            g.cash[c] = f64r(&mut r, Ext::from_f64(500.0), Ext::from_f64(10000.0));
            g.credit_rating_num[c] = cast::<i32, _>(r.range(4, 10));
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.ai_last_suit_plaintiff = active_corp(&g, &mut r);
        }
        let rs = (cast::<u32, _>(seed)).wrapping_mul(2654435761) ^ 0x7a;
        step_case(&format!("FileLawsuit seed {seed}"), &g, rs, LAWSUIT, "LawsuitRec", &hits);
    });
    assert_hits("FileLawsuit", &hits, 3);
}

// ------------------------------------------------------------------------------------- SpreadRumors

#[test]
fn diff_spread_rumors() {
    let hits = AtomicU32::new(0);
    par_seeds(1..=40, |seed| {
        let mut g = ai_world(seed + 9000);
        let mut r = Rand(seed * 49979687 + 9);
        let p = g.cur_player;
        g.difficulty_level = cast::<i32, _>(r.range(1, 4));
        g.player_notoriety[p] = cast::<i16, _>(r.range(0, 4));
        g.credit_rating_num[p] = cast::<i32, _>(r.range(4, 10));
        g.cash[p] = f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(50000.0));
        let rivals: Vec<i32> = (2..=g.num_players).filter(|&q| q != p).collect();
        g.libel_suit_defendant = match (seed % 3, rivals.is_empty()) {
            (0, false) => cast::<i16, _>(rivals[cast::<usize, _>(r.range(0, cast::<i64, _>(rivals.len()) - 1))]),
            (1, _) => cast::<i16, _>(p),
            _ => 0,
        };
        g.rumor_victim_id = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { active_corp(&g, &mut r) } else { 0 };
        for &q in &rivals {
            // small rivals invested in stock
            g.cash[q] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(50.0));
            g.t_bills[q] = Ext::from_f64(0.0);
            g.bank_debt[q] = Ext::from_f64(0.0);
            g.credit_rating_num[q] = cast::<i32, _>(r.range(1, 7));
            for s in 1..=15 {
                g.hold_stock_id[(s, q)] = 0;
                g.hold_stock_pct[(s, q)] = 0;
            }
            let c = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
                let hc = active_corp(&g, &mut r);
                g.industry_num[hc] = 70;
                let sub = active_corp(&g, &mut r);
                add_holding(&mut g, &mut r, hc, sub, 60);
                hc
            } else {
                active_corp(&g, &mut r)
            };
            g.market_cap[c] = f64r(&mut r, Ext::from_f64(20.0), Ext::from_f64(400.0));
            { let pct = cast::<i32, _>(r.range(10, 60)); add_holding(&mut g, &mut r, q, c, pct); }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                g.cash[c] = -g.t_bills[c] - Ext::from_f64(1.0);
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                g.earnings_ytd[c] = Ext::from_f64(250.0);
            }
        }
        let rs = (cast::<u32, _>(seed)).wrapping_mul(2246822519) ^ 0x33;
        step_case(&format!("SpreadRumors seed {seed}"), &g, rs, RUMORS, "LibelSuitDefendant", &hits);
    });
    assert_hits("SpreadRumors", &hits, 3);
}

// ------------------------------------------------------------------------------------- WriteOptions

#[test]
fn diff_write_options() {
    let hits = AtomicU32::new(0);
    par_seeds(1..=40, |seed| {
        let mut g = ai_world(seed + 9500);
        let mut r = Rand(seed * 86028121 + 11);
        let p = g.cur_player;
        if g.start_year == g.cur_year && g.cur_quarter == 1 {
            g.sim_co_count = 900;
        }
        let main = active_corp(&g, &mut r);
        let pct = cast::<i32, _>(r.range(30, 90));
        set_main(&mut g, &mut r, main, pct);
        for s in 1..=15 {
            g.hold_stock_id[(s, main)] = 0;
            g.hold_stock_pct[(s, main)] = 0;
        }
        recount(&mut g, main);
        for _ in 0..r.range(0, 4) {
            let c = active_corp(&g, &mut r);
            if c == main {
                continue;
            }
            g.corp_controller[c] = if r.chance(cast::<f64, _>(Ext::from_f64(0.8))) { p } else { 0 };
            g.market_cap[c] = f64r(&mut r, Ext::from_f64(50.0), Ext::from_f64(2000.0));
            g.shares_outstanding[c] = g.market_cap[c] / f64r(&mut r, Ext::from_f64(3.0), Ext::from_f64(120.0));
            { let pct = cast::<i32, _>(r.range(5, 90)); add_holding(&mut g, &mut r, main, c, pct); }
        }
        g.cash[main] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(20000.0));
        let rs = (cast::<u32, _>(seed)).wrapping_mul(3266489917) ^ 0x99;
        let Some(mut t) = run_turn(&g, rs, Some(&[WRITE_OPTIONS]), &[], &[]) else { return };
        same_option_recs(&mut t);
        if check_turn(&format!("WriteOptions seed {seed}"), &t, Ext::from_f64(1e-9)) && t.changed_field("OptionRecs") {
            hits.fetch_add(1, Ordering::Relaxed);
        }
    });
    assert_hits("WriteOptions", &hits, 2);
}
