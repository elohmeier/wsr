//! Differential tests of `logic::trade_screens::option_station`: ShowAdvancedOptionsStation (0x60b44b)
//! driven with the harness's scripted dialog actions (order sheet texts, button clicks), which covers
//! ProcessOptionOrderSheet (0x608b95), AutoBuildOptionStrategy (0x613212) with the strategy picker, and
//! the AdvOpt_* callbacks.
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Mutex;
use wsr_core::logic::newgame::LocalTime;
use wsr_core::logic::trade_screens::option_station::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::{ReportAction, Ui};

const STATION: u32 = 0x60b44b;
const TICKER: u32 = 0x41ef3f;
const CLOSE_ALL_OPTIONS: u32 = 0x611ab6;
const SIZE_LIMITS: u32 = 0x612d58;
const SHOW_OPTION_POSITIONS: u32 = 0x5f51fc;

/// Routines skipped on both sides (address, stdcall dword args).
const SKIPS: [(u32, u32); 4] = [(TICKER, 1), (CLOSE_ALL_OPTIONS, 1), (SIZE_LIMITS, 1), (SHOW_OPTION_POSITIONS, 3)];

fn is_action(a: &str) -> bool {
    ["click:", "close", "end", "select:", "dblclick:", "text:"].iter().any(|p| a.starts_with(p))
}

/// The harness's answer queue, shared by the Ui and the station's action source.
type Queue = Rc<RefCell<VecDeque<String>>>;

/// A Ui answering like the harness: MSGBOX / INPUTBOX take the queue's front only when it is not a dialog
/// action (else YES/OK resp. ""); ShowQuickMessage pop-ups (qm.exe) take nothing; the strategy picker
/// (`choose`) consumes dialog actions like the original's modal dialog.
struct H {
    q: Queue,
    replies: Queue,
    msgs: Vec<String>,
    titles: Vec<String>,
    inputs: usize,
    news: Vec<String>,
}

impl Ui for H {
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32 {
        self.msgs.push(text.to_display());
        self.titles.push(title.to_display());
        if style == 0 && title.is_empty() {
            return 1;
        }
        let yes_no = matches!(style & 0xf, 3 | 4);
        let mut q = self.q.borrow_mut();
        let result = match q.front() {
            Some(a) if !is_action(a) => match q.pop_front().unwrap().as_str() {
                "IDOK" => 1,
                "IDCANCEL" => 2,
                "IDYES" => 6,
                "IDNO" => 7,
                n => n.parse().unwrap_or(0),
            },
            _ => {
                if yes_no {
                    6
                } else {
                    1
                }
            }
        };
        self.replies.borrow_mut().push_back(result.to_string());
        result
    }
    fn inputbox(&mut self, prompt: &PbStr, _title: &PbStr, _default: &PbStr) -> PbStr {
        self.inputs += 1;
        let mut q = self.q.borrow_mut();
        let result = match q.front() {
            Some(a) if !is_action(a) => PbStr::from(q.pop_front().unwrap().as_str()),
            _ => PbStr::new(),
        };
        self.replies.borrow_mut().push_back(result.to_display());
        if std::env::var_os("WSR_DEBUG").is_some() { eprintln!("port input [{prompt}] -> {result}"); }
        result
    }
    fn report(&mut self, _title: &PbStr, _lines: &[PbStr], _yes_no: bool) -> ReportAction {
        self.replies.borrow_mut().push_back("click:1".into());
        ReportAction::Ok
    }
    fn news(&mut self, headline: &PbStr) {
        self.news.push(headline.to_display());
    }
    /// The strategy picker: option buttons 1403 + 3k select, GO (1) returns the selection, CANCEL (1433)
    /// none, closing keeps the selection; default action GO; after 5 actions the harness ends it.
    fn choose(&mut self, title: &PbStr, _prompt: &[PbStr], options: &[PbStr]) -> Option<usize> {
    let __content = wsr_core::content::current();
        assert_eq!(title.to_display(), __content.string(11793));
        assert_eq!(options.len(), 10);
        let mut sel = None;
        let mut q = self.q.borrow_mut();
        for _ in 0..5 {
            let a = match q.front() {
                Some(a) if is_action(a) => q.pop_front().unwrap(),
                _ => "click:1".to_string(),
            };
            self.replies.borrow_mut().push_back(if a == "close" { "sysclose".into() } else { a.clone() });
            match a.as_str() {
                "click:1" => return sel,
                "click:1433" => return None,
                "close" | "end" => return sel,
                _ => {
                    let id: usize = a[6..].parse().unwrap();
                    sel = Some((id - 1403) / 3);
                }
            }
        }
        sel
    }
}

