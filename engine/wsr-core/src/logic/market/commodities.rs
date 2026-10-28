//! The quarterly/tick commodity market move `UpdateCommodityPrices` (0x62a6a2, error context
//! "MoveCommodities") with its GOSUB `UpdateCommodityPrices_AdjustBasis` (0x62c92c).
//!
//! asm: the decompiled view of this routine is unreliable in many places (the `UpdateMetalsPriceBase`
//! comparisons show up as `extraout_ST1`, i.e. the RND expressions saved across the call with
//! PB_FpuSave are lost; several branches, single-vs-extended operations and the GOSUB's comparisons are
//! wrong). The port follows the machine code. All non-integer constants are SINGLE (`fmul dword`).
use crate::pb::real::{Ext, cast};


use crate::logic::futures::{calc_futures_basis, set_commodity_price_ranges, update_metals_price_base};
use crate::logic::news::post_news;
use crate::logic::valuation::{update_main_commodity_display, update_price_history_tick};
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{CaseMode, lcase};
use crate::pb::numeric::to_quad;
use crate::state::Globals;
use crate::ui::Ui;

/// A SINGLE constant of the original, widened.
fn s(x: f32) -> Ext {
    cast::<Ext, _>(x)
}

/// The common start of every price move: `delta = (target - price) * (0.25 * RND)`, negated with
/// probability 1/5, and when `RandSeed MOD modulus = 1` quadrupled and negated with probability 1/2.
fn random_delta(g: &Globals, rng: &mut Rng, gap: Ext, modulus: i32) -> Ext {
    let mut delta = gap * (s(0.25f32) * rng.rnd());
    if Ext::from_f64(5.0) * rng.rnd() < Ext::from_f64(1.0) {
        delta = -delta;
    }
    if g.rand_seed % modulus == 1 {
        delta = Ext::from_f64(4.0) * delta;
        if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
            delta = -delta;
        }
    }
    delta
}

/// The clamp of the SINGLE-priced commodities (gold, silver, wheat, corn): `|delta|` at most
/// 6% of the price (set to `-neg_factor * price` / `price * 0.065`), and no fall below zero.
fn clamp_delta(price: f32, mut delta: Ext, neg_factor: f32) -> Ext {
    let p = cast::<Ext, _>(price);
    if delta.abs() > p * s(0.06f32) {
        if Ext::from_f64(0.0) >= delta {
            delta = -s(neg_factor) * p;
        } else {
            delta = p * s(0.065f32);
        }
    }
    if delta.abs() > p && Ext::from_f64(0.0) > delta {
        delta = Ext::from_f64(0.0);
    }
    delta
}

