//! Differential tests of `logic::etf_derivs::etf_ops`: BondEtfManagePortfolio, EtfPayPerformanceBonus,
//! IndexEtfMaintenance, PromptReduceEtfFee, RemindEtfManagerDuties, EtfUpdateBonusEligibility and
//! EtfSellIneligibleHolding against the original.
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use wsr_core::logic::etf_derivs::etf_ops::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::pb::numeric::Cur;
use wsr_core::state::Globals;

const BOND_ETF: u32 = 0x655197;
const PAY_BONUS: u32 = 0x6520bf;
const INDEX_MAINT: u32 = 0x652f2e;
const PROMPT_FEE: u32 = 0x652b9a;
const REMIND: u32 = 0x6527da;
const BONUS_ELIG: u32 = 0x651df1;
const SELL_INELIGIBLE: u32 = 0x64f724;
/// No unported callees.
const SKIPS: &[(u32, u32)] = &[];

fn rng0(seed: u64) -> u32 {
    (cast::<u32, _>(seed)).wrapping_mul(2_654_435_761) ^ 0x5bd1_e995
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
    cmp().rel_tol(Ext::from_f64(1e-12)).assert(port, &o.globals);
    assert_eq!(rng.state(), o.rng, "{what}: RND state");
    assert_eq!(ui.msgs.len(), orig_msgs(o), "{what}: message boxes {:?}", ui.msgs);
}

fn hit(c: &AtomicUsize) {
    c.fetch_add(1, Relaxed);
}

#[track_caller]
fn assert_hit(name: &str, c: &AtomicUsize) {
    if unavailable().is_none() {
        assert!(c.load(Relaxed) > 0, "branch not covered: {name}");
    }
}

