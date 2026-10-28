//! Stock options: valuation (`CalcOptionPrice` 0x603c8e and its GOSUBs, `CalcSpreadFactor`,
//! `CalcOptionsValue`, `CalcVestedOptionsValue`, `GetPlrNakedLiab`), position queries (`GetOptionedStockPct`,
//! `GetNetStockPctAfterOptions`, `GetMaxOptionsPct`), record maintenance (`AddOrRemoveOptionRec`,
//! `MergeMatchingOptionRec`, `SortOptionRecs`, `TransferOptionsHolder`) and closing/settling
//! (`SettleOptionRec`, `CloseOptionPositions`, `CheckNakedOptionMargin`).
//!
//! Records live in `OptionRecs(1..1000)` (element 0 = count as text), `"|"`-delimited:
//! 1 = underlying corp, 2 = type (1 call, 2 put), 3 = holder, 4 = contracts in % of the shares (negative =
//! written/short), 5 = strike (in `MarketCap` units, i.e. price x shares), 6 = expiry month, 7 = expiry year,
//! 8 = premium paid (negative for written options) or `GRANT` / `VESTED` for executive options.
//!
//! Numeric conventions as in `finance.rs`: EXT is `f64`; SINGLE constants of the original
//! (`fmul dword [0x75e…]`) are written `0.8f32 as f64`; SINGLE/DOUBLE stores are rounded accordingly;
//! CURRENCYX locals use [`Cux`]. The association order of float expressions follows the machine code.
//! Comments marked "asm:" record where the port follows the machine code instead of the decompiled view.

// Statements keep the original BASIC shape (`x = x + y`, `a < 1 OR 5 < a`) on purpose; negated float
// comparisons (`!(a > b)`) mirror the x87 branch taken for unordered (NaN) operands.
#![allow(
    clippy::manual_range_contains,
    clippy::assign_op_pattern,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::too_many_arguments,
    clippy::manual_clamp,
    clippy::neg_multiply,
    clippy::needless_late_init
)]
use crate::pb::real::{Ext, cast};


use crate::logic::control::find_option_positions;
use crate::logic::earnings::{balance_cash_t_bills, calc_cur_month_of_year};
use crate::logic::finance::{calc_entity_financials, get_managing_player};
use crate::logic::holdings::{find_holding_slot, find_stock_holding, pack_holding_arrays_with_news as pack_holding_arrays};
use crate::logic::news::post_news;
use crate::logic::report::show_quick_message;
use crate::logic::valuation::{calc_dividend_yield, calc_stock_volatility, check_stock_split};
use crate::pb::numeric::format;
use crate::pb::ext80::Ext80;
use crate::pb::numeric::{str_num, str_num_ext, val, val_ext};
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{instr, ltrim, parse_any, parse_count_any, rtrim};
use crate::pb::numeric::{fix, frac, sqr, to_long, to_quad, Cux};
use crate::state::Globals;
use crate::ui::Ui;

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

/// `PARSE$(rec, ANY "|", n)`.
fn field(rec: &PbStr, n: i32) -> PbStr {
    parse_any(rec.as_bytes(), b"|", n)
}

/// `VAL(PARSE$(rec, ANY "|", n))`.
fn fval(rec: &PbStr, n: i32) -> Ext {
    val(field(rec, n).as_bytes())
}

/// `VAL(PARSE$(OptionRecs(i), ANY "|", n))`.
fn rec_val(g: &Globals, i: i32, n: i32) -> Ext {
    fval(&g.option_recs[i], n)
}

/// `STR$(x)` (PB_StrNum, 15 digits).
fn str15(x: Ext) -> PbStr {
    PbStr::from(str_num(x, 15).as_str())
}

/// `STR$(x)` of an 80-bit value.
fn str15_ext(x: Ext80) -> PbStr {
    PbStr::from(str_num_ext(x, 15).as_str())
}

/// `VAL(PARSE$(rec, ANY "|", n))` as the 80-bit result.
fn fval_ext(rec: &PbStr, n: i32) -> Ext80 {
    val_ext(field(rec, n).as_bytes())
}

/// `x = x + y` on an EXT global (stored as f64), the sum formed in 80 bits.
fn add_ext(x: &mut Ext, y: Ext80) {
    *x = Ext::from(Ext80::from_ext(*x).fadd(y).into_ext());
}

/// `x = x + y` on a QUAD (`fild; fadd; fistp`).
fn add_quad(x: &mut i64, y: Ext80) {
    *x = Ext80::from_i64(*x).fadd(y).to_i64();
}

/// `RTRIM$(LTRIM$(x))`.
fn trim(x: &PbStr) -> PbStr {
    rtrim(ltrim(x.as_bytes()).as_bytes())
}

/// A SINGLE constant of the original (`dword [0x75e…]`), widened to EXT.
fn f(x: f32) -> Ext {
    cast::<Ext, _>(x)
}

// ---------------------------------------------------------------------------------------------------------
// Valuation
// ---------------------------------------------------------------------------------------------------------

/// The locals of CalcOptionPrice, shared with its GOSUBs (zeroed at entry).
#[derive(Default)]
struct PriceFrame {
    stock_id: i32,
    opt_type: i32,
    strike: Ext,
    exp_month: i32,
    exp_year: i32,
    /// `[ebp-0xe4]` EXT: intrinsic value (0 when out of the money).
    in_money: Ext,
    /// `[ebp-0xf0]` EXT: 1 when more than 0.01 out of the money.
    out_of_money: Ext,
    /// `[ebp-0x168]` DOUBLE.
    pct_away: Ext,
    /// `[ebp-0x98]` DOUBLE.
    months_left: Ext,
    whole_months: i32,
    /// `[ebp-0x118]` DOUBLE.
    frac_month: Ext,
    /// `[ebp-0x124]` EXT.
    std_prem: Ext,
    /// `[ebp-0x104]` / `[ebp-0x170]` DOUBLE.
    pct_factor: Ext,
    factor_step: Ext,
    /// `[ebp-0xb4]` EXT.
    time_prem: Ext,
    /// `[ebp-0xc0]` EXT.
    div_adj: Ext,
    /// `[ebp-0x160]` LONG.
    loop_guard: i32,
}

/// One row of the premium table: base premium in % of the stock value (as a fraction), per-percent
/// factor for the distance from the money and its change at 15%; the variants for out-of-the-money
/// options (`None` = unchanged).
struct PremRow {
    coef: f32,
    pct_factor: f32,
    factor_step: f64,
    oom_pct_factor: Option<f32>,
    oom_factor_step: Option<f64>,
}

fn row(coef: f32, pf: f32, fs: f64, opf: Option<f32>, ofs: Option<f64>) -> PremRow {
    PremRow { coef, pct_factor: pf, factor_step: fs, oom_pct_factor: opf, oom_factor_step: ofs }
}

/// `-(x as f64)` for the negated SINGLE steps (`fld dword; fchs; fstp qword`).
fn neg(x: f32) -> f64 {
    -(x as f64)
}

/// CalcOptionPrice_PremiumTable rows for `wholeMonths` = 1..23 and > 23 (index 24); case 3 sets no step
/// (stays 0 from the frame).
#[allow(non_snake_case)]
fn PREM_ROWS(__content: &crate::content::ContentPack) -> [PremRow; 24] { [
    row(__content.single(14557), __content.single(14558), __content.single(14559) as f64, Some(__content.single(14560)), Some(__content.single(14561) as f64)),
    row(__content.single(14562), __content.single(14563), __content.single(14564) as f64, Some(__content.single(14565)), Some(__content.single(14561) as f64)),
    row(__content.single(14566), __content.single(14567), __content.single(14561) as f64, Some(__content.single(14568)), None),
    row(__content.single(14553), __content.single(14569), neg(__content.single(14570)), Some(__content.single(14571)), Some(neg(__content.single(14572)))),
    row(__content.single(14573), __content.single(14574), neg(__content.single(14559)), Some(__content.single(14575)), None),
    row(__content.single(14576), __content.single(14577), neg(__content.single(14578)), Some(__content.single(14579)), None),
    row(__content.single(14580), __content.single(14581), neg(__content.single(14559)), None, None),
    row(__content.single(14582), __content.single(14583), neg(__content.single(14559)), None, Some(neg(__content.single(14564)))),
    row(__content.single(14584), __content.single(14585), neg(__content.single(14559)), None, Some(neg(__content.single(14586)))),
    row(__content.single(14587), __content.single(14588), neg(__content.single(14559)), None, Some(__content.single(14561) as f64)),
    row(__content.single(14589), __content.single(14590), neg(__content.single(14591)), Some(__content.single(14592)), Some(neg(__content.single(14593)))),
    row(__content.single(14594), __content.single(14595), neg(__content.single(14564)), Some(__content.single(14596)), Some(neg(__content.single(14597)))),
    row(__content.single(14598), __content.single(14595), neg(__content.single(14578)), Some(__content.single(14599)), Some(neg(__content.single(14600)))),
    row(__content.single(14601), __content.single(14592), neg(__content.single(14602)), Some(__content.single(14603)), Some(neg(__content.single(14586)))),
    row(__content.single(14604), __content.single(14605), neg(__content.single(14559)), Some(__content.single(14606)), Some(neg(__content.single(14607)))),
    row(__content.single(14608), __content.single(14609), neg(__content.single(14610)), Some(__content.single(14611)), Some(neg(__content.single(14612)))),
    row(__content.single(14613), __content.single(14614), neg(__content.single(14615)), Some(__content.single(14606)), Some(neg(__content.single(14564)))),
    row(__content.single(14616), __content.single(14599), neg(__content.single(14559)), Some(__content.single(14617)), None),
    row(__content.single(14618), __content.single(14619), neg(__content.single(14620)), Some(__content.single(14621)), Some(neg(__content.single(14586)))),
    row(__content.single(14622), __content.single(14609), neg(__content.single(14623)), Some(__content.single(14621)), Some(neg(__content.single(14586)))),
    row(__content.single(14624), __content.single(14609), neg(__content.single(14625)), Some(__content.single(14626)), Some(neg(__content.single(14627)))),
    row(__content.single(14628), __content.single(14629), neg(__content.single(14630)), Some(__content.single(14631)), Some(neg(__content.single(14632)))),
    row(__content.single(14633), __content.single(14634), neg(__content.single(14578)), Some(__content.single(14635)), Some(neg(__content.single(14636)))),
    row(__content.single(14637), __content.single(14638), neg(__content.single(14600)), Some(__content.single(14639)), Some(neg(__content.single(14636)))),
] }

