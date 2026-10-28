//! Differential tests of `logic::corp_ai::misc`: every routine on random plausible worlds against the
//! original.
#![allow(unused_imports)]
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use wsr_core::logic::corp_ai::misc::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

fn seed_rng(seed: u64) -> u32 {
    cast::<u32, _>(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 17)
}

/// Runs the original at `addr` and the port on `g`; compares state, RND state, message boxes and news.
/// Returns the number of state differences to the input (None if the harness is unavailable).
fn check(
    addr: u32,
    args: &[&str],
    g: &Globals,
    rng0: u32,
    port: impl FnOnce(&mut Globals, &mut Rec, &mut Rng),
) -> Option<usize> {
    let o = run(addr, args, g, rng0, &[], &[])?;
    let mut p = g.clone();
    let mut ui = Rec::default();
    let mut rng = Rng::from_state(rng0);
    port(&mut p, &mut ui, &mut rng);
    cmp().assert(&p, &o.globals);
    assert_eq!(rng.state(), o.rng, "RND state (0x{addr:x})");
    assert_eq!(ui.msgs.len(), orig_msgs(&o), "message boxes (0x{addr:x}): {:?} / {:?}", ui.msgs, o.log);
    Some(Compare::default().diffs(g, &o.globals).len())
}

fn count(n: &AtomicUsize, hit: bool) {
    if hit {
        n.fetch_add(1, Ordering::Relaxed);
    }
}

fn report(what: &str, n: &AtomicUsize) {
    let __content = wsr_core::content::current();
    let k = n.load(Ordering::Relaxed);
    eprintln!("{__wsr_text_13574}{what}: {k}",__wsr_text_13574=__content.string(13574));
    if unavailable().is_none() {
        assert!(k > 0, "no seed reached: {what}");
    }
}

#[test]
fn diff_accrue_gov_bond_interest() {
    static N: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=8, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed ^ 0x1234);
        for e in (1..=130).chain(1581..=1583).chain([200, 700]) {
            if r.chance(Ext::from_f64(0.5)) {
                g.long_gov_bond_face[e] = r.range(0, 5000);
                g.lt_gov_bond_cost[e] = cast::<Ext, _>(g.long_gov_bond_face[e]) * f64r(&mut r, Ext::from_f64(0.8), Ext::from_f64(1.2));
            }
            if r.chance(Ext::from_f64(0.5)) {
                g.short_gov_bond_face[e] = r.range(-5, 5000);
                g.st_gov_bond_cost[e] = cast::<Ext, _>(g.short_gov_bond_face[e]) * f64r(&mut r, Ext::from_f64(0.8), Ext::from_f64(1.2));
            }
        }
        if seed % 3 == 0 {
            // matured descriptions: quarters to maturity floored at 1
            g.long_bond_desc = st(&format!("{} LONG T-BOND  4.25", g.cur_year - 1));
        }
        let n = check(0x4c572b, &[], &g, seed_rng(seed), |p, _, _| accrue_gov_bond_interest(p));
        count(&N, n.is_some_and(|n| n > 0));
    });
    report("gov bond interest accrued", &N);
}

#[test]
fn diff_turnaround_news() {
    static NEWS: AtomicUsize = AtomicUsize::new(0);
    static MARGIN: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=60, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed ^ 0x77);
        let c = cast::<i32, _>(r.range(131, 1580));
        g.sim_co_count = c;
        g.entity_status[c] = 1;
        g.industry_num[c] = cast::<i16, _>(r.range(3, 68));
        g.profit_margin[c] = cast::<i16, _>(r.range(-10, 18));
        g.earnings_ytd[c] = -f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(20.0));
        g.earnings_y1[c] = -f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(20.0));
        g.earnings_yr2_ago[c] = -f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(20.0));
        g.earnings_yr3_ago[c] = -f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(20.0));
        g.earnings_yr4_ago[c] = if r.chance(Ext::from_f64(0.9)) { -f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(20.0)) } else { Ext::from_f64(3.0) };
        g.gdp_growth = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(3.0), Ext::from_f64(5.0)));
        g.market_cap[c] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(50.0));
        g.business_assets[c] = g.market_cap[c] * f64r(&mut r, Ext::from_f64(30.0), Ext::from_f64(60.0));
        g.corp_controller[c] = if r.chance(Ext::from_f64(0.8)) { cast::<i32, _>(r.range(11, 1600)) } else { 1 };
        g.rand_seed = cast::<i32, _>(r.range(-10, 32000));
        if r.chance(Ext::from_f64(0.15)) {
            g.news_headline[60] = st("XYZ returns to profitability");
        }
        let pm = g.profit_margin[c];
        let mut news = false;
        let n = check(0x4c67a5, &[], &g, seed_rng(seed), |p, _, rng| {
            turnaround_news(p, rng);
            news = !p.news_msg.is_empty();
        });
        if n.is_some() {
            count(&NEWS, news);
            let mut p = g.clone();
            turnaround_news(&mut p, &mut Rng::from_state(seed_rng(seed)));
            count(&MARGIN, p.profit_margin[c] != pm);
        }
    });
    report("turnaround: news", &NEWS);
    report("turnaround: margin", &MARGIN);
}

