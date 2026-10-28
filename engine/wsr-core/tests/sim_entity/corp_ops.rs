//! Differential tests of `logic::sim_entity::corp_ops`.
#![allow(unused_imports)]
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use wsr_core::logic::sim_entity::corp_ops::*;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

const SIM_CORP_QTR_OPS: u32 = 0x4fc8a9;
const BANK_ALLOC: u32 = 0x69c127;
const TRADE_GOV_BONDS: u32 = 0x6a16ad;
const GOV_BOND_LARGE_TRADE: u32 = 0x5bbcc5;
const DRAW_CREDIT_LINE: u32 = 0x506842;
const ASBESTOS: u32 = 0x506c63;
const BOOST_RND: u32 = 0x4e07fa;

/// Callees skipped in the original: none any more.
const SKIPS: &[(u32, u32)] = &[];
/// The world of `common::world` plus bank allocation targets, government bond holdings, player
/// advances, credit freezes and the other data the quarterly operations read.
fn world2(seed: u64) -> Globals {
    let mut g = world(seed);
    let mut r = Rand(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) | 1);
    g.borrow_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(3.0), Ext::from_f64(14.0)));
    g.entity_borrow_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(3.0), Ext::from_f64(14.0)));
    g.long_bond_yield = cast::<f32, _>(f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(12.0)));
    g.subprime_crisis_state = [0, 0, 1, -1, -2][cast::<usize, _>(r.range(0, 4))];
    g.credit_rating_str = st(["AAA", "B", "D", "CC"][cast::<usize, _>(r.range(0, 3))]);
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        g.base_interest_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(5.0), Ext::from_f64(14.0)));
    }
    for e in 1..=130 {
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            g.long_gov_bond_face[e] = r.range(0, 30000);
            g.lt_gov_bond_cost[e] = cast::<Ext, _>(g.long_gov_bond_face[e]) * f64r(&mut r, Ext::from_f64(0.8), Ext::from_f64(1.1));
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            g.short_gov_bond_face[e] = r.range(0, 30000);
            g.st_gov_bond_cost[e] = cast::<Ext, _>(g.short_gov_bond_face[e]) * f64r(&mut r, Ext::from_f64(0.9), Ext::from_f64(1.05));
        }
        for p in 1..=g.num_players {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) {
                g.player_advances[(p, e)] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(300.0));
            }
        }
    }
    for e in 11..=85 {
        g.bank_alloc_prime_mort_pct[e] = cast::<i16, _>(r.range(0, 45));
        g.bank_alloc_subprime_pct[e] = cast::<i16, _>(r.range(0, 20));
        g.bank_alloc_consumer_pct[e] = cast::<i16, _>(r.range(0, 35));
        g.bank_alloc_bonds_pct[e] = cast::<i16, _>(r.range(0, 30));
        g.bank_alloc_cash_pct[e] = cast::<i16, _>(r.range(0, 15));
        if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) {
            g.bank_alloc_bonds_pct[e] = 60;
        }
        g.bank_alloc_off[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) { 1 } else { 0 };
        g.credit_frozen[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { 1 } else { 0 };
        g.bank_largest_debtor[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) { cast::<i32, _>(r.range(1, 1600)) } else { 0 };
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            g.subprime_secs[e] = r.range(0, 20000);
        }
    }
    for e in 86..=130 {
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            g.subprime_secs[e] = r.range(0, 5000);
        }
        g.insur_uw_profit_ytd[e] = f64r(&mut r, -Ext::from_f64(100.0), Ext::from_f64(100.0));
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.cash[e] = f64r(&mut r, Ext::from_f64(50000.0), Ext::from_f64(300000.0));
        }
    }
    for e in 1..=1600 {
        if r.chance(cast::<f64, _>(Ext::from_f64(0.05))) {
            g.bank_debt[e] = f64r(&mut r, Ext::from_f64(5000.0), Ext::from_f64(40000.0));
        }
    }
    g
}

fn rng0(seed: u64) -> u32 {
    (cast::<u32, _>(seed)).wrapping_mul(2_654_435_761) ^ 0x5a5a_1234
}

#[track_caller]
fn check(port: &Globals, rng: &Rng, ui: &Rec, o: &Original, what: &str) {
    // EXT vs f64: BondUnrealizedLoss is a difference of large sums
    cmp().abs_tol(Ext::from_f64(1e-9)).assert(port, &o.globals);
    assert_eq!(rng.state(), o.rng, "RND state after {what}");
    assert_eq!(ui.msgs.len(), orig_msgs(o), "message boxes of {what}: {:?} / {:?}", ui.msgs, o.log);
}

