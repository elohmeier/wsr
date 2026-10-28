//! Differential tests of logic::trade_screens::alerts_alloc: SetPriceAlert (0x6828ca) with its alert-type
//! dialog and SetCommodityOrRateAlert, and ShowDlgBankAssetAlloc (0x69f0a2) with its callbacks, against
//! the original driven through the harness's scripted dialog actions.
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use std::collections::VecDeque;
use std::sync::Mutex;
use wsr_core::logic::newgame::LocalTime;
use wsr_core::logic::trade_screens::alerts_alloc::*;
use wsr_core::logic::valuation::calc_stock_price;
use wsr_core::pb::numeric::format;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::{ReportAction, Ui};

/// Dialog tokens of the test scripts (everything else answers a MSGBOX / INPUTBOX, in call order, like
/// the harness): `type:k` (alert type option k, then APPLY), `type:c` (CANCEL), `sym:TEXT` (symbol
/// dialog), `set:k:TEXT` (type into allocation field k), `update`, `apply`, `cancel`, `off`, `close`.
fn is_dlg(a: &str) -> bool {
    a.starts_with("type:")
        || a.starts_with("sym:")
        || a.starts_with("set:")
        || matches!(a, "update" | "apply" | "cancel" | "off" | "close")
}

#[derive(Default)]
struct Tok {
    q: VecDeque<String>,
    msgs: Vec<String>,
    titles: Vec<String>,
    inputs: usize,
    prompts: Vec<String>,
    field_text: Option<String>,
    /// Replies actually consumed, translated to harness actions. Unused input text must not be
    /// passed to a message box that appeared before its intended input dialog.
    replies: Vec<String>,
}

impl Ui for Tok {
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32 {
        self.msgs.push(text.to_display());
        self.titles.push(title.to_display());
        if style == 0 && title.is_empty() {
            return 1; // ShowQuickMessage (qm.exe): takes no answer
        }
        let result = match self.q.front() {
            Some(a) if a.starts_with("ID") || a.parse::<i32>().is_ok() => match self.q.pop_front().unwrap().as_str() {
                "IDOK" => 1,
                "IDCANCEL" => 2,
                "IDYES" => 6,
                "IDNO" => 7,
                n => n.parse().unwrap(),
            },
            _ => {
                if matches!(style & 0xf, 3 | 4) {
                    6
                } else {
                    1
                }
            }
        };
        self.replies.push(result.to_string());
        result
    }
    fn inputbox(&mut self, prompt: &PbStr, _title: &PbStr, _default: &PbStr) -> PbStr {
        self.prompts.push(prompt.to_display());
        if let Some(t) = self.field_text.take() {
            return PbStr::from(t.as_str()); // a text box of the allocation dialog
        }
        if let Some(a) = self.q.front()
            && a.starts_with("sym:")
        {
            let token = self.q.pop_front().unwrap();
            self.replies.extend(orig_answers(std::slice::from_ref(&token)));
            return PbStr::from(&token[4..]); // the symbol dialog
        }
        self.inputs += 1;
        let result = match self.q.front() {
            Some(a) if !is_dlg(a) => PbStr::from(self.q.pop_front().unwrap().as_str()),
            _ => PbStr::new(),
        };
        self.replies.push(result.to_display());
        result
    }
    fn report(&mut self, _title: &PbStr, _lines: &[PbStr], _yes_no: bool) -> ReportAction {
        ReportAction::Ok
    }
    fn news(&mut self, _headline: &PbStr) {}
    fn choose(&mut self, title: &PbStr, _prompt: &[PbStr], options: &[PbStr]) -> Option<usize> {
    let __content = wsr_core::content::current();
        let bank = title.to_display() == BANK_ALLOC_DLG_TITLE(&__content);
        if bank {
            assert_eq!(options.len(), 9);
        } else {
            assert_eq!(options.len(), 13);
        }
        let a = match self.q.front() {
            Some(a) if is_dlg(a) && !a.starts_with("sym:") => self.q.pop_front().unwrap(),
            _ => {
                // Ui::choose(None) closes the window. Drive the actual title-bar system command:
                // bare WM_CLOSE falls through DDT to an unimplemented Windows default procedure.
                self.replies.push(if bank { "sysclose" } else { "click:1" }.into());
                return None;
            }
        };
        self.replies.extend(orig_answers(std::slice::from_ref(&a)));
        if let Some(k) = a.strip_prefix("type:") {
            return k.parse().ok();
        }
        if let Some(r) = a.strip_prefix("set:") {
            let (k, t) = r.split_once(':').unwrap();
            self.field_text = Some(t.to_string());
            return Some(k.parse().unwrap());
        }
        match a.as_str() {
            "update" => Some(5),
            "apply" => Some(6),
            "cancel" => Some(7),
            "off" => Some(8),
            _ => None,
        }
    }
}

