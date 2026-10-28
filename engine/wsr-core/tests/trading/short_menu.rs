//! Differential tests of `logic::trading::short_menu`: SellShort (0x5e0ea7) and CoverShortPosition
//! (0x5e2654).
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use super::script::*;
use wsr_core::logic::trading::short_menu::*;
use wsr_core::state::Globals;

const SELL_SHORT: u32 = 0x5e0ea7;
const COVER_SHORT: u32 = 0x5e2654;

/// Makes the monthly closes of `corp` rise (`months` > 0) for the last `months.abs()` months.
fn trend(g: &mut Globals, corp: i32, months: i32) {
    let j = cast::<i32, _>(g.cap_hist_close[(0, corp)]);
    let mut v = g.market_cap[corp];
    for k in 0..=months.abs() {
        let mut slot = j - k;
        if slot < 1 {
            slot += 60;
        }
        g.cap_hist_close[(slot, corp)] = v;
        v *= if months > 0 { Ext::from_f64(0.97) } else { Ext::from_f64(1.03) };
    }
}

/// Numbers serve both as message box answers (6 = YES, 7 = NO, 2 = CANCEL) and as entered percents.
fn nums(r: &mut Rand, n: usize, pool: &[i64]) -> Vec<String> {
    (0..n).map(|_| pool[cast::<usize, _>(r.range(0, cast::<i64, _>(pool.len()) - 1))].to_string()).collect()
}

struct Case {
    g: Globals,
    answers: Vec<String>,
}

