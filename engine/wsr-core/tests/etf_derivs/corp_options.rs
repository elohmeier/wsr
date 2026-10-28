//! Differential tests of `logic::etf_derivs::corp_options`: ForceInsiderShortCover,
//! VestExecutiveOptions, GrantExecutiveOptions, AiCorpOptionsTrading, ForceCloseDerivativesBadCredit
//! and ForceCloseExcessNakedOptions.

#![allow(clippy::manual_is_multiple_of)]
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use wsr_core::logic::etf_derivs::corp_options::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

const FORCE_INSIDER_SHORT_COVER: u32 = 0x5e319d;
const VEST_EXEC_OPTIONS: u32 = 0x5fd441;
const GRANT_EXEC_OPTIONS: u32 = 0x5fc7c1;
const AI_CORP_OPTIONS: u32 = 0x5fdd2a;
const FORCE_CLOSE_BAD_CREDIT: u32 = 0x4c4033;
const FORCE_CLOSE_EXCESS_NAKED: u32 = 0x5f92e9;
/// No unported callees.
const SKIPS: &[(u32, u32)] = &[];

fn rng0(seed: u64) -> u32 {
    (cast::<u32, _>(seed)).wrapping_mul(2_654_435_761) ^ 0x7f4a_7c15
}

#[track_caller]
fn check(what: &str, port: &Globals, port_rng: &Rng, ui: &Rec, o: &Original) {
    check_with(what, port, port_rng, ui, o, &[]);
}

/// [`check`], tolerating error 9 in the routines `allowed_err9` (see [`check_errs`]).
#[track_caller]
fn check_with(what: &str, port: &Globals, port_rng: &Rng, ui: &Rec, o: &Original, allowed_err9: &[&str]) {
    let __content = wsr_core::content::current();
    if std::env::var("DBG").is_ok() {
        for l in o.log.iter().filter(|l| !l.starts_with("API")) {
            eprintln!("{what}: {l}");
        }
        eprintln!("{what}: port msgs {:?}{__wsr_text_13611}{:?}", ui.msgs, ui.news,__wsr_text_13611=__content.string(13611));
    }
    let mut port = port.clone();
    align_option_premiums(&mut port, &o.globals);
    let d = cmp().rel_tol(Ext::from_f64(1e-9)).diffs(&port, &o.globals);
    let log: Vec<&String> = o.log.iter().filter(|l| !l.starts_with("API")).take(20).collect();
    assert!(
        d.is_empty(),
        "{what}: {} differences:\n{}\nlog: {log:?}",
        d.len(),
        d.iter().take(30).cloned().collect::<Vec<_>>().join("\n")
    );
    assert_eq!(port_rng.state(), o.rng, "{what}: RND state");
    check_errs(what, o, allowed_err9);
    assert_eq!(ui.msgs.len(), orig_msgs(o), "{what}: message boxes {:?} vs log {log:?}", ui.msgs);
}

/// No runtime errors in the original, except error 9 in the routines listed in `allowed` (`"0x…"`;
/// routines without an ON ERROR handler continue on the arrays' dummy elements, like the port).
#[track_caller]
fn check_errs(what: &str, o: &Original, allowed: &[&str]) {
    let errs: Vec<&String> = o
        .log
        .iter()
        .filter(|l| l.starts_with("PBERR") && !allowed.iter().any(|a| l.starts_with(&format!("PBERR 9 in {a} "))))
        .collect();
    assert!(errs.is_empty(), "{what}: runtime errors in the original: {errs:?}");
}

/// Option premiums are computed in 80-bit precision; the port's `calc_option_price` returns f64, so the
/// STR$ of a premium (last field of an option record) can differ in the 15th digit. Takes the original's
/// record where only that field differs, by at most 1e-12 relative.
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
        let (x, y): (Ext, Ext) = (
            pa[n - 1].trim().parse().unwrap_or(Ext::NAN),
            pb[n - 1].trim().parse().unwrap_or(Ext::NAN),
        );
        if (x - y).abs() <= Ext::from_f64(1e-12) * y.abs() {
            port.option_recs[i] = orig.option_recs[i].clone();
        }
    }
}

