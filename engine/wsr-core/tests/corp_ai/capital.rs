//! Differential tests of `logic::corp_ai::capital`: every routine on random plausible worlds against
//! the original (BuyStock runs on both sides; its calls are hooked for the coverage counts).
#![allow(unused_imports)]
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use wsr_core::logic::corp_ai::capital::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::pb::numeric::Cur;
use wsr_core::state::Globals;

const SPLITS: u32 = 0x4afc92;
const RECAP: u32 = 0x4b8da9;
const BUYS: u32 = 0x4bd8b4;
const REVISE: u32 = 0x4b8397;
const BOND_ISSUE: u32 = 0x4f77b2;

/// BuyStock (hooked for the coverage counts).
const BUY_STOCK: u32 = 0x4efe60;
/// UpdateStreamingQuotes (hooked to see whether the watch-list refresh ran).
const STREAM_QUOTES: u32 = 0x5dc70b;

/// What a check saw: the original's log, the port's state and Ui.
struct Seen {
    log: Vec<String>,
    port: Globals,
    ui: Rec,
}

impl Seen {
    fn logged(&self, pat: &str) -> bool {
        self.log.iter().any(|l| l.contains(pat))
    }
    fn news(&self, pat: &str) -> bool {
        self.ui.news.iter().any(|n| n.contains(pat)) || self.port.news_msg.to_display().contains(pat)
    }
}

/// Runs the original at `addr` and the port on `g` with RND state `rng0` and MSGBOX `answers`
/// (also fed to the port's Ui); compares the state, the RND state and the number of message boxes.
#[allow(clippy::too_many_arguments)]
fn check(
    addr: u32,
    args: &[&str],
    g: &Globals,
    rng0: u32,
    answers: &[&str],
    skips: &[(u32, u32)],
    hooks: &[u32],
    port: impl FnOnce(&mut Globals, &mut Rec, &mut Rng),
) -> Option<Seen> {
    let opts = Opts {
        args: args.iter().map(|a| a.to_string()).collect(),
        rng: Some(rng0),
        answers,
        skips: skips.to_vec(),
        hooks: hooks.to_vec(),
        ..Opts::default()
    };
    let o = run_original_with(addr, &opts, g)?;
    let mut p = g.clone();
    let mut ui = Rec::default();
    ui.answers = answers.iter().map(|a| a.parse().unwrap()).collect();
    let mut rng = Rng::from_state(rng0);
    port(&mut p, &mut ui, &mut rng);
    cmp().assert(&p, &o.globals);
    assert_eq!(rng.state(), o.rng, "RND state (0x{addr:x})");
    assert_eq!(ui.msgs.len(), orig_msgs(&o), "message boxes (0x{addr:x}): {:?} / {:?}", ui.msgs, o.log);
    Some(Seen { log: o.log, port: p, ui })
}

fn bump(n: &AtomicUsize, hit: bool) {
    if hit {
        n.fetch_add(1, Ordering::Relaxed);
    }
}

/// Prints the coverage counts and fails if one was not reached while the harness ran.
fn report(counts: &[(&str, &AtomicUsize)]) {
    let __content = wsr_core::content::current();
    let mut missed = Vec::new();
    for (what, n) in counts {
        let k = n.load(Ordering::Relaxed);
        eprintln!("{__wsr_text_13574}{what}: {k}",__wsr_text_13574=__content.string(13574));
        if k == 0 {
            missed.push(*what);
        }
    }
    if unavailable().is_none() {
        assert!(missed.is_empty(), "no seed reached: {missed:?}");
    }
}

fn seed_rng(seed: u64) -> u32 {
    cast::<u32, _>(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 17)
}

