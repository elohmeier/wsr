//! Differential tests of FindIndustryLeaderPair (0x4a30c0), CalcRequiredAnnualReturn (0x588c72) and the
//! shared part of BuildMenuAdviceText (0x557bbf): the not-controlled / advised-ETF texts and the
//! clean-up for unknown modes.
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::{Opts, run_original_with};
use wsr_core::logic::advice::{calc_required_annual_return, find_industry_leader_pair};

const FIND_INDUSTRY_LEADER_PAIR: u32 = 0x4a30c0;
const CALC_REQUIRED_ANNUAL_RETURN: u32 = 0x588c72;

#[test]
fn find_industry_leader_pair_matches_original() {
    let hits = std::sync::atomic::AtomicU32::new(0);
    par_seeds(1..=40, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed | 1);
        let ind = cast::<i16, _>(r.range(0, 71));
        // make the leader dominant in some worlds
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            let c = cast::<i32, _>(r.range(131, 1580));
            g.industry_num[c] = ind;
            g.entity_status[c] = 1;
            g.business_assets[c] = Ext::from_f64(100_000.0);
        }
        let opts = Opts { args: vec![format!("i:{ind}")], ..Opts::default() };
        let Some(o) = run_original_with(FIND_INDUSTRY_LEADER_PAIR, &opts, &g) else { return };
        let mut p = g.clone();
        let res = find_industry_leader_pair(&mut p, ind);
        assert_eq!(res, cast::<i32, _>(o.return_value), "seed {seed} industry {ind}");
        if res != 0 {
            hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        let d = cmp().diffs(&p, &o.globals);
        assert!(d.is_empty(), "seed {seed}: {}", d.join("\n"));
    });
    if super::diff::unavailable().is_none() {
        assert!(hits.into_inner() > 5, "leader pairs found");
    }
}

#[test]
fn calc_required_annual_return_matches_original() {
    let __content = wsr_core::content::current();
    par_seeds(1..=60, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed.wrapping_mul(77) | 1);
        g.start_net_worth = cast::<i32, _>(r.range(0, 5000));
        g.exchange_rate = cast::<f64, _>([Ext::from_f64(1.0), Ext::from_f64(0.7), Ext::from_f64(1.6), Ext::from_f64(110.0)][cast::<usize, _>(r.range(0, 3))]);
        let years = cast::<i32, _>(r.range(0, 40));
        let base = cast::<Ext, _>(g.start_net_worth) * g.exchange_rate;
        let nw = match r.range(0, 4) {
            0 => base * f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(1.2)),
            1 => base * f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(5.0)),
            2 => base * f64r(&mut r, Ext::from_f64(5.0), Ext::from_f64(2000.0)),
            3 => -f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(1000.0)),
            _ => base * Ext::from_f64(1e9),
        };
        let opts = Opts { args: vec![format!("e:{nw:e}"), years.to_string()], ..Opts::default() };
        let Some(o) = run_original_with(CALC_REQUIRED_ANNUAL_RETURN, &opts, &g) else { return };
        let p = calc_required_annual_return(&g, nw, years);
        let orig = o.return_float.unwrap_or(Ext::NAN);
        assert!(
            p == orig || ((p - orig) / orig).abs() < Ext::from_f64(1e-12),
            "seed {seed}: nw {nw}{__wsr_text_13551}{years}: port {p} vs original {orig}"
        ,__wsr_text_13551=__content.string(13551));
    });
}

/// Modes where only the shared part runs: unknown modes (clean-up only), entities the player does not
/// control (modes 3..7) and funds advised by a company of the player.
#[test]
fn shared_part_matches_original() {
    par_seeds(1..=60, |seed| {
        let mut r = Rand(seed | 1);
        let mode = [0, 8, 3, 4, 5, 6, 7][cast::<usize, _>(r.range(0, 6))];
        let mut g = advice_world(seed, mode);
        if mode != 0 && mode != 8 {
            // not controlled: active entity differs from the selected one
            g.active_entity = g.cur_player;
            g.active_entity_name = g.ent_name[g.cur_player].clone();
            if g.sel_entity == g.active_entity {
                g.sel_entity = cast::<i32, _>(r.range(131, 1580));
            }
        }
        check(seed, &g, cast::<u32, _>(seed));
    });
}
