//! Differential tests of `logic::sim_entity::ai_corp`: every routine on random plausible worlds against
//! the original (the trading/restructuring callees run on both sides, their calls are hooked for the
//! coverage counts).
#![allow(unused_imports)]
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use wsr_core::logic::sim_entity::ai_corp::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Counts seeds whose original log has a line containing `pat` (coverage of the tests).
fn hits(log: &[String], pat: &str, n: &AtomicUsize) {
    if log.iter().any(|l| l.contains(pat)) {
        n.fetch_add(1, Ordering::Relaxed);
    }
}

fn changed(log: &[String], n: &AtomicUsize) {
    if log.last().is_some_and(|l| l != "CHANGED 0") {
        n.fetch_add(1, Ordering::Relaxed);
    }
}

const IS_RECENT_OFFERING: u32 = 0x498a15;
const TAKE_PROFITS: u32 = 0x4c4739;
const RANDOM_LBO: u32 = 0x4f41e5;
const VOL_LIQ: u32 = 0x4f9223;
const CONSOLIDATE: u32 = 0x4f9975;
const REENTER: u32 = 0x4f9fd4;
const INFUSION: u32 = 0x4e51b1;
const CASH_RICH: u32 = 0x4fb4da;
const NEG_CASH: u32 = 0x4f4a0f;

/// Hooked callees (logged as `CALL Name(...)`).
const LBO_TENDER: u32 = 0x51f21c;
const BUY_STOCK: u32 = 0x4efe60;
const MERGE: u32 = 0x509293;
const LIQ_SUB: u32 = 0x512aa9;

/// Runs the original at `addr` and the port on `g` with RND state `rng0`; compares the state, RND state
/// and the number of message boxes. Returns the original's log plus a line `CHANGED n` with the number
/// of state differences to the input (None if the harness is unavailable).
fn check(
    addr: u32,
    args: &[&str],
    g: &Globals,
    rng0: u32,
    hooks: &[u32],
    port: impl FnOnce(&mut Globals, &mut Rec, &mut Rng),
) -> Option<Vec<String>> {
    let o = run_hooked(addr, args, g, rng0, &[], &[], hooks)?;
    let mut p = g.clone();
    let mut ui = Rec::default();
    let mut rng = Rng::from_state(rng0);
    port(&mut p, &mut ui, &mut rng);
    cmp().assert(&p, &o.globals);
    assert_eq!(rng.state(), o.rng, "RND state (0x{addr:x})");
    assert_eq!(ui.msgs.len(), orig_msgs(&o), "message boxes (0x{addr:x}): {:?} / {:?}", ui.msgs, o.log);
    let mut log = o.log;
    log.push(format!("CHANGED {}", Compare::default().diffs(g, &o.globals).len()));
    Some(log)
}

/// Prints a coverage count (and fails if nothing was reached while the harness ran).
fn report(what: &str, n: &AtomicUsize) {
    let __content = wsr_core::content::current();
    let k = n.load(Ordering::Relaxed);
    eprintln!("{__wsr_text_13574}{what}: {k}",__wsr_text_13574=__content.string(13574));
    if unavailable().is_none() {
        assert!(k > 0, "no seed reached: {what}");
    }
}

fn seed_rng(seed: u64) -> u32 {
    cast::<u32, _>(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 17)
}

#[test]
fn is_recent_offering_unit() {
    let mut g = Globals::default();
    g.recent_offerings[3] = -77;
    g.recent_offerings[9] = 77;
    g.recent_offerings[5] = -12;
    assert_eq!(is_recent_offering(&g, 77), 1);
    assert_eq!(is_recent_offering(&g, 12), -1);
    assert_eq!(is_recent_offering(&g, 13), 0);
}

#[test]
fn diff_is_recent_offering() {
    par_seeds(1..=4, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed);
        for i in 1..=10 {
            g.recent_offerings[i] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { cast::<i32, _>(r.range(-40, 40)) } else { 0 };
        }
        for id in [0, 5, 17, -3, 33] {
            let a = id.to_string();
            let Some(o) = run(IS_RECENT_OFFERING, &[&a], &g, 1, &[], &[]) else { return };
            assert_eq!(cast::<i32, _>(o.return_value), is_recent_offering(&g, id), "id {id}");
        }
    });
}