/// The harness's answer list for a script.
fn orig_answers(script: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for a in script {
        if let Some(k) = a.strip_prefix("type:") {
            match k.parse::<usize>() {
                Ok(k) => {
                    out.push(format!("click:{}", PRICE_ALERT_OPTION_IDS[k]));
                    out.push(format!("click:{PRICE_ALERT_APPLY_ID}"));
                }
                Err(_) => out.push("click:1".into()),
            }
        } else if let Some(t) = a.strip_prefix("sym:") {
            if t.is_empty() {
                out.push("click:2".into());
            } else {
                out.push(format!("text:310:{t}"));
                out.push("click:1".into());
            }
        } else if let Some(r) = a.strip_prefix("set:") {
            let (k, t) = r.split_once(':').unwrap();
            out.push(format!("text:{}:{t}", BANK_ALLOC_FIELD_IDS[k.parse::<usize>().unwrap()]));
        } else {
            out.push(match a.as_str() {
                "update" => format!("click:{BANK_ALLOC_UPDATE_ID}"),
                "apply" => format!("click:{BANK_ALLOC_APPLY_ID}"),
                "cancel" => format!("click:{BANK_ALLOC_CANCEL_ID}"),
                "off" => format!("click:{BANK_ALLOC_TURN_OFF_ID}"),
                "close" => "sysclose".into(),
                _ => a.clone(),
            });
        }
    }
    out
}

fn rng_of(seed: u64) -> u32 {
    cast::<u32, _>(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 20)
}

/// Run the port, then the original; compare globals, RND state and the message / input counts.
fn check(seed: u64, addr: u32, args: &[&str], g: &Globals, script: &[String], port: impl Fn(&mut Globals, &mut Tok, &mut Rng)) -> Option<Tok> {
    let __content = wsr_core::content::current();
    let rng0 = rng_of(seed);
    let mut pg = g.clone();
    let mut rng = Rng::from_state(rng0);
    let mut ui = Tok { q: script.iter().cloned().collect(), ..Tok::default() };
    port(&mut pg, &mut ui, &mut rng);
    let oa = ui.replies.clone();
    let oa: Vec<&str> = oa.iter().map(|s| s.as_str()).collect();
    if std::env::var("WSR_DEBUG").is_ok() { eprintln!("seed {seed}{__wsr_text_14176}{script:?}, replies {oa:?}",__wsr_text_14176=__content.string(14176)); }
    let o = run(addr, args, g, rng0, &oa, &[])?;
    if std::env::var("WSR_DEBUG").is_ok() {
        eprintln!("seed {seed}{__wsr_text_14176}{script:?}\nport titles {:?}", ui.titles,__wsr_text_14176=__content.string(14176));
        for l in o.log.iter().filter(|l| !l.starts_with("API")) {
            eprintln!("seed {seed} orig: {l}");
        }
    }
    let d = cmp()
        .abs_tol(Ext::from_f64(1e-10)) // f64 vs x87 cancellation can leave sub-picounit bond-value residuals
        .ignore("DlgSymbolEntry")
        .ignore("hPriceAlertDlg")
        .ignore("DlgBankAssetAlloc")
        .diffs(&pg, &o.globals);
    assert!(d.is_empty(), "seed {seed}: {} differences:\n{}", d.len(), d.iter().take(40).cloned().collect::<Vec<_>>().join("\n"));
    assert_eq!(rng.state(), o.rng, "rng seed {seed}");
    assert_eq!(ui.msgs.len(), orig_msgs(&o), "msgs seed {seed}: {:?}\n{:?}", ui.titles, o.log);
    let orig_inputs = o.log.iter().filter(|l| l.starts_with("INPUTBOX")).count();
    assert_eq!(ui.inputs, orig_inputs, "inputs seed {seed}");
    Some(ui)
}

