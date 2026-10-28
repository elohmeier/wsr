//! Differential tests of `logic::etf_derivs::option_expiry`: ExerciseOption, ProcessExpiringOptions and
//! OptionExpiryReminder against the original.
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use std::collections::HashMap;
use std::sync::Mutex;
use wsr_core::logic::etf_derivs::option_expiry::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

const EXERCISE_OPTION: u32 = 0x5f993e;
const PROCESS_EXPIRING: u32 = 0x5f2963;
const EXPIRY_REMINDER: u32 = 0x5ffdbc;
const SKIPS: &[(u32, u32)] = &[];

fn rng0(seed: u64) -> u32 {
    (cast::<u32, _>(seed)).wrapping_mul(2_246_822_519) ^ 0x5bd1_e995
}

/// Branch/outcome counters over all seeds of a test.
static COV: Mutex<Option<HashMap<String, usize>>> = Mutex::new(None);

fn cov(key: &str) {
    let mut m = COV.lock().unwrap();
    *m.get_or_insert_with(HashMap::new)
        .entry(key.to_string())
        .or_default() += 1;
}

fn cov_get(key: &str) -> usize {
    COV.lock()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(key).copied())
        .unwrap_or(0)
}

/// Option records hold STR$ of EXT values (strike, premium) computed in 80-bit precision; the port works
/// in f64, so the 15th digit of such a field can differ. Takes the original's record where only numeric
/// fields differ, by at most 1e-12 relative.
fn align_option_recs(port: &mut Globals, orig: &Globals) {
    for i in 0..=1000 {
        let (a, b) = (
            port.option_recs[i].to_display(),
            orig.option_recs[i].to_display(),
        );
        if a == b {
            continue;
        }
        let (pa, pb): (Vec<&str>, Vec<&str>) = (a.split('|').collect(), b.split('|').collect());
        if pa.len() != pb.len() || pa.len() < 2 {
            continue;
        }
        let close = pa.iter().zip(&pb).all(|(x, y)| {
            if x == y {
                return true;
            }
            match (x.trim().parse::<Ext>(), y.trim().parse::<Ext>()) {
                (Ok(x), Ok(y)) => (x - y).abs() <= Ext::from_f64(1e-12) * y.abs().max(Ext::from_f64(1e-300)),
                _ => false,
            }
        });
        if close {
            port.option_recs[i] = orig.option_recs[i].clone();
        }
    }
}

#[track_caller]
fn check(port: &Globals, rng: &Rng, ui: &Rec, o: &Original, what: &str) {
    let __content = wsr_core::content::current();
    if std::env::var("DBG").is_ok() {
        for l in o.log.iter().filter(|l| !l.starts_with("API")) {
            eprintln!("{what}: {l}");
        }
        eprintln!("{what}: port msgs {:?}{__wsr_text_13611}{:?}", ui.msgs, ui.news,__wsr_text_13611=__content.string(13611));
        if let Some(Some(f)) = o
            .files
            .iter()
            .find(|(k, _)| k.to_uppercase().contains("TMP_QM"))
            .map(|(_, v)| v)
        {
            eprintln!("{what}{__wsr_text_13793}{:?}", String::from_utf8_lossy(f),__wsr_text_13793=__content.string(13793));
        }
    }
    let mut port = port.clone();
    align_option_recs(&mut port, &o.globals);
    cmp().rel_tol(Ext::from_f64(1e-9)).assert(&port, &o.globals);
    assert_eq!(rng.state(), o.rng, "{what}: RND state");
    assert_eq!(
        ui.msgs.len(),
        orig_msgs(o),
        "{what}: message boxes {:?}",
        ui.msgs
    );
}

/// The last pop-up's text against the original's `TMP_QM.DAT` (CR/LF encoded as `@@` / `##`).
#[track_caller]
fn check_last_popup(ui: &Rec, o: &Original, what: &str) -> bool {
    let qm = o
        .files
        .iter()
        .find(|(k, _)| k.to_uppercase().contains("TMP_QM"))
        .and_then(|(_, v)| v.clone());
    let Some(i) = (0..ui.msgs.len()).rev().find(|&i| ui.titles[i].is_empty()) else {
        return false;
    };
    let Some(f) = qm else {
        return false;
    };
    let enc = ui.msgs[i].replace("\r\n\r\n", "@@").replace("\r\n", "##");
    assert_eq!(
        String::from_utf8_lossy(&f).trim_end_matches(['\r', '\n']),
        enc,
        "{what}: popup text"
    );
    true
}