/// [`world`] with active funds 1581..1600 (industry 71, advisers, holdings), bond-fund portfolios,
/// rising/falling price histories of the funds and a few advisers controlled by player 1.
fn fund_world(seed: u64) -> Globals {
    let mut g = world(seed);
    let mut r = Rand(seed * 37 + 11);
    g.cur_player = 1;
    g.is_computer_player[1] = 0;
    g.sim_co_count = cast::<i32, _>(r.range(500, 1600));
    g.base_interest_rate = cast::<f32, _>([f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(4.0)), f32r(&mut r, Ext::from_f64(4.0), Ext::from_f64(8.0)), f32r(&mut r, Ext::from_f64(7.0), Ext::from_f64(13.0))]
        [cast::<usize, _>(r.range(0, 2))]);
    g.short_bond_yield = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(10.0)));
    g.long_bond_yield = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(10.0)));
    if r.chance(Ext::from_f64(0.15)) {
        g.game_end_year = g.cur_year + 1;
    }
    if r.chance(Ext::from_f64(0.1)) {
        g.long_bond_price = cast::<f32, _>(Ext::from_f64(0.0));
    }
    if r.chance(Ext::from_f64(0.1)) {
        g.short_bond_price = cast::<f32, _>(Ext::from_f64(0.0));
    }
    g.lender_id = cast::<i32, _>(r.range(11, 85));
    g.rand_seed = cast::<i32, _>(r.range(0, 32000));
    // more bond issuers, some convertibles, a few high prices
    for e in 11..=1580 {
        if g.bonds_issued[e] == 0 && r.chance(Ext::from_f64(0.4)) {
            g.bonds_issued[e] = r.range(100, 3000);
            let mat = g.cur_year + cast::<i32, _>(r.range(0, 25));
            let cv = if r.chance(Ext::from_f64(0.2)) { "cv" } else { "" };
            g.bond_issue_info[e] = st(&format!("{mat} {cv}  {:5.2}", f32r(&mut r, Ext::from(Ext::from_f64(2.0)), Ext::from(Ext::from_f64(15.0)))));
            g.bond_price[e] = Cur::from_f64(f64r(&mut r, Ext::from_f64(20.0), Ext::from_f64(130.0)));
        }
        if g.bonds_issued[e] > 0 && r.chance(Ext::from_f64(0.05)) {
            g.bond_price[e] = Cur::from_f64(f64r(&mut r, Ext::from_f64(110.0), Ext::from_f64(260.0)));
        }
    }
    for f in 1581..=1600 {
        let active = r.chance(Ext::from_f64(0.9));
        g.entity_status[f] = if active { 1 } else { 0 };
        g.industry_num[f] = if r.chance(Ext::from_f64(0.95)) { 71 } else { 40 };
        g.corp_controller[f] = 0;
        g.fund_adviser[f] = cast::<i32, _>(r.range(86, 1580));
        g.ctrl_since_year[f] = if r.chance(Ext::from_f64(0.2)) { 0 } else { g.cur_year - cast::<i32, _>(r.range(0, 5)) };
        g.fund_fee_pct[f] = cast::<f32, _>(if r.chance(Ext::from_f64(0.5)) { Ext::from_f64(0.2) } else { Ext::from(f32r(&mut r, Ext::from_f64(0.2), Ext::from_f64(1.5))) });
        g.fund_bonus_year[f] = match r.range(0, 3) {
            0 => 0,
            1 => g.cur_year - cast::<i32, _>(r.range(0, 4)),
            _ => g.cur_year - 2 + cast::<i32, _>(r.range(0, 1)),
        };
        g.cash[f] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(4000.0));
        g.t_bills[f] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(4000.0));
        g.bank_debt[f] = if r.chance(Ext::from_f64(0.3)) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(3000.0)) } else { Ext::from_f64(0.0) };
        g.extraordinary_pl[f] = if r.chance(Ext::from_f64(0.3)) { r.range(-200, 500) } else { 0 };
        g.credit_rating_num[f] = cast::<i32, _>(r.range(5, 10));
        g.market_cap[f] = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(20000.0));
        // price history: market cap 24 months ago
        let cur = cast::<i32, _>(g.cap_hist_close[(0, f)]);
        g.cap_hist_close[(cur, f)] = g.market_cap[f];
        for (k, m) in [(24, f64r(&mut r, Ext::from_f64(0.4), Ext::from_f64(1.3))), (12, f64r(&mut r, Ext::from_f64(0.7), Ext::from_f64(1.4))), (6, f64r(&mut r, Ext::from_f64(0.8), Ext::from_f64(1.3))), (11, Ext::from_f64(1.0))] {
            let mut s = cur - k;
            if s < 1 {
                s += 60;
            }
            g.cap_hist_close[(s, f)] = g.market_cap[f] * m;
        }
        // adviser: sometimes an insurer/bank-like (2/37) corp controlled by player 1
        if r.chance(Ext::from_f64(0.4)) {
            // insurers (industry 2) are 86..130; brokers (37) anywhere above
            if r.chance(Ext::from_f64(0.5)) {
                g.fund_adviser[f] = cast::<i32, _>(r.range(86, 130));
                g.industry_num[g.fund_adviser[f]] = 2;
            } else {
                g.fund_adviser[f] = cast::<i32, _>(r.range(131, 1580));
                g.industry_num[g.fund_adviser[f]] = 37;
            }
            let adv = g.fund_adviser[f];
            g.entity_status[adv] = 1;
            g.corp_controller[adv] = if r.chance(Ext::from_f64(0.8)) { 1 } else { 2.min(g.num_players) };
            g.auto_pilot[adv] = cast::<i32, _>(r.range(0, 1));
        }
        // stock holdings 1..n (some in industry 70)
        let n = cast::<i32, _>(r.range(0, 15));
        for slot in 1..=15 {
            if slot <= n {
                let s = cast::<i32, _>(r.range(131, 1580));
                g.hold_stock_id[(slot, f)] = cast::<i16, _>(s);
                g.hold_stock_pct[(slot, f)] = cast::<i16, _>(r.range(0, 9));
                if r.chance(Ext::from_f64(0.1)) {
                    g.industry_num[s] = 70;
                }
            } else {
                g.hold_stock_id[(slot, f)] = 0;
                g.hold_stock_pct[(slot, f)] = 0;
            }
        }
        g.hold_stock_id[(0, f)] = cast::<i16, _>(n);
    }
    // bond-fund portfolios
    for f in 1581..=1583 {
        if r.chance(Ext::from_f64(0.9)) {
            g.entity_status[f] = 1;
            g.industry_num[f] = 71;
        }
        let issuers: Vec<i32> = (11..=1580).filter(|&e| g.bonds_issued[e] > 0).collect();
        let n = if r.chance(Ext::from_f64(0.4)) { 15 } else { cast::<i32, _>(r.range(0, 15)) };
        for slot in 1..=15 {
            if slot <= n && !issuers.is_empty() {
                let b = issuers[cast::<usize, _>(r.range(0, cast::<i64, _>(issuers.len()) - 1))];
                g.hold_bond_id[(slot, f)] = b;
                g.hold_bond_face[(slot, f)] = r.range(1, 400);
                g.bond_hold_cost[(slot, f)] = cast::<Ext, _>(g.hold_bond_face[(slot, f)]) * f64r(&mut r, Ext::from_f64(0.8), Ext::from_f64(1.1));
            } else {
                g.hold_bond_id[(slot, f)] = 0;
                g.hold_bond_face[(slot, f)] = 0;
            }
        }
        g.hold_bond_id[(0, f)] = n.min(cast::<i32, _>(issuers.len()));
        g.long_gov_bond_face[f] = match r.range(0, 4) {
            0 => 0,
            1 => r.range(50_000, 300_000),
            _ => r.range(0, 20000),
        };
        g.short_gov_bond_face[f] = if r.chance(Ext::from_f64(0.8)) { r.range(0, 20000) } else { 0 };
        g.st_gov_bond_cost[f] = cast::<Ext, _>(g.short_gov_bond_face[f]) * f64r(&mut r, Ext::from_f64(0.7), Ext::from_f64(1.1));
        if r.chance(Ext::from_f64(0.5)) {
            g.cash[f] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(30000.0));
        }
    }
    g
}

