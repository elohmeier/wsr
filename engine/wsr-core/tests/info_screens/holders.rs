//! Differential tests of the holder lists (ListFuturesOrPhysHolders 0x670040, ListSwapHolders 0x671001,
//! ListOptionHolders 0x671d38, ListStockCrossHoldings 0x672677), the streaming-quotes list buttons
//! (FillStreamingListWithHoldings 0x5dec84, ClearStreamingQuotesList 0x5dea80) and the research
//! report's rating click (ReportDlg_RatingClick 0x4188bf).
use wsr_core::pb::real::{Ext, cast};


use super::common::{Rec, cmp, orig_msgs, run, st, world};
use super::diff::{Original, Rand, par_seeds};
use std::sync::Mutex;
use wsr_core::logic::info_screens::holders::*;
use wsr_core::logic::newgame::LocalTime;
use wsr_core::logic::shell::Env;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

const LIST_FUTURES_OR_PHYS: u32 = 0x670040;
const LIST_SWAPS: u32 = 0x671001;
const LIST_OPTIONS: u32 = 0x671d38;
const LIST_CROSS: u32 = 0x672677;
const FILL_STREAM: u32 = 0x5dec84;
const CLEAR_STREAM: u32 = 0x5dea80;
const RATING_CLICK: u32 = 0x4188bf;
const DO_TRANSACTION_MENU_ACTION: u32 = 0x552f0b;

const COMMODS: [&str; 8] = ["OIL", "GOLD", "SILVER", "WHEAT", "CORN", "STOCKS", "BITCOIN", "ETHEREUM"];

/// Report dialogs of the original as (title, listbox 301 items).
fn orig_reports(o: &Original) -> Vec<(PbStr, Vec<PbStr>)> {
    o.dialogs.iter().filter_map(|d| d.control(301).map(|c| (d.title.clone(), c.items.clone()))).collect()
}

/// The port's reports as the original's listbox shows them (the `@END` line becomes " ").
fn port_reports(ui: &Rec) -> Vec<(PbStr, Vec<PbStr>)> {
    ui.reports
        .iter()
        .map(|(t, l)| {
            let mut l = l.clone();
            l.push(st(" "));
            (t.clone(), l)
        })
        .collect()
}

#[track_caller]
fn assert_reports(seed: u64, port: &[(PbStr, Vec<PbStr>)], orig: &[(PbStr, Vec<PbStr>)]) {
    let __content = wsr_core::content::current();
    assert_eq!(port.len(), orig.len(), "seed {seed}: number of reports (port vs original)");
    for ((pt, pl), (ot, ol)) in port.iter().zip(orig) {
        assert_eq!(pt, ot, "seed {seed}: report title");
        for (k, (a, b)) in pl.iter().zip(ol).enumerate() {
            assert_eq!(a, b, "seed {seed}: report {:?}{__wsr_text_13606}{}", pt.to_display(), k + 1,__wsr_text_13606=__content.string(13606));
        }
        assert_eq!(pl.len(), ol.len(), "seed {seed}: report {:?}: number of lines", pt.to_display());
    }
}

/// MSGBOX answer: (harness name, Ui result).
fn answer(r: &mut Rand) -> (&'static str, i32) {
    match r.range(0, 9) {
        0 => ("IDCANCEL", 2),
        1..=4 => ("IDNO", 7),
        _ => ("IDYES", 6),
    }
}

