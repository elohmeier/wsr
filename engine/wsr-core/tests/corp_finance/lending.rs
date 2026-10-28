//! Differential tests of logic::corp_finance::lending (advances, loan calls, credit freezes, corporate
//! policy, AutoPilot).
//!
//! The port is driven by a scripted Ui ([`Drv`]) that answers every MSGBOX / INPUTBOX / pick list from a
//! script and logs the equivalent harness answers (`IDYES`, the entered text, `dblclick:301:N` on the
//! report list box) in call order; the original is then run with that log as its answer queue. A pick
//! in a report is followed by the report dialog (`FollowReportLink`), as in the original.
#![allow(unused_imports, clippy::type_complexity)]
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use std::collections::VecDeque;
use wsr_core::logic::corp_finance::lending::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::{ReportAction, Ui};

const DEMAND_ADVANCE_REPAYMENT: u32 = 0x5be3f5;
const CALL_IN_LOAN: u32 = 0x595994;
const ADVANCE_FUNDS_TO_CORP: u32 = 0x5bdaaf;
const FREEZE_ALL_LENDING: u32 = 0x59d7d0;
const FREEZE_CREDIT_LINE: u32 = 0x59d28a;
const CHANGE_CORP_POLICY: u32 = 0x5acf48;
const TOGGLE_AUTO_PILOT: u32 = 0x5b65b9;

/// Callees skipped in the original: none.
const SKIPS: &[(u32, u32)] = &[];

/// Scripted Ui that logs the harness answers of everything it answers.
#[derive(Default)]
struct Drv {
    /// MSGBOX answers (default YES / OK).
    boxes: VecDeque<i32>,
    /// INPUTBOX answers (default "").
    inputs: VecDeque<String>,
    /// Pick-list choices: `Some(k)` = the k-th (mod count) link line, `None` = no pick (OK).
    picks: VecDeque<Option<usize>>,
    /// Link ids picked whenever they are on a pick list (before `picks`).
    prefer: Vec<i32>,
    /// Harness answer queue for the original.
    log: Vec<String>,
    msgs: Vec<String>,
    news: Vec<String>,
    /// (link id, list index) of every pick made.
    picked: Vec<(i32, usize)>,
}

impl Ui for Drv {
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32 {
        self.msgs.push(text.to_display());
        if style == 0 && title.is_empty() {
            return 1; // ShowQuickMessage (qm.exe in the original)
        }
        let yes_no = style & 0xf == 3 || style & 0xf == 4;
        let a = self.boxes.pop_front().unwrap_or(if yes_no { 6 } else { 1 });
        self.log.push(match a {
            1 => "IDOK".into(),
            2 => "IDCANCEL".into(),
            6 => "IDYES".into(),
            7 => "IDNO".into(),
            n => n.to_string(),
        });
        a
    }
    fn inputbox(&mut self, prompt: &PbStr, _title: &PbStr, _default: &PbStr) -> PbStr {
    let __content = wsr_core::content::current();
        let a = self.inputs.pop_front().unwrap_or_default();
        if prompt.to_display().ends_with(__content.string(6717)) {
            // InBox(3): the symbol-entry dialog (edit 310, OK 1, Cancel 2)
            if a.is_empty() {
                self.log.push("click:2".into());
            } else {
                self.log.push(format!("text:310:{a}"));
                self.log.push("click:1".into());
            }
        } else {
            self.log.push(a.clone());
        }
        PbStr::from(a.as_str())
    }
    fn report(&mut self, _title: &PbStr, lines: &[PbStr], _yes_no: bool) -> ReportAction {
    let __content = wsr_core::content::current();
        let links: Vec<(usize, i32)> = lines
            .iter()
            .enumerate()
            .filter_map(|(i, l)| {
                let t = l.to_display();
                let p = t.rfind("@C")?;
                let id: i32 = t[p + 2..].trim().get(..4)?.parse().ok()?;
                Some((i, id))
            })
            .collect();
        if links.is_empty() {
            return ReportAction::Ok;
        }
        let preferred = links.iter().position(|(_, id)| self.prefer.contains(id));
        let choice = match preferred {
            Some(p) => {
                self.picks.pop_front();
                let id = links[p].1;
                self.prefer.retain(|&x| x != id);
                Some(p)
            }
            None => self.picks.pop_front().flatten(),
        };
        if let Some(k) = choice {
            let (idx, id) = links[k % links.len()];
            self.log.push(format!("dblclick:301:{idx}"));
            self.picked.push((id, idx));
            return ReportAction::Link(PbStr::from(format!("{__wsr_text_13538}{id:04}",__wsr_text_13538=__content.string(13538)).as_str()));
        }
        ReportAction::Ok
    }
    fn news(&mut self, headline: &PbStr) {
        self.news.push(headline.to_display());
    }
}


