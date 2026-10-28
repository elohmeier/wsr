//! Differential tests of `logic::ai_player::mgmt`: FireManagement (0x4844cb), ElectSelfCeo (0x4835d4),
//! CheckCeoOptionForfeiture (0x5f1dae) and the turn steps AiPlayer_TuneControlledCorps (0x5cc888) and
//! AiPlayer_FireCeos (0x5cd0cd).
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
use wsr_core::logic::ai_player::{check_ceo_option_forfeiture, elect_self_ceo, fire_management};
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

const FIRE_MGMT: u32 = 0x4844cb;
const ELECT_SELF_CEO: u32 = 0x4835d4;
const CHECK_FORFEIT: u32 = 0x5f1dae;
const TUNE: u32 = 0x5cc888;
const FIRE_CEOS: u32 = 0x5cd0cd;

fn answer_code(a: &str) -> i32 {
    match a {
        "IDNO" => 7,
        "IDCANCEL" => 2,
        "IDOK" => 1,
        _ => 6,
    }
}

/// Compares the port with the original; false if the original raised a runtime error.
#[track_caller]
fn check(what: &str, port: &Globals, rng: &Rng, ui: &Rec, o: &Original) -> bool {
    let errs: Vec<&String> = o.log.iter().filter(|l| l.starts_with("PBERR")).collect();
    if !errs.is_empty() {
        eprintln!("{what}: original raised {errs:?}; not compared");
        return false;
    }
    let d = cmp().rel_tol(Ext::from_f64(1e-12)).diffs(port, &o.globals);
    let log: Vec<&String> = o.log.iter().filter(|l| !l.starts_with("API")).take(30).collect();
    assert!(d.is_empty(), "{what}: {} differences:\n{}\nlog: {log:#?}", d.len(), d.iter().take(30).cloned().collect::<Vec<_>>().join("\n"));
    assert_eq!(rng.state(), o.rng, "{what}: RND state");
    assert_eq!(ui.msgs.len(), orig_msgs(o), "{what}: message boxes {:?} vs {log:#?}", ui.msgs);
    true
}

/// Management-change lists containing `c` with some probability.
fn mgmt_lists(g: &mut Globals, r: &mut Rand, c: i32) {
    let make = |r: &mut Rand, with_c: bool| {
        let n = [0, 1, 3, 8, 20][cast::<usize, _>(r.range(0, 4))];
        let mut s = String::new();
        for k in 0..n {
            let id = if with_c && k == r.range(0, n - 1) { c } else { cast::<i32, _>(r.range(11, 1600)) };
            s += &format!("{id:04}/");
        }
        if with_c && n == 0 {
            s = format!("{c:04}/");
        }
        st(&s)
    };
    let a = r.chance(cast::<f64, _>(Ext::from_f64(0.3)));
    let b = r.chance(cast::<f64, _>(Ext::from_f64(0.3)));
    g.new_mgmt_list = make(r, a);
    g.recent_mgmt_change_list = make(r, b);
}

// ---------------------------------------------------------------------------------- FireManagement