/// Runs the port and the original and compares state, RND, messages and reports. Returns the port's Ui.
#[allow(clippy::too_many_arguments)]
fn check(
    seed: u64,
    addr: u32,
    args: &[&str],
    g: &Globals,
    answers: &[(&str, i32)],
    skips: &[(u32, u32)],
    port: impl Fn(&mut Globals, &mut Rec, &mut Rng),
) -> Option<(Rec, Original)> {
    let rng0 = cast::<u32, _>(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 20);
    let oa: Vec<&str> = answers.iter().map(|a| a.0).collect();
    let o = run(addr, args, g, rng0, &oa, skips)?;
    let mut pg = g.clone();
    let mut ui = Rec::default();
    ui.answers.extend(answers.iter().map(|a| a.1));
    let mut rng = Rng::from_state(rng0);
    port(&mut pg, &mut ui, &mut rng);
    assert_reports(seed, &port_reports(&ui), &orig_reports(&o));
    assert_eq!(ui.msgs.len(), orig_msgs(&o), "seed {seed}: messages: port {:?}, log {:?}", ui.titles, o.log);
    assert_eq!(rng.state(), o.rng, "seed {seed}: RND state");
    let d = cmp().diffs(&pg, &o.globals);
    assert!(d.is_empty(), "seed {seed}: {} differences:\n{}", d.len(), d.iter().take(40).cloned().collect::<Vec<_>>().join("\n"));
    Some((ui, o))
}

/// A world for the lists: SelEntity 0 (no market tick around the report), some companies controlled
/// by the current player, a player symbol longer than 6 characters.
fn list_world(seed: u64, r: &mut Rand) -> Globals {
    let mut g = world(seed);
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    g.sel_entity = 0;
    for e in 11..=1600 {
        if r.chance(Ext::from_f64(0.05)) {
            g.corp_controller[e] = g.cur_player;
        }
    }
    for p in 1..=5 {
        if r.chance(Ext::from_f64(0.5)) {
            g.ent_symbol[p] = st(["PLAYER1X", "ABCDEFG", "XY", "QWERTZ"][cast::<usize, _>(r.range(0, 3))]);
        }
    }
    g
}

/// A holder: the current player, another player, or a company (sometimes one the player controls).
fn holder(r: &mut Rand, g: &Globals) -> i32 {
    match r.range(0, 5) {
        0 => g.cur_player,
        1 => cast::<i32, _>(r.range(1, 5)),
        2 => (11..=1600).find(|&e| g.corp_controller[e] == g.cur_player && r.chance(Ext::from_f64(0.1))).unwrap_or(500),
        _ => cast::<i32, _>(r.range(11, 1600)),
    }
}

fn f64r(r: &mut Rand, lo: Ext, hi: Ext) -> Ext {
    lo + (hi - lo) * cast::<Ext, _>(r.range(0, 1_000_000_000)) / Ext::from_f64(1e9)
}

#[test]
fn list_futures_or_phys_holders_matches_original() {
    let __content = wsr_core::content::current();
    let hits = Mutex::new([0usize; 6]); // report, cancel, no-records popup, none-found popup, type 2 report, only-mine
    par_seeds(1u64..=40, |seed| {
        let mut r = Rand(seed * 7919 + 1);
        let mut g = list_world(seed, &mut r);
        let list_type = if seed % 2 == 0 { 1 } else { 2 };
        let n = if seed % 9 == 0 { 0 } else if seed % 7 == 0 { 2 } else { cast::<i32, _>(r.range(1, 80)) };
        for i in 1..=n {
            let c = COMMODS[cast::<usize, _>(r.range(0, 7))];
            let owner = holder(&mut r, &g);
            let qty = r.range(1, 60000) * if r.chance(Ext::from_f64(0.4)) { -1 } else { 1 };
            let year = g.cur_year + cast::<i32, _>(r.range(0, 3));
            let cost = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(20000.0));
            let price = if r.chance(Ext::from_f64(0.2)) { f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(99999.0)) } else { f64r(&mut r, Ext::from_f64(0.01), Ext::from_f64(999.0)) };
            let slot = cast::<i32, _>(if r.chance(Ext::from_f64(0.2)) { r.range(1, 1000) } else { cast::<i64, _>(i) });
            g.futures_recs[slot] =
                st(&format!("{c}|{owner}|{qty}|{}|{year}|{cost:.4}|{price:.3}", r.range(1, 12)));
            let units = r.range(1, 50000);
            g.phys_commod_rec[slot] = st(&format!("{c}|{owner}|{units}|{:.2}", f64r(&mut r, Ext::from(Ext::from_f64(0.5)), Ext::from(Ext::from_f64(900000.0)))));
        }
        g.futures_recs[0] = st(&n.to_string());
        g.phys_commod_rec[0] = st(&n.to_string());
        let a = answer(&mut r);
        let lt = list_type.to_string();
        let Some((ui, _)) = check(seed, LIST_FUTURES_OR_PHYS, &[&lt], &g, &[a], &[], |g, ui, rng| {
            list_futures_or_phys_holders(g, ui, rng, list_type)
        }) else {
            return;
        };
        let mut h = hits.lock().unwrap();
        h[0] += ui.reports.len();
        h[1] += cast::<usize, _>(a.1 == 2 && n > 0);
        h[2] += cast::<usize, _>(n == 0);
        h[3] += cast::<usize, _>(ui.msgs.iter().any(|m| m.contains(__content.string(13886))));
        h[4] += cast::<usize, _>(list_type == 2 && !ui.reports.is_empty());
        h[5] += cast::<usize, _>(a.1 == 6 && !ui.reports.is_empty());
    });
    let h = hits.lock().unwrap();
    eprintln!("hits {h:?}");
    if super::diff::unavailable().is_none() && std::env::var("WSR_ONLY_SEED").is_err() {
        assert!(h.iter().all(|&x| x > 0), "branches not hit: {h:?}");
    }
}