#[test]
fn diff_process_stock_splits_all() {
    let __content = wsr_core::content::current();
    static SPLIT: AtomicUsize = AtomicUsize::new(0);
    static REVERSE: AtomicUsize = AtomicUsize::new(0);
    static ROUNDED: AtomicUsize = AtomicUsize::new(0);
    static WATCHED: AtomicUsize = AtomicUsize::new(0);
    static FULL: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=40, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed ^ 0x5911);
        let high = seed % 4 == 0;
        g.million_prefix_uc = st(if r.chance(Ext::from_f64(0.5)) { "M" } else { "B" });
        for c in 11..=1600 {
            // prices: mostly cheap so the scan runs on; some seeds with a few split candidates
            let price = if high && r.chance(Ext::from_f64(0.02)) {
                f64r(&mut r, Ext::from_f64(50.0), Ext::from_f64(400.0))
            } else if r.chance(Ext::from_f64(0.3)) {
                f64r(&mut r, Ext::from_f64(0.1), Ext::from_f64(15.0))
            } else {
                f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(74.0))
            };
            g.market_cap[c] = [f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(12.0)), f64r(&mut r, Ext::from_f64(15.0), Ext::from_f64(90.0)), f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(5000.0))]
                [cast::<usize, _>(r.range(0, 2))];
            g.shares_outstanding[c] = if r.chance(Ext::from_f64(0.02)) { f64r(&mut r, Ext::from_f64(0.0001), Ext::from_f64(0.02)) } else { g.market_cap[c] / price };
            if r.chance(Ext::from_f64(0.3)) {
                // already a round count, or close to one
                g.shares_outstanding[c] = [Ext::from_f64(25.0), Ext::from_f64(50.0), Ext::from_f64(10.0), Ext::from_f64(1.0), Ext::from_f64(0.5), Ext::from_f64(2.5), Ext::from_f64(3000.0)][cast::<usize, _>(r.range(0, 6))];
            }
            if r.chance(Ext::from_f64(0.02)) {
                g.industry_num[c] = 71;
            }
            if !high && g.market_cap[c] / g.shares_outstanding[c] > Ext::from_f64(74.0) {
                g.shares_outstanding[c] = g.market_cap[c] / f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(74.0));
            }
        }
        for (k, sym) in ["BKH", "GOOG", "AAPL", "AMZN"].iter().enumerate() {
            let c = 200 + cast::<i32, _>(k) * 7;
            g.ent_symbol[c] = st(sym);
            g.market_cap[c] = Ext::from_f64(1000.0);
            g.shares_outstanding[c] = Ext::from_f64(1000.0) / f64r(&mut r, Ext::from_f64(500.0), Ext::from_f64(3000.0));
        }
        // human-managed corps
        for _ in 0..5 {
            let c = cast::<i32, _>(r.range(11, 1600));
            g.corp_controller[c] = 1;
            g.auto_pilot[c] = 0;
            g.shares_outstanding[c] = g.market_cap[c] / f64r(&mut r, Ext::from_f64(3.5), if high { Ext::from_f64(300.0) } else { Ext::from_f64(74.0) });
        }
        g.sel_entity = cast::<i32, _>(r.range(11, 1600));
        let cp = g.cur_player;
        for i in 1..=15 {
            g.watch_list[(i, cp)] = if r.chance(Ext::from_f64(0.6)) { cast::<i32, _>(r.range(11, 1600)) } else { 0 };
        }
        if r.chance(Ext::from_f64(0.3)) {
            g.news_headline[50] = st(__content.string(14423));
        }
        let Some(s) = check(SPLITS, &[], &g, seed_rng(seed), &[], &[], &[STREAM_QUOTES], |p, ui, rng| {
            process_stock_splits_all(p, ui, rng)
        }) else {
            return;
        };
        bump(&SPLIT, s.news(__content.string(2796)));
        bump(&FULL, !s.news(__content.string(2796)));
        bump(&REVERSE, s.news(__content.string(13630)));
        bump(&WATCHED, s.logged("CALL UpdateStreamingQuotes"));
        bump(&ROUNDED, (11..=1600).any(|c| s.port.shares_outstanding[c] == Ext::from_f64(50.0) && g.shares_outstanding[c] != Ext::from_f64(50.0)));
    });
    report(&[
        ("splits: split", &SPLIT),
        ("splits: reverse split", &REVERSE),
        ("splits: ladder rounding", &ROUNDED),
        ("splits: watch-list refresh", &WATCHED),
        ("splits: no split (full scan)", &FULL),
    ]);
}

