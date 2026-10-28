//! Differential tests of `logic::assets::offers`: ShowAssetSaleOffers (0x666a27) and OfferAssetsForSale
//! (0x6699ec).
#![allow(unused_imports)]
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use std::collections::VecDeque;
use std::sync::Mutex;
use wsr_core::logic::assets::offers::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::{ReportAction, Ui};

const SHOW_OFFERS: u32 = 0x666a27;
const OFFER: u32 = 0x6699ec;
const BUY_STOCK_MENU: u32 = 0x58d471;

/// Scripted UI sharing one answer queue the way the harness does: MSGBOX answers (`IDYES`, … or a number)
/// and INPUTBOX texts are popped in call order; `pick:k` tokens are dialog actions taken only by a report,
/// which then picks its k-th (mod count) link line. ShowQuickMessage pop-ups (style 0, no title) take
/// nothing. Records the listbox index of every pick so the original can be driven with `select:301:N`.
#[derive(Default)]
struct Script {
    q: VecDeque<String>,
    msgs: Vec<String>,
    titles: Vec<String>,
    inputs: usize,
    /// (token, listbox index or None when the report had no link line)
    picks: Vec<(String, Option<usize>)>,
}

fn is_pick(a: &str) -> bool {
    a.starts_with("pick:")
}

impl Ui for Script {
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32 {
        self.msgs.push(text.to_display());
        self.titles.push(title.to_display());
        let yes_no = matches!(style & 0xf, 3 | 4);
        if style == 0 && title.is_empty() {
            return 1;
        }
        match self.q.front() {
            Some(a) if !is_pick(a) => {
                let a = self.q.pop_front().unwrap();
                match a.as_str() {
                    "IDOK" => 1,
                    "IDCANCEL" => 2,
                    "IDYES" => 6,
                    "IDNO" => 7,
                    n => n.parse().unwrap(),
                }
            }
            _ => {
                if yes_no {
                    6
                } else {
                    1
                }
            }
        }
    }
    fn inputbox(&mut self, _prompt: &PbStr, _title: &PbStr, _default: &PbStr) -> PbStr {
        self.inputs += 1;
        match self.q.front() {
            Some(a) if !is_pick(a) => PbStr::from(self.q.pop_front().unwrap().as_str()),
            _ => PbStr::new(),
        }
    }
    fn report(&mut self, _title: &PbStr, lines: &[PbStr], _yes_no: bool) -> ReportAction {
        match self.q.front() {
            Some(a) if is_pick(a) => {
                let tok = self.q.pop_front().unwrap();
                let k: usize = tok[5..].parse().unwrap();
                let links: Vec<usize> =
                    (0..lines.len()).filter(|&i| lines[i].to_display().contains("@C")).collect();
                if links.is_empty() {
                    self.picks.push((tok, None));
                    return ReportAction::Ok;
                }
                let idx = links[k % links.len()];
                self.picks.push((tok, Some(idx)));
                let l = lines[idx].to_display();
                ReportAction::Link(PbStr::from(&l[l.len() - 6..]))
            }
            _ => ReportAction::Ok,
        }
    }
    fn news(&mut self, _headline: &PbStr) {}
}

/// The original's answer list for `answers` after the port's run: `pick:k` → `select:301:<index>`.
fn orig_answers(answers: &[String], ui: &Script) -> Vec<String> {
    let mut picks = ui.picks.iter();
    let mut out = Vec::new();
    for a in answers {
        if is_pick(a) {
            // unconsumed or link-less picks stay as a dialog action that blocks MSGBOX/INPUTBOX answers
            // the same way the port's token does
            match picks.next() {
                Some((_, Some(i))) => out.push(format!("select:301:{i}")),
                _ => out.push("select:301:0".to_string()),
            }
        } else {
            out.push(a.clone());
        }
    }
    out
}

fn rng_of(seed: u64) -> u32 {
    cast::<u32, _>(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 20)
}