/// Port of CalcOptionPrice (0x603c8e) with its GOSUBs PremiumTable (0x604922), VolAdjust (0x605a9c),
/// RateAdjust (0x605b6f) and DividendAdjust (0x605beb).
/// Market value (in `MarketCap` units, i.e. for 100% of the shares) of an option: intrinsic value plus a
/// time premium from a table by months to expiry and distance from the money, adjusted for the stock's
/// volatility, the interest rate, dividends, earnings and funds. With `rec_idx >= 1` the option is read
/// from `OptionRecs(rec_idx)`, else from the arguments (strike as ABS).
///
/// Side effects (callees): `CurQuarter` forced to >= 1, `DividendPerShare` reset for implausible yields,
/// the price history tick of `CalcStockVolatility`.
///
/// Error paths (ON ERROR -> ShowRuntimeError "GetOptionPrice", result 0): `MarketCap = 0` (ERROR 151) and
/// `SharesOutstanding = 0` (sets it to 2 and calls CheckStockSplit first); the port returns 0 without UI.
/// The debug MSGBOX (local flag "N") is never shown.
pub fn calc_option_price(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    stock_id_in: i32,
    strike_in: Ext,
    exp_year_in: i32,
    exp_month_in: i32,
    opt_type_in: i32,
    rec_idx: i32,
) -> Ext {
    let mut fr = PriceFrame::default();
    if rec_idx < 1 {
        fr.stock_id = stock_id_in;
        fr.strike = strike_in.abs();
        fr.exp_year = exp_year_in;
        fr.exp_month = exp_month_in;
        fr.opt_type = opt_type_in;
    } else {
        let rec = g.option_recs[rec_idx].clone();
        fr.stock_id = to_long(fval(&rec, 1));
        fr.opt_type = to_long(fval(&rec, 2));
        fr.strike = fval(&rec, 5).abs();
        fr.exp_month = to_long(fval(&rec, 6));
        fr.exp_year = to_long(fval(&rec, 7));
    }
    let stock_id = fr.stock_id;
    if fr.opt_type == 1 {
        fr.in_money = g.market_cap[stock_id] - fr.strike;
    } else {
        fr.in_money = fr.strike - g.market_cap[stock_id];
    }
    if g.market_cap[stock_id] == Ext::from_f64(0.0) {
        // ERROR 151 ("VarType= StokPrice##()") -> ShowRuntimeError; result 0
        return Ext::from_f64(0.0);
    }
    if g.shares_outstanding[stock_id] == Ext::from_f64(0.0) {
        g.shares_outstanding[stock_id] = Ext::from_f64(2.0);
        check_stock_split(g, stock_id);
        // ERROR 151 ("VarType= PMult()") -> ShowRuntimeError; result 0
        return Ext::from_f64(0.0);
    }
    if fr.in_money != Ext::from_f64(0.0) {
        fr.pct_away = crate::pb::numeric::double(Ext::from_f64(100.0) * (fr.in_money.abs() / g.market_cap[stock_id]));
    }
    if Ext::from_f64(60.0) < fr.pct_away {
        fr.pct_away = crate::pb::numeric::double(Ext::from_f64(60.0));
    }
    if fr.opt_type == 1 && fr.strike > g.market_cap[stock_id] * f(1.8f32) {
        fr.pct_away = crate::pb::numeric::double(Ext::from_f64(100.0) * (fr.strike / g.market_cap[stock_id]) - Ext::from_f64(100.0));
        if Ext::from_f64(150.0) < fr.pct_away {
            fr.pct_away = crate::pb::numeric::double((fr.pct_away - Ext::from_f64(150.0)) * f(0.1f32) + Ext::from_f64(150.0));
        }
    } else if fr.opt_type == 2 && g.market_cap[stock_id] > fr.strike * f(1.6f32) {
        if fr.strike == Ext::from_f64(0.0) {
            // ERROR 151 ("VarType = Exercise##") -> ShowRuntimeError; result 0
            return Ext::from_f64(0.0);
        }
        fr.pct_away = crate::pb::numeric::double(Ext::from_f64(100.0) * (g.market_cap[stock_id] / fr.strike) - Ext::from_f64(100.0));
        if Ext::from_f64(150.0) < fr.pct_away {
            fr.pct_away = crate::pb::numeric::double((fr.pct_away - Ext::from_f64(150.0)) * f(0.1f32) + Ext::from_f64(150.0));
        }
    }
    if fr.in_money < -f(0.01f32) {
        fr.out_of_money = Ext::from_f64(1.0);
    }
    if fr.in_money < f(0.001f32) {
        fr.in_money = Ext::from_f64(0.0);
    }
    let progress = g.sim_co_count;
    let cur_month_idx = calc_cur_month_of_year(g, progress);
    let mut exp_month_idx: Ext = cast::<Ext, _>(fr.exp_year - g.cur_year) * Ext::from_f64(12.0) + cast::<Ext, _>(fr.exp_month);
    exp_month_idx = exp_month_idx - f(0.5f32);
    fr.months_left = crate::pb::numeric::double(exp_month_idx - cur_month_idx);
    fr.whole_months = to_long(fix(fr.months_left));
    if 24 < fr.whole_months {
        fr.whole_months = 24;
    }
    fr.frac_month = crate::pb::numeric::double(frac(fr.months_left));

    premium_table(g, &mut fr);
    vol_adjust(g, ui, rng, &mut fr);
    fr.time_prem = fr.std_prem;
    rate_adjust(g, &mut fr);

    let mut div_change_corp = 0;
    if !g.recent_dividend_change_list.is_empty() {
        let list = g.recent_dividend_change_list.clone();
        let item_count = parse_count_any(list.as_bytes(), b"|");
        for i in 1..=item_count {
            let item = parse_any(list.as_bytes(), b"|", i);
            if val(item.as_bytes()) == cast::<Ext, _>(stock_id) {
                div_change_corp = stock_id;
            }
        }
    }
    if !(stock_id == div_change_corp && g.cur_player == g.corp_controller[stock_id]) {
        dividend_adjust(g, &mut fr);
    }
    if fr.time_prem < Ext::from_f64(0.0) {
        fr.time_prem = Ext::from_f64(0.0);
    }
    if g.industry_num[stock_id] == 71 && 1580 < stock_id && stock_id < 1586 {
        if Ext::from_f64(0.0) < fr.in_money && fr.time_prem < fr.in_money * f(0.2f32) {
            fr.time_prem = Ext::from_f64(2.0) * fr.time_prem;
        }
    } else {
        fr.time_prem = f(0.8f32) * fr.time_prem;
        if g.industry_num[stock_id] == 71 {
            if 1585 < stock_id {
                fr.time_prem = f(0.8f32) * fr.time_prem;
            }
        } else {
            if fr.opt_type == 1 && g.proj_qtr_earnings[stock_id] < Ext::from_f64(0.0) {
                fr.time_prem = f(0.9f32) * fr.time_prem;
            }
            if fr.opt_type == 2 && g.proj_qtr_earnings[stock_id] > g.earnings_y1[stock_id] * Ext::from_f64(0.25) {
                fr.time_prem = f(0.9f32) * fr.time_prem;
            }
        }
    }
    let mut opt_price = fr.in_money + fr.time_prem;
    if opt_price < g.shares_outstanding[stock_id] * f(0.02f32) && 3 < fr.whole_months {
        if opt_price < g.shares_outstanding[stock_id] * f(0.01f32) {
            opt_price = g.shares_outstanding[stock_id] * f(0.01f32);
        }
        if g.market_cap[stock_id] / g.shares_outstanding[stock_id] > Ext::from_f64(25.0) {
            // asm: (0.2 * (wholeMonths - 3)) * optPrice + optPrice
            opt_price = (f(0.2f32) * cast::<Ext, _>(fr.whole_months - 3)) * opt_price + opt_price;
        }
    }
    // (debug MSGBOX when the local flag is "Y"; it is "N")
    opt_price
}