/// The seeds to run (`WSR_SEED=n` runs only seed n, for debugging).
fn seeds(r: std::ops::RangeInclusive<u64>) -> Vec<u64> {
    match std::env::var("WSR_SEED").ok().and_then(|s| s.parse().ok()) {
        Some(n) => vec![n],
        None => r.collect(),
    }
}

fn rng0(seed: u64) -> u32 {
    (cast::<u32, _>(seed)).wrapping_mul(2_654_435_761) ^ 0x1d2c_3b4a
}

/// Runs the original with the driver's answer log and compares state, RND and message-box count.
#[track_caller]
fn compare(addr: u32, args: &[&str], g: &Globals, port: &Globals, rng: &Rng, ui: &Drv, what: &str) -> Option<Original> {
    let __content = wsr_core::content::current();
    let answers: Vec<&str> = ui.log.iter().map(|s| s.as_str()).collect();
    let o = run(addr, args, g, rng0_of(what), &answers, SKIPS)?;
    if std::env::var("WSR_DBG").is_ok() {
        eprintln!("{what}{__wsr_text_13677}{:?} msgs {:?}{__wsr_text_13678}{:?}\n{}", ui.log, ui.msgs, ui.picked, o.log.iter().filter(|l| !l.starts_with("API")).cloned().collect::<Vec<_>>().join("\n"),__wsr_text_13677=__content.string(13677), __wsr_text_13678=__content.string(13678));
    }
    cmp().ignore("TextLines").ignore("TextBoxTitle").ignore("DlgSymbolEntry").abs_tol(Ext::from_f64(1e-9)).assert(port, &o.globals);
    assert_eq!(rng.state(), o.rng, "RND state after {what}");
    assert_eq!(ui.msgs.len(), orig_msgs(&o), "message boxes of {what}: {:?}\n{:?}", ui.msgs, o.log);
    for (id, idx) in &ui.picked {
        let hit = o.dialogs.iter().any(|d| {
            d.control(301).is_some_and(|c| c.items.get(*idx).is_some_and(|t| t.to_display().contains(&format!("{__wsr_text_13538}{id:04}",__wsr_text_13538=__content.string(13538)))))
        });
        assert!(hit, "pick of {id} at list index {idx} not found in the original's list boxes ({what})");
    }
    Some(o)
}

/// The RND seed is carried in the `what` text as "seed N" (keeps the call sites short).
fn rng0_of(what: &str) -> u32 {
    let n: u64 = what.rsplit("seed ").next().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
    rng0(n)
}

/// Messages shown by the port over all seeds of a test (branch coverage checks).
#[derive(Default)]
struct Seen(std::sync::Mutex<Vec<String>>);

impl Seen {
    fn add(&self, ui: &Drv) {
        self.0.lock().unwrap().extend(ui.msgs.iter().cloned());
    }
    #[track_caller]
    fn require(&self, phrases: &[&str]) {
        let v = self.0.lock().unwrap();
        for p in phrases {
            assert!(v.iter().any(|m| m.contains(p)), "no seed showed {p:?}");
        }
    }
}