#[test]
fn list_swap_holders_matches_original() {
    let __content = wsr_core::content::current();
    let hits = Mutex::new([0usize; 5]); // report, cancel, none popup, no-records popup, PLAYER symbol
    par_seeds(1u64..=30, |seed| {
        let mut r = Rand(seed * 104_729 + 3);
        let mut g = list_world(seed, &mut r);
        let n = if seed % 10 == 0 { 0 } else { cast::<i32, _>(r.range(1, 60)) };
        let mut player_short = false;
        for i in 1..=n {
            if r.chance(Ext::from_f64(0.1)) {
                continue;
            }
            let long = holder(&mut r, &g);
            let short = if r.chance(Ext::from_f64(0.3)) { cast::<i32, _>(r.range(1, 5)) } else { holder(&mut r, &g) };
            player_short |= short < 11 && g.ent_symbol[short].len() > 6;
            let fixed = match r.range(0, 4) {
                0 => format!("{}", r.range(1, 15)),
                1 => format!("{}.{}", r.range(0, 15), r.range(0, 9)),
                2 => format!("{}.{:02}", r.range(0, 15), r.range(0, 99)),
                3 => format!(" {:.3} ", f64r(&mut r, Ext::from(Ext::from_f64(0.0)), Ext::from(Ext::from_f64(20.0)))),
                _ => format!(".{}", r.range(1, 9)),
            };
            let kind = ["P", "L", "S", " L "][cast::<usize, _>(r.range(0, 3))];
            let by = g.cur_year + cast::<i32, _>(r.range(-2, 2));
            let begin = format!("{by}.{}", r.range(1, 4));
            let end = format!("{}.{}", by + cast::<i32, _>(r.range(1, 10)), r.range(1, 4));
            let notional = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(3_000_000.0));
            g.swap_contracts[i] = st(&format!("{long}|{short}|{fixed}|{kind}|{begin}|{end}|{notional:.1}"));
        }
        g.swap_contracts[0] = st(&n.to_string());
        let mut a = answer(&mut r);
        if seed == 1 {
            // A nonempty book with no positions belonging to the player or controlled companies.
            g.swap_contracts[0] = st("1");
            g.swap_contracts[1] = st("131|132|5|P|2000.1|2040.1|1000");
            g.corp_controller[131] = 0;
            g.corp_controller[132] = 0;
            a = ("IDYES", 6);
        }
        let Some((ui, _)) = check(seed, LIST_SWAPS, &[], &g, &[a], &[], |g, ui, rng| list_swap_holders(g, ui, rng))
        else {
            return;
        };
        let mut h = hits.lock().unwrap();
        h[0] += ui.reports.len();
        h[1] += cast::<usize, _>(a.1 == 2 && n > 0);
        h[2] += cast::<usize, _>(ui.msgs.iter().any(|m| m.contains(__content.string(13887))));
        h[3] += cast::<usize, _>(n == 0);
        h[4] += cast::<usize, _>(player_short && !ui.reports.is_empty());
    });
    let h = hits.lock().unwrap();
    eprintln!("hits {h:?}");
    if super::diff::unavailable().is_none() && std::env::var("WSR_ONLY_SEED").is_err() {
        assert!(h.iter().all(|&x| x > 0), "branches not hit: {h:?}");
    }
}