/// CalcOptionPrice_PremiumTable (0x604922): base time premium `stdPrem` by whole months to expiry,
/// scaled for puts, the fraction of a month, and once per percent away from the money.
fn premium_table(g: &mut Globals, fr: &mut PriceFrame) {
    let mc = g.market_cap[fr.stock_id];
    let wm = fr.whole_months;
    let oom = Ext::from_f64(0.0) < fr.out_of_money;
    if wm < 1 {
        // asm: three bands by fracMonth (the decompiled view has their order reversed)
        let (coef, pf, fs, opf, ofs): (f32, f32, f32, f32, f32) = if !(f(0.3f32) < fr.frac_month) {
            (0.02436f32, 0.83f32, 0.1f32, 0.78f32, 0.12f32)
        } else if f(0.65f32) > fr.frac_month {
            (0.0286f32, 0.86f32, 0.05f32, 0.81f32, 0.1f32)
        } else {
            (0.0338f32, 0.885f32, 0.045f32, 0.86f32, 0.01f32)
        };
        fr.std_prem = mc * f(coef);
        fr.factor_step = f(fs);
        fr.pct_factor = crate::pb::numeric::double(f(pf));
        if oom {
            fr.pct_factor = crate::pb::numeric::double(f(opf));
            fr.factor_step = f(ofs);
        }
    } else {
        let r = &PREM_ROWS(&crate::content::current())[cast::<usize, _>(wm.min(24) - 1)];
        fr.std_prem = mc * f(r.coef);
        fr.pct_factor = crate::pb::numeric::double(f(r.pct_factor));
        fr.factor_step = Ext::from(r.factor_step); // (case 3 sets none: the frame's 0)
        if oom {
            if let Some(p) = r.oom_pct_factor {
                fr.pct_factor = crate::pb::numeric::double(f(p));
            }
            if let Some(st) = r.oom_factor_step {
                fr.factor_step = Ext::from(st);
            }
        }
    }
    if fr.opt_type == 2 {
        let mut put_factor = if wm <= 12 { f(0.95f32) } else { f(0.96f32) };
        let stock_id = fr.stock_id;
        if calc_dividend_yield(g, stock_id) > Ext::from_f64(1.5) {
            put_factor = Ext::from_f64(1.0);
        }
        fr.std_prem = put_factor * fr.std_prem;
    }
    let frac_coef: Option<f32> = match wm {
        1 => Some(0.38f32),
        2 => Some(0.32f32),
        3 => Some(0.27f32),
        4 => Some(0.13f32),
        5 => Some(0.09f32),
        6 => Some(0.075f32),
        7 => Some(0.06f32),
        8 | 9 => Some(0.05f32),
        10..=12 => Some(0.04f32),
        13..=15 => Some(0.03f32),
        16..=20 => Some(0.02f32),
        _ if 20 < wm => Some(0.01f32),
        _ => None,
    };
    let frac_factor: Ext = match frac_coef {
        Some(c) => f(c) * fr.frac_month + Ext::from_f64(1.0),
        None => Ext::from_f64(1.0),
    };
    fr.std_prem = frac_factor * fr.std_prem;
    if Ext::from_f64(0.0) < fr.pct_away {
        let n = to_long(fr.pct_away);
        for k in 1..=n {
            if k == 15 {
                fr.pct_factor = crate::pb::numeric::double(fr.factor_step + fr.pct_factor);
            }
            if k == 150 {
                fr.pct_factor = crate::pb::numeric::double(fr.pct_factor - fr.factor_step * Ext::from_f64(0.5));
            }
            fr.std_prem = fr.pct_factor * fr.std_prem;
        }
    }
}

/// CalcOptionPrice_VolAdjust (0x605a9c): scale `stdPrem` by `SQR(volatility%/100)`, limited to ±25%.
fn vol_adjust(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, fr: &mut PriceFrame) {
    if !(fr.std_prem > Ext::from_f64(0.0)) {
        return;
    }
    let mut vol_pct: Ext = cast::<Ext, _>(calc_stock_volatility(g, ui, rng, fr.stock_id, 0));
    if !(Ext::from_f64(0.0) < vol_pct) {
        return;
    }
    vol_pct = f(0.01f32) * vol_pct;
    let mut vol_factor: f32 = cast::<f32, _>(cast::<f32, _>(sqr(vol_pct)));
    vol_factor = cast::<f32, _>(vol_factor - Ext::from_f32(1.0f32));
    if Ext::from_f32(0.25f32) < vol_factor {
        vol_factor = 0.25f32;
    }
    if -Ext::from_f32(0.25f32) > vol_factor {
        vol_factor = -0.25f32;
    }
    vol_factor = cast::<f32, _>(vol_factor + Ext::from_f32(1.0f32));
    fr.std_prem = cast::<Ext, _>(vol_factor) * fr.std_prem;
}

/// CalcOptionPrice_RateAdjust (0x605b6f): scale `timePrem` by the 8th root of `BaseInterestRate / 7.5`.
fn rate_adjust(g: &mut Globals, fr: &mut PriceFrame) {
    if fr.time_prem > Ext::from_f64(0.0) {
        let rate_ratio: Ext = cast::<Ext, _>(g.base_interest_rate) / f(7.5f32);
        let r2: Ext = sqr(rate_ratio);
        let r4: Ext = sqr(r2);
        let r8: Ext = sqr(r4);
        fr.time_prem = r8 * fr.time_prem;
    }
}

/// CalcOptionPrice_DividendAdjust (0x605beb): puts on low-yield stocks lose 5-10%; for yields over 0.1%
/// the expected dividends until expiry lower call premiums and raise put premiums.
fn dividend_adjust(g: &mut Globals, fr: &mut PriceFrame) {
    let stock_id = fr.stock_id;
    let mut div_yield: Ext = calc_dividend_yield(g, stock_id);
    if f(0.1f32) > fr.months_left {
        return;
    }
    if fr.opt_type == 2 {
        if Ext::from_f64(2.0) > div_yield {
            fr.time_prem = f(0.95f32) * fr.time_prem;
        }
        if Ext::from_f64(1.0) > div_yield {
            fr.time_prem = f(0.95f32) * fr.time_prem;
        }
    }
    if !(f(0.1f32) < div_yield) {
        return;
    }
    if Ext::from_f64(10.0) < div_yield {
        div_yield = Ext::from_f64(10.0);
    }
    let ml = fr.months_left;
    let mut adj_factor: Ext = f(if ml < Ext::from_f64(3.0) {
        0.243f32
    } else if ml < Ext::from_f64(6.0) {
        0.477f32
    } else if ml < Ext::from_f64(9.0) {
        0.702f32
    } else if ml < Ext::from_f64(12.0) {
        0.92f32
    } else if ml < Ext::from_f64(15.0) {
        1.125f32
    } else if ml < Ext::from_f64(18.0) {
        1.32f32
    } else if ml < Ext::from_f64(21.0) {
        1.51f32
    } else {
        1.71f32
    });
    if stock_id < g.sim_co_count {
        adj_factor = adj_factor - f(0.2f32);
        if Ext::from_f64(0.0) > adj_factor {
            adj_factor = Ext::from_f64(0.0);
        }
        if f(0.25f32) > ml {
            adj_factor = Ext::from_f64(0.0);
        }
    }
    if Ext::from_f64(0.0) < adj_factor && fr.time_prem > Ext::from_f64(0.0) {
        fr.div_adj = ((f(0.01f32) * div_yield) * adj_factor) * g.market_cap[stock_id] * f(0.9f32);
        if fr.opt_type == 1 {
            fr.div_adj = f(0.75f32) * fr.div_adj;
            while fr.div_adj > fr.time_prem * f(0.8f32) {
                fr.div_adj = f(0.8f32) * fr.div_adj;
                fr.loop_guard = fr.loop_guard + 1;
                if 999 < fr.loop_guard {
                    return;
                }
            }
            fr.time_prem = fr.time_prem - fr.div_adj * Ext::from_f64(0.5);
        } else {
            fr.time_prem = fr.time_prem + fr.div_adj * Ext::from_f64(0.5);
        }
    }
}

/// Port of CalcSpreadFactor (0x606399).
/// Bid/ask spread multiplier for buying back a written option: `1 + 0.4 * gap/min(stock, strike)` (at
/// least 1.02), at most 1.05 when the option is in the money and at most 1.15 overall; 1.02 within 2%.
/// ERROR 151 for a zero stock value or strike (ON ERROR -> ShowRuntimeError, result 0; port: returns 0).
pub fn calc_spread_factor(opt_type: i32, stock_value: Ext, strike_value: Ext) -> Ext {
    let sv = stock_value;
    let kv = strike_value.abs();
    let gap: Ext = (kv - sv).abs();
    let mut spread_factor: Ext;
    if sv * f(0.02f32) > gap {
        spread_factor = f(1.02f32);
    } else {
        if sv == Ext::from_f64(0.0) || kv == Ext::from_f64(0.0) {
            return Ext::from_f64(0.0);
        }
        let mut pct_gap: Ext = gap / sv;
        if sv > kv {
            pct_gap = gap / kv;
        }
        spread_factor = f(0.4f32) * pct_gap;
        if spread_factor < f(0.02f32) {
            spread_factor = f(0.02f32);
        }
        spread_factor = spread_factor + Ext::from_f64(1.0);
    }
    if opt_type == 1 {
        if sv > kv && spread_factor > f(1.05f32) {
            spread_factor = f(1.05f32);
        }
    } else if kv > sv && spread_factor > f(1.05f32) {
        spread_factor = f(1.05f32);
    }
    if spread_factor > f(1.15f32) {
        spread_factor = f(1.15f32);
    }
    spread_factor
}