#[derive(Default)]
struct Hits(Mutex<Vec<String>>);

impl Hits {
    fn record(&self, ui: &Tok) {
        let mut h = self.0.lock().unwrap();
        h.extend(ui.titles.iter().cloned());
        h.extend(ui.msgs.iter().cloned());
        h.extend(ui.prompts.iter().cloned());
    }
    fn add(&self, s: &str) {
        self.0.lock().unwrap().push(s.to_string());
    }
    fn assert(&self, want: &[&str]) {
        if unavailable().is_some() || std::env::var("WSR_ONLY_SEED").is_ok() {
            return;
        }
        let h = self.0.lock().unwrap();
        for w in want {
            assert!(h.iter().any(|x| x.contains(w)), "branch not hit: {w:?}");
        }
    }
}

/// Current value of commodity / rate alert type `t` (3002..3013) as SetCommodityOrRateAlert shows it.
fn cur_text(g: &Globals, t: i32) -> String {
    let v = match t {
        3002 => cast::<Ext, _>(g.oil_price) * Ext::from_f64(100.0),
        3003 => cast::<Ext, _>(g.gold_price),
        3004 => cast::<Ext, _>(g.silver_price),
        3005 => cast::<Ext, _>(g.wheat_price),
        3006 => cast::<Ext, _>(g.corn_price),
        3007 => cast::<Ext, _>(g.bitcoin_price),
        3008 => cast::<Ext, _>(g.ethereum_price),
        3009 => g.stock_index,
        3010 => cast::<Ext, _>(g.prime_rate),
        3011 => cast::<Ext, _>(g.long_bond_yield),
        3012 => cast::<Ext, _>(g.short_bond_yield),
        _ => cast::<Ext, _>(0.1f32) * cast::<Ext, _>(wsr_core::pb::numeric::to_long(cast::<Ext, _>(g.gdp_growth) * Ext::from_f64(10.0))),
    };
    let m: &[u8] = if t == 3013 { b"* #########.0" } else { b"* ###.00" };
    String::from_utf8(format(v, m)).unwrap().trim().to_string()
}

/// A world with price alerts: other players' alerts, the current player's unreached ones (some expired)
/// and stale ones (entity < 11 / inactive stock); sometimes (almost) full.
fn alert_world(seed: u64) -> (Globals, Rand) {
    let mut g = world(seed);
    let mut r = Rand(seed.wrapping_mul(0x5851_f42d_4c95_7f2d) | 1);
    g.bitcoin_price = cast::<i32, _>(r.range(1000, 90000));
    g.ethereum_price = cast::<i32, _>(r.range(100, 5000));
    g.currency_code = st(if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { "USA" } else { "GBR" });
    g.currency_suffix = st(if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { "" } else { " GBP" });
    let n = match r.range(0, 9) {
        0 => 100,
        1 => 99,
        _ => r.range(0, 30),
    };
    for slot in 1..=n {
        let owner = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { 1 } else { r.range(2, 5) };
        let (ent, target) = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
            let first = if r.chance(cast::<f64, _>(Ext::from_f64(0.05))) { 1 } else { 11 };
            let c = cast::<i32, _>(r.range(first, 1600));
            let cap = if c > 10 { g.market_cap[c] } else { Ext::from_f64(100.0) };
            (c, cap)
        } else {
            (cast::<i32, _>(r.range(3002, 3013)), Ext::from_f64(50.0))
        };
        let up = r.chance(cast::<f64, _>(Ext::from_f64(0.5)));
        let target = if up { target * Ext::from_f64(50.0) + Ext::from_f64(1e6) } else { -Ext::from_f64(1e6) };
        let expired = r.chance(cast::<f64, _>(Ext::from_f64(0.2)));
        let (q, y, t) = if expired {
            (g.cur_quarter, g.cur_year, g.sim_co_count - cast::<i32, _>(r.range(0, 5)))
        } else {
            (g.cur_quarter, g.cur_year + 1, g.sim_co_count)
        };
        g.price_alerts[cast::<i32, _>(slot)] = st(&format!(
            "{ent}|{owner}|{}|{target}|{q}|{y}|{t}",
            if up { "UP" } else { "DN" }
        ));
    }
    if n == 99 && r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
        // one free slot in the middle
        let k = cast::<i32, _>(r.range(1, 98));
        g.price_alerts[k] = st("");
        g.price_alerts[99] = st("3003|2|UP|99999|1|3000|1");
    }
    g.price_alerts[0] = st(&n.to_string());
    (g, r)
}

