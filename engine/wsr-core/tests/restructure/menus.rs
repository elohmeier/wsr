//! Differential tests of `logic::restructure::menus`: the player menus StockForStockMerger and
//! LiquidateSubsidiary against the original (the symbol-entry dialog of `InBox(3)` is driven with
//! `text:310:<id>` + OK; the port gets the same text through `Ui::inputbox`).
#![allow(clippy::manual_is_multiple_of)]
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use super::tender::{Hits, Scr, check, give_stake, rng0};
use wsr_core::logic::newgame::LocalTime;
use wsr_core::logic::restructure::menus::*;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

const STOCK_FOR_STOCK_MERGER: u32 = 0x5a8815;
const LIQUIDATE_SUBSIDIARY: u32 = 0x5a7387;
/// Clears all holdings of `corp` by others (then `holders` get theirs).
fn clear_holders(g: &mut Globals, corp: i32) {
    for h in 1..=1600 {
        for slot in 1..=15 {
            if cast::<i32, _>(g.hold_stock_id[(slot, h)]) == corp {
                g.hold_stock_id[(slot, h)] = 0;
                g.hold_stock_pct[(slot, h)] = 0;
                g.hold_stock_basis[(slot, h)] = Ext::from_f64(0.0);
            }
        }
        g.hold_stock_id[(0, h)] = cast::<i16, _>((1..=15).filter(|&s| g.hold_stock_id[(s, h)] != 0).count());
    }
}