#[test]
fn diff_charge_superfund_costs() {
    let __content = wsr_core::content::current();
    static N: AtomicUsize = AtomicUsize::new(0);
    static SHORT: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=24, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed ^ 0x99);
        let c = match seed % 6 {
            0 => cast::<i32, _>(r.range(0, 10)),
            _ => cast::<i32, _>(r.range(131, 1580)),
        };
        g.superfund_corp = cast::<i16, _>(c);
        if c > 10 {
            g.entity_status[c] = if r.chance(Ext::from_f64(0.85)) { 1 } else { 0 };
            if seed % 8 == 1 {
                g.industry_num[c] = 71;
            }
            g.market_cap[c] = if r.chance(Ext::from_f64(0.1)) { Ext::from_f64(0.01) } else { f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(500.0)) };
            if seed % 2 == 0 {
                // a player is short the stock
                g.hold_stock_id[(15, 1)] = cast::<i16, _>(c);
                g.hold_stock_pct[(15, 1)] = -(cast::<i16, _>(r.range(1, 15)));
            }
        }
        if r.chance(Ext::from_f64(0.3)) {
            g.news_msg = st(__content.string(13571));
        }
        let mut shorted = false;
        let n = check(0x4bbc32, &[], &g, seed_rng(seed), |p, ui, rng| {
            let y = p.superfund_end_year;
            charge_superfund_costs(p, ui, rng);
            shorted = p.superfund_end_year != y;
        });
        if let Some(n) = n {
            count(&N, n > 0);
            count(&SHORT, shorted);
        }
    });
    report("superfund charged", &N);
    report("superfund: short squeeze", &SHORT);
}

#[test]
fn diff_amortize_loan_portfolios() {
    par_seeds(1..=6, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed ^ 0x31);
        for e in 11..=130 {
            g.subprime_secs[e] = if r.chance(Ext::from_f64(0.5)) { r.range(-10, 50_000) } else { 0 };
        }
        check(0x4c6227, &[], &g, seed_rng(seed), |p, _, _| amortize_loan_portfolios(p));
    });
}

#[test]
fn diff_asbestos_reserve_charge() {
    static N: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=24, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed ^ 0x5a);
        let c = cast::<i32, _>(r.range(131, 1580));
        g.asbestos_corp = if r.chance(Ext::from_f64(0.5)) { -c } else { c };
        if seed % 7 == 0 {
            g.asbestos_corp = 5;
        }
        g.entity_status[c] = 1;
        g.contingency_reserve[c] = if r.chance(Ext::from_f64(0.4)) { Ext::from_f64(0.0) } else { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(300.0)) };
        let mut charged = false;
        let n = check(0x4bc147, &[], &g, seed_rng(seed), |p, ui, rng| {
            asbestos_reserve_charge(p, ui, rng);
            charged = !ui.news.is_empty() || p.contingency_reserve[c] != g.contingency_reserve[c];
        });
        if n.is_some() {
            count(&N, charged);
        }
    });
    report("asbestos reserve added", &N);
}

#[test]
fn diff_rebase_stock_index_divisor() {
    static N: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=8, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed ^ 0x3);
        g.stock_index = match seed % 4 {
            0 => f64r(&mut r, Ext::from_f64(4850.0), Ext::from_f64(5150.0)),
            1 => Ext::from_f64(4850.0),
            _ => f64r(&mut r, Ext::from_f64(500.0), Ext::from_f64(20000.0)),
        };
        let n = check(0x4c5fef, &[], &g, seed_rng(seed), |p, ui, rng| rebase_stock_index_divisor(p, ui, rng));
        count(&N, n.is_some_and(|n| n > 0));
    });
    report("index rebased", &N);
}

