//! Differential tests of `logic::ai_player::invest`: the AiComputerPlayerTurn steps UseExcessCash,
//! PickAntitrustTarget + BuyTargetStock, ReviewSubHoldings, AdvanceFunds, EnterGrowthIndustry and
//! CallInAdvances (each run alone, the other steps skipped on both sides).
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
use wsr_core::state::Globals;

const USE_EXCESS_CASH: u32 = 0x5c67af;
const PICK_ANTITRUST: u32 = 0x5c7240;
const BUY_TARGET: u32 = 0x5c7855;
const REVIEW_SUBS: u32 = 0x5c8c0b;
const ADVANCE_FUNDS: u32 = 0x5c8e00;
const ENTER_GROWTH: u32 = 0x5c9506;
const CALL_IN: u32 = 0x5c9b05;
const ASSESS_POSITION: u32 = 0x5c3c65;

fn harness() -> bool {
    super::diff::unavailable().is_none()
}

/// Runs `steps` on `g` and compares; returns the run if comparable.
#[track_caller]
fn step(what: &str, g: &Globals, rng: u32, steps: &[u32]) -> Option<TurnRun> {
    let t = run_turn(g, rng, Some(steps), &[], &[])?;
    if check_turn(what, &t, Ext::from_f64(1e-9)) { Some(t) } else { None }
}

/// The mover's main holding, made controlled by the mover (first holding slot).
fn main_holding(g: &mut Globals, r: &mut Rand) -> i32 {
    let p = g.cur_player;
    let mut t = cast::<i32, _>(g.hold_stock_id[(1, p)]);
    if t == 0 {
        t = active_corp(g, r);
        add_holding(g, r, p, t, 40);
        let s = (1..=15).find(|&s| cast::<i32, _>(g.hold_stock_id[(s, p)]) == t).unwrap();
        if s != 1 {
            let (id, pct, b) = (g.hold_stock_id[(s, p)], g.hold_stock_pct[(s, p)], g.hold_stock_basis[(s, p)]);
            g.hold_stock_id[(s, p)] = g.hold_stock_id[(1, p)];
            g.hold_stock_pct[(s, p)] = g.hold_stock_pct[(1, p)];
            g.hold_stock_basis[(s, p)] = g.hold_stock_basis[(1, p)];
            g.hold_stock_id[(1, p)] = id;
            g.hold_stock_pct[(1, p)] = pct;
            g.hold_stock_basis[(1, p)] = b;
        }
    }
    g.corp_controller[t] = p;
    t
}

// ------------------------------------------------------------------------------------ UseExcessCash

#[test]
fn diff_use_excess_cash() {
    let __content = wsr_core::content::current();
    let (debt, div, inject) = (AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0));
    par_seeds(1..=32, |seed| {
        let mut g = ai_world(seed + 1000);
        let mut r = Rand(seed * 7919 + 1);
        let p = g.cur_player;
        g.cur_quarter = [1, 2, 3, 4, 4][cast::<usize, _>(r.range(0, 4))];
        g.accrued_pl[p] = f64r(&mut r, -Ext::from_f64(200.0), Ext::from_f64(300.0));
        g.div_recvd1[p] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(50.0));
        g.div_recvd2[p] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(50.0));
        g.extraordinary_pl[p] = r.range(-50, 50);
        g.player_cap_gains[p] = f64r(&mut r, -Ext::from_f64(100.0), Ext::from_f64(100.0));
        g.difficulty_level = cast::<i32, _>(r.range(1, 3));
        let t = if r.chance(cast::<f64, _>(Ext::from_f64(0.85))) { main_holding(&mut g, &mut r) } else { cast::<i32, _>(g.hold_stock_id[(1, p)]) };
        if t > 10 {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                g.hold_stock_pct[(1, p)] = 100;
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
                g.credit_rating_num[t] = cast::<i32, _>(r.range(9, 10));
                g.cash[t] = g.exchange_rate * f64r(&mut r, Ext::from_f64(15000.0), Ext::from_f64(40000.0));
                g.bank_debt[p] = (g.cash[p] + g.t_bills[p]) * f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(1.0));
            }
            g.dividend_per_share[t] = [Ext::from_f64(0.0), f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(0.5)), f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(5.0))][cast::<usize, _>(r.range(0, 2))];
            g.tax_loss_carry[t] = [Ext::from_f64(0.0), -Ext::from_f64(50.0), Ext::from_f64(30.0)][cast::<usize, _>(r.range(0, 2))];
        }
        let rs = cast::<u32, _>(r.next());
        let Some(tr) = step(&format!("UseExcessCash seed {seed}"), &g, rs, &[USE_EXCESS_CASH]) else { return };
        if tr.changed_field("BankDebt") {
            debt.fetch_add(1, Relaxed);
        }
        if tr.changed_field("DividendPerShare") {
            div.fetch_add(1, Relaxed);
        }
        if tr.changed_field("HoldStockBasis") {
            inject.fetch_add(1, Relaxed);
        }
    });
    if harness() {
        let (a, b, c) = (debt.into_inner(), div.into_inner(), inject.into_inner());
        assert!(a >= 4 && b >= 3 && c >= 1, "coverage: debt {a}{__wsr_text_13558}{b}{__wsr_text_13559}{c}",__wsr_text_13558=__content.string(13558), __wsr_text_13559=__content.string(13559));
    }
}