/// Puts a holding of `pct` % of `stock` into a slot of `holder` (its existing slot, or a free one).
fn hold(g: &mut Globals, holder: i32, stock: i32, pct: i16) {
    let mut slot = 0;
    for s in 1..=15 {
        if cast::<i32, _>(g.hold_stock_id[(s, holder)]) == stock {
            slot = s;
            break;
        }
        if slot == 0 && g.hold_stock_id[(s, holder)] == 0 {
            slot = s;
        }
    }
    if slot == 0 {
        slot = 15;
    }
    g.hold_stock_id[(slot, holder)] = cast::<i16, _>(stock);
    g.hold_stock_pct[(slot, holder)] = pct;
    g.hold_stock_basis[(slot, holder)] = Ext::from_f64(10.0);
    g.hold_stock_id[(0, holder)] = cast::<i16, _>((1..=15).filter(|&s| g.hold_stock_id[(s, holder)] != 0).count());
}

/// No short positions (the original hangs / errors on some of them; see the routine docs).
fn no_shorts(g: &mut Globals) {
    for e in 1..=1600 {
        for s in 1..=15 {
            if g.hold_stock_pct[(s, e)] < 0 {
                g.hold_stock_pct[(s, e)] = -g.hold_stock_pct[(s, e)];
            }
        }
    }
}

/// A world with pending stock and business-asset offers, an acting entity `ActiveEntity` and a scenario
/// for the pick.
fn offers_world(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let mut g = world(seed);
    let mut r = Rand(seed.wrapping_mul(7919) + 13);
    no_shorts(&mut g);
    g.sel_entity = 0;
    g.cur_player = 1;
    g.is_computer_player[1] = 0;
    // acting entity
    let active = match seed % 6 {
        0 => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
        1 => cast::<i32, _>(r.range(11, 130)),
        _ => cast::<i32, _>(r.range(131, 1580)),
    };
    g.active_entity = active;
    g.entity_status[active] = 1;
    g.active_entity_name = g.ent_name[active].clone();
    g.cur_player_name = g.player_name[1].clone();
    if active > 130 && r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        g.industry_num[active] = 70;
        g.tax_loss_carry[active] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { -f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(60.0)) } else { Ext::from_f64(0.0) };
        g.tax_credits[active] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(30.0)) } else { Ext::from_f64(0.0) };
    }
    if active > 10 && r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
        g.corp_controller[active] = 1;
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
        g.cash[active] = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(3000.0));
    }
    let n = cast::<i32, _>(if seed.is_multiple_of(17) { 0 } else { r.range(1, 14) });
    let ind = g.industry_num[active];
    for k in 1..=n {
        let seller = match r.range(0, 9) {
            0 => active,
            1 => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
            _ => cast::<i32, _>(r.range(131, 1580)),
        };
        g.entity_status[seller] = if r.chance(cast::<f64, _>(Ext::from_f64(0.95))) { 1 } else { 0 };
        let (q, y) = if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) {
            (cast::<i32, _>(r.range(1, 4)), g.cur_year + 1)
        } else {
            (if r.chance(cast::<f64, _>(Ext::from_f64(0.8))) { cast::<i32, _>(r.range(cast::<i64, _>(g.cur_quarter), 4)) } else { 7 }, g.cur_year)
        };
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) && seller > 130 {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) && (3..70).contains(&ind) {
                g.industry_num[seller] = ind;
            } else if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) {
                g.industry_num[seller] = 70;
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) {
                g.business_assets[seller] = Ext::from_f64(0.0);
            }
            let amt = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { cast::<i64, _>(g.business_assets[seller]) + r.range(0, 50) } else { r.range(5, 600) };
            g.asset_sale_offers[k] = st(&format!("{seller}{__wsr_text_13580}{amt}{__wsr_text_13588}{q}|{y}",__wsr_text_13580=__content.string(13580), __wsr_text_13588=__content.string(13588)));
        } else {
            let stock = cast::<i32, _>(r.range(131, 1600));
            g.entity_status[stock] = 1;
            let pct = r.range(1, 60);
            if r.chance(cast::<f64, _>(Ext::from_f64(0.9))) {
                hold(&mut g, seller, stock, cast::<i16, _>((pct + r.range(-10, 20)).clamp(1, 99)));
            }
            g.asset_sale_offers[k] = st(&format!("{seller}|{stock}|{pct}{__wsr_text_13588}{q}|{y}",__wsr_text_13588=__content.string(13588)));
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) && seller > 10 {
            g.corp_controller[seller] = 1;
        }
    }
    g.asset_sale_offers[0] = st(&n.to_string());
    if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
        g.currency_symbol = st(["", "EUR", "Fr", "$"][cast::<usize, _>(r.range(0, 3))]);
    }
    g
}

