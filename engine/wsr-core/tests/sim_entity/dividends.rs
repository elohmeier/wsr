//! Differential tests of `logic::sim_entity::dividends`: AiSetDividend, AiCorpGrowthAndDividends,
//! CalcConsolidatedTaxPayment, AdjustPriceForDistribution, ChargeShortsForDistribution,
//! EtfAddToHoldings, AdjustOptionsForCorpAction against the original routines.
#![allow(unused_imports)]
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use wsr_core::logic::sim_entity::dividends::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

const AI_SET_DIVIDEND: u32 = 0x49b683;
const AI_CORP_GROWTH: u32 = 0x6b2a08;
const CONSOL_TAX: u32 = 0x4fa767;
const ADJ_PRICE_DIST: u32 = 0x619ed2;
const CHARGE_SHORTS: u32 = 0x5e2f14;
const ETF_ADD: u32 = 0x64f921;
const ADJ_OPTIONS: u32 = 0x5f7680;
/// BuyStock (hooked: the purchases of EtfAddToHoldings).
const BUY_STOCK: u32 = 0x4efe60;
static ETF_BOUGHT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Port result (globals, RND state, value, news) vs. the original's; panics on differences.
#[track_caller]
fn check(what: &str, seed: u64, port: &Globals, port_rng: &Rng, o: &Original) {
    let d = cmp().diffs(port, &o.globals);
    let log: Vec<&String> = o.log.iter().filter(|l| !l.starts_with("API")).take(20).collect();
    assert!(d.is_empty(), "{what} seed {seed}: {} differences:\n{}\nlog: {log:?}", d.len(), d.iter().take(30).cloned().collect::<Vec<_>>().join("\n"));
    assert_eq!(port_rng.state(), o.rng, "{what} seed {seed}: RND state");
    let errs: Vec<&String> = o.log.iter().filter(|l| l.starts_with("PBERR")).collect();
    assert!(errs.is_empty(), "{what} seed {seed}: runtime errors in the original: {errs:?}");
}

/// News headlines the original stored (NewsHeadline changes are compared with the globals already).
fn rng_state(r: &mut Rand) -> u32 {
    cast::<u32, _>(r.next())
}

/// A corp id with status 1 in `lo..=hi` (or any id there).
fn pick_corp(g: &Globals, r: &mut Rand, lo: i64, hi: i64) -> i32 {
    for _ in 0..50 {
        let c = cast::<i32, _>(r.range(lo, hi));
        if g.entity_status[c] == 1 {
            return c;
        }
    }
    cast::<i32, _>(r.range(lo, hi))
}

/// BYVAL EXT as three stack dwords (lo, mid, sign/exponent) for the harness.
fn ext_dwords(x: Ext) -> [String; 3] {
    let bits = x.to_bits();
    let sign = bits >> 63;
    let exp = cast::<i64, _>((bits >> 52) & 0x7ff);
    let frac = bits & ((1u64 << 52) - 1);
    let (e80, m80): (u64, u64) = if exp == 0 && frac == 0 {
        (0, 0)
    } else if exp == 0 {
        // subnormal double: normalise
        let lz = cast::<i64, _>(frac.leading_zeros()) - 11;
        let m = frac << (lz + 11);
        (cast::<u64, _>(1 - 1023 - lz + 16383), m)
    } else {
        (cast::<u64, _>(exp - 1023 + 16383), (1u64 << 63) | (frac << 11))
    };
    let hi = (sign << 15) | e80;
    [
        format!("v:{}", cast::<u32, _>(m80)),
        format!("v:{}", cast::<u32, _>(m80 >> 32)),
        format!("v:{}", cast::<u32, _>(hi)),
    ]
}

// ------------------------------------------------------------------------------------- AiSetDividend

