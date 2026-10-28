//! Differential tests of `logic::corp_ai::spinoff`: SpinOffSubsidiary, SpinOffSubsidiaryMenu (dialog flow
//! driven by scripted MSGBOX/INPUTBOX answers), AiTaxFreeSpinoff, SpinOff_CancelMergerOnViolation and
//! CalcAssetSalePricePct on random plausible worlds. `TIME$` is fixed by the harness (10:00:00), so the
//! ports get `time_secs = 0`.
#![allow(unused_imports)]
use wsr_core::pb::real::{Ext, cast};


use super::common::{cmp, f64r, orig_msgs, world};
use super::diff::*;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use wsr_core::logic::corp_ai::spinoff::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::{ReportAction, Ui};

const SPIN_OFF: u32 = 0x525509;
const MENU: u32 = 0x5bfb37;
const CANCEL_MERGER: u32 = 0x5c17df;
const ASSET_SALE_PCT: u32 = 0x5db84b;
const AI_SPINOFF: u32 = 0x4c4e2c;

/// Callees skipped in the original (address, stdcall dword args): none left.
const SKIPS: &[(u32, u32)] = &[];

/// A Ui answering MSGBOX and INPUTBOX$ from one queue like the harness (quick messages take none).
#[derive(Default)]
struct Ans {
    answers: VecDeque<String>,
    msgs: Vec<String>,
    inputs: usize,
}

impl Ui for Ans {
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32 {
        self.msgs.push(text.to_display());
        if style == 0 && title.is_empty() {
            return 1;
        }
        if let Some(a) = self.answers.pop_front() {
            return match a.as_str() {
                "IDYES" => 6,
                "IDNO" => 7,
                "IDOK" => 1,
                "IDCANCEL" => 2,
                x => x.parse().unwrap(),
            };
        }
        if style & 0xf == 3 || style & 0xf == 4 { 6 } else { 1 }
    }
    fn inputbox(&mut self, _prompt: &PbStr, _title: &PbStr, _default: &PbStr) -> PbStr {
        self.inputs += 1;
        PbStr::from(self.answers.pop_front().unwrap_or_default().as_str())
    }
    fn report(&mut self, _title: &PbStr, _lines: &[PbStr], _yes_no: bool) -> ReportAction {
        ReportAction::Ok
    }
    fn news(&mut self, _headline: &PbStr) {}
}

/// All ticker headlines of a state, joined.
fn news(g: &Globals) -> String {
    (1..=60).map(|i| g.news_headline[i].to_display()).collect::<Vec<_>>().join("\n")
}

/// The stake `p` holds in `sub`.
fn stake(g: &Globals, p: i32, sub: i32) -> i32 {
    (1..=15).filter(|&s| cast::<i32, _>(g.hold_stock_id[(s, p)]) == sub).map(|s| cast::<i32, _>(g.hold_stock_pct[(s, p)])).sum()
}

/// `range`, or only the seed in `$SPIN_SEED` (debugging).
fn seeds(range: std::ops::RangeInclusive<u64>) -> Vec<u64> {
    match std::env::var("SPIN_SEED") {
        Ok(s) => vec![s.parse().unwrap()],
        Err(_) => range.collect(),
    }
}

fn seed_rng(seed: u64) -> u32 {
    cast::<u32, _>(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 17)
}

fn hit(n: &AtomicUsize, cond: bool) {
    if cond {
        n.fetch_add(1, Ordering::Relaxed);
    }
}

/// Prints a coverage count (and fails if nothing was reached while the harness ran).
fn report(what: &str, n: &AtomicUsize) {
    let __content = wsr_core::content::current();
    let k = n.load(Ordering::Relaxed);
    eprintln!("{__wsr_text_13574}{what}: {k}",__wsr_text_13574=__content.string(13574));
    if unavailable().is_none() {
        assert!(k > 0, "no seed reached: {what}");
    }
}