#[test]
fn diff_young_corp_margin_erosion() {
    static N: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=40, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed ^ 0x44);
        let mut c = cast::<i32, _>(r.range(131, 1580));
        c = c - c % 4 + g.cur_quarter % 4;
        if seed % 9 == 0 {
            c += 1;
        }
        g.sim_co_count = c;
        g.entity_status[c] = 1;
        g.industry_num[c] = cast::<i16, _>(r.range(3, 69));
        g.earnings_yr4_ago[c] = if r.chance(Ext::from_f64(0.9)) { Ext::from_f64(0.0) } else { Ext::from_f64(1.0) };
        g.earnings_yr3_ago[c] = if r.chance(Ext::from_f64(0.5)) { Ext::from_f64(0.0) } else { Ext::from_f64(1.0) };
        g.earnings_yr2_ago[c] = if r.chance(Ext::from_f64(0.5)) { Ext::from_f64(0.0) } else { Ext::from_f64(1.0) };
        g.earnings_y1[c] = if r.chance(Ext::from_f64(0.5)) { Ext::from_f64(0.0) } else { Ext::from_f64(1.0) };
        g.disc_spend_pct[c] = cast::<i16, _>(r.range(0, 14));
        let n = check(0x4b819b, &[], &g, seed_rng(seed), |p, _, rng| young_corp_margin_erosion(p, rng));
        count(&N, n.is_some_and(|n| n > 0));
    });
    report("margin eroded", &N);
}

#[test]
fn diff_trim_recent_dividend_changes() {
    let lists = [
        "", "|", "12", "1234", "1234|", "12|4567|", "1234|5678|", "123|4567|8901|", "1|2|3|4|5", "12345678",
        "1234|567", "12|3456", "abcd|efgh|ijkl|mnop|",
    ];
    par_seeds(0..cast::<u64, _>(lists.len()), |i| {
        let mut g = Globals::default();
        g.recent_dividend_change_list = st(lists[cast::<usize, _>(i)]);
        check(0x4bbfb1, &[], &g, 1, |p, _, _| trim_recent_dividend_changes(p));
    });
}

#[test]
fn diff_check_excess_loss_recapture() {
    static N: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=16, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed ^ 0xe1);
        let c = cast::<i32, _>(r.range(11, 1600));
        g.entity_status[c] = 1;
        for slot in 1..=15 {
            if r.chance(Ext::from_f64(0.4)) {
                let s = cast::<i16, _>(r.range(131, 1580));
                g.hold_stock_id[(slot, c)] = s;
                g.hold_stock_pct[(slot, c)] = cast::<i16, _>(r.range(1, 100));
                g.hold_stock_basis[(slot, c)] = f64r(&mut r, -Ext::from_f64(300.0), Ext::from_f64(300.0));
            }
        }
        g.hold_stock_id[(0, c)] = cast::<i16, _>((1..=15).filter(|&s| g.hold_stock_id[(s, c)] != 0).count());
        let a = c.to_string();
        let n = check(0x4ba8d5, &[&a], &g, seed_rng(seed), |p, ui, _| check_excess_loss_recapture(p, ui, c));
        count(&N, n.is_some_and(|n| n > 0));
    });
    report("excess loss recaptured", &N);
}

#[test]
fn diff_pick_hot_stock_corp() {
    static N: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=60, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed ^ 0xbb);
        g.hot_stock_corp = if r.chance(Ext::from_f64(0.5)) { 0 } else { cast::<i16, _>(r.range(11, 1600)) };
        g.sim_co_count = if r.chance(Ext::from_f64(0.5)) { 11 } else { cast::<i32, _>(r.range(11, 1600)) };
        for e in 131..=1580 {
            if r.chance(Ext::from_f64(0.3)) {
                g.market_cap[e] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(50.0));
            }
        }
        let mut picked = false;
        let n = check(0x4a5881, &[], &g, seed_rng(seed), |p, _, rng| {
            let h = p.hot_stock_corp;
            pick_hot_stock_corp(p, rng);
            picked = p.hot_stock_corp != h && p.hot_stock_corp != 0;
        });
        if n.is_some() {
            count(&N, picked);
        }
    });
    report("hot stock picked", &N);
}
