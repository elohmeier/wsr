//! Differential tests of `logic::etf_derivs::etf_mgmt`: CoverNegativeCash, EtfComplianceCheck,
//! EtfAdviserReview, AiEtfInvestCash, EtfLiquidateBondsOnDisqualify and ReviveDefunctEtfs.
//!
//! `TIME$` is fixed by the harness (10:00:00), so the ports get `time_secs = 0`.
#![allow(clippy::manual_is_multiple_of)]
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use wsr_core::logic::etf_derivs::etf_mgmt::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

const COVER_NEGATIVE_CASH: u32 = 0x504cd1;
const ETF_COMPLIANCE: u32 = 0x64b0c9;
const ETF_ADVISER_REVIEW: u32 = 0x64d7a0;
const AI_ETF_INVEST: u32 = 0x64e961;
const ETF_LIQ_BONDS: u32 = 0x656f9e;
const REVIVE_ETFS: u32 = 0x64ee77;
/// No callee is skipped any more.
const SKIPS: &[(u32, u32)] = &[];
/// BuyStock (hooked: the purchases of AiEtfInvestCash).
const BUY_STOCK: u32 = 0x4efe60;

fn rng0(seed: u64) -> u32 {
    (cast::<u32, _>(seed)).wrapping_mul(2_654_435_761) ^ 0x1b87_3593
}

#[track_caller]
fn check(port: &Globals, rng: &Rng, ui: &Rec, o: &Original, what: &str) {
    let __content = wsr_core::content::current();
    if std::env::var("DBG").is_ok() {
        for l in o.log.iter().filter(|l| !l.starts_with("API")) {
            eprintln!("{what}: {l}");
        }
        eprintln!("{what}: port msgs {:?}{__wsr_text_13611}{:?}", ui.msgs, ui.news,__wsr_text_13611=__content.string(13611));
    }
    let mut port = port.clone();
    align_option_premiums(&mut port, &o.globals);
    let d = cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-9)).diffs(&port, &o.globals);
    assert!(d.is_empty(), "{what}: {} differences (port vs original):\n{}", d.len(), d.join("\n"));
    assert_eq!(rng.state(), o.rng, "{what}: RND state");
    assert_eq!(ui.msgs.len(), orig_msgs(o), "{what}: message boxes {:?}", ui.msgs);
}

/// Option premiums are computed in 80-bit precision; the STR$ of a premium (last field of an option
/// record) can differ in the 15th digit. Takes the original's record where only that field differs.
fn align_option_premiums(port: &mut Globals, orig: &Globals) {
    for i in 0..=1000 {
        let (a, b) = (port.option_recs[i].to_display(), orig.option_recs[i].to_display());
        if a == b {
            continue;
        }
        let (pa, pb): (Vec<&str>, Vec<&str>) = (a.split('|').collect(), b.split('|').collect());
        let n = pa.len();
        if n < 2 || n != pb.len() || pa[..n - 1] != pb[..n - 1] {
            continue;
        }
        let (x, y): (Ext, Ext) = (pa[n - 1].trim().parse().unwrap_or(Ext::NAN), pb[n - 1].trim().parse().unwrap_or(Ext::NAN));
        if (x - y).abs() <= Ext::from_f64(1e-12) * y.abs() {
            port.option_recs[i] = orig.option_recs[i].clone();
        }
    }
}

/// The seeds to run (`ONLY_SEED=n` restricts to one).
fn seeds(r: std::ops::RangeInclusive<u64>) -> Vec<u64> {
    match std::env::var("ONLY_SEED").ok().and_then(|s| s.parse().ok()) {
        Some(n) => vec![n],
        None => r.collect(),
    }
}

fn hit(c: &AtomicUsize) {
    c.fetch_add(1, Ordering::Relaxed);
}

fn count(ui: &Rec, pat: &str) -> usize {
    ui.news.iter().filter(|n| n.contains(pat)).count()
}

