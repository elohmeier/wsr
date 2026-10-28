//! Differential tests of `logic::trading::select`: FollowReportLink (0x46c9f4), LookupSymbol (0x422b3b),
//! SelectActingEntity (0x4721dc, with ShowCompanyPickList 0x5825fc and InBox mode 3 0x423170),
//! PickFromStreamingList (0x5de64f) and GetSelectedPlayerEtf (0x652b17).
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use super::script::*;
use std::cell::Cell;
use wsr_core::logic::newgame::LocalTime;
use wsr_core::logic::trading::select::*;
use wsr_core::state::Globals;

const FOLLOW: u32 = 0x46c9f4;
const LOOKUP: u32 = 0x422b3b;
const SELECT_ACTING: u32 = 0x4721dc;
const PICK_STREAMING: u32 = 0x5de64f;
const SELECTED_ETF: u32 = 0x652b17;

#[allow(non_snake_case)]
fn PICK_MODES(__content: &wsr_core::content::ContentPack) -> [&str; 6] { ["GetCorp", "GetInd", __content.string(10154), __content.string(6093), "", "GetCorp"] }

/// A world in which the current player controls some companies, with the market display state set.
fn sel_world(seed: u64, r: &mut Rand) -> Globals {
    let mut g = world(seed);
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    g.cur_player_name = g.player_name[g.cur_player].clone();
    g.ent_name[g.cur_player] = g.player_name[g.cur_player].clone();
    g.active_entity = g.cur_player;
    g.active_entity_name = g.ent_name[g.cur_player].clone();
    g.sel_entity = pick_active(r, &g, 11, 1580, &[]);
    g.last_sel_entity = pick_active(r, &g, 11, 1580, &[]);
    g.sel_corp_name = g.ent_name[g.sel_entity].clone();
    g.sel_industry = cast::<i32, _>(r.range(0, 69));
    if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
        g.game_end_flag = st("Y");
    }
    for _ in 0..r.range(0, 6) {
        let c = pick_active(r, &g, 11, 1580, &[]);
        g.corp_controller[c] = g.cur_player;
    }
    g
}

/// A link code (or a whole report line ending in one).
fn link_for(r: &mut Rand, g: &Globals) -> String {
    let __content = wsr_core::content::current();
    let code = match r.range(0, 9) {
        0 => format!("@I{:04}", r.range(0, 75)),
        1 => format!("@C{:04}", g.cur_player),
        2 => format!("@C{:04}", r.range(1, 5)),
        3 => "@C1700".to_string(),
        4 => {
            let c = (11..=1600).find(|&c| g.entity_status[c] == 2).unwrap_or(500);
            format!("{__wsr_text_13538}{c:04}",__wsr_text_13538=__content.string(13538))
        }
        5 => "@C0000".to_string(),
        6 => "NO LINK HERE".to_string(),
        _ => format!("@C{:04}", pick_active(r, g, 11, 1580, &[])),
    };
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        format!("SOME COMPANY NAME (SYM)          {code}")
    } else {
        code
    }
}

#[test]
fn diff_follow_report_link() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(seeds(1..=120), |seed| {
        let mut r = Rand(seed * 7 + 1);
        let mut g = sel_world(seed + 9800, &mut r);
        g.entity_pick_mode = st(PICK_MODES(&__content)[cast::<usize, _>(r.range(0, 5))]);
        let mut link = link_for(&mut r, &g);
        if seed % 8 == 0 {
            // after the game's end: switch to another player
            g.entity_pick_mode = st(__content.string(6093));
            g.game_end_flag = st("Y");
            g.num_players = g.num_players.max(2);
            let p = if g.cur_player == 1 { 2 } else { 1 };
            g.entity_status[p] = 1;
            link = format!("{__wsr_text_13538}{p:04}",__wsr_text_13538=__content.string(13538));
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
            // the link's company is the selected one already
            let id = link.get(link.len() - 4..).and_then(|x| x.parse::<i32>().ok()).unwrap_or(0);
            if (11..=1600).contains(&id) {
                g.sel_entity = id;
            }
        }
        let answers = vec!["IDOK".to_string(); 3];
        let res = Cell::new(0);
        let cmp = cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-7));
        let args = [format!("{__wsr_text_13622}{link}",__wsr_text_13622=__content.string(13622))];
        let Some((ui, pg, o)) = check(seed, FOLLOW, &args, &g, &answers, &[], cmp, |g, ui, rng| {
            res.set(follow_report_link(g, ui, rng, &st(&link)));
        }) else {
            return;
        };
        assert_eq!(cast::<u32, _>(res.get()), o.return_value, "seed {seed}: result for {link:?} in {:?}", g.entity_pick_mode);
        hits.record(&ui);
        hits.add(&format!("<{} {}>", g.entity_pick_mode.to_display(), res.get()));
        if pg.cur_player != g.cur_player {
            hits.add("<player switched>");
        }
        if pg.sel_entity != g.sel_entity {
            hits.add(__content.string(14321));
        }
    });
    hits.assert(&[
        "<GetCorp 1>",
        "<GetInd 1>",
        "<ChangeInd 1>",
        "<ChangeCorp 1>",
        "<ChangeCorp 0>",
        "< 1>",
        "< 0>",
        "<player switched>",
        __content.string(14321),
    ]);
}