/// A corp with a bond issue that qualifies for a recapitalisation (most of the time).
fn recap_world(seed: u64) -> (Globals, i32, Vec<&'static str>) {
    let mut g = world(seed);
    let mut r = Rand(seed ^ 0x3ec4);
    // the other players are computer players, except sometimes one
    for p in 2..=5 {
        g.is_computer_player[p] = if r.chance(Ext::from_f64(0.1)) { 0 } else { 1 };
    }
    let c = cast::<i32, _>(r.range(131, 1580));
    g.entity_status[c] = 1;
    g.industry_num[c] = cast::<i16, _>(r.range(3, 68));
    g.corp_controller[c] = if r.chance(Ext::from_f64(0.8)) { cast::<i32, _>(r.range(11, 1600)) } else { cast::<i32, _>(r.range(1, 5)) };
    g.bonds_issued[c] = if r.chance(Ext::from_f64(0.95)) { r.range(25, 3000) } else { r.range(1, 24) };
    let mat = g.cur_year + cast::<i32, _>(r.range(1, 20));
    let coupon = if r.chance(Ext::from_f64(0.85)) { f32r(&mut r, Ext::from_f64(3.0), Ext::from_f64(10.6)) } else { f32r(&mut r, Ext::from_f64(10.0), Ext::from_f64(14.0)) };
    let cv = if r.chance(Ext::from_f64(0.25)) { format!("cv|{}|", r.range(10, 4000)) } else { String::new() };
    g.bond_issue_info[c] = st(&format!("{mat}{cv}@{coupon:5.2}"));
    g.bond_price[c] = Cur::from_f64(f64r(&mut r, Ext::from_f64(30.0), Ext::from_f64(110.0)));
    g.credit_rating_num[c] = cast::<i32, _>(r.range(0, 4));
    g.cash[c] = f64r(&mut r, -Ext::from_f64(100.0), Ext::from_f64(300.0));
    g.t_bills[c] = Ext::from_f64(0.0);
    // a weak balance sheet (CalcEntityFinancials re-rates the credit)
    for k in 0..=15 {
        g.hold_stock_id[(k, c)] = 0;
        g.hold_stock_pct[(k, c)] = 0;
    }
    g.business_assets[c] = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(3000.0));
    g.working_capital[c] = f64r(&mut r, -Ext::from_f64(200.0), Ext::from_f64(200.0));
    g.bank_debt[c] = g.business_assets[c] * f64r(&mut r, Ext::from_f64(0.3), Ext::from_f64(1.0));
    g.earnings_y1[c] = f64r(&mut r, -Ext::from_f64(100.0), Ext::from_f64(20.0));
    g.earnings_ytd[c] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(10.0));
    g.bonds_public_held = if r.chance(Ext::from_f64(0.5)) { 0 } else { r.range(1, 1000) };
    // bondholders: banks, insurers, funds, players
    for _ in 0..r.range(1, 8) {
        let h = match r.range(0, 3) {
            0 => 1,
            1 => cast::<i32, _>(r.range(1581, 1583)),
            _ => cast::<i32, _>(r.range(11, 130)),
        };
        g.entity_status[h] = 1;
        let slot = cast::<i32, _>(r.range(1, 15));
        g.hold_bond_id[(slot, h)] = c;
        g.hold_bond_face[(slot, h)] = if r.chance(Ext::from_f64(0.2)) { 1 } else { r.range(1, g.bonds_issued[c].max(1)) };
        g.bond_hold_cost[(slot, h)] = cast::<Ext, _>(g.hold_bond_face[(slot, h)]);
        g.hold_bond_id[(0, h)] = 15;
    }
    if r.chance(Ext::from_f64(0.3)) {
        // a 1-for-2 swap: coupon up to 8, negative net worth with 2..3x of it in bonds
        let coupon = f32r(&mut r, Ext::from_f64(3.0), Ext::from_f64(7.9));
        g.bond_issue_info[c] = st(&format!("{mat}@{coupon:5.2}"));
        g.bank_debt[c] = g.business_assets[c] * Ext::from_f64(1.5);
        let mut p = g.clone();
        let mut ui = Rec::default();
        let mut rng = Rng::from_state(1);
        for _ in 0..4 {
            wsr_core::logic::finance::calc_entity_financials(&mut p, &mut ui, &mut rng, c);
            let nw = p.net_worth;
            if nw < Ext::from_f64(0.0) {
                let other = nw + cast::<Ext, _>(p.bonds_issued[c]);
                // bonds = 2.5 x -(net worth without the bonds' own effect)
                let b = cast::<i64, _>((-(other) * Ext::from_f64(2.5) / Ext::from_f64(3.5)).max(Ext::from_f64(25.0)));
                p.bonds_issued[c] = b;
            }
        }
        g.bonds_issued[c] = p.bonds_issued[c];
    }
    let answers = vec![if r.chance(Ext::from_f64(0.5)) { "6" } else { "7" }];
    (g, c, answers)
}