#[test]
fn list_option_holders_matches_original() {
    let __content = wsr_core::content::current();
    let hits = Mutex::new([0usize; 5]); // report, cancel, none popup, no-records popup, no question
    par_seeds(1u64..=30, |seed| {
        let mut r = Rand(seed * 15_485_863 + 5);
        let mut g = list_world(seed, &mut r);
        if seed % 5 == 0 {
            let p = g.cur_player;
            for s in 0..=15 {
                g.hold_stock_id[(s, p)] = 0;
            }
        }
        let n = if seed % 11 == 0 { 0 } else { cast::<i32, _>(r.range(1, 70)) };
        for i in 1..=n {
            if r.chance(Ext::from_f64(0.1)) {
                continue;
            }
            let corp = r.range(11, 1600);
            let h = holder(&mut r, &g);
            let pct = r.range(1, 100) * if r.chance(Ext::from_f64(0.4)) { -1 } else { 1 };
            g.option_recs[i] = st(&format!(
                "{corp}|{}|{h}|{pct}|{}|{}|{:.2}",
                r.range(1, 2),
                g.cur_year + cast::<i32, _>(r.range(0, 2)),
                r.range(1, 12),
                f64r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(300.0)))
            ));
        }
        g.option_recs[0] = st(&n.to_string());
        let mut a = answer(&mut r);
        if seed == 1 {
            // Ask for only the player's entities, with an unrelated company's sole option.
            let player = g.cur_player;
            give(&mut g, player, 131, 1);
            g.option_recs[0] = st("1");
            g.option_recs[1] = st("131|1|132|10|2030|1|100");
            g.corp_controller[132] = 0;
            a = ("IDYES", 6);
        }
        let Some((ui, _)) = check(seed, LIST_OPTIONS, &[], &g, &[a], &[], |g, ui, rng| list_option_holders(g, ui, rng))
        else {
            return;
        };
        let mut h = hits.lock().unwrap();
        h[0] += ui.reports.len();
        h[1] += cast::<usize, _>(ui.titles.iter().any(|t| t.contains(__content.string(13888))) && a.1 == 2);
        h[2] += cast::<usize, _>(ui.msgs.iter().any(|m| m.contains(__content.string(13889))));
        h[3] += cast::<usize, _>(n == 0);
        h[4] += cast::<usize, _>(g.hold_stock_id[(0, g.cur_player)] == 0 && !ui.reports.is_empty());
    });
    let h = hits.lock().unwrap();
    eprintln!("hits {h:?}");
    if super::diff::unavailable().is_none() && std::env::var("WSR_ONLY_SEED").is_err() {
        assert!(h.iter().all(|&x| x > 0), "branches not hit: {h:?}");
    }
}