// --------------------------------------------------------------- PickAntitrustTarget + BuyTargetStock

/// Pending private suits in `LawsuitRec` (`"1 |defendant|plaintiff|share|damages|0|"`), a government
/// case, and controlled companies with cash.
fn antitrust_world(seed: u64) -> (Globals, u32) {
    let mut g = ai_world(seed + 2000);
    let mut r = Rand(seed * 104729 + 17);
    let p = g.cur_player;
    g.private_antitrust_off = if r.chance(cast::<f64, _>(Ext::from_f64(0.08))) { 1 } else { 0 };
    let t = main_holding(&mut g, &mut r);
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        // a shell or the defendant's industry as main holding (plaintiff branch)
        g.industry_num[t] = 70;
    }
    for k in 1..=10 {
        g.lawsuit_rec[k] = st("");
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            let d = active_corp(&g, &mut r);
            let pl = active_corp(&g, &mut r);
            let share = r.range(40, 80);
            g.lawsuit_rec[k] = st(&format!("1 | {d}| {pl}| {share}|{}|0|", r.range(10, 500)));
        } else if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.lawsuit_rec[k] = st(&format!("2 | {}| {}| 60|100|0|", r.range(131, 1580), r.range(131, 1580)));
        }
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        let free = cast::<i32, _>(r.range(1, 10));
        g.lawsuit_rec[free] = st("");
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
        g.antitrust_defendant = active_corp(&g, &mut r);
        g.antitrust_plaintiff = active_corp(&g, &mut r);
        g.antitrust_market_share = cast::<i32, _>(r.range(50, 75));
    }
    // companies of the player with liquid funds (buyers)
    for _ in 0..r.range(0, 4) {
        let c = active_corp(&g, &mut r);
        g.corp_controller[c] = p;
        g.cash[c] = f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(200000.0));
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
        g.cash[p] = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(5000.0));
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.25))) {
        for s in 0..=15 {
            g.hold_stock_id[(s, p)] = 0;
            g.hold_stock_pct[(s, p)] = 0;
        }
    }
    for i in 3..=69 {
        g.ind_demand[i] = r.range(1000, 300_000);
    }
    (g, cast::<u32, _>(r.next()))
}

#[test]
fn diff_pick_antitrust_target_and_buy_target_stock() {
    let __content = wsr_core::content::current();
    let (suits, cash) = (AtomicU32::new(0), AtomicU32::new(0));
    par_seeds(1..=40, |seed| {
        let (g, rs) = antitrust_world(seed);
        // the pick alone (frame only, AiFileAntitrustSuits' effects) and the pick with the purchases
        step(&format!("PickAntitrustTarget seed {seed}"), &g, rs, &[PICK_ANTITRUST]);
        let Some(tr) = step(&format!("Pick+BuyTargetStock seed {seed}"), &g, rs, &[PICK_ANTITRUST, BUY_TARGET]) else {
            return;
        };
        let alone = run_turn(&g, rs, Some(&[PICK_ANTITRUST]), &[], &[]).unwrap();
        let d = cmp().rel_tol(Ext::from_f64(0.0)).diffs(&alone.orig.globals, &tr.orig.globals);
        if d.iter().any(|l| l.starts_with("LawsuitRec")) {
            suits.fetch_add(1, Relaxed);
        }
        if d.iter().any(|l| l.starts_with("Cash") || l.starts_with("BankDebt")) {
            cash.fetch_add(1, Relaxed);
        }
    });
    if harness() {
        let (a, b) = (suits.into_inner(), cash.into_inner());
        assert!(a >= 2 && b >= 2, "coverage: suits filed {a}{__wsr_text_13560}{b}",__wsr_text_13560=__content.string(13560));
    }
}