/// Port of CalcOptionsValue (0x5f23e1).
/// Values `holder_id`'s option positions into `OptionsNetValue` (price x contracts, short negative),
/// `LongOptionsValue` (long positions), `OptionsCostBasis` (premiums, written ones negated) and
/// `CiOptIntrinsicVal` (in-the-money value of the long positions).
pub fn calc_options_value(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, holder_id_ref: i32) {
    g.options_net_value = Ext::from_f64(0.0);
    g.long_options_value = Ext::from_f64(0.0);
    g.options_cost_basis = Ext::from_f64(0.0);
    g.ci_opt_intrinsic_val = Ext::from_f64(0.0);
    let holder_id = holder_id_ref;
    let pos_code = find_option_positions(g, ui, holder_id, 0, 0, 0);
    if pos_code == 0 {
        return;
    }
    for rec_idx in 1..=1000 {
        let mut intrinsic_val: Ext = Ext::from_f64(0.0);
        if g.option_recs[rec_idx].is_empty() {
            continue;
        }
        if rec_val(g, rec_idx, 3) != cast::<Ext, _>(holder_id) {
            continue;
        }
        let corp_id = to_long(rec_val(g, rec_idx, 1));
        let opt_type = to_long(rec_val(g, rec_idx, 2));
        let qty = to_long(rec_val(g, rec_idx, 4));
        let strike = rec_val(g, rec_idx, 5).abs();
        let exp_month = to_long(rec_val(g, rec_idx, 6));
        let exp_year = to_long(rec_val(g, rec_idx, 7));
        let mut premium = rec_val(g, rec_idx, 8);
        let price = calc_option_price(g, ui, rng, corp_id, strike, exp_year, exp_month, opt_type, 0);
        let value = price * cast::<Ext, _>(qty);
        g.options_net_value = g.options_net_value + value;
        if 0 < qty {
            if opt_type == 1 {
                if strike < g.market_cap[corp_id] {
                    intrinsic_val = g.market_cap[corp_id] - strike;
                }
            } else if strike > g.market_cap[corp_id] {
                intrinsic_val = strike - g.market_cap[corp_id];
            }
            if Ext::from_f64(0.0) < intrinsic_val {
                g.ci_opt_intrinsic_val = g.ci_opt_intrinsic_val + intrinsic_val * cast::<Ext, _>(qty);
            }
        }
        if 0 < qty {
            g.long_options_value = g.long_options_value + value;
        } else {
            premium = -Ext::from_f64(1.0) * premium;
        }
        g.options_cost_basis = g.options_cost_basis + premium;
    }
}

/// Port of CalcVestedOptionsValue (0x5ff8c0).
/// In-the-money value (`MarketCap - strike`, at least 0, times contracts) of the first `VESTED` executive
/// option record on `corp_id_ref` that expires at the start of the current quarter (or the next one
/// once the simulation clock has passed the company: both > 250, clock < corp); 0 if there is none or the
/// clock position rules out vesting now (corp past the clock, or clock beyond 250 for a corp <= 250).
pub fn calc_vested_options_value(g: &mut Globals, corp_id_ref: i32) -> Ext {
    let mut result: Ext = Ext::from_f64(0.0);
    let corp_id = corp_id_ref;
    let mut scan_limit = to_long(val(g.option_recs[0].as_bytes()));
    if scan_limit == 0 {
        return result;
    }
    scan_limit = scan_limit + 100;
    if 1000 < scan_limit {
        scan_limit = 1000;
    }
    for rec_idx in 1..=scan_limit {
        if g.option_recs[rec_idx].is_empty() {
            continue;
        }
        let opt_rec = g.option_recs[rec_idx].clone();
        if field(&opt_rec, 8) != "VESTED" {
            continue;
        }
        if fval(&opt_rec, 1) != cast::<Ext, _>(corp_id) {
            continue;
        }
        let mut vest_year = g.cur_year;
        let mut vest_quarter = g.cur_quarter;
        let mut sim_pos = g.sim_co_count;
        if sim_pos < g.num_players + 1 {
            sim_pos = 1600;
        }
        // asm: the decompiled view of these tests is garbled
        if 250 < sim_pos && 250 < corp_id {
            if corp_id > sim_pos {
                return result;
            }
            vest_quarter = vest_quarter + 1;
        } else if sim_pos <= 250 && corp_id <= 250 {
            if corp_id > sim_pos {
                return result;
            }
        } else if 250 < sim_pos {
            return result;
        }
        if vest_quarter == 5 {
            vest_quarter = 1;
            vest_year = vest_year + 1;
        }
        let vest_month = 3 * vest_quarter - 2;
        if fval(&opt_rec, 6) != cast::<Ext, _>(vest_month) {
            continue;
        }
        if fval(&opt_rec, 7) != cast::<Ext, _>(vest_year) {
            continue;
        }
        let qty = to_long(fval(&opt_rec, 4));
        let mut intrinsic = g.market_cap[corp_id] - fval(&opt_rec, 5).abs();
        if intrinsic < Ext::from_f64(0.0) {
            intrinsic = Ext::from_f64(0.0);
        }
        result = intrinsic * cast::<Ext, _>(qty);
        return result;
    }
    result
}

/// The locals of GetPlrNakedLiab shared with its GOSUBs AddRef / FindRef.
struct LiabFrame {
    /// `DIM refCorps(0 TO 1000) AS LONG`: underlyings of the writer's short positions.
    ref_corps: Vec<i32>,
    ref_corp: i32,
    ref_idx: i32,
}

impl LiabFrame {
    /// GetPlrNakedLiab_AddRef (0x608837): append `ref_corp` to `ref_corps` unless present.
    fn add_ref(&mut self) {
        let mut ref_scan = 1;
        loop {
            if self.ref_corp == self.ref_corps[ref_scan] {
                return;
            }
            if self.ref_corps[ref_scan] == 0 {
                break;
            }
            ref_scan += 1;
            if 1000 < ref_scan {
                return;
            }
        }
        self.ref_corps[ref_scan] = self.ref_corp;
    }

    /// GetPlrNakedLiab_FindRef (0x6088dc): index of `ref_corp` in `ref_corps` (MSGBOX if missing).
    fn find_ref(&mut self, ui: &mut dyn Ui) {
    let __content = crate::content::current();
        self.ref_idx = 0;
        for ref_scan in 1..=1000 {
            if self.ref_corp == self.ref_corps[ref_scan] {
                self.ref_idx = cast::<i32, _>(ref_scan);
                break;
            }
        }
        if self.ref_idx == 0 {
            let msg = s(__content.string(6822)) + &str15(cast::<Ext, _>(self.ref_corp)) + __content.string(6823);
            ui.msgbox(&msg, 8208, &s(__content.string(6824)));
        }
    }
}