/// A world with the borrower/lender relations the lending actions look at.
fn world_l(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let mut g = world(seed);
    let mut r = Rand(seed.wrapping_mul(0x51_7cc1_b727_220a) | 1);
    g.cur_player = 1;
    g.borrow_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(3.0), Ext::from_f64(14.0)));
    g.entity_borrow_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(3.0), Ext::from_f64(14.0)));
    g.currency_suffix = PbStr::new();
    g.app_title = st(__content.string(13661));
    g.turn_action_count = cast::<i32, _>(r.range(0, 3));
    for e in 1..=1600 {
        g.credit_frozen[e] = if r.chance(Ext::from_f64(0.2)) { 1 } else { 0 };
        g.auto_pilot[e] = if r.chance(Ext::from_f64(0.3)) { 1 } else { 0 };
    }
    for e in 11..=85 {
        g.bank_largest_debtor[e] = if r.chance(Ext::from_f64(0.5)) { cast::<i32, _>(r.range(86, 1600)) } else { 0 };
    }
    for p in 1..=5 {
        g.player_growth_throttle[p] = cast::<i16, _>(r.range(5, 70));
    }
    g
}

/// Makes `e` a heavily indebted borrower (low credit rating after CalcEntityFinancials).
fn weaken(g: &mut Globals, r: &mut Rand, e: i32) {
    if e > 130 {
        g.business_assets[e] = f64r(r, Ext::from_f64(200.0), Ext::from_f64(2000.0));
        g.bank_debt[e] = g.business_assets[e] * f64r(r, Ext::from_f64(0.05), Ext::from_f64(1.2));
    } else {
        g.bank_debt[e] = f64r(r, Ext::from_f64(500.0), Ext::from_f64(3000.0));
    }
    g.cash[e] = if r.chance(Ext::from_f64(0.2)) { f64r(r, -Ext::from_f64(100.0), -Ext::from_f64(1.0)) } else { f64r(r, Ext::from_f64(1.0), Ext::from_f64(50.0)) };
    g.t_bills[e] = Ext::from_f64(0.0);
    g.market_cap[e] = f64r(r, Ext::from_f64(1.0), Ext::from_f64(50.0));
    if e > 10 {
        for slot in 1..=15 {
            g.hold_stock_id[(slot, e)] = 0;
        }
        g.hold_stock_id[(0, e)] = 0;
    }
    g.earnings_ytd[e] = f64r(r, -Ext::from_f64(50.0), Ext::from_f64(50.0));
    g.earnings_y1[e] = f64r(r, -Ext::from_f64(50.0), Ext::from_f64(50.0));
}

/// `holder` owns `pct`% of `corp` in its holding slot `slot` (all other holders of `corp` removed).
fn set_holding(g: &mut Globals, holder: i32, slot: i32, corp: i32, pct: i16) {
    for h in 1..=1600 {
        for sl in 1..=15 {
            if cast::<i32, _>(g.hold_stock_id[(sl, h)]) == corp {
                g.hold_stock_id[(sl, h)] = 0;
                g.hold_stock_pct[(sl, h)] = 0;
            }
        }
    }
    g.hold_stock_id[(slot, holder)] = cast::<i16, _>(corp);
    g.hold_stock_pct[(slot, holder)] = pct;
    g.hold_stock_id[(0, holder)] = cast::<i16, _>((1..=15).filter(|&k| g.hold_stock_id[(k, holder)] != 0).count());
}

/// Player 1 owns 60% of bank `b` (so the market update in the report dialog keeps him in control).
fn own_bank(g: &mut Globals, b: i32) {
    for h in 11..=1600 {
        for slot in 1..=15 {
            if cast::<i32, _>(g.hold_stock_id[(slot, h)]) == b {
                g.hold_stock_id[(slot, h)] = 0;
                g.hold_stock_pct[(slot, h)] = 0;
            }
        }
    }
    g.hold_stock_id[(1, 1)] = cast::<i16, _>(b);
    g.hold_stock_pct[(1, 1)] = 100;
    g.hold_stock_id[(0, 1)] = g.hold_stock_id[(0, 1)].max(1);
    g.corp_controller[b] = 1;
    g.sel_corp_name = g.ent_name[b].clone();
    g.cur_player_name = g.player_name[1].clone();
}