/// Port of UpdateCommodityPrices (0x62a6a2).
/// Moves the five commodity spot prices ( [runtime resource wording] corn) a random step towards their
/// targets, drifts the targets and min/max bands (gold/silver relative to `MetalsPriceBase`, gold also
/// with interest rates, GDP and the pandemic, oil with `PeakOilCrisis`), adjusts the futures bases
/// (GOSUB AdjustBasis), may post a "backwardation" headline, records the price-history ticks of the
/// commodities, rates and the stock index, and refreshes the main commodity display (state part).
pub fn update_commodity_prices(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    // ON ERROR → ShowRuntimeError "MoveCommodities" (ignored).
    g.oil_price = cast::<f32, _>(g.oil_price.abs());
    g.gold_price = cast::<f32, _>(g.gold_price.abs());
    g.silver_price = cast::<f32, _>(g.silver_price.abs());
    g.wheat_price = cast::<f32, _>(g.wheat_price.abs());
    g.corn_price = cast::<f32, _>(g.corn_price.abs());
    g.bitcoin_price = g.bitcoin_price.wrapping_abs();
    g.ethereum_price = g.ethereum_price.wrapping_abs();
    if g.gold_target_price == Ext::from_f32(0.0f32) {
        set_commodity_price_ranges(g, ui, rng);
    }

    // ---- oil (in cents)
    let mut oil_target: Ext = cast::<Ext, _>(g.oil_target_price) * Ext::from_f64(100.0);
    let mut oil_price: Ext = cast::<Ext, _>(g.oil_price) * Ext::from_f64(100.0);
    let mut delta = random_delta(g, rng, oil_target - oil_price, 16);
    if delta.abs() > oil_price * s(0.07f32) {
        if Ext::from_f64(0.0) >= delta {
            delta = -s(0.07f32) * oil_price;
        } else {
            delta = oil_price * s(0.075f32);
        }
    }
    oil_price = delta + oil_price;
    if g.peak_oil_crisis == 0 {
        if Ext::from_f64(92.0) < oil_target && -s(1.2f32) > cast::<Ext, _>(g.gdp_growth) {
            oil_target = s(0.9f32) * oil_target;
        } else if Ext::from_f64(111.0) > oil_target && s(4.2f32) < cast::<Ext, _>(g.gdp_growth) {
            oil_target = s(1.1f32) * oil_target;
        }
    }
    if g.peak_oil_crisis < 1 && Ext::from_f64(50.0) * rng.rnd() + Ext::from_f64(200.0) < oil_target {
        oil_target = s(0.85f32) * oil_target;
        if Ext::from_f64(210.0) < oil_price {
            oil_price = oil_price - Ext::from_f64(17.0) * rng.rnd();
        }
    }
    if Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(10.0) > oil_target {
        oil_target = Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(10.0);
    }
    if Ext::from_f64(10.0) > oil_price {
        oil_price = Ext::from_f64(6.0) * rng.rnd() + Ext::from_f64(10.0);
    }
    if Ext::from_f64(7.0) * rng.rnd() < Ext::from_f64(1.0) {
        if Ext::from_f64(20.0) * rng.rnd() + Ext::from_f64(120.0) < oil_target {
            oil_target = s(0.95f32) * oil_target;
        }
        if Ext::from_f64(25.0) * rng.rnd() + Ext::from_f64(50.0) > oil_target {
            oil_target = s(1.05f32) * oil_target;
        }
    }
    if Ext::from_f64(35.0) * rng.rnd() > Ext::from_f64(34.0) {
        let a = oil_price * s(0.25f32);
        delta = oil_price * (s(0.55f32) * rng.rnd()) - a;
        oil_target = delta + oil_target;
    }
    g.oil_price = cast::<f32, _>(cast::<f32, _>(oil_price * s(0.01f32)));
    g.oil_target_price = cast::<f32, _>(cast::<f32, _>(oil_target * s(0.01f32)));
    if s(0.15f32) > cast::<Ext, _>(g.oil_price) {
        g.oil_price = cast::<f32, _>(cast::<f32, _>(s(0.06f32) * rng.rnd() + s(0.13f32)));
    }
    if g.peak_oil_crisis == 1 && g.futures_basis[1] < 280 {
        g.futures_basis[1] = cast::<i16, _>(cast::<i32, _>(g.futures_basis[1]) + 40);
    }
    // Bug (kept): tests FuturesBasis(1) but lowers FuturesBasis(2) (gold) — see BUGS.md.
    if g.peak_oil_crisis == -1
        && Ext::from_f64(0.0) < cast::<Ext, _>(g.oil_crisis_level)
        && s(1.6f32) < cast::<Ext, _>(g.oil_price)
        && g.futures_basis[1] > -50
    {
        g.futures_basis[2] = cast::<i16, _>(cast::<i32, _>(g.futures_basis[2]) - 50);
    }
    if g.oil_crisis_level == Ext::from_f32(0.0f32) && g.peak_oil_crisis < 1 {
        adjust_basis(g, rng, 1);
    }

    // ---- gold
    let gap = cast::<Ext, _>(g.gold_target_price) - cast::<Ext, _>(g.gold_price);
    delta = random_delta(g, rng, gap, 19);
    delta = clamp_delta(g.gold_price, delta, 0.06f32);
    g.gold_price = cast::<f32, _>(cast::<f32, _>(delta + cast::<Ext, _>(g.gold_price)));
    // `base` is read before the first UpdateMetalsPriceBase call (which may change the base).
    let base = cast::<Ext, _>(g.metals_price_base);
    if g.pandemic_state <= 0 {
        if g.gold_price_max < g.gold_target_price {
            g.gold_target_price = cast::<f32, _>(cast::<f32, _>(s(0.97f32) * cast::<Ext, _>(g.gold_price_max)));
        }
        if g.gold_price_max < g.gold_price {
            let gp = cast::<Ext, _>(g.gold_price);
            g.gold_price = cast::<f32, _>(cast::<f32, _>(gp - s(0.2f32) * (gp - cast::<Ext, _>(g.gold_price_max))));
        }
        if base + Ext::from_f64(2250.0) > cast::<Ext, _>(g.gold_price_min) {
            g.gold_price_min = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(2000.0) * rng.rnd() + Ext::from_f64(2000.0) + base));
        }
        if g.gold_price_max <= g.gold_price_min {
            g.gold_price_max = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gold_price_min) * s(1.1f32)));
        }
        if g.gold_price_min > g.gold_target_price {
            g.gold_target_price = cast::<f32, _>(cast::<f32, _>(s(1.03f32) * cast::<Ext, _>(g.gold_price_min)));
        }
        if g.gold_price_min > g.gold_price {
            g.gold_price = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(55.0) * rng.rnd() + cast::<Ext, _>(g.gold_price_min)));
        }
        if Ext::from_f64(7.0) * rng.rnd() < Ext::from_f64(1.0) {
            if Ext::from_f64(2000.0) * rng.rnd() + Ext::from_f64(10500.0) + base < cast::<Ext, _>(g.gold_target_price) {
                g.gold_target_price = cast::<f32, _>(cast::<f32, _>(s(0.95f32) * cast::<Ext, _>(g.gold_target_price)));
            }
            if Ext::from_f64(1750.0) * rng.rnd() + Ext::from_f64(4250.0) + base > cast::<Ext, _>(g.gold_target_price) {
                g.gold_target_price = cast::<f32, _>(cast::<f32, _>(s(1.05f32) * cast::<Ext, _>(g.gold_target_price)));
            }
        }
        if base + Ext::from_f64(2000.0) > cast::<Ext, _>(g.gold_target_price) {
            g.gold_target_price = cast::<f32, _>(cast::<f32, _>(base + Ext::from_f64(2000.0)));
        } else if base + Ext::from_f64(21000.0) < cast::<Ext, _>(g.gold_target_price) {
            g.gold_target_price = cast::<f32, _>(cast::<f32, _>(base + Ext::from_f64(21000.0)));
        }
        if g.gold_price_max < g.gold_target_price {
            g.gold_price_max = cast::<f32, _>(cast::<f32, _>(s(1.1f32) * cast::<Ext, _>(g.gold_target_price)));
        }
        if g.gold_price_min > g.gold_target_price {
            g.gold_price_min = cast::<f32, _>(cast::<f32, _>(s(0.9f32) * cast::<Ext, _>(g.gold_target_price)));
        }
        // The RND expression is drawn before the call (kept on the FPU stack across it).
        let x = Ext::from_f64(2000.0) * rng.rnd() + Ext::from_f64(8000.0);
        let m = update_metals_price_base(g, ui);
        if m > x
            && (cast::<Ext, _>(g.base_interest_rate)) < cast::<Ext, _>(g.interest_rate_target)
            && s(6.2f32) < cast::<Ext, _>(g.base_interest_rate)
            && -Ext::from_f64(1.0) < cast::<Ext, _>(g.gdp_growth)
        {
            g.gold_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gold_price) - Ext::from_f64(50.0)));
        } else {
            let x = Ext::from_f64(3000.0) * rng.rnd() + Ext::from_f64(5000.0);
            let m = update_metals_price_base(g, ui);
            if m < x && s(3.5f32) > cast::<Ext, _>(g.interest_rate_target) && s(3.9f32) > cast::<Ext, _>(g.base_interest_rate) {
                g.gold_price = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(50.0) + cast::<Ext, _>(g.gold_price)));
                if -Ext::from_f64(5.0) > cast::<Ext, _>(g.gdp_growth) && cast::<Ext, _>(g.metals_price_base) + Ext::from_f64(14500.0) > cast::<Ext, _>(g.gold_target_price) {
                    g.gold_target_price = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(125.0) + cast::<Ext, _>(g.gold_target_price)));
                }
            }
        }
        if -Ext::from_f64(9.0) > cast::<Ext, _>(g.gdp_growth) {
            let x = Ext::from_f64(2500.0) * rng.rnd() + Ext::from_f64(17500.0);
            let m = update_metals_price_base(g, ui);
            if m < x && g.cur_year % 5 > 2 {
                g.gold_target_price = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(750.0) + cast::<Ext, _>(g.gold_target_price)));
            }
        }
        let x = Ext::from_f64(400.0) * rng.rnd() + Ext::from_f64(3000.0);
        let m = update_metals_price_base(g, ui);
        if m < x {
            g.gold_price = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(35.0) + cast::<Ext, _>(g.gold_price)));
            if g.gold_price > g.gold_target_price {
                g.gold_target_price = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(25.0) + cast::<Ext, _>(g.gold_target_price)));
            }
        } else {
            let x = Ext::from_f64(2700.0) * rng.rnd() + Ext::from_f64(8800.0);
            let m = update_metals_price_base(g, ui);
            if m > x {
                g.gold_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gold_price) - Ext::from_f64(35.0)));
                if g.gold_price < g.gold_target_price {
                    g.gold_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gold_target_price) - Ext::from_f64(25.0)));
                }
            }
        }
        if Ext::from_f64(35.0) * rng.rnd() > Ext::from_f64(34.0) {
            let gp = cast::<Ext, _>(g.gold_price);
            let a = gp * s(0.2f32);
            delta = gp * (s(0.45f32) * rng.rnd()) - a;
            g.gold_target_price = cast::<f32, _>(cast::<f32, _>(delta + cast::<Ext, _>(g.gold_target_price)));
        }
        adjust_basis(g, rng, 2);
    } else {
        g.gold_price_max = cast::<f32, _>(cast::<f32, _>(base + Ext::from_f64(25000.0)));
        let x = Ext::from_f64(9000.0) * rng.rnd() + Ext::from_f64(16000.0);
        let m = update_metals_price_base(g, ui);
        if m < x {
            g.gold_target_price = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(9000.0) * rng.rnd() + Ext::from_f64(16000.0) + cast::<Ext, _>(g.metals_price_base)));
        }
    }

    // ---- silver (bands relative to 1% of MetalsPriceBase; re-read after UpdateMetalsPriceBase)
    let sb = |g: &Globals| cast::<Ext, _>(g.metals_price_base) * s(0.01f32);
    g.silver_price_min = cast::<f32, _>(cast::<f32, _>(sb(g) + Ext::from_f64(4.0)));
    g.silver_price_max = cast::<f32, _>(cast::<f32, _>(sb(g) + Ext::from_f64(65.0)));
    let gap = cast::<Ext, _>(g.silver_target_price) - cast::<Ext, _>(g.silver_price);
    delta = random_delta(g, rng, gap, 18);
    delta = clamp_delta(g.silver_price, delta, 0.06f32);
    g.silver_price = cast::<f32, _>(cast::<f32, _>(delta + cast::<Ext, _>(g.silver_price)));
    if g.silver_price_max < g.silver_target_price {
        g.silver_target_price = cast::<f32, _>(cast::<f32, _>(s(0.97f32) * cast::<Ext, _>(g.silver_price_max)));
    }
    if g.silver_price_max < g.silver_price {
        g.silver_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.silver_price) * s(0.95f32)));
    }
    if g.silver_price_max <= g.silver_price_min {
        g.silver_price_max = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.silver_price_min) * s(1.1f32)));
    }
    if g.silver_price_min > g.silver_target_price {
        g.silver_target_price = cast::<f32, _>(cast::<f32, _>(s(1.03f32) * cast::<Ext, _>(g.silver_price_min)));
    }
    if g.silver_price_min > g.silver_price {
        g.silver_price = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(1.0) * rng.rnd() + cast::<Ext, _>(g.silver_price_min)));
    }
    if Ext::from_f64(7.0) * rng.rnd() < Ext::from_f64(1.0) {
        let lhs = sb(g) + Ext::from_f64(125.0);
        if lhs + Ext::from_f64(25.0) * rng.rnd() < cast::<Ext, _>(g.silver_target_price) {
            g.silver_target_price = cast::<f32, _>(cast::<f32, _>(s(0.95f32) * cast::<Ext, _>(g.silver_target_price)));
        }
        let lhs = sb(g) + Ext::from_f64(40.0);
        if lhs + Ext::from_f64(25.0) * rng.rnd() > cast::<Ext, _>(g.silver_target_price) {
            g.silver_target_price = cast::<f32, _>(cast::<f32, _>(s(1.05f32) * cast::<Ext, _>(g.silver_target_price)));
        }
    }
    let lhs = sb(g) + Ext::from_f64(120.0);
    if lhs + Ext::from_f64(40.0) * rng.rnd() < cast::<Ext, _>(g.silver_price)
        && (cast::<Ext, _>(g.base_interest_rate)) < cast::<Ext, _>(g.interest_rate_target)
        && s(7.1f32) < cast::<Ext, _>(g.base_interest_rate)
    {
        g.silver_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.silver_target_price) - Ext::from_f64(5.0)));
    } else {
        let lhs = sb(g) + Ext::from_f64(60.0);
        if lhs + Ext::from_f64(40.0) * rng.rnd() > cast::<Ext, _>(g.silver_price)
            && cast::<Ext, _>(g.base_interest_rate) > cast::<Ext, _>(g.interest_rate_target)
            && s(4.2f32) > cast::<Ext, _>(g.base_interest_rate)
        {
            g.silver_target_price = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(5.0) + cast::<Ext, _>(g.silver_target_price)));
        }
    }
    let lhs = sb(g) + Ext::from_f64(70.0);
    if lhs + Ext::from_f64(50.0) * rng.rnd() < cast::<Ext, _>(g.silver_price) && Ext::from_f64(1.0) > cast::<Ext, _>(g.gdp_growth) {
        g.silver_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.silver_price) - Ext::from_f64(5.0)));
        if -Ext::from_f64(2.0) > cast::<Ext, _>(g.gdp_growth) {
            g.silver_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.silver_target_price) - Ext::from_f64(1.0)));
        }
    } else {
        let lhs = sb(g) + Ext::from_f64(150.0);
        if lhs + Ext::from_f64(50.0) * rng.rnd() > cast::<Ext, _>(g.silver_price) && s(5.3f32) < cast::<Ext, _>(g.gdp_growth) {
            g.silver_price = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(5.0) + cast::<Ext, _>(g.silver_price)));
            g.silver_target_price = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(5.0) + cast::<Ext, _>(g.silver_target_price)));
        }
    }
    let lhs = sb(g) + Ext::from_f64(50.0);
    if lhs + Ext::from_f64(75.0) * rng.rnd() > cast::<Ext, _>(g.silver_price) && -Ext::from_f64(9.0) > cast::<Ext, _>(g.gdp_growth) && g.cur_year % 5 > 2 {
        g.silver_target_price = cast::<f32, _>(cast::<f32, _>((Ext::from_f64(1.0) * rng.rnd() + Ext::from_f64(1.0)) + cast::<Ext, _>(g.silver_target_price)));
    }
    if sb(g) + s(42.5f32) > cast::<Ext, _>(g.silver_price) {
        g.silver_price = cast::<f32, _>(cast::<f32, _>(s(0.5f32) + cast::<Ext, _>(g.silver_price)));
        if g.silver_price > g.silver_target_price {
            g.silver_target_price = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(5.0) + cast::<Ext, _>(g.silver_target_price)));
        }
    } else if sb(g) + Ext::from_f64(170.0) < cast::<Ext, _>(g.silver_price) {
        g.silver_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.silver_price) - s(0.5f32)));
        if g.silver_price < g.silver_target_price {
            g.silver_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.silver_target_price) - Ext::from_f64(5.0)));
        }
    }
    if Ext::from_f64(195.0) * rng.rnd() > Ext::from_f64(194.0) {
        let sp = cast::<Ext, _>(g.silver_price);
        let a = sp * s(0.3f32);
        delta = a + sp * (s(0.2f32) * rng.rnd());
        if Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(2.0) {
            delta = -delta;
        }
        g.silver_target_price = cast::<f32, _>(cast::<f32, _>(delta + cast::<Ext, _>(g.silver_target_price)));
        g.silver_price = cast::<f32, _>(cast::<f32, _>(delta * s(0.8f32) + cast::<Ext, _>(g.silver_price)));
        if g.rand_seed % 2 == 1 {
            g.gold_target_price = cast::<f32, _>(cast::<f32, _>(delta * Ext::from_f64(100.0) + cast::<Ext, _>(g.gold_target_price)));
            g.gold_price = cast::<f32, _>(cast::<f32, _>(delta * Ext::from_f64(60.0) + cast::<Ext, _>(g.gold_price)));
        }
        // (UI) BEEP
    }
    adjust_basis(g, rng, 3);

    // ---- wheat
    let gap = cast::<Ext, _>(g.wheat_target_price) - cast::<Ext, _>(g.wheat_price);
    delta = random_delta(g, rng, gap, 17);
    delta = clamp_delta(g.wheat_price, delta, 0.065f32);
    g.wheat_price = cast::<f32, _>(cast::<f32, _>(delta + cast::<Ext, _>(g.wheat_price)));
    if g.wheat_price_max < g.wheat_target_price {
        g.wheat_target_price = cast::<f32, _>(cast::<f32, _>(s(0.97f32) * cast::<Ext, _>(g.wheat_price_max)));
    }
    if g.wheat_price_max < g.wheat_price {
        let wp = cast::<Ext, _>(g.wheat_price);
        g.wheat_price = cast::<f32, _>(cast::<f32, _>(wp - s(0.25f32) * (wp - cast::<Ext, _>(g.wheat_price_max))));
    }
    if s(1.9f32) > cast::<Ext, _>(g.wheat_price_min) {
        g.wheat_price_min = cast::<f32, _>(cast::<f32, _>(s(0.6f32) * rng.rnd() + s(1.9f32)));
    }
    if g.wheat_price_max <= g.wheat_price_min {
        g.wheat_price_max = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.wheat_price_min) * s(1.1f32)));
    }
    if g.wheat_price_min > g.wheat_target_price {
        g.wheat_target_price = cast::<f32, _>(cast::<f32, _>(s(1.03f32) * cast::<Ext, _>(g.wheat_price_min)));
    }
    if g.wheat_price_min > g.wheat_price {
        g.wheat_price = cast::<f32, _>(cast::<f32, _>(s(0.5f32) * rng.rnd() + cast::<Ext, _>(g.wheat_price_min)));
    }
    if Ext::from_f64(7.0) * rng.rnd() < Ext::from_f64(1.0) {
        if Ext::from_f64(3.0) * rng.rnd() + Ext::from_f64(8.0) < cast::<Ext, _>(g.wheat_target_price) {
            g.wheat_target_price = cast::<f32, _>(cast::<f32, _>(s(0.95f32) * cast::<Ext, _>(g.wheat_target_price)));
        }
        if s(1.5f32) * rng.rnd() + s(2.5f32) > cast::<Ext, _>(g.wheat_target_price) {
            g.wheat_target_price = cast::<f32, _>(cast::<f32, _>(s(1.05f32) * cast::<Ext, _>(g.wheat_target_price)));
        }
    }
    if s(1.9f32) > cast::<Ext, _>(g.wheat_target_price) {
        g.wheat_target_price = cast::<f32, _>(Ext::from_f32(1.9f32));
    } else if Ext::from_f32(18.0f32) < g.wheat_target_price {
        g.wheat_target_price = cast::<f32, _>(Ext::from_f32(18.0f32));
    }
    if g.wheat_price_max < g.wheat_target_price {
        g.wheat_price_max = cast::<f32, _>(cast::<f32, _>(s(1.1f32) * cast::<Ext, _>(g.wheat_target_price)));
    }
    if g.wheat_price_min > g.wheat_target_price {
        g.wheat_price_min = cast::<f32, _>(cast::<f32, _>(s(0.9f32) * cast::<Ext, _>(g.wheat_target_price)));
    }
    if s(2.4f32) >= cast::<Ext, _>(g.wheat_price) {
        let a = s(0.02f32) + cast::<Ext, _>(g.wheat_price);
        g.wheat_price = cast::<f32, _>(cast::<f32, _>(a + s(0.07f32) * rng.rnd()));
        if g.wheat_price > g.wheat_target_price {
            g.wheat_target_price = cast::<f32, _>(cast::<f32, _>(s(0.3f32) + cast::<Ext, _>(g.wheat_target_price)));
        }
    } else if Ext::from_f32(13.0f32) < g.wheat_price {
        g.wheat_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.wheat_price) - (s(0.07f32) * rng.rnd() + s(0.03f32))));
        if g.wheat_price < g.wheat_target_price {
            g.wheat_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.wheat_target_price) - s(0.5f32)));
        }
    }
    if s(0.3f32) * rng.rnd() + s(1.2f32) < cast::<Ext, _>(g.oil_price) && Ext::from_f64(2.0) * rng.rnd() + Ext::from_f64(5.0) > cast::<Ext, _>(g.wheat_target_price) {
        g.wheat_target_price = cast::<f32, _>(cast::<f32, _>(s(0.35f32) + cast::<Ext, _>(g.wheat_target_price)));
    }
    if s(0.15f32) * rng.rnd() + s(0.6f32) > cast::<Ext, _>(g.oil_price) && Ext::from_f64(3.0) * rng.rnd() + Ext::from_f64(6.0) < cast::<Ext, _>(g.wheat_target_price) {
        g.wheat_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.wheat_target_price) - s(0.33f32)));
    }
    if Ext::from_f64(35.0) * rng.rnd() > Ext::from_f64(34.0) {
        let wp = cast::<Ext, _>(g.wheat_price);
        let a = wp * s(0.25f32);
        delta = wp * (s(0.6f32) * rng.rnd()) - a;
        g.wheat_target_price = cast::<f32, _>(cast::<f32, _>(delta + cast::<Ext, _>(g.wheat_target_price)));
    }
    adjust_basis(g, rng, 4);

    // ---- corn
    let gap = cast::<Ext, _>(g.corn_target_price) - cast::<Ext, _>(g.corn_price);
    delta = random_delta(g, rng, gap, 20);
    delta = clamp_delta(g.corn_price, delta, 0.065f32);
    g.corn_price = cast::<f32, _>(cast::<f32, _>(delta + cast::<Ext, _>(g.corn_price)));
    if g.corn_price_max < g.corn_target_price {
        g.corn_target_price = cast::<f32, _>(cast::<f32, _>(s(0.97f32) * cast::<Ext, _>(g.corn_price_max)));
    }
    if g.corn_price_max < g.corn_price {
        let cp = cast::<Ext, _>(g.corn_price);
        g.corn_price = cast::<f32, _>(cast::<f32, _>(cp - s(0.25f32) * (cp - cast::<Ext, _>(g.corn_price_max))));
    }
    if s(1.75f32) > cast::<Ext, _>(g.corn_price_min) {
        g.corn_price_min = cast::<f32, _>(cast::<f32, _>(s(0.8f32) * rng.rnd() + s(1.75f32)));
    }
    if g.corn_price_max <= g.corn_price_min {
        g.corn_price_max = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.corn_price_min) * s(1.12f32)));
    }
    if g.corn_price_min > g.corn_target_price {
        g.corn_target_price = cast::<f32, _>(cast::<f32, _>(s(1.03f32) * cast::<Ext, _>(g.corn_price_min)));
    }
    if g.corn_price_min > g.corn_price {
        g.corn_price = cast::<f32, _>(cast::<f32, _>(s(0.5f32) * rng.rnd() + cast::<Ext, _>(g.corn_price_min)));
    }
    if Ext::from_f64(7.0) * rng.rnd() < Ext::from_f64(1.0) {
        if s(3.5f32) * rng.rnd() + s(7.5f32) < cast::<Ext, _>(g.corn_target_price) {
            g.corn_target_price = cast::<f32, _>(cast::<f32, _>(s(0.95f32) * cast::<Ext, _>(g.corn_target_price)));
        }
        if s(1.5f32) * rng.rnd() + s(2.5f32) > cast::<Ext, _>(g.corn_target_price) {
            g.corn_target_price = cast::<f32, _>(cast::<f32, _>(s(1.05f32) * cast::<Ext, _>(g.corn_target_price)));
        }
    }
    if s(1.75f32) > cast::<Ext, _>(g.corn_target_price) {
        g.corn_target_price = cast::<f32, _>(Ext::from_f32(1.75f32));
    } else if Ext::from_f32(17.0f32) < g.corn_target_price {
        g.corn_target_price = cast::<f32, _>(Ext::from_f32(17.0f32));
    }
    if g.corn_price_max < g.corn_target_price {
        g.corn_price_max = cast::<f32, _>(cast::<f32, _>(s(1.1f32) * cast::<Ext, _>(g.corn_target_price)));
    }
    if g.corn_price_min > g.corn_target_price {
        g.corn_price_min = cast::<f32, _>(cast::<f32, _>(s(0.9f32) * cast::<Ext, _>(g.corn_target_price)));
    }
    if s(2.5f32) > cast::<Ext, _>(g.corn_price) {
        let a = s(0.02f32) + cast::<Ext, _>(g.corn_price);
        g.corn_price = cast::<f32, _>(cast::<f32, _>(a + s(0.07f32) * rng.rnd()));
        if g.corn_price > g.corn_target_price {
            g.corn_target_price = cast::<f32, _>(cast::<f32, _>(s(0.3f32) + cast::<Ext, _>(g.corn_target_price)));
        }
    } else if Ext::from_f32(13.0f32) < g.corn_price {
        g.corn_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.corn_price) - (s(0.07f32) * rng.rnd() + s(0.03f32))));
        if g.corn_price < g.corn_target_price {
            g.corn_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.corn_target_price) - s(0.5f32)));
        }
    }
    if cast::<Ext, _>(g.wheat_price) * s(0.63f32) > cast::<Ext, _>(g.corn_price) {
        g.corn_price = cast::<f32, _>(cast::<f32, _>(s(0.7f32) + cast::<Ext, _>(g.corn_price)));
    }
    if cast::<Ext, _>(g.wheat_target_price) * s(0.63f32) > cast::<Ext, _>(g.corn_target_price) {
        g.corn_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.wheat_target_price) * s(0.67f32)));
    }
    if cast::<Ext, _>(g.wheat_price) * s(1.3f32) < cast::<Ext, _>(g.corn_price) {
        g.corn_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.corn_price) - s(0.5f32)));
    }
    if cast::<Ext, _>(g.wheat_target_price) * s(1.13f32) < cast::<Ext, _>(g.corn_target_price) {
        g.corn_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.wheat_target_price) * s(1.07f32)));
    }
    if Ext::from_f64(35.0) * rng.rnd() > Ext::from_f64(34.0) {
        let cp = cast::<Ext, _>(g.corn_price);
        let a = cp * s(0.25f32);
        delta = cp * (s(0.55f32) * rng.rnd()) - a;
        g.corn_target_price = cast::<f32, _>(cast::<f32, _>(delta + cast::<Ext, _>(g.corn_target_price)));
    }
    adjust_basis(g, rng, 5);
    adjust_basis(g, rng, 6);

    // ---- futures basis limits and the backwardation headline
    for idx in 1..=6 {
        g.futures_basis[idx] = g.futures_basis[idx].clamp(-100, 300);
        if (idx == 2 || idx == 3) && g.futures_basis[idx] < 5 {
            g.futures_basis[idx] = cast::<i16, _>(to_quad(Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(2.0)));
        }
        if g.futures_basis[6] < -30 {
            g.futures_basis[6] = -30;
        }
        if g.futures_basis[idx] < 0 && Ext::from_f64(5.0) * rng.rnd() > Ext::from_f64(4.0) && g.sim_co_count > 1530 {
            let mut commod_lc = lcase(g.commodity_name[idx].as_bytes(), CaseMode::Ansi);
            if commod_lc == "stocks" {
                commod_lc = PbStr::from(__content.string(2547));
            }
            g.news_msg = PbStr::from(__content.string(6627)) + &commod_lc + __content.string(6628);
        }
    }
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    for idx in 6..=10 {
        update_price_history_tick(g, ui, rng, idx);
    }
    for idx in 1601..=1606 {
        update_price_history_tick(g, ui, rng, idx);
    }
    update_price_history_tick(g, ui, rng, 0);
    update_main_commodity_display(g);
    // EmptyStub_62cb79: no-op.
}