/// `STR$`-like text of an integer (leading space for non-negative values).
fn st_i(x: i64) -> String {
    if x < 0 {
        x.to_string()
    } else {
        format!(" {x}")
    }
}

/// [`world`] with plausible share prices, option settings of the players, funds 1581..1600 with
/// advisers and option records on stocks with various holders (players, corporations without, with a
/// player and with a corporate controller, funds, insurers, banks), many of them expiring now.
fn opt_world(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let mut g = world(seed);
    let mut r = Rand(seed * 37 + 11);
    g.sim_co_count = cast::<i32, _>(r.range(10, 1600));
    for e in 131..=1600 {
        if r.chance(Ext::from_f64(0.9)) {
            g.shares_outstanding[e] = g.market_cap[e] / f64r(&mut r, Ext::from_f64(5.0), Ext::from_f64(300.0));
        }
    }
    // smaller stakes so that most stocks have a public float
    for e in 1..=1600 {
        for slot in 1..=15 {
            if g.hold_stock_pct[(slot, e)] > 12 && r.chance(Ext::from_f64(0.85)) {
                g.hold_stock_pct[(slot, e)] = cast::<i16, _>(r.range(1, 12));
            }
        }
    }
    for p in 1..=5 {
        g.player_exercise_options[p] = cast::<i32, _>(r.range(0, 1));
        g.player_take_delivery[p] = cast::<i32, _>(r.range(0, 1));
    }
    for f in 1581..=1600 {
        if !r.chance(Ext::from_f64(0.8)) {
            continue;
        }
        g.entity_status[f] = 1;
        g.industry_num[f] = 71;
        g.fund_adviser[f] = cast::<i32, _>(r.range(86, 1580));
        g.ctrl_since_year[f] = g.cur_year - cast::<i32, _>(r.range(0, 5));
        g.corp_controller[f] = 0;
    }
    // some advisers controlled by players
    for f in 1581..=1600 {
        if r.chance(Ext::from_f64(0.5)) {
            let a = g.fund_adviser[f];
            if (86..=1580).contains(&a) {
                g.entity_status[a] = 1;
                g.corp_controller[a] = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
            }
        }
    }
    let month = (g.cur_quarter - 1) * 3 + cast::<i32, _>(r.range(1, 3));
    let n = cast::<i32, _>(r.range(1, 30));
    for i in 1..=n {
        let corp = cast::<i32, _>(r.range(131, 1580));
        let holder = match r.range(0, 9) {
            0 | 1 => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
            2 => cast::<i32, _>(r.range(1581, 1600)),
            3 => cast::<i32, _>(r.range(86, 129)),
            4 => cast::<i32, _>(r.range(11, 85)),
            _ => cast::<i32, _>(r.range(131, 1580)),
        };
        if holder > 10 && r.chance(Ext::from_f64(0.5)) {
            g.corp_controller[holder] = match r.range(0, 2) {
                0 => 0,
                1 => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
                _ => cast::<i32, _>(r.range(131, 1580)),
            };
        }
        let typ = r.range(1, 2);
        let mut qty = r.range(1, 15);
        if r.chance(Ext::from_f64(0.4)) {
            qty = -qty;
        }
        // stock to deliver for long puts / written calls, or an existing stake for buyers
        let sells = (typ == 2 && qty > 0) || (typ == 1 && qty < 0);
        if r.chance(if sells { Ext::from_f64(0.75) } else { Ext::from_f64(0.3) }) {
            let h = holder;
            let mut slot = (1..=15)
                .find(|&s| cast::<i32, _>(g.hold_stock_id[(s, h)]) == corp)
                .unwrap_or(0);
            if slot == 0 {
                slot = (1..=15)
                    .find(|&s| g.hold_stock_id[(s, h)] == 0)
                    .unwrap_or(0);
            }
            if slot > 0 {
                g.hold_stock_id[(slot, h)] = cast::<i16, _>(corp);
                g.hold_stock_pct[(slot, h)] = cast::<i16, _>(r.range(1, 20));
                g.hold_stock_basis[(slot, h)] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(500.0));
                g.hold_stock_id[(0, h)] =
                    cast::<i16, _>((1..=15).filter(|&s| g.hold_stock_id[(s, h)] != 0).count());
            }
        }
        if r.chance(Ext::from_f64(0.3)) {
            // free up a slot for a call buyer
            let h = holder;
            if (1..=15).all(|s| g.hold_stock_id[(s, h)] != 0) {
                g.hold_stock_id[(15, h)] = 0;
                g.hold_stock_pct[(15, h)] = 0;
            }
        }
        let strike = g.market_cap[corp] * f64r(&mut r, Ext::from_f64(0.6), Ext::from_f64(1.4));
        let (m, y) = if r.chance(Ext::from_f64(0.75)) {
            (month - cast::<i32, _>(r.range(0, 2)), g.cur_year)
        } else if r.chance(Ext::from_f64(0.5)) {
            (cast::<i32, _>(r.range(1, 12)), g.cur_year - 1)
        } else {
            (cast::<i32, _>(r.range(1, 12)), g.cur_year + cast::<i32, _>(r.range(0, 1)))
        };
        let m = m.max(1);
        let prem = match r.range(0, 5) {
            0 => " VESTED".to_string(),
            1 => " GRANT".to_string(),
            _ => format!(" {}", f64r(&mut r, Ext::from(Ext::from_f64(0.0)), Ext::from(Ext::from_f64(80.0)))),
        };
        g.option_recs[i] = PbStr::from(
            format!(
                "{}|{}|{}|{}| {}|{}|{}|{}",
                st_i(cast::<i64, _>(corp)),
                st_i(typ),
                st_i(cast::<i64, _>(holder)),
                st_i(qty),
                strike,
                st_i(cast::<i64, _>(m)),
                st_i(cast::<i64, _>(y)),
                prem
            )
            .as_str(),
        );
    }
    g.option_recs[0] = PbStr::from(n.to_string().as_str());
    if r.chance(Ext::from_f64(0.3)) {
        g.news_msg = PbStr::from(__content.string(13571));
    }
    g.active_entity = cast::<i32, _>(r.range(1, 1600));
    g.sel_entity = cast::<i32, _>(r.range(1, 1600));
    g.active_entity_name = g.ent_name[g.active_entity].clone();
    g.lender_id = cast::<i32, _>(r.range(11, 85));
    g
}

