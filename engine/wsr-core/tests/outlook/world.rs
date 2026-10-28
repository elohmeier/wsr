//! A random world for the outlook texts: the sim_entity world plus the economy/crisis/tax state the
//! texts depend on, with values spread around the thresholds the original tests.
#![allow(dead_code)]
use wsr_core::pb::real::{Ext, cast};


use super::common::{f32r, st, world};
use super::diff::Rand;
use wsr_core::state::Globals;

pub fn pick<T: Copy>(r: &mut Rand, xs: &[T]) -> T {
    xs[cast::<usize, _>(r.range(0, cast::<i64, _>(xs.len()) - 1))]
}

/// Random world for seed `seed` (deterministic).
pub fn outlook_world(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let mut g = world(seed + 7000);
    let mut r = Rand(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) ^ 0x1234_5678);
    // rates / GDP around the thresholds (1.8, 2.2, 3.6, 4.8 ...; 5.5, 6, 8, 9.2 ...)
    g.base_interest_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(12.0)));
    g.prime_rate = cast::<f32, _>(g.base_interest_rate + f32r(&mut r, Ext::from_f64(0.0), Ext::from_f64(3.0)));
    g.interest_rate_target = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(12.0)));
    g.gdp_growth = cast::<f32, _>({ let v = [f32r(&mut r, -Ext::from_f64(6.0), Ext::from_f64(8.0)), f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(6.0)), f32r(&mut r, -Ext::from_f64(3.0), Ext::from_f64(3.0))]; pick(&mut r, &v) });
    g.gdp_growth_target = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(5.0), Ext::from_f64(8.0)));
    g.monetary_policy = cast::<i16, _>(r.range(0, 2));
    g.central_bank_name = st(pick(&mut r, &[__content.string(4373), __content.string(6645), __content.string(6647)]));
    g.subprime_crisis_state = cast::<i16, _>(pick(&mut r, &[0, 0, -2, -1, 1, 2, 3]));
    g.peak_oil_crisis = cast::<i16, _>(pick(&mut r, &[0, 0, 1, 2, -1]));
    g.bubble_state = pick(&mut r, &[0, 0, -1, 100, 105, 108, 110, 113, 115, 200, 205, 206, 208, 210, 218, 220]);
    g.fad_industry = cast::<i32, _>(r.range(0, 69));
    g.fad_valuation_pct = cast::<i32, _>(pick(&mut r, &[100, 140, 152, 154, 155, 158, 170, 200]));
    g.oil_price = cast::<f32, _>({ let v = [f32r(&mut r, Ext::from_f64(0.3), Ext::from_f64(1.5)), f32r(&mut r, Ext::from_f64(0.6), Ext::from_f64(1.0)), f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(3.0))]; pick(&mut r, &v) });
    g.oil_target_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.3), Ext::from_f64(2.0)));
    g.gold_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(250.0), Ext::from_f64(2500.0)));
    g.gold_target_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(250.0), Ext::from_f64(2500.0)));
    g.silver_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(3.0), Ext::from_f64(40.0)));
    g.silver_target_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(3.0), Ext::from_f64(40.0)));
    g.metals_price_base = cast::<i32, _>(r.range(0, 2000));
    g.wheat_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(10.0)));
    g.corn_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.5), Ext::from_f64(8.0)));
    g.oil_windfall_tax_rate = cast::<i32, _>(pick(&mut r, &[0, 0, 5, 20]));
    g.carbon_tax_rate = pick(&mut r, &[0, 0, 3, 10]) as _;
    g.health_care_tax_rate = pick(&mut r, &[0, 0, 4, 10]) as _;
    g.holding_co_tax_rate = pick(&mut r, &[0, 0, 20]) as _;
    g.rand_seed = cast::<i32, _>(r.range(0, 32000));
    g.sel_entity = { let v = [0, 1, cast::<i32, _>(r.range(11, 130)), cast::<i32, _>(r.range(131, 1580)), cast::<i32, _>(r.range(131, 1580))]; pick(&mut r, &v) };
    g.sel_corp_name = g.ent_name[g.sel_entity.max(1)].clone();
    for (m, n) in ["", "January", __content.string(6796), "March", "April", "May", "June", "July", "August", __content.string(6797), "October", __content.string(6798), __content.string(6799)].iter().enumerate() {
        g.month_name[cast::<i32, _>(m)] = st(n);
    }
    for ind in 1..=71 {
        g.ind_long_term_growth[ind] = cast::<i16, _>({ let v = [r.range(-3, 3), r.range(0, 8), r.range(5, 20)]; pick(&mut r, &v) });
        g.ind_growth_rate[ind] = cast::<i16, _>({ let v = [r.range(-10, 30), r.range(-2, 3), cast::<i64, _>(g.ind_long_term_growth[ind]) + r.range(-8, 8)]; pick(&mut r, &v) });
        g.ind_growth_forecast[ind] = cast::<i16, _>(cast::<i64, _>(g.ind_growth_rate[ind]) + r.range(-12, 12));
    }
    g
}