static B_LONG_SELL: AtomicUsize = AtomicUsize::new(0);
static B_SHORT_SELL: AtomicUsize = AtomicUsize::new(0);
static B_GOV_BUY: AtomicUsize = AtomicUsize::new(0);
static B_DEBT: AtomicUsize = AtomicUsize::new(0);
static B_CORP_BUY: AtomicUsize = AtomicUsize::new(0);
static B_CORP_SELL: AtomicUsize = AtomicUsize::new(0);
static B_CORP_SELL_ALL: AtomicUsize = AtomicUsize::new(0);

#[test]
fn diff_bond_etf_manage_portfolio() {
    let __content = wsr_core::content::current();
    par_seeds(1..=60, |seed| {
        let g = fund_world(seed);
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        bond_etf_manage_portfolio(&mut port, &mut ui, &mut rng, 0);
        let Some(o) = run(BOND_ETF, &[], &g, rs, &[], SKIPS) else {
            return;
        };
        check(&port, &rng, &ui, &o, &format!("seed {seed}"));
        let f = 1581;
        if port.long_gov_bond_face[f] < g.long_gov_bond_face[f] {
            hit(&B_LONG_SELL);
        }
        if port.short_gov_bond_face[f] < g.short_gov_bond_face[f] {
            hit(&B_SHORT_SELL);
        }
        if port.long_gov_bond_face[f] > g.long_gov_bond_face[f] || port.short_gov_bond_face[f] > g.short_gov_bond_face[f] {
            hit(&B_GOV_BUY);
        }
        for f in 1581..=1583 {
            if port.bank_debt[f] < g.bank_debt[f] {
                hit(&B_DEBT);
            }
            let face = |g: &Globals| (1..=15).map(|s| g.hold_bond_face[(s, f)]).sum::<i64>();
            if f > 1581 && face(&port) > face(&g) {
                hit(&B_CORP_BUY);
            }
            if f > 1581 && (1..=15).any(|s| port.hold_bond_face[(s, f)] < g.hold_bond_face[(s, f)] && port.hold_bond_id[(s, f)] == g.hold_bond_id[(s, f)]) {
                hit(&B_CORP_SELL);
            }
            let held = |g: &Globals| (1..=15).filter(|&s| g.hold_bond_id[(s, f)] != 0).count();
            if f > 1581 && held(&port) < held(&g) {
                hit(&B_CORP_SELL_ALL);
            }
        }
    });
    assert_hit("long sell", &B_LONG_SELL);
    assert_hit(__content.string(13791), &B_SHORT_SELL);
    assert_hit("gov buy", &B_GOV_BUY);
    assert_hit(__content.string(13792), &B_DEBT);
    assert_hit("corp buy", &B_CORP_BUY);
    assert_hit("corp sell", &B_CORP_SELL);
    assert_hit("corp sell whole holding", &B_CORP_SELL_ALL);
}