/// MSGBOX/INPUTBOX counts in the original's log.
fn orig_inputs(o: &Original) -> usize {
    o.log.iter().filter(|l| l.starts_with("INPUTBOX")).count()
}

/// Run the port, then the original with the equivalent answers; compare state, RND, UI counts.
fn check(
    seed: u64,
    addr: u32,
    args: &[String],
    g: &Globals,
    answers: &[String],
    port: impl Fn(&mut Globals, &mut Script, &mut Rng),
    hits: &Mutex<Vec<String>>,
) {
    let rng0 = rng_of(seed);
    let mut pg = g.clone();
    let mut rng = Rng::from_state(rng0);
    let mut ui = Script { q: answers.iter().cloned().collect(), ..Script::default() };
    port(&mut pg, &mut ui, &mut rng);
    let oa = orig_answers(answers, &ui);
    let oa: Vec<&str> = oa.iter().map(|s| s.as_str()).collect();
    let opts = Opts {
        args: args.to_vec(),
        rng: Some(rng0),
        answers: &oa,
        hooks: vec![BUY_STOCK_MENU],
        ..Opts::default()
    };
    let Some(o) = run_original_with(addr, &opts, g) else { return };
    cmp().assert(&pg, &o.globals);
    assert_eq!(rng.state(), o.rng, "rng seed {seed}");
    assert_eq!(ui.msgs.len(), orig_msgs(&o), "msgs seed {seed}: {:?}\n{:?}", ui.titles, o.log);
    assert_eq!(ui.inputs, orig_inputs(&o), "inputs seed {seed}");
    let mut h = hits.lock().unwrap();
    if addr == SHOW_OFFERS && o.log.iter().any(|l| l.starts_with("CALL BuyStockMenu(")) {
        h.push("STOCK OFFER PICKED".to_string()); // BuyStockMenu + CheckCrossOwnership path
    }
    if addr == OFFER && ui.picks.iter().any(|p| p.1.is_some()) && pg.asset_sale_offers[0] != g.asset_sale_offers[0] {
        h.push("STOCK PICKED AND OFFERED".to_string());
    }
    h.extend(ui.titles.iter().cloned());
    h.extend(ui.msgs.iter().cloned());
}

fn answer_pool(r: &mut Rand) -> String {
    match r.range(0, 9) {
        0..=2 => "IDYES".into(),
        3 => "IDNO".into(),
        4 => "IDCANCEL".into(),
        5 => "0".into(),
        6 => r.range(1, 40).to_string(),
        7 => r.range(40, 700).to_string(),
        _ => r.range(700, 99999).to_string(),
    }
}

#[test]
fn diff_show_offers_pick() {
    let __content = wsr_core::content::current();
    let hits = Mutex::new(Vec::new());
    par_seeds(1..=160, |seed| {
        let mut g = offers_world(seed);
        let mut r = Rand(seed * 31 + 5);
        if seed.is_multiple_of(23) {
            g.game_end_flag = st("Y");
        }
        let mut answers = vec![format!("pick:{}", r.range(0, 20))];
        for _ in 0..6 {
            answers.push(answer_pool(&mut r));
        }
        check(seed, SHOW_OFFERS, &[], &g, &answers, |g, ui, rng| show_asset_sale_offers(g, ui, rng, &wsr_core::logic::newgame::LocalTime::HARNESS), &hits);
    });
    let h = hits.into_inner().unwrap();
    for want in [
        __content.string(2013),
        __content.string(2101),
        __content.string(2104),
        __content.string(2119),
        __content.string(2130),
        "STOCK OFFER PICKED",
        __content.string(13589),
    ] {
        assert!(h.iter().any(|t| t.starts_with(want)), "branch not hit: {want}");
    }
    assert!(h.iter().any(|t| t.contains(__content.string(2120))), "industry mismatch not hit");
    assert!(h.iter().any(|t| t.contains(__content.string(2136))), "can-only-afford not hit");
}