/// The station's action source emulating the harness: `text:ID:v` set texts (no callback), then one
/// action per call (`click:ID`, `close`, `end`; default click 1 = CALC); after 5 actions the dialog is
/// destroyed.
fn harness_actions(q: Queue, replies: Queue) -> impl FnMut(&OptionStation, &mut dyn Ui) -> StationAction {
    let mut actions = 0;
    move |_st: &OptionStation, _ui: &mut dyn Ui| {
        let mut q = q.borrow_mut();
        if let Some(a) = q.front().filter(|a| a.starts_with("text:")).cloned() {
            q.pop_front();
            replies.borrow_mut().push_back(a.clone());
            let mut p = a.splitn(3, ':').skip(1);
            let id: i32 = p.next().unwrap().parse().unwrap();
            return StationAction::SetText(id, PbStr::from(p.next().unwrap()));
        }
        actions += 1;
        if actions > 5 {
            replies.borrow_mut().push_back("end".into());
            return StationAction::End;
        }
        let a = match q.front() {
            Some(a) if is_action(a) => q.pop_front().unwrap(),
            _ => "click:1".to_string(),
        };
        replies.borrow_mut().push_back(if a == "close" { "sysclose".into() } else { a.clone() });
        match a.as_str() {
            "close" => StationAction::Close,
            "end" => StationAction::End,
            x if x.starts_with("click:") => StationAction::Click(x[6..].parse().unwrap()),
            x => panic!("unexpected action {x}"),
        }
    }
}

fn rng_of(seed: u64) -> u32 {
    cast::<u32, _>(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 20)
}

/// Appends an option record `corp|type|holder|qty|strike|month|year|premium`.
#[allow(clippy::too_many_arguments)]
fn add_rec(g: &mut Globals, corp: i32, opt_type: i32, holder: i32, qty: i32, strike: Ext, month: i32, year: i32, prem: Ext) {
    let k = (1..=1000).find(|&k| g.option_recs[k].is_empty()).unwrap_or(1000);
    g.option_recs[k] = st(&format!("{corp}|{opt_type}|{holder}|{qty}|{strike}|{month}|{year}|{prem}"));
    let n = (1..=1000).filter(|&i| !g.option_recs[i].is_empty()).count();
    g.option_recs[0] = st(&n.to_string());
}

/// Takes the original's option record where only numeric fields differ by at most 1e-12 relative
/// (STR$ of 80-bit values in the 15th digit).
fn align_option_recs(port: &mut Globals, orig: &Globals) {
    for i in 0..=1000 {
        let (a, b) = (port.option_recs[i].to_display(), orig.option_recs[i].to_display());
        if a == b {
            continue;
        }
        let (pa, pb): (Vec<&str>, Vec<&str>) = (a.split('|').collect(), b.split('|').collect());
        if pa.len() != pb.len() || pa.len() < 2 {
            continue;
        }
        let close = pa.iter().zip(&pb).all(|(x, y)| {
            x == y
                || match (x.trim().parse::<Ext>(), y.trim().parse::<Ext>()) {
                    (Ok(x), Ok(y)) => (x - y).abs() <= Ext::from_f64(1e-12) * y.abs().max(Ext::from_f64(1e-300)),
                    _ => false,
                }
        });
        if close {
            port.option_recs[i] = orig.option_recs[i].clone();
        }
    }
}

struct Case {
    g: Globals,
    answers: Vec<String>,
}