fn active_corp(g: &Globals, r: &mut Rand) -> i32 {
    for _ in 0..100 {
        let c = cast::<i32, _>(r.range(131, 1580));
        if g.entity_status[c] == 1 && g.industry_num[c] < 70 {
            return c;
        }
    }
    131
}

/// `holder` gets `pct` of `corp` (a new slot, replacing slot 15 if full).
fn give_holding(g: &mut Globals, holder: i32, corp: i32, pct: i16) {
    let mut slot = (1..=15).find(|&s| cast::<i32, _>(g.hold_stock_id[(s, holder)]) == corp);
    if slot.is_none() {
        slot = (1..=15).find(|&s| g.hold_stock_id[(s, holder)] == 0);
    }
    let s = slot.unwrap_or(15);
    g.hold_stock_id[(s, holder)] = cast::<i16, _>(corp);
    g.hold_stock_pct[(s, holder)] = pct;
    g.hold_stock_basis[(s, holder)] = g.market_cap[corp] * Ext::from_f64(0.9);
    g.hold_stock_id[(0, holder)] = cast::<i16, _>((1..=15).filter(|&k| g.hold_stock_id[(k, holder)] != 0).count());
}

/// Smaller stakes everywhere (so that one big holder controls), prices of 5..900 per share.
fn calm_world(seed: u64) -> Globals {
    let mut g = world(seed);
    let mut r = Rand(seed * 977 + 3);
    if g.start_year == g.cur_year && g.cur_quarter == 1 {
        g.start_year -= 1;
    }
    for e in 131..=1600 {
        if r.chance(Ext::from_f64(0.9)) {
            g.shares_outstanding[e] = g.market_cap[e] / f64r(&mut r, Ext::from_f64(5.0), Ext::from_f64(900.0));
        }
    }
    for e in 1..=1600 {
        for slot in 1..=15 {
            if g.hold_stock_pct[(slot, e)] > 12 && r.chance(Ext::from_f64(0.85)) {
                g.hold_stock_pct[(slot, e)] = cast::<i16, _>(r.range(1, 12));
            }
        }
    }
    g
}

/// Random option records (some executive GRANT / VESTED ones) on `corps` (or random corps).
fn add_options(g: &mut Globals, r: &mut Rand, n: i32, holders: &[i32], corps: &[i32]) {
    for i in 1..=n {
        let corp = if !corps.is_empty() && r.chance(Ext::from_f64(0.6)) {
            corps[cast::<usize, _>(r.range(0, cast::<i64, _>(corps.len()) - 1))]
        } else {
            cast::<i32, _>(r.range(131, 1580))
        };
        let holder = if !holders.is_empty() && r.chance(Ext::from_f64(0.6)) {
            holders[cast::<usize, _>(r.range(0, cast::<i64, _>(holders.len()) - 1))]
        } else {
            cast::<i32, _>(r.range(1, 1600))
        };
        let mut qty = if r.chance(Ext::from_f64(0.5)) { r.range(1, 20) } else { -r.range(1, 20) };
        let mut typ = r.range(1, 2);
        let strike = g.market_cap[corp] * f64r(r, Ext::from_f64(0.7), Ext::from_f64(1.3));
        let month = r.range(1, 12);
        let year = g.cur_year + cast::<i32, _>(r.range(0, 2));
        let last = match r.range(0, 9) {
            // executive options are long calls of a player
            0 | 1 if holder <= 5 => {
                typ = 1;
                qty = qty.abs();
                if r.chance(Ext::from_f64(0.5)) { "GRANT".to_string() } else { "VESTED".to_string() }
            }
            _ => format!("{:.4}", f64r(r, Ext::from(Ext::from_f64(0.0)), Ext::from(Ext::from_f64(50.0)))),
        };
        g.option_recs[i] = PbStr::from(
            format!("{corp}|{typ}|{holder}|{qty}|{strike:.4}|{month}|{year}|{last}").as_str(),
        );
    }
    g.option_recs[0] = PbStr::from(n.to_string().as_str());
}