/// StockForStockMerger: the acquirer is the active entity (controlled by player 1), the target is
/// entered by id; covers self-merger, funds, the bank divestment loop, the cost question, the revolt of a
/// nearly bankrupt target's holders, the fairness range for a controlled target and the premium entry.
#[test]
fn diff_stock_for_stock_merger() {
    let __content = wsr_core::content::current();
    let hits = Hits::new();
    let skips = Vec::<(u32, u32)>::new();
    par_seeds(1..=160, |seed| {
        if std::env::var("SEED").is_ok_and(|s| s != seed.to_string()) {
            return;
        }
        let mut g = world(seed + 9000);
        let mut r = Rand(seed * 41 + 7);
        g.cur_player = 1;
        g.is_computer_player[1] = 0;
        g.cheat_mode = st(if seed % 11 == 0 { "Y" } else { "N" });
        if seed % 11 == 0 {
            // cheat mode, but no scenario run (RunScenarioEvent reads story files the port is not given)
            g.scenario_corp = 1;
            g.scenario_num = 0;
            g.event_status[101] = 0;
            g.cheat_eq_fund_target = 0;
        }
        let acq = cast::<i32, _>(match seed % 6 {
            0 => r.range(11, 85),
            _ => r.range(131, 1580),
        });
        g.entity_status[acq] = 1;
        g.corp_controller[acq] = 1;
        g.active_entity = acq;
        g.active_entity_name = g.ent_name[acq].clone();
        g.sel_entity = acq;
        g.last_sel_entity = cast::<i32, _>(r.range(11, 1580));
        let mut tgt = match seed % 9 {
            0 => acq,
            1 => cast::<i32, _>(r.range(1581, 1583)),
            2 | 3 => cast::<i32, _>(r.range(11, 85)),
            _ => cast::<i32, _>(r.range(131, 1580)),
        };
        if tgt == acq && seed % 9 != 0 {
            tgt = if acq > 131 { acq - 1 } else { acq + 1 };
        }
        g.entity_status[tgt] = 1;
        if tgt > 1580 {
            g.industry_num[tgt] = 71;
            g.fund_adviser[tgt] = cast::<i32, _>(r.range(11, 130));
        }
        if seed % 4 == 1 {
            g.corp_controller[tgt] = 1;
        }
        if seed % 6 == 0 {
            // bank acquirer of the player who already merged banks: the target owns other banks
            g.bank_merger_player = 1;
            if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) {
                let (b, p) = (cast::<i32, _>(r.range(11, 85)), cast::<i16, _>(r.range(1, 30)));
                give_stake(&mut g, &mut r, tgt, b, p);
            }
        }
        if seed % 5 == 2 {
            // nearly bankrupt target
            g.cash[tgt] = -f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(5000.0));
            g.bank_debt[tgt] = f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(5000.0));
            g.credit_rating_num[tgt] = 0;
            for slot in 0..=15 {
                g.hold_stock_id[(slot, tgt)] = 0;
                g.hold_stock_pct[(slot, tgt)] = 0;
            }
            if tgt > 130 {
                g.business_assets[tgt] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(100.0));
            }
            g.t_bills[tgt] = Ext::from_f64(0.0);
        }
        if seed % 7 == 3 {
            let p = cast::<i16, _>(r.range(5, 60));
            give_stake(&mut g, &mut r, acq, tgt, p);
        }
        let id = tgt.to_string();
        let cost_ans = ["IDYES", "IDYES", "IDYES", "IDNO", "IDCANCEL"][cast::<usize, _>(r.range(0, 4))];
        let p1 = ["200", "4", "-1", "0", "12"][cast::<usize, _>(r.range(0, 4))];
        let p2 = ["30", "2", "15", "0"][cast::<usize, _>(r.range(0, 3))];
        let bank_ans = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { "IDNO" } else { "IDYES" };
        let mut rest = vec![cost_ans, p1, p2, "0"];
        if seed % 6 == 0 {
            rest.insert(0, bank_ans);
        }
        let text = format!("text:310:{id}");
        let mut orig_answers: Vec<&str> = vec![&text];
        orig_answers.extend(rest.iter().copied());
        let mut port_answers: Vec<&str> = vec![&id];
        port_answers.extend(rest.iter().copied());
        let rng_start = rng0(seed + 500);
        let sk = skips.clone();
        let Some(o) = run(STOCK_FOR_STOCK_MERGER, &[], &g, rng_start, &orig_answers, &sk) else {
            return;
        };
        let mut port = g.clone();
        let mut ui = Scr::new(&port_answers);
        let mut rng = Rng::from_state(rng_start);
        stock_for_stock_merger(&mut port, &mut ui, &mut rng, &LocalTime::HARNESS);
        check(&port, &rng, &ui, &o, &format!("seed {seed} SFS {acq} -> {tgt} {port_answers:?}"));
        for (key, part) in [
            ("self", __content.string(14058)),
            ("fund", __content.string(6736)),
            ("bank-divest", __content.string(14059)),
            ("cost", __content.string(14060)),
            ("revolt", "SHAREHOLDER"),
            ("revolt", __content.string(14061)),
            (__content.string(14062), __content.string(14062)),
            ("range-error", __content.string(14063)),
        ] {
            if ui.any_msg(part) {
                hits.hit(key);
            }
        }
        if ui.inputs > 1 {
            hits.hit("premium-asked");
        }
        if port.action_code.is_empty() && port.turn_action_count != g.turn_action_count {
            hits.hit("merged");
        }
    });
    if std::env::var("SEED").is_ok() {
        return;
    }
    hits.require(&["self", "fund", "bank-divest", "cost", "revolt", __content.string(14062), "range-error", "premium-asked"]);
}