// --------------------------------------------------------------------------------- ReviewSubHoldings

#[test]
fn diff_review_sub_holdings() {
    let __content = wsr_core::content::current();
    let hit = AtomicU32::new(0);
    par_seeds(1..=24, |seed| {
        let mut g = ai_world(seed + 3000);
        let mut r = Rand(seed * 31337 + 5);
        let p = g.cur_player;
        let t = main_holding(&mut g, &mut r);
        g.hold_stock_pct[(1, p)] = 60;
        // the main holding owns stakes in operating companies
        for _ in 0..r.range(2, 10) {
            let c = active_corp(&g, &mut r);
            let pct = cast::<i32, _>(r.range(5, 60));
            add_holding(&mut g, &mut r, t, c, pct);
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                g.corp_controller[c] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { p } else { t };
            }
        }
        let rs = cast::<u32, _>(r.next());
        let Some(tr) = step(&format!("ReviewSubHoldings seed {seed}"), &g, rs, &[REVIEW_SUBS]) else { return };
        if tr.changed().len() > 2 {
            hit.fetch_add(1, Relaxed);
        }
    });
    if harness() {
        let h = hit.into_inner();
        assert!(h >= 4, "{__wsr_text_13561}{h}",__wsr_text_13561=__content.string(13561));
    }
}

// ------------------------------------------------------------------------------------- AdvanceFunds

#[test]
fn diff_advance_funds() {
    let __content = wsr_core::content::current();
    let (adv, buy) = (AtomicU32::new(0), AtomicU32::new(0));
    par_seeds(1..=32, |seed| {
        let mut g = ai_world(seed + 4000);
        let mut r = Rand(seed * 6151 + 9);
        let p = g.cur_player;
        let t = main_holding(&mut g, &mut r);
        g.hold_stock_pct[(1, p)] = [10, 18, 30, 60, 100][cast::<usize, _>(r.range(0, 4))];
        if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
            g.cash[p] = g.market_cap[t] * f64r(&mut r, Ext::from_f64(5.0), Ext::from_f64(60.0)) + g.exchange_rate * Ext::from_f64(2000.0);
            g.bank_debt[p] = Ext::from_f64(0.0);
        }
        g.credit_rating_num[t] = cast::<i32, _>(r.range(3, 10));
        g.ai_advance_year = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { g.cur_year } else { 0 };
        g.market_valuation = cast::<f32, _>([Ext::from_f64(0.9), Ext::from_f64(1.0), Ext::from_f64(1.1), Ext::from_f64(1.3)][cast::<usize, _>(r.range(0, 3))]);
        g.holding_co_tax_rate = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { cast::<i32, _>(r.range(1, 20)) } else { 0 };
        let rs = cast::<u32, _>(r.next());
        let Some(tr) = step(&format!("AdvanceFunds seed {seed}"), &g, rs, &[ADVANCE_FUNDS]) else { return };
        if tr.changed_field("PlayerAdvances") {
            adv.fetch_add(1, Relaxed);
        }
        if tr.orig.log.iter().any(|l| l.starts_with("CALL BuyStock(")) {
            buy.fetch_add(1, Relaxed);
        }
    });
    if harness() {
        let (a, b) = (adv.into_inner(), buy.into_inner());
        assert!(a >= 3 && b >= 2, "{__wsr_text_13562}{a}, purchase attempts {b}",__wsr_text_13562=__content.string(13562));
    }
}

// ------------------------------------------------------------------------------ EnterGrowthIndustry