/// A world for OfferAssetsForSale: `ActiveEntity` with 0, 1 or several stock holdings, some business
/// assets and possibly pending offers of its own.
fn offer_world(seed: u64) -> Globals {
    let mut g = offers_world(seed);
    let mut r = Rand(seed * 97 + 11);
    let active = g.active_entity;
    match seed % 4 {
        0 => {
            for s in 1..=15 {
                g.hold_stock_id[(s, active)] = 0;
                g.hold_stock_pct[(s, active)] = 0;
            }
            g.hold_stock_id[(0, active)] = 0;
        }
        1 => {
            for s in 1..=15 {
                g.hold_stock_id[(s, active)] = 0;
                g.hold_stock_pct[(s, active)] = 0;
            }
            hold(&mut g, active, cast::<i32, _>(r.range(131, 1600)), cast::<i16, _>(r.range(1, 99)));
        }
        _ => {
            for _ in 0..r.range(2, 6) {
                hold(&mut g, active, cast::<i32, _>(r.range(131, 1600)), cast::<i16, _>(r.range(1, 99)));
            }
        }
    }
    if active > 130 {
        g.business_assets[active] = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(12.0)) } else { f64r(&mut r, Ext::from_f64(10.0), Ext::from_f64(3000.0)) };
    }
    // own pending offers (duplicates)
    let n = g.asset_sale_offers[0].to_display().trim().parse::<i32>().unwrap_or(0);
    if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) && n < 100 {
        let k = n + 1;
        let held: Vec<i32> = (1..=15).map(|s| cast::<i32, _>(g.hold_stock_id[(s, active)])).filter(|&x| x != 0).collect();
        g.asset_sale_offers[k] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) || held.is_empty() {
            st(&format!("{active}|CA|50|S|4|{}", g.cur_year + 1))
        } else {
            st(&format!("{active}|{}|5|S|4|{}", held[cast::<usize, _>(r.range(0, cast::<i64, _>(held.len()) - 1))], g.cur_year + 1))
        };
        g.asset_sale_offers[0] = st(&k.to_string());
    }
    if seed.is_multiple_of(29) {
        for k in 1..=100 {
            g.asset_sale_offers[k] = st(&format!("{}|CA|20|S|4|{}", r.range(131, 1580), g.cur_year + 1));
        }
        g.asset_sale_offers[0] = st("100");
    }
    g
}

#[test]
fn diff_offer_assets_for_sale() {
    let __content = wsr_core::content::current();
    let hits = Mutex::new(Vec::new());
    par_seeds(1..=200, |seed| {
        let g = offer_world(seed);
        let mut r = Rand(seed * 53 + 1);
        let offer_type = if seed.is_multiple_of(2) { 1 } else { 2 };
        let mut answers = Vec::new();
        if r.chance(cast::<f64, _>(Ext::from_f64(0.8))) {
            answers.push("IDYES".to_string());
        }
        answers.push(format!("pick:{}", r.range(0, 10)));
        if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
            answers.push("IDYES".to_string());
        }
        for _ in 0..2 {
            answers.push(match r.range(0, 4) {
                0 => r.range(1, 30).to_string(),
                1 => r.range(30, 120).to_string(),
                2 => r.range(100, 4000).to_string(),
                _ => "-5".to_string(),
            });
        }
        for _ in 0..4 {
            answers.push(match r.range(0, 5) {
                0 | 1 => "IDYES".to_string(),
                2 => "IDNO".to_string(),
                3 => "IDCANCEL".to_string(),
                _ => r.range(1, 60).to_string(),
            });
        }
        let args = vec![offer_type.to_string()];
        check(seed, OFFER, &args, &g, &answers, |g, ui, rng| offer_assets_for_sale(g, ui, rng, offer_type), &hits);
    });
    let h = hits.into_inner().unwrap();
    for want in [
        __content.string(2214),
        __content.string(2152),
        __content.string(2162),
        __content.string(2157),
        __content.string(2208),
        __content.string(13590),
        __content.string(13591),
        __content.string(2167),
        "STOCK PICKED AND OFFERED",
    ] {
        assert!(h.iter().any(|t| t.starts_with(want)), "branch not hit: {want}");
    }
    assert!(h.iter().any(|t| t.contains(__content.string(13592))), "stock duplicate not hit");
}