fn sell_case(seed: u64) -> Case {
    let __content = wsr_core::content::current();
    let mut g = world(seed + 9800);
    let mut r = Rand(seed * 48_271 + 5);
    let kind = seed % 8;
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    let cp = g.cur_player;
    g.cur_player_name = st(&format!("Player Number {cp}"));
    g.active_entity = cp;
    g.active_entity_name = g.player_name[cp].clone();
    g.player_name[cp] = g.active_entity_name.clone();
    g.ent_name[cp] = g.active_entity_name.clone();
    g.cash[cp] = f64r(&mut r, Ext::from_f64(2000.0), Ext::from_f64(200_000.0));
    g.player_short_margin_bal[cp] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(3000.0)) } else { Ext::from_f64(0.0) };
    g.app_title = st(__content.string(13661));
    if seed.is_multiple_of(23) {
        // a company acting: nothing happens
        g.active_entity = pick_active(&mut r, &g, 131, 1580, &[]);
    }
    let mut corp = pick_active(&mut r, &g, 131, 1580, &[]);
    clear_holders(&mut g, corp);
    for _ in 0..r.range(0, 3) {
        let h = pick_active(&mut r, &g, 11, 1580, &[corp]);
        give_holding(&mut g, h, corp, cast::<i16, _>(r.range(1, 25)), f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(100.0)));
    }
    g.credit_rating_num[corp] = cast::<i32, _>(r.range(2, 10));
    let mut pool: Vec<i64> = vec![6, 6, 6, 1, 3, 5, 12, 25];
    match kind {
        // the player controls the company (insider), maybe with a long position to sell instead
        1 => {
            clear_holders(&mut g, corp);
            give_holding(&mut g, cp, corp, cast::<i16, _>(r.range(51, 90)), f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(300.0)));
            g.corp_controller[corp] = cp;
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                // the stake is held through a company: no own long position
                let c = pick_active(&mut r, &g, 131, 1580, &[corp]);
                clear_holders(&mut g, corp);
                give_holding(&mut g, c, corp, 70, Ext::from_f64(10.0));
                g.corp_controller[c] = cp;
                g.corp_controller[corp] = c;
                clear_holders(&mut g, c);
                give_holding(&mut g, cp, c, 80, Ext::from_f64(10.0));
            }
            pool = vec![6, 7, 2];
        }
        // poor credit: bankrupt / prohibited (banks always count as nearly bankrupt)
        2 => {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                corp = pick_active(&mut r, &g, 11, 85, &[]);
                clear_holders(&mut g, corp);
            }
            g.credit_rating_num[corp] = cast::<i32, _>(r.range(0, 1));
            g.proj_qtr_earnings[corp] = f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(20.0));
            g.market_cap[corp] = g.shares_outstanding[corp] * [Ext::from_f64(1.0), Ext::from_f64(1.5), Ext::from_f64(3.0), Ext::from_f64(8.0), Ext::from_f64(40.0)][cast::<usize, _>(r.range(0, 4))];
            g.bank_debt[corp] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(6000.0)) } else { Ext::from_f64(0.0) };
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) && corp > 130 {
                // debt about equal to the assets: rated low, yet not the bankruptcy test (prohibited)
                for s in 0..=15 {
                    g.hold_stock_id[(s, corp)] = 0;
                    g.hold_stock_pct[(s, corp)] = 0;
                }
                g.bonds_issued[corp] = 0;
                g.cash[corp] = Ext::from_f64(10.0);
                g.t_bills[corp] = Ext::from_f64(0.0);
                g.working_capital[corp] = Ext::from_f64(0.0);
                g.business_assets[corp] = f64r(&mut r, Ext::from_f64(500.0), Ext::from_f64(3000.0));
                g.bank_debt[corp] = g.business_assets[corp] * f64r(&mut r, Ext::from_f64(0.95), Ext::from_f64(1.1));
            } else if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
                // deeply in debt: rated D, assets far below the debt
                g.bank_debt[corp] = f64r(&mut r, Ext::from_f64(20_000.0), Ext::from_f64(90_000.0));
                g.cash[corp] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(10.0));
                g.t_bills[corp] = Ext::from_f64(0.0);
            }
        }
        // an existing position: long, short 20%, full portfolio, or a short to add to
        3 => match r.range(0, 3) {
            0 => {
                give_holding(&mut g, cp, corp, cast::<i16, _>(r.range(1, 20)), Ext::from_f64(5.0));
            }
            1 => {
                give_holding(&mut g, cp, corp, -20, Ext::from_f64(50.0));
            }
            2 => {
                for s in 1..=15 {
                    if g.hold_stock_id[(s, cp)] == 0 {
                        g.hold_stock_id[(s, cp)] = cast::<i16, _>(pick_active(&mut r, &g, 131, 1580, &[corp]));
                        g.hold_stock_pct[(s, cp)] = 3;
                    }
                }
            }
            _ => {
                give_holding(&mut g, cp, corp, -(cast::<i16, _>(r.range(1, 12))), Ext::from_f64(30.0));
            }
        },
        // a steadily rising stock: the broker's warning
        4 => {
            trend(&mut g, corp, cast::<i32, _>(r.range(5, 9)));
            if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
                g.broker_rise_warn_stock = corp;
            }
            pool = vec![6, 6, 7, 2];
        }
        // margin: little net worth, or a big short liability already
        5 => {
            g.cash[cp] = f64r(&mut r, -Ext::from_f64(100.0), Ext::from_f64(3.0) * g.market_cap[corp]);
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                // enough for a few percent: the margin rule limits the short
                g.cash[cp] = g.market_cap[corp] * f64r(&mut r, Ext::from_f64(1.5), Ext::from_f64(8.0));
            }
            for s in 1..=15 {
                g.hold_stock_id[(s, cp)] = 0;
                g.hold_stock_pct[(s, cp)] = 0;
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                let other = pick_active(&mut r, &g, 131, 1580, &[corp]);
                give_holding(&mut g, cp, other, -15, Ext::from_f64(100.0));
                g.market_cap[other] = g.cash[cp].abs() * Ext::from_f64(2.0) + Ext::from_f64(500.0);
            }
        }
        // other players' shorts / a thin float
        6 => {
            clear_holders(&mut g, corp);
            let big = pick_active(&mut r, &g, 131, 1580, &[corp]);
            give_holding(&mut g, big, corp, cast::<i16, _>(r.range(40, 98)), Ext::from_f64(10.0));
            for p in 1..=g.num_players {
                if p != cp && r.chance(cast::<f64, _>(Ext::from_f64(0.7))) {
                    give_holding(&mut g, p, corp, -(cast::<i16, _>(r.range(1, 20))), Ext::from_f64(20.0));
                }
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
                g.player_name[g.num_players] = st("...OUT...");
            }
        }
        // a plain short sale (entries over the limit, 0 cancels)
        _ => {
            pool = vec![6, 3, 1, 99, 0, 15, 4];
        }
    }
    let sym = g.ent_symbol[corp].to_display();
    let mut answers = vec![if seed.is_multiple_of(17) { "sym:".to_string() } else { format!("sym:{sym}") }];
    answers.extend(nums(&mut r, 5, &pool));
    Case { g, answers }
}