#[test]
fn diff_ai_take_profits_tax_shelter() {
    static N: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=24, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed ^ 0x55);
        let c = cast::<i32, _>(r.range(11, 1600));
        g.sim_co_count = c;
        g.entity_status[c] = 1;
        if g.industry_num[c] == 71 {
            g.industry_num[c] = 30;
        }
        g.corp_controller[c] = cast::<i32, _>(r.range(11, 1600));
        g.tax_loss_carry[c] = -(cast::<Ext, _>(r.range(0, 2000)));
        for slot in 1..=15 {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                let h = cast::<i16, _>(r.range(131, 1580));
                g.hold_stock_id[(slot, c)] = h;
                g.hold_stock_pct[(slot, c)] = cast::<i16, _>(r.range(1, 100));
                g.hold_stock_basis[(slot, c)] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(3000.0));
                g.entity_status[cast::<i32, _>(h)] = 1;
            }
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            let h = g.hold_stock_id[(1, c)];
            g.hold_stock_id[(1, 1)] = h;
            g.hold_stock_pct[(1, 1)] = 5;
        }
        if let Some(log) = check(TAKE_PROFITS, &[], &g, seed_rng(seed), &[], |p, ui, rng| ai_take_profits_tax_shelter(p, ui, rng)) {
            changed(&log, &N);
        }
    });
    report("take profits: sold", &N);
}

#[test]
fn diff_ai_random_lbo() {
    static N: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=40, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed ^ 0x77);
        // a cheap, cash-rich target with a computer-controlled partner holding a stake
        let given = seed % 2 == 0;
        let t = cast::<i32, _>(r.range(131, 1600));
        let target_arg = if given { t } else { 0 };
        let tg = t;
        g.entity_status[tg] = 1;
        g.industry_num[tg] = cast::<i16, _>(r.range(3, 69));
        g.market_cap[tg] = f64r(&mut r, Ext::from_f64(20.0), Ext::from_f64(200.0));
        g.earnings_ytd[tg] = f64r(&mut r, -Ext::from_f64(5.0), Ext::from_f64(100.0));
        g.cash[tg] = f64r(&mut r, Ext::from_f64(3000.0), Ext::from_f64(20000.0));
        g.bank_debt[tg] = Ext::from_f64(0.0);
        g.bonds_issued[tg] = 0;
        g.corp_controller[tg] = 0;
        let a = target_arg.to_string();
        let skips = [LBO_TENDER];
        let Some(log) = check(RANDOM_LBO, &[&a], &g, seed_rng(seed), &skips, |p, ui, rng| {
            ai_random_lbo(p, ui, rng, target_arg, &wsr_core::logic::newgame::LocalTime::HARNESS)
        }) else {
            return;
        };
        hits(&log, "CALL LboTenderOffer(", &N);
    });
    report("LBO: tender offers", &N);
}

#[test]
fn diff_ai_voluntary_liquidation() {
    let __content = wsr_core::content::current();
    static N: AtomicUsize = AtomicUsize::new(0);
    static M: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=24, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed ^ 0x99);
        let c = cast::<i32, _>(r.range(131, 1600));
        g.entity_status[c] = 1;
        g.industry_num[c] = 70;
        g.business_assets[c] = Ext::from_f64(0.0);
        g.working_capital[c] = Ext::from_f64(0.0);
        g.bonds_issued[c] = 0;
        g.cash[c] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(20.0));
        g.t_bills[c] = Ext::from_f64(0.0);
        g.bank_debt[c] = if seed % 2 == 0 { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(20.0)) } else { g.cash[c] + f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(5.0)) };
        for s in 0..=15 {
            g.hold_stock_id[(s, c)] = 0;
            g.hold_stock_pct[(s, c)] = 0;
        }
        // nobody holds it: 100% public
        for h in 1..=1600 {
            for s in 1..=15 {
                if cast::<i32, _>(g.hold_stock_id[(s, h)]) == c {
                    g.hold_stock_id[(s, h)] = 0;
                    g.hold_stock_pct[(s, h)] = 0;
                }
            }
        }
        g.lender_id = cast::<i32, _>(r.range(11, 85));
        let a = c.to_string();
        let mut status = 0;
        if let Some(log) = check(VOL_LIQ, &[&a], &g, seed_rng(seed), &[], |p, ui, rng| {
            ai_voluntary_liquidation(p, ui, rng, c);
            status = p.entity_status[c];
        }) {
            changed(&log, &N);
            if status == 2 {
                M.fetch_add(1, Ordering::Relaxed);
            }
        }
    });
    report("voluntary liquidation: acted", &N);
    report(__content.string(14147), &M);
}

