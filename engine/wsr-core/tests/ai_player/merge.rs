//! Differential tests of `logic::ai_player::merge`: the AiComputerPlayerTurn steps MergeControlledCorps,
//! HostileMerger, IssueStockOffering, ImproveCreditRating, BuyoutPublicShares, MergeTaxLossSub and
//! AntitrustScan, each run alone (the other steps skipped on both sides).
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
use wsr_core::state::Globals;

const MERGE_CONTROLLED: u32 = 0x5cd435;
const HOSTILE_MERGER: u32 = 0x5cdf0e;
const ISSUE_OFFERING: u32 = 0x5ce762;
const IMPROVE_CREDIT: u32 = 0x5cebd9;
const BUYOUT_PUBLIC: u32 = 0x5cf5ba;
const MERGE_TAX_LOSS: u32 = 0x5cfad1;
const ANTITRUST_SCAN: u32 = 0x5cfe21;

fn harness() -> bool {
    super::diff::unavailable().is_none()
}

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

/// Clears all stock holdings of `holder`.
fn clear_portfolio(g: &mut Globals, holder: i32) {
    for s in 0..=15 {
        g.hold_stock_id[(s, holder)] = 0;
        g.hold_stock_pct[(s, holder)] = 0;
        g.hold_stock_basis[(s, holder)] = Ext::from_f64(0.0);
    }
}

/// `holder` gets `pct` of `corp` (all other holders removed first) and `corp` is controlled by `ctrl`.
fn own(g: &mut Globals, r: &mut Rand, holder: i32, corp: i32, pct: i32, ctrl: i32) {
    clear_holders(g, corp);
    add_holding(g, r, holder, corp, pct);
    g.corp_controller[corp] = ctrl;
}

/// Makes player 1 the computer player to move.
fn mover_is_player_1(g: &mut Globals) {
    g.cur_player = 1;
    g.is_computer_player[1] = 1;
}

/// An active operating corp in `lo..=hi` not in `not`.
fn corp_in(g: &Globals, r: &mut Rand, lo: i32, hi: i32, not: &[i32]) -> i32 {
    for _ in 0..500 {
        let c = cast::<i32, _>(r.range(cast::<i64, _>(lo), cast::<i64, _>(hi)));
        if g.entity_status[c] == 1 && g.industry_num[c] < 69 && g.industry_num[c] > 2 && !not.contains(&c) {
            return c;
        }
    }
    lo
}

/// Runs the step alone and compares; returns the run if compared.
#[track_caller]
fn step(what: &str, addr: u32, g: &Globals, rng: u32, extra: &[(u32, u32)]) -> Option<TurnRun> {
    let t = run_turn(g, rng, Some(&[addr]), extra, &[])?;
    if check_turn(what, &t, Ext::from_f64(1e-9)) { Some(t) } else { None }
}

/// The original's last headline is a divestiture.
fn instr_news(g: &Globals) -> bool {
    let __content = wsr_core::content::current();
    g.news_msg.to_display().contains(__content.string(13563))
}

/// Calls of the hooked callee `name` in the original.
fn skipped(t: &TurnRun, name: &str) -> usize {
    let __content = wsr_core::content::current();
    t.orig.log.iter().filter(|l| l.starts_with(&format!("{__wsr_text_13564}{name}(",__wsr_text_13564=__content.string(13564)))).count()
}

// --------------------------------------------------------------------------- MergeControlledCorps