// ------------------------------------------------------------------------------ ForceInsiderShortCover

#[test]
fn diff_force_insider_short_cover() {
    let __content = wsr_core::content::current();
    static HITS: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=16, |seed| {
        let mut g = calm_world(seed + 100);
        let mut r = Rand(seed * 31 + 9);
        for p in 1..=g.num_players {
            g.player_short_margin_bal[p] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(5000.0));
            g.player_cap_gains[p] = f64r(&mut r, -Ext::from_f64(100.0), Ext::from_f64(500.0));
            for _ in 0..r.range(1, 4) {
                let target = active_corp(&g, &mut r);
                if (1..=15).any(|s| cast::<i32, _>(g.hold_stock_id[(s, p)]) == target) {
                    continue;
                }
                give_holding(&mut g, p, target, -(cast::<i16, _>(r.range(1, 8))));
                if r.chance(Ext::from_f64(0.6)) {
                    // the player controls the target through a corp holding 60%
                    let parent = active_corp(&g, &mut r);
                    if parent != target {
                        g.corp_controller[parent] = p;
                        give_holding(&mut g, p, parent, 55);
                        give_holding(&mut g, parent, target, 60);
                        g.corp_controller[target] = p;
                    }
                }
            }
        }
        if seed % 5 == 0 {
            g.player_name[g.num_players] = st("...OUT...");
        }
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        force_insider_short_cover(&mut port, &mut ui, &mut rng);
        let Some(o) = run(FORCE_INSIDER_SHORT_COVER, &[], &g, rs, &[], SKIPS) else { return };
        check(&format!("ForceInsiderShortCover seed {seed}"), &port, &rng, &ui, &o);
        HITS.fetch_add(ui.news.iter().filter(|n| n.contains(__content.string(13771))).count(), Ordering::Relaxed);
    });
    if unavailable().is_none() {
        assert!(HITS.load(Ordering::Relaxed) > 3, "insider covers hit");
    }
}

// -------------------------------------------------------------------------------- VestExecutiveOptions

#[test]
fn diff_vest_executive_options() {
    let __content = wsr_core::content::current();
    static VESTED: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=16, |seed| {
        let mut g = calm_world(seed + 200);
        let mut r = Rand(seed * 37 + 1);
        let players: Vec<i32> = (1..=g.num_players).collect();
        for p in 1..=5 {
            g.player_ceo_of[p] = cast::<i32, _>(r.range(131, 1580));
        }
        let n = if seed % 8 == 0 { 0 } else { cast::<i32, _>(r.range(5, 40)) };
        add_options(&mut g, &mut r, n, &players, &[]);
        // grants of this year/last year
        for i in 1..=n {
            if r.chance(Ext::from_f64(0.3)) {
                let holder = if seed % 3 == 0 { 1 } else { r.range(1, cast::<i64, _>(g.num_players)) };
                // mostly the holder's own company (else RevokeExCeoOptions deletes the grant)
                let corp = if r.chance(Ext::from_f64(0.8)) { cast::<i64, _>(g.player_ceo_of[cast::<i32, _>(holder)]) } else { r.range(131, 1580) };
                let year = g.cur_year + cast::<i32, _>(r.range(0, 2));
                g.option_recs[i] = PbStr::from(
                    format!("{corp} | 1| {holder} | 2 | 123.5 | {} | {year} |GRANT", r.range(1, 12)).as_str(),
                );
            }
        }
        if seed % 4 == 0 {
            g.player_ceo_of[1] = 0;
        }
        if r.chance(Ext::from_f64(0.3)) {
            g.news_msg = st(__content.string(13571));
        }
        if seed % 6 == 0 {
            g.game_end_year = g.cur_year + 1;
        }
        let rs = rng0(seed);
        let mut port = g.clone();
        let rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        vest_executive_options(&mut port, &mut ui);
        let Some(o) = run(VEST_EXEC_OPTIONS, &[], &g, rs, &[], SKIPS) else { return };
        check(&format!("VestExecutiveOptions seed {seed}"), &port, &rng, &ui, &o);
        let v = (1..=1000).filter(|&i| port.option_recs[i].to_display().ends_with("VESTED")).count()
            - (1..=1000).filter(|&i| g.option_recs[i].to_display().ends_with("VESTED")).count();
        VESTED.fetch_add(v, Ordering::Relaxed);
    });
    if unavailable().is_none() {
        assert!(VESTED.load(Ordering::Relaxed) > 5, "vesting hit");
    }
}

