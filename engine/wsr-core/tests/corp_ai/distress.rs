//! Differential tests of `logic::corp_ai::distress`: every routine on random plausible worlds, against
//! the original (the trading/restructuring callees run on both sides; their calls are hooked for the
//! coverage counts).
#![allow(unused_imports)]
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use wsr_core::logic::corp_ai::distress::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

const BAILOUT: u32 = 0x4b9ef1;
const DISTRESSED: u32 = 0x4b26e0;
const DIV_CUTS: u32 = 0x4bb0e6;
const LIQ_SUB_RANDOM: u32 = 0x4b30f8;
const BOOST_RIVAL: u32 = 0x494506;
const HOLDCO_MERGER: u32 = 0x4baad0;
const SMALL_HOLDINGS: u32 = 0x4b2114;
const BANK_BUYS_LOAN: u32 = 0x4bc3e7;

/// Callees whose calls are hooked (logged) for the coverage counts.
const LIQ_SUB: u32 = 0x512aa9;
const MERGE: u32 = 0x509293;
const BUY_STOCK: u32 = 0x4efe60;

fn bump(n: &AtomicUsize) {
    n.fetch_add(1, Ordering::Relaxed);
}

fn hits(log: &[String], pat: &str, n: &AtomicUsize) {
    if log.iter().any(|l| l.contains(pat)) {
        bump(n);
    }
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

/// Runs the original at `addr` and the port on `g` with RND state `rng0`; compares the state, RND state
/// and the number of message boxes / headlines. Returns (original, port state, port ui).
fn check(
    addr: u32,
    args: &[&str],
    g: &Globals,
    rng0: u32,
    hooks: &[u32],
    port: impl FnOnce(&mut Globals, &mut Rec, &mut Rng),
) -> Option<(Original, Globals, Rec)> {
    let o = run_hooked(addr, args, g, rng0, &[], &[], hooks)?;
    let mut p = g.clone();
    let mut ui = Rec::default();
    let mut rng = Rng::from_state(rng0);
    port(&mut p, &mut ui, &mut rng);
    cmp().assert(&p, &o.globals);
    assert_eq!(rng.state(), o.rng, "RND state (0x{addr:x})");
    assert_eq!(ui.msgs.len(), orig_msgs(&o), "message boxes (0x{addr:x}): {:?} / {:?}", ui.msgs, o.log);
    Some((o, p, ui))
}

/// An RND state derived from `seed` whose first `RND` is below `p`.
fn low_rng(seed: u64, p: Ext) -> u32 {
    let mut s = seed_rng(seed);
    while Rng::from_state(s).rnd() >= p {
        s = s.wrapping_mul(1_103_515_245).wrapping_add(12345);
    }
    s
}

fn clear_holdings(g: &mut Globals, e: i32) {
    for s in 0..=15 {
        g.hold_stock_id[(s, e)] = 0;
        g.hold_stock_pct[(s, e)] = 0;
        g.hold_stock_basis[(s, e)] = Ext::from_f64(0.0);
    }
}

fn add_holding(g: &mut Globals, holder: i32, slot: i32, corp: i32, pct: i16, basis: Ext) {
    g.hold_stock_id[(slot, holder)] = cast::<i16, _>(corp);
    g.hold_stock_pct[(slot, holder)] = pct;
    g.hold_stock_basis[(slot, holder)] = basis;
    g.hold_stock_id[(0, holder)] = cast::<i16, _>((1..=15).filter(|&s| g.hold_stock_id[(s, holder)] != 0).count());
}

/// A random active operating corp 131..1580 (not a fund, not a holding company).
fn op_corp(g: &mut Globals, r: &mut Rand) -> i32 {
    let c = cast::<i32, _>(r.range(131, 1580));
    g.entity_status[c] = 1;
    g.industry_num[c] = cast::<i16, _>(r.range(3, 68));
    c
}

#[test]
fn diff_bank_bailout_injection() {
    let __content = wsr_core::content::current();
    static INJECT: AtomicUsize = AtomicUsize::new(0);
    static PLACEMENT: AtomicUsize = AtomicUsize::new(0);
    static PUBLIC: AtomicUsize = AtomicUsize::new(0);
    static FAILED: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=40, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed ^ 0xb1);
        g.bailout_max_credit_rating = cast::<i32, _>(r.range(0, 10));
        for k in 1..=10 {
            g.recent_offerings[k] = if r.chance(Ext::from_f64(0.3)) { cast::<i32, _>(r.range(-85, 85)) } else { 0 };
        }
        let mode = seed % 4;
        for b in 11..=85 {
            if r.chance(Ext::from_f64(0.2)) {
                continue;
            }
            g.entity_status[b] = 1;
            g.corp_controller[b] = cast::<i32, _>(r.range(11, 1600));
            g.market_cap[b] = f64r(&mut r, Ext::from_f64(5.0), Ext::from_f64(300.0));
            // remove existing holders of the bank
            for h in 1..=1600 {
                for s in 1..=15 {
                    if cast::<i32, _>(g.hold_stock_id[(s, h)]) == b {
                        g.hold_stock_id[(s, h)] = 0;
                        g.hold_stock_pct[(s, h)] = 0;
                    }
                }
            }
            let h = cast::<i32, _>(r.range(131, 1580));
            g.entity_status[h] = 1;
            if mode == 0 {
                // wholly owned by a rich parent
                g.corp_controller[b] = h;
                g.corp_controller[h] = h;
                g.cash[h] = f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(20000.0));
                g.t_bills[h] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(10000.0));
                clear_holdings(&mut g, h);
                add_holding(&mut g, h, 1, b, 100, f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(1000.0)));
            } else if mode == 1 || mode == 2 {
                // a corp-controlled largest holder (placement when rich)
                g.corp_controller[h] = cast::<i32, _>(r.range(11, 1600));
                if mode == 1 {
                    g.cash[h] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(20000.0));
                    g.t_bills[h] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(20000.0));
                }
                let slot = cast::<i32, _>(r.range(1, 15));
                g.hold_stock_id[(slot, h)] = 0;
                add_holding(&mut g, h, slot, b, cast::<i16, _>(r.range(20, 70)), f64r(&mut r, Ext::from_f64(10.0), Ext::from_f64(500.0)));
                g.corp_controller[b] = h;
            }
        }
        let Some((o, _, ui)) = check(BAILOUT, &[], &g, seed_rng(seed), &[], |p, ui, rng| {
            bank_bailout_injection(p, ui, rng)
        }) else {
            return;
        };
        if ui.news.iter().any(|n| n.contains(__content.string(13635))) {
            bump(&INJECT);
        }
        if (1..=10).any(|k| o.globals.recent_offerings[k] < 0 && g.recent_offerings[k] == 0) {
            bump(&PLACEMENT);
        }
        if (1..=10).any(|k| o.globals.recent_offerings[k] > 0 && g.recent_offerings[k] == 0) {
            bump(&PUBLIC);
        }
        if o.globals.bailout_max_credit_rating > g.bailout_max_credit_rating.max(6) {
            bump(&FAILED);
        }
    });
    report("bailout: capital injection", &INJECT);
    report("bailout: private placement", &PLACEMENT);
    report("bailout: public offering", &PUBLIC);
    report("bailout: 50 failed tries", &FAILED);
}