#[test]
fn diff_ai_consolidate_subsidiary() {
    let __content = wsr_core::content::current();
    static N: AtomicUsize = AtomicUsize::new(0);
    static B: AtomicUsize = AtomicUsize::new(0);
    static L: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=40, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed ^ 0x31);
        let p = cast::<i32, _>(r.range(131, 1500));
        g.entity_status[p] = 1;
        g.industry_num[p] = cast::<i16, _>(r.range(3, 69));
        g.corp_controller[p] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { 0 } else { cast::<i32, _>(r.range(11, 1600)) };
        g.cash[p] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(30000.0));
        for slot in 1..=15 {
            let s = cast::<i32, _>(r.range(131, 1580));
            g.hold_stock_id[(slot, p)] = cast::<i16, _>(s);
            g.hold_stock_pct[(slot, p)] = [100, 30, 60, 10][cast::<usize, _>(r.range(0, 3))];
            g.entity_status[s] = 1;
            g.industry_num[s] = if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) { g.industry_num[p] } else { 5 };
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                g.corp_controller[s] = g.corp_controller[p];
            }
        }
        g.tmp_ext1 = cast::<Ext, _>(r.range(0, 70));
        let a = p.to_string();
        let skips = [LIQ_SUB, BUY_STOCK];
        if let Some(log) = check(CONSOLIDATE, &[&a], &g, seed_rng(seed), &skips, |q, ui, rng| ai_consolidate_subsidiary(q, ui, rng, p, 0)) {
            changed(&log, &N);
            hits(&log, "CALL BuyStock(", &B);
            hits(&log, "CALL LiquidateSubIntoParent(", &L);
        }
    });
    report("consolidate: acted", &N);
    report(__content.string(14148), &B);
    report("consolidate: LiquidateSubIntoParent", &L);
}

#[test]
fn diff_ai_corp_reenter_industry() {
    static N: AtomicUsize = AtomicUsize::new(0);
    static M: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=40, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed ^ 0x41);
        let c = cast::<i32, _>(if seed % 3 == 0 { r.range(1471, 1500) } else { r.range(131, 1600) });
        g.entity_status[c] = 1;
        g.industry_num[c] = 70;
        g.market_cap[c] = if seed % 2 == 0 { f64r(&mut r, Ext::from_f64(35.0), Ext::from_f64(500.0)) } else { f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(34.0)) };
        g.cash[c] = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(9000.0));
        g.tax_loss_carry[c] = -f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(100.0));
        // the routine only acts one time in 15: pick an RND state whose first draw passes
        let mut rng0 = seed_rng(seed);
        loop {
            let mut t = Rng::from_state(rng0);
            if t.rnd() * Ext::from_f64(15.0) <= Ext::from_f64(1.0) {
                break;
            }
            rng0 = rng0.wrapping_add(0x9e37_79b9);
        }
        let a = c.to_string();
        if let Some(log) = check(REENTER, &[&a], &g, rng0, &[MERGE], |p, ui, rng| ai_corp_reenter_industry(p, ui, rng, c, 0)) {
            changed(&log, &N);
            hits(&log, "CALL MergeCompanies(", &M);
        }
    });
    report("re-enter: acted", &N);
    report("re-enter: MergeCompanies", &M);
}

