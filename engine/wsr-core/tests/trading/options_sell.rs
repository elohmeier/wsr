//! Differential tests of `logic::trading::options_sell`: SellOptions (0x5e95d8) and ExerciseOptionsMenu
//! (0x600b42).
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use super::script::*;
use wsr_core::logic::trading::options_sell::*;
use wsr_core::pb::numeric::str_num;
use wsr_core::state::Globals;

const SELL_OPTIONS: u32 = 0x5e95d8;
const EXERCISE_MENU: u32 = 0x600b42;

fn str15(x: Ext) -> String {
    str_num(x, 15)
}

/// An option record `corp|type|holder|qty|strike|month|year|premium` (STR$ fields).
#[allow(clippy::too_many_arguments)]
fn rec(corp: i32, opt_type: i32, holder: i32, qty: i32, strike: Ext, month: i32, year: i32, prem: &str) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}",
        str15(Ext::from(cast::<Ext, _>(corp))).trim(),
        str15(Ext::from(cast::<Ext, _>(opt_type))),
        str15(Ext::from(cast::<Ext, _>(holder))),
        str15(Ext::from(cast::<Ext, _>(qty))),
        str15(strike),
        str15(Ext::from(cast::<Ext, _>(month))),
        str15(Ext::from(cast::<Ext, _>(year))),
        prem
    )
}

fn add_rec(g: &mut Globals, text: &str) {
    if let Some(k) = (1..=1000).find(|&k| g.option_recs[k].is_empty()) {
        g.option_recs[k] = st(text);
    }
    let n = (1..=1000).filter(|&k| !g.option_recs[k].is_empty()).count();
    g.option_recs[0] = st(&n.to_string());
}

/// A stock with a unique symbol and a share price of 5..1000 (or outside for `odd_price`).
fn stock(g: &mut Globals, r: &mut Rand, not: &[i32], odd_price: bool) -> i32 {
    let c = pick_active(r, g, 131, 1580, not);
    g.ent_symbol[c] = st(&format!("ZQ{c}"));
    g.shares_outstanding[c] = f64r(r, Ext::from_f64(2.0), Ext::from_f64(40.0));
    let price = if odd_price { [f64r(r, Ext::from_f64(0.5), Ext::from_f64(4.5)), f64r(r, Ext::from_f64(1100.0), Ext::from_f64(3000.0))][cast::<usize, _>(r.range(0, 1))] } else { f64r(r, Ext::from_f64(6.0), Ext::from_f64(400.0)) };
    g.market_cap[c] = price * g.shares_outstanding[c];
    g.credit_rating_num[c] = cast::<i32, _>(r.range(2, 10));
    c
}

/// The acting entity: the current player or a company he controls.
fn acting(g: &mut Globals, r: &mut Rand, corp_seller: bool, lo: i64, hi: i64) -> i32 {
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    g.cur_player_name = st(&format!("Player Number {}", g.cur_player));
    g.is_computer_player[g.cur_player] = 0;
    let ae = if corp_seller { pick_active(r, g, lo, hi, &[]) } else { g.cur_player };
    if ae > 10 {
        g.corp_controller[ae] = g.cur_player;
    }
    g.active_entity = ae;
    g.active_entity_name = g.ent_name[ae].clone();
    g.sel_entity = ae;
    ae
}

fn yes_no(r: &mut Rand, p_yes: Ext) -> String {
    if r.chance(cast::<f64, _>(p_yes)) { "IDYES" } else { "IDNO" }.to_string()
}

struct Case {
    g: Globals,
    opt_type: i32,
    answers: Vec<String>,
}

