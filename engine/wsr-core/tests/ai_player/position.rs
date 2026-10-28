//! Differential tests of `logic::ai_player::position`: the turn steps AiPlayer_SqueezeOutMinority
//! (0x5c3730), AiPlayer_AssessPosition (0x5c3c65), AiPlayer_FreezeRivalCredit (0x5c3fc7),
//! AiPlayer_AcquireHoldingCo (0x5c4828), AiPlayer_BuyCorpBonds (0x5c5a9d), AiPlayer_SellCorpBonds
//! (0x5c5e5c) and AiPlayer_RepayFrozenLoan (0x5c603a), each run alone in AiComputerPlayerTurn.
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
use wsr_core::state::Globals;

const SQUEEZE: u32 = 0x5c3730;
const ASSESS: u32 = 0x5c3c65;
const FREEZE: u32 = 0x5c3fc7;
const ACQUIRE: u32 = 0x5c4828;
const BUY_BONDS: u32 = 0x5c5a9d;
const SELL_BONDS: u32 = 0x5c5e5c;
const REPAY: u32 = 0x5c603a;

/// Removes every holding of `corp` (all holders).
fn clear_holders(g: &mut Globals, corp: i32) {
    for e in 1..=1600 {
        for s in 1..=15 {
            if cast::<i32, _>(g.hold_stock_id[(s, e)]) == corp {
                g.hold_stock_id[(s, e)] = 0;
                g.hold_stock_pct[(s, e)] = 0;
                g.hold_stock_basis[(s, e)] = Ext::from_f64(0.0);
            }
        }
        recount(g, e);
    }
}

/// Removes all of `e`'s holdings.
fn clear_holdings(g: &mut Globals, e: i32) {
    for s in 0..=15 {
        g.hold_stock_id[(s, e)] = 0;
        g.hold_stock_pct[(s, e)] = 0;
        g.hold_stock_basis[(s, e)] = Ext::from_f64(0.0);
    }
}

/// Gives the controlled bank/insurer `b` up to `n` bond holdings of issuers with bonds outstanding.
fn add_bonds(g: &mut Globals, r: &mut Rand, b: i32, n: i64) {
    let mut k = cast::<i32, _>((1..=15).filter(|&s| g.hold_bond_id[(s, b)] != 0).count());
    for _ in 0..n {
        if k >= 15 {
            break;
        }
        let c = active_in(g, r, 131, 1600);
        if g.bonds_issued[c] == 0 || (1..=15).any(|s| g.hold_bond_id[(s, b)] == c) {
            continue;
        }
        let Some(s) = (1..=15).find(|&s| g.hold_bond_id[(s, b)] == 0) else { break };
        g.hold_bond_id[(s, b)] = c;
        g.hold_bond_face[(s, b)] = r.range(10, 2000);
        k += 1;
    }
    g.hold_bond_id[(0, b)] = k;
}

/// True if any bank/insurer's bond holdings differ.
fn bonds_changed(a: &Globals, b: &Globals) -> bool {
    (11..=130).any(|e| (0..=15).any(|s| a.hold_bond_id[(s, e)] != b.hold_bond_id[(s, e)]))
}

#[track_caller]
fn coverage(what: &str, v: &[u32]) {
    let __content = wsr_core::content::current();
    if super::diff::unavailable().is_none() {
        assert!(v.iter().all(|&x| x >= 1), "{what}{__wsr_text_13577}{v:?}",__wsr_text_13577=__content.string(13577));
    }
}