static P_PAID: AtomicUsize = AtomicUsize::new(0);
static P_NEWS: AtomicUsize = AtomicUsize::new(0);

#[test]
fn diff_etf_pay_performance_bonus() {
    par_seeds(1..=30, |seed| {
        let mut g = fund_world(seed + 1000);
        if seed % 5 != 0 {
            g.cur_quarter = 4;
        }
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        etf_pay_performance_bonus(&mut port, &mut ui, &mut rng, 0);
        let Some(o) = run(PAY_BONUS, &[], &g, rs, &[], SKIPS) else {
            return;
        };
        check(&port, &rng, &ui, &o, &format!("seed {seed}"));
        if (1586..=1600).any(|f| port.cash[f] < g.cash[f]) {
            hit(&P_PAID);
        }
        if !ui.news.is_empty() {
            hit(&P_NEWS);
        }
    });
    assert_hit("paid", &P_PAID);
    assert_hit("news", &P_NEWS);
}

static I_DIVEST: AtomicUsize = AtomicUsize::new(0);

#[test]
fn diff_index_etf_maintenance() {
    par_seeds(1..=16, |seed| {
        let mut g = fund_world(seed + 2000);
        let mut r = Rand(seed * 7 + 1);
        g.sim_co_count = cast::<i32, _>(r.range(1580, 1586));
        let e = g.sim_co_count;
        g.entity_status[e] = 1;
        g.industry_num[e] = 71;
        // corp-controlled holders of the index fund
        for _ in 0..r.range(0, 3) {
            let h = cast::<i32, _>(r.range(131, 1580));
            g.entity_status[h] = 1;
            g.corp_controller[h] = cast::<i32, _>(r.range(11, 1580));
            g.hold_stock_id[(1, h)] = cast::<i16, _>(e);
            g.hold_stock_pct[(1, h)] = cast::<i16, _>(r.range(1, 5));
            g.hold_stock_id[(0, h)] = g.hold_stock_id[(0, h)].max(1);
        }
        // some swaps involving the index funds
        let n = cast::<i32, _>(r.range(0, 4));
        for i in 1..=n {
            g.swap_contracts[i] = PbStr::from(format!("{}|{}|5.0|P|{}.1|{}.1|100", r.range(1581, 1585), r.range(11, 1580), g.cur_year, g.cur_year + 3).as_str());
        }
        g.swap_contracts[0] = PbStr::from(n.to_string().as_str());
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        index_etf_maintenance(&mut port, &mut ui, &mut rng);
        let Some(o) = run(INDEX_MAINT, &[], &g, rs, &[], SKIPS) else {
            return;
        };
        check(&port, &rng, &ui, &o, &format!("seed {seed}"));
        if !ui.news.is_empty() {
            hit(&I_DIVEST);
        }
    });
    assert_hit("divest", &I_DIVEST);
}

static F_ASKED: AtomicUsize = AtomicUsize::new(0);