#[test]
fn diff_sell_short() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(seeds(1..=400), |seed| {
        let c = sell_case(seed);
        let cmp = cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-7));
        let Some((ui, pg, _)) =
            check(seed, SELL_SHORT, &[], &c.g, &c.answers, &[], cmp, |g, ui, rng| sell_short(g, ui, rng, 0))
        else {
            return;
        };
        hits.record(&ui);
        if pg.short_pos_flag == 1 && c.g.short_pos_flag != 1 {
            hits.add("<shorted>");
        }
    });
    hits.assert(&[
        "<shorted>",
        __content.string(3253),
        __content.string(14327),
        __content.string(12752),
        __content.string(14328),
        __content.string(14329),
        __content.string(14330),
        __content.string(12760),
        __content.string(14331),
        __content.string(14332),
        __content.string(14333),
        __content.string(14334),
        __content.string(11972),
        __content.string(14335),
        __content.string(14336),
        __content.string(9864),
        __content.string(12599),
    ]);
}

fn cover_case(seed: u64) -> Case {
    let __content = wsr_core::content::current();
    let mut g = world(seed + 9900);
    let mut r = Rand(seed * 69_621 + 7);
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    let cp = g.cur_player;
    g.active_entity = cp;
    g.active_entity_name = g.player_name[cp].clone();
    g.app_title = st(__content.string(13661));
    for s in 1..=15 {
        if g.hold_stock_pct[(s, cp)] < 0 {
            g.hold_stock_pct[(s, cp)] = 1;
        }
    }
    let mut answers = Vec::new();
    if !seed.is_multiple_of(7) {
        for _ in 0..r.range(1, 3) {
            let corp = pick_active(&mut r, &g, 131, 1580, &[]);
            clear_holders(&mut g, corp);
            let pct = cast::<i16, _>(r.range(1, 15));
            // the basis: the short sale proceeds (gain or loss against the current cap)
            let basis = g.market_cap[corp] * cast::<Ext, _>(pct) * f64r(&mut r, Ext::from_f64(0.6), Ext::from_f64(1.5));
            give_holding(&mut g, cp, corp, -pct, basis);
            g.player_short_margin_bal[cp] += basis;
        }
        g.cash[cp] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(50_000.0));
        answers.push(format!("pick:{}", r.range(0, 4)));
        // (numbers only: an error box may take one)
        answers.extend(
            (0..4).map(|_| ["1", "2", "5", "99", "0", "15", "3"][cast::<usize, _>(r.range(0, 6))].to_string()),
        );
    }
    Case { g, answers }
}

#[test]
fn diff_cover_short_position() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(seeds(1..=90), |seed| {
        let c = cover_case(seed);
        let cmp = cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-7));
        let Some((ui, _, _)) =
            check(seed, COVER_SHORT, &[], &c.g, &c.answers, &[], cmp, |g, ui, rng| cover_short_position(g, ui, rng))
        else {
            return;
        };
        hits.record(&ui);
    });
    hits.assert(&[
        __content.string(14337),
        // (" [runtime resource wording] ": the listing only offers short positions)
        __content.string(14338),
        __content.string(14336),
        __content.string(12805),
        __content.string(14339),
        __content.string(14340),
    ]);
}