#[test]
fn diff_ai_buy_distressed_assets() {
    let __content = wsr_core::content::current();
    static SOLD: AtomicUsize = AtomicUsize::new(0);
    static POSTED: AtomicUsize = AtomicUsize::new(0);
    static CLEARED: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=60, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed ^ 0xd1);
        let buyer = op_corp(&mut g, &mut r);
        let mut seller = op_corp(&mut g, &mut r);
        if seller == buyer {
            seller = if buyer < 1500 { buyer + 1 } else { buyer - 1 };
        }
        g.sim_co_count = buyer;
        g.distressed_corp = seller;
        g.entity_status[seller] = if r.chance(Ext::from_f64(0.9)) { 1 } else { 2 };
        if r.chance(Ext::from_f64(0.9)) {
            g.industry_num[seller] = g.industry_num[buyer];
        }
        for c in [buyer, seller] {
            if r.chance(Ext::from_f64(0.8)) {
                g.corp_controller[c] = cast::<i32, _>(r.range(11, 1600));
            }
        }
        g.business_assets[seller] = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(3000.0));
        g.cash[seller] = f64r(&mut r, -Ext::from_f64(100.0), Ext::from_f64(50.0));
        g.t_bills[seller] = Ext::from_f64(0.0);
        g.bank_debt[seller] = g.business_assets[seller] * f64r(&mut r, Ext::from_f64(0.6), Ext::from_f64(1.3));
        g.working_capital[seller] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(100.0));
        g.bonds_issued[seller] = 0;
        clear_holdings(&mut g, seller);
        g.earnings_ytd[seller] = -f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(100.0));
        g.earnings_y1[seller] = -f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(100.0));
        g.cash[buyer] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(5000.0));
        g.t_bills[buyer] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(2000.0));
        g.goodwill[buyer] = f64r(&mut r, -Ext::from_f64(100.0), Ext::from_f64(300.0));
        g.news_msg = if r.chance(Ext::from_f64(0.3)) { st(__content.string(13636)) } else { PbStr::new() };
        let rng0 = if seed % 5 == 0 { seed_rng(seed) } else { low_rng(seed, Ext::from_f64(0.1)) };
        let Some((o, p, ui)) = check(DISTRESSED, &[], &g, rng0, &[], |p, ui, rng| {
            ai_buy_distressed_assets(p, ui, rng)
        }) else {
            return;
        };
        if ui.news.iter().any(|n| n.contains(" sells ")) {
            bump(&SOLD);
        }
        if ui.news.iter().any(|n| n.contains("PENDING")) {
            bump(&POSTED);
        }
        if p.distressed_corp == 0 && o.globals.business_assets[buyer] == g.business_assets[buyer] {
            bump(&CLEARED);
        }
    });
    report("distressed: assets sold", &SOLD);
    report("distressed: pending headline posted", &POSTED);
    report("distressed: inactive corp cleared", &CLEARED);
}