#[test]
fn diff_merge_controlled_corps() {
    let __content = wsr_core::content::current();
    let merges = AtomicU32::new(0);
    let liquidations = AtomicU32::new(0);
    par_seeds(1..=32, |seed| {
        let mut g = ai_world(seed + 5000);
        let mut r = Rand(seed * 7919 + 1);
        if seed % 3 == 0 {
            mover_is_player_1(&mut g);
        }
        let p = g.cur_player;
        let main = cast::<i32, _>(g.hold_stock_id[(1, p)]);
        if seed % 8 != 7 && main > 10 {
            g.corp_controller[main] = p;
        }
        // two controlled companies of one industry (scanned from any start: ids above 1100)
        if seed % 4 != 3 {
            let a = corp_in(&g, &mut r, 1101, 1580, &[main]);
            let b = corp_in(&g, &mut r, 1101, 1580, &[main, a]);
            g.industry_num[b] = g.industry_num[a];
            let v = cast::<i32, _>(r.range(30, 70));
            own(&mut g, &mut r, p, a, v, p);
            let v = cast::<i32, _>(r.range(30, 70));
            own(&mut g, &mut r, p, b, v, p);
            match seed % 5 {
                0 => {
                    add_holding(&mut g, &mut r, b, a, 20);
                }
                1 => {
                    add_holding(&mut g, &mut r, a, b, 25);
                }
                2 => {
                    // b wholly owned by a
                    clear_holders(&mut g, b);
                    add_holding(&mut g, &mut r, a, b, 100);
                    g.corp_controller[b] = p;
                }
                _ => {}
            }
        }
        // cheap well-held companies for the last scan
        for _ in 0..r.range(0, 6) {
            let c = corp_in(&g, &mut r, 86, 1580, &[main]);
            g.market_cap[c] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(20.0));
        }
        let rs = cast::<u32, _>(r.next());
        let Some(t) = step(&format!("MergeControlledCorps seed {seed}{__wsr_text_13565}{p}",__wsr_text_13565=__content.string(13565)), MERGE_CONTROLLED, &g, rs, &[]) else {
            return;
        };
        merges.fetch_add(cast::<u32, _>(skipped(&t, "MergeCompanies")), Relaxed);
        liquidations.fetch_add(cast::<u32, _>(skipped(&t, "LiquidateSubIntoParent")), Relaxed);
    });
    if harness() {
        let (m, l) = (merges.into_inner(), liquidations.into_inner());
        eprintln!("MergeControlledCorps: {m} merges, {l}{__wsr_text_13566}",__wsr_text_13566=__content.string(13566));
        assert!(m >= 4, "too few merges ({m})");
    }
}

// --------------------------------------------------------------------------------- HostileMerger