#[test]
fn diff_set_price_alert() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(1..=90, |seed| {
        let (mut g, mut r) = alert_world(seed);
        // active entity choice
        match r.range(0, 3) {
            0 => {
                g.sel_entity = cast::<i32, _>(r.range(1, 10));
                g.last_sel_entity = cast::<i32, _>(r.range(0, 10));
            }
            1 => {
                g.sel_entity = cast::<i32, _>(r.range(0, 10));
                g.last_sel_entity = cast::<i32, _>(r.range(11, 1600));
            }
            _ => {
                g.sel_entity = cast::<i32, _>(r.range(11, 1600));
                g.last_sel_entity = cast::<i32, _>(r.range(0, 1600));
            }
        }
        if g.sel_entity > 10 {
            g.sel_corp_name = g.ent_name[g.sel_entity].clone();
        }
        let mut script: Vec<String> = Vec::new();
        let rounds = r.range(1, 3);
        let mut sel = g.sel_entity;
        let last = g.last_sel_entity;
        for _ in 0..rounds {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) {
                script.push("type:c".into());
                break;
            }
            let k = if r.chance(cast::<f64, _>(Ext::from_f64(0.35))) { 0 } else { cast::<usize, _>(r.range(1, 12)) };
            script.push(format!("type:{k}"));
            if k == 0 {
                let mut corp = if sel > 10 { sel } else { last };
                if sel < 11 && last < 11 {
                    if r.chance(cast::<f64, _>(Ext::from_f64(0.15))) {
                        script.push("sym:".into());
                        break;
                    }
                    let c = loop {
                        let c = cast::<i32, _>(r.range(131, 1580));
                        if g.entity_status[c] == 1 && g.shares_outstanding[c] > Ext::from_f64(0.0) {
                            break c;
                        }
                    };
                    script.push(format!("sym:{}", g.ent_symbol[c].to_display()));
                    corp = c;
                    sel = c;
                }
                let mut pg = g.clone();
                pg.valuation_mode = 0;
                calc_stock_price(&mut pg, &mut Tok::default(), &mut Rng::from_state(1), corp, 0);
                let cur = pg.stock_price_str.to_display().trim().to_string();
                let price = if g.shares_outstanding[corp] > Ext::from_f64(0.0) { g.market_cap[corp] / g.shares_outstanding[corp] } else { Ext::from_f64(10.0) };
                if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
                    script.push(cur.clone()); // equal: input error, asked again
                }
                if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) {
                    script.push(if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { "0".into() } else { "-3".into() });
                    break;
                }
                let f = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { f64r(&mut r, Ext::from_f64(1.05), Ext::from_f64(3.0)) } else { f64r(&mut r, Ext::from_f64(0.2), Ext::from_f64(0.95)) };
                script.push(format!("{:.2}", price * f));
            } else {
                let t = 3001 + cast::<i32, _>(k);
                let cur = cur_text(&g, t);
                if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
                    script.push(cur.clone());
                }
                if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) {
                    script.push(if t == 3013 || r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { String::new() } else { "-1".into() });
                    break;
                }
                let v: Ext = cur.trim_end_matches('%').trim().parse().unwrap();
                let nv = if t == 3013 {
                    v + f64r(&mut r, -Ext::from_f64(6.0), Ext::from_f64(6.0)).round() + Ext::from_f64(0.5)
                } else if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                    v * f64r(&mut r, Ext::from_f64(1.05), Ext::from_f64(2.0)) + Ext::from_f64(0.01)
                } else {
                    v * f64r(&mut r, Ext::from_f64(0.3), Ext::from_f64(0.95))
                };
                let txt = if nv > Ext::from_f64(1000.0) && r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                    let i = cast::<i64, _>(nv.trunc());
                    format!("{},{:03}.{:02}", i / 1000, i % 1000, cast::<i64, _>((nv.fract()) * Ext::from_f64(100.0)))
                } else if t >= 3010 && r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                    format!("{nv:.2}%")
                } else {
                    format!("{nv:.2}")
                };
                script.push(txt);
            }
        }
        if seed == 1 {
            // A genuinely full book after expiry and price checks: all targets remain unreached.
            g.price_alerts[0] = st("100");
            for slot in 1..=100 {
                g.price_alerts[slot] = st(&format!("3003|2|UP|1e30|1|{}|1", g.cur_year + 10));
            }
            script = vec!["type:2".into()];
        }
        if let Some(ui) = check(seed, 0x6828ca, &[], &g, &script, |g, ui, rng| set_price_alert(g, ui, rng, &LocalTime::HARNESS)) {
            hits.record(&ui);
            if script.iter().any(|a| a.starts_with("sym:") && a.len() > 4) {
                hits.add("SYM");
            }
        }
    });
    hits.assert(&[
        __content.string(11298),
        __content.string(11310),
        __content.string(14177),
        __content.string(14178),
        __content.string(14179),
        __content.string(14180),
        "Input error: The rate you set",
        __content.string(14181),
        "SYM",
        __content.string(14182),
        "RATE CHANGE ALERT",
        __content.string(14183),
    ]);
}