/// SellOptions case of kind `seed % 7`.
fn sell_case(seed: u64) -> Case {
    let mut g = world(seed + 11_300);
    let mut r = Rand(seed * 7_177 + 41);
    let kind = seed % 7;
    let opt_type = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { 1 } else { 2 };
    g.cur_quarter = cast::<i32, _>(r.range(1, 4));
    let corp_seller = match kind {
        3 => true,
        _ => r.chance(cast::<f64, _>(Ext::from_f64(0.4))),
    };
    let (lo, hi) = if kind == 3 { (11, 130) } else { (131, 1580) };
    let ae = acting(&mut g, &mut r, corp_seller, lo, hi);
    let odd = kind == 4 && r.chance(cast::<f64, _>(Ext::from_f64(0.6)));
    let corp = stock(&mut g, &mut r, &[ae], odd);
    // a few computer-run companies able to take the other side
    for _ in 0..6 {
        let c = pick_active(&mut r, &g, 131, 1580, &[corp, ae]);
        g.credit_rating_num[c] = 9;
        g.corp_controller[c] = 0;
        g.cash[c] += Ext::from_f64(5000.0);
    }
    let mut answers = Vec::new();
    match kind {
        // closing sales of long options (sometimes several, GRANT / VESTED, the seller's own stock)
        0..=2 => {
            let target = if kind == 2 && ae > 10 && r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { ae } else { corp };
            if target == ae {
                g.ent_symbol[ae] = st(&format!("ZQ{ae}"));
                g.shares_outstanding[ae] = f64r(&mut r, Ext::from_f64(2.0), Ext::from_f64(40.0));
                g.market_cap[ae] = f64r(&mut r, Ext::from_f64(6.0), Ext::from_f64(400.0)) * g.shares_outstanding[ae];
            }
            // far out of the money: no bid (the broker's "mercy sale")
            let mercy = kind == 2 && r.chance(cast::<f64, _>(Ext::from_f64(0.5)));
            let n = if r.chance(cast::<f64, _>(Ext::from_f64(0.35))) { 2 } else { 1 };
            for _ in 0..n {
                let qty = cast::<i32, _>(r.range(1, 10));
                let moneyness = match (mercy, opt_type) {
                    (true, 1) => f64r(&mut r, Ext::from_f64(4.0), Ext::from_f64(8.0)),
                    (true, _) => f64r(&mut r, Ext::from_f64(0.02), Ext::from_f64(0.1)),
                    _ => f64r(&mut r, Ext::from_f64(0.6), Ext::from_f64(1.4)),
                };
                let strike = (g.market_cap[target] * moneyness * Ext::from_f64(100.0)).round() / Ext::from_f64(100.0);
                let month = cast::<i32, _>(r.range(1, 12));
                let year = g.cur_year + cast::<i32, _>(r.range(0, 1));
                let prem = match r.range(0, 9) {
                    0 if ae <= 10 => "GRANT".to_string(),
                    1 if ae <= 10 => "VESTED".to_string(),
                    2 => str15(-f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(50.0))),
                    _ => str15((g.market_cap[target] * f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(0.2)) * cast::<Ext, _>(qty) * Ext::from_f64(100.0)).round() / Ext::from_f64(100.0)),
                };
                {
                    let t = rec(target, opt_type, ae, qty, strike, month, year, &prem);
                    add_rec(&mut g, &t);
                }
            }
            if ae <= 10 && r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                g.player_ceo_of[ae] = target;
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                // a written option of someone else on the stock
                let w = pick_active(&mut r, &g, 131, 1580, &[ae, target]);
                {
                    let t = rec(target, opt_type, w, -3, g.market_cap[target], 6, g.cur_year + 1, "-5");
                    add_rec(&mut g, &t);
                }
            }
            answers.push(format!("sym:ZQ{target}"));
            answers.push(yes_no(&mut r, Ext::from_f64(0.85)));
            if n > 1 {
                answers.push(format!("pick:{}", r.range(0, 1)));
            }
            if mercy {
                answers.push(yes_no(&mut r, Ext::from_f64(0.8))); // the mercy sale
            }
            answers.push(format!("{}", r.range(1, 11)));
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                answers.push(format!("{}", r.range(1, 5))); // a retry after too many percent
            }
            answers.push(yes_no(&mut r, Ext::from_f64(0.85)));
            answers.push("IDYES".to_string());
        }
        // banks and insurers (kind 3), the new-game suspension, odd prices and the rule checks
        _ => {
            if kind == 5 && r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                g.start_year = g.cur_year;
                g.cur_quarter = 1;
                g.sim_co_count = cast::<i32, _>(r.range(10, 502));
            }
            match if kind == 6 { 1 } else { r.range(0, 6) } {
                0 => g.credit_rating_num[corp] = [0, 1][cast::<usize, _>(r.range(0, 1))],
                1 => {
                    // a weak seller (its rating is recomputed from these)
                    g.bank_debt[ae] = f64r(&mut r, Ext::from_f64(1.0e4), Ext::from_f64(1.0e6));
                    g.cash[ae] = f64r(&mut r, -Ext::from_f64(100.0), Ext::from_f64(50.0));
                    g.t_bills[ae] = Ext::from_f64(0.0);
                    if ae <= 10 && r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                        give_holding(&mut g, ae, corp, cast::<i16, _>(r.range(1, 10)), Ext::from_f64(10.0));
                    }
                }
                2 => {
                    g.merger_target = corp;
                    g.merger_acquirer = pick_active(&mut r, &g, 131, 1580, &[corp, ae]);
                    give_holding(&mut g, ae, corp, cast::<i16, _>(r.range(5, 30)), Ext::from_f64(10.0));
                }
                3 => {
                    give_holding(&mut g, ae, corp, cast::<i16, _>(r.range(1, 30)), Ext::from_f64(10.0));
                    g.corp_controller[corp] = g.cur_player;
                }
                4 => {
                    // written options on the stock up to the exchange limit
                    for _ in 0..[4, 10, 11][cast::<usize, _>(r.range(0, 2))] {
                        let w = pick_active(&mut r, &g, 131, 1580, &[ae, corp]);
                        {
                    let t = rec(corp, opt_type, w, -10, g.market_cap[corp], 9, g.cur_year + 1, "-5");
                    add_rec(&mut g, &t);
                }
                    }
                }
                _ => {
                    clear_holders(&mut g, corp);
                    if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                        let h = pick_active(&mut r, &g, 131, 1580, &[corp]);
                        give_holding(&mut g, h, corp, 100, Ext::from_f64(10.0));
                    }
                }
            }
            if kind == 6 && r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                answers.push("sym:".to_string());
            } else {
                answers.push(format!("sym:ZQ{corp}"));
            }
            if kind == 3 && r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                answers.push(yes_no(&mut r, Ext::from_f64(0.7)));
            }
            let price = g.market_cap[corp] / g.shares_outstanding[corp];
            answers.push(format!("{}", (price * f64r(&mut r, Ext::from(Ext::from_f64(0.6)), Ext::from(Ext::from_f64(1.4)))).round().max(Ext::from(Ext::from_f64(1.0)))));
            if r.chance(cast::<f64, _>(Ext::from_f64(0.15))) {
                // no month picked ("No option selected": YES = cancel; the original's chain dialog only
                // closes with its default button when no dialog action is left)
                answers.push("6".to_string());
            } else {
                answers.push(format!("pick:{}", r.range(0, 23)));
                answers.push(format!("{}", r.range(1, 12)));
                if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                    answers.push(format!("{}", r.range(1, 5))); // a retry after too many percent
                }
                answers.push(["6", "6", "6", "7"][cast::<usize, _>(r.range(0, 3))].to_string());
                answers.push("6".to_string());
            }
        }
    }
    Case { g, opt_type, answers }
}

