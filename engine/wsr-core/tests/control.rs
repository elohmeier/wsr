//! Unit tests for voting control: UpdateCorpControl (0x41f71b) + CalcHolders (0x4207fb),
//! SanitizeCorpGrowthSpending (0x4a14a7), FindOptionPositions (0x5f0cb9), and the holding finders
//! FindHoldingSlot (0x498c61), FindStockHolding (0x498d53), FindBondHoldingSlot (0x498e40),
//! GetLargestHoldingId (0x49100a).

#![allow(clippy::field_reassign_with_default)]
use wsr_core::pb::real::{Ext, cast};


use wsr_core::logic::control::{find_option_positions, sanitize_corp_growth_spending, update_corp_control};
use wsr_core::logic::holdings::{find_bond_holding_slot, find_holding_slot, find_stock_holding, get_largest_holding_id};
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::Scripted;

fn p(s: &str) -> PbStr {
    PbStr::from(s)
}

fn game() -> Globals {
    let __content = wsr_core::content::current();
    let mut g = Globals::default();
    g.num_players = 2;
    g.cur_player = 1;
    g.is_computer_player[1] = 0;
    g.is_computer_player[2] = 1;
    for pl in 1..=2 {
        g.entity_status[pl] = 1;
        g.player_name[pl] = p(&format!("{__wsr_text_13625}{pl}",__wsr_text_13625=__content.string(13625)));
        g.ent_name[pl] = p(&format!("{__wsr_text_13625}{pl}",__wsr_text_13625=__content.string(13625)));
        g.ent_symbol[pl] = p(&format!("PL{pl}"));
    }
    for c in 11..=40 {
        g.entity_status[c] = 1;
        g.ent_name[c] = p(&format!("Corp{c}"));
        g.ent_symbol[c] = p(&format!("C{c}"));
        g.industry_num[c] = 5;
        g.corp_controller[c] = c;
    }
    g.cur_year = 2030;
    g
}

fn hold(g: &mut Globals, owner: i32, slot: i32, corp_id: i32, pct: i16) {
    g.hold_stock_id[(slot, owner)] = cast::<i16, _>(corp_id);
    g.hold_stock_pct[(slot, owner)] = pct;
}

fn run(g: &mut Globals, corp: i32) -> Scripted {
    let mut ui = Scripted::default();
    let mut rng = Rng::default();
    assert_eq!(update_corp_control(g, &mut ui, &mut rng, corp), 0);
    ui
}

#[test]
fn single_large_holder_controls() {
    let mut g = game();
    hold(&mut g, 1, 1, 20, 30);
    hold(&mut g, 25, 3, 20, 10);
    g.corp_controller[20] = 1; // no change of control
    let ui = run(&mut g, 20);
    assert_eq!(g.voting_controller, 1);
    assert_eq!(g.corp_controller[20], 1);
    assert_eq!(g.public_pct, 60);
    assert_eq!(g.corp_pct_not_corp_held[20], 90);
    assert_eq!(g.largest_holder, 1);
    assert_eq!(g.largest_holder_pct, 30);
    assert_eq!(g.public_float_pct, 70);
    assert_eq!(g.wholly_owned_by, 0);
    assert_eq!(g.parent80_owner, 0);
    assert!(ui.log.is_empty());
}

#[test]
fn stakes_are_grouped_by_controller() {
    let mut g = game();
    hold(&mut g, 2, 1, 20, 10);
    hold(&mut g, 30, 1, 20, 15); // corp 30 is controlled by player 2
    g.corp_controller[30] = 2;
    hold(&mut g, 1, 1, 20, 22);
    g.corp_controller[20] = 2;
    run(&mut g, 20);
    assert_eq!(g.voting_controller, 2, "10% + 15% via C30 beat 22%");
    assert_eq!(g.largest_holder, 1, "LargestHolder is the largest single stake");
    assert_eq!(g.largest_holder_pct, 25);
    assert_eq!(g.public_pct, 53);
    assert_eq!(g.public_float_pct, 75);
}