#[test]
fn diff_toggle_auto_pilot() {
    par_seeds(seeds(1..=16), |seed| {
        let mut g = world_l(seed);
        let mut r = Rand(seed ^ 0x41);
        let e = cast::<i32, _>(r.range(11, 1600));
        g.entity_status[e] = 1;
        g.active_entity = e;
        g.active_entity_name = g.ent_name[e].clone();
        g.auto_pilot[e] = cast::<i32, _>(seed % 2);
        if seed % 4 < 2 {
            g.player_ceo_of[1] = e;
        }
        let mut port = g.clone();
        let rng = Rng::from_state(rng0(seed));
        let mut ui = Drv::default();
        toggle_auto_pilot(&mut port, &mut ui);
        compare(TOGGLE_AUTO_PILOT, &[], &g, &port, &rng, &ui, &format!("ToggleAutoPilot({e}) seed {seed}"));
    });
}

#[test]
fn diff_freeze_all_lending() {
    let __content = wsr_core::content::current();
    let changed = std::sync::atomic::AtomicUsize::new(0);
    let seen = Seen::default();
    par_seeds(seeds(1..=20), |seed| {
        let mut g = world_l(seed);
        let mut r = Rand(seed ^ 0x42);
        let b = cast::<i32, _>(r.range(11, 85));
        g.entity_status[b] = 1;
        g.active_entity = b;
        g.active_entity_name = g.ent_name[b].clone();
        g.sel_entity = b;
        own_bank(&mut g, b);
        g.credit_frozen[b] = cast::<i32, _>(seed % 2);
        let share = [Ext::from_f64(0.0), Ext::from_f64(0.002), Ext::from_f64(0.3)][cast::<usize, _>(seed % 3)];
        for e in 1..=1600 {
            if r.chance(share) {
                g.lending_bank[e] = cast::<i16, _>(b);
            }
        }
        if seed % 4 == 0 {
            g.lending_bank[1] = cast::<i16, _>(b);
        }
        if seed % 6 == 3 {
            for e in 1..=1600 {
                if cast::<i32, _>(g.lending_bank[e]) == b {
                    g.lending_bank[e] = if b == 11 { 12 } else { 11 };
                }
            }
        }
        let mut port = g.clone();
        let rng = Rng::from_state(rng0(seed));
        let mut ui = Drv::default();
        if seed % 7 == 3 {
            ui.boxes.push_back(7);
        }
        freeze_all_lending(&mut port, &mut ui);
        seen.add(&ui);
        if port.credit_frozen.as_slice() != g.credit_frozen.as_slice() {
            changed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        let Some(o) =
            compare(FREEZE_ALL_LENDING, &[], &g, &port, &rng, &ui, &format!("FreezeAllLending({b}) seed {seed}"))
        else {
            return;
        };
        let _ = o;
    });
    assert!(changed.into_inner() > 10);
    seen.require(&[
        __content.string(13679),
        "UNFREEZES LENDING TO",
        ") FREEZES LENDING TO",
        __content.string(13680),
    ]);
}

#[test]
fn diff_freeze_credit_line() {
    let __content = wsr_core::content::current();
    let seen = Seen::default();
    par_seeds(seeds(1..=24), |seed| {
        let mut g = world_l(seed);
        let mut r = Rand(seed ^ 0x43);
        let b = cast::<i32, _>(r.range(11, 85));
        g.entity_status[b] = 1;
        g.active_entity = b;
        g.active_entity_name = g.ent_name[b].clone();
        g.sel_entity = b;
        own_bank(&mut g, b);
        for e in 86..=1600 {
            if r.chance(Ext::from_f64(0.01)) {
                g.lending_bank[e] = cast::<i16, _>(b);
                g.bank_debt[e] = f64r(&mut r, Ext::from_f64(10.0), Ext::from_f64(500.0));
            }
        }
        g.lending_bank[1] = cast::<i16, _>(b);
        g.bank_debt[1] = Ext::from_f64(50.0);
        g.solo_human_mode = cast::<i32, _>(seed % 3 == 0);
        if seed % 8 == 2 {
            g.credit_frozen[1] = 1;
        }
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0(seed));
        let mut ui = Drv::default();
        for k in 0..4 {
            ui.picks.push_back(if seed % 5 == 4 && k == 0 { None } else { Some((cast::<usize, _>(seed) * 7 + k * 3) % 50) });
        }
        ui.picks.push_back(None);
        if seed % 4 == 1 {
            ui.boxes.extend([6, 6, 7]);
        }
        if seed % 4 == 2 {
            ui.prefer.push(1);
        }
        freeze_credit_line(&mut port, &mut ui, &mut rng);
        seen.add(&ui);
        compare(FREEZE_CREDIT_LINE, &[], &g, &port, &rng, &ui, &format!("FreezeCreditLine({b}) seed {seed}"));
    });
    seen.require(&[
        __content.string(3445),
        __content.string(13681),
        "IS NOW UNFROZEN",
        "IS NOW FROZEN",
        __content.string(3450),
        __content.string(13682),
    ]);
}
#[test]
fn diff_call_in_loan() {
    let __content = wsr_core::content::current();
    let seen = Seen::default();
    par_seeds(seeds(1..=48), |seed| {
        let mut g = world_l(seed);
        let mut r = Rand(seed ^ 0x44);
        let b = cast::<i32, _>(r.range(11, 85));
        g.entity_status[b] = 1;
        g.active_entity = b;
        g.active_entity_name = g.ent_name[b].clone();
        g.sel_entity = b;
        own_bank(&mut g, b);
        g.corp_controller[b] = 1;
        for e in (1..=g.num_players).chain(86..=1600) {
            if r.chance(Ext::from_f64(0.015)) || (e <= 5 && r.chance(Ext::from_f64(0.5))) {
                g.lending_bank[e] = cast::<i16, _>(b);
                g.bank_debt[e] = if r.chance(Ext::from_f64(0.1)) { Ext::from_f64(0.0) } else { f64r(&mut r, Ext::from_f64(10.0), Ext::from_f64(800.0)) };
                if r.chance(Ext::from_f64(0.6)) {
                    weaken(&mut g, &mut r, e);
                }
                if r.chance(Ext::from_f64(0.3)) {
                    g.corp_controller[e] = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
                }
            }
        }
        for e in 86..=1600 {
            if cast::<i32, _>(g.lending_bank[e]) == b && r.chance(Ext::from_f64(0.7)) {
                weaken(&mut g, &mut r, e);
            }
        }
        // the player's second bank lending to [runtime resource wording] player 2 (some also borrow from b)
        if g.num_players > 1 {
            let b2 = if b == 85 { 84 } else { b + 1 };
            g.entity_status[b2] = 1;
            set_holding(&mut g, 1, 2, b2, 60);
            g.corp_controller[b2] = 1;
            for k in 0..8 {
                let e = cast::<i32, _>(r.range(131, 1600));
                g.entity_status[e] = 1;
                set_holding(&mut g, 2, k + 1, e, 60);
                g.corp_controller[e] = 2;
                g.lending_bank[e] = cast::<i16, _>(if k < 3 { b } else { b2 });
                weaken(&mut g, &mut r, e);
                g.cash[e] = f64r(&mut r, Ext::from_f64(5.0), Ext::from_f64(50.0));
            }
        }
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0(seed));
        let mut ui = Drv::default();
        ui.picks.push_back(Some((cast::<usize, _>(seed) * 5) % 40));
        let pct = ["50%", "10%", "75%", "0", "25"][cast::<usize, _>(seed % 5)];
        ui.inputs.extend([pct.to_string(), "20%".to_string()]);
        if seed % 3 == 0 {
            ui.boxes.push_back(7);
        }
        call_in_loan(&mut port, &mut ui, &mut rng);
        seen.add(&ui);
        let what = format!("CallInLoan({b}) seed {seed}");
        let Some(o) = compare(CALL_IN_LOAN, &[], &g, &port, &rng, &ui, &what) else { return };
        let _ = o;
    });
    seen.require(&[
        __content.string(13683),
        __content.string(13684),
        __content.string(13685),
        __content.string(13686),
        __content.string(13687),
        __content.string(13688),
        "Do you wish to also call in",
        __content.string(3443),
        __content.string(13689),
    ]);
}