#[test]
fn diff_ai_bond_recapitalization() {
    let __content = wsr_core::content::current();
    static ASKED: AtomicUsize = AtomicUsize::new(0);
    static REJECTED: AtomicUsize = AtomicUsize::new(0);
    static ONE_FOR_TWO: AtomicUsize = AtomicUsize::new(0);
    static TWO_FOR_THREE: AtomicUsize = AtomicUsize::new(0);
    static PAID_OUT: AtomicUsize = AtomicUsize::new(0);
    static CV: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=120, |seed| {
        let (g, c, answers) = recap_world(seed);
        let a = c.to_string();
        let Some(s) = check(RECAP, &[&a], &g, seed_rng(seed), &answers, &[], &[], |p, ui, rng| {
            ai_bond_recapitalization(p, ui, rng, c)
        }) else {
            return;
        };
        bump(&ASKED, s.logged("MSGBOX"));
        bump(&REJECTED, s.news(__content.string(13631)));
        bump(&ONE_FOR_TWO, s.news("agree to 1-for-2"));
        bump(&TWO_FOR_THREE, s.news("agree to 2-for-3"));
        bump(&CV, s.news(__content.string(13632)) && s.port.bond_issue_info[c].to_display().contains("cv|"));
        bump(
            &PAID_OUT,
            s.news(__content.string(13632))
                && (1..=1583).any(|h| (1..=15).any(|k| g.hold_bond_id[(k, h)] == c && g.hold_bond_face[(k, h)] == 1)),
        );
    });
    report(&[
        ("recap: player asked", &ASKED),
        ("recap: rejected", &REJECTED),
        ("recap: 1-for-2", &ONE_FOR_TWO),
        ("recap: 2-for-3", &TWO_FOR_THREE),
        ("recap: convertible", &CV),
        ("recap: holding paid out", &PAID_OUT),
    ]);
}

#[test]
fn diff_ai_corp_buys_stock() {
    let __content = wsr_core::content::current();
    static BUY: AtomicUsize = AtomicUsize::new(0);
    static DIV: AtomicUsize = AtomicUsize::new(0);
    static DIV_SET: AtomicUsize = AtomicUsize::new(0);
    static ASKED: AtomicUsize = AtomicUsize::new(0);
    static CANDIDATE: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=80, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed ^ 0xb075);
        g.sim_co_count = cast::<i32, _>(r.range(11, 1600));
        g.monetary_stance = cast::<i32, _>(r.range(-1, 1));
        g.takeover_candidate_id = if r.chance(Ext::from_f64(0.5)) { cast::<i32, _>(r.range(131, 1580)) } else { 0 };
        g.cpu_max_corps = cast::<i32, _>(r.range(1, 30));
        for p in 1..=5 {
            g.player_corps_controlled[p] = cast::<i32, _>(r.range(0, 30));
        }
        for c in 86..=1600 {
            if r.chance(Ext::from_f64(0.6)) {
                g.cash[c] = f64r(&mut r, Ext::from_f64(20000.0), Ext::from_f64(200000.0));
                g.credit_rating_num[c] = cast::<i32, _>(r.range(6, 10));
                g.tax_loss_carry[c] = Ext::from_f64(0.0);
                if r.chance(Ext::from_f64(0.7)) {
                    g.earnings_y1[c] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(100.0));
                }
                if r.chance(Ext::from_f64(0.5)) {
                    // few holdings
                    let n = r.range(0, 3);
                    for k in 1..=15 {
                        g.hold_stock_id[(k, c)] = 0;
                        g.hold_stock_pct[(k, c)] = 0;
                    }
                    for k in 1..=cast::<i32, _>(n) {
                        g.hold_stock_id[(k, c)] = cast::<i16, _>(r.range(131, 1580));
                        g.hold_stock_pct[(k, c)] = if r.chance(Ext::from_f64(0.2)) { 100 } else { cast::<i16, _>(r.range(1, 40)) };
                    }
                    g.hold_stock_id[(0, c)] = cast::<i16, _>(n);
                }
                if r.chance(Ext::from_f64(0.2)) {
                    g.dividend_per_share[c] = Ext::from_f64(0.0);
                }
                if r.chance(Ext::from_f64(0.05)) {
                    g.corp_controller[c] = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
                    g.auto_pilot[c] = 1;
                }
            }
        }
        if r.chance(Ext::from_f64(0.3)) {
            g.sel_entity = cast::<i32, _>(r.range(86, 1600));
        }
        if r.chance(Ext::from_f64(0.3)) {
            g.news_headline[40] = st(__content.string(14424));
        }
        let Some(s) = check(BUYS, &[], &g, seed_rng(seed), &["7", "7"], &[], &[BUY_STOCK], |p, ui, rng| {
            ai_corp_buys_stock(p, ui, rng, 0)
        }) else {
            return;
        };
        bump(&BUY, s.logged("CALL BuyStock("));
        bump(&CANDIDATE, s.logged("CALL BuyStock(") && g.takeover_candidate_id != 0 && s.port.takeover_candidate_id == 0);
        bump(&DIV, s.news(__content.string(13633)));
        bump(&ASKED, s.logged("MSGBOX"));
        bump(&DIV_SET, (86..=1600).any(|c| g.dividend_per_share[c] == Ext::from_f64(0.0) && s.port.dividend_per_share[c] != Ext::from_f64(0.0)));
    });
    report(&[
        ("buys: BuyStock", &BUY),
        ("buys: takeover candidate bought", &CANDIDATE),
        ("buys: dividend raise", &DIV),
        ("buys: dividend started", &DIV_SET),
        ("buys: autopilot permission asked", &ASKED),
    ]);
}