#[test]
fn diff_ai_dividend_cuts_covenants() {
    let __content = wsr_core::content::current();
    static CUT: AtomicUsize = AtomicUsize::new(0);
    static NEWS: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=24, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed ^ 0xc5);
        g.sim_co_count = [1240, 220, 500, 1240, 220, 17][cast::<usize, _>(seed % 6)];
        let (lo, hi) = match g.sim_co_count {
            1240 => (511, 1020),
            220 => (1021, 1600),
            _ => (11, 510),
        };
        for c in lo..=hi {
            if g.entity_status[c] == 1 && g.industry_num[c] != 71 && r.chance(Ext::from_f64(0.12)) {
                let price = g.market_cap[c] / g.shares_outstanding[c];
                g.dividend_per_share[c] = price * f64r(&mut r, Ext::from_f64(0.08), Ext::from_f64(0.3));
                if r.chance(Ext::from_f64(0.4)) {
                    g.corp_controller[c] = 1;
                }
                g.earnings_y1[c] = f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(30.0));
                g.cash[c] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(300.0));
                g.bank_debt[c] = if r.chance(Ext::from_f64(0.5)) { Ext::from_f64(0.0) } else { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(3000.0)) };
            }
        }
        let Some((o, _, ui)) = check(DIV_CUTS, &[], &g, seed_rng(seed), &[], |p, ui, rng| {
            ai_dividend_cuts_covenants(p, ui, rng)
        }) else {
            return;
        };
        if (lo..=hi).any(|c| o.globals.dividend_per_share[c] < g.dividend_per_share[c]) {
            bump(&CUT);
        }
        if ui.news.iter().any(|n| n.contains(__content.string(13637))) {
            bump(&NEWS);
        }
    });
    report("dividend cuts: cut", &CUT);
    report("dividend cuts: headline", &NEWS);
}

#[test]
fn diff_ai_liquidate_random_subsidiary() {
    static LIQ: AtomicUsize = AtomicUsize::new(0);
    static LIQ_HC: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=80, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed ^ 0x1a);
        g.rand_seed = if r.chance(Ext::from_f64(0.5)) { 7 + 37 * cast::<i32, _>(r.range(0, 800)) } else { cast::<i32, _>(r.range(0, 32000)) };
        for c in 131..=1580 {
            if g.industry_num[c] != 71 && r.chance(Ext::from_f64(0.1)) {
                g.industry_num[c] = 70;
            }
        }
        for c in 131..=1580 {
            if g.entity_status[c] != 1 || g.industry_num[c] == 71 {
                continue;
            }
            for s in 1..=15 {
                let sub = cast::<i32, _>(g.hold_stock_id[(s, c)]);
                if sub < 11 || !r.chance(Ext::from_f64(0.6)) {
                    continue;
                }
                g.hold_stock_pct[(s, c)] = 100;
                if r.chance(Ext::from_f64(0.6)) && sub > 130 {
                    g.industry_num[sub] = if r.chance(Ext::from_f64(0.7)) { g.industry_num[c] } else { 70 };
                }
                if r.chance(Ext::from_f64(0.3)) {
                    g.tax_loss_carry[sub] = -f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(1000.0));
                }
            }
        }
        let rng0 = seed_rng(seed);
        let Some((o, _, _)) = check(LIQ_SUB_RANDOM, &[], &g, rng0, &[LIQ_SUB], |p, ui, rng| {
            ai_liquidate_random_subsidiary(p, ui, rng, 0)
        }) else {
            return;
        };
        if o.log.iter().any(|l| l.contains("CALL LiquidateSubIntoParent(")) {
            bump(&LIQ);
            let parent = Rng::from_state(rng0).rnd_range(1600, 131);
            if g.industry_num[parent] == 70 {
                bump(&LIQ_HC);
            }
        }
    });
    report("random liquidation: liquidated", &LIQ);
    report("random liquidation: holding-company parent", &LIQ_HC);
}

