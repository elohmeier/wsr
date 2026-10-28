//! Differential tests of logic::corp_finance::misc (rumors, lawsuits, CEO, company name, ETF adviser
//! contract and fee).
//!
//! A scripted Ui ([`Drv`]) answers every MSGBOX / INPUTBOX / symbol-entry dialog and logs the
//! equivalent harness answers in call order (`IDYES`/`IDNO`/`IDCANCEL`/`IDOK`, the entered text,
//! `text:310:SYM` + `click:1` or `click:2` in the symbol-entry dialog); the original is then run with
//! that log as its answer queue.
#![allow(unused_imports, clippy::too_many_arguments)]
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use std::collections::VecDeque;
use wsr_core::logic::corp_finance::misc::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::{ReportAction, Ui};

const SPREAD_RUMORS: u32 = 0x5aac8f;
const BECOME_OR_RESIGN_CEO: u32 = 0x59ca81;
const CHANGE_COMPANY_NAME: u32 = 0x4715f3;
const HARASSING_LAWSUIT: u32 = 0x59e9e5;
const FILE_ANTITRUST_SUIT: u32 = 0x588f0f;
const SET_ETF_ADVISORY_FEE: u32 = 0x5c1dae;
const SEEK_BECOME_ETF_ADVISER: u32 = 0x6501b3;

/// Scripted Ui that logs the harness answers of everything it answers.
#[derive(Default)]
struct Drv {
    /// MSGBOX answers (default YES / OK).
    boxes: VecDeque<i32>,
    /// INPUTBOX / symbol-entry answers (default "" = cancel).
    inputs: VecDeque<String>,
    /// Harness answer queue for the original.
    log: Vec<String>,
    msgs: Vec<String>,
    news: Vec<String>,
}