/// Adds `n` random option records (a few small ones held by `holder`).
fn add_options(g: &mut Globals, r: &mut Rand, n: i32, holder: i32) {
    for i in 1..=n {
        let corp = cast::<i32, _>(r.range(131, 1580));
        let own = r.chance(Ext::from_f64(0.15));
        let h = if own { holder } else { cast::<i32, _>(r.range(1, 1600)) };
        let qty = if own { if r.chance(Ext::from_f64(0.7)) { r.range(1, 3) } else { -r.range(1, 3) } } else if r.chance(Ext::from_f64(0.6)) { r.range(1, 20) } else { -r.range(1, 20) };
        let strike = g.market_cap[corp] * f64r(r, Ext::from_f64(0.7), Ext::from_f64(1.3));
        g.option_recs[i] = PbStr::from(
            format!(
                "{corp}|{}|{h}|{qty}|{strike:.4}|{}|{}|{:.4}",
                r.range(1, 2),
                r.range(1, 12),
                g.cur_year + cast::<i32, _>(r.range(0, 1)),
                f64r(r, Ext::from(Ext::from_f64(0.0)), Ext::from(Ext::from_f64(50.0)))
            )
            .as_str(),
        );
    }
    g.option_recs[0] = PbStr::from(n.to_string().as_str());
}

/// [`world`] with active funds 1581..1600 (industry 71, advisers, holdings), share prices of 5..900 and
/// solid credit ratings for many corps.
fn etf_world(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let mut g = world(seed);
    let mut r = Rand(seed * 31 + 7);
    for e in 131..=1600 {
        if r.chance(Ext::from_f64(0.9)) {
            g.shares_outstanding[e] = g.market_cap[e] / f64r(&mut r, Ext::from_f64(5.0), Ext::from_f64(900.0));
        }
        if r.chance(Ext::from_f64(0.5)) {
            g.credit_rating_num[e] = cast::<i32, _>(r.range(7, 10));
        }
    }
    for e in 1..=1600 {
        for slot in 1..=15 {
            if g.hold_stock_pct[(slot, e)] > 12 && r.chance(Ext::from_f64(0.85)) {
                g.hold_stock_pct[(slot, e)] = cast::<i16, _>(r.range(1, 12));
            }
        }
    }
    for f in 1581..=1600 {
        if !r.chance(Ext::from_f64(0.85)) {
            continue;
        }
        g.entity_status[f] = 1;
        g.industry_num[f] = 71;
        g.fund_adviser[f] = cast::<i32, _>(r.range(86, 1580));
        g.entity_status[g.fund_adviser[f]] = 1;
        g.fund_fee_pct[f] = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.1), Ext::from_f64(1.2)));
        g.ctrl_since_year[f] = g.cur_year - cast::<i32, _>(r.range(0, 9));
        g.corp_controller[f] = if r.chance(Ext::from_f64(0.2)) { cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))) } else { 0 };
        g.bonds_issued[f] = if r.chance(Ext::from_f64(0.3)) { r.range(1, 500) } else { 0 };
        if r.chance(Ext::from_f64(0.6)) {
            g.bank_debt[f] = Ext::from_f64(0.0);
        }
        g.cash[f] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(4000.0));
        g.t_bills[f] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(4000.0));
        let n = if r.chance(Ext::from_f64(0.5)) { 15 } else { cast::<i32, _>(r.range(0, 15)) };
        for slot in 1..=15 {
            if slot <= n {
                g.hold_stock_id[(slot, f)] = cast::<i16, _>(r.range(131, 1580));
                g.hold_stock_pct[(slot, f)] = cast::<i16, _>(r.range(0, 9));
            } else {
                g.hold_stock_id[(slot, f)] = 0;
                g.hold_stock_pct[(slot, f)] = 0;
            }
        }
        g.hold_stock_id[(0, f)] = cast::<i16, _>(n);
    }
    for p in 1..=5 {
        g.auto_pilot[p] = cast::<i32, _>(r.range(0, 1));
    }
    for e in 11..=1600 {
        if r.chance(Ext::from_f64(0.1)) {
            g.auto_pilot[e] = 1;
        }
    }
    if r.chance(Ext::from_f64(0.3)) {
        g.news_msg = PbStr::from(__content.string(13571));
    }
    g
}

// ------------------------------------------------------------------------------- CoverNegativeCash