#[test]
fn diff_ai_boost_leaders_rival() {
    static OFFER: AtomicUsize = AtomicUsize::new(0);
    static GROWTH: AtomicUsize = AtomicUsize::new(0);
    static REACHED: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=100, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed ^ 0xb0);
        g.difficulty_level = cast::<i32, _>(r.range(1, 4));
        g.has_cpu_players = if (2..=g.num_players).any(|p| g.is_computer_player[p] == 1) && r.chance(Ext::from_f64(0.7)) {
            st("Y")
        } else {
            st("N")
        };
        g.last_rival_offering_year = if r.chance(Ext::from_f64(0.15)) { cast::<i16, _>(g.cur_year) } else { cast::<i16, _>(g.cur_year - 1) };
        for p in 1..=g.num_players {
            clear_holdings(&mut g, p);
            let c = op_corp(&mut g, &mut r);
            add_holding(&mut g, p, 1, c, cast::<i16, _>(r.range(20, 80)), Ext::from_f64(100.0));
            g.cash[p] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(5000.0));
        }
        // make rivals cash-poor and controlled by corps
        for c in 131..=1580 {
            if r.chance(Ext::from_f64(0.8)) {
                g.corp_controller[c] = cast::<i32, _>(r.range(11, 1600));
                g.cash[c] = f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(50.0));
                g.t_bills[c] = Ext::from_f64(0.0);
                g.business_assets[c] = f64r(&mut r, Ext::from_f64(500.0), Ext::from_f64(5000.0));
                let lev = if seed % 2 == 0 { f64r(&mut r, Ext::from_f64(0.8), Ext::from_f64(1.3)) } else { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(0.5)) };
                g.bank_debt[c] = g.business_assets[c] * lev;
                clear_holdings(&mut g, c);
                g.bonds_issued[c] = 0;
                g.working_capital[c] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(100.0));
                g.growth_rate[c] = cast::<i16, _>(r.range(-5, 25));
            }
        }
        if seed % 4 != 0 {
            for ind in 3..=69 {
                g.ind_demand[ind] = 100_000_000;
            }
        }
        let rng0 = if seed % 3 == 0 { seed_rng(seed) } else { low_rng(seed, Ext::from_f64(0.3)) };
        let Some((o, _, _)) = check(BOOST_RIVAL, &[], &g, rng0, &[], |p, ui, rng| {
            ai_boost_leaders_rival(p, ui, rng)
        }) else {
            return;
        };
        if o.globals.tmp_ext1 != Ext::from_f64(0.0) || o.globals.last_rival_offering_year == 0 {
            bump(&REACHED);
        }
        if cast::<i32, _>(o.globals.last_rival_offering_year) == g.cur_year && cast::<i32, _>(g.last_rival_offering_year) != g.cur_year {
            bump(&OFFER);
        }
        if (131..=1600).any(|c| o.globals.growth_rate[c] != g.growth_rate[c]) {
            bump(&GROWTH);
        }
    });
    report("boost rival: reached the holding", &REACHED);
    report("boost rival: stock offering", &OFFER);
    report("boost rival: growth boost", &GROWTH);
}