#[test]
fn diff_capital_infusion() {
    let __content = wsr_core::content::current();
    static N: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=40, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed ^ 0x51);
        let inv = cast::<i32, _>(r.range(11, 1600));
        let t = cast::<i32, _>(r.range(131, 1600));
        g.entity_status[t] = 1;
        g.industry_num[t] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { 70 } else { cast::<i16, _>(r.range(3, 69)) };
        g.tax_loss_carry[t] = if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) { -f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(2000.0)) } else { Ext::from_f64(0.0) };
        g.tax_loss_carry[inv] = if r.chance(cast::<f64, _>(Ext::from_f64(0.8))) { Ext::from_f64(0.0) } else { -Ext::from_f64(5.0) };
        g.corp_controller[t] = cast::<i32, _>(r.range(11, 1600));
        g.corp_controller[inv] = cast::<i32, _>(r.range(11, 1600));
        g.cash[inv] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(5000.0));
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.news_msg = st(__content.string(13571));
        }
        let (a, b) = (inv.to_string(), t.to_string());
        let mut injected = false;
        if check(INFUSION, &[&a, &b], &g, seed_rng(seed), &[], |p, ui, rng| {
            capital_infusion(p, ui, rng, inv, t);
            injected = p.cash[t] != g.cash[t];
        })
        .is_some()
            && injected
        {
            N.fetch_add(1, Ordering::Relaxed);
        }
    });
    report(__content.string(14149), &N);
}

#[test]
fn diff_ai_cash_rich_takeover() {
    static N: AtomicUsize = AtomicUsize::new(0);
    static A: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=48, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed ^ 0x61);
        g.market_valuation = cast::<f32, _>(Ext::from_f64(1.0));
        let t = cast::<i32, _>(r.range(86, 1470));
        g.takeover_candidate_id = if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) { t } else { 0 };
        g.entity_status[t] = 1;
        g.credit_rating_num[t] = cast::<i32, _>(r.range(2, 10));
        g.market_cap[t] = f64r(&mut r, Ext::from_f64(5.0), Ext::from_f64(100.0));
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            let c = cast::<i16, _>(r.range(86, 1600));
            g.cash_rich_corp = c;
            g.cash[cast::<i32, _>(c)] = f64r(&mut r, Ext::from_f64(5000.0), Ext::from_f64(50000.0));
        }
        // nobody else holds the target; a few cash-rich computer-controlled holders with small stakes
        for h in 1..=1600 {
            for s in 1..=15 {
                if cast::<i32, _>(g.hold_stock_id[(s, h)]) == t {
                    g.hold_stock_id[(s, h)] = 0;
                    g.hold_stock_pct[(s, h)] = 0;
                }
            }
        }
        for s in 0..=15 {
            g.hold_stock_id[(s, t)] = 0;
            g.hold_stock_pct[(s, t)] = 0;
        }
        for _ in 0..4 {
            let h = cast::<i32, _>(r.range(86, 1600));
            g.cash[h] = f64r(&mut r, Ext::from_f64(2000.0), Ext::from_f64(30000.0));
            g.corp_controller[h] = cast::<i32, _>(r.range(11, 1600));
            let slot = cast::<i32, _>(r.range(1, 15));
            g.hold_stock_id[(slot, h)] = cast::<i16, _>(t);
            g.hold_stock_pct[(slot, h)] = cast::<i16, _>(r.range(1, 25));
            g.hold_stock_id[(0, h)] = 15;
        }
        for _ in 0..40 {
            let h = cast::<i32, _>(r.range(86, 1600));
            g.cash[h] = f64r(&mut r, Ext::from_f64(2000.0), Ext::from_f64(30000.0));
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                // insurers (industry 2) only in their slots 86..130 (elsewhere the original's
                // CalcEntityFinancials reads SubprimeSecs out of range)
                g.industry_num[h] = [2, 70][cast::<usize, _>(r.range(0, 1))];
                if h > 130 {
                    g.industry_num[h] = 70;
                }
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                // a human's company on autopilot
                g.corp_controller[h] = 1;
                g.auto_pilot[h] = 1;
            }
        }
        for p in 1..=g.num_players {
            let c = cast::<i32, _>(r.range(131, 1580));
            g.hold_stock_id[(1, p)] = cast::<i16, _>(c);
            g.corp_controller[c] = p;
            g.cash[c] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(30000.0));
        }
        g.player_corps_controlled[1] = cast::<i32, _>(r.range(0, 60));
        g.cpu_max_corps = 40;
        let skips = [BUY_STOCK];
        if let Some(log) = check(CASH_RICH, &[], &g, seed_rng(seed), &skips, |p, ui, rng| ai_cash_rich_takeover(p, ui, rng, 0)) {
            hits(&log, "CALL BuyStock(", &N);
            hits(&log, "MSGBOX", &A);
        }
    });
    report("cash-rich takeover: BuyStock", &N);
    report("cash-rich takeover: AskAutopilotPermission prompt", &A);
}

