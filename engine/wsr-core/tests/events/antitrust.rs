//! Differential tests of `logic::events::antitrust`: CalcAntitrustDamages, FindAntitrustPlaintiff,
//! ProcessAntitrustSuitQueue, AiFileAntitrustSuits, CheckPrivateAntitrustSuit, GovtAntitrustEnforcement
//! and AntitrustReviewAcquisition against the original routines.
use wsr_core::pb::real::{Ext, cast};


use super::check::*;
use super::common::*;
use super::diff::*;
use std::sync::Mutex;
use wsr_core::logic::events::*;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

const CALC_DAMAGES: u32 = 0x5414a0;
const FIND_PLAINTIFF: u32 = 0x53eee8;
const SUIT_QUEUE: u32 = 0x53f52a;
const AI_FILE: u32 = 0x5407f6;
const CHECK_PRIVATE: u32 = 0x6823ff;
const GOVT_ENFORCE: u32 = 0x68114c;
const REVIEW: u32 = 0x510aec;

/// Branch names hit by the seeds of a test (asserted at the end).
struct Hits(Mutex<Vec<String>>);
impl Hits {
    fn new() -> Self {
        Hits(Mutex::new(Vec::new()))
    }
    fn hit(&self, b: &str) {
        self.0.lock().unwrap().push(b.to_string());
    }
    #[track_caller]
    fn assert(&self, want: &[&str]) {
        let v = self.0.lock().unwrap();
        let mut u = v.clone();
        u.sort();
        let counts: Vec<(String, usize)> = {
            let mut d = u.clone();
            d.dedup();
            d.iter()
                .map(|b| (b.clone(), u.iter().filter(|x| *x == b).count()))
                .collect()
        };
        eprintln!("branch hits: {counts:?}");
        if v.is_empty() {
            return; // harness unavailable
        }
        for w in want {
            assert!(v.iter().any(|value| value == *w), "branch {w:?} never hit; hits: {:?}", {
                let mut u = v.clone();
                u.sort();
                u.dedup();
                u
            });
        }
    }
}

/// Active operating corps of `ind`.
fn corps_of(g: &Globals, ind: i16) -> Vec<i32> {
    (131..=1580)
        .filter(|&c| g.entity_status[c] == 1 && g.industry_num[c] == ind)
        .collect()
}

/// Makes `ctrl` control `n` corps of `ind` holding about `share` of the industry's business assets.
fn monopolize(
    g: &mut Globals,
    r: &mut Rand,
    ind: i16,
    ctrl: i32,
    n: usize,
    share: Ext,
) -> Vec<i32> {
    let corps = corps_of(g, ind);
    let picked: Vec<i32> = corps.iter().take(n.max(1)).cloned().collect();
    let others: Ext = corps
        .iter()
        .filter(|c| !picked.contains(c))
        .map(|&c| g.business_assets[c])
        .sum();
    let total_mine = if share >= Ext::from_f64(1.0) {
        Ext::from_f64(1000.0)
    } else {
        others * share / (Ext::from_f64(1.0) - share)
    };
    for (k, &c) in picked.iter().enumerate() {
        g.corp_controller[c] = ctrl;
        g.business_assets[c] =
            total_mine / cast::<Ext, _>(picked.len()) * if k == 0 { Ext::from_f64(1.3) } else { Ext::from_f64(0.7) } + f64r(r, Ext::from_f64(0.0), Ext::from_f64(5.0));
    }
    picked
}

/// A suit record `"1 |def|plt|share|damages|settle|"` as the game writes it.
fn suit_rec(def: i32, plt: i32, share: i32, damages: i64, settle: i64) -> PbStrS {
    st(&format!("1 | {def}| {plt}| {share}|{damages}|{settle}|"))
}
type PbStrS = wsr_core::pb::pbstr::PbStr;