#[test]
fn diff_cover_negative_cash() {
    let __content = wsr_core::content::current();
    let (credit, wc, opts, stocks) = (AtomicUsize::new(0), AtomicUsize::new(0), AtomicUsize::new(0), AtomicUsize::new(0));
    par_seeds(seeds(1..=60), |seed| {
        let mut g = etf_world(seed);
        let mut r = Rand(seed * 13 + 5);
        let e = match seed % 6 {
            0 => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
            1 => cast::<i32, _>(r.range(11, 130)),
            2 => cast::<i32, _>(r.range(1581, 1600)),
            _ => cast::<i32, _>(r.range(131, 1580)),
        };
        g.entity_status[e] = if r.chance(Ext::from_f64(0.95)) { 1 } else { 2 };
        g.cash[e] = -f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(3000.0));
        if r.chance(Ext::from_f64(0.6)) {
            g.t_bills[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(300.0));
        }
        if seed % 4 == 1 {
            g.bank_debt[e] = Ext::from_f64(0.0);
        }
        if seed % 5 == 2 {
            g.working_capital[e] = f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(9000.0));
        }
        g.tmp_subsid_stock_value = Ext::from_f64(0.0);
        let n = cast::<i32, _>(r.range(0, 40));
        add_options(&mut g, &mut r, n, e);
        let allow = cast::<i32, _>(seed % 3 != 0);
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        cover_negative_cash(&mut port, &mut ui, &mut rng, e, allow);
        let Some(o) = run(COVER_NEGATIVE_CASH, &[&e.to_string(), &allow.to_string()], &g, rs, &[], SKIPS) else {
            return;
        };
        check(&port, &rng, &ui, &o, &format!("seed {seed}{__wsr_text_13775}{e}{__wsr_text_13776}{allow}",__wsr_text_13775=__content.string(13775), __wsr_text_13776=__content.string(13776)));
        if port.bank_debt[e] > g.bank_debt[e] {
            hit(&credit);
        }
        if (11..=1600).contains(&e) && port.working_capital[e] < g.working_capital[e] {
            hit(&wc);
        }
        if (1..=1000).any(|i| port.option_recs[i] != g.option_recs[i]) {
            hit(&opts);
        }
        if (1..=15).any(|s| port.hold_stock_id[(s, e)] != g.hold_stock_id[(s, e)] || port.hold_stock_pct[(s, e)] != g.hold_stock_pct[(s, e)]) {
            hit(&stocks);
        }
    });
    if unavailable().is_none() {
        let c = [&credit, &wc, &opts, &stocks].map(|c| c.load(Ordering::Relaxed));
        eprintln!("cover_negative_cash coverage (credit, wc, options, stocks): {c:?}");
        assert!(c.iter().all(|&n| n > 0), "branch coverage {c:?}");
    }
}

// ------------------------------------------------------------------------------ EtfComplianceCheck

/// Sets fund `f`'s holdings: `n` small ones (market cap 5..50, 1..5%) plus, if `big_mc > 0`, one of
/// 8% with that market cap; all held stocks rated 5+.
fn set_fund_holdings(g: &mut Globals, r: &mut Rand, f: i32, n: i32, big_mc: Ext, big_pct: i16) {
    for slot in 1..=15 {
        g.hold_stock_id[(slot, f)] = 0;
        g.hold_stock_pct[(slot, f)] = 0;
    }
    for slot in 1..=n {
        let stock = cast::<i32, _>(r.range(131, 1580));
        g.hold_stock_id[(slot, f)] = cast::<i16, _>(stock);
        g.hold_stock_pct[(slot, f)] = cast::<i16, _>(r.range(1, 5));
        g.market_cap[stock] = f64r(r, Ext::from_f64(5.0), Ext::from_f64(50.0));
        g.credit_rating_num[stock] = cast::<i32, _>(r.range(5, 10));
        g.entity_status[stock] = 1;
    }
    if big_mc > Ext::from_f64(0.0) {
        let slot = cast::<i32, _>(r.range(1, cast::<i64, _>(n)));
        let stock = cast::<i32, _>(g.hold_stock_id[(slot, f)]);
        g.hold_stock_pct[(slot, f)] = big_pct;
        g.market_cap[stock] = big_mc;
    }
    g.hold_stock_id[(0, f)] = cast::<i16, _>(n);
}