const COMMODS: [&str; 6] = ["OIL", "GOLD", "SILVER", "WHEAT", "CORN", "STOCKS"];

#[test]
fn diff_process_negative_cash() {
    let __content = wsr_core::content::current();
    static N: AtomicUsize = AtomicUsize::new(0);
    static P: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=24, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed ^ 0x71);
        g.game_end_flag = st("N");
        g.cur_player = 1;
        g.is_computer_player[1] = 0;
        // entities in a cash squeeze, some with futures positions
        let mut owners = Vec::new();
        for _ in 0..12 {
            let e = cast::<i32, _>(r.range(86, 1600));
            g.entity_status[e] = 1;
            let hi = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { Ext::from_f64(3000.0) } else { Ext::from_f64(60000.0) };
            g.cash[e] = -f64r(&mut r, Ext::from_f64(1.0), hi);
            g.t_bills[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(500.0)) } else { Ext::from_f64(0.0) };
            g.working_capital[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(4000.0));
            g.corp_controller[e] = cast::<i32, _>(r.range(11, 1600));
            owners.push(e);
        }
        // the current player's corp (one position, so no selection dialog)
        let pc = owners[0];
        g.corp_controller[pc] = 1;
        g.cash[pc] = -f64r(&mut r, Ext::from_f64(20000.0), Ext::from_f64(50000.0));
        let two = seed % 3 == 0;
        g.working_capital[pc] = Ext::from_f64(0.0);
        g.credit_rating_num[pc] = cast::<i32, _>(r.range(0, 3));
        let mut n = 0;
        for (k, &o) in owners.iter().enumerate() {
            let m = if k == 0 { if two { 2 } else { 1 } } else { r.range(0, 3) };
            for _ in 0..m {
                n += 1;
                let c = COMMODS[cast::<usize, _>(r.range(0, 5))];
                let qty = r.range(1, 60) * if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) { -1 } else { 1 };
                let year = g.cur_year + cast::<i32, _>(r.range(0, 2));
                g.futures_recs[n] = st(&format!("{c}|{o}|{qty}|{}|{year}|{:.4}|{:.2}", r.range(1, 12), f64r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(2000.0))), f64r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(999.0)))));
            }
        }
        g.futures_recs[0] = st(&n.to_string());
        // (the player's closing trade, FuturesSellOrShort / FuturesBuyOrCover, follows a pick from the
        // futures list, which the harness cannot make; it is not reached here)
        let o = run(NEG_CASH, &[], &g, seed_rng(seed), &["IDOK", "IDNO"], &[]);
        let Some(o) = o else { return };
        let mut p = g.clone();
        let mut ui = Rec::default();
        ui.answers.push_back(1);
        ui.answers.push_back(7);
        let mut rng = Rng::from_state(seed_rng(seed));
        process_negative_cash(&mut p, &mut ui, &mut rng);
        // EXT vs f64: after an entity draws its whole credit line (DrawCreditLineForCash), the next
        // CalcEntityFinancials of it leaves `UnusedCreditLine` = 0 in the original but a residue of
        // ~1e-12 in the port, so the port goes on to recompute the lender's loan book
        // (`BankLargestDebtor`) where the original returns early.
        Compare::default().ignore("TickerPauseUntil").ignore("hReportDlg").ignore("BankLargestDebtor").abs_tol(Ext::from_f64(1e-9)).assert(&p, &o.globals);
        assert_eq!(rng.state(), o.rng, "RND state");
        assert_eq!(ui.msgs.len(), orig_msgs(&o), "message boxes: {:?} / {:?}", ui.msgs, o.log);
        if p.futures_recs[0] != g.futures_recs[0] {
            N.fetch_add(1, Ordering::Relaxed);
        }
        if !ui.msgs.is_empty() {
            P.fetch_add(1, Ordering::Relaxed);
        }
    });
    report(__content.string(14150), &N);
    report("negative cash: margin call box", &P);
}