fn random_suits(g: &mut Globals, r: &mut Rand, n: i32) {
    for slot in 1..=10 {
        g.lawsuit_rec[slot] = PbStrS::new();
    }
    for slot in 1..=n {
        let def = cast::<i32, _>(r.range(131, 1580));
        let plt = cast::<i32, _>(r.range(131, 1580));
        g.lawsuit_rec[slot] = if r.chance(Ext::from_f64(0.15)) {
            st(&format!("2|{def}|{plt}|{}|", r.range(1, 5)))
        } else {
            suit_rec(def, plt, cast::<i32, _>(r.range(45, 90)), r.range(50, 5000), 0)
        };
    }
    for k in 1..=10 {
        g.recent_antitrust_filers[k] = if r.chance(Ext::from_f64(0.3)) {
            cast::<i32, _>(r.range(131, 1580))
        } else {
            0
        };
    }
}

// -------------------------------------------------------------------------------- CalcAntitrustDamages

#[test]
fn diff_calc_antitrust_damages() {
    let __content = wsr_core::content::current();
    let hits = Hits::new();
    par_seeds(1..=40, |seed| {
        let mut r = Rand(seed * 7919 + 3);
        let mut g = world(seed + 3000);
        let p = active_corp(&g, &mut r);
        let d = active_corp(&g, &mut r);
        g.exchange_rate = cast::<f64, _>([Ext::from_f64(1.0), Ext::from_f64(0.001), Ext::from_f64(0.01), Ext::from_f64(2.0)][cast::<usize, _>(seed % 4)]);
        g.private_antitrust_off = if seed % 13 == 0 { 1 } else { 0 };
        g.contingency_reserve[d] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(500.0));
        if seed % 5 == 1 {
            g.cash[p] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(30.0));
            g.t_bills[p] = Ext::from_f64(0.0);
        }
        if seed % 5 == 3 {
            g.exchange_rate = cast::<f64, _>(Ext::from_f64(1.0));
            g.cash[p] = f64r(&mut r, Ext::from_f64(60.0), Ext::from_f64(120.0));
            g.t_bills[p] = Ext::from_f64(0.0);
            g.cash[d] = Ext::from_f64(3000.0);
        }
        let share = [50, 60, 71, 72, 74, 75, 90][cast::<usize, _>(r.range(0, 6))];
        let rs = cast::<u32, _>(r.next());
        let args = [format!("v:{p}"), format!("v:{d}"), format!("{__wsr_text_13741}{share}",__wsr_text_13741=__content.string(13741))];
        let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let Some(o) = run(CALC_DAMAGES, &args, &g, rs, &[], &[]) else {
            return;
        };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        let rv = calc_antitrust_damages(&mut port, &mut ui, &mut rng, p, d, share);
        let what = format!("CalcAntitrustDamages seed {seed}");
        check(&what, &port, &rng, &ui, &o);
        assert_eq!(Some(cast::<Ext, _>(rv)), o.return_float, "{what}: result");
        if g.private_antitrust_off != 1
            && g.exchange_rate * Ext::from_f64(50.0) < port.liquid_funds
            && share < 75
            && cast::<Ext, _>(rv) * Ext::from_f64(0.06) / Ext::from_f64(0.95) > port.liquid_funds
        {
            hits.hit("shrink loop");
        }
        if g.private_antitrust_off == 1 {
            hits.hit("off");
        } else if rv < 100 {
            hits.hit("shrunk");
        } else {
            hits.hit("damages");
        }
    });
    hits.assert(&["off", "damages", "shrink loop"]);
}

// ----------------------------------------------------------------------------- FindAntitrustPlaintiff