/// Port of GetPlrNakedLiab (0x60769f) with its GOSUBs AddRef (0x608837) and FindRef (0x6088dc).
/// Uncovered liability of `writer_id`'s written (short) options at market prices: per underlying, calls
/// covered by long stock and puts by short stock are netted out; with `stock_id = 0` the sum over all
/// underlyings of the larger of call and put liability, with `stock_id > 10` the call (`opt_type = 1`)
/// or put liability on that stock only. 0 when the writer has no short options (on that stock), for
/// inactive writers/stocks and for `stock_id` 1..10.
pub fn get_plr_naked_liab(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, writer_id: i32, stock_id: i32, opt_type: i32) -> Ext {
    // DIM longPcts(0 TO 15), shortPcts(0 TO 15), refCorps(0 TO 1000) AS LONG,
    //     callLiab(0 TO 1000), putLiab(0 TO 1000) AS EXT
    let mut long_pcts = [0i32; 16];
    let mut short_pcts = [0i32; 16];
    let mut call_liab = vec![Ext::ZERO; 1001];
    let mut put_liab = vec![Ext::ZERO; 1001];
    let mut lf = LiabFrame { ref_corps: vec![0; 1001], ref_corp: 0, ref_idx: 0 };
    let mut total_liab: Ext = Ext::from_f64(0.0);
    let writer = writer_id;
    let stock = stock_id;
    if 0 < writer && g.entity_status[writer] != 1 {
        return Ext::from_f64(0.0);
    }
    if 10 < stock && g.entity_status[stock] != 1 {
        return Ext::from_f64(0.0);
    }
    let mut max_recs = to_long(val(g.option_recs[0].as_bytes()) + Ext::from_f64(50.0));
    if 1000 < max_recs {
        max_recs = 1000;
    }
    if stock < 11 {
        let pos_result = find_option_positions(g, ui, writer, 0, 0, 0);
        if pos_result != -41 && pos_result != 42 {
            return Ext::from_f64(0.0);
        }
        for idx in 1..=max_recs {
            let rec = g.option_recs[idx].clone();
            if rec.is_empty() {
                continue;
            }
            if fval(&rec, 4) < Ext::from_f64(0.0) && fval(&rec, 3) == cast::<Ext, _>(writer) {
                lf.ref_corp = to_long(fval(&rec, 1));
                lf.add_ref();
            }
        }
    } else {
        let pos_result = find_option_positions(g, ui, writer, stock, 0, 0);
        if pos_result != -51 && pos_result != 52 {
            return Ext::from_f64(0.0);
        }
        lf.ref_corps[1] = stock;
    }
    if 0 < g.hold_stock_id[(0, writer)] {
        if stock < 11 {
            for idx in 1..=15 {
                if g.hold_stock_id[(idx, writer)] == 0 {
                    continue;
                }
                let pct = cast::<i32, _>(g.hold_stock_pct[(idx, writer)]);
                if pct < 1 {
                    short_pcts[cast::<usize, _>(idx)] = cast::<i32, _>(to_quad((cast::<Ext, _>(pct)).abs()));
                } else {
                    long_pcts[cast::<usize, _>(idx)] = pct;
                }
            }
        } else {
            let hold_slot = find_stock_holding(g, writer, stock);
            if hold_slot != 0 {
                if hold_slot < 1 {
                    let slot = to_long((cast::<Ext, _>(hold_slot)).abs());
                    let pct_slot = cast::<i32, _>(to_quad((cast::<Ext, _>(hold_slot)).abs()));
                    short_pcts[cast::<usize, _>(slot)] =
                        cast::<i32, _>(to_quad((cast::<Ext, _>(g.hold_stock_pct[(pct_slot, writer)])).abs()));
                } else {
                    long_pcts[cast::<usize, _>(hold_slot)] = cast::<i32, _>(g.hold_stock_pct[(hold_slot, writer)]);
                }
            }
        }
    }
    for idx in 1..=max_recs {
        let rec = g.option_recs[idx].clone();
        if rec.is_empty() {
            continue;
        }
        if !(fval(&rec, 4) < Ext::from_f64(0.0)) || fval(&rec, 3) != cast::<Ext, _>(writer) {
            continue;
        }
        if 10 < stock && fval(&rec, 1) != cast::<Ext, _>(stock) {
            continue;
        }
        let opt_pct = to_long(fval(&rec, 4).abs());
        let opt_price = calc_option_price(g, ui, rng, 0, Ext::from_f64(0.0), 0, 0, 0, idx);
        let pos_liab = opt_price.abs() * cast::<Ext, _>(opt_pct);
        lf.ref_corp = to_long(fval(&rec, 1));
        lf.find_ref(ui);
        if 0 < lf.ref_idx {
            let r = cast::<usize, _>(lf.ref_idx);
            if fval(&rec, 2) == Ext::from_f64(1.0) {
                if 10 < stock && opt_type == 2 {
                    continue;
                }
                call_liab[r] = call_liab[r] + pos_liab;
            } else {
                if 10 < stock && opt_type == 1 {
                    continue;
                }
                put_liab[r] = put_liab[r] + pos_liab;
            }
        }
    }
    if 0 < g.hold_stock_id[(0, writer)] {
        for idx in 1..=max_recs {
            let rec = g.option_recs[idx].clone();
            if rec.is_empty() {
                continue;
            }
            if !(fval(&rec, 4) < Ext::from_f64(0.0)) || fval(&rec, 3) != cast::<Ext, _>(writer) {
                continue;
            }
            if 10 < stock {
                if fval(&rec, 1) != cast::<Ext, _>(stock) {
                    continue;
                }
                if fval(&rec, 2) == Ext::from_f64(1.0) && opt_type == 2 {
                    continue;
                }
                if fval(&rec, 2) == Ext::from_f64(2.0) && opt_type == 1 {
                    continue;
                }
            }
            let opt_pct = to_long(fval(&rec, 4).abs());
            let opt_price = calc_option_price(g, ui, rng, 0, Ext::from_f64(0.0), 0, 0, 0, idx);
            let pos_liab = opt_price.abs() * cast::<Ext, _>(opt_pct);
            lf.ref_corp = to_long(fval(&rec, 1));
            lf.find_ref(ui);
            if !(0 < lf.ref_idx) {
                continue;
            }
            let mut hold_slot = find_stock_holding(g, writer, lf.ref_corp);
            if hold_slot == 0 {
                continue;
            }
            let r = cast::<usize, _>(lf.ref_idx);
            let mut cover_value: Ext = Ext::from_f64(0.0);
            if fval(&rec, 2) == Ext::from_f64(1.0) && 0 < hold_slot {
                let hs = cast::<usize, _>(hold_slot);
                if long_pcts[hs] == 0 {
                    continue;
                }
                if !(long_pcts[hs] < opt_pct) {
                    cover_value = pos_liab;
                } else if 0 < long_pcts[hs] {
                    cover_value = opt_price * cast::<Ext, _>(long_pcts[hs]);
                }
                long_pcts[hs] = long_pcts[hs] - opt_pct;
                if long_pcts[hs] < 0 {
                    long_pcts[hs] = 0;
                }
                call_liab[r] = call_liab[r] - cover_value;
            } else if fval(&rec, 2) == Ext::from_f64(2.0) && hold_slot < 0 {
                hold_slot = to_long((cast::<Ext, _>(hold_slot)).abs());
                let hs = cast::<usize, _>(hold_slot);
                if short_pcts[hs] == 0 {
                    continue;
                }
                if !(short_pcts[hs] < opt_pct) {
                    cover_value = pos_liab;
                } else if 0 < short_pcts[hs] {
                    cover_value = opt_price * cast::<Ext, _>(short_pcts[hs]);
                }
                short_pcts[hs] = short_pcts[hs] - opt_pct;
                if short_pcts[hs] < 0 {
                    short_pcts[hs] = 0;
                }
                put_liab[r] = put_liab[r] - cover_value;
            }
        }
    }
    if stock == 0 {
        for idx in 1..=1000 {
            if !(call_liab[idx] > put_liab[idx]) {
                total_liab = total_liab + put_liab[idx];
            } else {
                total_liab = total_liab + call_liab[idx];
            }
        }
    }
    if stock < 11 {
        total_liab
    } else if opt_type == 1 {
        Ext::from(call_liab[1])
    } else {
        Ext::from(put_liab[1])
    }
}

// ---------------------------------------------------------------------------------------------------------
// Position queries
// ---------------------------------------------------------------------------------------------------------

/// Port of GetOptionedStockPct (0x5f8b12).
/// Percentage of `corp_id_arg`'s shares tied up by `holder_id_arg`'s options that may require delivering
/// or taking stock: written calls and long puts (`calls_only = 1`: written calls only). 0 when the holder
/// has no options on the stock.
pub fn get_optioned_stock_pct(g: &mut Globals, ui: &mut dyn Ui, holder_id_arg: i32, corp_id_arg: i32, calls_only: i32) -> i32 {
    let holder_id = holder_id_arg;
    let corp_id = corp_id_arg;
    let mut total_pct = 0;
    if find_option_positions(g, ui, holder_id, corp_id, 0, 0) == 0 {
        return 0;
    }
    let mut scan_limit = to_long(Ext::from_f64(50.0) + val(g.option_recs[0].as_bytes()));
    if 1000 < scan_limit {
        scan_limit = 1000;
    }
    for rec_idx in 1..=scan_limit {
        let opt_rec = g.option_recs[rec_idx].clone();
        if fval(&opt_rec, 1) != cast::<Ext, _>(corp_id) || fval(&opt_rec, 3) != cast::<Ext, _>(holder_id) {
            continue;
        }
        if calls_only == 1 && fval(&opt_rec, 2) == Ext::from_f64(2.0) {
            continue;
        }
        if fval(&opt_rec, 2) == Ext::from_f64(1.0) && fval(&opt_rec, 4) > Ext::from_f64(0.0) {
            continue;
        }
        if fval(&opt_rec, 2) == Ext::from_f64(2.0) && fval(&opt_rec, 4) < Ext::from_f64(0.0) {
            continue;
        }
        let rec_pct = to_long(fval(&opt_rec, 4).abs());
        total_pct = total_pct + rec_pct;
    }
    total_pct
}

/// Port of GetNetStockPctAfterOptions (0x608a27).
/// `holder_id`'s percentage holding of `stock_id` minus the part tied up by options
/// ([`get_optioned_stock_pct`] with `opt_kind`); 0 unless both entities are active.
pub fn get_net_stock_pct_after_options(
    g: &mut Globals,
    ui: &mut dyn Ui,
    holder_id: i32,
    stock_id: i32,
    opt_kind: i32,
) -> i32 {
    let holder = holder_id;
    let stock = stock_id;
    if g.entity_status[holder] != 1 || g.entity_status[stock] != 1 {
        return 0;
    }
    let optioned = get_optioned_stock_pct(g, ui, holder, stock, opt_kind);
    let slot = cast::<i16, _>(find_holding_slot(g, holder, stock));
    let mut hold_pct: i16 = 0;
    if 0 < slot {
        hold_pct = g.hold_stock_pct[(cast::<i32, _>(slot), holder)];
    }
    cast::<i32, _>(hold_pct) - optioned
}

/// Port of GetMaxOptionsPct (0x606a14).
/// How many more percent of `stock_id`'s shares may be optioned in contracts of `opt_type` on side
/// `long_short` ("L"/"S"): 100% (40% for long calls) minus the non-GRANT contracts of that type on the
/// stock outstanding on the same side (long = positive records count against "L"... i.e. records of the
/// opposite sign are skipped), limited to 10 unless `allow_over10`, at least 0.
pub fn get_max_options_pct(g: &Globals, stock_id: i32, opt_type: i32, long_short: &PbStr, allow_over10: i32) -> i32 {
    if val(g.option_recs[0].as_bytes()) == Ext::from_f64(0.0) {
        if allow_over10 == 0 {
            return 10;
        }
        if opt_type == 1 && *long_short == "L" {
            return 40;
        }
        return 100;
    }
    let stock = stock_id;
    let mut used_pct = 0;
    for idx in 1..=1000 {
        let rec = g.option_recs[idx].clone();
        if rec.is_empty() {
            continue;
        }
        if fval(&rec, 1) != cast::<Ext, _>(stock) || fval(&rec, 2) != cast::<Ext, _>(opt_type) {
            continue;
        }
        let rec_note = trim(&field(&rec, 8));
        if 0 < instr(1, rec_note.as_bytes(), b"GRANT") {
            continue;
        }
        let rec_pct = to_long(fval(&rec, 4));
        if 0 < rec_pct && *long_short == "S" {
            continue;
        }
        if rec_pct < 0 && *long_short == "L" {
            continue;
        }
        used_pct = to_long((cast::<Ext, _>(rec_pct)).abs() + cast::<Ext, _>(used_pct));
    }
    let mut remaining_pct = 100 - used_pct;
    if opt_type == 1 && *long_short == "L" {
        remaining_pct = 40 - used_pct;
    }
    if 10 < remaining_pct && allow_over10 == 0 {
        remaining_pct = 10;
    }
    if remaining_pct < 0 {
        remaining_pct = 0;
    }
    remaining_pct
}