#[test]
fn diff_exercise_option() {
    let __content = wsr_core::content::current();
    par_seeds(1..=180, |seed| {
        let mut g = opt_world(seed);
        let mut r = Rand(seed * 7 + 3);
        let n: i32 = g.option_recs[0].to_display().trim().parse().unwrap();
        let rec = cast::<i32, _>(r.range(1, cast::<i64, _>(n)));
        if seed % 3 == 0 {
            // an in-the-money long call of a player (VESTED or not), possibly short of cash
            let mut f: Vec<String> = g.option_recs[rec]
                .to_display()
                .split('|')
                .map(|x| x.to_string())
                .collect();
            let corp: i32 = f[0].trim().parse().unwrap();
            let holder = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
            f[1] = st_i(1);
            f[2] = st_i(cast::<i64, _>(holder));
            f[3] = st_i(r.range(1, 8));
            f[4] = format!(" {}", g.market_cap[corp] * f64r(&mut r, Ext::from(Ext::from_f64(0.3)), Ext::from(Ext::from_f64(0.95))));
            if r.chance(Ext::from_f64(0.5)) {
                f[7] = " VESTED".to_string();
            }
            g.option_recs[rec] = PbStr::from(f.join("|").as_str());
            g.player_exercise_options[holder] = 1;
            if r.chance(Ext::from_f64(0.5)) {
                g.cash[holder] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(50.0));
                g.t_bills[holder] = Ext::from_f64(0.0);
            }
            if (1..=15).all(|s| g.hold_stock_id[(s, holder)] != 0) {
                g.hold_stock_id[(15, holder)] = 0;
                g.hold_stock_pct[(15, holder)] = 0;
            }
        }
        let manual = if r.chance(Ext::from_f64(0.15)) { 1 } else { 0 };
        let pct = if manual == 1 {
            cast::<i32, _>(r.range(0, 12))
        } else {
            0
        };
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        let v = exercise_option(&mut port, &mut ui, &mut rng, rec, manual, pct);
        let args = [rec.to_string(), manual.to_string(), pct.to_string()];
        let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let Some(o) = run(EXERCISE_OPTION, &args, &g, rs, &[], SKIPS) else {
            return;
        };
        let what = format!(
            "seed {seed} rec {} manual {manual} pct {pct}",
            g.option_recs[rec].to_display()
        );
        check(&port, &rng, &ui, &o, &what);
        assert_eq!(v, cast::<i32, _>(o.return_value), "{what}: result");
        cov(&format!("{__wsr_text_13794}{v}",__wsr_text_13794=__content.string(13794)));
        if o.log.iter().any(|l| l.contains("PBERR 9")) {
            cov("error 9");
        }
        check_last_popup(&ui, &o, &what);
        for (k, pat) in [
            (__content.string(13795), __content.string(13795)),
            ("puts sold", __content.string(13796)),
            ("calls bought", __content.string(13797)),
            ("put to", "put to"),
            ("vested", __content.string(4929)),
        ] {
            if ui.news.iter().any(|n| n.contains(pat)) {
                cov(k);
            }
        }
        let h: i32 = g.option_recs[rec]
            .to_display()
            .split('|')
            .nth(2)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        if port.bank_debt[h] != g.bank_debt[h] {
            cov(__content.string(13624));
        }
        if manual == 1 && v > 0 {
            cov("manual exercise");
        }
        if !ui.msgs.is_empty() {
            cov("msgs");
        }
    });
    if unavailable().is_none() {
        for k in [
            "result 0",
            "result 1",
            "result 2",
            "error 9",
            "msgs",
            __content.string(13795),
            "puts sold",
            "calls bought",
            "put to",
            "vested",
            __content.string(13624),
            "manual exercise",
        ] {
            eprintln!("COV {k}: {}", cov_get(k));
            assert!(cov_get(k) > 0, "branch not hit: {k}");
        }
    }
}