#[test]
fn diff_find_antitrust_plaintiff() {
    let hits = Hits::new();
    par_seeds(1..=40, |seed| {
        let mut r = Rand(seed * 6037 + 17);
        let mut g = world(seed + 3100);
        let def = active_corp(&g, &mut r);
        let ind = if seed % 11 == 0 {
            [1, 2, 70][cast::<usize, _>(r.range(0, 2))]
        } else {
            g.industry_num[def]
        };
        g.action_code = if seed % 3 == 0 { st("") } else { st("AUTO") };
        g.exchange_rate = cast::<f64, _>([Ext::from_f64(1.0), Ext::from_f64(0.1), Ext::from_f64(0.01)][cast::<usize, _>(seed % 3)]);
        g.private_antitrust_off = if seed % 17 == 0 { 1 } else { 0 };
        let nq = cast::<i32, _>(r.range(0, 6));
        random_suits(&mut g, &mut r, nq);
        let comp = corps_of(&g, g.industry_num[def]);
        // some competitors in the queue / recent filers / with a player's management
        for &c in comp.iter().take(3) {
            if r.chance(Ext::from_f64(0.3)) {
                g.recent_antitrust_filers[cast::<i32, _>(r.range(1, 10))] = c;
            }
            if r.chance(Ext::from_f64(0.3)) {
                g.lawsuit_rec[cast::<i32, _>(r.range(1, 10))] =
                    suit_rec(cast::<i32, _>(r.range(131, 1580)), c, 60, 100, 0);
            }
            if r.chance(Ext::from_f64(0.3)) {
                g.corp_controller[c] = g.corp_controller[def];
            }
        }
        if r.chance(Ext::from_f64(0.3)) && !comp.is_empty() {
            g.antitrust_plaintiff = comp[cast::<usize, _>(r.range(0, cast::<i64, _>(comp.len()) - 1))];
            g.antitrust_defendant = comp[cast::<usize, _>(r.range(0, cast::<i64, _>(comp.len()) - 1))];
        }
        g.antitrust_market_share = cast::<i32, _>(r.range(40, 80));
        let share = cast::<i32, _>(r.range(48, 90));
        let rs = cast::<u32, _>(r.next());
        let args = [def.to_string(), format!("i:{ind}"), share.to_string()];
        let args = [args[1].as_str(), args[0].as_str(), args[2].as_str()];
        let Some(o) = run(FIND_PLAINTIFF, &args, &g, rs, &[], &[]) else {
            return;
        };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        let rv = find_antitrust_plaintiff(&mut port, &mut ui, &mut rng, ind, def, share);
        let what = format!("FindAntitrustPlaintiff seed {seed}");
        check(&what, &port, &rng, &ui, &o);
        assert_eq!(rv, cast::<i32, _>(o.return_value), "{what}: result");
        hits.hit(if rv > 0 { "found" } else { "none" });
    });
    hits.assert(&["found", "none"]);
}

// -------------------------------------------------------------------------- ProcessAntitrustSuitQueue