/// Runs the original at `addr` and the port on `g`; compares state, RND state and message boxes.
/// Returns the original (None if the harness is unavailable) and the port's return value.
fn check<R: Copy + std::fmt::Debug>(
    addr: u32,
    args: &[String],
    g: &Globals,
    rng0: u32,
    answers: &[&str],
    port: impl FnOnce(&mut Globals, &mut Ans, &mut Rng) -> R,
) -> Option<(Original, Globals, R)> {
    let __content = wsr_core::content::current();
    let opts = Opts {
        args: args.to_vec(),
        rng: Some(rng0),
        answers,
        skips: SKIPS.to_vec(),
        ..Opts::default()
    };
    let o = run_original_with(addr, &opts, g)?;
    let mut p = g.clone();
    let mut ui = Ans { answers: answers.iter().map(|s| s.to_string()).collect(), ..Ans::default() };
    let mut rng = Rng::from_state(rng0);
    let r = port(&mut p, &mut ui, &mut rng);
    let d = cmp().diffs(&p, &o.globals);
    assert!(d.is_empty(), "{__wsr_text_13638}{addr:x} args {args:?} rng {rng0}: {} differences (port vs original):\n{}", d.len(), d.join("\n"),__wsr_text_13638=__content.string(13638));
    assert_eq!(rng.state(), o.rng, "RND state (0x{addr:x})");
    assert_eq!(ui.msgs.len(), orig_msgs(&o), "message boxes (0x{addr:x}): {:?} / {:?}", ui.msgs, o.log);
    let n_inputs = o.log.iter().filter(|l| l.starts_with("INPUTBOX")).count();
    assert_eq!(ui.inputs, n_inputs, "INPUTBOX calls (0x{addr:x})");
    Some((o, p, r))
}

/// A world with a spin-off situation: parent `p` holds a stake in `sub`; the parent's stock is held by
/// players, companies (some with full portfolios), possibly by `sub` itself (treasury stock) and with
/// tiny stakes (cash in lieu).
fn spin_world(seed: u64) -> (Globals, i32, i32) {
    let mut g = world(seed);
    let mut r = Rand(seed ^ 0x5b1f);
    let p = cast::<i32, _>(match r.range(0, 9) {
        0 => r.range(11, 85),
        1 => r.range(86, 130),
        _ => r.range(131, 1580),
    });
    let mut sub = cast::<i32, _>(r.range(131, 1580));
    if sub == p {
        sub += 1;
    }
    if r.chance(Ext::from_f64(0.15)) {
        sub = cast::<i32, _>(r.range(86, 130));
        if sub == p {
            sub = 130;
        }
    }
    for e in [p, sub] {
        g.entity_status[e] = 1;
    }
    if r.chance(Ext::from_f64(0.1)) && sub > 130 {
        g.industry_num[sub] = 70;
    } else if g.industry_num[sub] > 69 {
        g.industry_num[sub] = 20;
    }
    // the parent's stake in the subsidiary
    let held = cast::<i16, _>(if r.chance(Ext::from_f64(0.6)) { r.range(80, 100) } else { r.range(10, 79) });
    let slot = cast::<i32, _>(r.range(1, 15));
    for s in 1..=15 {
        if cast::<i32, _>(g.hold_stock_id[(s, p)]) == sub {
            g.hold_stock_id[(s, p)] = 0;
        }
    }
    g.hold_stock_id[(slot, p)] = cast::<i16, _>(sub);
    g.hold_stock_pct[(slot, p)] = held;
    g.hold_stock_basis[(slot, p)] = if r.chance(Ext::from_f64(0.2)) { -f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(500.0)) } else { f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(3000.0)) };
    // remove other holders of sub so that the stake is plausible
    for h in 1..=1600 {
        for s in 1..=15 {
            if h != p && cast::<i32, _>(g.hold_stock_id[(s, h)]) == sub && r.chance(Ext::from_f64(0.8)) {
                g.hold_stock_id[(s, h)] = 0;
            }
        }
    }
    g.ctrl_since_year[sub] = g.cur_year - cast::<i32, _>(r.range(0, 15));
    g.ctrl_since_year[p] = if r.chance(Ext::from_f64(0.2)) { 0 } else { g.cur_year - cast::<i32, _>(r.range(0, 15)) };
    // the parent's shareholders
    let mut left = 100i64;
    for k in 0..r.range(1, 8) {
        let h = cast::<i32, _>(match r.range(0, 9) {
            0 => r.range(1, cast::<i64, _>(g.num_players)),
            1 if k == 0 => cast::<i64, _>(sub),
            _ => r.range(11, 1600),
        });
        if h == p {
            continue;
        }
        g.entity_status[h] = 1;
        let pct = if r.chance(Ext::from_f64(0.25)) { 1 } else { r.range(1, left.clamp(1, 60)) };
        if pct > left {
            break;
        }
        left -= pct;
        let mut s = (1..=15).find(|&s| cast::<i32, _>(g.hold_stock_id[(s, h)]) == p).unwrap_or(0);
        if s == 0 {
            s = cast::<i32, _>(r.range(1, 15));
        }
        g.hold_stock_id[(s, h)] = cast::<i16, _>(p);
        g.hold_stock_pct[(s, h)] = if h <= 10 && r.chance(Ext::from_f64(0.15)) { -(cast::<i16, _>(pct)) } else { cast::<i16, _>(pct) };
        g.hold_stock_basis[(s, h)] = f64r(&mut r, -Ext::from_f64(10.0), Ext::from_f64(800.0));
        if r.chance(Ext::from_f64(0.25)) {
            // a full portfolio: no slot left for the new stock
            for s2 in 1..=15 {
                if g.hold_stock_id[(s2, h)] == 0 || cast::<i32, _>(g.hold_stock_id[(s2, h)]) == sub {
                    let mut x = cast::<i32, _>(r.range(131, 1580));
                    if x == sub || x == p {
                        x = 1500;
                    }
                    g.hold_stock_id[(s2, h)] = cast::<i16, _>(x);
                    g.hold_stock_pct[(s2, h)] = cast::<i16, _>(r.range(1, 5));
                }
            }
            if r.chance(Ext::from_f64(0.5)) {
                g.corp_controller[h] = g.corp_controller[p];
            }
        }
    }
    for e in 1..=1600 {
        g.hold_stock_id[(0, e)] = cast::<i16, _>((1..=15).filter(|&s| g.hold_stock_id[(s, e)] != 0).count());
    }
    if r.chance(Ext::from_f64(0.3)) {
        g.cur_player = g.corp_controller[p].clamp(1, g.num_players);
        g.corp_controller[p] = g.cur_player;
    }
    if r.chance(Ext::from_f64(0.3)) {
        g.credit_rating_num[p] = cast::<i32, _>(r.range(0, 10));
    }
    g.ctrl_change_msg_mode = 0;
    (g, p, sub)
}