impl Ui for Drv {
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32 {
        self.msgs.push(text.to_display());
        if style == 0 && title.is_empty() {
            return 1; // ShowQuickMessage (qm.exe in the original)
        }
        let yes_no = style & 0xf == 3 || style & 0xf == 4;
        let mut a = self.boxes.pop_front().unwrap_or(if yes_no { 6 } else { 1 });
        if !yes_no {
            a = 1;
        }
        if style & 0xf == 4 && a == 2 {
            a = 7;
        }
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
    fn report(&mut self, _title: &PbStr, _lines: &[PbStr], _yes_no: bool) -> ReportAction {
        ReportAction::Ok
    }
    fn news(&mut self, headline: &PbStr) {
        self.news.push(headline.to_display());
    }
}

/// Counts seeds where a branch was taken; asserts at the end that it happened at least `min` times.
struct Hits(std::sync::atomic::AtomicUsize, String, usize);

impl Hits {
    fn new(what: &str, min: usize) -> Self {
        Hits(std::sync::atomic::AtomicUsize::new(0), what.to_string(), min)
    }
    fn count(&self, hit: bool) {
        if hit {
            self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
    fn check(&self) {
    let __content = wsr_core::content::current();
        if unavailable().is_none() {
            let n = self.0.load(std::sync::atomic::Ordering::Relaxed);
            assert!(n >= self.2, "{}{__wsr_text_13645}{n}{__wsr_text_13646}", self.1,__wsr_text_13645=__content.string(13645), __wsr_text_13646=__content.string(13646));
        }
    }
}

fn rng0(seed: u64) -> u32 {
    (cast::<u32, _>(seed)).wrapping_mul(2_654_435_761) ^ 0x3c6e_f372
}

/// Runs the original with the driver's answer log and compares state, RND and message-box count.
#[track_caller]
fn compare(addr: u32, args: &[&str], g: &Globals, port: &Globals, rng: &Rng, ui: &Drv, seed: u64, what: &str) -> Option<Original> {
    let __content = wsr_core::content::current();
    if std::env::var("MISC_SEED").is_ok_and(|v| v != seed.to_string()) {
        return None;
    }
    let answers: Vec<&str> = ui.log.iter().map(|s| s.as_str()).collect();
    let o = run(addr, args, g, rng0(seed), &answers, &[])?;
    if std::env::var("MISC_DEBUG").is_ok() {
        eprintln!("{what} seed {seed}{__wsr_text_13677}{answers:?}\nport msgs {:?}\norig log {:?}", ui.msgs, o.log,__wsr_text_13677=__content.string(13677));
    }
    // ActionMenuOpen: the port of ToggleStreamingQuote (logic::market) leaves out its CloseAllDialogs(1)
    // call, which decrements this counter in the original (reported; not this module's routine).
    cmp().ignore("ActionMenuOpen").ignore("DlgSymbolEntry").ignore("TextLines").ignore("TextBoxTitle").abs_tol(Ext::from_f64(1e-9)).assert(port, &o.globals);
    assert_eq!(rng.state(), o.rng, "RND state after {what} (seed {seed})");
    assert_eq!(ui.msgs.len(), orig_msgs(&o), "message boxes of {what} (seed {seed}): {:?}\n{:?}", ui.msgs, o.log);
    assert!(!o.log.iter().any(|l| l.contains("FORCED END")), "dialog not answered in {what} (seed {seed}): {:?}", o.log);
    Some(o)
}

/// A world with unique stock symbols, the current human player 1 and a few game-specific settings.
fn world_m(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let mut g = world(seed);
    let mut r = Rand(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) | 1);
    for e in 1..=1600 {
        // unique symbols (letters only): base-26 code of the id, 2..3 letters
        let mut n = e + 26;
        let mut sym = Vec::new();
        while n > 0 {
            sym.push(b'A' + cast::<u8, _>(n % 26));
            n /= 26;
        }
        g.ent_symbol[e] = PbStr(sym);
    }
    g.cur_player = 1;
    g.is_computer_player[1] = 0;
    g.sel_entity = 0;
    g.last_sel_entity = 0;
    g.currency_suffix = PbStr::new();
    g.app_title = st(__content.string(13661));
    g.cur_player_name = g.player_name[1].clone();
    g.turn_action_count = cast::<i32, _>(r.range(0, 3));
    g.difficulty_level = cast::<i32, _>(r.range(1, 3));
    for p in 1..=5 {
        g.player_notoriety[p] = cast::<i16, _>(r.range(0, 12));
        g.player_law_firm[p] = cast::<i32, _>(r.range(1, 3));
    }
    for e in 1581..=1600 {
        g.entity_status[e] = if e <= 1590 { 1 } else { 0 };
        g.industry_num[e] = 71;
        g.fund_fee_pct[e] = cast::<f32, _>([0.2f32, 0.3f32, 0.5f32, 0.8f32, 1.0f32, 0.1f32][cast::<usize, _>(r.range(0, 5))]);
        g.fund_adviser[e] = if r.chance(Ext::from_f64(0.5)) { cast::<i32, _>(r.range(86, 130)) } else { 0 };
        g.ctrl_since_year[e] = g.cur_year - cast::<i32, _>(r.range(0, 6));
        g.fund_bonus_year[e] = if r.chance(Ext::from_f64(0.5)) { g.cur_year } else { 0 };
    }
    g
}

fn act_as(g: &mut Globals, e: i32) {
    g.entity_status[e] = 1;
    if e > 10 {
        g.corp_controller[e] = 1;
    }
    g.active_entity = e;
    g.active_entity_name = if e > 10 { g.ent_name[e].clone() } else { g.player_name[e].clone() };
}

fn sym(g: &Globals, e: i32) -> String {
    g.ent_symbol[e].to_display()
}

// ------------------------------------------------------------------------------------ SpreadRumors

#[test]
fn diff_spread_rumors() {
    let __content = wsr_core::content::current();
    let spread = Hits::new("rumor spread", 8);
    let libel = Hits::new("libel suit", 3);
    let refused = Hits::new(__content.string(13720), 2);
    par_seeds(1..=48, |seed| {
        let mut g = world_m(seed);
        let mut r = Rand(seed ^ 0x51);
        act_as(&mut g, 1);
        if seed % 16 == 0 {
            g.game_end_year = g.cur_year + 1;
        }
        let target = match seed % 8 {
            0 => 1582,
            1 => {
                let t = cast::<i32, _>(r.range(131, 1580));
                g.corp_controller[t] = 1;
                t
            }
            _ => cast::<i32, _>(r.range(131, 1580)),
        };
        g.entity_status[target] = 1;
        g.player_notoriety[1] = cast::<i16, _>(r.range(0, 12));
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0(seed));
        let mut ui = Drv::default();
        ui.inputs = [if seed % 11 == 0 { String::new() } else { sym(&g, target) }].into();
        ui.boxes = [if seed % 3 == 0 { 7 } else { 6 }].into();
        spread_rumors(&mut port, &mut ui, &mut rng);
        if compare(SPREAD_RUMORS, &[], &g, &port, &rng, &ui, seed, "SpreadRumors").is_some() {
            spread.count(port.market_cap[target] != g.market_cap[target]);
            libel.count(ui.news.iter().any(|n| n.contains("libel")));
            refused.count(ui.msgs.iter().any(|m| m.contains(__content.string(13720))));
        }
    });
    spread.check();
    libel.check();
    refused.check();
}

// ------------------------------------------------------------------------------- BecomeOrResignCeo

#[test]
fn diff_become_or_resign_ceo() {
    let elected = Hits::new("elected", 6);
    let rejected = Hits::new("message only", 4);
    par_seeds(1..=40, |seed| {
        let mut g = world_m(seed);
        let mut r = Rand(seed ^ 0x52);
        // a few holdings of the player, some of them controlling
        for slot in 1..=15 {
            g.hold_stock_id[(slot, 1)] = 0;
            g.hold_stock_pct[(slot, 1)] = 0;
        }
        let n = r.range(0, 3);
        for slot in 1..=n {
            let c = cast::<i32, _>(r.range(131, 1580));
            g.entity_status[c] = 1;
            g.hold_stock_id[(cast::<i32, _>(slot), 1)] = cast::<i16, _>(c);
            g.hold_stock_pct[(cast::<i32, _>(slot), 1)] = if r.chance(Ext::from_f64(0.6)) { 60 } else { 10 };
            g.corp_controller[c] = if g.hold_stock_pct[(cast::<i32, _>(slot), 1)] > 50 { 1 } else { 0 };
        }
        g.hold_stock_id[(0, 1)] = cast::<i16, _>(n);
        for e in 11..=1600 {
            if g.corp_controller[e] == 1 && !(1..=15).any(|s| cast::<i32, _>(g.hold_stock_id[(s, 1)]) == e) {
                g.corp_controller[e] = 0;
            }
        }
        g.player_ceo_of[1] = if r.chance(Ext::from_f64(0.4)) { cast::<i32, _>(g.hold_stock_id[(1, 1)]) } else { 0 };
        // (a tenure without a post makes ElectSelfCeo read EntName(0): error 9 in the original)
        g.player_ceo_since_year[1] = if g.player_ceo_of[1] > 10 { g.cur_year - cast::<i32, _>(r.range(0, 5)) } else { 0 };
        let ae = if seed % 4 == 0 { cast::<i32, _>(r.range(131, 1580)) } else { 1 };
        if ae > 10 {
            act_as(&mut g, ae);
        } else {
            g.active_entity = 1;
            g.active_entity_name = g.player_name[1].clone();
        }
        g.sel_entity = match seed % 3 {
            0 => 1,
            1 => cast::<i32, _>(r.range(131, 1580)),
            _ => cast::<i32, _>(g.hold_stock_id[(1, 1)].max(0)),
        };
        if g.sel_entity > 10 {
            g.sel_corp_name = g.ent_name[g.sel_entity].clone();
        }
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0(seed));
        let mut ui = Drv::default();
        become_or_resign_ceo(&mut port, &mut ui, &mut rng);
        if compare(BECOME_OR_RESIGN_CEO, &[], &g, &port, &rng, &ui, seed, "BecomeOrResignCeo").is_some() {
            elected.count(port.player_ceo_of[1] != g.player_ceo_of[1]);
            rejected.count(port.player_ceo_of[1] == g.player_ceo_of[1] && !ui.msgs.is_empty());
        }
    });
    elected.check();
    rejected.check();
}