fn entity_for(seed: u64, k: u64, g: &Globals, r: &mut Rand) -> i32 {
    let _ = g;
    match (seed + k) % 5 {
        0 => cast::<i32, _>(r.range(1, 4)),
        1 | 2 => cast::<i32, _>(r.range(11, 85)),
        3 => cast::<i32, _>(r.range(86, 130)),
        _ => cast::<i32, _>(r.range(131, 1600)),
    }
}

#[test]
fn diff_sim_corp_quarter_operations() {
    par_seeds(1..=40, |seed| {
        let mut g = world2(seed);
        let mut r = Rand(seed ^ 0x77);
        if seed % 3 == 0 {
            g.sim_co_count = 1450;
        }
        let e = entity_for(seed, 0, &g, &mut r);
        g.entity_status[e] = 1;
        if (86..=130).contains(&e) && seed % 2 == 1 {
            // cash-rich AI insurer at high rates: buys government bonds (or trades bonds)
            g.cash[e] = Ext::from_f64(400_000.0);
            g.base_interest_rate = cast::<f32, _>(Ext::from_f64(8.5));
            g.corp_controller[e] = 1500;
        }
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0(seed));
        let mut ui = Rec::default();
        sim_corp_quarter_operations(&mut port, &mut ui, &mut rng, e);
        let es = e.to_string();
        let Some(o) = run(SIM_CORP_QTR_OPS, &[&es], &g, rng0(seed), &[], SKIPS) else { return };
        check(&port, &rng, &ui, &o, &format!("SimCorpQuarterOperations({e}) seed {seed}"));
    });
}

#[test]
fn diff_bank_allocate_assets() {
    par_seeds(1..=30, |seed| {
        let mut g = world2(seed);
        let mut r = Rand(seed ^ 0x99);
        let b = cast::<i32, _>(r.range(11, 85));
        g.entity_status[b] = 1;
        g.bank_alloc_off[b] = 0;
        let mode = [3, 3, 3, 1, 2][cast::<usize, _>(seed % 5)];
        let force = cast::<i32, _>(seed % 2);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0(seed));
        let mut ui = Rec::default();
        let res = bank_allocate_assets(&mut port, &mut ui, &mut rng, b, mode, force);
        let args = [b.to_string(), mode.to_string(), force.to_string()];
        let a: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let Some(o) = run(BANK_ALLOC, &a, &g, rng0(seed), &[], SKIPS) else { return };
        check(&port, &rng, &ui, &o, &format!("BankAllocateAssets({b}, {mode}, {force}) seed {seed}"));
        assert_eq!(cast::<u32, _>(res), o.return_value, "result seed {seed}");
    });
}

#[test]
fn diff_trade_gov_bonds() {
    let __content = wsr_core::content::current();
    par_seeds(1..=12, |seed| {
        let g = world2(seed);
        let mut r = Rand(seed ^ 0x55);
        let e = cast::<i32, _>(if seed % 2 == 0 { r.range(11, 130) } else { r.range(1581, 1583) });
        let face = r.range(1, 20000);
        let bond_type = 1 + cast::<i32, _>(seed % 2);
        let action = 1 + cast::<i32, _>((seed / 2) % 2);
        let mut port = g.clone();
        trade_gov_bonds(&mut port, e, face, bond_type, action);
        let args = [e.to_string(), format!("{__wsr_text_13664}{face}",__wsr_text_13664=__content.string(13664)), bond_type.to_string(), action.to_string()];
        let a: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let Some(o) = run(TRADE_GOV_BONDS, &a, &g, 1, &[], &[]) else { return };
        cmp().assert(&port, &o.globals);
    });
}

