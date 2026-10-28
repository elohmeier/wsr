//! Differential tests of `logic::ai_player::portfolio`: the AiComputerPlayerTurn steps BuyMoreStock,
//! LiquidateIdleHoldingCo, ConsolidateTopHoldings, SellStockRaiseCash, BuyEthereum and EndGamePayDebt
//! (each run through the dispatcher with the other steps skipped).
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
use wsr_core::state::Globals;

const BUY_MORE_STOCK: u32 = 0x5d020b;
const LIQUIDATE_IDLE: u32 = 0x5d0c41;
const CONSOLIDATE: u32 = 0x5d0eb9;
const SELL_RAISE_CASH: u32 = 0x5d12ad;
const BUY_ETHEREUM: u32 = 0x5d177e;
const END_GAME_PAY_DEBT: u32 = 0x5d18dd;

/// Runs `step` on `g` and compares; returns the run if it was comparable.
fn step(g: &Globals, rng: u32, addr: u32, what: &str) -> Option<TurnRun> {
    let t = run_turn(g, rng, Some(&[addr]), &[], &[])?;
    if check_turn(what, &t, Ext::from_f64(1e-9)) { Some(t) } else { None }
}

/// Makes the mover control its main company and some more; returns the main company.
fn give_control(g: &mut Globals, r: &mut Rand) -> i32 {
    let p = g.cur_player;
    let mut t = cast::<i32, _>(g.hold_stock_id[(1, p)]);
    if t < 11 {
        t = active_corp(g, r);
        g.hold_stock_id[(1, p)] = cast::<i16, _>(t);
        g.hold_stock_pct[(1, p)] = cast::<i16, _>(r.range(30, 90));
        g.hold_stock_basis[(1, p)] = Ext::from_f64(100.0);
        recount(g, p);
    }
    g.corp_controller[t] = p;
    for _ in 0..r.range(2, 12) {
        let c = active_in(g, r, 11, 1580);
        g.corp_controller[c] = p;
    }
    t
}

#[test]
fn diff_buy_more_stock() {
    let (sold, bought) = (AtomicU32::new(0), AtomicU32::new(0));
    par_seeds(1..=24, |seed| {
        let mut g = ai_world(seed + 100);
        let mut r = Rand(seed * 71 + 9);
        give_control(&mut g, &mut r);
        let p = g.cur_player;
        // the controlled companies hold minority stakes in companies with positive earnings
        for c in 11..=1600 {
            if g.corp_controller[c] != p || g.entity_status[c] != 1 {
                continue;
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                g.cash[c] = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(50000.0));
            }
            for _ in 0..r.range(0, 3) {
                let o = active_corp(&g, &mut r);
                let pct = cast::<i32, _>(r.range(5, 40));
            add_holding(&mut g, &mut r, c, o, pct);
                if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) {
                    g.proj_qtr_earnings[o] = f64r(&mut r, Ext::from_f64(0.1), Ext::from_f64(30.0));
                }
            }
        }
        g.cpu_max_corps = cast::<i32, _>(r.range(5, 80));
        g.player_corps_controlled[p] = cast::<i32, _>(r.range(0, 60));
        let rs = cast::<u32, _>(r.next());
        let Some(t) = step(&g, rs, BUY_MORE_STOCK, &format!("BuyMoreStock seed {seed}")) else { return };
        if t.changed_field("HoldStockPct") || t.changed_field("HoldStockId") {
            sold.fetch_add(1, Relaxed);
        }
        if t.changed_field("BankDebt") || t.changed_field("TBills") {
            bought.fetch_add(1, Relaxed);
        }
    });
    eprintln!("BuyMoreStock: holdings changed {} / purchases funded {}", sold.load(Relaxed), bought.load(Relaxed));
    if super::diff::unavailable().is_none() {
        assert!(sold.load(Relaxed) >= 2, "too few seeds traded");
    }
}

#[test]
fn diff_liquidate_idle_holding_co() {
    let hits = AtomicU32::new(0);
    par_seeds(1..=12, |seed| {
        let mut g = ai_world(seed + 200);
        let mut r = Rand(seed * 37 + 1);
        let p = g.cur_player;
        for _ in 0..r.range(1, 3) {
            let c = active_in(&g, &mut r, 131, 1580);
            g.industry_num[c] = 70;
            g.corp_controller[c] = p;
            if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
                g.bonds_issued[c] = 0;
                g.tax_loss_carry[c] = f64r(&mut r, -Ext::from_f64(12.0), Ext::from_f64(0.0));
                g.tax_credits[c] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(12.0));
                // an idle shell: no business, no holdings, some cash (else LiquidateCorporation refuses)
                g.business_assets[c] = Ext::from_f64(0.0);
                g.cash[c] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(500.0));
                for s in 0..=15 {
                    g.hold_stock_id[(s, c)] = 0;
                    g.hold_stock_pct[(s, c)] = 0;
                }
            }
        }
        let rs = cast::<u32, _>(r.next());
        let Some(t) = step(&g, rs, LIQUIDATE_IDLE, &format!("LiquidateIdleHoldingCo seed {seed}")) else { return };
        if t.changed_field("EntityStatus") {
            hits.fetch_add(1, Relaxed);
        }
    });
    if super::diff::unavailable().is_none() {
        assert!(hits.load(Relaxed) >= 2, "too few liquidations");
    }
}