#[test]
fn list_stock_cross_holdings_matches_original() {
    let __content = wsr_core::content::current();
    let hits = Mutex::new([0usize; 4]); // report, cancel, none popup, NOT TRADED line
    par_seeds(1u64..=24, |seed| {
        let mut r = Rand(seed * 32_452_843 + 7);
        let mut g = list_world(seed, &mut r);
        // thin the holdings out (every listed holding runs CalcAnalystRating / UpdateCorpControl)
        let keep = if seed % 8 == 0 { Ext::from_f64(0.0) } else { Ext::from_f64(0.02) };
        for e in 11..=1600 {
            for s in 1..=15 {
                if g.hold_stock_id[(s, e)] != 0 && !r.chance(keep) {
                    g.hold_stock_id[(s, e)] = 0;
                    g.hold_stock_pct[(s, e)] = 0;
                }
                if g.hold_stock_id[(s, e)] != 0 && r.chance(Ext::from_f64(0.2)) {
                    g.hold_stock_pct[(s, e)] = 100;
                }
            }
            g.hold_stock_id[(0, e)] = cast::<i16, _>((1..=15).filter(|&s| g.hold_stock_id[(s, e)] != 0).count());
        }
        let a = answer(&mut r);
        let Some((ui, _)) = check(seed, LIST_CROSS, &[], &g, &[a], &[], |g, ui, rng| list_stock_cross_holdings(g, ui, rng))
        else {
            return;
        };
        let mut h = hits.lock().unwrap();
        h[0] += ui.reports.len();
        h[1] += cast::<usize, _>(ui.titles.iter().any(|t| t.contains(__content.string(13888))) && a.1 == 2);
        h[2] += cast::<usize, _>(ui.msgs.iter().any(|m| m.contains(__content.string(13890))));
        h[3] += cast::<usize, _>(ui.reports.iter().any(|(_, l)| l.iter().any(|x| x.to_display().contains(__content.string(13891)))));
    });
    let h = hits.lock().unwrap();
    eprintln!("hits {h:?}");
    if super::diff::unavailable().is_none() && std::env::var("WSR_ONLY_SEED").is_err() {
        assert!(h.iter().all(|&x| x > 0), "branches not hit: {h:?}");
    }
}

/// A world for the streaming list: some slots of the player's list used.
fn stream_world(seed: u64, r: &mut Rand) -> Globals {
    let mut g = list_world(seed, r);
    let p = g.cur_player;
    let used = match seed % 4 {
        0 => 0,
        1 => 15,
        _ => cast::<i32, _>(r.range(0, 14)),
    };
    for s in 1..=15 {
        let id = if s <= used { cast::<i32, _>(r.range(11, 1600)) } else if r.chance(Ext::from_f64(0.1)) { cast::<i32, _>(r.range(0, 10)) } else { 0 };
        g.watch_list[(s, p)] = id;
        g.stream_cur_price[(s, p)] = st(if id > 10 { "12.50" } else { "" });
        g.stream_prev_price_str[(s, p)] = st(if id > 10 { "12.25" } else { "" });
        g.stream_price_dir[(s, p)] = if id > 10 { cast::<i32, _>(r.range(-1, 1)) } else { 0 };
    }
    g.watch_list[(0, p)] = used;
    g.sel_entity = if r.chance(Ext::from_f64(0.5)) { cast::<i32, _>(r.range(11, 1600)) } else { cast::<i32, _>(r.range(0, 10)) };
    if r.chance(Ext::from_f64(0.1)) {
        g.game_end_flag = st("Y");
    }
    g
}

#[test]
fn clear_streaming_quotes_list_matches_original() {
    let hits = Mutex::new([0usize; 3]); // asked+yes, asked+no, cleared without question
    par_seeds(1u64..=16, |seed| {
        let mut r = Rand(seed * 49_979_687 + 9);
        let g = stream_world(seed, &mut r);
        let a = if r.chance(Ext::from_f64(0.5)) { ("IDYES", 6) } else { ("IDNO", 7) };
        let Some((ui, _)) = check(seed, CLEAR_STREAM, &[], &g, &[a], &[], |g, ui, _| clear_streaming_quotes_list(g, ui))
        else {
            return;
        };
        let mut h = hits.lock().unwrap();
        h[0] += cast::<usize, _>(!ui.msgs.is_empty() && a.1 == 6);
        h[1] += cast::<usize, _>(!ui.msgs.is_empty() && a.1 == 7);
        h[2] += cast::<usize, _>(ui.msgs.is_empty() && g.game_end_flag != "Y");
    });
    let h = hits.lock().unwrap();
    eprintln!("hits {h:?}");
    if super::diff::unavailable().is_none() && std::env::var("WSR_ONLY_SEED").is_err() {
        assert!(h.iter().all(|&x| x > 0), "branches not hit: {h:?}");
    }
}

