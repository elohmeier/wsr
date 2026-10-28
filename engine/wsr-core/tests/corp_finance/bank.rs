//! Differential tests of logic::corp_finance::bank (BankLiquidityAndCapitalCheck and helpers).
#![allow(unused_imports)]
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use wsr_core::logic::corp_finance::bank::*;
use wsr_core::logic::finance::calc_entity_financials;
use wsr_core::pb::rng::Rng;
use wsr_core::pb::numeric::Cur;
use wsr_core::state::Globals;

const BANK_LIQ_CHECK: u32 = 0x5021e2;
const SELL_SUBPRIME: u32 = 0x501bad;
const CONFIRM_DISCOUNT: u32 = 0x4a2d43;

/// Counts seeds where a branch was taken; asserts at the end that it happened at least `min` times.
struct Hits(std::sync::atomic::AtomicUsize, String, usize);

impl Hits {
    fn new(what: &str, min: usize) -> Self {
        Hits(std::sync::atomic::AtomicUsize::new(0), what.to_string(), min)
    }
    fn count(&self, hit: bool) {
        if hit {
            self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
    fn check(&self) {
    let __content = wsr_core::content::current();
        if unavailable().is_none() {
            let n = self.0.load(std::sync::atomic::Ordering::Relaxed);
            assert!(n >= self.2, "{}{__wsr_text_13645}{n}{__wsr_text_13646}", self.1,__wsr_text_13645=__content.string(13645), __wsr_text_13646=__content.string(13646));
        }
    }
}

/// A world where bank `b` is short of cash: deposits, loan books, subprime securities, borrowers.
fn bworld(seed: u64) -> (Globals, i32) {
    let mut g = world(seed);
    let mut r = Rand(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) ^ 0x7777);
    let b = cast::<i32, _>(r.range(11, 85));
    g.entity_status[b] = 1;
    for h in 11..=130 {
        g.subprime_secs[h] = if r.chance(Ext::from_f64(0.5)) { r.range(0, 8000) } else { 0 };
        g.bank_alloc_off[h] = if r.chance(Ext::from_f64(0.6)) { 1 } else { 0 };
        g.long_gov_bond_face[h] = if r.chance(Ext::from_f64(0.4)) { r.range(0, 4000) } else { 0 };
        g.short_gov_bond_face[h] = if r.chance(Ext::from_f64(0.4)) { r.range(0, 4000) } else { 0 };
        g.lt_gov_bond_cost[h] = cast::<Ext, _>(g.long_gov_bond_face[h]) * f64r(&mut r, Ext::from_f64(0.8), Ext::from_f64(1.1));
        g.st_gov_bond_cost[h] = cast::<Ext, _>(g.short_gov_bond_face[h]) * f64r(&mut r, Ext::from_f64(0.9), Ext::from_f64(1.05));
        if r.chance(Ext::from_f64(0.3)) {
            g.corp_controller[h] = h;
        }
    }
    // BankAllocate_SellLosingStocks: keep the banks' stock holdings profitable (see sim_entity tests).
    for h in 11..=85 {
        for slot in 1..=15 {
            let c = cast::<i32, _>(g.hold_stock_id[(slot, h)]);
            if c > 10 && g.earnings_ytd[c] < Ext::from_f64(0.0) {
                g.earnings_ytd[c] = -g.earnings_ytd[c];
            }
        }
    }
    // mostly without stock holdings (else ForcedSellStocks usually fixes the cash at once)
    if r.chance(Ext::from_f64(0.75)) {
        for slot in 0..=15 {
            g.hold_stock_id[(slot, b)] = 0;
            g.hold_stock_pct[(slot, b)] = 0;
        }
    }
    // the bank's trouble
    let hi = if r.chance(Ext::from_f64(0.5)) { Ext::from_f64(3000.0) } else { Ext::from_f64(20000.0) };
    g.cash[b] = -f64r(&mut r, Ext::from_f64(50.0), hi);
    g.t_bills[b] = if r.chance(Ext::from_f64(0.7)) { Ext::from_f64(0.0) } else { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(500.0)) };
    g.bank_debt[b] = match r.range(0, 3) {
        0 => Ext::from_f64(0.0),
        1 => f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(2000.0)),
        _ => f64r(&mut r, Ext::from_f64(2000.0), Ext::from_f64(3000.0)),
    };
    g.bank_consumer_loans[b] = if r.chance(Ext::from_f64(0.6)) { r.range(0, 30000) } else { r.range(0, 1500) };
    g.bank_mortgage_loans[b] = if r.chance(Ext::from_f64(0.6)) { r.range(0, 30000) } else { r.range(0, 1200) };
    g.subprime_secs[b] = if r.chance(Ext::from_f64(0.5)) { r.range(1000, 9000) } else { 0 };
    g.bank_cust_deposits[b] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(5000.0));
    if r.chance(Ext::from_f64(0.3)) {
        g.bonds_issued[b] = 0;
    }
    g.corp_controller[b] = match r.range(0, 4) {
        0 => 0,
        1 => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
        2 => b,
        _ => cast::<i32, _>(r.range(131, 1580)),
    };
    g.credit_rating_num[b] = cast::<i32, _>(r.range(1, 8));
    g.subprime_crisis_state = [0, 0, -2, 1, 2][cast::<usize, _>(r.range(0, 4))];
    // borrowers of the bank
    let n = r.range(0, 120);
    for _ in 0..n {
        let d = cast::<i32, _>(if r.chance(Ext::from_f64(0.05)) { r.range(1, cast::<i64, _>(g.num_players)) } else { r.range(86, 1600) });
        g.lending_bank[d] = cast::<i16, _>(b);
        g.bank_debt[d] = if r.chance(Ext::from_f64(0.5)) { f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(300.0)) } else { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(3000.0)) };
    }
    g.bank_largest_debtor[b] = if r.chance(Ext::from_f64(0.5)) { cast::<i32, _>(r.range(86, 1600)) } else { 0 };
    g.borrow_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(3.0), Ext::from_f64(14.0)));
    g.player_recent_corp = if r.chance(Ext::from_f64(0.5)) { cast::<i16, _>(r.range(131, 1580)) } else { 0 };
    // CalcEntityFinancials re-totals the customer deposits and moves the difference into cash: start
    // from the consistent total so the bank stays short of cash.
    let mut g2 = g.clone();
    calc_entity_financials(&mut g2, &mut Rec::default(), &mut Rng::from_state(1), b);
    g.bank_cust_deposits[b] = g2.bank_cust_deposits[b];
    (g, b)
}