#[test]
fn diff_process_antitrust_suit_queue() {
    let hits = Hits::new();
    par_seeds(1..=48, |seed| {
        let mut r = Rand(seed * 4111 + 29);
        let mut g = world(seed + 3200);
        g.sim_co_count = if seed % 12 == 0 {
            198
        } else {
            199 + 200 * cast::<i32, _>(r.range(0, 6))
        };
        g.private_antitrust_off = if seed % 19 == 0 { 1 } else { 0 };
        g.action_code = if seed % 2 == 0 { st("AUTO") } else { st("") };
        g.exchange_rate = cast::<f64, _>([Ext::from_f64(1.0), Ext::from_f64(0.1)][cast::<usize, _>(seed % 2)]);
        let nq = [0, 1, 2, 3, 5][cast::<usize, _>(r.range(0, 4))];
        random_suits(&mut g, &mut r, nq);
        g.antitrust_scan_industry = cast::<i32, _>(r.range(0, 69));
        let ind = cast::<i16, _>(if g.antitrust_scan_industry >= 69 {
            3
        } else {
            (g.antitrust_scan_industry + 1).max(3)
        });
        let ctrl = if r.chance(Ext::from_f64(0.5)) {
            cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)))
        } else {
            cast::<i32, _>(r.range(131, 1580))
        };
        let (n, sh) = (cast::<usize, _>(r.range(1, 3)), f64r(&mut r, Ext::from_f64(0.45), Ext::from_f64(0.9)));
        monopolize(&mut g, &mut r, ind, ctrl, n, sh);
        match seed % 5 {
            0 | 1 => {
                // a queued suit becomes active
                let d = active_corp(&g, &mut r);
                let p = active_corp(&g, &mut r);
                if r.chance(Ext::from_f64(0.3)) {
                    g.corp_controller[p] = g.corp_controller[d];
                }
                for k in (2..=10).rev() {
                    g.lawsuit_rec[k] = g.lawsuit_rec[k - 1].clone();
                }
                g.lawsuit_rec[1] = suit_rec(d, p, 70, r.range(100, 9000), r.range(0, 50));
                g.antitrust_plaintiff = 0;
                g.antitrust_defendant = 0;
            }
            2 => {
                // defendant without plaintiff: scans the defendant's industry
                g.antitrust_defendant = active_corp(&g, &mut r);
                g.antitrust_plaintiff = 0;
                let i2 = g.industry_num[g.antitrust_defendant];
                monopolize(&mut g, &mut r, i2, ctrl, 1, Ext::from_f64(0.8));
            }
            3 => {
                g.antitrust_defendant = active_corp(&g, &mut r);
                g.antitrust_plaintiff = active_corp(&g, &mut r);
                g.antitrust_market_share = if r.chance(Ext::from_f64(0.5)) { 0 } else { 60 };
            }
            _ => {}
        }
        if seed % 7 == 3 {
            for c in corps_of(&g, ind) {
                g.earnings_yr3_ago[c] = Ext::from_f64(0.0);
                g.earnings_yr2_ago[c] = Ext::from_f64(0.0);
            }
        }
        let before = (1..=10).filter(|&k| !g.lawsuit_rec[k].is_empty()).count();
        let rs = cast::<u32, _>(r.next());
        let Some(o) = run(SUIT_QUEUE, &[], &g, rs, &[], &[]) else {
            return;
        };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        process_antitrust_suit_queue(&mut port, &mut ui, &mut rng);
        let what = format!("ProcessAntitrustSuitQueue seed {seed}");
        check(&what, &port, &rng, &ui, &o);
        let after = (1..=10)
            .filter(|&k| !port.lawsuit_rec[k].is_empty())
            .count();
        if port.antitrust_plaintiff != g.antitrust_plaintiff && port.antitrust_plaintiff != 0 {
            hits.hit("activated");
        }
        if g.lawsuit_rec[1].as_bytes().starts_with(b"1")
            && g.antitrust_plaintiff == 0
            && port.antitrust_plaintiff == 0
        {
            hits.hit("cancelled");
        }
        if after > before
            || (port.antitrust_plaintiff != 0 && after == before && g.antitrust_plaintiff == 0)
        {
            hits.hit("queued");
        }
    });
    hits.assert(&["activated", "cancelled", "queued"]);
}

// ------------------------------------------------------------------------------- AiFileAntitrustSuits