#[test]
fn diff_hostile_merger() {
    let __content = wsr_core::content::current();
    let reached = AtomicU32::new(0);
    let buyer = AtomicU32::new(0);
    par_seeds(1..=48, |seed| {
        let mut g = ai_world(seed + 6000);
        let mut r = Rand(seed * 104_729 + 3);
        g.difficulty_level = cast::<i32, _>(r.range(1, 3));
        if seed % 6 != 5 {
            g.difficulty_level = g.difficulty_level.max(2);
        }
        g.ai_hostile_merger_qtr = [-5, -5, 9, -1, g.cur_quarter, 0][cast::<usize, _>(r.range(0, 5))];
        let p = g.cur_player;
        // a human rival (2..NumPlayers) with a large holding
        let rival = cast::<i32, _>(r.range(2, cast::<i64, _>(g.num_players)));
        if seed % 5 != 4 {
            for q in 1..=5 {
                g.is_computer_player[q] = cast::<i32, _>(q == p);
            }
        }
        let t = corp_in(&g, &mut r, 131, 1580, &[]);
        if rival != p {
            clear_portfolio(&mut g, rival);
            g.market_cap[t] = f64r(&mut r, Ext::from_f64(50.0), Ext::from_f64(500.0));
            // 20..28%: the rival controls it, weakly held (10..19%: not controlled)
            let v = cast::<i32, _>(if seed % 3 == 0 { r.range(10, 19) } else { r.range(20, 28) });
            own(&mut g, &mut r, rival, t, v, rival);
            if seed % 4 == 1 {
                // the rival's company holds a weakly held, big company
                let pick = corp_in(&g, &mut r, 131, 1580, &[t]);
                g.market_cap[pick] = f64r(&mut r, Ext::from_f64(50.0), Ext::from_f64(500.0));
                let v = cast::<i32, _>(r.range(10, 28));
                own(&mut g, &mut r, t, pick, v, rival);
                g.business_assets[t] = f64r(&mut r, Ext::from_f64(10.0), Ext::from_f64(2000.0));
            }
        }
        // the toxic company
        let z = corp_in(&g, &mut r, 131, 1580, &[t]);
        clear_holders(&mut g, z);
        g.corp_controller[z] = z;
        g.market_cap[z] = g.market_cap[t] * f64r(&mut r, Ext::from_f64(0.2), Ext::from_f64(3.0));
        g.cash[z] = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(3000.0));
        g.asbestos_corp = 0;
        g.superfund_corp = 0;
        match seed % 4 {
            0 => g.asbestos_corp = z,
            1 => g.asbestos_corp = -z,
            2 => g.superfund_corp = cast::<i16, _>(z),
            _ => {
                if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                    g.asbestos_corp = corp_in(&g, &mut r, 131, 1580, &[t, z]);
                }
                g.superfund_corp = cast::<i16, _>(z);
            }
        }
        // companies of player 1 as buyers (ValidateBuyerCorp only accepts them when player 1 moves):
        // liquid enough for 22x the toxic company's market cap, cash below the 20% price
        if seed % 2 == 0 && rival != 1 {
            mover_is_player_1(&mut g);
        }
        let cost = g.market_cap[z] * Ext::from_f64(22.0);
        for _ in 0..r.range(0, 6) {
            let c = corp_in(&g, &mut r, 86, 1580, &[t, z]);
            g.corp_controller[c] = 1;
            g.cash[c] = cost * f64r(&mut r, Ext::from_f64(0.3), Ext::from_f64(0.9));
            g.t_bills[c] = cost * f64r(&mut r, Ext::from_f64(0.2), Ext::from_f64(2.0));
            g.working_capital[c] = cost * f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(1.0));
        }
        let rs = cast::<u32, _>(r.next());
        let Some(t) = step(&format!("HostileMerger seed {seed}"), HOSTILE_MERGER, &g, rs, &[]) else { return };
        if t.changed_field("AiHostileMergerQtr") {
            reached.fetch_add(1, Relaxed);
        }
        if t.changed_field("BankDebt") || t.changed_field("WorkingCapital") {
            buyer.fetch_add(1, Relaxed);
        }
    });
    if harness() {
        let (a, b) = (reached.into_inner(), buyer.into_inner());
        eprintln!("HostileMerger: {a}{__wsr_text_13567}{b} with a buyer",__wsr_text_13567=__content.string(13567));
        assert!(a >= 4 && b >= 1, "{__wsr_text_13568}{a}, buyer {b}",__wsr_text_13568=__content.string(13568));
    }
}

// --------------------------------------------------------------------------- IssueStockOffering

#[test]
fn diff_issue_stock_offering() {
    let __content = wsr_core::content::current();
    let offerings = AtomicU32::new(0);
    par_seeds(1..=32, |seed| {
        let mut g = ai_world(seed + 7000);
        let mut r = Rand(seed * 15_485_863 + 5);
        g.market_valuation = cast::<f32, _>([Ext::from_f64(0.8), Ext::from_f64(0.95), Ext::from_f64(1.0), Ext::from_f64(1.2)][cast::<usize, _>(r.range(0, 3))]);
        let p = g.cur_player;
        let main = cast::<i32, _>(g.hold_stock_id[(1, p)]);
        if main > 10 && seed % 2 == 0 {
            let v = cast::<i32, _>(r.range(60, 100));
            own(&mut g, &mut r, p, main, v, p);
            if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) {
                g.market_cap[main] = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(3000.0));
            }
        }
        for i in 1..=10 {
            g.recent_offerings[i] = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { main } else { cast::<i32, _>(r.range(-1600, 1600)) };
        }
        // controlled companies with 25%+ holders
        for _ in 0..r.range(0, 4) {
            let c = corp_in(&g, &mut r, 131, 1580, &[main]);
            let v = cast::<i32, _>(r.range(25, 100));
            own(&mut g, &mut r, p, c, v, p);
            g.credit_rating_num[c] = cast::<i32, _>(r.range(1, 9));
        }
        let rs = cast::<u32, _>(r.next());
        let Some(t) = step(&format!("IssueStockOffering seed {seed}"), ISSUE_OFFERING, &g, rs, &[]) else { return };
        if t.changed_field("TurnActionCount") {
            offerings.fetch_add(1, Relaxed);
        }
    });
    if harness() {
        let n = offerings.into_inner();
        eprintln!("IssueStockOffering: {n}{__wsr_text_13569}",__wsr_text_13569=__content.string(13569));
        assert!(n >= 3, "too few offerings ({n})");
    }
}