// ------------------------------------------------------------------------------- GrantExecutiveOptions

#[test]
fn diff_grant_executive_options() {
    let __content = wsr_core::content::current();
    static GRANTED: AtomicUsize = AtomicUsize::new(0);
    static REFUSED: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=40, |seed| {
        let mut g = calm_world(seed + 300);
        let mut r = Rand(seed * 41 + 5);
        for p in 1..=g.num_players {
            let corp = active_corp(&g, &mut r);
            g.player_ceo_of[p] = if r.chance(Ext::from_f64(0.1)) { 0 } else { corp };
            g.player_ceo_since_year[p] = if r.chance(Ext::from_f64(0.15)) { g.cur_year } else { g.cur_year - cast::<i32, _>(r.range(1, 5)) };
            match r.range(0, 3) {
                0 => {}
                1 => give_holding(&mut g, p, corp, cast::<i16, _>(r.range(30, 49))),
                _ => give_holding(&mut g, p, corp, cast::<i16, _>(r.range(51, 80))),
            }
            if r.chance(Ext::from_f64(0.15)) {
                // privately held: 100% held by the player
                give_holding(&mut g, p, corp, 100);
                for e in 11..=1600 {
                    if e != p {
                        for s in 1..=15 {
                            if cast::<i32, _>(g.hold_stock_id[(s, e)]) == corp {
                                g.hold_stock_pct[(s, e)] = 0;
                            }
                        }
                    }
                }
                for q in 1..=5 {
                    if q != p {
                        for s in 1..=15 {
                            if cast::<i32, _>(g.hold_stock_id[(s, q)]) == corp {
                                g.hold_stock_pct[(s, q)] = 0;
                            }
                        }
                    }
                }
            }
            let prev = match r.range(0, 4) {
                0 => Ext::from_f64(0.0),
                1 => -f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(10.0)),
                _ => f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(20.0)),
            };
            let cur = match r.range(0, 4) {
                0 => -f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(10.0)),
                1 => prev * f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(1.0)),
                _ => prev.abs() * f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(1.4)) + Ext::from_f64(0.1),
            };
            g.ent_misc_data[corp] = st(&format!("1|2|{cur:.3}|{prev:.3}|5|6"));
            if r.chance(Ext::from_f64(0.1)) {
                g.player_name[p] = st("A very long player name exceeding twenty");
            }
        }
        g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
        if seed % 7 == 0 {
            g.player_name[g.num_players] = st("...OUT...");
        }
        let players: Vec<i32> = (1..=g.num_players).collect();
        let n = cast::<i32, _>(r.range(0, 30));
        add_options(&mut g, &mut r, n, &players, &[]);
        if seed % 9 == 0 {
            // full option table
            for i in 1..=1000 {
                if g.option_recs[i].is_empty() {
                    g.option_recs[i] = st("131|1|1600|1|10|1|2099|1.0");
                }
            }
        }
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        grant_executive_options(&mut port, &mut ui, &mut rng);
        let Some(o) = run(GRANT_EXEC_OPTIONS, &[], &g, rs, &[], SKIPS) else { return };
        check(&format!("GrantExecutiveOptions seed {seed}"), &port, &rng, &ui, &o);
        GRANTED.fetch_add(ui.news.iter().filter(|n| n.contains(__content.string(4817))).count(), Ordering::Relaxed);
        REFUSED.fetch_add(ui.news.iter().filter(|n| n.contains(__content.string(13772))).count(), Ordering::Relaxed);
        if std::env::var("COV").is_ok() {
            eprintln!("COV grant seed {seed}: {:?}", ui.news);
        }
    });
    if unavailable().is_none() {
        assert!(GRANTED.load(Ordering::Relaxed) > 3, "grants hit");
        assert!(REFUSED.load(Ordering::Relaxed) > 3, "refusals hit");
    }
}