#[test]
fn diff_prompt_reduce_etf_fee() {
    par_seeds(1..=24, |seed| {
        let mut g = fund_world(seed + 3000);
        let mut r = Rand(seed * 3 + 2);
        g.cur_quarter = if seed % 6 == 0 { 3 } else { 4 };
        g.sim_co_count = cast::<i32, _>(r.range(1484, 1500));
        let e = g.sim_co_count + 100;
        if (1581..=1600).contains(&e) && r.chance(Ext::from_f64(0.8)) {
            g.entity_status[e] = 1;
            g.industry_num[e] = 71;
            let adv = cast::<i32, _>(r.range(86, 130));
            g.fund_adviser[e] = adv;
            g.entity_status[adv] = 1;
            g.industry_num[adv] = 2;
            g.corp_controller[adv] = 1;
            g.fund_fee_pct[e] = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.15), Ext::from_f64(1.0)));
        }
        let ans = if seed % 2 == 0 { "IDYES" } else { "IDNO" };
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        ui.answers.push_back(if seed % 2 == 0 { 6 } else { 7 });
        prompt_reduce_etf_fee(&mut port, &mut ui, &mut rng);
        let Some(o) = run(PROMPT_FEE, &[], &g, rs, &[ans], SKIPS) else {
            return;
        };
        check(&port, &rng, &ui, &o, &format!("seed {seed}"));
        if !ui.msgs.is_empty() {
            hit(&F_ASKED);
        }
    });
    assert_hit("asked", &F_ASKED);
}

static R_SHOWN: AtomicUsize = AtomicUsize::new(0);

#[test]
fn diff_remind_etf_manager_duties() {
    par_seeds(1..=16, |seed| {
        let g = fund_world(seed + 4000);
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        remind_etf_manager_duties(&mut port, &mut ui, &mut rng);
        let Some(o) = run(REMIND, &[], &g, rs, &[], SKIPS) else {
            return;
        };
        check(&port, &rng, &ui, &o, &format!("seed {seed}"));
        if !ui.msgs.is_empty() {
            hit(&R_SHOWN);
        }
    });
    assert_hit("shown", &R_SHOWN);
}

static E_LOST: AtomicUsize = AtomicUsize::new(0);
static E_SET: AtomicUsize = AtomicUsize::new(0);

#[test]
fn diff_etf_update_bonus_eligibility() {
    par_seeds(1..=16, |seed| {
        let mut g = fund_world(seed + 5000);
        if seed % 4 != 0 {
            g.cur_quarter = 4;
        }
        let rs = rng0(seed);
        let mut port = g.clone();
        let rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        etf_update_bonus_eligibility(&mut port, &mut ui);
        let Some(o) = run(BONUS_ELIG, &[], &g, rs, &[], SKIPS) else {
            return;
        };
        check(&port, &rng, &ui, &o, &format!("seed {seed}"));
        if !ui.news.is_empty() {
            hit(&E_LOST);
        }
        if (1586..=1600).any(|f| port.fund_bonus_year[f] == port.cur_year && g.fund_bonus_year[f] != g.cur_year) {
            hit(&E_SET);
        }
    });
    assert_hit("lost (news)", &E_LOST);
    assert_hit("set", &E_SET);
}

static S_SOLD: AtomicUsize = AtomicUsize::new(0);

#[test]
fn diff_etf_sell_ineligible_holding() {
    par_seeds(1..=24, |seed| {
        let g = fund_world(seed + 6000);
        let mut r = Rand(seed * 5 + 3);
        let e = cast::<i32, _>(if r.chance(Ext::from_f64(0.1)) { r.range(1500, 1600) } else { r.range(1586, 1600) });
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        etf_sell_ineligible_holding(&mut port, &mut ui, &mut rng, e);
        let Some(o) = run(SELL_INELIGIBLE, &[&e.to_string()], &g, rs, &[], SKIPS) else {
            return;
        };
        check(&port, &rng, &ui, &o, &format!("seed {seed} etf {e}"));
        if (1..=15).any(|s| port.hold_stock_pct[(s, e)] < g.hold_stock_pct[(s, e)]) || port.hold_stock_id[(0, e)] < g.hold_stock_id[(0, e)] {
            hit(&S_SOLD);
        }
    });
    assert_hit("sold", &S_SOLD);
}