#[test]
fn fill_streaming_list_with_holdings_matches_original() {
    let __content = wsr_core::content::current();
    // added, already listed, full -> filled to 15, list full question, no positions, shorted, fund
    let hits = Mutex::new([0usize; 6]);
    par_seeds(1u64..=40, |seed| {
        let mut r = Rand(seed * 67_867_967 + 11);
        let mut g = stream_world(seed, &mut r);
        let p = g.cur_player;
        if seed % 7 == 3 {
            // no stocks, options or convertible bonds
            for s in 0..=15 {
                g.hold_stock_id[(s, p)] = 0;
                g.hold_stock_pct[(s, p)] = 0;
            }
            for s in 1..=15 {
                if r.chance(Ext::from_f64(0.5)) {
                    g.hold_bond_id[(s, p)] = 0;
                }
            }
        }
        if seed % 5 == 1 {
            // fewer controlled companies: the list does not fill up
            for e in 11..=1600 {
                if g.corp_controller[e] == p && r.chance(Ext::from_f64(0.9)) {
                    g.corp_controller[e] = 0;
                }
            }
            for s in 1..=15 {
                g.hold_stock_id[(s, p)] = if r.chance(Ext::from_f64(0.3)) { cast::<i16, _>(r.range(11, 1600)) } else { 0 };
                g.hold_stock_pct[(s, p)] = cast::<i16, _>(r.range(-20, 30));
            }
            g.hold_stock_id[(0, p)] = cast::<i16, _>((1..=15).filter(|&s| g.hold_stock_id[(s, p)] != 0).count());
        }
        // options of the player
        let n = cast::<i32, _>(r.range(0, 20));
        for i in 1..=n {
            let h = if r.chance(Ext::from_f64(0.4)) { p } else { cast::<i32, _>(r.range(11, 1600)) };
            let pct = r.range(1, 50) * if r.chance(Ext::from_f64(0.4)) { -1 } else { 1 };
            g.option_recs[i] = st(&format!("{}|{}|{h}|{pct}|{}|3|100|2.5", r.range(11, 1600), r.range(1, 2), g.cur_year + 1));
        }
        g.option_recs[0] = st(&n.to_string());
        // funds managed by companies the player controls
        for f in 1581..=1600 {
            if r.chance(Ext::from_f64(0.3)) {
                g.entity_status[f] = 1;
                g.industry_num[f] = 71;
                g.market_cap[f] = f64r(&mut r, Ext::from_f64(5.0), Ext::from_f64(300.0));
                g.shares_outstanding[f] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(10.0));
                g.fund_adviser[f] = if r.chance(Ext::from_f64(0.5)) {
                    (86..=1580).find(|&e| g.corp_controller[e] == p && g.entity_status[e] == 1).unwrap_or(0)
                } else {
                    cast::<i32, _>(r.range(0, 1600))
                };
            }
        }
        if seed <= 2 {
            // Keep explicit already-listed and no-position worlds alongside the random worlds.
            g.game_in_progress = 1;
            g.game_end_flag = st("N");
            for slot in 0..=15 {
                g.hold_stock_id[(slot, p)] = 0;
                g.hold_stock_pct[(slot, p)] = 0;
                g.hold_bond_id[(slot, p)] = 0;
                g.watch_list[(slot, p)] = 0;
            }
            for i in 0..=1000 { g.option_recs[i] = st(""); }
            for corp in 11..=1600 {
                if g.corp_controller[corp] == p { g.corp_controller[corp] = 0; }
            }
            for fund in 1581..=1600 { g.fund_adviser[fund] = 0; }
            if seed == 1 {
                g.entity_status[131] = 1;
                give(&mut g, p, 131, 1);
                g.watch_list[(0, p)] = 1;
                g.watch_list[(1, p)] = 131;
            }
        }
        let a = if r.chance(Ext::from_f64(0.7)) { ("IDYES", 6) } else { ("IDNO", 7) };
        let answers = [a, a];
        let Some((ui, _)) = check(seed, FILL_STREAM, &[], &g, &answers, &[], |g, ui, rng| fill_streaming_list_with_holdings(g, ui, rng)) else {
            return;
        };
        let mut h = hits.lock().unwrap();
        h[0] += cast::<usize, _>(ui.msgs.iter().any(|m| m.contains(__content.string(13892))));
        h[1] += cast::<usize, _>(ui.msgs.iter().any(|m| m.contains(__content.string(13893))));
        h[2] += cast::<usize, _>(ui.msgs.iter().any(|m| m.contains(__content.string(13894))));
        h[3] += cast::<usize, _>(ui.msgs.iter().any(|m| m.contains(__content.string(13895))));
        h[4] += cast::<usize, _>(ui.msgs.iter().any(|m| m.contains(__content.string(13896))));
        h[5] += cast::<usize, _>(ui.msgs.iter().any(|m| m.contains(__content.string(13897))));
    });
    let h = hits.lock().unwrap();
    eprintln!("hits {h:?}");
    if super::diff::unavailable().is_none() && std::env::var("WSR_ONLY_SEED").is_err() {
        assert!(h.iter().all(|&x| x > 0), "branches not hit: {h:?}");
    }
}