fn compliance_world(seed: u64) -> Globals {
    let mut g = etf_world(seed);
    let mut r = Rand(seed * 17 + 3);
    g.sim_co_count = [200, 200, 500, 900, 1500][cast::<usize, _>(r.range(0, 4))];
    for f in 1581..=1600 {
        if g.entity_status[f] != 1 {
            continue;
        }
        let adv = g.fund_adviser[f];
        if r.chance(Ext::from_f64(0.8)) {
            g.corp_controller[adv] = 0;
        }
        g.credit_rating_num[f] = cast::<i32, _>(r.range(1, 9));
        let case = r.range(0, 7);
        match case {
            0 => {
                // a holding over 50% of the assets
                let mc = f64r(&mut r, Ext::from_f64(300.0), Ext::from_f64(800.0));
                set_fund_holdings(&mut g, &mut r, f, 6, mc, 8);
                g.cash[f] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(300.0));
                g.t_bills[f] = Ext::from_f64(0.0);
            }
            1 => {
                // 20..50%, bonds nearing maturity
                let mc = f64r(&mut r, Ext::from_f64(45.0), Ext::from_f64(80.0));
                set_fund_holdings(&mut g, &mut r, f, 6, mc, 5);
                g.bonds_issued[f] = 1000;
                g.bond_issue_info[f] = st(&format!("{}   {:5.2}", g.cur_year + cast::<i32, _>(r.range(0, 4)), f64r(&mut r, Ext::from(Ext::from_f64(3.0)), Ext::from(Ext::from_f64(9.0)))));
                g.bank_debt[f] = Ext::from_f64(0.0);
                g.cash[f] = f64r(&mut r, Ext::from_f64(450.0), Ext::from_f64(700.0));
                g.t_bills[f] = Ext::from_f64(0.0);
            }
            2 => {
                // 20..50% with bank debt
                let mc = f64r(&mut r, Ext::from_f64(45.0), Ext::from_f64(80.0));
                set_fund_holdings(&mut g, &mut r, f, 6, mc, 5);
                g.bonds_issued[f] = 0;
                g.bank_debt[f] = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(2000.0));
                g.cash[f] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(700.0));
                g.t_bills[f] = Ext::from_f64(0.0);
            }
            3 => {
                // illiquid: stock offering
                set_fund_holdings(&mut g, &mut r, f, 8, Ext::from_f64(0.0), 0);
                g.bonds_issued[f] = 0;
                g.bank_debt[f] = Ext::from_f64(0.0);
                g.cash[f] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(5.0));
                g.t_bills[f] = Ext::from_f64(0.0);
                g.credit_rating_num[f] = cast::<i32, _>(r.range(4, 6));
            }
            4 => {
                // liquid with cheap bonds: buyback
                g.bonds_issued[f] = r.range(100, 1000);
                g.bond_issue_info[f] = st(&format!("{}   {:5.2}", g.cur_year + cast::<i32, _>(r.range(5, 15)), f64r(&mut r, Ext::from(Ext::from_f64(3.0)), Ext::from(Ext::from_f64(9.0)))));
                g.bond_price[f] = wsr_core::pb::numeric::Cur::from_f64(f64r(&mut r, Ext::from_f64(50.0), Ext::from_f64(89.0)));
                g.bank_debt[f] = Ext::from_f64(0.0);
                g.cash[f] = f64r(&mut r, Ext::from_f64(3000.0), Ext::from_f64(9000.0));
                g.credit_rating_num[f] = cast::<i32, _>(r.range(3, 9));
            }
            5 => {
                // over-leveraged: the 300% rule
                g.bank_debt[f] = f64r(&mut r, Ext::from_f64(2000.0), Ext::from_f64(30000.0));
                g.cash[f] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(300.0));
                g.t_bills[f] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(300.0));
                g.credit_rating_num[f] = cast::<i32, _>(r.range(2, 9));
            }
            _ => {
                let n = cast::<i32, _>(g.hold_stock_id[(0, f)]);
                if n > 0 && r.chance(Ext::from_f64(0.5)) {
                    let s = cast::<i32, _>(r.range(1, cast::<i64, _>(n)));
                    g.hold_stock_pct[(s, f)] = cast::<i16, _>(r.range(11, 30));
                }
                if n > 0 && r.chance(Ext::from_f64(0.4)) {
                    let s = cast::<i32, _>(r.range(1, cast::<i64, _>(n)));
                    let stock = cast::<i32, _>(g.hold_stock_id[(s, f)]);
                    if r.chance(Ext::from_f64(0.5)) {
                        g.credit_rating_num[stock] = cast::<i32, _>(r.range(0, 1));
                    } else if r.chance(Ext::from_f64(0.5)) {
                        g.asbestos_corp = if r.chance(Ext::from_f64(0.5)) { stock } else { -stock };
                    } else {
                        g.superfund_corp = cast::<i16, _>(stock);
                    }
                }
            }
        }
    }
    for i in 1..=10 {
        g.recent_offerings[i] = if r.chance(Ext::from_f64(0.3)) { cast::<i32, _>(r.range(131, 1600)) } else { 0 };
    }
    let n = cast::<i32, _>(r.range(0, 20));
    add_options(&mut g, &mut r, n, 1590);
    g
}