#[test]
fn diff_revise_earnings_estimates() {
    let __content = wsr_core::content::current();
    static RAISE: AtomicUsize = AtomicUsize::new(0);
    static LOWER: AtomicUsize = AtomicUsize::new(0);
    static GUIDANCE: AtomicUsize = AtomicUsize::new(0);
    static TARGET: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=64, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed ^ 0x4e7e);
        g.sim_co_count = cast::<i32, _>(if r.chance(Ext::from_f64(0.8)) { r.range(86, 1600) } else { r.range(11, 85) });
        g.rand_seed = cast::<i32, _>(r.range(0, 32000));
        g.news_msg = if r.chance(Ext::from_f64(0.2)) { st(__content.string(13571)) } else { PbStr::new() };
        // few active corps (the original runs CalcProjectedEarnings for each of them)
        for c in 11..=1600 {
            if r.chance(Ext::from_f64(0.92)) {
                g.entity_status[c] = 0;
            }
        }
        for c in 131..=1580 {
            if g.entity_status[c] == 1 && r.chance(Ext::from_f64(0.3)) {
                g.industry_num[c] = 37;
            }
        }
        let picks: Vec<i32> = (0..3).map(|_| cast::<i32, _>(r.range(131, 1580))).collect();
        g.active_entity = picks[0];
        g.sel_entity = picks[1];
        g.last_sel_entity = picks[2];
        for &c in &picks {
            g.entity_status[c] = 1;
            g.shares_outstanding[c] = f64r(&mut r, Ext::from_f64(0.02), Ext::from_f64(0.4));
            g.earnings_y1[c] = f64r(&mut r, Ext::from_f64(20.0), Ext::from_f64(200.0));
            g.earnings_ytd[c] = f64r(&mut r, Ext::from_f64(10.0), Ext::from_f64(100.0));
            g.industry_num[c] = cast::<i16, _>(r.range(3, 68));
            // a public float
            for h in [1, 11, 86] {
                for k in 1..=15 {
                    if cast::<i32, _>(g.hold_stock_id[(k, h)]) == c {
                        g.hold_stock_id[(k, h)] = 0;
                    }
                }
            }
        }
        // the watched corps' projection jumps (the new value comes from CalcProjectedEarnings)
        let mut probe = g.clone();
        let mut ui = Rec::default();
        let mut rng = Rng::from_state(1);
        for &c in &picks {
            let p = wsr_core::logic::earnings::calc_projected_earnings(&mut probe, &mut ui, &mut rng, c);
            g.proj_qtr_earnings[c] = if r.chance(Ext::from_f64(0.5)) { p * Ext::from_f64(0.5) } else { p * Ext::from_f64(2.0) };
            if g.proj_qtr_earnings[c] == Ext::from_f64(0.0) {
                g.proj_qtr_earnings[c] = Ext::from_f64(1.0);
            }
        }
        let Some(s) = check(REVISE, &[], &g, seed_rng(seed), &[], &[], &[], |p, ui, rng| {
            revise_earnings_estimates(p, ui, rng)
        }) else {
            return;
        };
        bump(&RAISE, ["raises", "boosts", "lifts", "hikes", "ups"].iter().any(|v| s.news(v)));
        bump(&LOWER, ["lowers", "cuts", "slashes", "reduces"].iter().any(|v| s.news(v)));
        bump(&GUIDANCE, s.news(__content.string(13634)));
        bump(&TARGET, s.news("earnings target"));
    });
    report(&[
        ("revise: raise", &RAISE),
        ("revise: lower", &LOWER),
        ("revise: guidance", &GUIDANCE),
        ("revise: target", &TARGET),
    ]);
}