#[test]
fn tie_or_wide_float_means_self_control() {
    let mut g = game();
    hold(&mut g, 1, 1, 20, 25);
    hold(&mut g, 2, 1, 20, 25);
    g.corp_controller[20] = 20;
    run(&mut g, 20);
    assert_eq!(g.voting_controller, 20);
    assert_eq!(g.corp_controller[20], 20);
    assert_eq!(g.largest_holder_pct, 25);

    let mut g = game();
    hold(&mut g, 1, 1, 21, 19);
    run(&mut g, 21);
    assert_eq!(g.voting_controller, 21, "public > 80%");
    assert_eq!(g.public_pct, 81);
    assert_eq!(g.largest_holder, 0, "only stakes >= 20 count");
}

#[test]
fn wholly_owned_and_80_percent_parent() {
    let mut g = game();
    hold(&mut g, 33, 1, 20, 100);
    g.corp_controller[20] = 33;
    g.ctrl_since_year[20] = 0;
    run(&mut g, 20);
    assert_eq!(g.wholly_owned_by, 33);
    assert_eq!(g.parent80_owner, 33);
    assert_eq!(g.public_pct, 0);
    assert_eq!(g.public_float_pct, 0);
    assert_eq!(g.voting_controller, 33);
    assert_eq!(g.ctrl_since_year[20], 2030, "CtrlSinceYear starts when an 80% parent appears");
    assert_eq!(g.has_options_outstanding[20], 0);
    assert!(g.option_recs[0].is_empty(), "count 0 = VAL(\"\"): OptionRecs(0) is not rewritten");
}

#[test]
fn ctrl_since_year_restarts_on_new_parent() {
    let mut g = game();
    hold(&mut g, 33, 1, 20, 85);
    g.corp_controller[20] = 33;
    g.ctrl_since_year[20] = 2001;
    let fields: Vec<String> = (1..=31).map(|i| if i == 28 { "34".to_string() } else { "x".to_string() }).collect();
    g.ent_misc_data[20] = p(&fields.join("|"));
    run(&mut g, 20);
    assert_eq!(g.ctrl_since_year[20], 2030, "field 28 names another parent");
    g.ctrl_since_year[20] = 2001;
    let fields: Vec<String> = (1..=31).map(|i| if i == 28 { "33".to_string() } else { "x".to_string() }).collect();
    g.ent_misc_data[20] = p(&fields.join("|"));
    run(&mut g, 20);
    assert_eq!(g.ctrl_since_year[20], 2001);
}

#[test]
fn takeover_by_human_player_posts_news() {
    let mut g = game();
    hold(&mut g, 1, 1, 20, 40);
    hold(&mut g, 20, 1, 35, 60); // C20 owns 60% of C35 -> C35 follows
    g.corp_controller[20] = 20;
    g.auto_pilot[1] = 1;
    let ui = run(&mut g, 20);
    assert_eq!(g.corp_controller[20], 1);
    assert_eq!(g.corp_controller[35], 1, "subsidiary >50% gets the new controller");
    assert_eq!(g.auto_pilot[20], 1, "AutoPilot follows the new (human) controller");
    let h = g.news_headline[60].to_display();
    assert!(h.starts_with("Voting control of C20 "), "{h}");
    assert!(h.ends_with(" by PL1."), "{h}");
    // old controller was the company itself: the news is about player 1 -> popup
    assert!(ui.log.iter().any(|l| l.starts_with("MSGBOX") && l.contains("Voting control of C20")));
    assert_eq!(g.ctrl_change_msg_mode, 0);
}