#[test]
fn diff_etf_compliance_check() {
    let __content = wsr_core::content::current();
    let pats = [__content.string(13777), __content.string(13778), __content.string(13779), __content.string(13780), __content.string(13781)];
    let counters: Vec<AtomicUsize> = (0..pats.len() + 3).map(|_| AtomicUsize::new(0)).collect();
    par_seeds(seeds(1..=60), |seed| {
        let g = compliance_world(seed);
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        etf_compliance_check(&mut port, &mut ui, &mut rng);
        let Some(o) = run(ETF_COMPLIANCE, &[], &g, rs, &[], SKIPS) else {
            return;
        };
        check(&port, &rng, &ui, &o, &format!("seed {seed}"));
        for (k, p) in pats.iter().enumerate() {
            if count(&ui, p) > 0 {
                hit(&counters[k]);
            }
        }
        let n = pats.len();
        if (1581..=1600).any(|f| port.bonds_issued[f] < g.bonds_issued[f]) {
            hit(&counters[n]); // bond buyback
        }
        if (1581..=1600).any(|f| g.cash[f] <= Ext::from_f64(5.0) && port.cash[f] > g.cash[f] + Ext::from_f64(1.0)) {
            hit(&counters[n + 1]); // stock offering
        }
        if ui.news.iter().any(|h| h.contains("sells") || h.contains("sell")) {
            hit(&counters[n + 2]);
        }
    });
    if unavailable().is_none() {
        let c: Vec<usize> = counters.iter().map(|c| c.load(Ordering::Relaxed)).collect();
        eprintln!("etf_compliance_check coverage {pats:?} + buyback, offering, sales: {c:?}");
        assert!(c.iter().all(|&n| n > 0), "branch coverage {c:?}");
    }
}

// -------------------------------------------------------------------------------- EtfAdviserReview

/// Sets the close of series `s` 60 months ago so that the 5-year ratio is `ratio`.
fn set_ratio(g: &mut Globals, s: i32, ratio: Ext) {
    let cur = cast::<i32, _>(g.cap_hist_close[(0, s)]);
    let mut old = cur + 1;
    if old > 60 {
        old = 1;
    }
    g.cap_hist_close[(old, s)] = g.cap_hist_close[(cur, s)] / ratio;
}