#[test]
fn report_dlg_rating_click_matches_original() {
    let __content = wsr_core::content::current();
    // buy offer, sell (own shares), cancelling, major shareholder offer, declined, not a research report
    let hits = Mutex::new([0usize; 6]);
    par_seeds(1u64..=60, |seed| {
        let mut r = Rand(seed * 86_028_121 + 13);
        let mut g = list_world(seed, &mut r);
        let p = g.cur_player;
        g.cur_player_name = g.player_name[p].clone();
        let corp = (cast::<i32, _>(r.range(131, 1580))..=1580).find(|&e| g.entity_status[e] == 1).unwrap_or(1580);
        g.entity_status[corp] = 1;
        g.research_report_corp = corp;
        g.text_box_title = st(if seed % 13 == 0 { __content.string(13898) } else { "     === RESEARCH REPORT ON X ===" });
        g.cur_analyst_rating = cast::<i32, _>(r.range(1, 5));
        // acting / selected entity combinations
        let other = cast::<i32, _>(r.range(131, 1580));
        match r.range(0, 3) {
            0 => {
                g.active_entity = corp;
                g.sel_entity = corp;
            }
            1 => {
                g.active_entity = p;
                g.sel_entity = p;
            }
            2 => {
                g.active_entity = other;
                g.sel_entity = other;
            }
            _ => {
                g.active_entity = p;
                g.sel_entity = corp;
            }
        }
        g.sel_corp_name = g.ent_name[g.sel_entity].clone();
        g.active_entity_name = g.ent_name[g.active_entity].clone();
        // the player's holding of the stock, a major shareholder he controls
        if r.chance(Ext::from_f64(0.4)) {
            give(&mut g, p, corp, cast::<i16, _>(r.range(1, 20)));
        }
        if r.chance(Ext::from_f64(0.3)) {
            let (active, selected) = (g.active_entity, g.sel_entity);
            give(&mut g, active, selected, cast::<i16, _>(r.range(1, 20)));
        }
        if r.chance(Ext::from_f64(0.5)) {
            let big = cast::<i32, _>(r.range(131, 1580));
            g.entity_status[big] = 1;
            g.corp_controller[big] = if r.chance(Ext::from_f64(0.7)) { p } else { 0 };
            give(&mut g, big, corp, 60);
            let selected = g.sel_entity;
            give(&mut g, big, selected, 60);
        }
        if r.chance(Ext::from_f64(0.3)) {
            g.corp_controller[corp] = p;
        }
        let a = if r.chance(Ext::from_f64(0.75)) { ("IDYES", 6) } else { ("IDNO", 7) };
        let answers = [a, a, a];
        let args = ["v:0", "v:0", "v:0", "v:0"];
        let Some((ui, _)) = check(seed, RATING_CLICK, &args, &g, &answers, &[(DO_TRANSACTION_MENU_ACTION, 1)], |g, ui, rng| {
            report_dlg_rating_click_with_env(g, ui, rng, &Env::skipping(&LocalTime::HARNESS, &[DO_TRANSACTION_MENU_ACTION]));
        }) else {
            return;
        };
        let mut h = hits.lock().unwrap();
        h[0] += cast::<usize, _>(ui.titles.iter().any(|t| t.contains(__content.string(13899))));
        h[1] += cast::<usize, _>(ui.msgs.iter().any(|m| m.contains(__content.string(13900))));
        h[2] += cast::<usize, _>(ui.msgs.iter().any(|m| m.contains("Cancel")));
        h[3] += cast::<usize, _>(ui.msgs.iter().any(|m| m.contains(__content.string(13901)) || m.contains(__content.string(13902))));
        h[4] += cast::<usize, _>(ui.titles.iter().any(|t| t.contains("POTENTIAL")) && a.1 == 7);
        h[5] += cast::<usize, _>(seed % 13 == 0);
    });
    let h = hits.lock().unwrap();
    eprintln!("hits {h:?}");
    if super::diff::unavailable().is_none() && std::env::var("WSR_ONLY_SEED").is_err() {
        assert!(h.iter().all(|&x| x > 0), "branches not hit: {h:?}");
    }
}