// ------------------------------------------------------------------------- ImproveCreditRating

#[test]
fn diff_improve_credit_rating() {
    let restructured = AtomicU32::new(0);
    par_seeds(1..=40, |seed| {
        let mut g = ai_world(seed + 8000);
        let mut r = Rand(seed * 32_452_843 + 7);
        let p = g.cur_player;
        g.difficulty_level = cast::<i32, _>(r.range(1, 3));
        g.ai_credit_fix_quarter = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { g.cur_quarter } else { 0 };
        let main = cast::<i32, _>(g.hold_stock_id[(1, p)]);
        if main > 10 {
            g.profit_margin[main] = cast::<i16, _>(r.range(-20, 20));
            g.credit_rating_num[main] = cast::<i32, _>(r.range(3, 10));
        }
        for _ in 0..r.range(0, 5) {
            let c = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { active_in(&g, &mut r, 86, 130) } else { corp_in(&g, &mut r, 131, 1580, &[]) };
            g.corp_controller[c] = p;
            g.profit_margin[c] = cast::<i16, _>(r.range(-20, 15));
            g.credit_rating_num[c] = cast::<i32, _>(r.range(3, 10));
        }
        let rs = cast::<u32, _>(r.next());
        let Some(t) = step(&format!("ImproveCreditRating seed {seed}"), IMPROVE_CREDIT, &g, rs, &[]) else { return };
        if t.changed_field("ProfitMargin") {
            restructured.fetch_add(1, Relaxed);
        }
    });
    if harness() {
        let n = restructured.into_inner();
        eprintln!("ImproveCreditRating: {n} margin changes");
        assert!(n >= 4, "too few restructurings ({n})");
    }
}

// --------------------------------------------------------------------------- BuyoutPublicShares

#[test]
fn diff_buyout_public_shares() {
    let buyouts = AtomicU32::new(0);
    par_seeds(1..=32, |seed| {
        let mut g = ai_world(seed + 9000);
        let mut r = Rand(seed * 49_979_687 + 9);
        g.difficulty_level = cast::<i32, _>(r.range(1, 3));
        let p = g.cur_player;
        let main = corp_in(&g, &mut r, 131, 1580, &[]);
        clear_portfolio(&mut g, p);
        // no public float: the player and a controlled company hold everything
        let pct = if seed % 5 == 4 { 100 } else { cast::<i32, _>(r.range(50, 95)) };
        own(&mut g, &mut r, p, main, pct, p);
        if pct < 100 && seed % 4 != 3 {
            let sub = corp_in(&g, &mut r, 131, 1580, &[main]);
            g.corp_controller[sub] = p;
            add_holding(&mut g, &mut r, sub, main, 100 - pct);
        }
        g.market_cap[main] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(50.0));
        g.cash[p] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(3000.0));
        if seed % 3 == 0 {
            g.business_assets[main] = f64r(&mut r, Ext::from_f64(3000.0), Ext::from_f64(20000.0));
            g.cash[main] = f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(5000.0));
        }
        for i in 1..=10 {
            g.recent_offerings[i] = if r.chance(cast::<f64, _>(Ext::from_f64(0.05))) { main } else { cast::<i32, _>(r.range(-1600, 1600)) };
        }
        let rs = cast::<u32, _>(r.next());
        let Some(t) = step(&format!("BuyoutPublicShares seed {seed}"), BUYOUT_PUBLIC, &g, rs, &[])
        else {
            return;
        };
        if t.changed_field("RecentOfferings") {
            buyouts.fetch_add(1, Relaxed);
        }
    });
    if harness() {
        let n = buyouts.into_inner();
        eprintln!("BuyoutPublicShares: {n} buyouts");
        assert!(n >= 3, "too few buyouts ({n})");
    }
}

// ------------------------------------------------------------------------------ MergeTaxLossSub