// ------------------------------------------------------------------------------- ChangeCompanyName

#[test]
fn diff_change_company_name() {
    let renamed = Hits::new("renamed", 8);
    let new_symbol = Hits::new("new symbol", 5);
    par_seeds(1..=40, |seed| {
        let mut g = world_m(seed);
        let mut r = Rand(seed ^ 0x53);
        let c = cast::<i32, _>(r.range(131, 1580));
        act_as(&mut g, c);
        g.sel_entity = if seed % 3 == 0 { cast::<i32, _>(r.range(131, 1580)) } else { c };
        if seed % 13 == 0 {
            act_as(&mut g, 1);
        }
        if seed % 5 == 0 {
            g.watch_list[(3, 1)] = c;
        }
        g.auto_add_stream_list = cast::<i32, _>(seed % 2);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0(seed));
        let mut ui = Drv::default();
        ui.boxes = match seed % 4 {
            0 => [6, 6].into(),
            1 => [7, 6].into(),
            2 => [2].into(),
            _ => [6, 7].into(),
        };
        let other = g.ent_name[cast::<i32, _>(r.range(131, 1580))].to_display();
        let name = match seed % 6 {
            0 => "ab".to_string(),
            1 => "xyz".to_string(),
            2 => other.to_string(),
            _ => format!("New Name {seed} Holdings & Co!"),
        };
        let mut inputs: Vec<String> = vec![name];
        if seed % 6 == 2 {
            inputs.push(format!("Renamed {seed}"));
        }
        // symbol answers: invalid / reserved / taken / fine
        match seed % 5 {
            0 => inputs.extend(["A1".into(), "QZQZ".into()]),
            1 => inputs.extend(["IPO".into(), "QZQY".into()]),
            2 => inputs.extend([sym(&g, 200), "QQQX".into()]),
            3 => inputs.extend(["toolong".into(), String::new()]),
            _ => inputs.push(sym(&g, c)),
        }
        ui.inputs = inputs.into();
        let ret = change_company_name(&mut port, &mut ui, &mut rng, 0);
        if let Some(o) = compare(CHANGE_COMPANY_NAME, &[], &g, &port, &rng, &ui, seed, "ChangeCompanyName") {
            assert_eq!(cast::<u32, _>(ret), o.return_value, "result (seed {seed})");
            renamed.count(port.ent_name[c] != g.ent_name[c]);
            new_symbol.count(port.ent_symbol[c] != g.ent_symbol[c]);
        }
    });
    renamed.check();
    new_symbol.check();
}