#[test]
fn diff_demand_advance_repayment() {
    let __content = wsr_core::content::current();
    let seen = Seen::default();
    par_seeds(seeds(1..=60), |seed| {
        let mut g = world_l(seed);
        let mut r = Rand(seed ^ 0x45);
        g.active_entity = 1;
        g.active_entity_name = g.player_name[1].clone();
        let n = if seed % 3 == 0 { 3 } else { 1 };
        let mut first = 0;
        for k in 0..n {
            let e = cast::<i32, _>(match (seed + k) % 4 {
                0 => r.range(11, 85),
                1 => r.range(86, 130),
                _ => r.range(131, 1600),
            });
            g.entity_status[e] = 1;
            g.player_advances[(1, e)] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(400.0));
            if k == 0 {
                first = e;
            }
            if seed % 2 == 0 {
                weaken(&mut g, &mut r, e);
                if e <= 130 && seed % 4 == 0 {
                    g.bonds_issued[e] = 0;
                }
                if seed % 4 == 2 {
                    g.bonds_issued[e] = 500;
                    if e > 85 {
                        g.bank_debt[e] = Ext::from_f64(0.0);
                    }
                }
            }
            if seed % 10 == 8 {
                weaken(&mut g, &mut r, e);
                g.bank_debt[e] = g.business_assets[e].max(Ext::from_f64(500.0)) * Ext::from_f64(3.0);
                g.corp_controller[e] = 1;
                g.hold_stock_id[(1, 1)] = cast::<i16, _>(e);
                g.hold_stock_pct[(1, 1)] = 100;
            }
        }
        let mut repay = ["10", "1000", "5.5", "0", "50%"][cast::<usize, _>(seed % 5)];
        if seed % 8 == 6 && first > 130 {
            // too large a repayment for a debt-free but cash-poor borrower
            g.bank_debt[first] = Ext::from_f64(0.0);
            g.bonds_issued[first] = 0;
            g.player_advances[(1, first)] = Ext::from_f64(1500.0);
            repay = "1000";
        }
        if seed % 10 == 8 {
            repay = "5.5";
        }
        if seed % 20 == 19 {
            for e in 0..=1600 {
                g.player_advances[(1, e)] = Ext::from_f64(0.0);
            }
        }
        if n == 1 && seed % 12 == 4 {
            // the player holds a long call on the borrower
            g.option_recs[0] = st("1");
            g.option_recs[1] = st(&format!("{first}| 1| 1| 5| 10| 3| {}| 1", g.cur_year + 1));
        }
        if n == 1 && seed % 12 == 10 {
            // a company the player controls has written a put on the borrower
            let h = cast::<i32, _>(r.range(131, 1600));
            g.entity_status[h] = 1;
            g.corp_controller[h] = 1;
            g.option_recs[0] = st("1");
            g.option_recs[1] = st(&format!("{first}| 2| {h}| -5| 10| 3| {}| -1", g.cur_year + 1));
        }
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0(seed));
        let mut ui = Drv::default();
        if n > 1 {
            ui.picks.push_back(Some(cast::<usize, _>(seed) % 3));
        }
        ui.inputs.extend([repay.to_string(), "1".to_string()]);
        if seed % 4 == 1 {
            ui.boxes.push_back(7);
        }
        // forgiveness percentage after the YES, then the repayment prompt again (cancel)
        ui.inputs.extend([["30%", "100", "0", "150", "5"][cast::<usize, _>(seed / 2 % 5)].to_string(), "40".to_string()]);
        demand_advance_repayment(&mut port, &mut ui, &mut rng);
        seen.add(&ui);
        let what = format!("DemandAdvanceRepayment seed {seed}");
        compare(DEMAND_ADVANCE_REPAYMENT, &[], &g, &port, &rng, &ui, &what);
    });
    seen.require(&[
        __content.string(13690),
        __content.string(13691),
        __content.string(13692),
        __content.string(13693),
        __content.string(13694),
        __content.string(13695),
        __content.string(13696),
        "conflict of interest -- a long call",
        "conflict of interest -- a short put",
        __content.string(13697),
        __content.string(13698),
        __content.string(13699),
        __content.string(13700),
        __content.string(13701),
    ]);
}