#[test]
fn diff_step_squeeze_out_minority() {
    let sold = AtomicU32::new(0);
    let squeezed = AtomicU32::new(0);
    par_seeds(1..=24, |seed| {
        let mut g = ai_world(seed + 2100);
        let mut r = Rand(seed * 67 + 3);
        let p = g.cur_player;
        let t = match cast::<i32, _>(g.hold_stock_id[(1, p)]) {
            t if t > 10 => t,
            _ => {
                let t = active_corp(&g, &mut r);
                clear_holdings(&mut g, p);
                t
            }
        };
        if seed % 6 != 0 {
            // the target fully held: the player and companies (some controlled by the player)
            clear_holders(&mut g, t);
            let own = cast::<i32, _>(r.range(51, 90));
            add_holding(&mut g, &mut r, p, t, own);
            if cast::<i32, _>(g.hold_stock_id[(1, p)]) != t {
                clear_holdings(&mut g, p);
                add_holding(&mut g, &mut r, p, t, own);
            }
            let mut rest = 100 - own;
            while rest > 0 {
                let c = active_in(&g, &mut r, 11, 1600);
                if c == t {
                    continue;
                }
                let pct = rest.min(cast::<i32, _>(r.range(1, 30)));
                if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
                    clear_holders(&mut g, c);
                    add_holding(&mut g, &mut r, p, c, 60);
                    g.corp_controller[c] = p;
                }
                if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                    g.market_cap[c] = g.market_cap[t] * f64r(&mut r, Ext::from_f64(0.2), Ext::from_f64(2.0));
                }
                if add_holding(&mut g, &mut r, c, t, pct) {
                    rest -= pct;
                }
            }
            g.corp_controller[t] = p;
            g.cash[p] = g.market_cap[t] * f64r(&mut r, Ext::from_f64(5.0), Ext::from_f64(40.0));
            g.auto_buyout_year = 0;
            // the target's own stakes (the random merge)
            for _ in 0..r.range(0, 3) {
                let c = active_corp(&g, &mut r);
                if c != t {
                    let pct = cast::<i32, _>(r.range(5, 100));
                    add_holding(&mut g, &mut r, t, c, pct);
                }
            }
        }
        let rs = cast::<u32, _>(r.next());
        let Some(tr) = run_turn(&g, rs, Some(&[SQUEEZE]), &[], &[]) else { return };
        if !check_turn(&format!("SqueezeOutMinority seed {seed}"), &tr, Ext::from_f64(1e-12)) {
            return;
        }
        if (11..=1600).any(|e| tr.port.hold_stock_id[(0, e)] < g.hold_stock_id[(0, e)]) {
            sold.fetch_add(1, Relaxed);
        }
        if tr.port.auto_buyout_year != g.auto_buyout_year || (1..=1600).any(|e| tr.port.cash[e] != g.cash[e]) {
            squeezed.fetch_add(1, Relaxed);
        }
    });
    coverage("SqueezeOutMinority (sold, changed)", &[sold.into_inner(), squeezed.into_inner()]);
}

#[test]
fn diff_step_assess_position() {
    let cheat = AtomicU32::new(0);
    let boosted = AtomicU32::new(0);
    par_seeds(1..=24, |seed| {
        let mut g = ai_world(seed + 2300);
        let mut r = Rand(seed * 71 + 5);
        let p = g.cur_player;
        g.difficulty_level = cast::<i32, _>(r.range(1, 4));
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            g.num_players = 2;
            g.entity_status[2] = 1;
            if p > 2 {
                g.cur_player = 1;
                g.is_computer_player[1] = 1;
            }
        }
        if seed % 2 == 0 {
            // player 1 far behind player 2 at difficulty >= 2: the AI cheats
            g.num_players = 2;
            g.entity_status[2] = 1;
            g.cur_player = 1;
            g.is_computer_player[1] = 1;
            g.difficulty_level = cast::<i32, _>(r.range(2, 3));
            g.cash[1] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(100.0));
            g.cash[2] = f64r(&mut r, Ext::from_f64(1e7), Ext::from_f64(1e8));
            for _ in 0..6 {
                let c = active_corp(&g, &mut r);
                g.corp_controller[c] = 1;
            }
        }
        let p2 = g.cur_player;
        let owned: Vec<i32> = (11..=1600).filter(|&c| g.entity_status[c] == 1 && g.corp_controller[c] == p2).collect();
        if !owned.is_empty() {
            let c = owned[cast::<usize, _>(r.range(0, cast::<i64, _>(owned.len()) - 1))];
            match r.range(0, 1 + cast::<i64, _>(seed % 2)) {
                0 => g.asbestos_corp = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { c } else { -c },
                1 => {
                    g.superfund_corp = cast::<i16, _>(c);
                    g.superfund_end_year = g.cur_year + 3;
                }
                _ => {}
            }
        }
        g.ai_mgmt_boost_year = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { g.cur_year } else { g.cur_year - 1 };
        let _ = p;
        let rs = cast::<u32, _>(r.next());
        let Some(tr) = run_turn(&g, rs, Some(&[ASSESS]), &[], &[]) else { return };
        if !check_turn(&format!("AssessPosition seed {seed}"), &tr, Ext::from_f64(1e-12)) {
            return;
        }
        if tr.port.asbestos_corp != g.asbestos_corp || tr.port.superfund_corp != g.superfund_corp {
            cheat.fetch_add(1, Relaxed);
        }
        if (11..=1600).any(|e| tr.port.profit_margin[e] != g.profit_margin[e]) {
            boosted.fetch_add(1, Relaxed);
        }
    });
    coverage("AssessPosition (cheat, boosted)", &[cheat.into_inner(), boosted.into_inner()]);
}