// -------------------------------------------------------------------------------- AiCorpOptionsTrading

#[test]
fn diff_ai_corp_options_trading() {
    let __content = wsr_core::content::current();
    static BOUGHT: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=40, |seed| {
        let mut g = calm_world(seed + 400);
        let mut r = Rand(seed * 43 + 7);
        let e = active_corp(&g, &mut r);
        g.sim_co_count = if seed % 10 == 0 { cast::<i32, _>(r.range(11, 1600)) } else { e };
        // a controlling corp so that it is AI-controlled
        if r.chance(Ext::from_f64(0.8)) {
            let parent = active_corp(&g, &mut r);
            if parent != e {
                give_holding(&mut g, parent, e, 55);
                g.corp_controller[e] = parent;
            }
        }
        // only 45..78% stakes in a few stocks (any other stake picked ends the routine)
        if seed % 5 != 0 {
            for s in 1..=15 {
                g.hold_stock_id[(s, e)] = 0;
                g.hold_stock_pct[(s, e)] = 0;
            }
            g.hold_stock_id[(0, e)] = 0;
        }
        for _ in 0..r.range(1, 4) {
            let t = active_corp(&g, &mut r);
            if t != e {
                give_holding(&mut g, e, t, cast::<i16, _>(r.range(44, 79)));
                if r.chance(Ext::from_f64(0.7)) {
                    g.credit_rating_num[t] = cast::<i32, _>(r.range(7, 10));
                    g.earnings_y1[t] = g.earnings_yr2_ago[t] * Ext::from_f64(1.3) + Ext::from_f64(1.0);
                }
            }
        }
        g.cash[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(30000.0));
        let n = cast::<i32, _>(r.range(0, 30));
        add_options(&mut g, &mut r, n, &[], &[]);
        if seed % 13 == 0 {
            g.option_recs[250] = st("131|1|1600|1|10|1|2099|1.0");
        }
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        ai_corp_options_trading(&mut port, &mut ui, &mut rng);
        let Some(o) = run(AI_CORP_OPTIONS, &[], &g, rs, &[], SKIPS) else { return };
        check(&format!("AiCorpOptionsTrading seed {seed}"), &port, &rng, &ui, &o);
        let before = (1..=1000).filter(|&i| !g.option_recs[i].is_empty()).count();
        let after = (1..=1000).filter(|&i| !port.option_recs[i].is_empty()).count();
        if after > before {
            BOUGHT.fetch_add(1, Ordering::Relaxed);
        }
        if std::env::var("COV").is_ok() {
            eprintln!("COV aicorp seed {seed}: {before}->{after}{__wsr_text_13611}{:?}", ui.news,__wsr_text_13611=__content.string(13611));
        }
    });
    if unavailable().is_none() {
        assert!(BOUGHT.load(Ordering::Relaxed) > 0, "option purchase hit");
    }
}

// ---------------------------------------------------------------------- ForceCloseDerivativesBadCredit