// -------------------------------------------------------------------------------- HarassingLawsuit

#[test]
fn diff_harassing_lawsuit() {
    let __content = wsr_core::content::current();
    let filed = Hits::new(__content.string(13721), 8);
    let dismissed = Hits::new("dismissed / pending", 3);
    par_seeds(1..=48, |seed| {
        let mut g = world_m(seed);
        let mut r = Rand(seed ^ 0x54);
        let ae = if seed % 4 == 0 { 1 } else { cast::<i32, _>(r.range(131, 1580)) };
        act_as(&mut g, ae);
        g.cash[ae] = f64r(&mut r, Ext::from_f64(50.0), Ext::from_f64(1500.0));
        g.t_bills[ae] = if r.chance(Ext::from_f64(0.5)) { Ext::from_f64(0.0) } else { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(300.0)) };
        let target = if seed % 9 == 0 { ae } else { cast::<i32, _>(r.range(131, 1580)) };
        g.entity_status[target] = 1;
        if seed % 7 == 0 {
            g.corp_controller[target] = 1;
        }
        for slot in 1..=10 {
            g.lawsuit_rec[slot] = PbStr::new();
            if r.chance(Ext::from_f64(0.5)) {
                let d = if r.chance(Ext::from_f64(0.2)) { target } else { cast::<i32, _>(r.range(131, 1580)) };
                g.lawsuit_rec[slot] = st(&format!("2| {}| {}| 5|", r.range(131, 1580), d));
            }
        }
        if seed % 6 == 0 {
            for slot in 1..=10 {
                g.lawsuit_rec[slot] = st("1 | 300| 400| 20| 500| 400|");
            }
        }
        g.rand_seed = cast::<i32, _>(r.range(0, 30000));
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0(seed));
        let mut ui = Drv::default();
        ui.boxes = [if seed % 5 == 0 { 7 } else { 6 }].into();
        let fee = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(500.0));
        ui.inputs = [sym(&g, target), if seed % 8 == 0 { "50".into() } else { format!("{fee:.1}") }, "0".into()].into();
        harassing_lawsuit(&mut port, &mut ui, &mut rng);
        if compare(HARASSING_LAWSUIT, &[], &g, &port, &rng, &ui, seed, "HarassingLawsuit").is_some() {
            filed.count((1..=10).any(|s| port.lawsuit_rec[s] != g.lawsuit_rec[s]));
            dismissed.count(ui.msgs.iter().any(|m| m.contains(__content.string(8070)) || m.contains("pending") || m.contains("thrown")));
        }
    });
    filed.check();
    dismissed.check();
}

// ------------------------------------------------------------------------------- FileAntitrustSuit