// ---------------------------------------------------------------------------------------------------------
// Record maintenance
// ---------------------------------------------------------------------------------------------------------

/// Port of AddOrRemoveOptionRec (0x5e922a).
/// `rec_idx < 1`: stores `rec_text` in the first empty slot of `OptionRecs` and increments the count;
/// else clears `OptionRecs(rec_idx)` and decrements the count (not below 0). Then refreshes the position
/// flags (FindOptionPositions) of the record's holder and underlying.
pub fn add_or_remove_option_rec(g: &mut Globals, ui: &mut dyn Ui, rec_text: &PbStr, rec_idx: i32) {
    let mut opt_rec = rec_text.clone();
    if 0 < rec_idx {
        opt_rec = g.option_recs[rec_idx].clone();
    }
    let holder_id = to_long(fval(&opt_rec, 3));
    let corp_id = to_long(fval(&opt_rec, 1));
    if rec_idx < 1 {
        for slot in 1..=1000 {
            if g.option_recs[slot].is_empty() {
                g.option_recs[slot] = opt_rec.clone();
                let count = to_long(val(g.option_recs[0].as_bytes())) + 1;
                g.option_recs[0] = trim(&str15(cast::<Ext, _>(count)));
                break;
            }
        }
    } else {
        g.option_recs[rec_idx] = PbStr::new();
        let mut new_count = to_long(val(g.option_recs[0].as_bytes()));
        if 0 < new_count {
            new_count = new_count - 1;
        } else {
            new_count = 0;
        }
        g.option_recs[0] = trim(&str15(cast::<Ext, _>(new_count)));
    }
    find_option_positions(g, ui, holder_id, 0, 0, 0);
    find_option_positions(g, ui, 0, corp_id, 0, 0);
}

/// Port of MergeMatchingOptionRec (0x5f7ea7).
/// Merges `new_rec` into an existing record with the same holder, underlying, type, strike and expiry on
/// the same side (not a GRANT record expiring in another year) if the combined size stays within 100%:
/// the record is rewritten with the summed contracts and premiums (`STR$` fields). Returns 1 if merged,
/// else 0.
pub fn merge_matching_option_rec(g: &mut Globals, new_rec: &PbStr) -> i32 {
    let rec_text = new_rec.clone();
    let corp_id = to_long(fval(&rec_text, 1));
    let opt_type = to_long(fval(&rec_text, 2));
    let holder_id = to_long(fval(&rec_text, 3));
    let new_qty = to_long(fval(&rec_text, 4));
    let strike = fval(&rec_text, 5).abs();
    let exp_month = to_long(fval(&rec_text, 6));
    let exp_year = to_long(fval(&rec_text, 7));
    let mut new_premium = Cux::default();
    new_premium.set_ext(fval_ext(&rec_text, 8));
    for rec_idx in 1..=1000 {
        if rec_val(g, rec_idx, 3) != cast::<Ext, _>(holder_id)
            || rec_val(g, rec_idx, 1) != cast::<Ext, _>(corp_id)
            || rec_val(g, rec_idx, 2) != cast::<Ext, _>(opt_type)
        {
            continue;
        }
        if rec_val(g, rec_idx, 5).abs() != strike {
            continue;
        }
        if rec_val(g, rec_idx, 6) != cast::<Ext, _>(exp_month) || rec_val(g, rec_idx, 7) != cast::<Ext, _>(exp_year) {
            continue;
        }
        if trim(&field(&g.option_recs[rec_idx], 8)) == "GRANT" && g.cur_year != exp_year {
            continue;
        }
        let rec_qty = to_long(rec_val(g, rec_idx, 4));
        if 0 < rec_qty && new_qty < 0 {
            continue;
        }
        if rec_qty < 0 && 0 < new_qty {
            continue;
        }
        let mut rec_premium = Cux::default();
        rec_premium.set_ext(fval_ext(&g.option_recs[rec_idx], 8));
        let sum_qty = new_qty + rec_qty;
        if (cast::<Ext, _>(sum_qty)).abs() > Ext::from_f64(100.0) {
            continue;
        }
        let mut sum_premium = Cux::default();
        sum_premium.set_ext(new_premium.get_ext().fadd(rec_premium.get_ext()));
        let sep = "|";
        let mut merged = str15(cast::<Ext, _>(corp_id)) + sep + &str15(cast::<Ext, _>(opt_type)) + sep;
        merged = merged + &str15(cast::<Ext, _>(holder_id)) + sep + &str15(cast::<Ext, _>(sum_qty)) + sep;
        merged = merged + &str15(strike) + sep + &str15(cast::<Ext, _>(exp_month)) + sep + &str15(cast::<Ext, _>(exp_year)) + sep;
        merged = merged + &str15_ext(sum_premium.get_ext());
        g.option_recs[rec_idx] = merged;
        return 1;
    }
    0
}

/// Port of SortOptionRecs (0x60661c).
/// Packs the record lists (PackHoldingArrays) and bubble-sorts `OptionRecs(1..count)` by expiry year,
/// then month (at most 10000 comparisons; stops at an empty record).
pub fn sort_option_recs(g: &mut Globals, ui: &mut dyn Ui) {
    pack_holding_arrays(g, ui);
    if !(val(g.option_recs[0].as_bytes()) >= Ext::from_f64(2.0)) {
        return;
    }
    let rec_count = to_long(val(g.option_recs[0].as_bytes()));
    let mut sorted = 0;
    let mut iter_guard = 0;
    while sorted == 0 {
        sorted = 1;
        for idx in 2..=rec_count {
            iter_guard = iter_guard + 1;
            if iter_guard == 10000 {
                return;
            }
            let prev_idx = idx - 1;
            let prev_rec = g.option_recs[prev_idx].clone();
            let next_rec = g.option_recs[idx].clone();
            if prev_rec.is_empty() || next_rec.is_empty() {
                return;
            }
            let swap = if !(fval(&prev_rec, 7) <= fval(&next_rec, 7)) {
                true
            } else {
                fval(&prev_rec, 7) == fval(&next_rec, 7) && fval(&prev_rec, 6) > fval(&next_rec, 6)
            };
            if swap {
                let tmp = g.option_recs[idx].clone();
                g.option_recs[idx] = g.option_recs[prev_idx].clone();
                g.option_recs[prev_idx] = tmp;
                sorted = 0;
            }
        }
    }
}

/// Port of TransferOptionsHolder (0x5fc45b).
/// With `do_transfer = 1`, rewrites every option record of `old_holder` with `STR$(new_holder)` as
/// holder (the other fields copied as they are).
pub fn transfer_options_holder(g: &mut Globals, ui: &mut dyn Ui, old_holder: i32, new_holder: i32, do_transfer: i32) {
    if do_transfer != 1 {
        return;
    }
    if find_option_positions(g, ui, old_holder, 0, 0, 0) == 0 {
        return;
    }
    for rec_idx in 1..=1000 {
        if g.option_recs[rec_idx].is_empty() {
            continue;
        }
        if rec_val(g, rec_idx, 3) != cast::<Ext, _>(old_holder) {
            continue;
        }
        let opt_rec = g.option_recs[rec_idx].clone();
        let mut new_rec = PbStr::new();
        for fld in 1..=8 {
            if fld < 3 || 3 < fld {
                new_rec = new_rec + &field(&opt_rec, fld);
            } else if fld == 3 {
                new_rec = new_rec + &str15(cast::<Ext, _>(new_holder));
            }
            if fld < 8 {
                new_rec = new_rec + "|";
            }
        }
        g.option_recs[rec_idx] = new_rec;
    }
}

// ---------------------------------------------------------------------------------------------------------
// Settling and closing
// ---------------------------------------------------------------------------------------------------------