#[test]
fn diff_step_freeze_rival_credit() {
    let frozen = AtomicU32::new(0);
    let called = AtomicU32::new(0);
    let renewed = AtomicU32::new(0);
    par_seeds(1..=32, |seed| {
        let mut g = ai_world(seed + 2500);
        let mut r = Rand(seed * 79 + 7);
        g.num_players = 2;
        let p = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { 1 } else { 2 };
        let h = 3 - p;
        g.cur_player = p;
        g.is_computer_player[p] = 1;
        g.is_computer_player[h] = 0;
        g.entity_status[1] = 1;
        g.entity_status[2] = 1;
        if g.hold_stock_id[(1, p)] < 11 {
            let t = active_corp(&g, &mut r);
            clear_holdings(&mut g, p);
            add_holding(&mut g, &mut r, p, t, 40);
        }
        let b = active_in(&g, &mut r, 11, 85);
        g.lending_bank[h] = cast::<i16, _>(b);
        if seed % 4 != 0 {
            g.corp_controller[b] = p;
        }
        g.difficulty_level = cast::<i32, _>(r.range(1, 3));
        g.loan_call_news_count = cast::<i32, _>(r.range(0, 2));
        if seed % 3 != 2 {
            clear_holdings(&mut g, h);
            g.t_bills[h] = Ext::from_f64(0.0);
        }
        match seed % 3 {
            0 => {
                g.bank_debt[h] = f64r(&mut r, Ext::from_f64(5000.0), Ext::from_f64(50000.0));
                g.cash[h] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(500.0));
                g.t_bills[h] = Ext::from_f64(0.0);
                g.credit_frozen[h] = cast::<i32, _>(r.range(0, 1));
            }
            1 => {
                g.bank_debt[h] = f64r(&mut r, Ext::from_f64(2000.0), Ext::from_f64(8000.0));
                g.cash[h] = g.bank_debt[h] * f64r(&mut r, Ext::from_f64(0.1), Ext::from_f64(0.4));
                g.credit_frozen[h] = cast::<i32, _>(r.range(0, 1));
            }
            _ => {
                g.bank_debt[h] = Ext::from_f64(0.0);
                g.cash[h] = f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(50000.0));
                g.credit_frozen[h] = 1;
            }
        }
        let rs = cast::<u32, _>(r.next());
        let Some(tr) = run_turn(&g, rs, Some(&[FREEZE]), &[], &[]) else { return };
        if !check_turn(&format!("FreezeRivalCredit seed {seed}"), &tr, Ext::from_f64(1e-12)) {
            return;
        }
        if tr.port.credit_frozen[h] == 1 && g.credit_frozen[h] == 0 {
            frozen.fetch_add(1, Relaxed);
        }
        if tr.port.bank_debt[h] < g.bank_debt[h] {
            called.fetch_add(1, Relaxed);
        }
        if tr.port.credit_frozen[h] == 0 && g.credit_frozen[h] == 1 {
            renewed.fetch_add(1, Relaxed);
        }
    });
    coverage(
        "FreezeRivalCredit (frozen, called, renewed)",
        &[frozen.into_inner(), called.into_inner(), renewed.into_inner()],
    );
}

#[test]
fn diff_step_acquire_holding_co() {
    let drew = AtomicU32::new(0);
    let founded = AtomicU32::new(0);
    par_seeds(1..=32, |seed| {
        let mut g = ai_world(seed + 2700);
        let mut r = Rand(seed * 83 + 11);
        let p = g.cur_player;
        clear_holdings(&mut g, p);
        g.cash[p] = match r.range(0, 3) {
            0 => f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(200.0)),
            1 => f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(5000.0)),
            _ => f64r(&mut r, Ext::from_f64(5000.0), Ext::from_f64(500000.0)),
        } * g.exchange_rate;
        g.t_bills[p] = if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(3000.0)) } else { Ext::from_f64(0.0) };
        // free slots for new insurers / holding companies
        if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
            let i = cast::<i32, _>(r.range(121, 130));
            g.entity_status[i] = 0;
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
            let i = cast::<i32, _>(r.range(1471, 1580));
            g.entity_status[i] = 0;
        }
        let rs = cast::<u32, _>(r.next());
        let Some(tr) = run_turn(&g, rs, Some(&[ACQUIRE]), &[], &[]) else { return };
        if !check_turn(&format!("AcquireHoldingCo seed {seed}"), &tr, Ext::from_f64(1e-12)) {
            return;
        }
        // (purchases go through BuyStock, skipped on both sides until it is ported)
        let h = cast::<i32, _>(tr.port.hold_stock_id[(1, p)]);
        if tr.port.bank_debt[p] != g.bank_debt[p] {
            drew.fetch_add(1, Relaxed);
        }
        if h > 0 && g.entity_status[h] != 1 {
            founded.fetch_add(1, Relaxed);
        }
    });
    coverage("AcquireHoldingCo (credit drawn, founded)", &[drew.into_inner(), founded.into_inner()]);
}