/// LiquidateSubsidiary in both modes: tax-free into a 100% parent (same industry / holding company /
/// other), banks and insurers, a nearly bankrupt parent, the book-value and loss-carryover warnings and
/// the switch to a taxable liquidation.
#[test]
fn diff_liquidate_subsidiary() {
    let __content = wsr_core::content::current();
    let hits = Hits::new();
    let skips = Vec::<(u32, u32)>::new();
    par_seeds(1..=160, |seed| {
        if std::env::var("SEED").is_ok_and(|s| s != seed.to_string()) {
            return;
        }
        let mut g = world(seed + 12000);
        let mut r = Rand(seed * 59 + 3);
        g.cur_player = 1;
        g.is_computer_player[1] = 0;
        let sub = cast::<i32, _>(match seed % 8 {
            0 => r.range(11, 85),
            1 => r.range(86, 130),
            _ => r.range(131, 1580),
        });
        g.entity_status[sub] = 1;
        g.active_entity = sub;
        g.active_entity_name = g.ent_name[sub].clone();
        g.sel_entity = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { sub } else { cast::<i32, _>(r.range(11, 1580)) };
        clear_holders(&mut g, sub);
        let parent = match r.range(0, 4) {
            0 => cast::<i32, _>(r.range(11, 130)),
            1 => 1,
            _ => cast::<i32, _>(r.range(131, 1580)),
        };
        if parent != sub {
            g.entity_status[parent] = 1;
            match r.range(0, 4) {
                0 if (parent > 130) == (sub > 130) && (parent < 86) == (sub < 86) => g.industry_num[parent] = g.industry_num[sub],
                1 if parent > 130 => g.industry_num[parent] = 70,
                _ => {}
            }
            if sub > 130 && r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
                g.industry_num[sub] = 70;
            }
            let pct = if r.chance(cast::<f64, _>(Ext::from_f64(0.75))) { 100 } else { cast::<i16, _>(r.range(80, 99)) };
            give_stake(&mut g, &mut r, parent, sub, pct);
            if pct < 100 && r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                let h = cast::<i32, _>(r.range(131, 1580));
                give_stake(&mut g, &mut r, h, sub, 100 - pct);
            }
            g.corp_controller[sub] = parent;
            g.corp_controller[parent] = 1;
        }
        g.tax_loss_carry[sub] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { -f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(200.0)) } else { Ext::from_f64(0.0) };
        g.market_cap[sub] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { f64r(&mut r, Ext::from_f64(0.01), Ext::from_f64(20.0)) } else { f64r(&mut r, Ext::from_f64(20.0), Ext::from_f64(2000.0)) };
        if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
            g.cash[sub] = -f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(9000.0));
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
            g.cash[parent] = -f64r(&mut r, Ext::from_f64(3000.0), Ext::from_f64(20000.0));
            g.bank_debt[parent] = f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(9000.0));
        }
        g.country_idx[sub] = [0, 3, 30, 12][cast::<usize, _>(r.range(0, 3))];
        let liq_mode = if seed % 3 == 0 { 2 } else { 1 };
        let answers: Vec<&str> = (0..5).map(|_| if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { "IDNO" } else { "IDYES" }).collect();
        let rng_start = rng0(seed + 900);
        let Some(o) = run(LIQUIDATE_SUBSIDIARY, &[&liq_mode.to_string()], &g, rng_start, &answers, &skips) else {
            return;
        };
        let mut port = g.clone();
        let mut ui = Scr::new(&answers);
        let mut rng = Rng::from_state(rng_start);
        liquidate_subsidiary(&mut port, &mut ui, &mut rng, liq_mode, &LocalTime::HARNESS);
        if std::env::var("DBG").is_ok() {
            eprintln!("AE port {} orig {} sel {} {}", port.active_entity, o.globals.active_entity, port.sel_entity, o.globals.sel_entity);
        }
        check(&port, &rng, &ui, &o, &format!("seed {seed}{__wsr_text_14064}{liq_mode}) sub {sub}{__wsr_text_14037}{parent}",__wsr_text_14064=__content.string(14064), __wsr_text_14037=__content.string(14037)));
        for (key, part) in [
            ("not-permitted", __content.string(14065)),
            ("bank-refused", __content.string(14066)),
            ("parent-worth", __content.string(14067)),
            ("parent-bankrupt", __content.string(14068)),
            ("book-value", __content.string(14069)),
            ("nol", __content.string(9349)),
            (__content.string(13827), __content.string(14070)),
            (__content.string(14071), __content.string(14072)),
            ("taxable", __content.string(14073)),
        ] {
            if ui.any_msg(part) {
                hits.hit(key);
            }
        }
        if port.entity_status[sub] != 1 {
            hits.hit(__content.string(13621));
        }
    });
    if std::env::var("SEED").is_ok() {
        return;
    }
    hits.require(&["not-permitted", "bank-refused", "book-value", "nol", __content.string(13827), __content.string(14071), "taxable"]);
}
