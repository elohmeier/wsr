//! Differential tests of `logic::sim_entity::quarter_start`: PayAnnualWealthTax,
//! InsiderTradingInvestigation, ExpireAssetSaleOffers (+ ReduceAssetSaleOffer / AddRemoveAssetSaleOffer),
//! GovBondMaturityAndAuction, RevokeExCeoOptions, CallInPlayerAdvances, ClearInsiderTip,
//! CancelPendingMerger and PackHoldingArrays against the original (full state, RND state, message
//! boxes / pop-ups, news).
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::{par_seeds, Rand};
use wsr_core::logic::sim_entity::quarter_start::*;
use wsr_core::pb::numeric::str_num;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::pb::numeric::Cux;
use wsr_core::state::Globals;

const PAY_WEALTH_TAX: u32 = 0x5082ae;
const INSIDER: u32 = 0x67fbfa;
const EXPIRE_OFFERS: u32 = 0x6694c4;
const GOV_BONDS: u32 = 0x4e6b8c;
const REVOKE_OPTIONS: u32 = 0x5fd937;
const CALL_IN_ADVANCES: u32 = 0x5059ac;
const CLEAR_TIP: u32 = 0x67fb97;
const CANCEL_MERGER: u32 = 0x52962a;
const PACK: u32 = 0x49117c;

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

fn str15(x: Ext) -> String {
    str_num(x, 15)
}

/// Runs the original at `addr` and the port `port` on `g0` and compares state, RND state and the
/// number of message boxes / pop-ups.
fn check(
    addr: u32,
    args: &[&str],
    g0: &Globals,
    seed: u64,
    skips: &[(u32, u32)],
    port: impl FnOnce(&mut Globals, &mut Rec, &mut Rng),
) -> Option<Globals> {
    let rng0 = (cast::<u32, _>(seed)).wrapping_mul(0x9e37_79b9) ^ 0x5a5a_1234;
    let o = run(addr, args, g0, rng0, &[], skips)?;
    let mut g = g0.clone();
    let mut ui = Rec::default();
    let mut rng = Rng::from_state(rng0);
    port(&mut g, &mut ui, &mut rng);
    if std::env::var_os("QS_DUMP").is_some() {
        eprintln!("DUMP {addr:x} seed {seed}: news {:?} msgs {:?}", ui.news, ui.msgs);
    }
    let errs: Vec<&String> = o.log.iter().filter(|l| l.starts_with("PBERR")).collect();
    assert!(errs.is_empty(), "seed {seed}: runtime errors in the original: {errs:?}");
    cmp().assert(&g, &o.globals);
    assert_eq!(rng.state(), o.rng, "seed {seed}: RND state");
    assert_eq!(ui.msgs.len(), orig_msgs(&o), "seed {seed}: message boxes / pop-ups {:?}\n{:?}", ui.msgs, o.log);
    Some(g)
}

/// Option records `corp|type|holder|qty|strike|month|year|premium-or-tag` (1..n packed, some gaps).
fn option_book(g: &mut Globals, r: &mut Rand) {
    let mut k = 0;
    for _ in 0..r.range(0, 60) {
        let corp = cast::<i32, _>(r.range(11, 1600));
        let holder = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))) } else { cast::<i32, _>(r.range(11, 1600)) };
        let tag = match r.range(0, 4) {
            0 | 1 if holder <= 10 => "GRANT".to_string(),
            2 if holder <= 10 => "VESTED".to_string(),
            _ => str15(f64r(r, Ext::from_f64(1.0), Ext::from_f64(50.0))),
        };
        k += if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) { 2 } else { 1 };
        g.option_recs[k] = s(&format!(
            "{}|{}|{}|{}|{}|{}|{}|{}",
            str15(Ext::from(cast::<Ext, _>(corp))).trim(),
            str15(Ext::from(cast::<Ext, _>(r.range(1, 2)))),
            str15(Ext::from(cast::<Ext, _>(holder))),
            str15(Ext::from(cast::<Ext, _>(r.range(-20, 20)))),
            str15(f64r(r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(200.0)))),
            str15(Ext::from(cast::<Ext, _>(r.range(1, 12)))),
            str15(Ext::from(cast::<Ext, _>(g.cur_year + cast::<i32, _>(r.range(-1, 2))))),
            tag
        ));
        if r.chance(cast::<f64, _>(Ext::from_f64(0.05))) {
            g.option_recs[k] = s(&(g.option_recs[k].to_display() + " "));
        }
    }
    g.option_recs[0] = s(str15(cast::<Ext, _>(k)).trim());
}

