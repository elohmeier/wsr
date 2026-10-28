//! Differential tests of `logic::assets::sell`: SellBusinessAssets (0x5d8ab9) and its GOSUBs.
#![allow(unused_imports)]
use wsr_core::pb::real::{Ext, cast};


use super::buy::{SKIPS, answer_pool, buy_world};
use super::common::*;
use super::diff::*;
use super::script::*;
use std::sync::Mutex;
use wsr_core::logic::assets::sell::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

const SELL: u32 = 0x5d8ab9;

/// [`buy_world`] turned into a selling scenario: the acting company has assets; the industry has
/// company-controlled buyers with money, holding companies (some with tax losses or credits), and
/// sometimes companies of the player's own.
fn sell_world(seed: u64) -> Globals {
    let (mut g, ind) = buy_world(seed + 5000);
    let mut r = Rand(seed.wrapping_mul(2654435761) + 9);
    let ae = g.active_entity;
    if ae > 130 && g.industry_num[ae] == 70 && !seed.is_multiple_of(7) {
        g.industry_num[ae] = ind;
    }
    if ae > 130 {
        g.business_assets[ae] = match r.range(0, 4) {
            0 => f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(100.0)),
            1 => f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(1000.0)),
            2 => f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(5000.0)),
            _ => f64r(&mut r, Ext::from_f64(5000.0), Ext::from_f64(30000.0)),
        };
        g.working_capital[ae] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(600.0));
    }
    // the player's own companies of the industry / holding companies
    let own = match seed % 5 {
        0 => 0,
        1 => 1,
        _ => r.range(0, 3),
    };
    for c in 131..=1580 {
        if c != ae && g.corp_controller[c] == 1 && (g.industry_num[c] == ind || g.industry_num[c] == 70) {
            g.corp_controller[c] = c;
        }
    }
    for k in 0..own {
        let c = cast::<i32, _>(r.range(131, 1580));
        if c == ae {
            continue;
        }
        g.entity_status[c] = 1;
        g.corp_controller[c] = 1;
        // with the player's voting control (SelectTargetCorp makes the buyer the ActiveEntity)
        for e in 1..=1600 {
            for s in 1..=15 {
                if cast::<i32, _>(g.hold_stock_id[(s, e)]) == c {
                    g.hold_stock_id[(s, e)] = 0;
                    g.hold_stock_pct[(s, e)] = 0;
                }
            }
        }
        let slot = 2 + cast::<i32, _>(k);
        g.hold_stock_id[(slot, 1)] = cast::<i16, _>(c);
        g.hold_stock_pct[(slot, 1)] = 65;
        g.hold_stock_basis[(slot, 1)] = Ext::from_f64(50.0);
        g.hold_stock_id[(0, 1)] = cast::<i16, _>((1..=15).filter(|&s| g.hold_stock_id[(s, 1)] != 0).count());
        g.industry_num[c] = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { ind } else { 70 };
    }
    // holding [runtime resource wording] companies
    for _ in 0..r.range(0, 6) {
        let c = cast::<i32, _>(r.range(131, 1580));
        if c == ae {
            continue;
        }
        g.entity_status[c] = 1;
        g.industry_num[c] = 70;
        g.corp_controller[c] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { c } else { cast::<i32, _>(r.range(131, 1580)) };
        g.business_assets[c] = Ext::from_f64(0.0);
        g.tax_loss_carry[c] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { -f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(300.0)) } else { Ext::from_f64(0.0) };
        g.tax_credits[c] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(30.0)) } else { Ext::from_f64(0.0) };
        g.cash[c] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(6000.0));
    }
    // buyers' money
    for c in 131..=1580 {
        if g.industry_num[c] == ind && c != ae && r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            g.cash[c] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(6000.0));
        }
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
        g.lending_bank[ae] = cast::<i16, _>(r.range(11, 85));
    }
    g.total_assets = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(9000.0));
    if seed.is_multiple_of(37) {
        g.active_entity = 1;
        g.active_entity_name = g.player_name[1].clone();
    }
    g
}

#[test]
fn diff_sell_business_assets() {
    let __content = wsr_core::content::current();
    let hits = Mutex::new(Vec::new());
    par_seeds(seeds(1..=400), |seed| {
        let g = sell_world(seed);
        let mut r = Rand(seed * 59 + 21);
        let mut answers = Vec::new();
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            answers.push(["IDYES", "IDNO", "IDCANCEL", "IDNO"][cast::<usize, _>(r.range(0, 3))].to_string());
        }
        answers.push(match r.range(0, 5) {
            0 => "dflt".to_string(),
            1 => "frac:0.1".to_string(),
            2 => "frac:0.5".to_string(),
            3 => "frac:0.9".to_string(),
            _ => answer_pool(&mut r),
        });
        for _ in 0..r.range(2, 9) {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.12))) {
                answers.push(format!("pick:{}", r.range(0, 10)));
            } else {
                answers.push(answer_pool(&mut r));
            }
        }
        let Some((ui, _)) = check(seed, SELL, &[], &g, &answers, &SKIPS, |g, ui, rng| {
            sell_business_assets(g, ui, rng, &wsr_core::logic::newgame::LocalTime::HARNESS)
        }) else {
            return;
        };
        let mut h = hits.lock().unwrap();
        h.extend(ui.titles.iter().cloned());
        h.extend(ui.msgs.iter().cloned());
        h.extend(ui.reports.iter().cloned());
        h.extend(ui.input_titles.iter().cloned());
    });
    let h = hits.into_inner().unwrap();
    for want in [
        __content.string(2228),
        __content.string(2235),
        __content.string(2242),
        __content.string(2250),
        __content.string(2030),
        "Error number 9",
    ] {
        assert!(h.iter().any(|t| t.starts_with(want)), "branch not hit: {want}");
    }
    for want in [
        __content.string(13593),
        " sells ",
        __content.string(13594),
        " at loss of ",
        __content.string(2259),
        __content.string(13595),
        __content.string(13596),
        __content.string(13597),
        __content.string(2238),
    ] {
        assert!(h.iter().any(|t| t.contains(want)), "branch not hit: ..{want}..");
    }
}