/// Weak corps writing options on stocks they (partly) own, some futures records.
fn bad_credit_world(seed: u64) -> Globals {
    let mut g = calm_world(seed + 500);
    let mut r = Rand(seed * 47 + 11);
    // weak corps writing puts / calls on stocks they (partly) own
    let mut writers = Vec::new();
    let mut corps = Vec::new();
    for _ in 0..6 {
        let w = active_corp(&g, &mut r);
        let c = active_corp(&g, &mut r);
        if w == c {
            continue;
        }
        // a weak balance sheet (CalcEntityFinancials recomputes the rating)
        g.credit_rating_num[w] = cast::<i32, _>(r.range(0, 5));
        g.cash[w] = f64r(&mut r, -Ext::from_f64(500.0), Ext::from_f64(10.0));
        g.t_bills[w] = Ext::from_f64(0.0);
        g.bank_debt[w] = f64r(&mut r, Ext::from_f64(3000.0), Ext::from_f64(9000.0));
        g.business_assets[w] = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(1000.0));
        g.working_capital[w] = f64r(&mut r, -Ext::from_f64(300.0), Ext::from_f64(0.0));
        g.earnings_y1[w] = -f64r(&mut r, Ext::from_f64(10.0), Ext::from_f64(100.0));
        g.earnings_ytd[w] = -f64r(&mut r, Ext::from_f64(10.0), Ext::from_f64(100.0));
        g.proj_qtr_earnings[w] = -f64r(&mut r, Ext::from_f64(5.0), Ext::from_f64(30.0));
        if r.chance(Ext::from_f64(0.5)) {
            give_holding(&mut g, w, c, cast::<i16, _>(r.range(1, 5)));
        }
        writers.push(w);
        corps.push(c);
    }
    let n = if seed % 10 == 0 { 0 } else { cast::<i32, _>(r.range(5, 30)) };
    add_options(&mut g, &mut r, n, &writers, &corps);
    // written options of the writers on their corps
    for i in 1..=n {
        if r.chance(Ext::from_f64(0.5)) && !writers.is_empty() {
            let k = cast::<usize, _>(r.range(0, cast::<i64, _>(writers.len()) - 1));
            let (w, c) = (writers[k], corps[k]);
            let strike = g.market_cap[c] * f64r(&mut r, Ext::from_f64(0.7), Ext::from_f64(1.3));
            g.option_recs[i] = PbStr::from(
                format!(
                    "{c}|{}|{w}|{}|{strike:.4}|{}|{}|{:.4}",
                    r.range(1, 2),
                    -r.range(3, 20),
                    r.range(1, 12),
                    g.cur_year + 1,
                    f64r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(50.0)))
                )
                .as_str(),
            );
        }
    }
    if seed % 3 == 0 {
        // futures of non-bank, non-insurer holders
        let codes = ["OIL", "GOLD", "SILVER", "WHEAT", "CORN"];
        let k = cast::<i32, _>(r.range(1, 12));
        for i in 1..=k {
            let o = if r.chance(Ext::from_f64(0.5)) && !writers.is_empty() {
                writers[cast::<usize, _>(r.range(0, cast::<i64, _>(writers.len()) - 1))]
            } else {
                cast::<i32, _>(r.range(131, 1580))
            };
            let mut qty = r.range(1, 40);
            if r.chance(Ext::from_f64(0.3)) {
                qty = -qty;
            }
            g.futures_recs[i] = st(&format!(
                "{}|{o}|{qty}|{}|{}|{:.2}|{:.2}",
                codes[cast::<usize, _>(r.range(0, 4))],
                r.range(1, 12),
                g.cur_year + cast::<i32, _>(r.range(0, 2)),
                f64r(&mut r, Ext::from(Ext::from_f64(10.0)), Ext::from(Ext::from_f64(2000.0))),
                f64r(&mut r, Ext::from(Ext::from_f64(2.0)), Ext::from(Ext::from_f64(2000.0)))
            ));
        }
        g.futures_recs[0] = st(&k.to_string());
    }
    g.sel_entity = if seed % 2 == 0 && !writers.is_empty() { writers[0] } else { cast::<i32, _>(r.range(0, 1600)) };
    g
}

