//! Differential tests of `logic::trading::buy_menu`: BuyStockMenu (0x58d471) with its GOSUBs.
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use super::script::*;
use wsr_core::logic::newgame::LocalTime;
use wsr_core::logic::trading::buy_menu::buy_stock_menu;
use wsr_core::state::Globals;

const BUY_STOCK_MENU: u32 = 0x58d471;
const SKIPS: &[(u32, u32)] = &[];

struct Case {
    g: Globals,
    answers: Vec<String>,
}

/// The symbol-dialog token for company `t`.
fn sym(g: &Globals, t: i32) -> String {
    format!("sym:{}", g.ent_symbol[t].to_display())
}

fn yes(n: usize) -> Vec<String> {
    vec!["IDYES".to_string(); n]
}

/// A purchase scenario of kind `seed % 10` in a random world.
fn case(seed: u64) -> Case {
    let __content = wsr_core::content::current();
    let mut g = world(seed + 11_000);
    let mut r = Rand(seed * 6_700_417 + 1);
    let kind = seed % 10;
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    g.cur_player_name = st(&format!("Player Number {}", g.cur_player));
    let cp = g.cur_player;
    g.is_computer_player[cp] = 0;
    // the acting entity: the player or a company he controls
    let ae = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { cp } else { pick_active(&mut r, &g, 131, 1580, &[]) };
    g.active_entity = ae;
    g.active_entity_name = g.ent_name[ae].clone();
    if ae > 10 {
        g.corp_controller[ae] = cp;
    }
    g.sel_entity = 0;
    g.last_sel_entity = 0;
    g.valuation_mode = 1;
    g.currency_code = st(if r.chance(cast::<f64, _>(Ext::from_f64(0.8))) { "USA" } else { "GBR" });
    let mut target = pick_active(&mut r, &g, 131, 1580, &[ae]);
    clear_holders(&mut g, target);
    // enough money, mostly
    if r.chance(cast::<f64, _>(Ext::from_f64(0.85))) {
        g.cash[ae] = g.market_cap[target] * f64r(&mut r, Ext::from_f64(20.0), Ext::from_f64(200.0));
    }
    let mut answers = Vec::new();
    match kind {
        // open market purchase of a few percent
        0 => {
            answers.push(sym(&g, target));
            answers.push(r.range(1, 4).to_string());
            answers.extend(yes(4));
        }
        // a tender offer (over 5% of the group's stake): SEC notice, maybe financing
        1 => {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
                give_holding(&mut g, ae, target, cast::<i16, _>(r.range(1, 20)), f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(200.0)));
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                g.cash[ae] = g.market_cap[target] * f64r(&mut r, Ext::from_f64(2.0), Ext::from_f64(15.0));
                g.t_bills[ae] = Ext::from_f64(0.0);
            }
            answers.push(sym(&g, target));
            answers.push("IDOK".to_string());
            answers.push(r.range(6, 40).to_string());
            answers.extend(yes(5));
        }
        // from a company the player controls (related party), maybe at a disallowed loss
        2 => {
            let h = pick_active(&mut r, &g, 131, 1580, &[target, ae]);
            g.corp_controller[h] = cp;
            let pct = cast::<i16, _>(r.range(5, 60));
            let loss = r.chance(cast::<f64, _>(Ext::from_f64(0.4)));
            let basis = if loss { g.market_cap[target] * cast::<Ext, _>(pct) * f64r(&mut r, Ext::from_f64(1.2), Ext::from_f64(3.0)) } else { f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(50.0)) };
            give_holding(&mut g, h, target, pct, basis);
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                // all of the stock is privately held
                let other = pick_active(&mut r, &g, 131, 1580, &[target, ae, h]);
                give_holding(&mut g, other, target, 100 - pct, Ext::from_f64(10.0));
            }
            answers.push(sym(&g, target));
            answers.push("IDYES".to_string());
            if loss {
                answers.push("IDYES".to_string());
            }
            answers.push(r.range(1, cast::<i64, _>(pct) + 3).to_string());
            answers.extend(yes(4));
        }
        // from an uncontrolled company: planned premium, negotiated buyout, legal fees
        3 => {
            let h = pick_active(&mut r, &g, 131, 1580, &[target, ae]);
            g.corp_controller[h] = 0;
            let pct = cast::<i16, _>(r.range(10, 70));
            give_holding(&mut g, h, target, pct, f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(500.0)));
            answers.push(sym(&g, target));
            answers.push("IDYES".to_string());
            answers.push(["5", "15", "30", "150", "60"][cast::<usize, _>(r.range(0, 4))].to_string());
            answers.push("20".to_string());
            answers.push(r.range(1, cast::<i64, _>(pct)).to_string());
            answers.push(["IDYES", "IDNO"][cast::<usize, _>(r.range(0, 1))].to_string());
            answers.push(["10", "25", "40", "80"][cast::<usize, _>(r.range(0, 3))].to_string());
            answers.extend(yes(2));
            answers.push(r.range(1, 10).to_string());
            answers.extend(yes(3));
        }
        // a posted sale offer (TmpExt1 = offer index)
        4 => {
            let seller = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
                cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)))
            } else {
                pick_active(&mut r, &g, 131, 1580, &[target, ae])
            };
            if seller > 10 {
                g.corp_controller[seller] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { cp } else { 0 };
            }
            let pct = cast::<i16, _>(r.range(5, 40));
            if r.chance(cast::<f64, _>(Ext::from_f64(0.85))) {
                give_holding(&mut g, seller, target, pct, f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(300.0)));
            }
            let offered = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { cast::<i64, _>(pct) + 10 } else { cast::<i64, _>(pct) };
            let k = cast::<i32, _>(r.range(1, 5));
            for s in 1..=5 {
                g.asset_sale_offers[s] = st(&format!("{seller}|{target}|{offered}{__wsr_text_13588}{}|{}", g.cur_quarter, g.cur_year,__wsr_text_13588=__content.string(13588)));
            }
            g.asset_sale_offers[0] = st("5");
            if r.chance(cast::<f64, _>(Ext::from_f64(0.15))) {
                target = ae;
                g.asset_sale_offers[k] = st(&format!("{seller}|{ae}|{offered}|S|1|1"));
            }
            g.tmp_ext1 = cast::<Ext, _>(k);
            answers.push("IDYES".to_string());
            answers.push(r.range(1, offered + 2).to_string());
            answers.extend(yes(4));
        }
        // an ETF the player advises buys
        5 => {
            let etf = cast::<i32, _>(r.range(1586, 1600));
            g.entity_status[etf] = 1;
            g.industry_num[etf] = 71;
            let adviser = pick_active(&mut r, &g, 131, 1580, &[target, ae]);
            g.industry_num[adviser] = 37;
            g.corp_controller[adviser] = cp;
            g.fund_adviser[etf] = adviser;
            g.sel_entity = etf;
            g.cash[etf] = g.market_cap[target] * f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(40.0));
            g.fund_bonus_year[etf] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { g.cur_year - 1 } else { 0 };
            for s in 0..=15 {
                g.hold_stock_id[(s, etf)] = 0;
                g.hold_stock_pct[(s, etf)] = 0;
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                give_holding(&mut g, etf, target, cast::<i16, _>(r.range(1, 12)), Ext::from_f64(10.0));
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                // a target in the fund's sector: holding companies always qualify
                g.industry_num[target] = 70;
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
                let other = pick_active(&mut r, &g, 131, 1580, &[target]);
                give_holding(&mut g, other, target, cast::<i16, _>(r.range(90, 100)), Ext::from_f64(5.0));
            }
            answers.push(["IDYES", "IDYES", "IDNO", "IDCANCEL"][cast::<usize, _>(r.range(0, 3))].to_string());
            answers.push(sym(&g, target));
            answers.push(r.range(1, 12).to_string());
            answers.extend(yes(3));
        }
        // bank regulators, merger insider, [runtime resource wording] , banking concentration
        6 => {
            match r.range(0, 3) {
                0 => {
                    let b = pick_active(&mut r, &g, 11, 85, &[]);
                    g.active_entity = b;
                    g.active_entity_name = g.ent_name[b].clone();
                    g.corp_controller[b] = cp;
                    g.cash[b] = g.market_cap[target] * Ext::from_f64(50.0);
                    if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                        // no stocks held (else the bank regulators stop it first)
                        for s in 0..=15 {
                            g.hold_stock_id[(s, b)] = 0;
                            g.hold_stock_pct[(s, b)] = 0;
                        }
                        g.bank_merger_player = -cp;
                        target = pick_active(&mut r, &g, 11, 85, &[b]);
                        clear_holders(&mut g, target);
                    }
                }
                1 => {
                    g.merger_target = target;
                    g.merger_acquirer = pick_active(&mut r, &g, 131, 1580, &[target]);
                    give_holding(&mut g, ae, target, cast::<i16, _>(r.range(2, 8)), Ext::from_f64(10.0));
                }
                2 => {
                    // (a buyback of the own stock: see diff_buy_stock_menu_own_stock)
                    g.broker_fall_warn_stock = target;
                }
                _ => {
                    g.broker_fall_warn_stock = 0;
                }
            }
            answers.push(sym(&g, target));
            answers.push(r.range(1, 5).to_string());
            answers.extend(yes(5));
        }
        // several holders: pick list of sellers
        7 => {
            for k in 0..r.range(2, 4) {
                let h = pick_active(&mut r, &g, 131, 1580, &[target, ae]);
                g.corp_controller[h] = if k < 2 || r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { cp } else { 0 };
                give_holding(&mut g, h, target, cast::<i16, _>(r.range(3, 20)), f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(50.0)));
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) && ae > 10 {
                give_holding(&mut g, cp, target, cast::<i16, _>(r.range(1, 5)), Ext::from_f64(5.0));
            }
            // (no broker warning: the answers must line up; a `pick:` token the pick list does not
            // take would block every later INPUTBOX of the scripted Ui)
            g.broker_fall_warn_stock = target;
            answers.push(sym(&g, target));
            let a = ["IDYES", "IDYES", "IDNO", "IDCANCEL"][cast::<usize, _>(r.range(0, 3))];
            answers.push(a.to_string());
            if a == "IDYES" {
                answers.push(format!("pick:{}", r.range(0, 3)));
            }
            answers.push("20".to_string());
            answers.push(r.range(1, 15).to_string());
            answers.extend(yes(3));
            answers.push("25".to_string());
            answers.extend(yes(3));
        }
        // short positions, Japanese targets, an ETF share, no money, cross holdings
        8 => {
            match r.range(0, 5) {
                0 => {
                    // the player is short the target (as an individual)
                    let s = (1..=15).find(|&s| g.hold_stock_id[(s, cp)] == 0).unwrap_or(15);
                    g.hold_stock_id[(s, cp)] = cast::<i16, _>(target);
                    g.hold_stock_pct[(s, cp)] = -(cast::<i16, _>(r.range(1, 10)));
                }
                1 => {
                    g.country_idx[target] = 26;
                    let vc = pick_active(&mut r, &g, 131, 1580, &[target, ae]);
                    g.country_idx[vc] = 26;
                    give_holding(&mut g, vc, target, cast::<i16, _>(r.range(30, 60)), Ext::from_f64(10.0));
                    if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                        g.cheat_mode = st("Y");
                        g.difficulty_level = 3;
                    }
                }
                2 => {
                    target = cast::<i32, _>(r.range(1581, 1583));
                    g.entity_status[target] = 1;
                    clear_holders(&mut g, target);
                    if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                        give_holding(&mut g, cp, target, cast::<i16, _>(r.range(5, 15)), Ext::from_f64(10.0));
                    }
                }
                3 => {
                    g.cash[ae] = Ext::from_f64(0.0);
                    g.t_bills[ae] = Ext::from_f64(0.0);
                }
                4 if ae > 10 => {
                    g.corp_controller[target] = cp;
                    give_holding(&mut g, target, ae, cast::<i16, _>(r.range(5, 40)), Ext::from_f64(10.0));
                }
                _ => {}
            }
            answers.push(sym(&g, target));
            answers.extend(yes(1));
            answers.push(r.range(1, 25).to_string());
            answers.extend(yes(5));
        }
        // insider conflict: the player is short a company controlled by the target's controller
        _ => {
            let ctl = pick_active(&mut r, &g, 131, 1580, &[target, ae]);
            g.corp_controller[target] = ctl;
            let short = pick_active(&mut r, &g, 131, 1580, &[target, ae, ctl]);
            g.corp_controller[short] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { ctl } else { target };
            let s = (1..=15).find(|&s| g.hold_stock_id[(s, cp)] == 0).unwrap_or(15);
            g.hold_stock_id[(s, cp)] = cast::<i16, _>(short);
            g.hold_stock_pct[(s, cp)] = -(cast::<i16, _>(r.range(1, 10)));
            give_holding(&mut g, ctl, target, cast::<i16, _>(r.range(20, 45)), Ext::from_f64(10.0));
            give_holding(&mut g, ae, target, cast::<i16, _>(r.range(1, 10)), Ext::from_f64(10.0));
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                give_holding(&mut g, target, short, cast::<i16, _>(r.range(15, 40)), Ext::from_f64(10.0));
            }
            if g.hold_stock_id[(0, target)] == 0 {
                let other = pick_active(&mut r, &g, 131, 1580, &[target]);
                give_holding(&mut g, target, other, 3, Ext::from_f64(1.0));
            }
            answers.push(sym(&g, target));
            answers.push(["IDYES", "IDNO"][cast::<usize, _>(r.range(0, 1))].to_string());
            answers.extend(yes(1));
            answers.push(r.range(1, 10).to_string());
            answers.extend(yes(4));
        }
    }
    // A negative entry ends the percentage loop (it re-asks while the entry exceeds the float, which
    // is negative in some random worlds); the original would wait for the player forever.
    answers.extend(vec!["-999".to_string(); 3]);
    if kind != 5 && r.chance(cast::<f64, _>(Ext::from_f64(0.15))) {
        g.sel_entity = target;
    }
    Case { g, answers }
}