#[test]
fn diff_sell_options() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(seeds(1..=210), |seed| {
        let c = sell_case(seed);
        if std::env::var("WSR_DEBUG").is_ok() {
            let ae = c.g.active_entity;
            eprintln!("seed {seed}: seller {ae}{__wsr_text_13737}{} ind {}{__wsr_text_13848}{}", c.opt_type, c.g.industry_num[ae], c.g.bank_debt[ae],__wsr_text_13737=__content.string(13737), __wsr_text_13848=__content.string(13848));
        }
        let cmp = cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-7));
        // the harness clock is 10:00:00: TIME$ seconds = 0
        let Some((ui, pg, _)) = check(seed, SELL_OPTIONS, &[c.opt_type.to_string()], &c.g, &c.answers, &[], cmp, |g, ui, rng| {
            sell_options(g, ui, rng, c.opt_type, 0)
        }) else {
            return;
        };
        hits.record(&ui);
        if (0..=1000).any(|k| pg.option_recs[k] != c.g.option_recs[k]) {
            hits.add("<records changed>");
        }
    });
    hits.assert(&[
        "<records changed>",
        __content.string(14297),
        __content.string(12367),
        __content.string(14302),
        __content.string(12648),
        __content.string(12675),
        __content.string(14303),
        __content.string(14304),
        __content.string(14305),
        __content.string(14306),
        __content.string(14298),
        __content.string(14242),
        __content.string(14307),
        __content.string(11731),
        __content.string(12620),
        __content.string(14308),
        __content.string(14309),
        __content.string(14294),
        __content.string(14295),
        __content.string(14310),
        __content.string(14311),
        __content.string(14236),
        __content.string(14312),
        __content.string(14300),
        __content.string(14313),
        __content.string(14243),
        __content.string(14314),
    ]);
}