#[test]
fn diff_force_close_derivatives_bad_credit() {
    let __content = wsr_core::content::current();
    static OPT_NEWS: AtomicUsize = AtomicUsize::new(0);
    static OPT_CLOSED: AtomicUsize = AtomicUsize::new(0);
    static FUT_CLOSED: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=60, |seed| {
        let g = bad_credit_world(seed);
        let mode = if seed % 4 == 0 { 2 } else { 1 };
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        force_close_derivatives_bad_credit(&mut port, &mut ui, &mut rng, mode);
        let Some(o) = run(FORCE_CLOSE_BAD_CREDIT, &[&mode.to_string()], &g, rs, &[], SKIPS) else { return };
        // futures-picked entities leave optHolder = 0 (BUGS.md #175): error 9 on EntSymbol(0),
        // CorpController(0) and EntityStatus(0) in GetNetStockPctAfterOptions, no ON ERROR in either
        let what = format!("ForceCloseDerivativesBadCredit seed {seed}{__wsr_text_13548}{mode}",__wsr_text_13548=__content.string(13548));
        check_with(&what, &port, &rng, &ui, &o, &["0x4c4033", "0x608a27"]);
        OPT_NEWS.fetch_add(ui.news.iter().filter(|n| n.contains(__content.string(13773))).count(), Ordering::Relaxed);
        if (1..=1000).any(|i| port.option_recs[i] != g.option_recs[i]) {
            OPT_CLOSED.fetch_add(1, Ordering::Relaxed);
        }
        if (1..=1000).any(|i| port.futures_recs[i] != g.futures_recs[i]) {
            FUT_CLOSED.fetch_add(1, Ordering::Relaxed);
        }
        if std::env::var("COV").is_ok() {
            eprintln!("COV badcredit seed {seed}: {:?} opt {} fut {}", ui.news, OPT_CLOSED.load(Ordering::Relaxed), FUT_CLOSED.load(Ordering::Relaxed));
        }
    });
    if unavailable().is_none() {
        assert!(OPT_NEWS.load(Ordering::Relaxed) > 0, "option close news hit");
        assert!(OPT_CLOSED.load(Ordering::Relaxed) > 3, "options closed");
        assert!(FUT_CLOSED.load(Ordering::Relaxed) > 0, "futures closed");
    }
}

// ----------------------------------------------------------------------- ForceCloseExcessNakedOptions

#[test]
fn diff_force_close_excess_naked_options() {
    let __content = wsr_core::content::current();
    static CLOSED: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=40, |seed| {
        let mut g = calm_world(seed + 600);
        let mut r = Rand(seed * 53 + 13);
        let corp = active_corp(&g, &mut r);
        let holder = match seed % 4 {
            0 => 0,
            1 => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
            _ => active_corp(&g, &mut r),
        };
        let h = if holder == 0 {
            let c = if r.chance(Ext::from_f64(0.7)) { cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))) } else { active_corp(&g, &mut r) };
            g.corp_controller[corp] = c;
            if r.chance(Ext::from_f64(0.5)) {
                g.sim_co_count = corp;
            }
            c
        } else {
            holder
        };
        if h > 130 && r.chance(Ext::from_f64(0.7)) {
            g.corp_controller[h] = g.corp_controller[corp];
        }
        if r.chance(Ext::from_f64(0.6)) {
            give_holding(&mut g, h, corp, cast::<i16, _>(r.range(0, 6)));
        }
        // the holder's options on the corp: long puts / written calls covering several percent
        let n = cast::<i32, _>(r.range(3, 25));
        add_options(&mut g, &mut r, n, &[h], &[corp]);
        for i in 1..=n {
            if r.chance(Ext::from_f64(0.5)) {
                let (typ, qty) = if r.chance(Ext::from_f64(0.5)) { (2, r.range(1, 6)) } else { (1, -r.range(1, 6)) };
                let strike = g.market_cap[corp] * f64r(&mut r, Ext::from_f64(0.7), Ext::from_f64(1.3));
                g.option_recs[i] = PbStr::from(
                    format!("{corp}|{typ}|{h}|{qty}|{strike:.4}|{}|{}|{:.4}", r.range(1, 12), g.cur_year + 1, f64r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(50.0))))
                        .as_str(),
                );
            }
        }
        if h <= 10 {
            g.player_short_margin_bal[h] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(5000.0));
        }
        if r.chance(Ext::from_f64(0.3)) {
            g.news_msg = st(__content.string(13571));
        }
        g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        force_close_excess_naked_options(&mut port, &mut ui, &mut rng, holder, corp);
        let args = [holder.to_string(), corp.to_string()];
        let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let Some(o) = run(FORCE_CLOSE_EXCESS_NAKED, &args, &g, rs, &[], SKIPS) else { return };
        check(&format!("ForceCloseExcessNakedOptions seed {seed}{__wsr_text_13774}{holder} corp {corp}",__wsr_text_13774=__content.string(13774)), &port, &rng, &ui, &o);
        let before = (1..=1000).filter(|&i| !g.option_recs[i].is_empty()).count();
        let after = (1..=1000).filter(|&i| !port.option_recs[i].is_empty()).count();
        if after < before {
            CLOSED.fetch_add(1, Ordering::Relaxed);
        }
        if std::env::var("COV").is_ok() {
            eprintln!("COV naked seed {seed}: {before}->{after} {:?} msgs {:?}", ui.news, ui.msgs);
        }
    });
    if unavailable().is_none() {
        assert!(CLOSED.load(Ordering::Relaxed) > 3, "closing hit");
    }
}