#[test]
fn diff_lookup_symbol() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(seeds(1..=100), |seed| {
        let mut r = Rand(seed * 13 + 5);
        let g = sel_world(seed + 9900, &mut r);
        let c = pick_active(&mut r, &g, 11, 1580, &[]);
        let sym = g.ent_symbol[c].to_display();
        let dead = (11..=1600).find(|&c| g.entity_status[c] == 2).unwrap_or(500);
        let text = match r.range(0, 11) {
            0 => sym.to_lowercase(),
            1 => format!("  {sym} "),
            2 => g.ent_symbol[dead].to_display(),
            3 => "CANCEL".to_string(),
            4 => "QZXQZXQ".to_string(),
            5 => format!("{}", r.range(11, 1600)),
            6 => format!("{}", r.range(1, 5)),
            7 => "N/A".to_string(),
            8 => "...ABC".to_string(),
            9 => String::new(),
            _ => sym,
        };
        let answers = vec!["IDOK".to_string(); 3];
        let cmp = cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-7));
        let args = [format!("{__wsr_text_13622}{text}",__wsr_text_13622=__content.string(13622))];
        let Some((ui, pg, _)) = check(seed, LOOKUP, &args, &g, &answers, &[], cmp, |g, ui, rng| {
            lookup_symbol(g, ui, rng, &st(&text));
        }) else {
            return;
        };
        hits.record(&ui);
        hits.add(if pg.target_corp > 0 { "<found>" } else { __content.string(14322) });
        if pg.cur_player != g.cur_player {
            hits.add("<player switched>");
        }
    });
    hits.assert(&["<found>", __content.string(14322), "<player switched>", __content.string(7948)]);
}

#[test]
fn diff_select_acting_entity() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(seeds(1..=120), |seed| {
        let mut r = Rand(seed * 17 + 3);
        let mut g = sel_world(seed + 10_000, &mut r);
        let kind = seed % 6;
        let mut answers = Vec::new();
        let mode = match kind {
            0 => 1,
            // another player after the game's end (sometimes one who is out)
            1 => {
                g.game_end_flag = st("Y");
                let p = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
                if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                    g.player_name[p] = st("...OUT...");
                    g.entity_status[p] = 2;
                }
                -p
            }
            // the company pick list
            2 | 3 => {
                g.game_end_flag = st("N");
                // a computer player's pick list ends its turn instead (EndTurn); the turn passes to a
                // human player
                g.is_computer_player[g.cur_player] = 0;
                if seed % 12 == 3 {
                    g.num_players = g.num_players.max(2);
                    for p in 1..=g.num_players {
                        g.player_turn_order[p] = p;
                        g.entity_status[p] = 1;
                        g.is_computer_player[p] = 0;
                    }
                    g.is_computer_player[g.cur_player] = 1;
                    g.has_cpu_players = st("Y");
                }
                if kind == 3 && r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                    for c in 11..=1600 {
                        if g.corp_controller[c] == g.cur_player {
                            g.corp_controller[c] = 0;
                        }
                    }
                }
                answers.push(format!("pick:{}", r.range(0, 5)));
                answers.push("IDOK".to_string());
                2
            }
            // symbol entry
            _ => {
                let c = pick_active(&mut r, &g, 11, 1580, &[]);
                let text = match r.range(0, 4) {
                    0 => String::new(),
                    1 => "QZXQZXQ".to_string(),
                    2 => g.ent_symbol[c].to_display().to_lowercase(),
                    _ => g.ent_symbol[c].to_display(),
                };
                answers.push(format!("sym:{text}"));
                answers.push("IDOK".to_string());
                if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                    g.input_dlg_prompt = st(__content.string(14323));
                    g.input_dlg_title = st("Pick a Company");
                }
                3
            }
        };
        let cmp = cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-7));
        let args = [mode.to_string()];
        let Some((ui, pg, _)) = check(seed, SELECT_ACTING, &args, &g, &answers, &[], cmp, |g, ui, rng| {
            select_acting_entity(g, ui, rng, mode, &LocalTime::HARNESS)
        }) else {
            return;
        };
        hits.record(&ui);
        if pg.sel_entity != g.sel_entity {
            hits.add(&format!("<mode {}{__wsr_text_14324}", mode.clamp(-1, 3),__wsr_text_14324=__content.string(14324)));
        }
        if !ui.reports.is_empty() {
            hits.add("<pick list>");
        }
        if pg.cur_player != g.cur_player && g.is_computer_player[g.cur_player] == 1 {
            hits.add("<turn ended>");
        }
    });
    hits.assert(&["<mode 1 selected>", "<mode -1 selected>", "<mode 2 selected>", "<mode 3 selected>", "<pick list>", "<turn ended>", __content.string(14325)]);
}