#[test]
fn diff_ai_holding_co_merger_plan() {
    static PICKED: AtomicUsize = AtomicUsize::new(0);
    static MERGED: AtomicUsize = AtomicUsize::new(0);
    static INFUSION: AtomicUsize = AtomicUsize::new(0);
    static LIQ: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=60, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed ^ 0x4e);
        let a = cast::<i32, _>(r.range(131, 800));
        let b = cast::<i32, _>(r.range(801, 1580));
        for c in [a, b] {
            g.entity_status[c] = 1;
            g.industry_num[c] = if r.chance(Ext::from_f64(0.92)) { 70 } else { 30 };
            g.corp_controller[c] = if r.chance(Ext::from_f64(0.85)) { c } else { cast::<i32, _>(r.range(1, 1600)) };
            g.bank_debt[c] = Ext::from_f64(0.0);
            g.cash[c] = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(5000.0));
        }
        g.difficulty_level = cast::<i32, _>(r.range(1, 4));
        if seed % 3 == 0 {
            // no pair yet: SimCoCount may become the acquirer or the target
            g.ai_merger_acquirer_id = if r.chance(Ext::from_f64(0.5)) { 0 } else { b };
            g.ai_merger_target_id = 0;
            g.sim_co_count = a;
            if r.chance(Ext::from_f64(0.5)) {
                g.market_cap[a] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(60.0));
            }
            if r.chance(Ext::from_f64(0.5)) {
                for s in 1..=15 {
                    g.hold_stock_id[(s, a)] = cast::<i16, _>(200 + s);
                    g.hold_stock_pct[(s, a)] = 5;
                }
                g.hold_stock_id[(0, a)] = 15;
            }
        } else {
            let (acq, tgt) = if r.chance(Ext::from_f64(0.5)) { (a, b) } else { (b, a) };
            g.ai_merger_acquirer_id = acq;
            g.ai_merger_target_id = tgt;
            // the holder owns the other one outright (so it ends up wholly owned)
            let (holder, held) = if r.chance(Ext::from_f64(0.5)) { (acq, tgt) } else { (tgt, acq) };
            for h in 1..=1600 {
                for s in 1..=15 {
                    if cast::<i32, _>(g.hold_stock_id[(s, h)]) == held {
                        g.hold_stock_id[(s, h)] = 0;
                        g.hold_stock_pct[(s, h)] = 0;
                    }
                }
            }
            if r.chance(Ext::from_f64(0.9)) {
                add_holding(&mut g, holder, 3, held, if r.chance(Ext::from_f64(0.8)) { 100 } else { 60 }, Ext::from_f64(500.0));
                if g.corp_controller[held] != held && r.chance(Ext::from_f64(0.5)) {
                    g.corp_controller[held] = held;
                }
                g.corp_controller[holder] = if r.chance(Ext::from_f64(0.8)) { holder } else { 1 };
            }
            g.market_cap[held] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(100.0));
            g.tax_loss_carry[held] = if r.chance(Ext::from_f64(0.5)) { -f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(20000.0)) } else { Ext::from_f64(0.0) };
            g.tax_loss_carry[holder] = if r.chance(Ext::from_f64(0.5)) { -f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(2000.0)) } else { Ext::from_f64(0.0) };
        }
        let rng0 = if seed % 3 != 0 && seed % 5 != 0 { low_rng(seed, Ext::from_f64(0.05)) } else { seed_rng(seed) };
        let Some((o, _, _)) = check(HOLDCO_MERGER, &[], &g, rng0, &[MERGE, LIQ_SUB], |p, ui, rng| {
            ai_holding_co_merger_plan(p, ui, rng, 0)
        }) else {
            return;
        };
        if g.ai_merger_target_id == 0
            && (o.globals.ai_merger_acquirer_id != g.ai_merger_acquirer_id || o.globals.ai_merger_target_id != 0)
        {
            bump(&PICKED);
        }
        hits(&o.log, "CALL MergeCompanies(", &MERGED);
        hits(&o.log, "CALL LiquidateSubIntoParent(", &LIQ);
        if o.log.iter().any(|l| l.contains("CALL MergeCompanies("))
            && !o.log.iter().any(|l| l.contains("CALL LiquidateSubIntoParent("))
            && o.globals.wholly_owned_by != 0
        {
            bump(&INFUSION);
        }
    });
    report("holding merger: pair member picked", &PICKED);
    report("holding merger: merged", &MERGED);
    report("holding merger: liquidated", &LIQ);
    report("holding merger: capital infusion", &INFUSION);
}