#[test]
fn diff_calc_asset_sale_price_pct() {
    static N: AtomicUsize = AtomicUsize::new(0);
    par_seeds(seeds(1..=24), |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed);
        let c = cast::<i32, _>(match seed % 6 {
            0 => r.range(11, 130),
            _ => r.range(131, 1580),
        });
        g.profit_margin[c] = cast::<i16, _>(r.range(-40, 80));
        let Some((o, _, rv)) =
            check(ASSET_SALE_PCT, &[c.to_string()], &g, seed_rng(seed), &[], |p, _ui, rng| {
                calc_asset_sale_price_pct(p, rng, c)
            })
        else {
            return;
        };
        assert_eq!(o.return_float, Some(rv), "return value seed {seed}");
        hit(&N, rv != Ext::from_f64(0.0));
    });
    report("asset sale pct nonzero", &N);
}

#[test]
fn diff_spin_off_cancel_merger_on_violation() {
    par_seeds(seeds(1..=6), |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed);
        g.active_entity = cast::<i32, _>(r.range(5, 1600));
        g.merger_target = if seed % 2 == 0 { g.active_entity } else { 0 };
        g.merger_acquirer = cast::<i32, _>(r.range(11, 1600));
        g.merger_proposer_player = 1;
        g.next_takeover_tick = 7;
        check(CANCEL_MERGER, &[], &g, 1, &[], |p, ui, _rng| spin_off_cancel_merger_on_violation(p, ui));
    });
}

#[test]
fn diff_spin_off_subsidiary() {
    let __content = wsr_core::content::current();
    static DONE: AtomicUsize = AtomicUsize::new(0);
    static BLOCKED: AtomicUsize = AtomicUsize::new(0);
    static TAXABLE: AtomicUsize = AtomicUsize::new(0);
    static TREASURY: AtomicUsize = AtomicUsize::new(0);
    static CASH: AtomicUsize = AtomicUsize::new(0);
    static DIVEST: AtomicUsize = AtomicUsize::new(0);
    par_seeds(seeds(1..=60), |seed| {
        let (g, p, sub) = spin_world(seed);
        let mut r = Rand(seed ^ 0x77);
        let mode = cast::<i32, _>(r.range(0, 2));
        let pct = cast::<i32, _>(if r.chance(Ext::from_f64(0.6)) { 100 } else { r.range(1, 99) });
        let args = [p.to_string(), sub.to_string(), mode.to_string(), pct.to_string()];
            let Some((o, _, rv)) = check(SPIN_OFF, &args, &g, seed_rng(seed), &[], |q, ui, rng| {
            spin_off_subsidiary(q, ui, rng, p, sub, mode, pct, 0)
        }) else {
            return;
        };
        assert_eq!(cast::<i32, _>(o.return_value), rv, "result seed {seed}");
        let log = o.log.join("\n");
        let nw = news(&o.globals);
        hit(&DONE, rv == 1);
        hit(&BLOCKED, rv == 0);
        hit(&TAXABLE, nw.contains(__content.string(13639)));
        hit(&TREASURY, rv == 1 && o.globals.shares_outstanding[sub] != g.shares_outstanding[sub]);
        hit(&CASH, nw.contains(__content.string(13640)) || nw.contains("in lieu"));
        hit(&DIVEST, log.contains(__content.string(13641)));
    });
    report("spin-off done", &DONE);
    report("spin-off blocked", &BLOCKED);
    report(__content.string(13642), &TAXABLE);
    report(__content.string(13643), &TREASURY);
    report("cash in lieu", &CASH);
    report("divestment required", &DIVEST);
}

