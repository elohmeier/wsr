//! Differential tests of `logic::trading::bonds`: TradeCorpBonds (0x5b6dbe) and TradeTBills (0x69af92).
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use super::script::*;
use wsr_core::logic::trading::bonds::*;
use wsr_core::pb::numeric::Cur;
use wsr_core::state::Globals;

const TRADE_CORP_BONDS: u32 = 0x5b6dbe;
const TRADE_T_BILLS: u32 = 0x69af92;
/// Callees skipped in the original: none (BuyCorpBonds / BondBuyback borrow through BorrowToCoverPurchase).
const SKIPS: &[(u32, u32)] = &[];

/// Removes all holdings of `issuer`'s bonds.
fn clear_bond_holders(g: &mut Globals, issuer: i32) {
    for h in 1..=1600 {
        for s in 1..=15 {
            if g.hold_bond_id[(s, h)] == issuer {
                g.hold_bond_id[(s, h)] = 0;
                g.hold_bond_face[(s, h)] = 0;
                g.bond_hold_cost[(s, h)] = Ext::from_f64(0.0);
            }
        }
        g.hold_bond_id[(0, h)] = cast::<i32, _>((1..=15).filter(|&s| g.hold_bond_id[(s, h)] != 0).count());
    }
}

fn give_bonds(g: &mut Globals, holder: i32, issuer: i32, face: i64) {
    if let Some(s) = (1..=15).find(|&s| g.hold_bond_id[(s, holder)] == 0) {
        g.hold_bond_id[(s, holder)] = issuer;
        g.hold_bond_face[(s, holder)] = face;
        g.bond_hold_cost[(s, holder)] = cast::<Ext, _>(face) * Ext::from_f64(0.95);
        g.hold_bond_id[(0, holder)] = cast::<i32, _>((1..=15).filter(|&k| g.hold_bond_id[(k, holder)] != 0).count());
    }
}

/// A bond issuer 131..1580 (not `not`) with `issued` bonds, `held` of them placed with a holder.
fn issuer(g: &mut Globals, r: &mut Rand, not: &[i32], issued: i64, held: i64) -> i32 {
    let i = pick_active(r, g, 131, 1580, not);
    clear_bond_holders(g, i);
    g.bonds_issued[i] = issued;
    let mat = g.cur_year + cast::<i32, _>(r.range(1, 20));
    let coupon = f32r(r, Ext::from_f64(2.0), Ext::from_f64(14.0));
    g.bond_issue_info[i] = st(&format!("{mat}   {coupon:5.2}"));
    g.bond_price[i] = Cur::from_f64(f64r(r, Ext::from_f64(50.0), Ext::from_f64(120.0)));
    if held > 0 {
        let h = pick_active(r, g, 86, 130, not);
        give_bonds(g, h, i, held);
    }
    i
}

struct Case {
    g: Globals,
    mode: i32,
    side: i32,
    answers: Vec<String>,
}

fn case(seed: u64) -> Case {
    let __content = wsr_core::content::current();
    let mut g = world(seed + 10_100);
    let mut r = Rand(seed * 92_821 + 3);
    g.app_title = st(__content.string(13661));
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    let cp = g.cur_player;
    let kind = seed % 7;
    let mut mode = 1;
    let mut side = 1;
    let ae = match kind {
        // a non-financial company in mode 1
        5 if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) => pick_active(&mut r, &g, 131, 1580, &[]),
        // buy back own bonds (mode 2), the player's own buys
        4 | 6 => pick_active(&mut r, &g, 131, 1580, &[]),
        _ => {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                pick_active(&mut r, &g, 11, 85, &[])
            } else {
                pick_active(&mut r, &g, 86, 130, &[])
            }
        }
    };
    g.active_entity = ae;
    g.active_entity_name = g.ent_name[ae].clone();
    g.corp_controller[ae] = cp;
    g.cash[ae] = f64r(&mut r, Ext::from_f64(2000.0), Ext::from_f64(60_000.0));
    g.bank_debt[ae] = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { Ext::from_f64(0.0) } else { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(500.0)) };
    if (11..=85).contains(&ae) {
        // keep most banks within the regulators' loans-to-deposits limit
        g.bank_c_ds[ae] = Ext::from_f64(1_000_000.0);
        if r.chance(cast::<f64, _>(Ext::from_f64(0.25))) {
            // over-lent: small deposits, many loans
            g.bank_c_ds[ae] = Ext::from_f64(0.0);
            g.bank_demand_deposits[ae] = Ext::from_f64(1_000_000.0);
            g.bank_cust_deposits[ae] = Ext::from_f64(0.0);
            g.bank_mortgage_loans[ae] = 1_000_000;
            g.bank_consumer_loans[ae] = 0;
            g.subprime_secs[ae] = 0;
            g.bad_debt_reserve[ae] = Ext::from_f64(0.0);
            g.cash[ae] = Ext::from_f64(100.0);
            g.t_bills[ae] = Ext::from_f64(0.0);
            for s in 0..=15 {
                g.hold_stock_id[(s, ae)] = 0;
                g.hold_stock_pct[(s, ae)] = 0;
            }
        }
    }
    let mut answers: Vec<String> = Vec::new();
    let amounts = |r: &mut Rand| -> Vec<String> {
        (0..4).map(|_| ["5", "40", "120", "999999", "0", "7", "15"][cast::<usize, _>(r.range(0, 6))].to_string()).collect()
    };
    match kind {
        // buy another company's bonds
        0 | 1 | 5 => {
            let issued = r.range(50, 3000);
            let held = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { issued } else { r.range(0, issued - 1) };
            let i = issuer(&mut g, &mut r, &[ae], issued, held);
            if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
                g.cash[ae] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { f64r(&mut r, -Ext::from_f64(10.0), Ext::from_f64(30.0)) } else { -f64r(&mut r, Ext::from_f64(50_000.0), Ext::from_f64(900_000.0)) };
                g.t_bills[ae] = Ext::from_f64(0.0);
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.15))) {
                // all 15 bond slots taken
                for s in 1..=15 {
                    if g.hold_bond_id[(s, ae)] == 0 {
                        g.hold_bond_id[(s, ae)] = pick_active(&mut r, &g, 131, 1580, &[i]);
                        g.hold_bond_face[(s, ae)] = 1;
                    }
                }
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                g.cur_player = cp;
                g.corp_controller[i] = cp;
            }
            let sym = g.ent_symbol[i].to_display();
            answers.push(if seed.is_multiple_of(19) { "sym:".into() } else { format!("sym:{sym}") });
        }
        // sell bonds held
        2 | 3 => {
            side = 2;
            for s in 1..=15 {
                g.hold_bond_id[(s, ae)] = 0;
                g.hold_bond_face[(s, ae)] = 0;
            }
            g.hold_bond_id[(0, ae)] = 0;
            if !seed.is_multiple_of(5) {
                for _ in 0..r.range(1, 3) {
                    let issued = r.range(50, 3000);
                    let held = r.range(0, issued / 2);
                    let i = issuer(&mut g, &mut r, &[ae], issued, held);
                    give_bonds(&mut g, ae, i, r.range(1, issued / 2));
                }
            }
            answers.push(format!("pick:{}", r.range(0, 3)));
        }
        // the issuer buys back its own bonds
        _ => {
            mode = 2;
            side = cast::<i32, _>(r.range(0, 1));
            let issued = r.range(50, 3000);
            let held = if r.chance(cast::<f64, _>(Ext::from_f64(0.25))) { issued } else { r.range(0, issued - 1) };
            clear_bond_holders(&mut g, ae);
            g.bonds_issued[ae] = issued;
            g.bond_issue_info[ae] = st(&format!("{}   {:5.2}", g.cur_year + 9, Ext::from_f64(7.25)));
            g.bond_price[ae] = Cur::from_f64(f64r(&mut r, Ext::from_f64(50.0), Ext::from_f64(120.0)));
            if held > 0 {
                let h = pick_active(&mut r, &g, 86, 130, &[ae]);
                give_bonds(&mut g, h, ae, held);
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                // nearly bankrupt: rated D with bank debt
                g.bank_debt[ae] = f64r(&mut r, Ext::from_f64(50_000.0), Ext::from_f64(90_000.0));
                g.cash[ae] = Ext::from_f64(10.0);
            }
        }
    }
    answers.extend(amounts(&mut r));
    Case { g, mode, side, answers }
}