/// ExerciseOption of a corporation's long call that would give its group control of a company in an
/// industry the group already dominates: AntitrustReviewAcquisition(3) bars the deal and the exercise is
/// limited to the stake below control (or halted).
#[test]
fn diff_exercise_option_antitrust() {
    let __content = wsr_core::content::current();
    par_seeds(1..=48, |seed| {
        let mut g = opt_world(seed + 5000);
        let mut r = Rand(seed * 13 + 5);
        let ind = cast::<i16, _>(r.range(3, 69));
        let target = cast::<i32, _>(r.range(131, 1580));
        let mut holder = cast::<i32, _>(r.range(131, 1580));
        if holder == target {
            holder = if target < 1580 { target + 1 } else { target - 1 };
        }
        // the holder's controller: a player (not the current one) or another corporation
        let controller = if r.chance(Ext::from_f64(0.5)) {
            let p = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
            if p == g.cur_player { 0 } else { p }
        } else {
            cast::<i32, _>(r.range(131, 1580))
        };
        let controller = if controller == 0 || controller == target || controller == holder { 131 + (cast::<i32, _>(seed) % 50) * 7 } else { controller };
        for e in 131..=1600 {
            if g.industry_num[e] == ind {
                g.business_assets[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(50.0));
            }
        }
        for &(e, ctl) in &[(holder, controller), (target, 0)] {
            g.entity_status[e] = 1;
            g.industry_num[e] = ind;
            g.corp_controller[e] = ctl;
        }
        if controller > 10 {
            g.entity_status[controller] = 1;
        }
        // every 4th seed the group does not dominate the industry: the deal is allowed
        g.business_assets[holder] = if seed % 4 == 0 { Ext::from_f64(1.0) } else { f64r(&mut r, Ext::from_f64(3000.0), Ext::from_f64(9000.0)) };
        g.business_assets[target] = if seed % 4 == 0 { Ext::from_f64(1.0) } else { f64r(&mut r, Ext::from_f64(500.0), Ext::from_f64(3000.0)) };
        g.cash[holder] = Ext::from_f64(1e6);
        // nobody else holds the target; the holder owns 3..15% and calls on 20..45% more
        for h in 1..=1600 {
            for slot in 1..=15 {
                if cast::<i32, _>(g.hold_stock_id[(slot, h)]) == target {
                    g.hold_stock_id[(slot, h)] = 0;
                    g.hold_stock_pct[(slot, h)] = 0;
                }
            }
        }
        let slot = (1..=15).find(|&s| g.hold_stock_id[(s, holder)] == 0).unwrap_or(15);
        g.hold_stock_id[(slot, holder)] = cast::<i16, _>(target);
        g.hold_stock_pct[(slot, holder)] = cast::<i16, _>(r.range(3, 15));
        g.hold_stock_basis[(slot, holder)] = Ext::from_f64(100.0);
        g.hold_stock_id[(0, holder)] = cast::<i16, _>((1..=15).filter(|&s| g.hold_stock_id[(s, holder)] != 0).count());
        let qty = r.range(20, 45);
        let strike = g.market_cap[target] * f64r(&mut r, Ext::from_f64(0.3), Ext::from_f64(0.9));
        let rec = 1;
        g.option_recs[rec] = PbStr::from(
            format!("{}|{}|{}|{}| {}|{}|{}| 5", st_i(cast::<i64, _>(target)), st_i(1), st_i(cast::<i64, _>(holder)), st_i(qty), strike, st_i(1), st_i(cast::<i64, _>(g.cur_year))).as_str(),
        );
        let manual = 0;
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        let v = exercise_option(&mut port, &mut ui, &mut rng, rec, manual, 0);
        let Some(o) = run(EXERCISE_OPTION, &[&rec.to_string(), "0", "0"], &g, rs, &[], SKIPS) else {
            return;
        };
        let what = format!("antitrust seed {seed} rec {}", g.option_recs[rec].to_display());
        check(&port, &rng, &ui, &o, &what);
        assert_eq!(v, cast::<i32, _>(o.return_value), "{what}: result");
        if ui.news.iter().any(|n| n.contains(__content.string(13798))) {
            cov("antitrust limited");
        }
        if ui.news.iter().any(|n| n.contains(__content.string(13799))) {
            cov("antitrust halted");
        }
        if ui.news.iter().any(|n| n.contains(__content.string(13800))) {
            cov("antitrust allowed");
        }
    });
    if unavailable().is_none() {
        for k in ["antitrust limited", "antitrust allowed"] {
            eprintln!("COV {k}: {}", cov_get(k));
            assert!(cov_get(k) > 0, "branch not hit: {k}");
        }
        eprintln!("COV antitrust halted: {}", cov_get("antitrust halted"));
    }
}