/// The bank allocation dialog: random banks, settings and action scripts.
#[test]
fn diff_show_dlg_bank_asset_alloc() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(1..=80, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ 0x1234_5678);
        let bank = loop {
            let b = cast::<i32, _>(r.range(11, 85));
            if g.entity_status[b] == 1 {
                break b;
            }
        };
        g.active_entity = bank;
        g.active_entity_name = g.ent_name[bank].clone();
        g.corp_controller[bank] = 1;
        let mut left = 100;
        for (k, p) in [10, 25, 20, 25].iter().enumerate() {
            let v = (*p + cast::<i32, _>(r.range(-8, 8))).min(left);
            left -= v;
            match k {
                0 => g.bank_alloc_prime_mort_pct[bank] = cast::<i16, _>(v),
                1 => g.bank_alloc_subprime_pct[bank] = cast::<i16, _>(v),
                2 => g.bank_alloc_consumer_pct[bank] = cast::<i16, _>(v),
                _ => g.bank_alloc_bonds_pct[bank] = cast::<i16, _>(v),
            }
        }
        g.bank_alloc_cash_pct[bank] = cast::<i16, _>(left);
        g.bank_alloc_off[bank] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { 1 } else { 0 };
        g.bank_alloc_help_plr = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { 0 } else { 1 };
        g.turn_action_count = cast::<i32, _>(r.range(0, 5));
        let mut script: Vec<String> = Vec::new();
        if g.bank_alloc_off[bank] == 1 {
            script.push(if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { "IDNO" } else { "IDYES" }.into());
        }
        let mut pcts: Vec<i32> = vec![
            cast::<i32, _>(g.bank_alloc_prime_mort_pct[bank]),
            cast::<i32, _>(g.bank_alloc_subprime_pct[bank]),
            cast::<i32, _>(g.bank_alloc_consumer_pct[bank]),
            cast::<i32, _>(g.bank_alloc_bonds_pct[bank]),
            cast::<i32, _>(g.bank_alloc_cash_pct[bank]),
        ];
        for _ in 0..r.range(1, 3) {
            // edit some fields: a valid 100% split, an off-by total, or cash below 10
            match r.range(0, 4) {
                0 | 1 => {
                    let mut rest = 100;
                    let cash = cast::<i32, _>(r.range(10, 40));
                    rest -= cash;
                    for k in 0..4 {
                        let v = if k == 3 { rest } else { cast::<i32, _>(r.range(0, cast::<i64, _>(rest))) };
                        rest -= v;
                        pcts[k] = v;
                    }
                    pcts[4] = cash;
                    for k in 0..5 {
                        if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) {
                            let txt = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { format!("{}.4", pcts[k]) } else { pcts[k].to_string() };
                            script.push(format!("set:{k}:{txt}"));
                        }
                    }
                }
                2 => {
                    let k = cast::<usize, _>(r.range(0, 3));
                    script.push(format!("set:{k}:{}", pcts[k] + cast::<i32, _>(r.range(1, 20))));
                }
                _ => {
                    let c = r.range(0, 9);
                    script.push(format!("set:4:{c}"));
                    if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                        let k = cast::<usize, _>(r.range(0, 3));
                        script.push(format!("set:{k}:{}", (pcts[k] + pcts[4] - 10).max(0)));
                    }
                }
            }
            let a = match r.range(0, 9) {
                0..=4 => "apply",
                5 | 6 => "update",
                7 => "cancel",
                _ => "off",
            };
            script.push(a.into());
            if a == "off" {
                script.push(if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { "IDNO" } else { "IDYES" }.into());
            }
        }
        script.push("close".into());
        if let Some(ui) = check(seed, 0x69f0a2, &["v:0"], &g, &script, |g, ui, rng| show_dlg_bank_asset_alloc(g, ui, rng, 0)) {
            hits.record(&ui);
        }
    });
    hits.assert(&[
        __content.string(14184),
        __content.string(11252),
        __content.string(14185),
        __content.string(14186),
        __content.string(14187),
        __content.string(14188),
        __content.string(14189),
        __content.string(14190),
    ]);
}