/// A grid line's texts: valid most of the time, sometimes one bad field.
fn line(r: &mut Rand, g: &Globals, row: i32, price: Ext, bad: bool) -> Vec<String> {
    let side = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { "B" } else { "S" };
    let strike = cast::<i64, _>((price * f64r(r, Ext::from_f64(0.8), Ext::from_f64(1.2))).round().max(Ext::from_f64(5.0)));
    let month = r.range(1, 12);
    let year = cast::<i64, _>(g.cur_year) + r.range(0, 2);
    let exp = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { format!("{month}/{year}") } else { format!("{month:02}/{:02}", year % 100) };
    let qty = r.range(1, 6);
    let mut f = vec![side.to_string(), strike.to_string(), exp, qty.to_string()];
    if bad {
        let k = cast::<usize, _>(r.range(0, 3));
        f[k] = match k {
            0 => ["X", "b", "BS"][cast::<usize, _>(r.range(0, 2))].to_string(),
            1 => (cast::<i64, _>(price * f64r(r, Ext::from_f64(0.2), Ext::from_f64(0.6))) + if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { 0 } else { cast::<i64, _>(price * Ext::from_f64(2.0)) }).to_string(),
            2 => ["13/2030", "1-2030", "0/30", "1/1999", "12/2099", "6/2"][cast::<usize, _>(r.range(0, 5))].to_string(),
            _ => ["0", "11", "-1"][cast::<usize, _>(r.range(0, 2))].to_string(),
        };
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.08))) {
        // an incomplete line is cleared
        let k = cast::<usize, _>(r.range(1, 3));
        f[k] = String::new();
    }
    f.iter()
        .enumerate()
        .filter(|(_, v)| !v.is_empty())
        .map(|(c, v)| format!("text:{}:{}", 1320 + 10 * cast::<i32, _>(c) + row, v))
        .collect()
}