#[test]
fn diff_fire_management() {
    let __content = wsr_core::content::current();
    let asked = AtomicU32::new(0);
    let bank = AtomicU32::new(0);
    let shakeup = AtomicU32::new(0);
    let dismissed = AtomicU32::new(0);
    par_seeds(1..=48, |seed| {
        let mut g = ai_world(seed + 100);
        let mut r = Rand(seed * 1237 + 9);
        let c = match seed % 8 {
            0 => active_in(&g, &mut r, 11, 85),
            1 => cast::<i32, _>(r.range(1, 1600)),
            2 => {
                let c = active_corp(&g, &mut r);
                g.industry_num[c] = 70;
                c
            }
            _ => active_corp(&g, &mut r),
        };
        mgmt_lists(&mut g, &mut r, c);
        g.action_code = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { st("AUTO") } else { st("") };
        g.cur_player_name = g.player_name[g.cur_player].clone();
        g.profit_margin[c.clamp(1, 1600)] = [-30, -10, -3, 0, 5][cast::<usize, _>(r.range(0, 4))];
        g.market_cap[c.clamp(1, 1600)] = [f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(50.0)), f64r(&mut r, Ext::from_f64(50.0), Ext::from_f64(3000.0))][cast::<usize, _>(r.range(0, 1))];
        for p in 1..=g.num_players {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
                g.player_ceo_of[p] = c;
                g.player_ceo_since_year[p] = g.cur_year - cast::<i32, _>(r.range(0, 4));
            }
            g.auto_pilot[p] = cast::<i32, _>(r.range(0, 1));
        }
        g.rand_seed = cast::<i32, _>(r.range(0, 32000));
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.news_msg = st(__content.string(13571));
        }
        let ans = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { "IDNO" } else { "IDYES" };
        let rs = cast::<u32, _>(r.next());
        let Some(o) = run(FIRE_MGMT, &[&c.to_string()], &g, rs, &[ans], &[]) else { return };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        ui.answers.push_back(answer_code(ans));
        fire_management(&mut port, &mut ui, &mut rng, c);
        if !check(&format!("FireManagement seed {seed} corp {c}{__wsr_text_13572}{ans}",__wsr_text_13572=__content.string(13572)), &port, &rng, &ui, &o) {
            return;
        }
        if o.log.iter().any(|l| l.starts_with("MSGBOX")) {
            asked.fetch_add(1, Relaxed);
        }
        if (1..86).contains(&c) && port.turn_action_count != g.turn_action_count {
            bank.fetch_add(1, Relaxed);
        }
        if port.recent_mgmt_change_list != g.recent_mgmt_change_list || port.news_msg.to_display().contains(__content.string(13573)) {
            shakeup.fetch_add(1, Relaxed);
        }
        if (1..=g.num_players).any(|p| g.player_ceo_of[p] == c && port.player_ceo_of[p] == 0) {
            dismissed.fetch_add(1, Relaxed);
        }
    });
    if super::diff::unavailable().is_none() {
        let v = [asked.into_inner(), bank.into_inner(), shakeup.into_inner(), dismissed.into_inner()];
        eprintln!("{__wsr_text_13574}{v:?}",__wsr_text_13574=__content.string(13574));
        assert!(v.iter().all(|&x| x >= 1), "coverage (asked, bank, shake-up, dismissed): {v:?}");
    }
}

// ------------------------------------------------------------------------------------ ElectSelfCeo

/// A GRANT option record of `holder` on `corp` in a random OptionRecs slot.
fn add_grant(g: &mut Globals, r: &mut Rand, corp: i32, holder: i32) {
    let k = cast::<i32, _>(r.range(1, 1000));
    let note = if r.chance(cast::<f64, _>(Ext::from_f64(0.8))) { "GRANT" } else { "VESTED" };
    g.option_recs[k] = st(&format!("{corp}|1|{holder}|{}|{:.2}|6|{}|{note}", r.range(1, 50), f64r(r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(50.0))), g.cur_year + 2));
    g.option_recs[0] = st(&format!("{}", (1..=1000).filter(|&i| !g.option_recs[i].is_empty()).count()));
}