#[test]
fn diff_file_antitrust_suit() {
    let __content = wsr_core::content::current();
    let filed = Hits::new(__content.string(13721), 8);
    let dropped = Hits::new("oldest suit dropped", 2);
    par_seeds(1..=48, |seed| {
        let mut g = world_m(seed);
        let mut r = Rand(seed ^ 0x55);
        let ae = cast::<i32, _>(r.range(131, 1580));
        act_as(&mut g, ae);
        g.cash[ae] = if seed % 10 == 0 { Ext::from_f64(1.0) } else { f64r(&mut r, Ext::from_f64(50.0), Ext::from_f64(2000.0)) };
        g.t_bills[ae] = Ext::from_f64(0.0);
        let target = cast::<i32, _>(r.range(131, 1580));
        g.entity_status[target] = 1;
        if seed % 6 != 0 {
            g.industry_num[target] = g.industry_num[ae];
        }
        if seed % 9 == 0 {
            g.corp_controller[target] = 1;
        } else if g.corp_controller[target] == 1 {
            g.corp_controller[target] = 0;
        }
        g.private_antitrust_off = if seed % 17 == 0 { 1 } else { 0 };
        g.antitrust_plaintiff = if seed % 19 == 0 { ae } else { 0 };
        for k in 1..=10 {
            g.recent_antitrust_filers[k] = 0;
            g.lawsuit_rec[k] = PbStr::new();
            if r.chance(if seed % 4 == 0 { Ext::from_f64(1.0) } else { Ext::from_f64(0.3) }) {
                g.lawsuit_rec[k] = if r.chance(Ext::from_f64(0.5)) {
                    st(&format!("1 | {}| {}| 20| 500| 400|", r.range(131, 1580), r.range(131, 1580)))
                } else {
                    st(&format!("2| {}| {}| 5|", r.range(131, 1580), r.range(131, 1580)))
                };
            }
        }
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0(seed));
        let mut ui = Drv::default();
        let damages = match seed % 3 {
            0 => "50".to_string(),
            1 => "99999999".to_string(),
            _ => format!("{}", r.range(100, 2000)),
        };
        let mut inputs = vec![sym(&g, target), damages];
        if seed % 3 == 1 {
            inputs.push(format!("{}", r.range(100, 300)));
        }
        inputs.push(match seed % 4 {
            0 => String::new(),
            1 => "1".into(),
            _ => format!("{}", r.range(50, 150)),
        });
        ui.inputs = inputs.into();
        file_antitrust_suit(&mut port, &mut ui, &mut rng);
        if compare(FILE_ANTITRUST_SUIT, &[], &g, &port, &rng, &ui, seed, "FileAntitrustSuit").is_some() {
            filed.count((1..=10).any(|s| port.lawsuit_rec[s] != g.lawsuit_rec[s]));
            dropped.count(ui.news.iter().any(|n| n.contains(__content.string(13722))));
        }
    });
    filed.check();
    dropped.check();
}

// ------------------------------------------------------------------------------ ETF adviser / fees

/// The active entity (an insurer or broker controlled by player 1) advises some ETFs.
fn etf_world(seed: u64) -> (Globals, i32) {
    let mut g = world_m(seed);
    let mut r = Rand(seed ^ 0x56);
    let adv = if r.chance(Ext::from_f64(0.5)) { cast::<i32, _>(r.range(86, 130)) } else { cast::<i32, _>(r.range(131, 1580)) };
    act_as(&mut g, adv);
    g.industry_num[adv] = if adv <= 130 { 2 } else { 37 };
    for e in 1581..=1600 {
        if r.chance(Ext::from_f64(0.3)) {
            g.fund_adviser[e] = adv;
        }
    }
    (g, adv)
}