/// GOSUB UpdateCommodityPrices_AdjustBasis (0x62c92c): moves `FuturesBasis(commod)` towards
/// CalcFuturesBasis: up by 10..15 if below it (snapping to it when `RandSeed MOD 10 = 7`), down by
/// 10..15 if above (snapping when `RandSeed MOD 10 = 1`), and by +10..-10 at random when equal.
fn adjust_basis(g: &mut Globals, rng: &mut Rng, commod: i32) {
    let calc_basis = calc_futures_basis(g, rng, commod);
    let c = commod;
    let fb = cast::<Ext, _>(g.futures_basis[c]);
    if fb < calc_basis {
        let a = Ext::from_f64(10.0) + fb;
        g.futures_basis[c] = cast::<i16, _>(to_quad(a + Ext::from_f64(5.0) * rng.rnd()));
        if g.rand_seed % 10 == 7 {
            g.futures_basis[c] = cast::<i16, _>(to_quad(calc_basis));
        }
    } else if calc_basis < fb {
        let a = Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(10.0);
        g.futures_basis[c] = cast::<i16, _>(to_quad(fb - a));
        if g.rand_seed % 10 == 1 {
            g.futures_basis[c] = cast::<i16, _>(to_quad(calc_basis));
        }
    } else {
        let a = Ext::from_f64(20.0) * rng.rnd();
        g.futures_basis[c] = cast::<i16, _>(to_quad((Ext::from_f64(10.0) + fb) - a));
    }
}