#[test]
fn diff_etf_adviser_review() {
    let __content = wsr_core::content::current();
    // the "overpaid fund adviser" headline is overwritten by the dismissal headline before it is posted
    let pats = [__content.string(13782), "After 3 years of poor", __content.string(13783), __content.string(13784), __content.string(13785), __content.string(13786)];
    let counters: Vec<AtomicUsize> = (0..pats.len() + 3).map(|_| AtomicUsize::new(0)).collect();
    par_seeds(seeds(1..=120), |seed| {
        let mut g = etf_world(seed + 300);
        let mut r = Rand(seed * 7 + 11);
        g.cur_quarter = if seed % 15 == 0 { 3 } else { 4 };
        let etf = cast::<i32, _>(r.range(1586, 1600));
        g.sim_co_count = etf;
        g.entity_status[etf] = 1;
        g.industry_num[etf] = 71;
        g.ctrl_since_year[etf] = g.cur_year - cast::<i32, _>(r.range(2, 9));
        let adv = g.fund_adviser[etf];
        g.entity_status[adv] = if r.chance(Ext::from_f64(0.95)) { 1 } else { 2 };
        g.corp_controller[adv] = if r.chance(Ext::from_f64(0.6)) { cast::<i32, _>(r.range(11, 1600)) } else { cast::<i32, _>(r.range(0, 4)) };
        g.fund_fee_pct[etf] = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.1), Ext::from_f64(1.2)));
        let (er, ir) = match seed % 6 {
            0 => (f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(0.95)), f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(1.6))),
            1 => (f64r(&mut r, Ext::from_f64(1.34), Ext::from_f64(3.0)), f64r(&mut r, Ext::from_f64(0.6), Ext::from_f64(1.5))),
            2 => {
                // overpaid adviser
                g.fund_fee_pct[etf] = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.75), Ext::from_f64(1.2)));
                g.ctrl_since_year[etf] = g.cur_year - cast::<i32, _>(r.range(6, 9));
                (f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(0.95)), f64r(&mut r, Ext::from_f64(0.6), Ext::from_f64(1.05)))
            }
            _ => (f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(2.5)), f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(2.0))),
        };
        set_ratio(&mut g, 0, ir);
        set_ratio(&mut g, etf, er);
        if seed % 4 == 0 {
            // a steady rise: positive trends and 1-year change
            let cur = cast::<i32, _>(g.cap_hist_close[(0, etf)]);
            let v0 = f64r(&mut r, Ext::from_f64(20.0), Ext::from_f64(200.0));
            for k in 0..60 {
                let mut slot = cur - k;
                if slot < 1 {
                    slot += 60;
                }
                g.cap_hist_close[(slot, etf)] = v0 * (Ext::from_f64(1.0) - Ext::from_f64(0.012) * cast::<Ext, _>(k));
            }
        }
        if seed % 6 == 3 {
            // a 5-year decline (3-year down trend), adviser in place for 4 years
            let cur = cast::<i32, _>(g.cap_hist_close[(0, etf)]);
            let v0 = f64r(&mut r, Ext::from_f64(20.0), Ext::from_f64(200.0));
            for k in 0..60 {
                let mut slot = cur - k;
                if slot < 1 {
                    slot += 60;
                }
                g.cap_hist_close[(slot, etf)] = v0 * (Ext::from_f64(1.0) + Ext::from_f64(0.01) * cast::<Ext, _>(k));
            }
            g.ctrl_since_year[etf] = g.cur_year - 4;
        }
        if seed % 5 == 0 {
            g.sel_entity = etf;
        }
        if seed % 7 == 0 {
            g.bank_debt[etf] = Ext::from_f64(0.0);
            g.bonds_issued[etf] = 0;
        }
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        etf_adviser_review(&mut port, &mut ui, &mut rng);
        let Some(o) = run(ETF_ADVISER_REVIEW, &[], &g, rs, &[], SKIPS) else {
            return;
        };
        check(&port, &rng, &ui, &o, &format!("seed {seed} etf {etf}"));
        for (k, p) in pats.iter().enumerate() {
            if count(&ui, p) > 0 {
                hit(&counters[k]);
            }
        }
        let n = pats.len();
        if port.fund_fee_pct[etf] < g.fund_fee_pct[etf] {
            hit(&counters[n]); // fee cut
        }
        if count(&ui, "After 5 years of poor") > 0 && count(&ui, __content.string(13782)) == 0 {
            hit(&counters[n + 2]); // overpaid adviser
        }
        if !ui.msgs.is_empty() {
            hit(&counters[n + 1]); // pop-up
        }
    });
    if unavailable().is_none() {
        let c: Vec<usize> = counters.iter().map(|c| c.load(Ordering::Relaxed)).collect();
        eprintln!("etf_adviser_review coverage {pats:?} + fee cut, popup, overpaid: {c:?}");
        assert!(c.iter().all(|&n| n > 0), "branch coverage {c:?}");
    }
}

// --------------------------------------------------------------------------------- AiEtfInvestCash

#[test]
fn diff_ai_etf_invest_cash() {
    let bought = AtomicUsize::new(0);
    par_seeds(seeds(1..=30), |seed| {
        let mut g = etf_world(seed + 600);
        let mut r = Rand(seed * 19 + 1);
        for f in 1586..=1600 {
            if r.chance(Ext::from_f64(0.6)) {
                g.bank_debt[f] = Ext::from_f64(0.0);
                g.bonds_issued[f] = 0;
                g.cash[f] = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(20000.0));
            }
            if r.chance(Ext::from_f64(0.3)) {
                g.extraordinary_pl[f] = r.range(-100, 300);
                g.accrued_pl[f] = f64r(&mut r, -Ext::from_f64(100.0), Ext::from_f64(300.0));
            }
        }
        g.futures_net_value = Ext::from_f64(0.0);
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        ai_etf_invest_cash(&mut port, &mut ui, &mut rng, 0);
        let Some(o) = run_hooked(AI_ETF_INVEST, &[], &g, rs, &[], SKIPS, &[BUY_STOCK]) else {
            return;
        };
        check(&port, &rng, &ui, &o, &format!("seed {seed}"));
        if o.log.iter().any(|l| l.starts_with("CALL BuyStock(")) {
            hit(&bought);
        }
    });
    if unavailable().is_none() {
        assert!(bought.load(Ordering::Relaxed) > 0, "no purchase");
    }
}