#[test]
fn diff_buy_stock_menu() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(seeds(1..=300), |seed| {
        let c = case(seed);
        if std::env::var("WSR_DEBUG").is_ok() {
            let mut pg = c.g.clone();
            let mut rng = wsr_core::pb::rng::Rng::from_state(rng_of(seed));
            let mut ui = Script { q: c.answers.iter().cloned().collect(), ..Script::default() };
            buy_stock_menu(&mut pg, &mut ui, &mut rng, &LocalTime::HARNESS);
            eprintln!("seed {seed} port msgs {:?}", ui.msgs);
        }
        let cmp = cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-7));
        let Some((ui, pg, _)) = check(seed, BUY_STOCK_MENU, &[], &c.g, &c.answers, SKIPS, cmp, |g, ui, rng| {
            buy_stock_menu(g, ui, rng, &LocalTime::HARNESS)
        }) else {
            return;
        };
        hits.record(&ui);
        for rep in &ui.reports {
            hits.add(rep);
        }
        let ae = pg.active_entity;
        if (1..=15).any(|s| pg.hold_stock_pct[(s, ae)] != c.g.hold_stock_pct[(s, ae)]) {
            hits.add("<bought>");
        }
        if (1581..=1600).any(|e| (1..=15).any(|s| pg.hold_stock_pct[(s, e)] != c.g.hold_stock_pct[(s, e)])) {
            hits.add("<etf bought>");
        }
    });
    hits.assert(&[
        "<bought>",
        "<etf bought>",
        __content.string(14265),
        __content.string(11914),
        __content.string(11947),
        __content.string(14266),
        __content.string(14267),
        __content.string(14268),
        __content.string(12042),
        __content.string(14269),
        __content.string(14270),
        __content.string(14271),
        __content.string(14272),
        __content.string(14273),
        __content.string(14274),
        __content.string(11939),
        __content.string(14275),
        __content.string(12006),
        __content.string(14276),
        __content.string(12172),
        __content.string(14277),
        __content.string(14278),
        "offered",
    ]);
}