fn case(seed: u64) -> Case {
    let __content = wsr_core::content::current();
    let mut g = world(seed + 23_000);
    let mut r = Rand(seed * 69_069 + 7);
    let kind = seed % 12;
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    let cp = g.cur_player;
    g.app_title = st(__content.string(13661));
    g.is_computer_player[cp] = 0;
    g.option_recs[0] = st("0");
    g.sim_co_count = cast::<i32, _>(r.range(10, 1590));
    g.turn_action_count = cast::<i32, _>(r.range(0, 3));
    g.merger_target = 0;
    g.merger_acquirer = 0;
    g.currency_suffix = st("");
    // acting entity: the player, sometimes one of their companies
    g.active_entity = cp;
    g.active_entity_name = g.player_name[cp].clone();
    g.cash[cp] = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(20_000.0));
    g.t_bills[cp] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(5_000.0)) } else { Ext::from_f64(0.0) };
    g.bank_debt[cp] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(5_000.0)) } else { Ext::from_f64(0.0) };
    if r.chance(cast::<f64, _>(Ext::from_f64(0.25))) {
        let c = super::script::pick_active(&mut r, &g, 131, 1580, &[]);
        g.corp_controller[c] = cp;
        g.credit_rating_num[c] = if r.chance(cast::<f64, _>(Ext::from_f64(0.85))) { cast::<i32, _>(r.range(7, 10)) } else { cast::<i32, _>(r.range(1, 6)) };
        g.active_entity = c;
        g.active_entity_name = g.ent_name[c].clone();
        g.cash[c] = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(20_000.0));
    }
    // the option stock
    let stock = if seed == 241 { 1581 } else { super::script::pick_active(&mut r, &g, 131, 1580, &[g.active_entity]) };
    if seed == 241 { g.entity_status[stock] = 1; g.industry_num[stock] = 71; }
    super::script::clear_holders(&mut g, stock);
    for _ in 0..r.range(0, 3) {
        let h = super::script::pick_active(&mut r, &g, 11, 1580, &[stock]);
        super::script::give_holding(&mut g, h, stock, cast::<i16, _>(r.range(1, 25)), f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(100.0)));
    }
    g.corp_controller[stock] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { 0 } else { super::script::pick_active(&mut r, &g, 131, 1580, &[stock]) };
    g.credit_rating_num[stock] = cast::<i32, _>(r.range(2, 10));
    let price = f64r(&mut r, Ext::from_f64(12.0), Ext::from_f64(400.0));
    g.shares_outstanding[stock] = g.market_cap[stock] / price;
    g.sel_entity = stock;
    g.sel_corp_name = g.ent_name[stock].clone();
    g.last_sel_entity = super::script::pick_active(&mut r, &g, 131, 1580, &[stock]);
    // existing contracts on the stock (size limits)
    for _ in 0..r.range(0, 6) {
        let t = cast::<i32, _>(r.range(1, 2));
        let h = cast::<i32, _>(r.range(1, 1580));
        let q = cast::<i32, _>(r.range(1, 40)) * if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { 1 } else { -1 };
        let strike = g.market_cap[stock] * f64r(&mut r, Ext::from_f64(0.8), Ext::from_f64(1.2));
        let expiry_year = g.cur_year + 1;
        add_rec(&mut g, stock, t, h, q, strike.round(), cast::<i32, _>(r.range(1, 12)), expiry_year, Ext::from_f64(12.5));
    }
    let mut answers: Vec<String> = Vec::new();
    match kind {
        // entry checks
        0 => match r.range(0, 9) {
            0 => {
                let c = g.active_entity;
                if c > 10 {
                    g.credit_rating_num[c] = 3;
                }
            }
            1 => g.option_recs[0] = st("995"),
            2 => {
                // SelEntity controlled by the player: switch to LastSelEntity (asks) or refuse
                g.corp_controller[stock] = cp;
                if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                    g.corp_controller[g.last_sel_entity] = cp;
                }
                answers.push(["IDYES", "IDNO", "IDCANCEL"][cast::<usize, _>(r.range(0, 2))].to_string());
            }
            3 => {
                g.sel_entity = cp;
                if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                    g.corp_controller[g.last_sel_entity] = cp;
                }
            }
            4 => {
                let h = super::script::pick_active(&mut r, &g, 11, 1580, &[stock]);
                super::script::give_holding(&mut g, h, stock, if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { 95 } else { 100 }, Ext::from_f64(1.0));
            }
            5 => g.shares_outstanding[stock] = g.market_cap[stock] / f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(1500.0)).max(Ext::from_f64(1.0)),
            6 => {
                g.credit_rating_num[stock] = cast::<i32, _>(r.range(0, 1));
                // Keep banks/ETFs in their bounded id ranges; the dedicated ETF fixture below
                // uses a real fund id rather than changing an operating company's type.
                let _ = r.chance(cast::<f64, _>(Ext::from_f64(0.5)));
                g.industry_num[stock] = 20;
                g.proj_qtr_earnings[stock] = f64r(&mut r, -Ext::from_f64(5.0), Ext::from_f64(5.0));
            }
            7 => {
                g.merger_target = stock;
                g.merger_acquirer = super::script::pick_active(&mut r, &g, 131, 1580, &[stock]);
                g.tmp_ext1 = cast::<Ext, _>(r.range(0, 10));
                super::script::give_holding(&mut g, cp, stock, cast::<i16, _>(r.range(0, 9)), Ext::from_f64(1.0));
            }
            8 => {
                let a = g.active_entity;
                g.cash[a] = -f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(100.0));
                g.t_bills[a] = Ext::from_f64(0.0);
                g.bank_debt[a] = f64r(&mut r, Ext::from_f64(1e5), Ext::from_f64(1e6));
            }
            _ => {},
        },
        // close / clear / list / limits / close-all buttons
        1 => {
            let row = cast::<i32, _>(r.range(1, 8));
            answers.extend(line(&mut r, &g, row, price, false));
            for _ in 0..r.range(1, 3) {
                answers.push(["click:1372", "click:1379", "click:1381", "click:1380", "click:1376"][cast::<usize, _>(r.range(0, 4))].to_string());
            }
            match r.range(0, 2) {
                0 => answers.push("close".into()),
                1 => {
                    answers.push("click:1378".into());
                    answers.push(["IDYES", "IDNO"][cast::<usize, _>(r.range(0, 1))].into());
                    answers.push("end".into());
                }
                _ => answers.push("end".into()),
            }
        }
        // auto-trade
        2 | 3 => {
            answers.push("click:1382".into());
            if r.chance(cast::<f64, _>(Ext::from_f64(0.15))) {
                answers.push("click:1433".into());
            } else {
                answers.push(format!("click:{}", 1403 + 3 * r.range(0, 9)));
                if r.chance(cast::<f64, _>(Ext::from_f64(0.8))) {
                    answers.push("click:1".into());
                } else {
                    answers.push("close".into());
                }
                let mut tries = 0;
                loop {
                    tries += 1;
                    let m = r.range(0, 13);
                    let y = cast::<i64, _>(g.cur_year) + r.range(-1, 3);
                    let t = if r.chance(cast::<f64, _>(Ext::from_f64(0.15))) { String::new() } else if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { format!("{m}/{y}") } else { format!("{m}/{:02}", y % 100) };
                    answers.push(t.clone());
                    if t.is_empty() || tries > 3 || r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                        break;
                    }
                }
                for _ in 0..r.range(1, 3) {
                    answers.push(["10", "5", "1", "0", "12", "3", ""][cast::<usize, _>(r.range(0, 6))].to_string());
                }
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                answers.push("click:1374".into());
                answers.push(["IDYES", "IDNO"][cast::<usize, _>(r.range(0, 1))].into());
            }
            answers.push("end".into());
        }
        // the order sheet: CALC / SUBMIT
        _ => {
            let n = r.range(1, 5);
            let bad = r.chance(cast::<f64, _>(Ext::from_f64(0.3)));
            for k in 0..n {
                let row = cast::<i32, _>(r.range(1, 8));
                answers.extend(line(&mut r, &g, row, price, bad && k == 0));
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                answers.push("click:1".into());
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
                // big orders: too many / insufficient funds
                for row in 1..=8 {
                    answers.push(format!("text:{}:10", 1350 + row));
                }
            }
            answers.push("click:1374".into());
            answers.push(["IDYES", "IDYES", "IDNO"][cast::<usize, _>(r.range(0, 2))].into());
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                answers.push("click:1374".into());
                answers.push("IDYES".into());
            }
            answers.push("end".into());
        }
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.15))) {
        g.turn_action_count = 4;
    }
    if seed == 242 {
        // CalcEntityFinancials replaces any assigned rating. Use an actually insolvent company
        // so the credit-rating entry check is exercised after that recalculation.
        g = Globals::default();
        g.num_players = 2;
        g.cur_player = 1;
        g.active_entity = 131;
        g.active_entity_name = st("Insolvent Company");
        g.ent_name[131] = g.active_entity_name.clone();
        g.entity_status[131] = 1;
        g.industry_num[131] = 20;
        g.cash[131] = -Ext::from_f64(1.0e9);
        g.app_title = st(__content.string(13661));
        answers.clear();
    }
    if (243..=248).contains(&seed) {
        // Small, valid balance sheets make the remaining entry and affordability branches
        // deterministic; random corporations tend to have ample assets and borrowing capacity.
        g = Globals::default();
        g.num_players = 2;
        g.cur_player = 1;
        g.active_entity = 1;
        g.active_entity_name = st("Test Player");
        g.player_name[1] = g.active_entity_name.clone();
        g.ent_name[1] = g.active_entity_name.clone();
        g.entity_status[1] = 1;
        g.cash[1] = Ext::from_f64(10_000.0);
        g.app_title = st(__content.string(13661));
        g.currency_code = st("USA");
        g.exchange_rate = cast::<f64, _>(Ext::from_f64(1.0));
        g.cur_year = 2020;
        g.start_year = 2019;
        g.cur_quarter = 1;
        g.sim_co_count = 100;
        let stock = if seed == 247 { 1581 } else { 131 };
        g.sel_entity = stock;
        g.sel_corp_name = st("Test Stock");
        g.ent_name[stock] = g.sel_corp_name.clone();
        g.ent_symbol[stock] = st("TEST");
        g.entity_status[stock] = 1;
        g.industry_num[stock] = if seed == 247 { 71 } else { 20 };
        g.cash[stock] = Ext::from_f64(10_000.0);
        g.market_cap[stock] = Ext::from_f64(1_000.0);
        g.shares_outstanding[stock] = Ext::from_f64(10.0);
        answers = vec!["end".into()];
        match seed {
            243 => {
                g.entity_status[2] = 1;
                for i in 1..=995 {
                    g.option_recs[i] = st(&format!("131|1|2|1|{}|12|2021|1", 1000 + i));
                }
                g.option_recs[0] = st("995");
            }
            244 => {
                g.corp_controller[stock] = 1;
                g.last_sel_entity = 132;
                g.ent_name[132] = st("Alternative Stock");
                g.ent_symbol[132] = st("ALT");
                answers = vec!["IDCANCEL".into()];
            }
            245 => g.shares_outstanding[stock] = Ext::from_f64(200.0),
            246 => g.cash[stock] = -Ext::from_f64(1.0e9),
            248 => {
                g.cash[1] = Ext::from_f64(0.01);
                answers = ["text:1321:B", "text:1331:125", "text:1341:12/2020", "text:1351:10", "click:1374", "end"]
                    .into_iter().map(str::to_owned).collect();
            }
            _ => {}
        }
    }
    Case { g, answers }
}