struct ExCase {
    g: Globals,
    opt_type: i32,
    answers: Vec<String>,
}

fn exercise_case(seed: u64) -> ExCase {
    let mut g = world(seed + 11_700);
    let mut r = Rand(seed * 9_133 + 7);
    let opt_type = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { 1 } else { 2 };
    let cs = r.chance(cast::<f64, _>(Ext::from_f64(0.4)));
    let ae = acting(&mut g, &mut r, cs, 131, 1580);
    let corp = stock(&mut g, &mut r, &[ae], false);
    clear_holders(&mut g, corp);
    let h = pick_active(&mut r, &g, 131, 1580, &[corp, ae]);
    let p = cast::<i16, _>(r.range(10, 60));
    give_holding(&mut g, h, corp, p, Ext::from_f64(10.0));
    if opt_type == 2 {
        let p = cast::<i16, _>(r.range(5, 40));
        let b = g.market_cap[corp] * Ext::from_f64(10.0);
        give_holding(&mut g, ae, corp, p, b);
    }
    g.cash[ae] = g.market_cap[corp] * f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(60.0));
    let n = match seed % 5 {
        0 => 0,
        1 => 2,
        _ => 1,
    };
    for _ in 0..n {
        let qty = cast::<i32, _>(r.range(1, 10));
        let itm = r.chance(cast::<f64, _>(Ext::from_f64(0.75)));
        let m = if itm == (opt_type == 1) { f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(0.95)) } else { f64r(&mut r, Ext::from_f64(1.05), Ext::from_f64(1.5)) };
        let strike = (g.market_cap[corp] * m * Ext::from_f64(100.0)).round() / Ext::from_f64(100.0);
        let prem = if ae <= 10 && r.chance(cast::<f64, _>(Ext::from_f64(0.15))) { "GRANT".to_string() } else { str15(f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(40.0))) };
        {
                    let t = rec(corp, opt_type, ae, qty, strike, cast::<i32, _>(r.range(1, 12)), g.cur_year + 1, &prem);
                    add_rec(&mut g, &t);
                }
    }
    if seed.is_multiple_of(5) && r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
        // a long option on another stock only
        let other = stock(&mut g, &mut r, &[ae, corp], false);
        {
                    let t = rec(other, opt_type, ae, 3, g.market_cap[other], 9, g.cur_year + 1, "5");
                    add_rec(&mut g, &t);
                }
    }
    let answers = vec![
        format!("sym:ZQ{corp}"),
        format!("pick:{}", r.range(0, 1)),
        format!("{}", r.range(0, 11)),
        format!("{}", r.range(1, 5)),
        yes_no(&mut r, Ext::from(Ext::from_f64(0.8))),
        yes_no(&mut r, Ext::from(Ext::from_f64(0.8))),
        yes_no(&mut r, Ext::from(Ext::from_f64(0.8))),
    ];
    ExCase { g, opt_type, answers }
}

#[test]
fn diff_exercise_options_menu() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(seeds(1..=120), |seed| {
        let c = exercise_case(seed);
        let cmp = cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-7));
        let Some((ui, pg, _)) = check(seed, EXERCISE_MENU, &[c.opt_type.to_string()], &c.g, &c.answers, &[], cmp, |g, ui, rng| {
            exercise_options_menu(g, ui, rng, c.opt_type)
        }) else {
            return;
        };
        hits.record(&ui);
        if (0..=1000).any(|k| pg.option_recs[k] != c.g.option_recs[k]) {
            hits.add("<records changed>");
        }
    });
    hits.assert(&[
        "<records changed>",
        __content.string(14315),
        __content.string(14316),
        __content.string(14317),
        __content.string(14318),
        __content.string(14319),
    ]);
}