#[test]
fn diff_ai_set_dividend() {
    let __content = wsr_core::content::current();
    par_seeds(1..=48, |seed| {
        let mut r = Rand(seed * 7919 + 13);
        let mut g = world(seed);
        g.monetary_stance = [-1, 0, 0, 1][cast::<usize, _>(r.range(0, 3))];
        let c = match seed % 6 {
            0 => pick_corp(&g, &mut r, 11, 85),
            1 => pick_corp(&g, &mut r, 86, 130),
            2 => pick_corp(&g, &mut r, 1581, 1583),
            _ => pick_corp(&g, &mut r, 131, 1580),
        };
        if seed % 5 == 0 {
            g.industry_num[c] = [69, 70][cast::<usize, _>(r.range(0, 1))];
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.cash[c] = f64r(&mut r, Ext::from_f64(10000.0), Ext::from_f64(200000.0));
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.earnings_y1[c] = f64r(&mut r, Ext::from_f64(10.0), Ext::from_f64(2000.0));
            g.dividend_per_share[c] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(2.0));
        }
        let qtr = f64r(&mut r, -Ext::from_f64(100.0), Ext::from_f64(300.0));
        let nw = f64r(&mut r, -Ext::from_f64(500.0), Ext::from_f64(30000.0));
        let hold = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { cast::<i32, _>(r.range(1, 3)) } else { 0 };
        let rs = rng_state(&mut r);
        let args = [c.to_string(), format!("{__wsr_text_14156}{qtr:?}",__wsr_text_14156=__content.string(14156)), format!("{__wsr_text_14156}{nw:?}",__wsr_text_14156=__content.string(14156)), hold.to_string()];
        let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let Some(o) = run(AI_SET_DIVIDEND, &args, &g, rs, &[], &[]) else { return };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let v = ai_set_dividend(&mut port, &mut rng, c, qtr, nw, hold);
        check("AiSetDividend", seed, &port, &rng, &o);
        assert_eq!(v, cast::<i32, _>(o.return_value), "AiSetDividend seed {seed}: result");
    });
}

// ---------------------------------------------------------------------------- AiCorpGrowthAndDividends

#[test]
fn diff_ai_corp_growth_and_dividends() {
    par_seeds(1..=48, |seed| {
        let mut r = Rand(seed * 104729 + 7);
        let mut g = world(seed);
        g.monetary_stance = [-1, 0, 0, 0, 1][cast::<usize, _>(r.range(0, 4))];
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.non_fin_corp_count = 0;
        } else {
            g.non_fin_corp_count = cast::<i32, _>(r.range(500, 1200));
            g.good_credit_pct = cast::<i32, _>(r.range(20, 90));
        }
        let c = if seed % 8 == 0 { pick_corp(&g, &mut r, 86, 130) } else { pick_corp(&g, &mut r, 131, 1580) };
        match seed % 7 {
            0 => g.industry_num[c] = [7, 8, 15, 50][cast::<usize, _>(r.range(0, 3))],
            1 => g.industry_num[c] = 69,
            2 => g.business_assets[c] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(400.0)),
            _ => {}
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.corp_controller[c] = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
            g.player_growth_throttle[g.corp_controller[c]] = cast::<i16, _>(r.range(0, 30));
            g.auto_pilot[c] = cast::<i32, _>(r.range(0, 1));
        } else {
            g.corp_controller[c] = cast::<i32, _>(r.range(11, 1600));
        }
        let rs = rng_state(&mut r);
        let cs = c.to_string();
        let Some(o) = run(AI_CORP_GROWTH, &[cs.as_str()], &g, rs, &[], &[]) else { return };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        ai_corp_growth_and_dividends(&mut port, &mut ui, &mut rng, c);
        check("AiCorpGrowthAndDividends", seed, &port, &rng, &o);
    });
}

// -------------------------------------------------------------------------- CalcConsolidatedTaxPayment