/// Ragged record lists for PackHoldingArrays.
fn ragged_records(g: &mut Globals, r: &mut Rand) {
    let __content = wsr_core::content::current();
    for k in 1..=200 {
        if r.chance(cast::<f64, _>(Ext::from_f64(0.15))) {
            g.futures_recs[k] = s(&format!("OIL|{}|{}|1.5", r.range(1, 1600), r.range(-5, 5)));
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) {
            g.swap_contracts[k] = s(&format!("{}|{}{__wsr_text_13985}{}", r.range(11, 130), r.range(11, 130), r.range(1, 999),__wsr_text_13985=__content.string(13985)));
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) {
            g.phys_commod_rec[k] = s(&format!(" GOLD|{}|{}|1.00 ", r.range(1, 1600), r.range(1, 50)));
        }
    }
    for k in 1..=100 {
        if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) {
            g.price_alerts[k] = s(&format!("{}|{}", r.range(11, 1600), r.range(1, 99)));
        }
    }
    // holdings with gaps, some negative bases (excess loss accounts)
    for e in 1..=1600 {
        for slot in 1..=15 {
            if g.hold_stock_id[(slot, e)] != 0 && r.chance(cast::<f64, _>(Ext::from_f64(0.1))) {
                g.hold_stock_basis[(slot, e)] = -f64r(r, Ext::from_f64(0.0), Ext::from_f64(20.0));
            }
        }
        g.hold_stock_id[(0, e)] = cast::<i16, _>(r.range(0, 15));
    }
    for h in 1..=1583 {
        g.hold_bond_id[(0, h)] = cast::<i32, _>(r.range(0, 15));
    }
}

/// Asset-sale offers `seller|stock or CA|qty|flag|expQtr|expYear`.
fn offers(g: &mut Globals, r: &mut Rand) {
    let mut n = 0;
    for k in 1..=100 {
        if !r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
            continue;
        }
        let seller = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))) } else { cast::<i32, _>(r.range(11, 1600)) };
        let year = g.cur_year + cast::<i32, _>(r.range(-1, 1));
        let qtr = r.range(1, 4);
        let ca = r.chance(cast::<f64, _>(Ext::from_f64(0.4)));
        let seller = if ca { cast::<i32, _>(r.range(131, 1600)) } else { seller };
        let (what, qty) = if ca {
            let ba = g.business_assets[seller];
            ("CA".to_string(), str15((ba * f64r(r, Ext::from_f64(0.3), Ext::from_f64(1.6))).round()))
        } else {
            let slot = cast::<i32, _>(r.range(1, 15));
            let mut stock = cast::<i32, _>(g.hold_stock_id[(slot, seller)]);
            let mut pct = cast::<i32, _>(g.hold_stock_pct[(slot, seller)]) + cast::<i32, _>(r.range(-10, 10));
            if stock == 0 || r.chance(cast::<f64, _>(Ext::from_f64(0.1))) {
                stock = cast::<i32, _>(r.range(11, 1600));
                pct = cast::<i32, _>(r.range(1, 50));
            }
            (str15(cast::<Ext, _>(stock)).trim().to_string(), str15(cast::<Ext, _>(pct.max(1))))
        };
        g.asset_sale_offers[k] = s(&format!("{}|{}|{}|{}|{}|{}", str15(Ext::from(cast::<Ext, _>(seller))).trim(), what, qty.trim(), r.range(0, 1), qtr, year));
        n = k;
        if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) {
            g.auto_pilot_rejected_target = what.parse().unwrap_or(0);
        }
    }
    g.asset_sale_offers[0] = s(str15(cast::<Ext, _>(n)).trim());
}

fn base(seed: u64) -> (Globals, Rand) {
    let g = world(seed);
    let r = Rand(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) ^ 0xabcdef);
    (g, r)
}