/// Puts (or replaces) a holding of `pct` percent of `corp` into a slot of `holder`.
fn give(g: &mut Globals, holder: i32, corp: i32, pct: i16) {
    let s = (1..=15)
        .find(|&s| cast::<i32, _>(g.hold_stock_id[(s, holder)]) == corp)
        .or_else(|| (1..=15).find(|&s| g.hold_stock_id[(s, holder)] == 0))
        .unwrap_or(15);
    g.hold_stock_id[(s, holder)] = cast::<i16, _>(corp);
    g.hold_stock_pct[(s, holder)] = pct;
    g.hold_stock_basis[(s, holder)] = Ext::from_f64(10.0);
    g.hold_stock_id[(0, holder)] = cast::<i16, _>((1..=15).filter(|&k| g.hold_stock_id[(k, holder)] != 0).count());
}

#[test]
fn list_etf_advisers_matches_original() {
    par_seeds(1..=12, |seed| {
        let mut r = Rand(seed * 321 + 91);
        let mut g = list_world(seed, &mut r);
        for etf in 1581..=1600 {
            g.entity_status[etf] = if seed % 4 == 0 || !r.chance(Ext::from_f64(0.5)) { 0 } else { 1 };
            g.industry_num[etf] = 71;
            g.fund_adviser[etf] = 86;
            g.fund_fee_pct[etf] = cast::<f32, _>([Ext::from_f64(0.2), Ext::from_f64(0.5), Ext::from_f64(1.0)][cast::<usize, _>(r.range(0, 2))]);
            g.fund_bonus_year[etf] = if r.chance(Ext::from_f64(0.5)) { g.cur_year } else { 0 };
        }
        check(seed, 0x673009, &[], &g, &[], &[], |g, ui, rng| list_etf_advisers(g, ui, rng));
    });
}