#[test]
fn human_loses_control() {
    let __content = wsr_core::content::current();
    let mut g = game();
    hold(&mut g, 1, 1, 20, 10);
    hold(&mut g, 2, 1, 20, 30);
    g.corp_controller[20] = 1;
    g.industry_num[20] = 1;
    g.credit_rating_num[20] = 8;
    g.credit_frozen[20] = 1;
    g.auto_pilot[20] = 1;
    let ui = run(&mut g, 20);
    assert_eq!(g.corp_controller[20], 2);
    assert_eq!(g.auto_pilot[20], 0);
    assert_eq!(g.credit_frozen[20], 0);
    let msgs: Vec<&String> = ui.log.iter().filter(|l| l.starts_with("MSGBOX")).collect();
    assert_eq!(msgs[0], __content.string(14419));
    let news: Vec<String> = (55..=60).map(|i| g.news_headline[i].to_display()).collect();
    assert!(news.iter().any(|h| h == "PL1 no longer controls C20 board of directors."), "{news:?}");
    assert!(news.iter().any(|h| h.starts_with("Voting control of C20") && h.contains(" from PL1 by PL2")), "{news:?}");
}

#[test]
fn fund_loses_tax_exemption() {
    let __content = wsr_core::content::current();
    let mut g = game();
    g.industry_num[1590] = 71;
    g.entity_status[1590] = 1;
    g.ent_symbol[1590] = p("FND");
    g.corp_controller[1590] = 1590;
    g.country_idx[1590] = 26;
    g.fund_adviser[1590] = 0;
    g.earnings_y1[1590] = Ext::from_f64(5.0);
    g.extraordinary_pl[1590] = 7;
    hold(&mut g, 1, 1, 1590, 16);
    let ui = run(&mut g, 1590);
    assert_eq!(g.industry_num[1590], 70);
    // 16% beats the fund threshold of 15% but not the 20% needed for control: the fund stays
    // self-controlled, yet the >15% stake (lostExempt) disqualifies it
    assert_eq!(g.voting_controller, 1590);
    assert_eq!(g.earnings_y1[1590], Ext::from_f64(0.0));
    assert_eq!(g.extraordinary_pl[1590], 0);
    assert!(ui.log.iter().any(|l| l.contains(__content.string(14420))
        && l.contains(__content.string(13626))));
    assert_eq!(g.largest_holder_pct, 0, "16 < 20 is reset inside the (already entered) industry-71 block");
}

#[test]
fn players_and_bad_ids() {
    let mut g = game();
    run(&mut g, 1);
    assert_eq!(g.voting_controller, 1);
    assert_eq!(g.corp_controller[1], 1);
    let mut g = game();
    g.voting_controller = 5;
    run(&mut g, 0);
    assert_eq!(g.voting_controller, 5, "id 0: error exit before anything is reset");
    g.entity_status[22] = 2;
    run(&mut g, 22);
    assert_eq!(g.corp_controller[22], 0, "inactive company: nobody");
}

#[test]
fn sanitize_growth_and_spending() {
    let mut g = game();
    let mut rng = Rng::default();
    g.industry_num[20] = 2;
    g.growth_rate[20] = 9;
    g.disc_spend_pct[20] = 50;
    sanitize_corp_growth_spending(&mut g, &mut rng, 20);
    assert!((-4..=4).contains(&g.growth_rate[20]));
    assert!((3..=6).contains(&g.disc_spend_pct[20]));

    g.industry_num[21] = 7;
    g.growth_rate[21] = 3;
    g.ind_demand_gap[7] = cast::<f64, _>(Ext::from_f64(12.0));
    g.disc_spend_pct[21] = 4;
    sanitize_corp_growth_spending(&mut g, &mut rng, 21);
    assert!((10..=12).contains(&g.growth_rate[21]));
    assert_eq!(g.disc_spend_pct[21], 4);

    g.growth_rate[22] = 40;
    sanitize_corp_growth_spending(&mut g, &mut rng, 22);
    assert!((9..=13).contains(&g.growth_rate[22]), "{}", g.growth_rate[22]);

    // reproducible with the RNG state
    let mut a = game();
    let mut b = game();
    a.industry_num[20] = 2;
    a.growth_rate[20] = -30;
    b.industry_num[20] = 2;
    b.growth_rate[20] = -30;
    sanitize_corp_growth_spending(&mut a, &mut Rng::from_state(12345), 20);
    sanitize_corp_growth_spending(&mut b, &mut Rng::from_state(12345), 20);
    assert_eq!(a.growth_rate[20], b.growth_rate[20]);
}