#[test]
fn diff_ai_file_antitrust_suits() {
    let __content = wsr_core::content::current();
    let hits = Hits::new();
    par_seeds(1..=48, |seed| {
        let mut r = Rand(seed * 3301 + 41);
        let mut g = world(seed + 3300);
        g.private_antitrust_off = if seed % 23 == 0 { 1 } else { 0 };
        g.action_code = if seed % 2 == 0 { st("AUTO") } else { st("") };
        g.exchange_rate = cast::<f64, _>([Ext::from_f64(1.0), Ext::from_f64(0.1), Ext::from_f64(0.01)][cast::<usize, _>(seed % 3)]);
        let nq = [0, 0, 1, 2, 4, 6][cast::<usize, _>(r.range(0, 5))];
        random_suits(&mut g, &mut r, nq);
        if seed % 9 == 4 {
            for k in 1..=10 {
                g.lawsuit_rec[k] = suit_rec(200, 300, 60, 100, 0);
            }
        }
        for _ in 0..r.range(1, 4) {
            let ind = cast::<i16, _>(r.range(3, 68));
            let ctrl = match r.range(0, 2) {
                0 => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
                1 => cast::<i32, _>(r.range(131, 1580)),
                _ => 0,
            };
            let (n, sh) = (cast::<usize, _>(r.range(1, 3)), f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(0.9)));
            monopolize(&mut g, &mut r, ind, ctrl, n, sh);
            if r.chance(Ext::from_f64(0.4)) {
                g.ind_demand_gap[cast::<i32, _>(ind)] = cast::<f64, _>(f64r(&mut r, Ext::from_f64(5.5), Ext::from_f64(40.0)));
                for c in corps_of(&g, ind) {
                    if g.corp_controller[c] != ctrl {
                        g.recent_antitrust_filers[cast::<i32, _>(r.range(1, 10))] = c;
                    }
                }
            }
        }
        if seed % 3 == 1 {
            // one controller owns the whole industry: no plaintiff, a new company may be started
            let ind = cast::<i16, _>(r.range(3, 68));
            let ctrl = cast::<i32, _>(r.range(131, 1580));
            let mut total = Ext::from_f64(0.0);
            for c in corps_of(&g, ind) {
                if c != ctrl {
                    g.corp_controller[c] = ctrl;
                    total += g.business_assets[c];
                }
            }
            g.ind_demand[cast::<i32, _>(ind)] = cast::<i64, _>(total * Ext::from_f64(2.0)) + 100;
            g.crowded_industry = 0;
        }
        let rs = cast::<u32, _>(r.next());
        let Some(mut o) = run(AI_FILE, &[], &g, rs, &[], &[]) else {
            return;
        };
        // GetControllerType(leaderController) with no controller reads CorpController(0) (bug 145)
        o.log.retain(|l| !l.starts_with("PBERR 9 in 0x4a09b9"));
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        let rv = ai_file_antitrust_suits(&mut port, &mut ui, &mut rng, 0);
        let what = format!("AiFileAntitrustSuits seed {seed}");
        check(&what, &port, &rng, &ui, &o);
        assert_eq!(rv, cast::<i32, _>(o.return_value), "{what}: result");
        hits.hit(if rv > 0 { "filed" } else { "none" });
        if port
            .entity_status
            .as_slice()
            .iter()
            .zip(g.entity_status.as_slice())
            .any(|(a, b)| a != b)
        {
            hits.hit(__content.string(13811));
        }
    });
    hits.assert(&["filed", "none", __content.string(13811)]);
}

// -------------------------------------------------------------------------- CheckPrivateAntitrustSuit

#[test]
fn diff_check_private_antitrust_suit() {
    let hits = Hits::new();
    par_seeds(1..=24, |seed| {
        let mut r = Rand(seed * 2203 + 5);
        let mut g = world(seed + 3400);
        let corp = active_corp(&g, &mut r);
        let ind = g.industry_num[corp];
        let ctrl = cast::<i32, _>(r.range(131, 1580));
        monopolize(
            &mut g,
            &mut r,
            ind,
            ctrl,
            1,
            if seed % 2 == 0 { Ext::from_f64(0.7) } else { Ext::from_f64(0.2) },
        );
        let big = corps_of(&g, ind)[0];
        let nq = cast::<i32, _>(r.range(0, 5));
        random_suits(&mut g, &mut r, nq);
        for k in 1..=10 {
            if r.chance(Ext::from_f64(0.3)) {
                g.lawsuit_rec[k] = suit_rec(
                    big,
                    cast::<i32, _>(r.range(131, 1580)),
                    55,
                    r.range(50, 900),
                    r.range(0, 9),
                );
            }
        }
        let rs = cast::<u32, _>(r.next());
        let args = [big.to_string(), format!("i:{ind}")];
        let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let Some(o) = run(CHECK_PRIVATE, &args, &g, rs, &[], &[]) else {
            return;
        };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        check_private_antitrust_suit(&mut port, &mut ui, &mut rng, big, ind);
        let what = format!("CheckPrivateAntitrustSuit seed {seed}");
        check(&what, &port, &rng, &ui, &o);
        let n0 = (1..=10).filter(|&k| !g.lawsuit_rec[k].is_empty()).count();
        let n1 = (1..=10)
            .filter(|&k| !port.lawsuit_rec[k].is_empty())
            .count();
        if n1 < n0 {
            hits.hit("dropped");
        } else if (1..=10).any(|k| port.lawsuit_rec[k] != g.lawsuit_rec[k]) {
            hits.hit("rewritten");
        }
    });
    hits.assert(&["dropped", "rewritten"]);
}