#[test]
fn diff_calc_consolidated_tax_payment() {
    par_seeds(1..=48, |seed| {
        let mut r = Rand(seed * 15485863 + 3);
        let mut g = world(seed);
        // funds (industry 71) are 1581..1583 (another id relabelled 71 makes UpdateCorpControl fail)
        let c = if seed % 10 == 0 { pick_corp(&g, &mut r, 1581, 1583) } else { pick_corp(&g, &mut r, 11, 1580) };
        // an 80 % owner for tax consolidation in some states
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            let p = pick_corp(&g, &mut r, 131, 1580);
            if p != c {
                g.hold_stock_id[(1, p)] = cast::<i16, _>(c);
                g.hold_stock_pct[(1, p)] = cast::<i16, _>(r.range(80, 100));
                g.corp_controller[c] = p;
                if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                    g.market_cap[p] = f64r(&mut r, Ext::from_f64(0.1), Ext::from_f64(3.0));
                }
            }
        }
        g.tax_loss_carry[c] = match r.range(0, 3) {
            0 => Ext::from_f64(0.0),
            1 => f64r(&mut r, -Ext::from_f64(300.0), Ext::from_f64(0.0)),
            2 => f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(300.0)),
            _ => f64r(&mut r, -Ext::from_f64(5.0), Ext::from_f64(5.0)),
        };
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.extraordinary_pl[c] = r.range(-200, 100);
        }
        g.tmp_ext1 = f64r(&mut r, -Ext::from_f64(400.0), Ext::from_f64(400.0));
        g.parent80_owner = 0;
        let rs = rng_state(&mut r);
        let cs = c.to_string();
        let Some(o) = run(CONSOL_TAX, &[cs.as_str()], &g, rs, &[], &[]) else { return };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        calc_consolidated_tax_payment(&mut port, &mut ui, &mut rng, c);
        check("CalcConsolidatedTaxPayment", seed, &port, &rng, &o);
    });
}

// ---------------------------------------------------------- AdjustPriceForDistribution / ChargeShorts

#[test]
fn diff_adjust_price_for_distribution() {
    let __content = wsr_core::content::current();
    par_seeds(1..=24, |seed| {
        let mut r = Rand(seed * 31 + 5);
        let g = world(seed);
        let c = pick_corp(&g, &mut r, 11, 1600);
        let amt = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(300000.0)) } else { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(3000.0)) };
        let rs = rng_state(&mut r);
        let args = [c.to_string(), format!("{__wsr_text_14156}{amt:?}",__wsr_text_14156=__content.string(14156))];
        let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let Some(o) = run(ADJ_PRICE_DIST, &args, &g, rs, &[], &[]) else { return };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        adjust_price_for_distribution(&mut port, &mut ui, &mut rng, c, amt);
        check("AdjustPriceForDistribution", seed, &port, &rng, &o);
    });
}

#[test]
fn diff_charge_shorts_for_distribution() {
    par_seeds(1..=24, |seed| {
        let mut r = Rand(seed * 37 + 11);
        let mut g = world(seed);
        let c = pick_corp(&g, &mut r, 131, 1580);
        for p in 1..=g.num_players {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
                let slot = cast::<i32, _>(r.range(1, 15));
                g.hold_stock_id[(slot, p)] = cast::<i16, _>(c);
                g.hold_stock_pct[(slot, p)] = cast::<i16, _>(r.range(-30, 20));
            }
        }
        let amt = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(500.0));
        let rs = rng_state(&mut r);
        let mut args: Vec<String> = ext_dwords(amt).to_vec();
        args.push(format!("v:{c}"));
        let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let Some(o) = run(CHARGE_SHORTS, &args, &g, rs, &[], &[]) else { return };
        let mut port = g.clone();
        let (mut ui, mut rng) = (Rec::default(), Rng::from_state(rs));
        charge_shorts_for_distribution(&mut port, &mut ui, &mut rng, amt, c);
        check("ChargeShortsForDistribution", seed, &port, &rng, &o);
    });
}

// ---------------------------------------------------------------------------------- EtfAddToHoldings