#[test]
fn diff_check_govt_bond_large_trade() {
    let __content = wsr_core::content::current();
    par_seeds(1..=16, |seed| {
        let mut g = world2(seed);
        let mut r = Rand(seed ^ 0x33);
        let action = [7, 8, 1, 2, 3, 4, 5, 6][cast::<usize, _>(seed % 8)];
        let face = if seed % 3 == 0 { r.range(0, 400) } else { r.range(1000, 400_000_000) };
        if seed % 5 == 0 {
            g.long_gov_bond_face[11] = 150_000_000_000;
        }
        let mut port = g.clone();
        let mut ui = Rec::default();
        let x = check_govt_bond_large_trade(&mut port, &mut ui, action, face);
        let args = [action.to_string(), format!("{__wsr_text_13664}{face}",__wsr_text_13664=__content.string(13664))];
        let a: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let Some(o) = run(GOV_BOND_LARGE_TRADE, &a, &g, 1, &[], &[]) else { return };
        cmp().assert(&port, &o.globals);
        let y = o.return_float.expect("ST0");
        assert!(x == y || (x - y).abs() <= Ext::from_f64(1e-12) * x.abs(), "CheckGovtBondLargeTrade({action}, {face}): {x} vs {y}");
        assert_eq!(ui.msgs.len(), orig_msgs(&o), "messages");
    });
}

#[test]
fn diff_draw_credit_line_for_cash() {
    let __content = wsr_core::content::current();
    par_seeds(1..=16, |seed| {
        let mut g = world2(seed);
        let mut r = Rand(seed ^ 0x11);
        let e = entity_for(seed, 1, &g, &mut r);
        g.entity_status[e] = 1;
        g.cash[e] = -f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(3000.0));
        if seed % 2 == 0 {
            g.sim_co_count = e;
        }
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0(seed));
        let mut ui = Rec::default();
        let x = draw_credit_line_for_cash(&mut port, &mut ui, &mut rng, e);
        let es = e.to_string();
        let Some(o) = run(DRAW_CREDIT_LINE, &[&es], &g, rng0(seed), &[], &[]) else { return };
        check(&port, &rng, &ui, &o, &format!("DrawCreditLineForCash({e}) seed {seed}"));
        let y = o.return_float.expect("ST0");
        assert!(x == y || (x - y).abs() <= Ext::from_f64(1e-12) * x.abs(), "{__wsr_text_14155}{x} vs {y}",__wsr_text_14155=__content.string(14155));
    });
}

#[test]
fn diff_process_asbestos_liability() {
    par_seeds(1..=24, |seed| {
        let mut g = world2(seed);
        let mut r = Rand(seed ^ 0x44);
        let c = cast::<i32, _>(r.range(131, 1580));
        g.entity_status[c] = 1;
        g.credit_rating_num[c] = cast::<i32, _>(r.range(1, 9));
        g.sim_co_count = c;
        g.cur_quarter = (c + g.cur_year).rem_euclid(4);
        if g.cur_quarter == 0 {
            g.cur_year += 1;
            g.cur_quarter = (c + g.cur_year).rem_euclid(4);
        }
        g.asbestos_corp = if seed % 2 == 0 { c } else { -c };
        if seed % 7 == 0 {
            g.asbestos_corp = 0;
        }
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0(seed));
        let mut ui = Rec::default();
        process_asbestos_liability(&mut port, &mut ui, &mut rng);
        let Some(o) = run(ASBESTOS, &[], &g, rng0(seed), &[], &[]) else { return };
        if g.asbestos_corp == 0 {
            // EntityStatus(0): error 9, the ON ERROR handler shows ShowRuntimeError (not ported) and exits
            cmp().assert(&port, &o.globals);
            assert_eq!(rng.state(), o.rng);
            assert!(o.log.iter().any(|l| l.starts_with("PBERR 9")));
            return;
        }
        check(&port, &rng, &ui, &o, &format!("ProcessAsbestosLiability seed {seed}"));
    });
}

#[test]
fn diff_ai_boost_rnd_and_marketing() {
    let __content = wsr_core::content::current();
    par_seeds(1..=8, |seed| {
        let mut g = world2(seed);
        let mut r = Rand(seed ^ 0x66);
        g.sim_co_count = if seed % 2 == 0 { 117 } else { 1213 };
        g.difficulty_level = cast::<i32, _>(r.range(1, 4));
        g.news_msg = if seed % 3 == 0 { st(__content.string(13571)) } else { st("") };
        for ind in 3..=69 {
            g.ind_demand_gap[ind] = cast::<f64, _>(f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(40.0)));
        }
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0(seed));
        let mut ui = Rec::default();
        ai_boost_rnd_and_marketing(&mut port, &mut ui, &mut rng);
        let Some(o) = run(BOOST_RND, &[], &g, rng0(seed), &[], &[]) else { return };
        check(&port, &rng, &ui, &o, &format!("AiBoostRnDAndMarketing seed {seed}"));
    });
}