#[test]
fn diff_step_buy_corp_bonds() {
    let bought = AtomicU32::new(0);
    par_seeds(1..=24, |seed| {
        let mut g = ai_world(seed + 2900);
        let mut r = Rand(seed * 89 + 13);
        let p = if seed % 6 == 0 { 2 } else { 1 };
        g.cur_player = p;
        g.is_computer_player[p] = 1;
        g.entity_status[p] = 1;
        g.base_interest_rate = cast::<f32, _>([8.0f32, 11.0f32, 12.5f32, 14.0f32][cast::<usize, _>(r.range(0, 3))]);
        for _ in 0..r.range(1, 4) {
            let b = active_in(&g, &mut r, 11, 130);
            g.corp_controller[b] = p;
            g.cash[b] = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(20000.0));
        }
        for c in 131..=1600 {
            if g.entity_status[c] == 1 && g.bonds_issued[c] != 0 && r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                g.proj_qtr_earnings[c] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(100.0));
            }
        }
        let rs = cast::<u32, _>(r.next());
        let Some(tr) = run_turn(&g, rs, Some(&[BUY_BONDS]), &[], &[]) else { return };
        if !check_turn(&format!("BuyCorpBonds seed {seed}"), &tr, Ext::from_f64(1e-12)) {
            return;
        }
        if bonds_changed(&tr.port, &g) {
            bought.fetch_add(1, Relaxed);
        }
    });
    coverage("BuyCorpBonds (bought)", &[bought.into_inner()]);
}

#[test]
fn diff_step_sell_corp_bonds() {
    let sold = AtomicU32::new(0);
    par_seeds(1..=24, |seed| {
        let mut g = ai_world(seed + 3100);
        let mut r = Rand(seed * 97 + 17);
        let p = g.cur_player;
        g.base_interest_rate = cast::<f32, _>([4.0f32, 5.5f32, 6.0f32][cast::<usize, _>(r.range(0, 2))]);
        for _ in 0..r.range(1, 5) {
            let b = active_in(&g, &mut r, 11, 130);
            g.corp_controller[b] = p;
            let n = r.range(1, 6);
            add_bonds(&mut g, &mut r, b, n);
        }
        let rs = cast::<u32, _>(r.next());
        let Some(tr) = run_turn(&g, rs, Some(&[SELL_BONDS]), &[], &[]) else { return };
        if !check_turn(&format!("SellCorpBonds seed {seed}"), &tr, Ext::from_f64(1e-12)) {
            return;
        }
        if tr.port.turn_action_count != tr.before.turn_action_count && bonds_changed(&tr.port, &g) {
            sold.fetch_add(1, Relaxed);
        }
    });
    coverage("SellCorpBonds (sold)", &[sold.into_inner()]);
}

#[test]
fn diff_step_repay_frozen_loan() {
    let repaid = AtomicU32::new(0);
    let new_banker = AtomicU32::new(0);
    let sold_loan = AtomicU32::new(0);
    par_seeds(1..=32, |seed| {
        let mut g = ai_world(seed + 3300);
        let mut r = Rand(seed * 101 + 19);
        let p = g.cur_player;
        let b = active_in(&g, &mut r, 11, 85);
        g.lending_bank[p] = cast::<i16, _>(b);
        if seed % 2 == 0 {
            g.credit_frozen[p] = 1;
            g.bank_debt[p] = [Ext::from_f64(0.0), f64r(&mut r, Ext::from_f64(10.0), Ext::from_f64(500.0)), f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(20000.0))][cast::<usize, _>(r.range(0, 2))];
            g.cash[p] = g.bank_debt[p] * f64r(&mut r, Ext::from_f64(0.1), Ext::from_f64(2.0));
        } else {
            g.credit_frozen[p] = 0;
            if g.corp_controller[b] == p {
                g.corp_controller[b] = 0;
            }
            g.bank_debt[p] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { Ext::from_f64(0.0) } else { f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(5000.0)) };
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
            let c = active_in(&g, &mut r, 11, 85);
            if c != b {
                g.corp_controller[c] = p;
                g.cash[c] = f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(50000.0));
            }
        }
        let rs = cast::<u32, _>(r.next());
        let Some(tr) = run_turn(&g, rs, Some(&[REPAY]), &[], &[]) else { return };
        if !check_turn(&format!("RepayFrozenLoan seed {seed}"), &tr, Ext::from_f64(1e-12)) {
            return;
        }
        if g.credit_frozen[p] == 1 && tr.port.bank_debt[p] < g.bank_debt[p] {
            repaid.fetch_add(1, Relaxed);
        }
        if g.credit_frozen[p] == 1 && tr.port.credit_frozen[p] == 0 {
            new_banker.fetch_add(1, Relaxed);
        }
        if g.credit_frozen[p] == 0 && tr.port.lending_bank[p] != g.lending_bank[p] {
            sold_loan.fetch_add(1, Relaxed);
        }
    });
    coverage(
        "RepayFrozenLoan (repaid, new banker, switched)",
        &[repaid.into_inner(), new_banker.into_inner(), sold_loan.into_inner()],
    );
}