#[test]
fn diff_trade_corp_bonds() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(seeds(1..=210), |seed| {
        let c = case(seed);
        let args = [c.mode.to_string(), c.side.to_string()];
        let cmp = cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-7));
        let Some((ui, pg, _)) = check(seed, TRADE_CORP_BONDS, &args, &c.g, &c.answers, SKIPS, cmp, |g, ui, rng| {
            trade_corp_bonds(g, ui, rng, c.mode, c.side, 0)
        }) else {
            return;
        };
        hits.record(&ui);
        let ae = c.g.active_entity;
        if pg.cash[ae] != c.g.cash[ae] {
            hits.add("<traded>");
        }
    });
    hits.assert(&[
        "<traded>",
        __content.string(14257),
        __content.string(11802),
        __content.string(11808),
        __content.string(11811),
        __content.string(14258),
        __content.string(14259),
        __content.string(11822),
        __content.string(14260),
        // ("doesn't own any bonds of" / the symbol dialog after the listing: the listing only links held
        // bonds, and a report closed without a pick can't be scripted for both sides alike)
    ]);
}

#[test]
fn diff_trade_t_bills() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(seeds(1..=80), |seed| {
        let mut g = world(seed + 10_300);
        let mut r = Rand(seed * 15_485_863 + 1);
        g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
        let cp = g.cur_player;
        let ae = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { cp } else { pick_active(&mut r, &g, 131, 1580, &[]) };
        g.active_entity = ae;
        g.active_entity_name = g.ent_name[ae].clone();
        if ae > 10 {
            g.corp_controller[ae] = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { cp } else { 0 };
            g.auto_pilot[ae] = cast::<i32, _>(r.range(0, 1));
        }
        g.cash[ae] = [Ext::from_f64(0.0), Ext::from_f64(0.004), f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(900.0)), f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(900.0))][cast::<usize, _>(r.range(0, 3))];
        g.t_bills[ae] = [Ext::from_f64(0.0), Ext::from_f64(0.003), f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(900.0)), f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(900.0))][cast::<usize, _>(r.range(0, 3))];
        g.t_bill_help_shown_plr = [0, 0, cp, 1][cast::<usize, _>(r.range(0, 3))];
        // numbers only: message boxes may take them (6 = YES, 7 = NO, 2 = CANCEL)
        let answers: Vec<String> = (0..4)
            .map(|_| ["6", "7", "2", "1", "99999", "0", "3", "50"][cast::<usize, _>(r.range(0, 7))].to_string())
            .collect();
        let cmp = cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-7));
        let Some((ui, _, _)) =
            check(seed, TRADE_T_BILLS, &[], &g, &answers, &[], cmp, |g, ui, _rng| trade_t_bills(g, ui))
        else {
            return;
        };
        hits.record(&ui);
    });
    hits.assert(&[
        __content.string(11861),
        __content.string(11866),
        "Buy T-bills",
        "Sell T-bills",
        __content.string(14261),
        "has sold short-term",
        __content.string(14262),
        __content.string(14263),
    ]);
}