fn check(addr: u32, args: &[&str], g: &Globals, rng0: u32, answers: &[&str], port: impl FnOnce(&mut Globals, &mut Rec, &mut Rng)) -> Option<(Globals, Rec, u32)> {
    let o = run(addr, args, g, rng0, answers, &[])?;
    let mut p = g.clone();
    let mut ui = Rec::default();
    ui.answers.extend(answers.iter().map(|a| if *a == "IDNO" { 7 } else { 6 }));
    let mut rng = Rng::from_state(rng0);
    port(&mut p, &mut ui, &mut rng);
    cmp().assert(&p, &o.globals);
    assert_eq!(rng.state(), o.rng, "RND state");
    assert_eq!(ui.msgs.len(), orig_msgs(&o), "message boxes: {:?} / {:?}", ui.msgs, o.log);
    assert!(!o.log.iter().any(|l| l.starts_with("PBERR")), "runtime error in the original: {:?}", o.log);
    Some((p, ui, o.return_value))
}

#[test]
fn diff_bank_liquidity_and_capital_check() {
    let __content = wsr_core::content::current();
    let consumer = Hits::new("consumer loan sale", 5);
    let mortgage = Hits::new("mortgage sale", 5);
    let subprime = Hits::new("subprime sale", 3);
    let bonds = Hits::new(__content.string(13647), 1);
    let stock = Hits::new(__content.string(13648), 1);
    let call_in = Hits::new("loan call-in", 3);
    let sold_loans = Hits::new("business loans sold", 3);
    let frozen = Hits::new("credit frozen", 2);
    let fixed = Hits::new("cash fixed", 10);
    par_seeds(1..=160, |seed| {
        let (mut g, b) = bworld(seed);
        let mut r = Rand(seed ^ 0xabc);
        // vary the stage at which the rescue starts
        match seed % 4 {
            1 => {
                g.bank_consumer_loans[b] = 0;
                g.bank_mortgage_loans[b] = 0;
                g.subprime_secs[b] = 0;
            }
            2 => {
                g.bank_consumer_loans[b] = 0;
                g.bank_mortgage_loans[b] = 0;
                g.subprime_secs[b] = 0;
                g.bonds_issued[b] = 0;
                g.corp_controller[b] = cast::<i32, _>(r.range(131, 1580));
                g.cash[b] = -f64r(&mut r, Ext::from_f64(3000.0), Ext::from_f64(30000.0));
                // assets (government bonds) so that the net worth stays positive
                g.long_gov_bond_face[b] = r.range(40000, 80000);
                g.lt_gov_bond_cost[b] = cast::<Ext, _>(g.long_gov_bond_face[b]);
                g.credit_rating_num[b] = cast::<i32, _>(r.range(2, 6));
                g.borrow_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(3.0), Ext::from_f64(10.0)));
            }
            _ => {}
        }
        let rng0 = (cast::<u32, _>(seed)).wrapping_mul(0x9e37_79b9) ^ 0x1234;
        let a = b.to_string();
        if let Some((p, ui, _)) = check(BANK_LIQ_CHECK, &[&a], &g, rng0, &[], |g, ui, rng| {
            bank_liquidity_and_capital_check(g, ui, rng, b);
        }) {
            consumer.count(p.bank_consumer_loans[b] < g.bank_consumer_loans[b]);
            mortgage.count(p.bank_mortgage_loans[b] < g.bank_mortgage_loans[b]);
            subprime.count(p.subprime_secs[b] < g.subprime_secs[b]);
            bonds.count(g.bonds_issued[b] == 0 && p.bonds_issued[b] > 0);
            let news = |s: &str| ui.news.iter().any(|n| n.contains(s)) || p.news_msg.to_display().contains(s);
            stock.count(news(__content.string(13649)));
            call_in.count(news(__content.string(13650)));
            sold_loans.count(news(__content.string(6814)));
            frozen.count(p.credit_frozen[b] == 1 && g.credit_frozen[b] == 0);
            fixed.count(p.cash[b] >= Ext::from_f64(0.0));
        }
    });
    for h in [&consumer, &mortgage, &subprime, &bonds, &stock, &call_in, &sold_loans, &frozen, &fixed] {
        h.check();
    }
}