#[test]
fn diff_process_expiring_options() {
    let __content = wsr_core::content::current();
    par_seeds(1..=24, |seed| {
        let g = opt_world(seed + 1000);
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        process_expiring_options(&mut port, &mut ui, &mut rng);
        let Some(o) = run(PROCESS_EXPIRING, &[], &g, rs, &[], SKIPS) else {
            return;
        };
        let what = format!("seed {seed}");
        check(&port, &rng, &ui, &o, &what);
        let left = (1..=1000)
            .filter(|&i| !port.option_recs[i].is_empty())
            .count();
        let before = (1..=1000).filter(|&i| !g.option_recs[i].is_empty()).count();
        if left < before {
            cov("expired");
        }
        if ui.news.iter().any(|n| n.contains(__content.string(4939))) {
            cov("expired news");
        }
        if ui.news.iter().any(|n| n.contains(__content.string(13801))) {
            cov("settled news");
        }
    });
    if unavailable().is_none() {
        for k in ["expired", "expired news", "settled news"] {
            eprintln!("COV {k}: {}", cov_get(k));
            assert!(cov_get(k) > 0, "branch not hit: {k}");
        }
    }
}

#[test]
fn diff_option_expiry_reminder() {
    let __content = wsr_core::content::current();
    par_seeds(1..=40, |seed| {
        let mut g = opt_world(seed + 2000);
        let mut r = Rand(seed * 3 + 1);
        // option records expiring next month for the current player and its companies
        let next = ((g.cur_quarter - 1) * 3 + cast::<i32, _>(r.range(1, 3))) % 12 + 1;
        let n: i32 = g.option_recs[0].to_display().trim().parse().unwrap();
        for i in 1..=n {
            if r.chance(Ext::from_f64(0.5)) {
                let rec = g.option_recs[i].to_display();
                let mut f: Vec<String> = rec.split('|').map(|x| x.to_string()).collect();
                f[5] = st_i(cast::<i64, _>(next));
                f[6] = st_i(cast::<i64, _>(if next == 1 {
                    g.cur_year + 1
                } else {
                    g.cur_year
                }));
                if r.chance(Ext::from_f64(0.5)) {
                    f[2] = st_i(cast::<i64, _>(g.cur_player));
                }
                g.option_recs[i] = PbStr::from(f.join("|").as_str());
            }
        }
        if r.chance(Ext::from_f64(0.1)) {
            g.option_recs[0] = PbStr::new();
        }
        if r.chance(Ext::from_f64(0.2)) {
            g.action_code = PbStr::from("AUTO");
        }
        if r.chance(Ext::from_f64(0.15)) {
            g.sim_co_count = 1500;
            g.cur_quarter = 4;
            g.game_end_year = g.cur_year + 1;
            g.player_cap_gains[g.cur_player] = f64r(&mut r, -Ext::from_f64(100.0), Ext::from_f64(100.0));
        }
        // futures records: kind|holder|qty|month|year|...
        let kinds = ["STOCKS", "OIL", "GOLD", "BITCOIN", "ETHEREUM", "WHEAT"];
        let nf = cast::<i32, _>(r.range(0, 8));
        for i in 1..=nf {
            let holder = if r.chance(Ext::from_f64(0.5)) {
                g.cur_player
            } else {
                cast::<i32, _>(r.range(11, 1600))
            };
            if holder > 10 && r.chance(Ext::from_f64(0.5)) {
                g.corp_controller[holder] = g.cur_player;
            }
            let qty = if r.chance(Ext::from_f64(0.5)) {
                r.range(1, 50)
            } else {
                -r.range(1, 50)
            };
            let (m, y) = if r.chance(Ext::from_f64(0.7)) {
                (
                    next,
                    if next == 1 {
                        g.cur_year + 1
                    } else {
                        g.cur_year
                    },
                )
            } else {
                (cast::<i32, _>(r.range(1, 12)), g.cur_year)
            };
            g.futures_recs[i] = PbStr::from(
                format!(
                    "{}|{}|{}|{}|{}| 123.5",
                    kinds[cast::<usize, _>(r.range(0, 5))],
                    st_i(cast::<i64, _>(holder)),
                    st_i(qty),
                    st_i(cast::<i64, _>(m)),
                    st_i(cast::<i64, _>(y))
                )
                .as_str(),
            );
        }
        g.futures_recs[0] = PbStr::from(nf.to_string().as_str());
        for m in 0..=13 {
            g.month_name[m] = PbStr::from(format!("{__wsr_text_13802}{m}",__wsr_text_13802=__content.string(13802)).as_str());
        }
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        option_expiry_reminder(&mut port, &mut ui, &mut rng);
        let Some(o) = run(EXPIRY_REMINDER, &[], &g, rs, &[], SKIPS) else {
            return;
        };
        let what = format!("seed {seed}");
        check(&port, &rng, &ui, &o, &what);
        let qm = o
            .files
            .iter()
            .find(|(k, _)| k.to_uppercase().contains("TMP_QM"))
            .and_then(|(_, v)| v.clone());
        if let (Some(m), Some(f)) = (ui.msgs.last(), qm) {
            // TMP_QM.DAT holds the text with CR/LF encoded as @@ / ##
            let enc = m.replace("\r\n\r\n", "@@").replace("\r\n", "##");
            assert_eq!(
                String::from_utf8_lossy(&f).trim_end_matches(['\r', '\n']),
                enc,
                "{what}: popup text"
            );
            cov("text checked");
        }
        for m in &ui.msgs {
            if m.contains(__content.string(13803)) {
                cov("options msg");
            }
            if m.contains(__content.string(13804)) {
                cov("futures msg");
            }
            if m.contains(__content.string(13805)) {
                cov("tax msg");
            }
            if m.contains(__content.string(4954)) {
                cov("both");
            }
        }
    });
    if unavailable().is_none() {
        for k in [
            "options msg",
            "futures msg",
            "tax msg",
            "both",
            "text checked",
        ] {
            eprintln!("COV {k}: {}", cov_get(k));
            assert!(cov_get(k) > 0, "branch not hit: {k}");
        }
    }
}