#[test]
fn diff_advance_funds_to_corp() {
    let __content = wsr_core::content::current();
    let seen = Seen::default();
    par_seeds(seeds(1..=24), |seed| {
        let mut g = world_l(seed);
        let mut r = Rand(seed ^ 0x46);
        g.active_entity = 1;
        g.active_entity_name = g.player_name[1].clone();
        if seed % 4 != 0 {
            for e in 11..=1600 {
                if g.corp_controller[e] == 1 {
                    g.corp_controller[e] = 0;
                }
            }
            let c = cast::<i32, _>(r.range(131, 1600));
            g.entity_status[c] = 1;
            set_holding(&mut g, 1, 1, c, 80);
            g.corp_controller[c] = 1;
            if seed % 3 == 0 {
                weaken(&mut g, &mut r, c);
                g.bank_debt[c] = g.business_assets[c].max(Ext::from_f64(500.0)) * Ext::from_f64(3.0);
            }
        }
        if seed % 8 == 4 {
            for e in 11..=1600 {
                if g.corp_controller[e] == 1 {
                    g.corp_controller[e] = 0;
                }
            }
        }
        g.cash[1] = f64r(&mut r, Ext::from_f64(10.0), Ext::from_f64(500.0));
        if seed % 8 == 5 {
            g.cash[1] = -Ext::from_f64(500.0);
            g.t_bills[1] = Ext::from_f64(0.0);
            g.bank_debt[1] = Ext::from_f64(1.0e6);
        }
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0(seed));
        let mut ui = Drv::default();
        if seed % 5 == 2 {
            ui.boxes.push_back(7);
        }
        let amt = ["5", "2,000", "25.5", "0"][cast::<usize, _>(seed % 4)];
        // several controlled companies: the symbol dialog asks which one
        let ctrl: Vec<i32> = (11..=1600).filter(|&e| g.entity_status[e] == 1 && g.corp_controller[e] == 1).collect();
        if ctrl.len() > 1 {
            let pick = ctrl[(cast::<usize, _>(seed)) % ctrl.len()];
            ui.inputs.push_back(if seed % 7 == 3 { String::new() } else { g.ent_symbol[pick].to_display() });
        }
        ui.inputs.extend([amt.to_string(), "1".to_string(), "".to_string()]);
        advance_funds_to_corp(&mut port, &mut ui, &mut rng);
        seen.add(&ui);
        let what = format!("AdvanceFundsToCorp seed {seed}");
        compare(ADVANCE_FUNDS_TO_CORP, &[], &g, &port, &rng, &ui, &what);
    });
    seen.require(&[
        __content.string(13702),
        __content.string(13703),
        __content.string(13704),
        __content.string(13705),
        __content.string(13706),
    ]);
}