/// The original's dialog-control texts of the station (`id → text`) after the run.
fn orig_grid(o: &Original) -> Option<(OptionGrid, PbStr)> {
    let __content = wsr_core::content::current();
    let d = o.dialogs.iter().rev().find(|d| d.title.to_display().starts_with(__content.string(14235)))?;
    let mut grid = OptionGrid::default();
    for id in 1321..=1368 {
        if let Some(c) = d.control(id) {
            grid.set_text(id, &c.text);
        }
    }
    Some((grid, d.control(1436).map(|c| c.text.clone()).unwrap_or_default()))
}

#[derive(Default)]
struct Hits(Mutex<Vec<String>>);

fn run_case(seed: u64, c: &Case, hits: &Hits) {
    let __content = wsr_core::content::current();
    let rng0 = rng_of(seed);
    let mut pg = c.g.clone();
    let mut rng = Rng::from_state(rng0);
    let q: Queue = Rc::new(RefCell::new(c.answers.iter().cloned().collect()));
    let replies: Queue = Default::default();
    let mut ui = H { q: q.clone(), replies: replies.clone(), msgs: vec![], titles: vec![], inputs: 0, news: vec![] };
    let skip: Vec<u32> = SKIPS.iter().map(|s| s.0).collect();
    let now = LocalTime::HARNESS;
    let env = StationEnv::skipping(&now, &skip);
    let mut next = harness_actions(q.clone(), replies.clone());
    let last = show_advanced_options_station_with(&mut pg, &mut ui, &mut rng, &env, 0, &mut next);
    let replies = replies.borrow();
    let answers: Vec<&str> = replies.iter().map(|s| s.as_str()).collect();
    let opts = Opts {
        args: vec!["v:0".into()],
        rng: Some(rng0),
        answers: &answers,
        skips: SKIPS.to_vec(),
        hooks: std::env::var("WSR_HOOKS").ok().map(|s| s.split(',').filter_map(|a| u32::from_str_radix(a.trim_start_matches("0x"), 16).ok()).collect()).unwrap_or_default(),
        ..Opts::default()
    };
    let Some(o) = run_original_with(STATION, &opts, &c.g) else { return };
    if std::env::var("WSR_DEBUG").is_ok() {
        eprintln!("seed {seed}{__wsr_text_13576}{:?}\nport titles {:?}\nport messages {:?}\nreplies {:?}", c.answers, ui.titles, ui.msgs, answers,__wsr_text_13576=__content.string(13576));
        for l in o.log.iter().filter(|l| !l.starts_with("API")) {
            eprintln!("seed {seed} orig: {l}");
        }
    }
    align_option_recs(&mut pg, &o.globals);
    let d = cmp().ignore("hDlgOptions").ignore("hOptionStrategyDlg").diffs(&pg, &o.globals);
    assert!(d.is_empty(), "seed {seed}: {} differences:\n{}", d.len(), d.iter().take(40).cloned().collect::<Vec<_>>().join("\n"));
    assert_eq!(rng.state(), o.rng, "rng seed {seed}");
    assert_eq!(ui.msgs.len(), orig_msgs(&o), "msgs seed {seed}: {:?}\n{:?}", ui.titles, o.log);
    let orig_inputs = o.log.iter().filter(|l| l.starts_with("INPUTBOX")).count();
    assert_eq!(ui.inputs, orig_inputs, "inputs seed {seed}");
    let orig_skips: Vec<String> = o.log.iter().filter(|l| l.starts_with("SKIP")).cloned().collect();
    assert_eq!(env.trace.borrow().len(), orig_skips.len(), "skipped calls seed {seed}: {orig_skips:?}");
    let orig_news = o.log.iter().filter(|l| l.contains("PostNews") || l.starts_with("NEWS")).count();
    let _ = orig_news;
    match (&last, orig_grid(&o)) {
        (Some(st), Some((grid, status))) => {
            assert_eq!(st.grid, grid, "grid seed {seed}");
            assert_eq!(st.status, status, "status seed {seed}");
            hits.0.lock().unwrap().push("dialog".into());
        }
        (None, None) => {}
        (a, b) => panic!("seed {seed}: dialog shown: port {} orig {}", a.is_some(), b.is_some()),
    }
    let mut h = hits.0.lock().unwrap();
    h.extend(ui.titles.iter().cloned());
    h.extend(ui.msgs.iter().cloned());
    h.extend(ui.news.iter().cloned());
}