#[test]
fn diff_ai_random_bond_issue() {
    static ISSUED: AtomicUsize = AtomicUsize::new(0);
    static FUND: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=200, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed ^ 0xb0d5);
        let c = cast::<i32, _>(if r.chance(Ext::from_f64(0.5)) { r.range(1586, 1600) } else { r.range(131, 1580) });
        g.sim_co_count = c;
        g.entity_status[c] = 1;
        if (1586..=1600).contains(&c) {
            g.industry_num[c] = 71;
        }
        g.bonds_issued[c] = if r.chance(Ext::from_f64(0.9)) { 0 } else { 100 };
        for k in 0..=15 {
            g.hold_stock_id[(k, c)] = 0;
            g.hold_stock_pct[(k, c)] = 0;
        }
        if g.industry_num[c] == 71 {
            // a fund: large stock holdings, little cash
            for k in 1..=10 {
                let h = cast::<i32, _>(r.range(131, 1580));
                g.entity_status[h] = 1;
                g.hold_stock_id[(k, c)] = cast::<i16, _>(h);
                g.hold_stock_pct[(k, c)] = cast::<i16, _>(r.range(5, 30));
                g.market_cap[h] = f64r(&mut r, Ext::from_f64(20.0), Ext::from_f64(150.0));
            }
            g.hold_stock_id[(0, c)] = 10;
        }
        g.base_interest_rate = cast::<f32, _>(if r.chance(Ext::from_f64(0.8)) { f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(5.0)) } else { f32r(&mut r, Ext::from_f64(5.0), Ext::from_f64(10.0)) });
        g.credit_rating_num[c] = cast::<i32, _>(r.range(4, 10));
        g.cash[c] = if g.industry_num[c] == 71 { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(300.0)) } else { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(1000.0)) };
        g.t_bills[c] = Ext::from_f64(0.0);
        g.business_assets[c] = if g.industry_num[c] == 71 { Ext::from_f64(0.0) } else { f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(20000.0)) };
        g.working_capital[c] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(1000.0));
        g.bank_debt[c] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(300.0));
        g.earnings_y1[c] = f64r(&mut r, Ext::from_f64(10.0), Ext::from_f64(300.0));
        g.max_bond_issue = r.range(2000, 50000);
        if r.chance(Ext::from_f64(0.3)) {
            g.corp_controller[c] = 1;
            g.auto_pilot[c] = cast::<i32, _>(r.range(0, 1));
        }
        let Some(s) = check(BOND_ISSUE, &[], &g, seed_rng(seed), &[], &[], &[], |p, ui, rng| {
            ai_random_bond_issue(p, ui, rng)
        }) else {
            return;
        };
        let issued = s.port.bonds_issued[c] > 0 && g.bonds_issued[c] == 0;
        bump(&ISSUED, issued);
        bump(&FUND, issued && g.industry_num[c] == 71);
    });
    report(&[
        ("bond issue: issued", &ISSUED),
        ("bond issue: fund issued", &FUND),
    ]);
}