/// Port of SettleOptionRec (0x5f379e).
/// Settles `OptionRecs(rec_idx)` at [runtime resource wording] : an in-the-money option pays its intrinsic
/// value x contracts to the holder's cash (negative for written options); the gain or loss against the
/// premium goes to `PlayerCapGains` (players; GRANT/VESTED options are also charged to the company as
/// compensation expense, with news over 5) or `ExtraordinaryPL`. Removes the record. Returns 1 gain,
/// 2 loss, 3 neither, 0 for an empty record.
pub fn settle_option_rec(g: &mut Globals, ui: &mut dyn Ui, rec_idx_ref: i32) -> i32 {
    let __content = crate::content::current();
    let rec_idx = rec_idx_ref;
    let opt_rec = g.option_recs[rec_idx].clone();
    if opt_rec.is_empty() {
        return 0;
    }
    let corp_id = to_long(fval(&opt_rec, 1));
    let opt_type = to_long(fval(&opt_rec, 2));
    let holder_id = to_long(fval(&opt_rec, 3));
    let qty = to_long(fval(&opt_rec, 4));
    let strike = fval(&opt_rec, 5).abs();
    let premium = fval(&opt_rec, 8);
    let mut is_worthless = 0;
    let mut gain_loss: Ext = Ext::from_f64(0.0);
    if opt_type == 1 && !(g.market_cap[corp_id] > strike) {
        is_worthless = 1;
    }
    if opt_type == 2 && !(g.market_cap[corp_id] < strike) {
        is_worthless = 1;
    }
    if is_worthless == 1 {
        gain_loss = -Ext::from_f64(1.0) * premium.abs();
        if qty < 0 {
            gain_loss = premium.abs();
        }
    } else if is_worthless == 0 {
        let payoff = if opt_type == 1 {
            (g.market_cap[corp_id] - strike) * cast::<Ext, _>(qty)
        } else {
            (strike - g.market_cap[corp_id]) * cast::<Ext, _>(qty)
        };
        gain_loss = payoff.abs() - premium.abs();
        if qty < 0 {
            gain_loss = -Ext::from_f64(1.0) * gain_loss;
        }
        g.cash[holder_id] = g.cash[holder_id] + payoff;
    }
    if holder_id < 11 {
        g.player_cap_gains[holder_id] = g.player_cap_gains[holder_id] + gain_loss;
        let tag_str = trim(&field(&opt_rec, 8));
        if tag_str == "GRANT" || tag_str == "VESTED" {
            g.accrued_pl[corp_id] = g.accrued_pl[corp_id] - gain_loss;
            g.cash[corp_id] = g.cash[corp_id] - gain_loss;
            if Ext::from_f64(5.0) < gain_loss {
                g.news_msg = g.ent_symbol[corp_id].clone()
                    + __content.string(6825)
                    + &trim(&g.currency_symbol)
                    + &trim(&PbStr(format(gain_loss, b"* ###,")))
                    + " "
                    + &g.million_prefix_uc.clone();
                post_news(g, ui, 0, 0);
            }
        }
    } else {
        g.extraordinary_pl[holder_id] = to_quad(cast::<Ext, _>(g.extraordinary_pl[holder_id]) + gain_loss);
    }
    add_or_remove_option_rec(g, ui, &opt_rec, rec_idx);
    if gain_loss > Ext::from_f64(0.0) {
        1
    } else if !(gain_loss >= Ext::from_f64(0.0)) {
        2
    } else {
        3
    }
}

/// Port of CloseOptionPositions (0x5f3e5f).
///
/// `holder_id > 0`: buys back / sells one contract of the first matching position of the holder (on
/// `corp_id` if > 0) at `CalcOptionPrice` (written options at the price times [`calc_spread_factor`], at
/// least 0.01), books the gain/loss against the per-contract premium and rewrites or removes the record.
/// `pos_filter`: 1 = only long calls / short puts are skipped..., i.e. 1 skips short puts and long calls,
/// 2 = long calls only, 3 = written calls only, 4 = written puts only. Flags passed in `TmpExt1 = 99`
/// (skip GRANT options) and `TmpExt2 = 99` (skip long positions), both reset to 0. Returns the
/// underlying (1 if it is <= 10) or 0 if nothing was closed.
///
/// `holder_id <= 0`, `corp_id > 10`: closes all options on the stock — `settle_mode = 1` settles them at
/// the stock price ([`settle_option_rec`]), else they are cancelled: premiums are kept/lost and puts are
/// exercised at the strike, with news per record. Returns 1 if anything was settled or a nonzero
/// gain/loss was booked.
///
/// Bug (reproduced, BUGS.md): for GRANT/VESTED records closed by a player the compensation expense is
/// booked to `AccruedPL(recIdx)` / `Cash(recIdx)` (the record index) instead of the company's.
/// A record with 0 contracts raises ERROR 151 (ON ERROR -> ShowRuntimeError; the port returns 0).
pub fn close_option_positions(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    holder_id: i32,
    corp_id: i32,
    settle_mode: i32,
    pos_filter: i32,
) -> i32 {
    let __content = crate::content::current();
    let mut result = 0;
    let mut is_mode3 = 0;
    let mut is_mode4 = 0;
    let mut skip_longs_flag = 0;
    let mut short_only = 0;
    // EXT local; kept in 80 bits (it is added to the QUAD ExtraordinaryPL, where a 0.5 tie decides)
    let mut gain_loss = Ext80::ZERO;
    if pos_filter == 3 {
        is_mode3 = 1;
    }
    if pos_filter == 4 {
        is_mode4 = 1;
    }
    let skip_grants_flag = to_long(g.tmp_ext1);
    g.tmp_ext1 = Ext::from_f64(0.0);
    if 0 < holder_id {
        skip_longs_flag = to_long(g.tmp_ext2);
    }
    g.tmp_ext2 = Ext::from_f64(0.0);
    if 0 < holder_id {
        let pos_code = find_option_positions(g, ui, holder_id, 0, 0, 0);
        if pos_code == 0 || (pos_code == 41 && skip_longs_flag == 99) {
            return result;
        }
        if pos_code == 41 || pos_code == 42 {
            short_only = 1;
        }
        if 0 < corp_id {
            short_only = 0;
        }
        if skip_longs_flag == 99 {
            short_only = 0;
        }
        for rec_idx in 1..=1000 {
            if rec_val(g, rec_idx, 3) != cast::<Ext, _>(holder_id) {
                continue;
            }
            if 0 < corp_id && rec_val(g, rec_idx, 1) != cast::<Ext, _>(corp_id) {
                continue;
            }
            let qty = to_long(rec_val(g, rec_idx, 4));
            if qty < 0 && short_only == 1 {
                continue;
            }
            if 0 < qty && skip_longs_flag == 99 {
                continue;
            }
            if qty == 0 {
                // ERROR 151 -> ShowRuntimeError; result
                return result;
            }
            // asm: the position filters (the decompiled view mixes up their jumps)
            if pos_filter == 2 && (rec_val(g, rec_idx, 2) == Ext::from_f64(2.0) || !(rec_val(g, rec_idx, 4) >= Ext::from_f64(0.0))) {
                continue;
            }
            if pos_filter == 1 {
                if rec_val(g, rec_idx, 2) == Ext::from_f64(2.0) && qty < 0 {
                    continue;
                }
                if rec_val(g, rec_idx, 2) == Ext::from_f64(1.0) && 0 < qty {
                    continue;
                }
            }
            if is_mode4 == 1 && (rec_val(g, rec_idx, 2) == Ext::from_f64(1.0) || !(rec_val(g, rec_idx, 4) <= Ext::from_f64(0.0))) {
                continue;
            }
            if is_mode3 == 1 && (rec_val(g, rec_idx, 2) == Ext::from_f64(2.0) || !(rec_val(g, rec_idx, 4) <= Ext::from_f64(0.0))) {
                continue;
            }
            let opt_rec = g.option_recs[rec_idx].clone();
            if skip_grants_flag == 99 && 0 < instr(1, field(&opt_rec, 8).as_bytes(), b"GRANT") {
                continue;
            }
            let rec_corp = to_long(fval(&opt_rec, 1));
            let mut opt_price = Ext80::from_ext(calc_option_price(g, ui, rng, 0, Ext::from_f64(0.0), 0, 0, 0, rec_idx));
            if qty < 0 {
                let strike_val = fval(&opt_rec, 5).abs();
                let opt_type = to_long(fval(&opt_rec, 2));
                opt_price = Ext80::from_ext(calc_spread_factor(opt_type, g.market_cap[rec_corp], strike_val)).fmul(opt_price);
                if opt_price.cmp_val(Ext80::from_f32(0.01f32)).is_lt() {
                    opt_price = Ext80::from_f32(0.01f32);
                }
            }
            // CURRENCYX locals: loaded/stored through the x87 (`fild; fdiv 100` / `fmul 100; fistp`), with
            // all intermediates in 80 bits; f64 would round some half cents the other way.
            let mut close_price = Cux::default();
            close_price.set_ext(opt_price);
            let mut premium = Cux::default();
            premium.set_ext(fval_ext(&opt_rec, 8));
            let mut grant_state = 0;
            if 0 < instr(1, field(&opt_rec, 8).as_bytes(), b"GRANT") && (cast::<Ext, _>(g.cur_year)) < fval(&opt_rec, 7) {
                grant_state = 1;
            }
            if 0 < instr(1, field(&opt_rec, 8).as_bytes(), b"VESTED") {
                grant_state = 2;
            }
            let mut unit_basis = Cux::default();
            // asm: |premium| * (1 / |qty|) (`fidivr word [1]`)
            unit_basis.set_ext(premium.get_ext().abs().fmul(Ext80::ONE.fdiv(Ext80::from_i64(cast::<i64, _>(qty.unsigned_abs())))));
            let new_qty;
            if qty < 1 {
                new_qty = qty + 1;
                gain_loss = unit_basis.get_ext().fsub(close_price.get_ext());
                add_ext(&mut g.cash[holder_id], close_price.get_ext().chs());
                balance_cash_t_bills(g, holder_id);
            } else {
                new_qty = qty - 1;
                gain_loss = close_price.get_ext().fsub(unit_basis.get_ext());
                add_ext(&mut g.cash[holder_id], close_price.get_ext());
            }
            if holder_id < 11 {
                add_ext(&mut g.player_cap_gains[holder_id], gain_loss);
                if 0 < grant_state {
                    // Bug (asm): indexed by the record index, not the company
                    add_ext(&mut g.accrued_pl[rec_idx], gain_loss.chs());
                    add_ext(&mut g.cash[rec_idx], gain_loss.chs());
                }
            } else {
                add_quad(&mut g.extraordinary_pl[holder_id], gain_loss);
            }
            premium.set_ext(premium.get_ext().fsub(unit_basis.get_ext()));
            if new_qty == 0 {
                add_or_remove_option_rec(g, ui, &PbStr::new(), rec_idx);
            } else {
                let sep = "|";
                let mut new_rec = field(&g.option_recs[rec_idx], 1) + sep + &field(&opt_rec, 2) + sep;
                new_rec = new_rec + &str15(cast::<Ext, _>(holder_id)) + sep + &str15(cast::<Ext, _>(new_qty)) + sep + &field(&opt_rec, 5) + sep;
                new_rec = new_rec + &field(&opt_rec, 6) + sep + &field(&opt_rec, 7) + sep;
                if grant_state == 1 {
                    new_rec = new_rec + "GRANT";
                } else if grant_state == 2 {
                    new_rec = new_rec + "VESTED";
                } else {
                    new_rec = new_rec + &str15_ext(premium.get_ext());
                }
                g.option_recs[rec_idx] = new_rec;
            }
            result = if rec_corp < 11 { 1 } else { rec_corp };
            return result;
        }
    } else if 10 < corp_id {
        find_option_positions(g, ui, 0, corp_id, 0, 0);
        if g.has_options_outstanding[corp_id] == 0 {
            return result;
        }
        for rec_idx in 1..=1000 {
            if rec_val(g, rec_idx, 1) != cast::<Ext, _>(corp_id) {
                continue;
            }
            if settle_mode == 1 {
                settle_option_rec(g, ui, rec_idx);
                result = 1;
                continue;
            }
            let opt_rec = g.option_recs[rec_idx].clone();
            let qty = to_long(fval(&opt_rec, 4));
            let mut premium = Cux::default();
            premium.set_ext(fval_ext(&opt_rec, 8));
            let opt_type = to_long(fval(&opt_rec, 2));
            let strike = fval_ext(&opt_rec, 5).abs();
            let rec_holder = to_long(fval(&opt_rec, 3));
            let qty_x = Ext80::from_i64(cast::<i64, _>(qty));
            if qty < 1 {
                if opt_type == 1 {
                    gain_loss = premium.get_ext().abs();
                } else {
                    let put_payout = qty_x.abs().fmul(strike.abs());
                    gain_loss = premium.get_ext().abs().fsub(put_payout);
                    add_ext(&mut g.cash[rec_holder], put_payout.chs());
                    balance_cash_t_bills(g, rec_holder);
                }
            } else if opt_type == 1 {
                gain_loss = Ext80::ONE.chs().fmul(premium.get_ext());
            } else {
                let put_proceeds = qty_x.fmul(strike);
                gain_loss = put_proceeds.fsub(premium.get_ext());
                add_ext(&mut g.cash[rec_holder], put_proceeds);
            }
            if rec_holder < 11 {
                add_ext(&mut g.player_cap_gains[rec_holder], gain_loss);
            } else {
                add_quad(&mut g.extraordinary_pl[rec_holder], gain_loss);
            }
            add_or_remove_option_rec(g, ui, &PbStr::new(), rec_idx);
            let word = if gain_loss.cmp_val(Ext80::ZERO).is_lt() { "loss" } else { "gain" };
            g.news_msg = g.ent_name[rec_holder].clone()
                + " has "
                + word
                + __content.string(6826)
                + &g.ent_symbol[corp_id].clone()
                + __content.string(6827);
            post_news(g, ui, rec_holder, corp_id);
        }
        if !gain_loss.is_zero() {
            result = 1;
        }
    }
    result
}