// -------------------------------------------------------------------------- GovtAntitrustEnforcement

#[test]
fn diff_govt_antitrust_enforcement() {
    let __content = wsr_core::content::current();
    let hits = Hits::new();
    par_seeds(1..=64, |seed| {
        let mut r = Rand(seed * 1543 + 7);
        let mut g = world(seed + 3500);
        g.govt_antitrust_off = if seed % 29 == 0 { 1 } else { 0 };
        let ind = cast::<i16, _>(r.range(3, 68));
        g.sim_co_count = if seed % 15 == 0 {
            2
        } else {
            500 * cast::<i32, _>(r.range(0, 2)) + cast::<i32, _>(ind)
        };
        g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
        g.is_computer_player[g.cur_player] = 0;
        g.cur_player_name = g.player_name[g.cur_player].clone();
        g.player_antitrust_level[g.cur_player] = [0, 0, 1, 1, 2, 3, 4, 5][cast::<usize, _>(r.range(0, 7))];
        g.difficulty_level = cast::<i32, _>(r.range(1, 3));
        g.currency_code = st(["USA", "UK", "EUR", "CAN", "AUS", "JPN"][cast::<usize, _>(r.range(0, 5))]);
        g.antitrust_act_key = if r.chance(Ext::from_f64(0.2)) {
            g.cur_quarter + g.cur_year
        } else {
            0
        };
        let n = if seed % 3 == 0 {
            1
        } else {
            cast::<usize, _>(r.range(2, 4))
        };
        let (cp0, sh) = (g.cur_player, f64r(&mut r, Ext::from_f64(0.55), Ext::from_f64(0.97)));
        let mine = monopolize(&mut g, &mut r, ind, cp0, n, sh);
        let cp = g.cur_player;
        // the player holds the stock (directly or through a controlled corp)
        for (k, &c) in mine.iter().enumerate() {
            let holder = if k % 2 == 0 || mine.len() == 1 {
                cp
            } else {
                mine[0]
            };
            for slot in 1..=15 {
                if g.hold_stock_id[(slot, holder)] == 0 || slot == 15 {
                    g.hold_stock_id[(slot, holder)] = cast::<i16, _>(c);
                    g.hold_stock_pct[(slot, holder)] = cast::<i16, _>(r.range(51, 90));
                    g.hold_stock_basis[(slot, holder)] = f64r(&mut r, Ext::from_f64(10.0), Ext::from_f64(500.0));
                    break;
                }
            }
            g.hold_stock_id[(0, holder)] = cast::<i16, _>((1..=15)
                .filter(|&s| g.hold_stock_id[(s, holder)] != 0)
                .count());
        }
        if r.chance(Ext::from_f64(0.3)) {
            g.last_divest_corp = mine[0];
        }
        g.growth_rate[mine[0]] = cast::<i16, _>(r.range(0, 40));
        let rs = cast::<u32, _>(r.next());
        let Some(o) = run(GOVT_ENFORCE, &[], &g, rs, &[], &[]) else {
            return;
        };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        govt_antitrust_enforcement(&mut port, &mut ui, &mut rng, 0);
        let what = format!("GovtAntitrustEnforcement seed {seed}");
        check(&what, &port, &rng, &ui, &o);
        if ui
            .titles
            .iter()
            .any(|t| t.starts_with(__content.string(13812)))
        {
            hits.hit("warning");
        }
        if ui.titles.iter().any(|t| t.starts_with(__content.string(13813))) {
            hits.hit("fine");
            if ui.msgs.iter().any(|m| m.contains(__content.string(13814))) {
                hits.hit("divest");
            }
        }
    });
    hits.assert(&["warning", "fine", "divest"]);
}