#[test]
fn diff_consolidate_top_holdings() {
    let hits = AtomicU32::new(0);
    par_seeds(1..=20, |seed| {
        let mut g = ai_world(seed + 300);
        let mut r = Rand(seed * 53 + 7);
        let p = g.cur_player;
        if seed % 2 == 0 {
            let t = give_control(&mut g, &mut r);
            let o = active_corp(&g, &mut r);
            let pct = cast::<i32, _>(r.range(10, 50));
            add_holding(&mut g, &mut r, p, o, pct);
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                let pct = cast::<i32, _>(r.range(5, 30));
            add_holding(&mut g, &mut r, t, o, pct);
            }
            g.credit_rating_num[t] = cast::<i32, _>(r.range(0, 9));
        }
        if seed % 5 == 1 {
            g.asbestos_corp = cast::<i32, _>(g.hold_stock_id[(1, p)]);
        }
        let rs = cast::<u32, _>(r.next());
        let Some(t) = step(&g, rs, CONSOLIDATE, &format!("ConsolidateTopHoldings seed {seed}")) else { return };
        if t.changed_field("HoldStockPct") || t.changed_field("HoldStockId") {
            hits.fetch_add(1, Relaxed);
        }
    });
    if super::diff::unavailable().is_none() {
        assert!(hits.load(Relaxed) >= 3, "too few sales");
    }
}

#[test]
fn diff_sell_stock_raise_cash() {
    let hits = AtomicU32::new(0);
    par_seeds(1..=24, |seed| {
        let mut g = ai_world(seed + 400);
        let mut r = Rand(seed * 29 + 3);
        let p = g.cur_player;
        let t = give_control(&mut g, &mut r);
        g.hold_stock_pct[(1, p)] = cast::<i16, _>(r.range(20, 95));
        if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
            g.industry_num[t] = [70, 71, cast::<i16, _>(r.range(3, 68))][cast::<usize, _>(r.range(0, 2))];
        }
        g.ai_target_corp = t;
        // affordable buyers (CorpController = 1)
        for _ in 0..r.range(0, 10) {
            let c = active_in(&g, &mut r, 131, 1580);
            g.corp_controller[c] = 1;
            g.cash[c] = f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(200000.0));
        }
        let rs = cast::<u32, _>(r.next());
        let Some(t) = step(&g, rs, SELL_RAISE_CASH, &format!("SellStockRaiseCash seed {seed}")) else { return };
        if t.changed_field("SharesOutstanding") || t.changed_field("HoldStockPct") || t.changed_field("OptionRecs") {
            hits.fetch_add(1, Relaxed);
        }
    });
    eprintln!("SellStockRaiseCash: {} seeds acted", hits.load(Relaxed));
    if super::diff::unavailable().is_none() {
        assert!(hits.load(Relaxed) >= 2, "too few seeds acted");
    }
}

#[test]
fn diff_buy_ethereum() {
    let hits = AtomicU32::new(0);
    par_seeds(1..=16, |seed| {
        let mut g = ai_world(seed + 500);
        let mut r = Rand(seed * 17 + 5);
        if seed % 2 == 0 && g.num_players >= 3 {
            let p = cast::<i32, _>(r.range(3, cast::<i64, _>(g.num_players)));
            g.cur_player = p;
            g.is_computer_player[p] = 1;
            g.credit_rating_num[p] = cast::<i32, _>(r.range(6, 10));
            g.cash[p] = f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(100000.0));
        }
        g.crypto_ban_state = cast::<i32, _>(r.range(-1, 2));
        g.ethereum_price = cast::<i32, _>(r.range(500, 5000));
        let rs = cast::<u32, _>(r.next());
        let Some(t) = step(&g, rs, BUY_ETHEREUM, &format!("BuyEthereum seed {seed}")) else { return };
        if t.changed_field("PhysCommodRec") {
            hits.fetch_add(1, Relaxed);
        }
    });
    eprintln!("BuyEthereum: {} seeds traded", hits.load(Relaxed));
}

#[test]
fn diff_end_game_pay_debt() {
    let hits = AtomicU32::new(0);
    par_seeds(1..=10, |seed| {
        let mut g = ai_world(seed + 600);
        let mut r = Rand(seed * 11 + 2);
        if seed % 3 != 0 {
            g.cur_year = g.game_end_year - cast::<i32, _>(r.range(0, 1));
        }
        give_control(&mut g, &mut r);
        let rs = cast::<u32, _>(r.next());
        let Some(t) = step(&g, rs, END_GAME_PAY_DEBT, &format!("EndGamePayDebt seed {seed}")) else { return };
        if t.changed_field("BankDebt") {
            hits.fetch_add(1, Relaxed);
        }
    });
    if super::diff::unavailable().is_none() {
        assert!(hits.load(Relaxed) >= 3, "too few repayments");
    }
}