#[test]
fn diff_show_advanced_options_station() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    let seeds: Vec<u64> = match std::env::var("WSR_ONLY_SEED").ok().and_then(|s| s.parse().ok()) {
        Some(s) => vec![s],
        None => (1..=248).collect(),
    };
    par_seeds(seeds, |seed| {
        let c = case(seed);
        run_case(seed, &c, &hits);
    });
    if unavailable().is_some() || std::env::var("WSR_ONLY_SEED").is_ok() {
        return;
    }
    let h = hits.0.lock().unwrap();
    let missing: Vec<_> = [
        "dialog",
        __content.string(14236),
        __content.string(14237),
        __content.string(14238),
        __content.string(14239),
        __content.string(14240),
        __content.string(14241),
        __content.string(14242),
        __content.string(14243),
        __content.string(14244),
        __content.string(14245),
        __content.string(14246),
        __content.string(14247),
        __content.string(14248),
        __content.string(14249),
        __content.string(14250),
        __content.string(14251),
        __content.string(1928),
        __content.string(11661),
        __content.string(11664),
        __content.string(14252),
        __content.string(11667),
        __content.string(14253),
        __content.string(11743),
        __content.string(14254),
        __content.string(14255),
        __content.string(3956),
        __content.string(11765),
        __content.string(14256),
        __content.string(11760),
    ].into_iter().filter(|w| !h.iter().any(|x| x.contains(w))).collect();
    assert!(missing.is_empty(), "branches not hit: {missing:?}");
}