#[test]
fn diff_pay_annual_wealth_tax() {
    par_seeds(1..=8, |seed| {
        let (mut g, mut r) = base(seed);
        g.cur_quarter = 4;
        g.wealth_tax_rate = cast::<i32, _>(r.range(0, 3));
        g.corp_shares_tax_country = cast::<i32, _>(r.range(0, 25));
        for p in 1..=5 {
            g.cash[p] = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(50000.0));
        }
        check(PAY_WEALTH_TAX, &[], &g, seed, &[], |g, ui, rng| pay_annual_wealth_tax(g, ui, rng, 0));
    });
}

#[test]
fn diff_insider_trading_investigation() {
    let __content = wsr_core::content::current();
    par_seeds(1..=24, |seed| {
        let (mut g, mut r) = base(seed);
        g.num_players = cast::<i32, _>(r.range(2, 5));
        for p in 1..=5 {
            g.player_name[p] = if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) { s("...OUT...") } else { s(&format!("{__wsr_text_13556}{p}",__wsr_text_13556=__content.string(13556))) };
            g.tipster_state[p] = [0, 4, 4, -1, 2][cast::<usize, _>(r.range(0, 4))];
        }
        g.insider_probe_player = if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) { cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))) } else { 0 };
        g.currency_code = s(["USA", "UK", ""][cast::<usize, _>(r.range(0, 2))]);
        if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
            g.merger_acquirer = if r.chance(cast::<f64, _>(Ext::from_f64(0.8))) { cast::<i32, _>(r.range(11, 1600)) } else { 0 };
            g.merger_target = cast::<i32, _>(r.range(11, 1600));
            g.merger_proposer_player = cast::<i32, _>(r.range(0, 2));
            g.merger_price_per_share = Cux(r.range(0, 5000));
            g.next_takeover_tick = cast::<i32, _>(r.range(0, 100));
        }
        match r.range(0, 3) {
            0 => {}
            1 => {
                g.merger_tip_corp = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { cast::<i16, _>(g.merger_target) } else { cast::<i16, _>(r.range(11, 1600)) };
                g.insider_tippee = if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) { cast::<i16, _>(g.cur_player) } else { cast::<i16, _>(r.range(0, 3)) };
                g.insider_tip_event_tick = cast::<i16, _>(r.range(0, 120));
            }
            _ => {
                let c = cast::<i16, _>(r.range(11, 1600));
                g.insider_earn_tip_corp = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { c } else { -c };
                g.insider_tippee = cast::<i16, _>(r.range(0, 2));
                g.insider_tip_event_qtr = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { cast::<i16, _>(g.cur_quarter) } else { cast::<i16, _>(r.range(1, 4)) };
                g.insider_tip_event_tick = cast::<i16, _>(r.range(1, 4));
            }
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
            g.news_msg = s("Some pending headline");
        }
        check(INSIDER, &[], &g, seed, &[], |g, ui, rng| {
            insider_trading_investigation(g, ui, rng)
        });
    });
}

#[test]
fn diff_expire_asset_sale_offers() {
    par_seeds(1..=12, |seed| {
        let (mut g, mut r) = base(seed);
        offers(&mut g, &mut r);
        option_book(&mut g, &mut r);
        ragged_records(&mut g, &mut r);
        check(EXPIRE_OFFERS, &[], &g, seed, &[], |g, ui, _| expire_asset_sale_offers(g, ui));
    });
}

#[test]
fn diff_gov_bond_maturity_and_auction() {
    let __content = wsr_core::content::current();
    par_seeds(1..=8, |seed| {
        let (mut g, mut r) = base(seed);
        let y = if seed % 4 == 0 { g.cur_year + 1 } else { g.cur_year };
        g.short_bond_desc = s(&format!("{y} SHORT T-NOTE {:5.2}", f32r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(10.0)))));
        g.long_bond_yield = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(14.0)));
        for e in (1..=130).chain(1581..=1583) {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
                g.short_gov_bond_face[e] = r.range(1, 5000);
                g.st_gov_bond_cost[e] = cast::<Ext, _>(g.short_gov_bond_face[e]) * f64r(&mut r, Ext::from_f64(0.9), Ext::from_f64(1.05));
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
                g.long_gov_bond_face[e] = r.range(1, 5000);
                g.lt_gov_bond_cost[e] = cast::<Ext, _>(g.long_gov_bond_face[e]) * f64r(&mut r, Ext::from_f64(0.8), Ext::from_f64(1.1));
            }
        }
        for k in 1..=40 {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                let l = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { "L" } else { "S" };
                g.swap_contracts[k] = s(&format!("{}|{}|{l}|{}{__wsr_text_14132}", r.range(11, 130), r.range(11, 130), r.range(1, 999),__wsr_text_14132=__content.string(14132)));
            }
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.news_msg = s("A pending headline");
        }
        check(GOV_BONDS, &[], &g, seed, &[], |g, ui, rng| gov_bond_maturity_and_auction(g, ui, rng));
    });
}