#[test]
fn diff_pick_from_streaming_list() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(seeds(1..=40), |seed| {
        let mut r = Rand(seed * 19 + 11);
        let mut g = sel_world(seed + 10_100, &mut r);
        g.game_end_flag = st(if seed % 10 == 0 { "Y" } else { "N" });
        let cp = g.cur_player;
        let n = if seed % 4 < 2 { 0 } else { r.range(1, 15) };
        for s in 1..=15 {
            g.watch_list[(s, cp)] = if (cast::<i64, _>(s)) <= n { pick_active(&mut r, &g, 11, 1580, &[]) } else { 0 };
        }
        let mode = cast::<i32, _>(seed % 2);
        let answers = vec![format!("pick:{}", r.range(0, 14)), "IDOK".to_string()];
        let res = Cell::new(0);
        let cmp = cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-7));
        let args = [mode.to_string()];
        let Some((ui, _, o)) = check(seed, PICK_STREAMING, &args, &g, &answers, &[], cmp, |g, ui, rng| {
            res.set(pick_from_streaming_list(g, ui, rng, mode));
        }) else {
            return;
        };
        assert_eq!(cast::<u32, _>(res.get()), o.return_value, "seed {seed}: result");
        hits.record(&ui);
        hits.add(if res.get() > 0 { "<picked>" } else { "<none>" });
    });
    hits.assert(&["<picked>", "<none>", __content.string(14326)]);
}

#[test]
fn diff_get_selected_player_etf() {
    let hits = Hits::default();
    par_seeds(seeds(1..=30), |seed| {
        let mut r = Rand(seed * 23 + 7);
        let mut g = sel_world(seed + 10_200, &mut r);
        for etf in 1581..=1583 {
            g.entity_status[etf] = 1;
            let adviser = pick_active(&mut r, &g, 86, 1580, &[]);
            g.fund_adviser[etf] = adviser;
            if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
                g.industry_num[adviser] = [2, 37, 37, 20][cast::<usize, _>(r.range(0, 3))];
                g.corp_controller[adviser] = if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) { g.cur_player } else { cast::<i32, _>(r.range(1, 5)) };
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
                g.fund_adviser[etf] = cast::<i32, _>(r.range(0, 85));
            }
        }
        g.sel_entity = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { cast::<i32, _>(r.range(1581, 1583)) } else { pick_active(&mut r, &g, 11, 1580, &[]) };
        g.last_sel_entity = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { cast::<i32, _>(r.range(1581, 1583)) } else { 0 };
        let res = Cell::new(0);
        let cmp = cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-7));
        let Some((ui, _, o)) = check(seed, SELECTED_ETF, &[], &g, &[], &[], cmp, |g, ui, rng| {
            res.set(get_selected_player_etf(g, ui, rng));
        }) else {
            return;
        };
        assert_eq!(cast::<u32, _>(res.get()), o.return_value, "seed {seed}: result");
        hits.record(&ui);
        hits.add(if res.get() > 0 { "<etf>" } else { "<none>" });
    });
    hits.assert(&["<etf>", "<none>"]);
}