#[test]
fn diff_elect_self_ceo() {
    let __content = wsr_core::content::current();
    let elected = AtomicU32::new(0);
    let refused = AtomicU32::new(0);
    let fired_other = AtomicU32::new(0);
    par_seeds(1..=40, |seed| {
        let mut g = ai_world(seed + 500);
        let mut r = Rand(seed * 733 + 1);
        let cp = g.cur_player;
        g.is_computer_player[cp] = if seed % 3 == 0 { 0 } else { 1 };
        g.action_code = if g.is_computer_player[cp] == 1 || r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { st("AUTO") } else { st("") };
        g.cur_player_name = g.player_name[cp].clone();
        let c = active_corp(&g, &mut r);
        let old = match r.range(0, 3) {
            0 => 0,
            1 => c,
            _ => active_corp(&g, &mut r),
        };
        g.player_ceo_of[cp] = old;
        g.player_ceo_since_year[cp] = if old > 0 { g.cur_year - cast::<i32, _>(r.range(0, 4)) } else { 0 };
        if old > 10 && r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
            g.entity_status[old] = 2;
        }
        if old > 10 && r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            add_grant(&mut g, &mut r, old, cp);
        }
        for p in 1..=g.num_players {
            if p != cp && r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
                g.player_ceo_of[p] = c;
                g.player_ceo_since_year[p] = g.cur_year - 1;
            }
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            g.corp_controller[c] = cp;
        }
        g.tmp_ext1 = cast::<Ext, _>(c);
        let ans: Vec<&str> = (0..3).map(|_| if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { "IDNO" } else { "IDYES" }).collect();
        let rs = cast::<u32, _>(r.next());
        let Some(o) = run(ELECT_SELF_CEO, &[], &g, rs, &ans, &[]) else { return };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        ui.answers.extend(ans.iter().map(|a| answer_code(a)));
        elect_self_ceo(&mut port, &mut ui, &mut rng);
        if !check(&format!("ElectSelfCeo seed {seed} corp {c}{__wsr_text_13575}{old}{__wsr_text_13576}{ans:?}",__wsr_text_13575=__content.string(13575), __wsr_text_13576=__content.string(13576)), &port, &rng, &ui, &o) {
            return;
        }
        if port.player_ceo_of[cp] == c && old != c {
            elected.fetch_add(1, Relaxed);
        } else if old != c {
            refused.fetch_add(1, Relaxed);
        }
        if (1..=g.num_players).any(|p| p != cp && g.player_ceo_of[p] == c && port.player_ceo_of[p] == 0) {
            fired_other.fetch_add(1, Relaxed);
        }
    });
    if super::diff::unavailable().is_none() {
        let v = [elected.into_inner(), refused.into_inner(), fired_other.into_inner()];
        eprintln!("{__wsr_text_13574}{v:?}",__wsr_text_13574=__content.string(13574));
        assert!(v.iter().all(|&x| x >= 2), "coverage (elected, refused, fired other): {v:?}");
    }
}

// ------------------------------------------------------------------------- CheckCeoOptionForfeiture

#[test]
fn diff_check_ceo_option_forfeiture() {
    let __content = wsr_core::content::current();
    let ones = AtomicU32::new(0);
    let zeros = AtomicU32::new(0);
    par_seeds(1..=24, |seed| {
        let mut g = ai_world(seed + 700);
        let mut r = Rand(seed * 17 + 4);
        let p = match seed % 6 {
            0 => 0,
            1 => g.num_players + 1,
            _ => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
        };
        g.is_computer_player[p.clamp(1, 5)] = cast::<i32, _>(r.range(0, 1));
        let corp = active_corp(&g, &mut r);
        let new_corp = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { active_corp(&g, &mut r) } else { 0 };
        for _ in 0..r.range(0, 3) {
            let holder = if r.chance(cast::<f64, _>(Ext::from_f64(0.8))) { p.max(1) } else { cast::<i32, _>(r.range(1, 4)) };
            add_grant(&mut g, &mut r, corp, holder);
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.corp_controller[corp] = g.cur_player;
        }
        let ans = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { "IDNO" } else { "IDYES" };
        let args = [p.to_string(), corp.to_string(), new_corp.to_string()];
        let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let Some(o) = run(CHECK_FORFEIT, &args, &g, 1, &[ans], &[]) else { return };
        let mut port = g.clone();
        let rng = Rng::from_state(1);
        let mut ui = Rec::default();
        ui.answers.push_back(answer_code(ans));
        let res = check_ceo_option_forfeiture(&mut port, &mut ui, p, corp, new_corp);
        let what = format!("CheckCeoOptionForfeiture seed {seed}{__wsr_text_13565}{p} corp {corp}",__wsr_text_13565=__content.string(13565));
        if !check(&what, &port, &rng, &ui, &o) {
            return;
        }
        assert_eq!(cast::<u32, _>(res), o.return_value, "{what}: result");
        if res == 1 { ones.fetch_add(1, Relaxed) } else { zeros.fetch_add(1, Relaxed) };
    });
    if super::diff::unavailable().is_none() {
        let v = [ones.into_inner(), zeros.into_inner()];
        eprintln!("{__wsr_text_13574}{v:?}",__wsr_text_13574=__content.string(13574));
        assert!(v[0] >= 3 && v[1] >= 3);
    }
}

// --------------------------------------------------------------------------------------- turn steps