#[test]
fn diff_spin_off_subsidiary_menu() {
    let __content = wsr_core::content::current();
    static DONE: AtomicUsize = AtomicUsize::new(0);
    static TAX_BOX: AtomicUsize = AtomicUsize::new(0);
    static FAVORABLE: AtomicUsize = AtomicUsize::new(0);
    static INPUT: AtomicUsize = AtomicUsize::new(0);
    static MISC: AtomicUsize = AtomicUsize::new(0);
    static SHORT: AtomicUsize = AtomicUsize::new(0);
    static MERGER: AtomicUsize = AtomicUsize::new(0);
    par_seeds(seeds(1..=60), |seed| {
        let (mut g, p, sub) = spin_world(seed);
        let mut r = Rand(seed ^ 0x99);
        let spec = match seed % 5 {
            0 => {
                // the only holding: picked without the listing
                for s in 1..=15 {
                    if cast::<i32, _>(g.hold_stock_id[(s, p)]) != sub {
                        g.hold_stock_id[(s, p)] = 0;
                    }
                }
                g.hold_stock_id[(0, p)] = 1;
                0
            }
            1 => -sub,
            2 if seed % 3 == 0 => 0, // several holdings: the portfolio listing in "GetCorp" mode
            _ => sub,
        };
        if r.chance(Ext::from_f64(0.3)) {
            g.active_entity = p;
            g.merger_target = p;
            g.merger_acquirer = cast::<i32, _>(r.range(131, 1580));
        }
        let pct_answer = match r.range(0, 3) {
            0 => "100".to_string(),
            1 => format!("{}%", r.range(1, 99)),
            2 => "0".to_string(),
            _ => "250".to_string(),
        };
        let mut answers: Vec<String> = Vec::new();
        for _ in 0..3 {
            answers.push(if r.chance(Ext::from_f64(0.85)) { "IDYES" } else { "IDNO" }.to_string());
        }
        if spec.abs() <= 10 {
            // INPUTBOX after the short/merger questions: put the % in the right place by scripting
            // enough answers: harness and port take MSGBOX and INPUTBOX answers from one queue in order.
            answers.clear();
            let n_q = r.range(0, 2);
            for _ in 0..n_q {
                answers.push("IDYES".into());
            }
            answers.push(pct_answer.clone());
            if pct_answer == "250" {
                answers.push("50".into());
            }
            answers.push(if r.chance(Ext::from_f64(0.85)) { "IDYES" } else { "IDNO" }.into());
            answers.push("IDYES".into());
        }
        let ans: Vec<&str> = answers.iter().map(|s| s.as_str()).collect();
        let args = [p.to_string(), spec.to_string()];
        let Some((o, _, _)) = check(MENU, &args, &g, seed_rng(seed), &ans, |q, ui, rng| {
            spin_off_subsidiary_menu(q, ui, rng, p, spec, 0)
        }) else {
            return;
        };
        let log = o.log.join("\n");
        hit(&DONE, stake(&o.globals, p, sub) < stake(&g, p, sub));
        hit(&TAX_BOX, log.contains(__content.string(13644)));
        hit(&FAVORABLE, log.contains(__content.string(2975)));
        hit(&INPUT, log.contains("INPUTBOX"));
        hit(&MISC, o.globals.ent_misc_data[sub] != g.ent_misc_data[sub]);
        hit(&SHORT, log.contains(__content.string(2903)));
        hit(&MERGER, log.contains(__content.string(2909)));
    });
    report("menu: spin-off done", &DONE);
    report("menu: unfavorable tax ruling", &TAX_BOX);
    report("menu: favorable tax ruling", &FAVORABLE);
    report("menu: inputbox", &INPUT);
    report("menu: misc data parent field", &MISC);
    report("menu: short position question", &SHORT);
    report("menu: pending merger question", &MERGER);
}