#[test]
fn diff_merge_tax_loss_sub() {
    let merges = AtomicU32::new(0);
    par_seeds(1..=24, |seed| {
        let mut g = ai_world(seed + 10_000);
        let mut r = Rand(seed * 86_028_121 + 11);
        let p = g.cur_player;
        let main = corp_in(&g, &mut r, 131, 1580, &[]);
        clear_portfolio(&mut g, p);
        let v = cast::<i32, _>(r.range(45, 90));
        own(&mut g, &mut r, p, main, v, p);
        g.tax_loss_carry[main] = -f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(9000.0)) * g.exchange_rate;
        g.market_cap[main] = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(3000.0));
        clear_portfolio(&mut g, main);
        g.cpu_max_corps = cast::<i32, _>(r.range(3, 30));
        g.player_corps_controlled[p] = cast::<i32, _>(r.range(0, 30));
        for _ in 0..r.range(2, 8) {
            let c = corp_in(&g, &mut r, 131, 1580, &[main]);
            clear_holders(&mut g, c);
            let v = cast::<i32, _>(r.range(5, 90));
            add_holding(&mut g, &mut r, main, c, v);
            g.corp_controller[c] = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { p } else { c };
            g.market_cap[c] = f64r(&mut r, Ext::from_f64(10.0), Ext::from_f64(2000.0));
            g.tax_loss_carry[c] = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { Ext::from_f64(0.0) } else { -f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(100.0)) };
        }
        let rs = cast::<u32, _>(r.next());
        let Some(t) = step(&format!("MergeTaxLossSub seed {seed}"), MERGE_TAX_LOSS, &g, rs, &[]) else { return };
        merges.fetch_add(cast::<u32, _>(skipped(&t, "MergeCompanies")), Relaxed);
    });
    if harness() {
        let n = merges.into_inner();
        eprintln!("MergeTaxLossSub: {n} merges");
        assert!(n >= 3, "too few merges ({n})");
    }
}

// ------------------------------------------------------------------------------- AntitrustScan

#[test]
fn diff_antitrust_scan() {
    let __content = wsr_core::content::current();
    let divested = AtomicU32::new(0);
    par_seeds(1..=24, |seed| {
        let mut g = ai_world(seed + 11_000);
        let mut r = Rand(seed * 179_424_673 + 13);
        g.private_antitrust_off = if seed % 9 == 8 { 1 } else { 0 };
        let p = g.cur_player;
        // the player's holding company `hold` controls the big companies of one industry
        let hold = corp_in(&g, &mut r, 131, 1580, &[]);
        clear_portfolio(&mut g, p);
        own(&mut g, &mut r, p, hold, 60, p);
        clear_portfolio(&mut g, hold);
        let ind = loop {
            let i = cast::<i16, _>(r.range(3, 68));
            if i != g.industry_num[hold] {
                break i;
            }
        };
        for c in 131..=1600 {
            if g.industry_num[c] == ind {
                g.business_assets[c] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(20.0));
            }
        }
        let n = if seed % 5 == 4 { 1 } else { cast::<i32, _>(r.range(2, 4)) };
        for _ in 0..n {
            let c = corp_in(&g, &mut r, 131, 1580, &[hold]);
            g.industry_num[c] = ind;
            g.business_assets[c] = f64r(&mut r, Ext::from_f64(200.0), Ext::from_f64(5000.0)) * g.exchange_rate;
            let v = cast::<i32, _>(r.range(51, 80));
            own(&mut g, &mut r, hold, c, v, p);
        }
        let rs = cast::<u32, _>(r.next());
        let Some(t) = step(&format!("AntitrustScan seed {seed}"), ANTITRUST_SCAN, &g, rs, &[]) else { return };
        if t.ui.news.iter().any(|n| n.contains(__content.string(13563))) || instr_news(&t.orig.globals) {
            divested.fetch_add(1, Relaxed);
        }
    });
    if harness() {
        let n = divested.into_inner();
        eprintln!("AntitrustScan: {n}{__wsr_text_13570}",__wsr_text_13570=__content.string(13570));
        assert!(n >= 4, "too few divestitures ({n})");
    }
}