/// The Ui-driven station: buttons and line edits through `choose` / `inputbox`.
#[test]
fn ui_driven_station_edits_and_closes() {
    let __content = wsr_core::content::current();
    let mut g = case(7).g;
    // make the entry checks pass
    let stock = g.sel_entity;
    g.corp_controller[stock] = 0;
    g.credit_rating_num[stock] = 8;
    g.industry_num[stock] = 20;
    let a = g.active_entity;
    g.credit_rating_num[a] = 9;
    g.cash[a] = Ext::from_f64(5000.0);
    g.option_recs[0] = st("0");
    let mut ui = wsr_core::ui::Scripted::default();
    // Edit line 5 (index 9 + 4), then CLEAR ALL (2), then CLOSE (8)
    for a in ["13", "B", "100", "1/2099", "5", "2", "8"] {
        ui.answers.push_back(st(a));
    }
    let mut rng = Rng::from_state(1);
    show_advanced_options_station(&mut g, &mut ui, &mut rng, &LocalTime::HARNESS, 0);
    let log = ui.log.join("\n");
    assert!(log.contains(__content.string(14451)), "{log}");
    assert_eq!(ui.log.iter().filter(|l| l.starts_with("INPUTBOX")).count(), 4, "{log}");
    assert!(ui.answers.is_empty(), "{log}");
}

#[test]
fn grid_control_ids() {
    let mut grid = OptionGrid::default();
    grid.set_text(1325, &st("B"));
    grid.set_text(1338, &st("120"));
    grid.set_text(1330, &st("ignored"));
    grid.set_text(1369, &st("ignored"));
    assert_eq!(grid.side[4], st("B"));
    assert_eq!(grid.strike[7], st("120"));
    assert_eq!(grid.text(1330), st(""));
    clear_option_grid_fields(&mut grid);
    assert_eq!(grid, OptionGrid::default());
}