#[test]
fn diff_change_corp_policy() {
    let __content = wsr_core::content::current();
    let seen = Seen::default();
    let lists = std::sync::atomic::AtomicUsize::new(0);
    par_seeds(seeds(1..=96), |seed| {
        let mut g = world_l(seed);
        let mut r = Rand(seed ^ 0x47);
        let e = cast::<i32, _>(match seed % 4 {
            0 => r.range(11, 85),
            1 => r.range(86, 130),
            _ => r.range(131, 1600),
        });
        g.entity_status[e] = 1;
        g.active_entity = e;
        g.active_entity_name = g.ent_name[e].clone();
        g.corp_controller[e] = 1;
        if seed % 6 == 5 {
            g.industry_num[e] = 70;
        }
        let code = ["GR", "RD", "DV"][cast::<usize, _>(seed / 4 % 3)];
        let mut inputs: Vec<String> = match code {
            "GR" => vec!["80".into(), ["15%", "-3", "45", "0"][cast::<usize, _>(seed % 4)].into(), "4".into()],
            "RD" => vec![["12%", "40", "0", "2"][cast::<usize, _>(seed % 4)].into(), "7".into()],
            _ => vec![["0.5", "9999", "0", "1.25"][cast::<usize, _>(seed % 4)].into(), "0.1".into()],
        };
        if seed % 11 == 0 {
            inputs.insert(0, String::new());
        }
        if code == "DV" && seed % 4 == 0 {
            g.dividend_per_share[e] = Ext::from_f64(0.0);
        }
        if code == "DV" && seed % 4 == 3 {
            g.dividend_per_share[e] = Ext::from_f64(0.5);
            g.market_cap[e] = g.shares_outstanding[e] * Ext::from_f64(100.0);
        }
        if code == "GR" && seed % 3 == 0 {
            g.auto_pilot[e] = 1;
            g.player_growth_throttle[1] = 5;
        }
        if seed % 5 == 0 {
            g.news_msg = st("STALE HEADLINE");
        }
        if code == "DV" && seed % 3 == 0 {
            // op [runtime resource wording] company (held by an outsider): the change is recorded
            g.option_recs[0] = st("1");
            g.option_recs[1] = st(&format!("{e}| 1| 1500| 5| 10| 3| {}| 1", g.cur_year + 1));
            if seed % 2 == 1 {
                g.recent_dividend_change_list = st("0131|0132|0133|0134|0135|0136|0137|0138|0139|0140");
            }
        }
        if seed % 9 == 2 {
            // cash-strapped: the projection shows a deficit
            g.cash[e] = -Ext::from_f64(5000.0);
            g.t_bills[e] = Ext::from_f64(0.0);
            g.credit_frozen[e] = 1;
            g.bank_debt[e] = Ext::from_f64(20000.0);
        }
        if code == "DV" && (seed % 7 == 1 || seed % 12 == 8) {
            weaken(&mut g, &mut r, e);
            g.bank_debt[e] = g.business_assets[e].max(Ext::from_f64(500.0)) * Ext::from_f64(20.0);
            g.cash[e] = -Ext::from_f64(2000.0);
            g.earnings_ytd[e] = -Ext::from_f64(500.0);
            g.earnings_y1[e] = -Ext::from_f64(500.0);
        }
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0(seed));
        let mut ui = Drv::default();
        ui.inputs.extend(inputs);
        if seed % 7 == 0 {
            ui.boxes.extend([6, 7]);
        }
        let pc = PbStr::from(code);
        change_corp_policy(&mut port, &mut ui, &mut rng, &pc);
        seen.add(&ui);
        if port.recent_dividend_change_list != g.recent_dividend_change_list {
            lists.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        let what = format!("ChangeCorpPolicy({code}{__wsr_text_13707}{e} seed {seed}",__wsr_text_13707=__content.string(13707));
        compare(CHANGE_CORP_POLICY, &[&format!("{__wsr_text_13622}{code}",__wsr_text_13622=__content.string(13622))], &g, &port, &rng, &ui, &what);
    });
    seen.require(&[
        "Growth rate of capital spending changed",
        __content.string(13708),
        "Growth rate of insurance in force changed",
        __content.string(13709),
        __content.string(13710),
        __content.string(13711),
        __content.string(13712),
        __content.string(13713),
        "raises annual dividend",
        "cuts annual dividend",
        __content.string(13714),
        __content.string(13715),
        __content.string(13716),
        __content.string(13717),
        __content.string(13718),
    ]);
    assert!(lists.into_inner() > 2, "RecentDividendChangeList never updated");
}