#[test]
fn diff_step_tune_controlled_corps() {
    let __content = wsr_core::content::current();
    let fired = AtomicU32::new(0);
    let tuned = AtomicU32::new(0);
    par_seeds(1..=24, |seed| {
        let mut g = ai_world(seed + 900);
        let mut r = Rand(seed * 41 + 2);
        let p = g.cur_player;
        g.ai_div_check_quarter = if seed % 6 == 0 { g.cur_quarter } else { 0 };
        // the player controls a few loss makers
        for _ in 0..r.range(0, 12) {
            let c = active_in(&g, &mut r, 11, 1580);
            g.corp_controller[c] = p;
            g.profit_margin[c] = cast::<i16, _>(r.range(-15, 10));
            g.disc_spend_pct[c] = cast::<i16, _>(r.range(0, 6));
            g.growth_rate[c] = cast::<i16, _>(r.range(-5, 60));
        }
        g.difficulty_level = cast::<i32, _>(r.range(1, 4));
        let t = cast::<i32, _>(g.hold_stock_id[(1, p)]);
        if t > 10 && r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            g.dividend_per_share[t] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(0.2));
            g.earnings_y1[t] = f64r(&mut r, Ext::from_f64(10.0), Ext::from_f64(500.0));
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                g.corp_controller[t] = 0;
            }
        }
        mgmt_lists(&mut g, &mut r, 0);
        g.cur_player_name = g.player_name[p].clone();
        let rs = cast::<u32, _>(r.next());
        let Some(t) = run_turn(&g, rs, Some(&[TUNE]), &[], &[]) else { return };
        if !check_turn(&format!("TuneControlledCorps seed {seed}"), &t, Ext::from_f64(1e-12)) {
            return;
        }
        if t.changed_field("NewMgmtList") {
            fired.fetch_add(1, Relaxed);
        }
        if t.changed_field("DiscSpendPct") || t.changed_field("GrowthRate") {
            tuned.fetch_add(1, Relaxed);
        }
    });
    if super::diff::unavailable().is_none() {
        let v = [fired.into_inner(), tuned.into_inner()];
        eprintln!("{__wsr_text_13574}{v:?}",__wsr_text_13574=__content.string(13574));
        assert!(v.iter().all(|&x| x >= 2), "coverage (fired, tuned): {v:?}");
    }
}

#[test]
fn diff_step_fire_ceos() {
    let __content = wsr_core::content::current();
    let fired = AtomicU32::new(0);
    let elected = AtomicU32::new(0);
    par_seeds(1..=24, |seed| {
        let mut g = ai_world(seed + 1300);
        let mut r = Rand(seed * 53 + 8);
        let p = g.cur_player;
        let mut owned: Vec<i32> = (11..=1600).filter(|&c| g.entity_status[c] == 1 && g.corp_controller[c] == p).collect();
        for _ in 0..r.range(0, 4) {
            let c = active_corp(&g, &mut r);
            g.corp_controller[c] = p;
            owned.push(c);
        }
        for q in 1..=g.num_players {
            if !owned.is_empty() && r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                g.player_ceo_of[q] = owned[cast::<usize, _>(r.range(0, cast::<i64, _>(owned.len()) - 1))];
                g.player_ceo_since_year[q] = g.cur_year - cast::<i32, _>(r.range(0, 3));
            }
        }
        g.cur_player_name = g.player_name[p].clone();
        let rs = cast::<u32, _>(r.next());
        let Some(t) = run_turn(&g, rs, Some(&[FIRE_CEOS]), &[], &[]) else { return };
        if !check_turn(&format!("FireCeos seed {seed}"), &t, Ext::from_f64(1e-12)) {
            return;
        }
        if (1..=g.num_players).any(|q| q != p && g.player_ceo_of[q] != 0 && t.port.player_ceo_of[q] == 0) {
            fired.fetch_add(1, Relaxed);
        }
        if t.port.player_ceo_of[p] != g.player_ceo_of[p] {
            elected.fetch_add(1, Relaxed);
        }
    });
    if super::diff::unavailable().is_none() {
        let v = [fired.into_inner(), elected.into_inner()];
        eprintln!("{__wsr_text_13574}{v:?}",__wsr_text_13574=__content.string(13574));
        assert!(v.iter().all(|&x| x >= 2), "coverage (fired, elected): {v:?}");
    }
}