/// The acting company buys its own stock: BuyStockMenu hands over to StockBuybackMenu, an LBO (mode 2,
/// a tender offer to the public through LboTenderOffer) or, without a public float, greenmail (mode 1).
#[test]
fn diff_buy_stock_menu_own_stock() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(seeds(1..=60), |seed| {
        let mut g = world(seed + 15_000);
        let mut r = Rand(seed * 7_700_417 + 3);
        g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
        g.cur_player_name = st(&format!("Player Number {}", g.cur_player));
        let cp = g.cur_player;
        g.is_computer_player[cp] = 0;
        let ae = pick_active(&mut r, &g, 131, 1580, &[]);
        g.active_entity = ae;
        g.active_entity_name = g.ent_name[ae].clone();
        // its holders: the player and a company (a public float; without one, greenmail)
        clear_holders(&mut g, ae);
        give_holding(&mut g, cp, ae, cast::<i16, _>(r.range(20, 45)), Ext::from_f64(50.0));
        if seed % 6 != 5 {
            let h = pick_active(&mut r, &g, 131, 1580, &[ae]);
            give_holding(&mut g, h, ae, cast::<i16, _>(r.range(5, 20)), Ext::from_f64(20.0));
        }
        if seed % 7 == 3 {
            let h = pick_active(&mut r, &g, 131, 1580, &[ae]);
            give_holding(&mut g, h, ae, 100 - 45 - 20, Ext::from_f64(20.0));
        }
        g.corp_controller[ae] = cp;
        g.sel_entity = 0;
        g.last_sel_entity = 0;
        g.valuation_mode = 1;
        if !seed.is_multiple_of(4) {
            g.cash[ae] = g.market_cap[ae] * f64r(&mut r, Ext::from_f64(30.0), Ext::from_f64(60.0)) + Ext::from_f64(1000.0);
            g.credit_rating_num[ae] = cast::<i32, _>(r.range(6, 10));
        }
        if seed.is_multiple_of(5) {
            g.t_bills[ae] = Ext::from_f64(0.0);
            g.cash[ae] = g.market_cap[ae] * Ext::from_f64(2.0);
        }
        let mut answers = vec![sym(&g, ae)];
        answers.push(["5", "8", "12", "3", "25"][cast::<usize, _>(r.range(0, 4))].to_string());
        answers.extend(yes(6));
        answers.extend(vec!["-999".to_string(); 3]);
        let cmp = cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-7));
        let Some((ui, pg, _)) = check(seed, BUY_STOCK_MENU, &[], &g, &answers, SKIPS, cmp, |g, ui, rng| {
            buy_stock_menu(g, ui, rng, &LocalTime::HARNESS)
        }) else {
            return;
        };
        hits.record(&ui);
        if pg.shares_outstanding[ae] < g.shares_outstanding[ae] {
            hits.add(__content.string(14279));
        }
    });
    hits.assert(&[__content.string(14280), __content.string(14279)]);
}