#[test]
fn option_positions() {
    let __content = wsr_core::content::current();
    let mut g = game();
    let mut ui = Scripted::default();
    g.option_recs[0] = p("2");
    g.option_recs[1] = p("20|1|1|10|x|x|x|GRANT");
    g.option_recs[2] = p("20|2|1|-5");
    g.option_recs[3] = p("25|1|1|-3");
    let mut f = |g: &mut Globals, h, c, t, e| find_option_positions(g, &mut ui, h, c, t, e);
    assert_eq!(f(&mut g, 0, 20, 0, 0), 0);
    assert_eq!(g.option_recs[0], p(" 3"));
    assert_eq!(g.has_options_outstanding[20], 1);
    assert_eq!(f(&mut g, 1, 20, 1, 0), 1);
    assert_eq!(f(&mut g, 1, 20, 1, 1), 0, "granted calls excluded");
    assert_eq!(f(&mut g, 1, 20, 2, 0), -31);
    assert_eq!(f(&mut g, 1, 20, 0, 0), 52);
    assert_eq!(f(&mut g, 1, 0, 1, 0), 12);
    assert_eq!(g.has_options_outstanding[25], 1);
    assert_eq!(f(&mut g, 1, 0, 2, 0), -21);
    assert_eq!(f(&mut g, 1, 0, 0, 0), 42);
    assert_eq!(f(&mut g, 2, 25, 1, 0), 0);
    assert_eq!(f(&mut g, 1, 20, 7, 0), 0);
    assert_eq!(f(&mut g, 1, 5, 1, 0), 0);
    assert_eq!(
        ui.log,
        vec![
            __content.string(14421).to_string(),
            __content.string(14422).to_string()
        ]
    );
}

#[test]
fn holding_finders() {
    let __content = wsr_core::content::current();
    let mut g = game();
    hold(&mut g, 1, 1, 20, 10);
    hold(&mut g, 1, 3, 30, -5);
    assert_eq!(find_holding_slot(&g, 1, 20), 1);
    assert_eq!(find_holding_slot(&g, 1, 30), 3);
    assert_eq!(find_holding_slot(&g, 1, 40), 2, "first empty slot");
    assert_eq!(find_stock_holding(&g, 1, 20), 1);
    assert_eq!(find_stock_holding(&g, 1, 30), -3, "{__wsr_text_13627}",__wsr_text_13627=__content.string(13627));
    assert_eq!(find_stock_holding(&g, 1, 40), 0);

    g.hold_bond_id[(1, 5)] = 20;
    g.hold_bond_id[(2, 5)] = 0;
    assert_eq!(find_bond_holding_slot(&g, 5, 20), 1);
    assert_eq!(find_bond_holding_slot(&g, 5, 99), 15, "the last empty slot");
    assert_eq!(find_bond_holding_slot(&g, 1581, 99), 0, "fund 1581 is excluded");
    assert_eq!(find_bond_holding_slot(&g, 1582, 99), 15);
    assert_eq!(find_bond_holding_slot(&g, 131, 99), 15);
    assert_eq!(find_bond_holding_slot(&g, 132, 99), 0);

    g.market_cap[20] = Ext::from_f64(1000.0);
    g.market_cap[30] = Ext::from_f64(5000.0);
    g.market_cap[31] = Ext::from_f64(300.0);
    hold(&mut g, 1, 4, 31, 50);
    assert_eq!(get_largest_holding_id(&g, 1), 31, "30 is short; 300*50 > 1000*10");
    g.market_cap[31] = Ext::from_f64(200.0);
    assert_eq!(get_largest_holding_id(&g, 1), 20, "a tie keeps the first");
    assert_eq!(get_largest_holding_id(&g, 0), 0);
}

