//! ShowIndustryGrowthProjections / ShowIndustryProjection (and the PB ARRAY SORT port they use).
use wsr_core::pb::real::{Ext, cast};


use super::common::{assert_reports, cmp, orig_msgs, orig_reports, port_reports, pow_safe, st, word, world, Rec};
use super::diff::{run_original_with, Opts, Original, Rand};
use wsr_core::logic::reports_lists::{show_etf_strategies_report, show_industry_growth_projections, show_industry_projection};
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

const SHOW_INDUSTRY_GROWTH_PROJECTIONS: u32 = 0x45f1af;
const SHOW_INDUSTRY_PROJECTION: u32 = 0x460058;

fn run(addr: u32, args: &[String], g: &Globals, rng: u32) -> Option<Original> {
    let opts = Opts { args: args.to_vec(), rng: Some(rng), ..Opts::default() };
    run_original_with(addr, &opts, g)
}

fn industry_names(g: &mut Globals, r: &mut Rand) {
    for i in 0..=71 {
        // mixed case and blanks so the alphabetical sort sees varied bytes
        let mut n = word(r, 9).to_display();
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            n = n.to_lowercase();
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            n = format!("{n} {}", word(r, 6).to_display());
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) && i > 0 {
            n = g.ind_name[i - 1].to_display(); // duplicate names
        }
        g.ind_name[i] = PbStr::from(n.as_str());
    }
    for e in 11..=1600 {
        g.ent_symbol[e] = PbStr::from(format!("{}{e}", word(r, 3).to_display()).as_str());
    }
}

#[test]
fn show_industry_growth_projections_matches_original() {
    for seed in 1u64..=4 {
        let mut g = world(seed * 31);
        pow_safe(&mut g, seed);
        let mut r = Rand(seed * 977);
        industry_names(&mut g, &mut r);
        if seed == 2 {
            g.ind_total_assets = Ext::from_f64(0.0);
        }
        let rng0 = cast::<u32, _>(r.next());
        let Some(o) = run(SHOW_INDUSTRY_GROWTH_PROJECTIONS, &[], &g, rng0) else { return };
        let mut port = g.clone();
        let mut ui = Rec::default();
        let mut rng = Rng::from_state(rng0);
        show_industry_growth_projections(&mut port, &mut ui, &mut rng);
        assert_reports(&port_reports(&ui, &[]), &orig_reports(&o));
        assert_eq!(ui.msgs.len(), orig_msgs(&o));
        assert_eq!(rng.state(), o.rng, "RND state");
        cmp().rel_tol(Ext::from_f64(1e-10)).assert(&port, &o.globals);
    }
}

#[test]
fn show_industry_projection_matches_original() {
    let __content = wsr_core::content::current();
    let cases: &[(u64, i32)] = &[(1, 5), (2, 17), (3, 33), (4, 0), (5, 2), (6, 70), (7, 12), (8, 40), (9, 1), (10, 25)];
    for &(seed, industry) in cases {
        let mut g = world(seed * 131);
        pow_safe(&mut g, seed);
        let mut r = Rand(seed * 7919);
        industry_names(&mut g, &mut r);
        g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
        g.cur_player_name = g.player_name[g.cur_player].clone();
        g.entity_pick_mode = st(if seed % 2 == 0 { __content.string(6093) } else { "" });
        g.sel_entity = 0;
        // CalcStockPrice would call ShowCashFlowProjection (not ported, FUNCTION AS EXT: a skipped call
        // leaves a stale ST0 that the original rounds into its cash-flow outlook). After the game has
        // ended CalcStockPrice only recomputes the financials, so both sides agree.
        g.game_end_flag = st("Y");
        if seed == 7 {
            // many ties in the market share (same assets and growth)
            for c in 131..=1580 {
                if g.industry_num[c] == cast::<i16, _>(industry) && r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
                    g.business_assets[c] = Ext::from_f64(100.0);
                    g.growth_rate[c] = 5;
                }
            }
        }
        let rng0 = cast::<u32, _>(r.next());
        let Some(o) = run(SHOW_INDUSTRY_PROJECTION, &[industry.to_string()], &g, rng0) else { return };
        let mut port = g.clone();
        let mut ui = Rec::default();
        let mut rng = Rng::from_state(rng0);
        show_industry_projection(&mut port, &mut ui, &mut rng, industry, 0);
        let orig = orig_reports(&o);
        eprintln!("seed {seed}{__wsr_text_1975}{industry}: {}{__wsr_text_14000}{:.2}{__wsr_text_13748}", orig.first().map_or(0, |x| x.1.len()), o.elapsed,__wsr_text_1975=__content.string(1975), __wsr_text_14000=__content.string(14000), __wsr_text_13748=__content.string(13748));
        cmp().rel_tol(Ext::from_f64(1e-10)).assert(&port, &o.globals);
        assert_reports(&port_reports(&ui, &[]), &orig);
        assert_eq!(ui.msgs.len(), orig_msgs(&o));
        assert_eq!(rng.state(), o.rng, "RND state");
    }
}

#[test]
fn industry_projection_no_industry_note() {
    let __content = wsr_core::content::current();
    let mut g = Globals::default();
    g.ind_name[2] = st("INSURANCE");
    let mut ui = Rec::default();
    let mut rng = Rng::from_state(1);
    show_industry_projection(&mut g, &mut ui, &mut rng, 2, 0);
    let (title, lines) = &ui.reports[0];
    assert_eq!(*title, __content.string(14007));
    assert_eq!(lines.len(), 7);
    assert_eq!(lines[5], __content.string(14008));
}

#[test]
fn etf_strategies_and_industry_projection_match_original() {
    for seed in 1..=6 {
        let mut g = world(seed * 131);
        let mut r = Rand(seed * 7919);
        g.saved_game_version = st(if seed % 2 == 0 { "9.85" } else { "6.2" });
        for etf in 1581..=1600 {
            g.entity_status[etf] = if seed <= 2 || r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { 2 } else { 1 };
            g.industry_num[etf] = if r.chance(cast::<f64, _>(Ext::from_f64(0.8))) { 71 } else { 70 };
            g.ent_name[etf] = word(&mut r, 36);
        }
        let rng0 = cast::<u32, _>(r.next());
        for addr in [0x64aa96, SHOW_INDUSTRY_PROJECTION] {
            let args = if addr == SHOW_INDUSTRY_PROJECTION { vec!["71".to_string()] } else { vec![] };
            let Some(o) = run(addr, &args, &g, rng0) else { return };
            let mut port = g.clone();
            let mut ui = Rec::default();
            let mut rng = Rng::from_state(rng0);
            if addr == SHOW_INDUSTRY_PROJECTION {
                show_industry_projection(&mut port, &mut ui, &mut rng, 71, 0);
            } else {
                show_etf_strategies_report(&mut port, &mut ui, &mut rng);
            }
            cmp().assert(&port, &o.globals);
            assert_reports(&port_reports(&ui, &[]), &orig_reports(&o));
            assert_eq!(ui.msgs.len(), orig_msgs(&o), "seed {seed}");
            assert_eq!(rng.state(), o.rng, "seed {seed}");
            if let Some(Some(bytes)) = o.files.get("TMP_QM.DAT") {
                let want = ui.msgs.last().unwrap().replace("\r\n\r\n", "@@").replace("\r\n", "##");
                assert_eq!(String::from_utf8_lossy(bytes).trim_end_matches("\r\n"), want);
            }
        }
    }
}