// ------------------------------------------- ForceRegulatedCallSales (caller of ForceCloseExcessNaked…)

/// ForceRegulatedCallSales (0x5f8e12, logic::sim_entity::bonds) now calls the ForceCloseExcessNakedOptions
/// port for corporate option positions.
#[test]
fn diff_force_regulated_call_sales_wiring() {
    use wsr_core::logic::sim_entity::bonds::force_regulated_call_sales;
    static CLOSED: AtomicUsize = AtomicUsize::new(0);
    par_seeds(1..=24, |seed| {
        let mut g = calm_world(seed + 700);
        let mut r = Rand(seed * 59 + 17);
        let mut holders = Vec::new();
        let mut corps = Vec::new();
        for _ in 0..5 {
            let h = if r.chance(Ext::from_f64(0.3)) { cast::<i32, _>(r.range(11, 130)) } else { active_corp(&g, &mut r) };
            let c = active_corp(&g, &mut r);
            if h == c {
                continue;
            }
            if r.chance(Ext::from_f64(0.6)) {
                g.corp_controller[h] = g.corp_controller[c];
            }
            holders.push(h);
            corps.push(c);
        }
        let n = cast::<i32, _>(r.range(4, 30));
        add_options(&mut g, &mut r, n, &holders, &corps);
        for i in 1..=n {
            if r.chance(Ext::from_f64(0.5)) && !holders.is_empty() {
                let k = cast::<usize, _>(r.range(0, cast::<i64, _>(holders.len()) - 1));
                let (h, c) = (holders[k], corps[k]);
                let (typ, qty) = if r.chance(Ext::from_f64(0.5)) { (2, r.range(1, 6)) } else { (1, -r.range(1, 6)) };
                let strike = g.market_cap[c] * f64r(&mut r, Ext::from_f64(0.7), Ext::from_f64(1.3));
                g.option_recs[i] = PbStr::from(
                    format!("{c}|{typ}|{h}|{qty}|{strike:.4}|{}|{}|{:.4}", r.range(1, 12), g.cur_year + 1, f64r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(50.0))))
                        .as_str(),
                );
            }
        }
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        force_regulated_call_sales(&mut port, &mut ui, &mut rng);
        let Some(o) = run(0x5f8e12, &[], &g, rs, &[], SKIPS) else { return };
        let what = format!("ForceRegulatedCallSales seed {seed}");
        check(&what, &port, &rng, &ui, &o);
        let before = (1..=1000).filter(|&i| !g.option_recs[i].is_empty()).count();
        let after = (1..=1000).filter(|&i| !port.option_recs[i].is_empty()).count();
        if after < before {
            CLOSED.fetch_add(1, Ordering::Relaxed);
        }
    });
    if unavailable().is_none() {
        assert!(CLOSED.load(Ordering::Relaxed) > 2, "closing hit");
    }
}