// ------------------------------------------------------------------ differential tests (original code)

mod diff;

use diff::{Compare, Opts, Rand, run_original_with};

/// Players 1..n, companies 11..60 (a few inactive) with cross holdings concentrated on few targets,
/// random previous controllers (never 6..10: the original's IsComputerPlayer(1 TO 5) would raise error 9).
fn random_control_state(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let mut r = Rand(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut g = Globals::default();
    g.num_players = cast::<i32, _>(r.range(1, 4));
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    g.has_cpu_players = PbStr::from(if r.chance(Ext::from_f64(0.5)) { "Y" } else { "N" });
    g.action_code = PbStr::from(if r.chance(Ext::from_f64(0.3)) { "AUTO" } else { "" });
    g.suppress_popups = 1; // popups write TMP_QM.DAT, which fails under emulation
    g.cur_year = 2031;
    g.rand_seed = cast::<i32, _>(r.range(0, 30000));
    for pl in 1..=g.num_players {
        g.entity_status[pl] = 1;
        g.is_computer_player[pl] = cast::<i32, _>(r.range(0, 1));
        g.player_name[pl] = PbStr::from(if r.chance(Ext::from_f64(0.1)) { "...OUT..." } else { "Somebody" });
        g.ent_name[pl] = PbStr::from(format!("{__wsr_text_13556}{pl}",__wsr_text_13556=__content.string(13556)).as_str());
        g.ent_symbol[pl] = PbStr::from(format!("P{pl}").as_str());
        g.auto_pilot[pl] = cast::<i32, _>(r.range(0, 1));
        g.player_ceo_of[pl] = cast::<i32, _>(r.range(0, 60));
    }
    let targets = [cast::<i16, _>(r.range(11, 60)), cast::<i16, _>(r.range(11, 60)), cast::<i16, _>(r.range(11, 60))];
    for c in 11..=60 {
        g.entity_status[c] = if r.chance(Ext::from_f64(0.92)) { 1 } else { 2 };
        g.ent_name[c] = PbStr::from(format!("{__wsr_text_13628}{c}",__wsr_text_13628=__content.string(13628)).as_str());
        g.ent_symbol[c] = PbStr::from(format!("CO{c}").as_str());
        g.industry_num[c] = [1, 2, 5, 9, 33, 69, 70][cast::<usize, _>(r.range(0, 6))];
        g.growth_rate[c] = cast::<i16, _>(r.range(-10, 40));
        g.disc_spend_pct[c] = cast::<i16, _>(r.range(0, 15));
        g.credit_rating_num[c] = cast::<i32, _>(r.range(1, 10));
        g.credit_frozen[c] = cast::<i32, _>(r.range(0, 1));
        g.auto_pilot[c] = cast::<i32, _>(r.range(0, 1));
        g.ctrl_since_year[c] = if r.chance(Ext::from_f64(0.5)) { 0 } else { cast::<i32, _>(r.range(1990, 2030)) };
        if r.chance(Ext::from_f64(0.3)) {
            let f: Vec<String> = (1..=r.range(20, 35)).map(|_| r.range(0, 60).to_string()).collect();
            g.ent_misc_data[c] = PbStr::from(f.join("|").as_str());
        }
        g.corp_controller[c] = match r.range(0, 3) {
            0 => c,
            1 => 0,
            2 => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
            _ => cast::<i32, _>(r.range(11, 60)),
        };
    }
    for i in 1..=60 {
        g.ind_demand_gap[i] = cast::<f64, _>(cast::<Ext, _>(r.range(0, 20)));
    }
    for owner in (1..=g.num_players).chain(11..=60) {
        for slot in 1..=15 {
            if r.chance(Ext::from_f64(0.35)) {
                let c = if r.chance(Ext::from_f64(0.6)) { targets[cast::<usize, _>(r.range(0, 2))] } else { cast::<i16, _>(r.range(11, 60)) };
                g.hold_stock_id[(slot, owner)] = c;
                g.hold_stock_pct[(slot, owner)] =
                    if r.chance(Ext::from_f64(0.1)) { -(cast::<i16, _>(r.range(1, 10))) } else { cast::<i16, _>(r.range(1, 60)) };
            }
        }
    }
    for i in 1..=60 {
        g.news_headline[i] = PbStr::from(format!("old {i}").as_str());
    }
    g
}

#[test]
fn update_corp_control_matches_original() {
    let __content = wsr_core::content::current();
    let mut changes = 0;
    for seed in 1u64..=10 {
        let g0 = random_control_state(seed);
        let mut r = Rand(seed + 1000);
        for k in 0..3 {
            let corp = if k == 0 { cast::<i32, _>(r.range(1, 4)) } else { cast::<i32, _>(r.range(11, 60)) };
            let rng0 = (cast::<u32, _>(seed)).wrapping_mul(2_654_435_761) ^ k;
            let opts = Opts { args: vec![corp.to_string()], rng: Some(rng0), ..Opts::default() };
            let Some(o) = run_original_with(0x41f71b, &opts, &g0) else {
                return;
            };
            let mut port = g0.clone();
            let mut ui = Scripted::default();
            let mut rng = Rng::from_state(rng0);
            update_corp_control(&mut port, &mut ui, &mut rng, corp);
            assert!(o.log.is_empty(), "seed {seed} corp {corp}: {:?}", o.log);
            assert_eq!(rng.state(), o.rng, "seed {seed} corp {corp}: RND state");
            if 10 < corp && g0.corp_controller[corp] != port.corp_controller[corp] {
                changes += 1;
            }
            Compare::default().assert(&port, &o.globals);
        }
    }
    eprintln!("{changes}{__wsr_text_13629}",__wsr_text_13629=__content.string(13629));
    assert!(changes > 3);
}

#[test]
fn sanitize_corp_growth_spending_matches_original() {
    for seed in 1u64..=6 {
        let g0 = random_control_state(seed);
        let mut r = Rand(seed + 77);
        let corp = cast::<i32, _>(r.range(5, 60));
        let rng0 = 0x1234_5678 ^ cast::<u32, _>(seed);
        let opts = Opts { args: vec![corp.to_string()], rng: Some(rng0), ..Opts::default() };
        let Some(o) = run_original_with(0x4a14a7, &opts, &g0) else {
            return;
        };
        let mut port = g0.clone();
        let mut rng = Rng::from_state(rng0);
        sanitize_corp_growth_spending(&mut port, &mut rng, corp);
        assert_eq!(rng.state(), o.rng);
        Compare::default().assert(&port, &o.globals);
    }
}

#[test]
fn find_option_positions_matches_original() {
    for seed in 1u64..=4 {
        let mut g0 = random_control_state(seed);
        let mut r = Rand(seed + 5);
        let n = r.range(0, 30);
        for i in 1..=n {
            let slot = cast::<i32, _>(r.range(1, 140));
            g0.option_recs[slot] = PbStr::from(
                format!(
                    "{}|{}|{}|{}|x|y|z|{}",
                    r.range(11, 20),
                    r.range(1, 2),
                    r.range(0, 4),
                    r.range(-5, 5),
                    if r.chance(Ext::from_f64(0.3)) { "GRANT" } else { "" }
                )
                .as_str(),
            );
            if i == 1 {
                g0.option_recs[0] = PbStr::from(r.range(0, 9).to_string().as_str());
            }
        }
        for (holder, corp, ty, ex) in [(0, 15, 0, 0), (1, 15, 1, 0), (1, 12, 1, 1), (2, 13, 2, 0), (3, 14, 0, 0), (1, 0, 1, 0), (2, 0, 2, 0), (3, 0, 0, 0)] {
            let opts = Opts {
                args: vec![holder.to_string(), corp.to_string(), ty.to_string(), ex.to_string()],
                ..Opts::default()
            };
            let Some(o) = run_original_with(0x5f0cb9, &opts, &g0) else {
                return;
            };
            let mut port = g0.clone();
            let mut ui = Scripted::default();
            let rv = find_option_positions(&mut port, &mut ui, holder, corp, ty, ex);
            assert_eq!(rv, cast::<i32, _>(o.return_value), "seed {seed} {holder} {corp} {ty} {ex}");
            Compare::default().assert(&port, &o.globals);
        }
    }
}

#[test]
fn holding_finders_match_original() {
    let mut g = random_control_state(3);
    let mut r = Rand(99);
    for e in [1, 5, 130, 131, 132, 1581, 1582, 1583] {
        for slot in 1..=15 {
            if r.chance(Ext::from_f64(0.5)) {
                g.hold_bond_id[(slot, e)] = cast::<i32, _>(r.range(1, 40));
            }
        }
    }
    for c in 11..=60 {
        g.market_cap[c] = cast::<Ext, _>(r.range(0, 100_000)) / Ext::from_f64(3.0);
    }
    for k in 0..12 {
        let holder = [1, 2, 11, 20, 33, 47, 60][k % 7];
        let target = cast::<i32, _>(r.range(11, 60));
        for (addr, port) in [
            (0x498c61u32, find_holding_slot(&g, holder, target)),
            (0x498d53, find_stock_holding(&g, holder, target)),
        ] {
            let opts = Opts { args: vec![holder.to_string(), target.to_string()], ..Opts::default() };
            let Some(o) = run_original_with(addr, &opts, &g) else {
                return;
            };
            assert_eq!(port, cast::<i32, _>(o.return_value), "0x{addr:x}({holder}, {target})");
        }
        let opts = Opts { args: vec![holder.to_string()], ..Opts::default() };
        let Some(o) = run_original_with(0x49100a, &opts, &g) else {
            return;
        };
        assert_eq!(get_largest_holding_id(&g, holder), cast::<i32, _>(o.return_value), "GetLargestHoldingId({holder})");
    }
    for e in [1, 5, 130, 131, 132, 1581, 1582, 1583] {
        let issuer = cast::<i32, _>(r.range(1, 40));
        let opts = Opts { args: vec![e.to_string(), issuer.to_string()], ..Opts::default() };
        let Some(o) = run_original_with(0x498e40, &opts, &g) else {
            return;
        };
        assert_eq!(find_bond_holding_slot(&g, e, issuer), cast::<i32, _>(o.return_value), "FindBondHoldingSlot({e}, {issuer})");
    }
}

/// Loss of control by the human player with popups on: two ShowQuickMessage calls (TMP_QM.DAT) and two
/// headlines.
#[test]
fn update_corp_control_with_popups_matches_original() {
    let mut g = game();
    hold(&mut g, 1, 1, 20, 10);
    hold(&mut g, 2, 1, 20, 30);
    g.corp_controller[20] = 1;
    let opts = Opts { args: vec!["20".into()], rng: Some(4711), ..Opts::default() };
    let Some(o) = run_original_with(0x41f71b, &opts, &g) else {
        return;
    };
    let mut port = g.clone();
    let mut ui = Scripted::default();
    let mut rng = Rng::from_state(4711);
    update_corp_control(&mut port, &mut ui, &mut rng, 20);
    let files = o.log.iter().filter(|l| l.contains("TMP_QM.DAT")).count();
    assert_eq!(files, ui.log.iter().filter(|l| l.starts_with("MSGBOX")).count());
    assert_eq!(rng.state(), o.rng);
    Compare::default().ignore("TickerPauseUntil").assert(&port, &o.globals);
}