#[test]
fn diff_ai_manage_small_holdings() {
    static BUY: AtomicUsize = AtomicUsize::new(0);
    static SELL: AtomicUsize = AtomicUsize::new(0);
    static CPU_CAP: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=48, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed ^ 0x5a);
        let c = op_corp(&mut g, &mut r);
        g.corp_controller[c] = match seed % 3 {
            0 => cast::<i32, _>(r.range(11, 1600)),
            _ => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
        };
        g.cpu_max_corps = cast::<i32, _>(r.range(0, 10));
        for p in 1..=5 {
            g.player_corps_controlled[p] = cast::<i32, _>(r.range(0, 12));
        }
        clear_holdings(&mut g, c);
        for s in 1..=15 {
            if r.chance(Ext::from_f64(0.8)) {
                let h = op_corp(&mut g, &mut r);
                add_holding(&mut g, c, s, h, cast::<i16, _>(r.range(1, 25)), f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(300.0)));
                g.market_cap[h] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(400.0));
            }
        }
        g.cash[c] = if r.chance(Ext::from_f64(0.5)) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(100.0)) } else { f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(20000.0)) };
        let a = c.to_string();
        let Some((o, _, _)) = check(SMALL_HOLDINGS, &[&a], &g, seed_rng(seed), &[BUY_STOCK], |p, ui, rng| {
            ai_manage_small_holdings(p, ui, rng, c, 0)
        }) else {
            return;
        };
        hits(&o.log, "CALL BuyStock(", &BUY);
        if (1..=15).any(|s| o.globals.hold_stock_pct[(s, c)] < g.hold_stock_pct[(s, c)])
            || o.globals.hold_stock_id[(0, c)] < g.hold_stock_id[(0, c)]
        {
            bump(&SELL);
        }
        if g.is_computer_player[g.corp_controller[c]] == 1 && o.globals.tmp_ext2 == Ext::from_f64(0.0) {
            bump(&CPU_CAP);
        }
    });
    report("small holdings: buy", &BUY);
    report("small holdings: sell", &SELL);
    report("small holdings: computer player corp", &CPU_CAP);
}

#[test]
fn diff_ai_bank_buys_loan() {
    static SOLD: AtomicUsize = AtomicUsize::new(0);
    static MOVED: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=60, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed ^ 0x10);
        g.num_players = cast::<i32, _>(r.range(2, 4));
        let cpu = cast::<i32, _>(r.range(2, cast::<i64, _>(g.num_players)));
        g.is_computer_player[cpu] = 1;
        g.has_cpu_players = if r.chance(Ext::from_f64(0.9)) { st("Y") } else { st("N") };
        // banks: controlled by the computer player, a human or a corp (never by nobody: error 9)
        for b in 11..=85 {
            g.entity_status[b] = if r.chance(Ext::from_f64(0.9)) { 1 } else { 2 };
            g.corp_controller[b] = match r.range(0, 3) {
                0 | 1 => cpu,
                2 => 1,
                _ => cast::<i32, _>(r.range(86, 1600)),
            };
        }
        let c = op_corp(&mut g, &mut r);
        g.sim_co_count = c;
        g.corp_controller[c] = if r.chance(Ext::from_f64(0.8)) { cpu } else { cast::<i32, _>(r.range(1, 1600)) };
        let lender = cast::<i32, _>(r.range(11, 85));
        g.lending_bank[c] = cast::<i16, _>(lender);
        g.corp_controller[lender] = if r.chance(Ext::from_f64(0.8)) { cast::<i32, _>(r.range(86, 1600)) } else { cpu };
        g.bank_debt[c] = if r.chance(Ext::from_f64(0.2)) { Ext::from_f64(0.0) } else { f64r(&mut r, Ext::from_f64(10.0), Ext::from_f64(3000.0)) };
        g.cash[c] = if r.chance(Ext::from_f64(0.2)) { Ext::from_f64(0.0) } else { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(500.0)) };
        g.news_msg = PbStr::new();
        let Some((o, _, ui)) = check(BANK_BUYS_LOAN, &[], &g, seed_rng(seed), &[], |p, ui, rng| {
            ai_bank_buys_loan(p, ui, rng)
        }) else {
            return;
        };
        if o.globals.lending_bank[c] != g.lending_bank[c] {
            if g.bank_debt[c] == Ext::from_f64(0.0) {
                bump(&MOVED);
            } else {
                bump(&SOLD);
            }
        }
        let _ = ui;
    });
    report("bank buys loan: loan sold", &SOLD);
    report("bank buys loan: debt-free borrower moved", &MOVED);
}