fn growth_world(seed: u64) -> (Globals, u32) {
    let mut g = ai_world(seed + 5000);
    let mut r = Rand(seed * 2851 + 3);
    let p = g.cur_player;
    main_holding(&mut g, &mut r);
    g.cpu_max_corps = cast::<i32, _>(r.range(3, 30));
    g.player_corps_controlled[p] = cast::<i32, _>(r.range(0, 10));
    for i in 3..=69 {
        g.ind_growth_rate[i] = cast::<i16, _>(r.range(-5, 20));
        g.ind_growth_forecast[i] = g.ind_growth_rate[i] + cast::<i16, _>(r.range(0, 25));
    }
    // player 1's rich companies are the buyers (AiFindAffordableBuyer)
    for _ in 0..r.range(0, 30) {
        let c = active_corp(&g, &mut r);
        g.corp_controller[c] = 1;
        g.cash[c] = f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(100000.0));
    }
    // cheap well-rated companies
    for _ in 0..r.range(0, 60) {
        let c = active_corp(&g, &mut r);
        g.market_cap[c] = f64r(&mut r, Ext::from_f64(0.05), Ext::from_f64(3.0));
        g.credit_rating_num[c] = cast::<i32, _>(r.range(6, 10));
    }
    for p in 1..=g.num_players {
        g.cash[p] += g.exchange_rate * f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(5000.0));
    }
    (g, cast::<u32, _>(r.next()))
}

#[test]
fn diff_enter_growth_industry() {
    par_seeds(1..=16, |seed| {
        let (g, rs) = growth_world(seed);
        step(&format!("EnterGrowthIndustry seed {seed}"), &g, rs, &[ENTER_GROWTH]);
    });
}

/// With AssessPosition before it (sets `maxLiquid`, which the company scan needs).
#[test]
fn diff_enter_growth_industry_after_assess_position() {
    let __content = wsr_core::content::current();
    let hit = AtomicU32::new(0);
    par_seeds(1..=24, |seed| {
        let (g, rs) = growth_world(seed + 100);
        let base = run_turn(&g, rs, Some(&[ASSESS_POSITION]), &[], &[]);
        let Some(tr) = step(&format!("AssessPosition+EnterGrowthIndustry seed {seed}"), &g, rs, &[ASSESS_POSITION, ENTER_GROWTH])
        else {
            return;
        };
        if let Some(b) = base
            && !cmp().rel_tol(Ext::from_f64(0.0)).diffs(&b.orig.globals, &tr.orig.globals).is_empty()
        {
            hit.fetch_add(1, Relaxed);
        }
    });
    if harness() {
        let h = hit.into_inner();
        assert!(h >= 2, "{__wsr_text_13561}{h}",__wsr_text_13561=__content.string(13561));
    }
}

// ----------------------------------------------------------------------------------- CallInAdvances

#[test]
fn diff_call_in_advances() {
    let (part, full) = (AtomicU32::new(0), AtomicU32::new(0));
    par_seeds(1..=32, |seed| {
        let mut g = ai_world(seed + 6000);
        let mut r = Rand(seed * 977 + 13);
        let p = g.cur_player;
        g.ai_adv_call_quarter = 0;
        for _ in 0..r.range(0, 4) {
            let c = active_in(&g, &mut r, 11, 1580);
            g.player_advances[(p, c)] = [f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(15.0)), f64r(&mut r, Ext::from_f64(20.0), Ext::from_f64(2000.0))][cast::<usize, _>(r.range(0, 1))];
            if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
                g.bonds_issued[c] = 0;
                g.bank_debt[c] = Ext::from_f64(0.0);
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                g.corp_controller[c] = p;
            }
        }
        let rs = cast::<u32, _>(r.next());
        let Some(tr) = step(&format!("CallInAdvances seed {seed}"), &g, rs, &[CALL_IN]) else { return };
        if tr.changed_field("AiAdvCallQuarter") || tr.changed_field("PlayerAdvances") {
            if tr.changed_field("NewsMsg") {
                part.fetch_add(1, Relaxed);
            } else {
                full.fetch_add(1, Relaxed);
            }
        }
    });
    if harness() {
        let (a, b) = (part.into_inner(), full.into_inner());
        assert!(a >= 2 && b >= 2, "coverage: partial calls {a}, full calls {b}");
    }
}