#[test]
fn diff_bank_sell_subprime_secs() {
    let __content = wsr_core::content::current();
    let bought = Hits::new(__content.string(13651), 3);
    let steep = Hits::new("steep discount", 2);
    par_seeds(1..=40, |seed| {
        let (mut g, _) = bworld(seed + 1000);
        let mut r = Rand(seed);
        let id = cast::<i32, _>(r.range(5, 135));
        if (11..=130).contains(&id) {
            g.entity_status[id] = 1;
            g.subprime_secs[id] = r.range(0, 10000);
        }
        g.gdp_growth = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(12.0), Ext::from_f64(6.0)));
        g.subprime_crisis_state = [0, -1, -2, 1, 2][cast::<usize, _>(r.range(0, 4))];
        for h in 11..=130 {
            if r.chance(Ext::from_f64(0.5)) {
                g.cash[h] = f64r(&mut r, Ext::from_f64(3000.0), Ext::from_f64(30000.0));
            }
        }
        let rng0 = (cast::<u32, _>(seed)).wrapping_mul(0x85eb_ca6b);
        let a = id.to_string();
        if let Some((_, ui, _)) = check(SELL_SUBPRIME, &[&a], &g, rng0, &[], |g, ui, rng| {
            bank_sell_subprime_secs(g, ui, rng, id);
        }) {
            let s = ui.news.join("|");
            bought.count(s.contains(__content.string(13652)));
            steep.count(s.contains("steep"));
        }
    });
    bought.check();
    steep.check();
}

#[test]
fn diff_confirm_subprime_sale_discount() {
    let __content = wsr_core::content::current();
    let asked = Hits::new("asked", 5);
    par_seeds(1..=40, |seed| {
        let mut g = world(seed + 2000);
        let mut r = Rand(seed);
        g.gdp_growth = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(12.0), Ext::from_f64(6.0)));
        g.subprime_crisis_state = [0, -1, -2, 1, 2][cast::<usize, _>(r.range(0, 4))];
        let ask = cast::<i32, _>(r.range(0, 1));
        let quality = cast::<i16, _>(r.range(-30, 30));
        let answer = if r.chance(Ext::from_f64(0.5)) { "IDYES" } else { "IDNO" };
        let rng0 = cast::<u32, _>(seed);
        let a = ask.to_string();
        let q = format!("{__wsr_text_13653}{quality}",__wsr_text_13653=__content.string(13653));
        let mut ret = 0;
        if let Some((_, ui, rv)) = check(CONFIRM_DISCOUNT, &[&a, &q], &g, rng0, &[answer], |g, ui, _| {
            ret = confirm_subprime_sale_discount(g, ui, ask, quality);
        }) {
            assert_eq!(ret, cast::<i32, _>(rv), "result");
            asked.count(!ui.msgs.is_empty());
        }
    });
    asked.check();
}