// ------------------------------------------------------------------- EtfLiquidateBondsOnDisqualify

#[test]
fn diff_etf_liquidate_bonds_on_disqualify() {
    let __content = wsr_core::content::current();
    par_seeds(seeds(1..=12), |seed| {
        let g = etf_world(seed + 900);
        let f = 1580 + cast::<i32, _>(seed % 4);
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        etf_liquidate_bonds_on_disqualify(&mut port, &mut ui, &mut rng, f);
        let Some(o) = run(ETF_LIQ_BONDS, &[&f.to_string()], &g, rs, &[], SKIPS) else {
            return;
        };
        check(&port, &rng, &ui, &o, &format!("seed {seed}{__wsr_text_13787}{f}",__wsr_text_13787=__content.string(13787)));
    });
}

// ------------------------------------------------------------------------------- ReviveDefunctEtfs

#[test]
fn diff_revive_defunct_etfs() {
    let __content = wsr_core::content::current();
    let pats = [__content.string(13788), __content.string(13789), __content.string(13790)];
    let counters: Vec<AtomicUsize> = (0..pats.len() + 1).map(|_| AtomicUsize::new(0)).collect();
    par_seeds(seeds(1..=40), |seed| {
        let mut g = etf_world(seed + 1200);
        let mut r = Rand(seed * 23 + 9);
        for f in 1581..=1600 {
            if g.entity_status[f] != 1 {
                g.entity_status[f] = 1;
                g.industry_num[f] = 71;
            }
        }
        let f = cast::<i32, _>(r.range(1586, 1600));
        match seed % 4 {
            0 => {
                let d = [1581, 1582, 1583, 1584, 1590][cast::<usize, _>(r.range(0, 4))];
                g.entity_status[d] = 2;
            }
            _ => {
                // fully public and unlevered
                for e in 1..=1600 {
                    for s in 1..=15 {
                        if cast::<i32, _>(g.hold_stock_id[(s, e)]) == f {
                            g.hold_stock_id[(s, e)] = 0;
                            g.hold_stock_pct[(s, e)] = 0;
                        }
                    }
                }
                g.bank_debt[f] = Ext::from_f64(0.0);
                g.bonds_issued[f] = 0;
                if seed % 4 == 1 {
                    g.industry_num[f] = 70;
                    if r.chance(Ext::from_f64(0.5)) {
                        g.player_ceo_of[1] = f;
                    }
                    g.market_cap[f] = f64r(&mut r, Ext::from_f64(10.0), Ext::from_f64(200.0));
                } else {
                    g.market_cap[f] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(14.0));
                    let cur = cast::<i32, _>(g.cap_hist_close[(0, f)]);
                    for k in 0..60 {
                        let mut slot = cur - k;
                        if slot < 1 {
                            slot += 60;
                        }
                        g.cap_hist_close[(slot, f)] = Ext::from_f64(10.0) * (Ext::from_f64(1.0) + Ext::from_f64(0.01) * cast::<Ext, _>(k));
                    }
                }
            }
        }
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        revive_defunct_etfs(&mut port, &mut ui, &mut rng, 0);
        let Some(o) = run(REVIVE_ETFS, &[], &g, rs, &[], SKIPS) else {
            return;
        };
        check(&port, &rng, &ui, &o, &format!("seed {seed}{__wsr_text_13787}{f}",__wsr_text_13787=__content.string(13787)));
        for (k, p) in pats.iter().enumerate() {
            if count(&ui, p) > 0 {
                hit(&counters[k]);
            }
        }
        if (1581..=1600).any(|e| port.industry_num[e] == 71 && g.industry_num[e] == 70) {
            hit(&counters[pats.len()]);
        }
    });
    if unavailable().is_none() {
        let c: Vec<usize> = counters.iter().map(|c| c.load(Ordering::Relaxed)).collect();
        eprintln!("revive_defunct_etfs coverage {pats:?} + readmitted: {c:?}");
        assert!(c.iter().all(|&n| n > 0), "branch coverage {c:?}");
    }
}