#[test]
fn diff_set_etf_advisory_fee() {
    let __content = wsr_core::content::current();
    let changed = Hits::new("fee changed", 5);
    let gains_text = Hits::new("refusal text with both gains compared", 1);
    let texts: Vec<Hits> = [__content.string(13723), __content.string(13724), __content.string(13725), __content.string(13726), __content.string(13710), __content.string(13727)]
        .iter()
        .map(|t| Hits::new(t, 1))
        .collect();
    par_seeds(1..=60, |seed| {
        let (mut g, adv) = etf_world(seed);
        let mut r = Rand(seed ^ 0x57);
        if seed % 10 == 0 {
            g.industry_num[adv] = 5;
        }
        let mut port = g.clone();
        let rng = Rng::from_state(rng0(seed));
        let mut ui = Drv::default();
        ui.boxes = (0..20).map(|_| [6, 6, 6, 7, 2][cast::<usize, _>(r.range(0, 4))]).collect();
        ui.inputs = (0..20).map(|_| ["0.2", ".5", "1", "0.3%", "7", "0"][cast::<usize, _>(r.range(0, 5))].to_string()).collect();
        set_etf_advisory_fee(&mut port, &mut ui);
        if let Some(o) = compare(SET_ETF_ADVISORY_FEE, &[], &g, &port, &rng, &ui, seed, "SetEtfAdvisoryFee") {
            // the Board's refusal names both 24-month figures (ETF gain, index gain): when it is the
            // last pop-up, its text is the original's last quick message (TMP_QM.DAT)
            if let Some(last) = ui.msgs.last().filter(|m| m.contains(__content.string(13728))) {
                let qm = o.files.get("TMP_QM.DAT").cloned().flatten().unwrap_or_default();
                let qm = String::from_utf8_lossy(&qm).to_string();
                // (ShowQuickMessage writes CRLF CRLF as "@@" and CRLF as "##")
                let want = last.trim_end().replace("\r\n\r\n", "@@").replace("\r\n", "##");
                assert_eq!(qm.trim_end(), want, "SetEtfAdvisoryFee seed {seed}: quick message text");
                gains_text.count(true);
            }
            changed.count((1581..=1600).any(|e| port.fund_fee_pct[e] != g.fund_fee_pct[e]));
            for h in &texts {
                h.count(ui.msgs.iter().any(|m| m.contains(h.1.as_str())));
            }
        }
    });
    changed.check();
    gains_text.check();
    texts.iter().for_each(Hits::check);
}

#[test]
fn diff_seek_become_etf_adviser() {
    let __content = wsr_core::content::current();
    let bought = Hits::new("contract bought", 5);
    let texts: Vec<Hits> = [
        __content.string(13729),
        __content.string(13730),
        __content.string(13731),
        __content.string(13732),
        __content.string(13733),
        __content.string(13734),
        __content.string(13735),
    ]
    .iter()
    .map(|t| Hits::new(t, 1))
    .collect();
    par_seeds(1..=60, |seed| {
        let (mut g, adv) = etf_world(seed);
        let mut r = Rand(seed ^ 0x58);
        for e in 1581..=1600 {
            if g.fund_adviser[e] == adv {
                g.fund_adviser[e] = 0;
            }
        }
        if seed % 10 == 0 {
            g.industry_num[adv] = 5;
        }
        g.cash[adv] = f64r(&mut r, Ext::from_f64(10.0), Ext::from_f64(3000.0));
        let etf = cast::<i32, _>(r.range(1581, 1590));
        if seed % 6 == 1 && adv <= 130 {
            // an insurer with almost no net worth buying an expensive contract
            g.cash[etf] = Ext::from_f64(60000.0);
            g.cash[adv] = Ext::from_f64(3000.0);
            g.t_bills[adv] = Ext::from_f64(0.0);
            g.ins_reserves[adv] = Ext::from_f64(2900.0) + g.business_assets[adv];
            for slot in 0..=15 {
                g.hold_stock_id[(slot, adv)] = 0;
                g.hold_bond_id[(slot, adv)] = 0;
            }
        }
        if seed % 5 == 0 {
            let other = cast::<i32, _>(r.range(131, 1580));
            g.fund_adviser[etf] = other;
            g.corp_controller[other] = 2;
        }
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0(seed));
        let mut ui = Drv::default();
        ui.boxes = (0..8).map(|_| [6, 6, 6, 7][cast::<usize, _>(r.range(0, 3))]).collect();
        let mut inputs = Vec::new();
        if seed % 4 == 0 {
            inputs.push(sym(&g, cast::<i32, _>(r.range(131, 1580))));
        }
        inputs.push(sym(&g, etf));
        if seed % 5 == 0 {
            inputs.push(String::new());
        }
        inputs.extend((0..6).map(|_| ["0.2", ".5", "0"][cast::<usize, _>(r.range(0, 2))].to_string()));
        ui.inputs = inputs.into();
        seek_become_etf_adviser(&mut port, &mut ui, &mut rng);
        if compare(SEEK_BECOME_ETF_ADVISER, &["0"], &g, &port, &rng, &ui, seed, "SeekBecomeEtfAdviser").is_some() {
            bought.count(port.fund_adviser[etf] == adv);
            for h in &texts {
                h.count(ui.msgs.iter().any(|m| m.contains(h.1.as_str())));
            }
        }
    });
    bought.check();
    texts.iter().for_each(Hits::check);
}