#[test]
fn diff_etf_add_to_holdings() {
    let __content = wsr_core::content::current();
    par_seeds(1..=24, |seed| {
        let mut r = Rand(seed * 41 + 17);
        let mut g = world(seed);
        let f = cast::<i32, _>(r.range(1584, 1600));
        g.entity_status[f] = 1;
        g.industry_num[f] = 71;
        let n = cast::<i32, _>(r.range(8, 15));
        for s in 1..=15 {
            if s <= n {
                g.hold_stock_id[(s, f)] = cast::<i16, _>(pick_corp(&g, &mut r, 131, 1580));
                g.hold_stock_pct[(s, f)] = cast::<i16, _>(r.range(1, 15));
            } else {
                g.hold_stock_id[(s, f)] = 0;
                g.hold_stock_pct[(s, f)] = 0;
            }
        }
        g.hold_stock_id[(0, f)] = cast::<i16, _>(n);
        g.cash[f] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(20000.0));
        g.t_bills[f] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(5000.0));
        g.fund_adviser[f] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { pick_corp(&g, &mut r, 86, 1580) } else { 0 };
        let reserve = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(3000.0));
        let rs = rng_state(&mut r);
        let args = [f.to_string(), format!("{__wsr_text_14156}{reserve:?}",__wsr_text_14156=__content.string(14156))];
        let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let Some(o) = run_hooked(ETF_ADD, &args, &g, rs, &[], &[], &[BUY_STOCK]) else { return };
        if o.log.iter().any(|l| l.starts_with("CALL BuyStock(")) {
            ETF_BOUGHT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        let v = etf_add_to_holdings(&mut port, &mut ui, &mut rng, f, reserve, 0);
        check("EtfAddToHoldings", seed, &port, &rng, &o);
        assert_eq!(v, cast::<i32, _>(o.return_value), "EtfAddToHoldings seed {seed}: result");
    });
    let n = ETF_BOUGHT.load(std::sync::atomic::Ordering::Relaxed);
    eprintln!("coverage EtfAddToHoldings BuyStock: {n}");
    if unavailable().is_none() {
        assert!(n > 0, "EtfAddToHoldings: no purchase");
    }
}

// ------------------------------------------------------------------------ AdjustOptionsForCorpAction

#[test]
fn diff_adjust_options_for_corp_action() {
    let __content = wsr_core::content::current();
    par_seeds(1..=24, |seed| {
        let mut r = Rand(seed * 43 + 19);
        let mut g = world(seed);
        let c = pick_corp(&g, &mut r, 131, 1580);
        let n = cast::<i32, _>(r.range(0, 30));
        for i in 1..=n {
            let corp = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { c } else { pick_corp(&g, &mut r, 131, 1580) };
            let holder = r.range(1, 1600);
            let qty = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { r.range(1, 50) } else { -r.range(1, 50) };
            let strike = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(900.0));
            g.option_recs[i] = PbStr::from(format!("{corp}|{}|{holder}|{qty}|{strike:.2}|{}|1|{:.2}", r.range(1, 2), g.cur_year + 1, f64r(&mut r, Ext::from(Ext::from_f64(0.0)), Ext::from(Ext::from_f64(50.0)))).as_str());
        }
        g.option_recs[0] = PbStr::from(n.to_string().as_str());
        if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) {
            g.shares_outstanding[c] = Ext::from_f64(0.0);
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.news_msg = PbStr::from(__content.string(13571));
        }
        let amt = match r.range(0, 3) {
            0 => f64r(&mut r, Ext::from_f64(50.0), Ext::from_f64(200.0)),
            1 => -f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(300.0)),
            2 => Ext::from_f64(0.0),
            _ => f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(100.0)),
        };
        let pct = [0, 50, 120, 200, cast::<i32, _>(r.range(11, 1600))][cast::<usize, _>(r.range(0, 4))];
        let dist = cast::<i32, _>(r.range(0, 3));
        let rs = rng_state(&mut r);
        let args = [c.to_string(), ext_arg(amt), pct.to_string(), dist.to_string()];
        let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let Some(o) = run(ADJ_OPTIONS, &args, &g, rs, &[], &[]) else { return };
        let mut port = g.clone();
        let mut ui = Rec::default();
        adjust_options_for_corp_action(&mut port, &mut ui, c, amt, pct, dist);
        let d = cmp().diffs(&port, &o.globals);
        assert!(d.is_empty(), "AdjustOptionsForCorpAction seed {seed}: {} differences:\n{}", d.len(), d.iter().take(30).cloned().collect::<Vec<_>>().join("\n"));
    });
}