// ------------------------------------------------------------------------ AntitrustReviewAcquisition

#[test]
fn diff_antitrust_review_acquisition() {
    let __content = wsr_core::content::current();
    let hits = Hits::new();
    par_seeds(1..=64, |seed| {
        let mut r = Rand(seed * 977 + 13);
        let mut g = world(seed + 3600);
        g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
        let cp = g.cur_player;
        g.antitrust_cheat_used = if seed % 31 == 0 { 1 } else { 0 };
        let ind = cast::<i16, _>(r.range(3, 68));
        let corps = corps_of(&g, ind);
        let mut mode = [0, 0, 0, 1, 2][cast::<usize, _>(r.range(0, 4))];
        let mut review_mode = [1, 1, 2, 3][cast::<usize, _>(r.range(0, 3))];
        let mut acquirer = if seed % 4 == 0 {
            cp
        } else {
            corps.first().cloned().unwrap_or(131)
        };
        let n = corps.len();
        let mut target = corps.get(n / 2).cloned().unwrap_or(500);
        let scenario = if n >= 6 { seed % 4 } else { 0 };
        let others = |g: &mut Globals, r: &mut Rand, skip: &[i32]| -> Ext {
            let mut sum = Ext::from_f64(0.0);
            for &c in &corps {
                if !skip.contains(&c) {
                    g.corp_controller[c] = cast::<i32, _>(r.range(131, 1580));
                    sum += g.business_assets[c];
                }
            }
            sum
        };
        if scenario == 1 {
            // lenient policy: the player has ~15%, would get ~57%
            mode = 0;
            acquirer = cp;
            let p = corps[0];
            target = corps[1];
            let o = others(&mut g, &mut r, &[p, target]);
            g.corp_controller[p] = cp;
            g.business_assets[p] = o * Ext::from_f64(0.15) / Ext::from_f64(0.43);
            g.corp_controller[target] = 0;
            g.business_assets[target] = o * Ext::from_f64(0.42) / Ext::from_f64(0.43);
        } else if scenario == 2 {
            // blocked; the acquirer could divest its  [runtime resource wording]
            mode = 0;
            review_mode = 1;
            let (a, b) = (corps[0], corps[1]);
            acquirer = a;
            target = corps[2];
            let o = others(&mut g, &mut r, &[a, b, target]);
            for &c in &[a, b] {
                g.corp_controller[c] = cp;
                g.business_assets[c] = o * Ext::from_f64(0.3);
            }
            g.corp_controller[target] = 0;
            g.business_assets[target] = o * Ext::from_f64(0.9);
            for slot in 1..=15 {
                g.hold_stock_id[(slot, a)] = 0;
                g.hold_stock_id[(slot, cp)] = 0;
            }
            g.hold_stock_id[(1, a)] = cast::<i16, _>(b);
            g.hold_stock_pct[(1, a)] = 60;
            g.hold_stock_basis[(1, a)] = Ext::from_f64(100.0);
            g.hold_stock_id[(0, a)] = 1;
            g.hold_stock_id[(1, cp)] = cast::<i16, _>(a);
            g.hold_stock_pct[(1, cp)] = 70;
            g.hold_stock_basis[(1, cp)] = Ext::from_f64(100.0);
            g.hold_stock_id[(0, cp)] = 1;
        } else {
            // the acquiring side: the player's corps (and corps controlled by the acquirer)
            for (k, &c) in corps.iter().enumerate() {
                if k < n / 3 {
                    g.corp_controller[c] = if mode < 2 { cp } else { acquirer };
                    g.business_assets[c] =
                        f64r(&mut r, Ext::from_f64(50.0), Ext::from_f64(3000.0)) * [Ext::from_f64(0.2), Ext::from_f64(1.0), Ext::from_f64(3.0)][cast::<usize, _>(seed % 3)];
                }
            }
            g.corp_controller[target] = if r.chance(Ext::from_f64(0.3)) {
                cast::<i32, _>(r.range(131, 1580))
            } else {
                0
            };
            g.business_assets[target] = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(4000.0));
            if r.chance(Ext::from_f64(0.5)) {
                // a company of the target's group in the same industry
                if let Some(&sub) = corps.get(n / 2 + 1) {
                    g.corp_controller[sub] = target;
                    g.hold_stock_id[(1, target)] = cast::<i16, _>(sub);
                    g.hold_stock_pct[(1, target)] = 60;
                    g.hold_stock_id[(0, target)] = g.hold_stock_id[(0, target)].max(1);
                }
            }
            if seed % 6 == 1 {
                g.corp_controller[target] = cp;
            }
            if seed % 5 == 2 {
                // acquirer is the target's largest holder
                g.hold_stock_id[(2, acquirer)] = cast::<i16, _>(target);
                g.hold_stock_pct[(2, acquirer)] = 40;
            }
        }
        g.country_idx[target] = [0, 3, 14, 50, 7][cast::<usize, _>(r.range(0, 4))];
        g.country_idx[acquirer] = cast::<i32, _>(r.range(0, 2));
        g.tmp_ext1 = cast::<Ext, _>(acquirer);
        g.tmp_ext2 = cast::<Ext, _>(target);
        g.tmp_ext3 = cast::<Ext, _>(mode);
        let n_ans = if scenario > 0 { 3 } else { r.range(0, 3) };
        let ans: Vec<&str> = (0..n_ans)
            .map(|_| if r.chance(Ext::from_f64(0.6)) { "IDYES" } else { "IDNO" })
            .collect();
        let rs = cast::<u32, _>(r.next());
        let args = [review_mode.to_string()];
        let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let Some(o) = run(REVIEW, &args, &g, rs, &ans, &[]) else {
            return;
        };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        for a in &ans {
            ui.answers.push_back(if *a == "IDYES" { 6 } else { 7 });
        }
        let rv = antitrust_review_acquisition(&mut port, &mut ui, &mut rng, review_mode);
        let what =
            format!("AntitrustReviewAcquisition seed {seed}{__wsr_text_13548}{mode} review {review_mode}",__wsr_text_13548=__content.string(13548));
        check(&what, &port, &rng, &ui, &o);
        assert_eq!(rv, cast::<i32, _>(o.return_value), "{what}: result");
        assert_eq!(ui.reports.len(), o.dialogs.len(), "{what}: reports");
        hits.hit(if rv == 1 { "allowed" } else { "blocked" });
        if !ui.reports.is_empty() {
            hits.hit("ruling");
        }
        if ui.titles.iter().any(|t| t.starts_with(__content.string(13815))) {
            hits.hit("lenient");
        }
        if ui.titles.iter().any(|t| t.starts_with(__content.string(13816))) {
            hits.hit("divest offer");
        }
        if port.news_headline.as_slice() != g.news_headline.as_slice() {
            hits.hit("news");
        }
    });
    hits.assert(&[
        "allowed",
        "blocked",
        "ruling",
        "news",
        "lenient",
        "divest offer",
    ]);
}