/// Port of CheckNakedOptionMargin (0x601c99).
/// For an active company with written options: if its naked option liability ([`get_plr_naked_liab`]) is
/// not covered by its credit-adjusted net worth (`TmpCreditNetWorth` from CalcEntityFinancials) and
/// credit rating, the options exchange forces it to buy back its short puts / calls (calls only as far as
/// not covered by stock), one contract at a time, until the margin is satisfied again; posts news per
/// stock and tells a human controller.
///
/// Bug (reproduced, BUGS.md): the per-stock call/put liabilities are computed for `ActiveEntity`
/// instead of the company being checked.
pub fn check_naked_option_margin(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, entity_id_ref: i32) {
    check_naked_option_margin_with(g, ui, rng, entity_id_ref, &mut |g, ui, rng, id| {
        calc_entity_financials(g, ui, rng, id)
    });
}

/// [`check_naked_option_margin`] with the CalcEntityFinancials call replaced by `financials` (for tests:
/// the Rust CalcEntityFinancials does not call CalcOptionsValue yet).
#[doc(hidden)]
pub fn check_naked_option_margin_with(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    entity_id_ref: i32,
    financials: &mut dyn FnMut(&mut Globals, &mut dyn Ui, &mut Rng, i32),
) {
    let __content = crate::content::current();
    // local debugFlag = "N": the debug messages (ShowQuickMessage / MSGBOX "Step n: ...") never show
    let entity_id = entity_id_ref;
    if entity_id < 11 || g.entity_status[entity_id] != 1 {
        return;
    }
    financials(g, ui, rng, entity_id);
    let adj_net_worth = g.tmp_credit_net_worth;
    let credit_rating = g.credit_rating_num[entity_id];
    if !(g.long_options_value - g.options_net_value > Ext::from_f64(0.0)) {
        return;
    }
    let mut naked_liab = get_plr_naked_liab(g, ui, rng, entity_id, 0, 0);
    if !(naked_liab > Ext::from_f64(0.0)) {
        return;
    }
    if 0 < get_managing_player(g, entity_id) {
        if 5 <= credit_rating && adj_net_worth > Ext::from_f64(0.0) && adj_net_worth > naked_liab * f(0.3f32) {
            return;
        }
    } else if 5 <= credit_rating && adj_net_worth > Ext::from_f64(0.0) && naked_liab < adj_net_worth * Ext::from_f64(0.5) {
        return;
    }
    if g.net_worth > Ext::from_f64(0.0) && g.net_worth > naked_liab * Ext::from_f64(0.5) && 7 < credit_rating && !(g.cash[entity_id] < Ext::from_f64(0.0)) {
        return;
    }
    let mut covered_list = PbStr::new();
    let mut satisfied = false;
    'recs: for rec_idx in 1..=1000 {
        let opt_rec = g.option_recs[rec_idx].clone();
        if fval(&opt_rec, 3) != cast::<Ext, _>(entity_id) {
            continue;
        }
        let mut short_qty = to_long(fval(&opt_rec, 4));
        if !(short_qty < 0) {
            continue;
        }
        short_qty = to_long((cast::<Ext, _>(short_qty)).abs());
        let corp_id = to_long(fval(&opt_rec, 1));
        // Bug (asm): ActiveEntity instead of entityId
        let active = g.active_entity;
        let call_liab = get_plr_naked_liab(g, ui, rng, active, corp_id, 1);
        let put_liab = get_plr_naked_liab(g, ui, rng, active, corp_id, 2);
        let pos_filter;
        let opt_kind;
        if fval(&opt_rec, 2) == Ext::from_f64(2.0) {
            if call_liab > put_liab {
                continue;
            }
            pos_filter = 4;
            opt_kind = "put";
        } else {
            if !(put_liab < call_liab) {
                continue;
            }
            let slot = cast::<i16, _>(find_holding_slot(g, entity_id, corp_id));
            if 0 < slot && cast::<i32, _>(g.hold_stock_id[(cast::<i32, _>(slot), entity_id)]) == corp_id {
                let optioned = get_optioned_stock_pct(g, ui, entity_id, corp_id, 1);
                let uncovered_pct = cast::<i32, _>(g.hold_stock_pct[(cast::<i32, _>(slot), entity_id)]) - optioned;
                if short_qty > uncovered_pct {
                    short_qty = uncovered_pct;
                }
                if !(0 < short_qty) {
                    continue;
                }
            }
            pos_filter = 3;
            opt_kind = "call";
        }
        for step in 1..=short_qty {
            g.tmp_ext1 = Ext::from_f64(0.0);
            g.tmp_ext2 = Ext::from_f64(99.0);
            if close_option_positions(g, ui, rng, entity_id, corp_id, 0, pos_filter) == corp_id {
                g.news_msg = s(__content.string(6828))
                    + &g.ent_symbol[entity_id].clone()
                    + __content.string(6829)
                    + &g.ent_symbol[corp_id].clone()
                    + " "
                    + opt_kind
                    + __content.string(6830);
                if step == 1 && g.cur_player == g.corp_controller[entity_id] {
                    if covered_list.is_empty() {
                        covered_list = g.ent_name[corp_id].clone();
                    } else if instr(1, covered_list.as_bytes(), g.ent_name[corp_id].as_bytes()) == 0 {
                        covered_list = covered_list + ", " + &g.ent_name[corp_id].clone();
                    }
                }
            }
            naked_liab = get_plr_naked_liab(g, ui, rng, entity_id, 0, 0);
            if 0 < get_managing_player(g, entity_id) {
                if 2 <= credit_rating && adj_net_worth > Ext::from_f64(0.0) && adj_net_worth > naked_liab * f(0.3f32) {
                    satisfied = true;
                    break 'recs;
                }
            } else if 5 <= credit_rating && adj_net_worth > Ext::from_f64(0.0) && naked_liab < adj_net_worth * Ext::from_f64(0.5) {
                satisfied = true;
                break 'recs;
            }
        }
        if !g.news_msg.is_empty() {
            post_news(g, ui, 0, 0);
        }
    }
    if !satisfied && covered_list.is_empty() {
        return;
    }
    if covered_list.len() > 240 {
        return;
    }
    if !covered_list.is_empty() {
        let mut msg = s(__content.string(6831))
            + &g.ent_name[entity_id].clone()
            + __content.string(6832)
            + &covered_list;
        show_quick_message(g, ui, &mut msg);
    }
}