/// Window plumbing and the pure dialog functions.
#[test]
fn dialog_plumbing() {
    let __content = wsr_core::content::current();
    use wsr_core::logic::restructure::dlg_util::{Nmhdr, TTN_NEEDTEXT};
    let mut g = Globals::default();
    for k in 0..=40 {
        g.brush_handles[k] = 100 + cast::<i32, _>(k);
    }
    assert_eq!(dlg_proc_price_alert_colors(&g, 310, true, 0, None), 130);
    assert_eq!(dlg_proc_price_alert_colors(&g, 312, false, 1200, None), 115);
    assert_eq!(dlg_proc_price_alert_colors(&g, 309, false, 1239, None), 130);
    let mut nm = Nmhdr { hwnd_from: 1242, code: TTN_NEEDTEXT, ..Nmhdr::default() };
    dlg_proc_price_alert_colors(&g, 78, false, 0, Some(&mut nm));
    assert!(nm.text.to_display().starts_with(__content.string(11313)));
    assert_eq!(dlg_proc_bank_asset_alloc_colors(&g, 310, true, 0, None), 109);
    assert_eq!(dlg_proc_bank_asset_alloc_colors(&g, 307, false, 1055, None), 114);
    assert_eq!(dlg_proc_bank_asset_alloc_colors(&g, 307, false, 1085, None), 100);
    let mut nm = Nmhdr { hwnd_from: 1125, code: TTN_NEEDTEXT, ..Nmhdr::default() };
    dlg_proc_bank_asset_alloc_colors(&g, 78, false, 0, Some(&mut nm));
    assert!(nm.text.to_display().starts_with(__content.string(11280)));
    // APPLY without a selection
    let mut ui = Tok::default();
    g.tmp_ext1 = Ext::from_f64(17.0);
    dlg_price_alert_ok(&mut g, &mut ui);
    assert_eq!(g.tmp_ext1, Ext::from_f64(0.0));
    assert_eq!(ui.msgs, vec![__content.string(11312).to_string()]);
    dlg_price_alert_opt(&mut g, 3007);
    dlg_price_alert_ok(&mut g, &mut ui);
    assert_eq!(g.tmp_ext1, Ext::from_f64(3007.0));
    g.sel_entity = 3;
    g.last_sel_entity = 0;
    assert_eq!(price_alert_type_options(&g)[0].to_display(), __content.string(14191));
}