/// A world for AiTaxFreeSpinoff: `SimCoCount - 70` is an 80%+ subsidiary of a company.
fn ai_world(seed: u64) -> (Globals, i32, i32) {
    let (mut g, p, sub) = spin_world(seed);
    let mut r = Rand(seed ^ 0x3c);
    g.sim_co_count = sub + 70;
    for s in 1..=15 {
        if cast::<i32, _>(g.hold_stock_id[(s, p)]) == sub {
            g.hold_stock_pct[(s, p)] = if r.chance(Ext::from_f64(0.7)) { 100 } else { cast::<i16, _>(r.range(80, 99)) };
            if r.chance(Ext::from_f64(0.85)) {
                g.hold_stock_basis[(s, p)] = g.hold_stock_basis[(s, p)].abs();
            }
        }
    }
    // no other holders of sub
    for h in 1..=1600 {
        for s in 1..=15 {
            if h != p && cast::<i32, _>(g.hold_stock_id[(s, h)]) == sub {
                g.hold_stock_id[(s, h)] = 0;
                g.hold_stock_pct[(s, h)] = 0;
            }
        }
    }
    if r.chance(Ext::from_f64(0.85)) {
        g.ctrl_since_year[sub] = g.cur_year - cast::<i32, _>(r.range(5, 20));
    }
    if r.chance(Ext::from_f64(0.8)) {
        // a strong parent and a small, operating subsidiary
        g.cash[p] = f64r(&mut r, Ext::from_f64(3000.0), Ext::from_f64(20000.0));
        g.t_bills[p] = f64r(&mut r, Ext::from_f64(50_000.0), Ext::from_f64(500_000.0));
        g.bank_debt[p] = Ext::from_f64(0.0);
        g.bonds_issued[p] = 0;
        g.tax_loss_carry[p] = Ext::from_f64(0.0);
        g.tax_loss_carry[sub] = Ext::from_f64(0.0);
        g.cash[sub] = Ext::from_f64(1.0);
        g.t_bills[sub] = Ext::from_f64(0.0);
        g.market_cap[sub] = f64r(&mut r, Ext::from_f64(5.0), Ext::from_f64(300.0));
        if r.chance(Ext::from_f64(0.8)) {
            g.ent_name[sub] = PbStr::from(format!("ZZ{}", g.ent_name[sub].to_display()).as_str());
        }
    }
    g.corp_controller[p] = cast::<i32, _>(r.range(11, 1600));
    g.corp_controller[sub] = g.corp_controller[p];
    (g, p, sub)
}

#[test]
fn diff_ai_tax_free_spinoff() {
    static DONE: AtomicUsize = AtomicUsize::new(0);
    static PAST_FILTERS: AtomicUsize = AtomicUsize::new(0);
    par_seeds(seeds(1..=100), |seed| {
        let (g, p, sub) = ai_world(seed);
        let Some((o, _, _)) = check(AI_SPINOFF, &[], &g, seed_rng(seed), &[], |q, ui, rng| {
            ai_tax_free_spinoff(q, ui, rng, 0)
        }) else {
            return;
        };
        hit(&PAST_FILTERS, o.globals.action_code == "SELL");
        hit(&DONE, stake(&o.globals, p, sub) < stake(&g, p, sub));
    });
    report("ai spin-off: reached the menu", &PAST_FILTERS);
    report("ai spin-off: done", &DONE);
}

/// Debugging aid: the original's call log of SpinOffSubsidiary for `$SPIN_SEED`.
#[test]
#[ignore]
fn debug_spin_off_log() {
    let seed: u64 = std::env::var("SPIN_SEED").map_or(1, |s| s.parse().unwrap());
    let (g, p, sub) = spin_world(seed);
    let mut r = Rand(seed ^ 0x77);
    let mode = cast::<i32, _>(r.range(0, 2));
    let pct = cast::<i32, _>(if r.chance(Ext::from_f64(0.6)) { 100 } else { r.range(1, 99) });
    let hooks: Vec<u32> = std::env::var("SPIN_HOOKS")
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| u32::from_str_radix(s, 16).unwrap())
        .collect();
    let opts = Opts {
        args: vec![p.to_string(), sub.to_string(), mode.to_string(), pct.to_string()],
        rng: Some(seed_rng(seed)),
        skips: SKIPS.to_vec(),
        hooks,
        ..Opts::default()
    };
    let Some(o) = run_original_with(SPIN_OFF, &opts, &g) else { return };
    for l in &o.log {
        eprintln!("{l}");
    }
}