#[test]
fn diff_revoke_ex_ceo_options() {
    par_seeds(1..=10, |seed| {
        let (mut g, mut r) = base(seed);
        option_book(&mut g, &mut r);
        for p in 1..=5 {
            g.player_ceo_of[p] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { cast::<i32, _>(r.range(11, 1600)) } else { 0 };
        }
        // make some GRANT holders CEO / controller of the company
        for k in 1..=60 {
            let rec = g.option_recs[k].to_display();
            if rec.contains("GRANT") && r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                let f: Vec<&str> = rec.split('|').collect();
                let corp: i32 = f[0].trim().parse().unwrap();
                let holder: i32 = f[2].trim().parse().unwrap();
                if (1..=5).contains(&holder) {
                    if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                        g.player_ceo_of[cast::<i32, _>(cast::<usize, _>(holder))] = corp;
                    } else {
                        g.corp_controller[corp] = holder;
                    }
                }
            }
        }
        check(REVOKE_OPTIONS, &[], &g, seed, &[], |g, ui, _| revoke_ex_ceo_options(g, ui));
    });
}

#[test]
fn diff_call_in_player_advances() {
    par_seeds(1..=16, |seed| {
        let (mut g, mut r) = base(seed);
        let p = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
        g.cash[p] = -f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(3000.0));
        g.is_computer_player[p] = cast::<i32, _>(r.range(0, 1));
        for _ in 0..r.range(1, 12) {
            let c = cast::<i32, _>(r.range(11, 1600));
            g.player_advances[(p, c)] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(2000.0));
        }
        let mode = r.range(1, 2);
        let mode_s = mode.to_string();
        check(CALL_IN_ADVANCES, &[&p.to_string(), &mode_s], &g, seed, &[], |g, ui, rng| {
            call_in_player_advances(g, ui, rng, p, cast::<i32, _>(mode))
        });
    });
}

#[test]
fn diff_clear_tip_and_cancel_merger() {
    par_seeds(1..=6, |seed| {
        let (mut g, mut r) = base(seed);
        g.insider_earn_tip_corp = cast::<i16, _>(r.range(-500, 500));
        g.merger_tip_corp = cast::<i16, _>(r.range(0, 500));
        g.insider_tippee = cast::<i16, _>(r.range(0, 3));
        g.insider_tip_event_tick = cast::<i16, _>(r.range(0, 100));
        g.insider_tip_event_qtr = cast::<i16, _>(r.range(0, 4));
        g.tip_industry = cast::<i16, _>(r.range(0, 60));
        g.merger_acquirer = cast::<i32, _>(r.range(11, 1600));
        g.merger_target = cast::<i32, _>(r.range(11, 1600));
        g.merger_proposer_player = 1;
        g.merger_price_per_share = Cux(r.range(1, 9999));
        g.next_takeover_tick = 77;
        check(CLEAR_TIP, &[], &g, seed, &[], |g, _, _| clear_insider_tip(g));
        let e = if seed % 2 == 0 { 0 } else { cast::<i32, _>(r.range(11, 1600)) };
        g.entity_status[e] = if seed % 3 == 0 { 1 } else { 2 };
        check(CANCEL_MERGER, &[&e.to_string()], &g, seed, &[], |g, ui, _| cancel_pending_merger(g, ui, e));
    });
}

#[test]
fn diff_pack_holding_arrays() {
    par_seeds(1..=8, |seed| {
        let (mut g, mut r) = base(seed);
        option_book(&mut g, &mut r);
        offers(&mut g, &mut r);
        ragged_records(&mut g, &mut r);
        check(PACK, &[], &g, seed, &[], |g, ui, _| pack_holding_arrays(g, ui));
    });
}
