//! Commodity [runtime resource wording] ies: contract data and futures prices, contract valuation,
//! futures margin (`CommodMarginBal`), the `FuturesRecs` list, storage costs, corporate hedge gains and the
//! commodity-market setup (`InitCommodityMarkets`, `SetCommodityPriceRanges`, `UpdateMetalsPriceBase`).
//!
//! Records: `FuturesRecs(i)` (1..1000, count as a string in element 0) =
//! `"COMMODITY|owner|qty|month|year|cost|price"` (qty < 0: short); `PhysCommodRec(i)` =
//! `"COMMODITY|owner|units|cost"`. Commodities: OIL, GOLD, SILVER, WHEAT, CORN, STOCKS (index futures),
//! BITCOIN, ETHEREUM (`GetCommodData` gives units per contract, unit name, number, …).
//!
//! Numeric conventions as in `logic::finance`: `EXT` uses software x87 arithmetic, SINGLE stores are rounded with `as f32`,
//! SINGLE constants of the original are written `0.05f32 as f64`; float expressions follow the operand
//! order of the machine code (comments "asm:" where the decompiled view differs).

// Statements keep the original BASIC shape (`x = x + y`, `a < 1 OR 5 < a`) on purpose; negated float
// comparisons (`!(a >= b)`) mirror the x87 branch taken for unordered (NaN) operands.
#![allow(
    clippy::manual_range_contains,
    clippy::assign_op_pattern,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::too_many_arguments,
    clippy::manual_clamp
)]
use crate::pb::real::{Ext, cast};


use crate::logic::finance::{get_controller_type, get_managing_player, get_phys_commod_value};
use crate::logic::holdings::{PackEvent, pack_holding_arrays};
use crate::logic::news::post_news;
use crate::logic::report::show_quick_message;
use crate::pb::array::Arr1;
use crate::pb::numeric::format;
use crate::pb::numeric::{str_num, val};
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{instr, ltrim, parse_any, rtrim};
use crate::pb::numeric::{ceil, int, to_long, to_quad};
use crate::state::Globals;
use crate::ui::Ui;

/// `PARSE$(rec, ANY "|", n)`.
fn field(rec: &PbStr, n: i32) -> PbStr {
    parse_any(rec.as_bytes(), b"|", n)
}

/// `VAL(PARSE$(rec, ANY "|", n))`.
fn field_val(rec: &PbStr, n: i32) -> Ext {
    val(field(rec, n).as_bytes())
}

/// `RTRIM$(LTRIM$(s))`.
fn trim(s: &[u8]) -> PbStr {
    rtrim(ltrim(s).as_bytes())
}

/// `x MOD y` on the x87 (`PB_FMod`, `fprem`): truncating remainder.
fn fmod(x: Ext, y: Ext) -> Ext {
    x % y
}

/// `PB_Pow` (0x6bcabc): `base ^ exp`.
fn pow(base: Ext, exp: Ext) -> Ext {
    base.powf(exp)
}

// ---------------------------------------------------------------------------------------------------------
// Contract data and prices
// ---------------------------------------------------------------------------------------------------------

use crate::logic::earnings::calc_cur_month_of_year;


/// Port of GetCommodData (0x61c5aa).
/// Contract data of `commodity` (trimmed): `"units per contract|unit|number|…|…"`, e.g. OIL
/// `"10000|barrel|1|15000|175000"`. Unknown names show an error MSGBOX (with `mode` and `SimCoCount`) and
/// give `"ERROR"`.
pub fn get_commod_data(g: &mut Globals, ui: &mut dyn Ui, commodity: &PbStr, mode: i32) -> PbStr {
    let __content = crate::content::current();
    let name = trim(commodity.as_bytes());
    let data = if name == "OIL" {
        __content.string(5686)
    } else if name == "GOLD" {
        __content.string(5687)
    } else if name == "SILVER" {
        __content.string(5688)
    } else if name == "WHEAT" {
        __content.string(5689)
    } else if name == "CORN" {
        __content.string(5690)
    } else if name == "STOCKS" {
        __content.string(5691)
    } else if name == "BITCOIN" {
        __content.string(5692)
    } else if name == "ETHEREUM" {
        __content.string(5693)
    } else {
        let text = PbStr::from(__content.string(5694))
            + &name
            + "  Z$ = "
            + commodity
            + __content.string(5695)
            + &g.cur_commodity
            + " X&="
            + str_num(cast::<Ext, _>(mode), 15).as_str()
            + __content.string(5696)
            + str_num(cast::<Ext, _>(g.sim_co_count), 15).as_str();
        ui.msgbox(&text, 0, &PbStr::new());
        return PbStr::from("ERROR");
    };
    PbStr::from(data)
}

/// Port of GetFuturesPrice (0x61b5b4).
/// Futures price of `commodity` for delivery in `exp_month`/`exp_year` (a past year counts as next year),
/// as text `FORMAT$(price, "* #####.00")` trimmed: the spot price (OIL in cents) compounded monthly by the
/// carry rate `FuturesBasis(n) * 0.000065 * MIN(BaseInterestRate, 12) / 12` (x0.7 after every further 12
/// months) over the months to delivery. `"ERROR"` for unknown commodities; `""` for STOCKS when
/// `StockIndex` is not in (0, 150000].
///
/// Side effects (outside game end): `FuturesBasis(n)` is clamped (0..4 -> 3; GOLD/SILVER/crypto below 5
/// -> `INT(RND*5+5)`; -100..300; STOCKS >= -30); the spot price used is made positive (`ABS`). Updating
/// the main window's price labels is UI; for OIL/STOCKS with `TickerSpeed > 60` it is done only when
/// `RND * 3 < 1` (an RND draw). `diag` is only shown by the ON ERROR handler.
pub fn get_futures_price(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    commodity: &PbStr,
    exp_month: i32,
    exp_year: i32,
    diag: i32,
) -> PbStr {
    // ON ERROR GOTO handler: ShowRuntimeError("GetCommodPrice", ERR) + MSGBOX "Diag& =" + STR$(diag) …
    let _ = diag;
    let commod_name = commodity.clone();
    let month = exp_month;
    let mut year = exp_year;
    if g.cur_year > year {
        year = g.cur_year + 1;
    }
    // fmtMask = "* #######,.00" is only used for the (UI) spot price labels
    let commod_data = get_commod_data(g, ui, &commod_name, 2);
    if commod_data == "ERROR" {
        return commod_data;
    }
    let i = to_long(field_val(&commod_data, 3));
    if g.game_end_flag != "Y" {
        if g.futures_basis[i] >= 0 && g.futures_basis[i] < 5 {
            g.futures_basis[i] = 3;
        }
        if i == 2 || i == 3 || i == 7 || i == 8 {
            if g.futures_basis[i] < 5 {
                let tmp_basis = to_quad(Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(5.0));
                g.futures_basis[i] = cast::<i16, _>(tmp_basis);
            }
        }
        if g.futures_basis[i] > 300 {
            g.futures_basis[i] = 300;
        }
        if g.futures_basis[i] < -100 {
            g.futures_basis[i] = -100;
        }
        if g.futures_basis[6] < -30 {
            g.futures_basis[6] = -30;
        }
    }
    let mut rate: Ext = cast::<Ext, _>(g.base_interest_rate);
    if !(Ext::from_f64(12.0) >= rate) {
        rate = Ext::from_f64(12.0);
    }
    // asm: (0.000065f * FuturesBasis(i) * rate) / 12, stored as DOUBLE
    let mut carry_rate: Ext = (cast::<Ext, _>(6.5e-05f32)) * cast::<Ext, _>(g.futures_basis[i]) * rate / Ext::from_f64(12.0);
    let cur_month = to_long(ceil(calc_cur_month_of_year(g, g.sim_co_count)));
    let mut months_out = (year - g.cur_year) * 12 + month - cur_month;
    if 250 < g.sim_co_count && g.sim_co_count < 501 {
        months_out -= 1;
    }
    if 750 < g.sim_co_count && g.sim_co_count < 1001 {
        months_out -= 1;
    }
    if g.num_players > g.sim_co_count || g.sim_co_count > 1250 {
        months_out -= 1;
    }
    let mut carry: Ext = Ext::from_f64(100.0);
    if months_out <= 0 {
        // asm: the count is set to 0 (the FOR loop does not run)
    } else {
        let mut m = 1;
        while m <= months_out {
            if 12 < m && m % 12 == 1 {
                carry_rate = (cast::<Ext, _>(0.7f32)) * carry_rate;
            }
            carry = carry * (carry_rate + Ext::from_f64(1.0));
            m += 1;
        }
    }
    carry = (cast::<Ext, _>(0.01f32)) * carry;

    let mut fut_price: Ext = Ext::from_f64(0.0);
    if commod_name == "OIL" {
        g.oil_price = cast::<f32, _>(g.oil_price.abs());
        fut_price = (Ext::from_f64(100.0) * cast::<Ext, _>(g.oil_price)) * carry;
        // spotText = FORMAT$(100 * OilPrice, fmtMask); (UI) label 154 = RIGHT$(spotText, 9)
        if g.ticker_speed > 60 {
            if rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) {
                // (UI) label 154
            }
        }
    } else if commod_name == "GOLD" {
        g.gold_price = cast::<f32, _>(g.gold_price.abs());
        fut_price = carry * cast::<Ext, _>(g.gold_price);
        // (UI) label 159
    } else if commod_name == "SILVER" {
        g.silver_price = cast::<f32, _>(g.silver_price.abs());
        fut_price = carry * cast::<Ext, _>(g.silver_price);
        // (UI) label 161
    } else if commod_name == "WHEAT" {
        g.wheat_price = cast::<f32, _>(g.wheat_price.abs());
        fut_price = carry * cast::<Ext, _>(g.wheat_price);
        // (UI) label 163
    } else if commod_name == "CORN" {
        g.corn_price = cast::<f32, _>(g.corn_price.abs());
        fut_price = carry * cast::<Ext, _>(g.corn_price);
        // (UI) label 165
    } else if commod_name == "STOCKS" {
        let index = g.stock_index.abs();
        fut_price = carry * index;
        if !(g.stock_index > Ext::from_f64(0.0)) || !(g.stock_index <= Ext::from_f64(150000.0)) {
            return PbStr::new();
        }
        // (UI) label 114
        if g.ticker_speed > 60 {
            if rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) {
                // (UI) label 114
            }
        }
    } else if commod_name == "BITCOIN" {
        g.bitcoin_price = to_long((cast::<Ext, _>(g.bitcoin_price)).abs());
        fut_price = carry * cast::<Ext, _>(g.bitcoin_price);
        // (UI) crypto dialog label 1172
    } else if commod_name == "ETHEREUM" {
        g.ethereum_price = to_long((cast::<Ext, _>(g.ethereum_price)).abs());
        fut_price = carry * cast::<Ext, _>(g.ethereum_price);
        // (UI) crypto dialog label 1175
    }
    trim(&format(fut_price, b"* #####.00"))
}

/// Locals of [`calc_futures_basis`] used by its GOSUB.
struct BasisFrame {
    commod_no: i32,
    price: Ext,
    /// Price bands, high to low (DOUBLE locals).
    band1: Ext,
    band2: Ext,
    band3: Ext,
    band4: Ext,
    basis: Ext,
}

/// GOSUB CalcFuturesBasis_FromPriceBand (0x636454) of [`calc_futures_basis`]. `Some(result)` where the
/// GOSUB jumps straight to the routine's exit (bypassing the caller's clamps), else it sets `f.basis`:
/// price in (band3, band2): RND*10 + 90 (exit); >= band1: -100; <= band4: 300 (exit); in [band2, band1):
/// from +100 down to -100 (at most 93); in (band4, band3]: 100 + 200 * distance / width (at least 110).
fn basis_from_price_band(f: &mut BasisFrame, rng: &mut Rng) -> Option<Ext> {
    if !(f.band3 >= f.price) && f.band2 > f.price && f.commod_no < 6 {
        return Some(Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(90.0));
    }
    if !(f.band1 > f.price) {
        f.basis = -Ext::from_f64(100.0);
        return None;
    }
    if !(f.band4 < f.price) {
        return Some(Ext::from_f64(300.0));
    }
    if !(f.band2 > f.price) {
        let band_width: Ext = f.band1 - f.band2;
        let price_dist: Ext = f.price - f.band2;
        let band_frac: Ext = price_dist / band_width;
        f.basis = (Ext::from_f64(1.0) - band_frac) * Ext::from_f64(200.0);
        f.basis = f.basis - Ext::from_f64(100.0);
        if !(Ext::from_f64(93.0) >= f.basis) {
            f.basis = Ext::from_f64(93.0);
        }
    } else {
        let band_width: Ext = f.band3 - f.band4;
        let price_dist: Ext = f.band3 - f.price;
        let band_frac: Ext = (price_dist / band_width).abs();
        f.basis = Ext::from_f64(200.0) * band_frac;
        f.basis = Ext::from_f64(100.0) + f.basis;
        if Ext::from_f64(110.0) > f.basis {
            f.basis = Ext::from_f64(110.0);
        }
    }
    None
}

/// Port of CalcFuturesBasis (0x635fd1) with its GOSUB CalcFuturesBasis_FromPriceBand (0x636454).
/// New futures basis (DOUBLE) for commodity `commod_idx` 1..6 (0 for other numbers, and for OIL during an
/// oil crisis `OilCrisisLevel > 0`). Commodities 1..5 derive it from where the spot price lies in four
/// price bands (OIL 155/102/92/45 cents; GOLD `MetalsPriceBase` + 2300/1200/1100/600; SILVER 1% of the
/// base + 28/16/15/9; WHEAT 10.4/5.8/5.4/2.8; CORN 10.2/5.75/5.5/2.7); STOCKS (6) drift from
/// `FuturesBasis(6)` (down when `(CurYear + CurQuarter + SelEntity) MOD 7 = 1`, else up towards 260..300,
/// 120..150 when `(CurQuarter + CurYear) MOD 5 = 1`). Then clamped to -100..300 (GOLD/SILVER >= 5) with
/// 0 replaced by 10 — except the band results that exit directly (90..100 and 300).
///
/// asm: the decompiled view of this routine (and its GOSUB) is wrong in several places (select cases,
/// band constants, the final `0 -> 10`); the port follows the machine code.
pub fn calc_futures_basis(g: &mut Globals, rng: &mut Rng, commod_idx: i32) -> Ext {
    if 1 > commod_idx || 6 < commod_idx {
        return Ext::from_f64(0.0);
    }
    let mut f = BasisFrame { commod_no: commod_idx, price: Ext::from_f64(0.0), band1: crate::pb::numeric::double(Ext::from_f64(0.0)), band2: Ext::from_f64(0.0), band3: Ext::from_f64(0.0), band4: Ext::from_f64(0.0), basis: Ext::from_f64(0.0) };
    let sel = cast::<Ext, _>(f.commod_no);
    let base = cast::<Ext, _>(g.metals_price_base);
    if sel == Ext::from_f64(1.0) {
        if !(Ext::from_f32(0.0f32) >= g.oil_crisis_level) {
            return Ext::from_f64(0.0);
        }
        f.price = Ext::from_f64(100.0) * cast::<Ext, _>(g.oil_price);
        (f.band1, f.band2, f.band3, f.band4) = (Ext::from_f64(155.0), Ext::from_f64(102.0), Ext::from_f64(92.0), Ext::from_f64(45.0));
    } else if sel == Ext::from_f64(2.0) {
        f.price = cast::<Ext, _>(g.gold_price);
        (f.band1, f.band2, f.band3, f.band4) = (base + Ext::from_f64(2300.0), base + Ext::from_f64(1200.0), base + Ext::from_f64(1100.0), base + Ext::from_f64(600.0));
    } else if sel == Ext::from_f64(3.0) {
        f.price = cast::<Ext, _>(g.silver_price);
        let b = base * (cast::<Ext, _>(0.01f32));
        (f.band1, f.band2, f.band3, f.band4) = (b + Ext::from_f64(28.0), b + Ext::from_f64(16.0), b + Ext::from_f64(15.0), b + Ext::from_f64(9.0));
    } else if sel == Ext::from_f64(4.0) {
        f.price = cast::<Ext, _>(g.wheat_price);
        (f.band1, f.band2, f.band3, f.band4) = (cast::<Ext, _>(10.4f32), cast::<Ext, _>(5.8f32), cast::<Ext, _>(5.4f32), cast::<Ext, _>(2.8f32));
    } else if sel == Ext::from_f64(5.0) {
        f.price = cast::<Ext, _>(g.corn_price);
        (f.band1, f.band2, f.band3, f.band4) = (cast::<Ext, _>(10.2f32), Ext::from_f64(5.75), Ext::from_f64(5.5), cast::<Ext, _>(2.7f32));
    } else if sel == Ext::from_f64(6.0) {
        let fb6 = cast::<Ext, _>(g.futures_basis[6]);
        if (g.cur_year + g.cur_quarter + g.sel_entity) % 7 == 1 {
            if Ext::from_f64(5.0) * rng.rnd() + -Ext::from_f64(70.0) < fb6 {
                f.basis = fb6 - (Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(10.0));
            }
            if -Ext::from_f64(95.0) > f.basis {
                f.basis = -Ext::from_f64(100.0) + Ext::from_f64(8.0) * rng.rnd();
            }
        } else if Ext::from_f64(40.0) * rng.rnd() + Ext::from_f64(260.0) > fb6 {
            f.basis = (Ext::from_f64(20.0) + fb6) + Ext::from_f64(30.0) * rng.rnd();
        }
        if (g.cur_quarter + g.cur_year) % 5 == 1 {
            f.basis = Ext::from_f64(30.0) * rng.rnd() + Ext::from_f64(120.0);
        }
    }
    if f.commod_no < 6 {
        // GOSUB CalcFuturesBasis_FromPriceBand
        if let Some(result) = basis_from_price_band(&mut f, rng) {
            return result;
        }
    }
    if Ext::from_f64(5.0) > f.basis && (f.commod_no == 2 || f.commod_no == 3) {
        f.basis = Ext::from_f64(5.0);
    }
    if -Ext::from_f64(100.0) > f.basis {
        f.basis = -Ext::from_f64(100.0);
    }
    if !(Ext::from_f64(300.0) >= f.basis) {
        f.basis = Ext::from_f64(300.0);
    }
    if f.basis == Ext::from_f64(0.0) {
        f.basis = Ext::from_f64(10.0);
    }
    f.basis
}

/// Port of CalcFuturesContractValue (0x61b3c3).
/// Value of the futures record `futures_rec`: `qty * 0.000001 * units per contract * futures price *
/// ExchangeRate` (negative for short positions). Calls `GetFuturesPrice` (RND, state side effects).
pub fn calc_futures_contract_value(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, futures_rec: &PbStr) -> Ext {
    let rec = futures_rec.clone();
    let commodity = trim(field(&rec, 1).as_bytes());
    let qty = to_quad(field_val(&rec, 3));
    let exp_month = to_long(field_val(&rec, 4));
    let exp_year = to_long(field_val(&rec, 5));
    let price_text = get_futures_price(g, ui, rng, &commodity, exp_month, exp_year, 1);
    let fut_price: Ext = val(price_text.as_bytes());
    let commod_data = get_commod_data(g, ui, &commodity, 1);
    let units_per_contract = to_long(field_val(&commod_data, 1));
    (cast::<Ext, _>(0.000001f32)) * cast::<Ext, _>(units_per_contract) * cast::<Ext, _>(qty) * fut_price * g.exchange_rate
}

/// Port of FormatCommodPrice (0x6657eb).
/// Spot price of `commod_name` as trimmed text (OIL in cents, `"* ######.00"`; GOLD with thousands
/// separators; crypto as whole numbers); `""` for other names (STOCKS included).
pub fn format_commod_price(g: &Globals, commod_name: &PbStr) -> PbStr {
    let key = commod_name;
    let mut price_str = PbStr::new();
    if *key == "OIL" {
        price_str = trim(&format(Ext::from_f64(100.0) * cast::<Ext, _>(g.oil_price), b"* ######.00"));
    }
    if *key == "GOLD" {
        price_str = trim(&format(cast::<Ext, _>(g.gold_price), b"* ######,.00"));
    }
    if *key == "SILVER" {
        price_str = trim(&format(cast::<Ext, _>(g.silver_price), b"* ######.00"));
    }
    if *key == "WHEAT" {
        price_str = trim(&format(cast::<Ext, _>(g.wheat_price), b"* ######.00"));
    }
    if *key == "CORN" {
        price_str = trim(&format(cast::<Ext, _>(g.corn_price), b"* ######.00"));
    }
    if *key == "BITCOIN" {
        price_str = trim(&format(cast::<Ext, _>(g.bitcoin_price), b"* ######,"));
    }
    if *key == "ETHEREUM" {
        price_str = trim(&format(cast::<Ext, _>(g.ethereum_price), b"* ######,"));
    }
    price_str
}

// ---------------------------------------------------------------------------------------------------------
// Positions and margin
// ---------------------------------------------------------------------------------------------------------

/// Port of CalcEntityFuturesNetValue (0x61c244).
/// Net value of `entity_id`'s futures positions (stored in `FuturesNetValue` and returned): for each of
/// its records the contract value minus the cost (field 6) for longs, cost minus |value| for shorts.
/// Scans records 1..MIN(count + 50, 1000) and then recalculates the entity's margin balance
/// (`RecalcCommodMarginBalances(entity)`). With no records (`VAL(FuturesRecs(0)) = 0`) it returns 0 and
/// does nothing else.
pub fn calc_entity_futures_net_value(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, entity_id: i32) -> Ext {
    // ON ERROR GOTO handler: ShowRuntimeError only
    let entity = entity_id;
    g.futures_net_value = Ext::from_f64(0.0);
    if val(g.futures_recs[0].as_bytes()) == Ext::from_f64(0.0) {
        return Ext::from_f64(0.0);
    }
    let mut max_recs = to_long(Ext::from_f64(50.0) + val(g.futures_recs[0].as_bytes()));
    if max_recs > 1000 {
        max_recs = 1000;
    }
    let mut rec_idx = 1;
    while rec_idx <= max_recs {
        if !g.futures_recs[rec_idx].is_empty() {
            let rec = g.futures_recs[rec_idx].clone();
            if field_val(&rec, 2) == cast::<Ext, _>(entity) {
                let mut contract_val = calc_futures_contract_value(g, ui, rng, &rec);
                let cost_basis = field_val(&rec, 6);
                if contract_val > Ext::from_f64(0.0) {
                    contract_val = contract_val - cost_basis;
                } else {
                    contract_val = cost_basis - contract_val.abs();
                }
                g.futures_net_value = contract_val + g.futures_net_value;
            }
        }
        rec_idx += 1;
    }
    let result = g.futures_net_value;
    recalc_commod_margin_balances(g, ui, rng, entity);
    result
}

/// GOSUB RecalcCommodMargin_RecMargin (0x61cd65) of [`recalc_commod_margin_balances`]: margin required for
/// one record = 5% of the cost plus the unrealised loss (cost - value for longs, |value| - cost for shorts).
fn rec_margin(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, fut_rec: &PbStr) -> Ext {
    let contract_cost = field_val(fut_rec, 6);
    let contract_value = calc_futures_contract_value(g, ui, rng, fut_rec);
    if contract_value > Ext::from_f64(0.0) {
        (contract_cost - contract_value) + contract_cost * (cast::<Ext, _>(0.05f32))
    } else {
        (contract_value.abs() - contract_cost) + contract_cost * (cast::<Ext, _>(0.05f32))
    }
}

/// Port of RecalcCommodMarginBalances (0x61c8fc) with its GOSUB RecalcCommodMargin_RecMargin (0x61cd65).
/// Sets `CommodMarginBal(e)` to the margin the entity's futures records require and moves the difference
/// from/to `Cash(e)`: for `entity_id >= 1` that entity only (balance 0 if it has no records), for
/// `entity_id < 1` every entity 1..1600 (all 1000 records are scanned in both cases).
pub fn recalc_commod_margin_balances(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, entity_id: i32) {
    // ON ERROR GOTO handler: ShowRuntimeError only (e.g. an owner id outside 0..1600 in the all-mode)
    let target_id = entity_id;
    if target_id > 0 {
        let ent_id = target_id;
        let mut total_margin: Ext = Ext::from_f64(0.0);
        let mut has_contracts = 0;
        for rec_idx in 1..=1000 {
            if g.futures_recs[rec_idx].is_empty() {
                continue;
            }
            let fut_rec = g.futures_recs[rec_idx].clone();
            if field_val(&fut_rec, 2) == cast::<Ext, _>(target_id) {
                let m = rec_margin(g, ui, rng, &fut_rec);
                total_margin = m + total_margin;
                has_contracts = 1;
            }
        }
        let margin_delta = total_margin - g.commod_margin_bal[ent_id];
        g.cash[ent_id] = g.cash[ent_id] - margin_delta;
        g.commod_margin_bal[ent_id] = margin_delta + g.commod_margin_bal[ent_id];
        if has_contracts == 0 {
            g.commod_margin_bal[ent_id] = Ext::from_f64(0.0);
        }
        return;
    }
    let mut margin_by_ent: Arr1<Ext> = Arr1::new(0, 1600);
    for rec_idx in 1..=1000 {
        if g.futures_recs[rec_idx].is_empty() {
            continue;
        }
        let fut_rec = g.futures_recs[rec_idx].clone();
        let ent_id = to_long(field_val(&fut_rec, 2));
        let m = rec_margin(g, ui, rng, &fut_rec);
        margin_by_ent[ent_id] = m + margin_by_ent[ent_id];
    }
    for ent_id in 1..=1600 {
        if g.commod_margin_bal[ent_id] == margin_by_ent[ent_id] {
            continue;
        }
        let margin_delta = margin_by_ent[ent_id] - g.commod_margin_bal[ent_id];
        g.cash[ent_id] = g.cash[ent_id] - margin_delta;
        g.commod_margin_bal[ent_id] = margin_delta + g.commod_margin_bal[ent_id];
    }
}

/// Port of CountFuturesPositions (0x633d45).
/// `count_mode = 0`: number of futures records of `entity_id`. `count_mode = 1`: direction of its
/// positions in `CurCommodity`: 1 long only, -1 short only, 2 both, 0 none. Other modes give 0.
pub fn count_futures_positions(g: &Globals, entity_id: i32, count_mode: i32) -> i32 {
    let ent_id = entity_id;
    let mut count = 0;
    let mut has_long = false;
    let mut has_short = false;
    for rec_idx in 1..=1000 {
        let fut_rec = &g.futures_recs[rec_idx];
        if fut_rec.is_empty() || field_val(fut_rec, 2) != cast::<Ext, _>(ent_id) {
            continue;
        }
        if count_mode == 0 {
            count += 1;
            continue;
        }
        if count_mode == 1 && field(fut_rec, 1) != g.cur_commodity {
            continue;
        }
        if field_val(fut_rec, 3) > Ext::from_f64(0.0) {
            has_long = true;
        } else if field_val(fut_rec, 3) < Ext::from_f64(0.0) {
            has_short = true;
        }
    }
    if count_mode == 1 {
        count = 0;
        if has_long && has_short {
            count = 2;
        } else if has_long {
            count = 1;
        } else if has_short {
            count = -1;
        }
    }
    count
}

/// Port of SortFuturesRecs (0x634002).
/// Packs the holding/record arrays (`PackHoldingArrays`, whose events are returned) and bubble-sorts
/// `FuturesRecs(1..count)` by delivery year, then month (fields 5, 4). Stops at the first empty record
/// and after 10000 comparisons.
pub fn sort_futures_recs(g: &mut Globals) -> Vec<PackEvent> {
    // ON ERROR GOTO handler: ShowRuntimeError only
    let events = pack_holding_arrays(g);
    if !(val(g.futures_recs[0].as_bytes()) >= Ext::from_f64(2.0)) {
        return events;
    }
    let count = to_long(val(g.futures_recs[0].as_bytes()));
    let mut sorted = 0;
    let mut iter_guard = 0;
    while sorted == 0 {
        sorted = 1;
        let mut idx = 2;
        while idx <= count {
            iter_guard += 1;
            if iter_guard == 10000 {
                return events;
            }
            let prev_idx = idx - 1;
            let rec_a = g.futures_recs[prev_idx].clone();
            let rec_b = g.futures_recs[idx].clone();
            if rec_a.is_empty() || rec_b.is_empty() {
                return events;
            }
            let swap = if field_val(&rec_a, 5) > field_val(&rec_b, 5) {
                true
            } else {
                field_val(&rec_a, 5) == field_val(&rec_b, 5) && field_val(&rec_a, 4) > field_val(&rec_b, 4)
            };
            if swap {
                let t = g.futures_recs[idx].clone();
                g.futures_recs[idx] = g.futures_recs[prev_idx].clone();
                g.futures_recs[prev_idx] = t;
                sorted = 0;
            }
            idx += 1;
        }
    }
    events
}

/// Port of AddOrRemoveFuturesRec (0x62a37e).
/// `rec_idx < 1`: store `rec_text` in the first empty slot of `FuturesRecs(1..1000)` and increment the
/// count. Else clear record `rec_idx`, move the last record (index = count) into the gap (if it is not
/// empty and the count is above 1) and decrement the count (not below 0).
pub fn add_or_remove_futures_rec(g: &mut Globals, rec_text: &PbStr, rec_idx: i32) {
    // ON ERROR GOTO handler: ShowRuntimeError only
    let new_rec = rec_text.clone();
    if rec_idx < 1 {
        for slot in 1..=1000 {
            if g.futures_recs[slot].is_empty() {
                g.futures_recs[slot] = new_rec;
                let n = to_long(val(g.futures_recs[0].as_bytes())) + 1;
                g.futures_recs[0] = trim(str_num(cast::<Ext, _>(n), 15).as_bytes());
                break;
            }
        }
    } else {
        g.futures_recs[rec_idx] = PbStr::new();
        let mut last_idx = to_long(val(g.futures_recs[0].as_bytes()));
        if !g.futures_recs[last_idx].is_empty() && last_idx > 1 {
            g.futures_recs[rec_idx] = g.futures_recs[last_idx].clone();
            g.futures_recs[last_idx] = PbStr::new();
        }
        if last_idx > 0 {
            last_idx -= 1;
        } else {
            last_idx = 0;
        }
        g.futures_recs[0] = trim(str_num(cast::<Ext, _>(last_idx), 15).as_bytes());
    }
}

/// Port of SumCommodityHoldings (0x6395e2).
/// Total |quantity| in `CurCommodity`: of the futures records (`futures_mode >= 1`) or the physical
/// holdings (`futures_mode <= 0`), of all holders, or with mode 2 (`futures_mode = 2` resp.
/// `phys_group_mode = 2`) only of holders controlled by `entity_id`'s controller (a player 1..NumPlayers
/// is its own controller). STOCKS futures of the index funds 1584/1585 (industry 71) are skipped, unless
/// `entity_id` is such a fund (then all records count).
pub fn sum_commodity_holdings(g: &Globals, entity_id: i32, futures_mode: i32, phys_group_mode: i32) -> i64 {
    let ent_id = entity_id;
    let commod = g.cur_commodity.clone();
    let mut controller_id = 0;
    let mut total: i64 = 0;
    if futures_mode == 2 || phys_group_mode == 2 {
        if 0 < ent_id && ent_id <= g.num_players {
            controller_id = ent_id;
        } else {
            controller_id = g.corp_controller[ent_id];
        }
    }
    if futures_mode > 0 {
        for rec_idx in 1..=1000 {
            let rec = &g.futures_recs[rec_idx];
            if rec.is_empty() || field(rec, 1) != commod {
                continue;
            }
            let holder_id = to_long(field_val(rec, 2));
            if futures_mode == 2 && controller_id != g.corp_controller[holder_id] {
                continue;
            }
            let qty = to_quad(field_val(rec, 3));
            if 1583 < ent_id && ent_id < 1586 && g.industry_num[ent_id] == 71 && commod == "STOCKS" {
                total = total + qty.abs();
                continue;
            }
            if 1583 < holder_id && holder_id < 1586 && g.industry_num[holder_id] == 71 && commod == "STOCKS" {
                continue;
            }
            total = total + qty.abs();
        }
    } else {
        for rec_idx in 1..=1000 {
            let rec = &g.phys_commod_rec[rec_idx];
            if rec.is_empty() || field(rec, 1) != commod {
                continue;
            }
            let holder_id = to_long(field_val(rec, 2));
            if phys_group_mode == 2 && controller_id != g.corp_controller[holder_id] {
                continue;
            }
            let qty = to_quad(field_val(rec, 3));
            total = total + qty.abs();
        }
    }
    total
}

/// Port of CalcPhysCommodStorageCost (0x6616d9) with its GOSUB CalcStorageCost_Rate (0x66195b).
/// Quarterly storage cost of `entity_id`'s physical commodities: units x rate (OIL/WHEAT/CORN 0.025,
/// GOLD 0.001, SILVER 0.0025 per unit) plus 2% insurance of the position value, times `ExchangeRate`.
/// Crypto holdings are free; 0 when `GetPhysCommodValue(entity, "")` is 0 (which also sets
/// `CiPhysCommodCost`). Unknown commodity names use the default rate 0.025 and the previous record's
/// position value (the GOSUB leaves it unchanged).
pub fn calc_phys_commod_storage_cost(g: &mut Globals, entity_id: i32) -> Ext {
    // ON ERROR GOTO handler: ShowRuntimeError only
    let ent_id = entity_id;
    if get_phys_commod_value(g, ent_id, &PbStr::new()) == Ext::from_f64(0.0) {
        return Ext::from_f64(0.0);
    }
    let mut storage_sum: Ext = Ext::from_f64(0.0);
    let mut insurance_sum: Ext = Ext::from_f64(0.0);
    let mut pos_value: Ext = Ext::from_f64(0.0);
    let mut storage_rate: Ext;
    for rec_idx in 1..=1000 {
        let rec_text = g.phys_commod_rec[rec_idx].clone();
        if rec_text.is_empty() || field_val(&rec_text, 2) != cast::<Ext, _>(ent_id) {
            continue;
        }
        let commod_name = trim(field(&rec_text, 1).as_bytes());
        if commod_name == "BITCOIN" || commod_name == "ETHEREUM" {
            continue;
        }
        let units = to_quad(field_val(&rec_text, 3));
        // GOSUB CalcStorageCost_Rate (0x66195b)
        storage_rate = cast::<Ext, _>(0.025f32);
        if commod_name == "OIL" {
            storage_rate = cast::<Ext, _>(0.025f32);
            pos_value = cast::<Ext, _>(units) * cast::<Ext, _>(g.oil_price);
        } else if commod_name == "GOLD" {
            storage_rate = cast::<Ext, _>(0.001f32);
            pos_value = (cast::<Ext, _>(0.001f32)) * cast::<Ext, _>(g.gold_price) * cast::<Ext, _>(units);
        } else if commod_name == "SILVER" {
            storage_rate = cast::<Ext, _>(0.0025f32);
            pos_value = cast::<Ext, _>(units) * cast::<Ext, _>(g.silver_price) * (cast::<Ext, _>(0.01f32));
        } else if commod_name == "WHEAT" {
            storage_rate = cast::<Ext, _>(0.025f32);
            pos_value = cast::<Ext, _>(units) * cast::<Ext, _>(g.wheat_price) * (cast::<Ext, _>(0.1f32));
        } else if commod_name == "CORN" {
            storage_rate = cast::<Ext, _>(0.025f32);
            pos_value = cast::<Ext, _>(units) * cast::<Ext, _>(g.corn_price) * (cast::<Ext, _>(0.1f32));
        }
        storage_sum = cast::<Ext, _>(units) * storage_rate + storage_sum;
        insurance_sum = (cast::<Ext, _>(0.02f32)) * pos_value + insurance_sum;
    }
    (insurance_sum + storage_sum) * g.exchange_rate
}

/// Locals of [`estimate_corp_hedge_gains`] used by its GOSUB.
struct HedgeFrame {
    corp_id: i32,
    ctrl_type: i32,
    deliv_month: i32,
    deliv_year: i32,
    cost_basis: Ext,
    expiring: i32,
}

/// GOSUB AiCorpHedging_EvalPosition (0x633635) of [`estimate_corp_hedge_gains`]: `expiring` = 1 if the
/// contract delivers within the next simulated quarter-step, 2 for small positions (cost below
/// `500 * ExchangeRate` or fewer than 5 contracts, unless the small-cost position has 5+ contracts and
/// delivers after this year; never for a human-managed corp), else 0.
fn eval_position(g: &mut Globals, f: &mut HedgeFrame, fut_rec: &PbStr) {
    f.expiring = 0;
    let abs_qty = to_long(field_val(fut_rec, 3).abs());
    if !(f.cost_basis >= g.exchange_rate * Ext::from_f64(500.0)) || abs_qty < 5 {
        f.expiring = 2;
        if !(f.cost_basis >= g.exchange_rate * Ext::from_f64(500.0)) && abs_qty > 4 && g.cur_year < f.deliv_year {
            f.expiring = 0;
        }
        if f.ctrl_type == 1 {
            f.expiring = 0;
        }
    }
    if g.cur_year + 1 < f.deliv_year {
        return;
    }
    if g.cur_year < f.deliv_year && f.deliv_month > 3 {
        return;
    }
    if g.cur_year < f.deliv_year && g.cur_quarter < 4 {
        return;
    }
    let mut next_month: Ext = calc_cur_month_of_year(g, f.corp_id);
    let cur_month: Ext = calc_cur_month_of_year(g, g.sim_co_count);
    if cur_month >= next_month {
        next_month = Ext::from_f64(3.0) + next_month;
    }
    let mut next_year = g.cur_year;
    if !(Ext::from_f64(12.0) >= next_month) {
        next_month = next_month - Ext::from_f64(12.0);
        next_year += 1;
    }
    if f.deliv_year < next_year {
        f.expiring = 1;
    }
    if !(cast::<Ext, _>(f.deliv_month) - Ext::from_f64(0.5) >= next_month) && f.deliv_year == next_year {
        f.expiring = 1;
    }
}

/// Port of EstimateCorpHedgeGains (0x6330c1) with its GOSUB AiCorpHedging_EvalPosition (0x633635).
/// Gain a corporation (status 1) would realise this period on the futures positions that hedge its
/// industry (OIL for oil-related industries 7, 8, 11, 16, 23, 25, 52, 69; WHEAT/CORN for 15, 28; STOCKS
/// for 37; GOLD/SILVER for 50; other industries: 0): per position the full gain when expiring (or
/// small), else 20% (10% for delivery after next year) of value minus cost (cost minus value for shorts);
/// losses are dropped when `(CurQuarter + corp) MOD 2 = 1`. Positions of a human-managed corp count only
/// when expiring. Valuation goes through `CalcFuturesContractValue` (RND, state side effects).
pub fn estimate_corp_hedge_gains(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, corp_id_arg: i32) -> Ext {
    // ON ERROR GOTO handler: ShowRuntimeError only
    let corp_id = corp_id_arg;
    if g.entity_status[corp_id] != 1 || val(g.futures_recs[0].as_bytes()) == Ext::from_f64(0.0) {
        return Ext::from_f64(0.0);
    }
    let mut ctrl_type = get_controller_type(g, corp_id);
    if get_managing_player(g, corp_id) == 0 {
        ctrl_type = 0;
    }
    let industry = cast::<Ext, _>(g.industry_num[corp_id]);
    let hedge_commods: &[u8] = if [Ext::from_f64(7.0), Ext::from_f64(8.0), Ext::from_f64(11.0), Ext::from_f64(16.0), Ext::from_f64(23.0), Ext::from_f64(25.0), Ext::from_f64(52.0), Ext::from_f64(69.0)].contains(&industry) {
        b"OIL"
    } else if industry == Ext::from_f64(15.0) || industry == Ext::from_f64(28.0) {
        b"WHEATCORN"
    } else if industry == Ext::from_f64(37.0) {
        b"STOCKS"
    } else if industry == Ext::from_f64(50.0) {
        b"GOLDSILVER"
    } else {
        return Ext::from_f64(0.0);
    };
    let mut f = HedgeFrame { corp_id, ctrl_type, deliv_month: 0, deliv_year: 0, cost_basis: Ext::from_f64(0.0), expiring: 0 };
    let mut total_gain: Ext = Ext::from_f64(0.0);
    let n = to_long(val(g.futures_recs[0].as_bytes()));
    let mut rec_idx = 1;
    while rec_idx <= n {
        let fut_rec = g.futures_recs[rec_idx].clone();
        rec_idx += 1;
        if field_val(&fut_rec, 2) != cast::<Ext, _>(corp_id) {
            continue;
        }
        let commod = field(&fut_rec, 1);
        if instr(1, hedge_commods, commod.as_bytes()) == 0 {
            continue;
        }
        f.deliv_month = to_long(field_val(&fut_rec, 4));
        f.deliv_year = to_long(field_val(&fut_rec, 5));
        f.cost_basis = field_val(&fut_rec, 6);
        eval_position(g, &mut f, &fut_rec);
        if f.ctrl_type == 1 && f.expiring == 0 {
            continue;
        }
        let realize_frac: Ext = if f.expiring > 0 {
            Ext::from_f64(1.0)
        } else if g.cur_year + 2 < f.deliv_year {
            cast::<Ext, _>(0.1f32)
        } else {
            cast::<Ext, _>(0.2f32)
        };
        let cost_portion = f.cost_basis * realize_frac;
        let cur_value = calc_futures_contract_value(g, ui, rng, &fut_rec).abs();
        let mut pos_gain = if field_val(&fut_rec, 3) > Ext::from_f64(0.0) {
            cur_value * realize_frac - cost_portion
        } else {
            cost_portion - cur_value * realize_frac
        };
        if !(pos_gain >= Ext::from_f64(0.0)) && (g.cur_quarter + corp_id) % 2 == 1 {
            pos_gain = Ext::from_f64(0.0);
        }
        total_gain = pos_gain + total_gain;
    }
    total_gain
}

// ---------------------------------------------------------------------------------------------------------
// Market setup
// ---------------------------------------------------------------------------------------------------------

/// Port of InitCommodityMarkets (0x629b50).
/// Sets `CommodityName(1..8)`; if any of the gold, silver, wheat or corn prices is 0, draws the futures
/// bases (1..5 from `RND(-10, 90)`, GOLD/SILVER at least ~30; STOCKS 280), start prices and target prices
/// (gold around 4800 from `RandSeed MOD 800`, silver around 80, wheat 5..7, corn 63..113% of wheat), then
/// runs `SetCommodityPriceRanges` and `UpdateCommodityPrices` (PORT-TODO).
pub fn init_commodity_markets(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    for (k, name) in ["OIL", "GOLD", "SILVER", "WHEAT", "CORN", "STOCKS", "BITCOIN", "ETHEREUM"].iter().enumerate() {
        g.commodity_name[cast::<i32, _>(k) + 1] = PbStr::from(*name);
    }
    if !(g.gold_price == Ext::from_f32(0.0f32) || g.silver_price == Ext::from_f32(0.0f32) || g.wheat_price == Ext::from_f32(0.0f32) || g.corn_price == Ext::from_f32(0.0f32)) {
        return;
    }
    for commod_idx in 1..=5 {
        let mut basis: Ext = int(cast::<Ext, _>(rng.rnd_range(90, -10)));
        if (commod_idx == 2 || commod_idx == 3) && Ext::from_f64(20.0) > basis {
            basis = rng.rnd() * Ext::from_f64(40.0) + Ext::from_f64(30.0);
        }
        g.futures_basis[commod_idx] = cast::<i16, _>(to_quad(basis));
    }
    g.futures_basis[6] = 280;
    // asm: (RandSeed MOD 800 + 4800) - 400
    g.gold_price = cast::<f32, _>(cast::<f32, _>((fmod(cast::<Ext, _>(g.rand_seed), Ext::from_f64(800.0)) + Ext::from_f64(4800.0)) - Ext::from_f64(400.0)));
    let mut t: Ext = rng.rnd() * Ext::from_f64(30.0);
    t = t - Ext::from_f64(15.0);
    t = t + Ext::from_f64(100.0);
    g.gold_target_price = cast::<f32, _>(cast::<f32, _>((cast::<Ext, _>(0.01f32)) * t * cast::<Ext, _>(g.gold_price)));
    g.silver_price = cast::<f32, _>(cast::<f32, _>((fmod(cast::<Ext, _>(g.rand_seed), Ext::from_f64(20.0)) + Ext::from_f64(80.0)) - Ext::from_f64(10.0)));
    t = (rng.rnd() * Ext::from_f64(40.0) + Ext::from_f64(100.0)) - Ext::from_f64(20.0);
    g.silver_target_price = cast::<f32, _>(cast::<f32, _>((cast::<Ext, _>(0.01f32)) * t * cast::<Ext, _>(g.silver_price)));
    g.wheat_price = cast::<f32, _>(cast::<f32, _>((rng.rnd() * Ext::from_f64(2.0) + Ext::from_f64(6.0)) - Ext::from_f64(1.0)));
    t = (rng.rnd() * Ext::from_f64(20.0) + Ext::from_f64(100.0)) - Ext::from_f64(10.0);
    g.wheat_target_price = cast::<f32, _>(cast::<f32, _>((cast::<Ext, _>(0.01f32)) * t * cast::<Ext, _>(g.wheat_price)));
    t = cast::<Ext, _>(rng.rnd_range(113, 63));
    g.corn_price = cast::<f32, _>(cast::<f32, _>((cast::<Ext, _>(0.01f32)) * t * cast::<Ext, _>(g.wheat_price)));
    t = (rng.rnd() * Ext::from_f64(20.0) + Ext::from_f64(100.0)) - Ext::from_f64(10.0);
    g.corn_target_price = cast::<f32, _>(cast::<f32, _>((cast::<Ext, _>(0.01f32)) * t * cast::<Ext, _>(g.wheat_target_price)));
    set_commodity_price_ranges(g, ui, rng);
    crate::logic::market::commodities::update_commodity_prices(g, ui, rng); // 0x62a6a2
}

/// Port of SetCommodityPriceRanges (0x629f0f).
/// Makes the spot prices positive and draws the min/max price bands of gold (only when `PandemicState <
/// 1`; around `MetalsPriceBase`), silver, wheat and corn, pulling prices and target prices back into the
/// bands. Calls `UpdateMetalsPriceBase` twice (gold below/above the base band is reset).
pub fn set_commodity_price_ranges(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    g.oil_price = cast::<f32, _>(g.oil_price.abs());
    g.gold_price = cast::<f32, _>(g.gold_price.abs());
    g.silver_price = cast::<f32, _>(g.silver_price.abs());
    g.wheat_price = cast::<f32, _>(g.wheat_price.abs());
    g.corn_price = cast::<f32, _>(g.corn_price.abs());
    let base = |g: &Globals| cast::<Ext, _>(g.metals_price_base);
    if g.pandemic_state < 1 {
        g.gold_price_min = cast::<f32, _>(cast::<f32, _>(rng.rnd() * Ext::from_f64(2000.0) + Ext::from_f64(2000.0) + base(g)));
        g.gold_price_max = cast::<f32, _>(cast::<f32, _>(rng.rnd() * Ext::from_f64(15000.0) + Ext::from_f64(6000.0) + base(g)));
        if !(update_metals_price_base(g, ui) >= Ext::from_f64(400.0)) {
            g.gold_price = cast::<f32, _>(cast::<f32, _>(rng.rnd() * Ext::from_f64(125.0) + Ext::from_f64(2000.0) + base(g)));
        }
        if update_metals_price_base(g, ui) > Ext::from_f64(4200.0) {
            g.gold_price = cast::<f32, _>(cast::<f32, _>(rng.rnd() * Ext::from_f64(750.0) + Ext::from_f64(20000.0) + base(g)));
        }
        if !(base(g) + Ext::from_f64(22500.0) >= cast::<Ext, _>(g.gold_price_max)) {
            g.gold_price_max = cast::<f32, _>(cast::<f32, _>(base(g) + Ext::from_f64(22500.0)));
        }
        if g.gold_price_min > g.gold_target_price {
            g.gold_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gold_price_min) * (cast::<Ext, _>(1.04f32))));
        }
        if !(g.gold_price_max >= g.gold_target_price) {
            g.gold_target_price = cast::<f32, _>(cast::<f32, _>((cast::<Ext, _>(0.97f32)) * cast::<Ext, _>(g.gold_price_max)));
        }
    }
    g.silver_price_min = cast::<f32, _>(cast::<f32, _>(base(g) * (cast::<Ext, _>(0.01f32)) + Ext::from_f64(20.0)));
    g.silver_price_max = cast::<f32, _>(cast::<f32, _>(base(g) * (cast::<Ext, _>(0.01f32)) + Ext::from_f64(335.0)));
    if g.silver_price_min > g.silver_price {
        let b = base(g) * (cast::<Ext, _>(0.01f32)) + Ext::from_f64(20.0);
        g.silver_price = cast::<f32, _>(cast::<f32, _>(b + (cast::<Ext, _>(3.75f32)) * rng.rnd()));
    }
    if !(g.silver_price_max >= g.silver_price) {
        let b = base(g) * (cast::<Ext, _>(0.01f32)) + Ext::from_f64(320.0);
        g.silver_price = cast::<f32, _>(cast::<f32, _>(b + Ext::from_f64(15.0) * rng.rnd()));
    }
    if g.silver_price_min > g.silver_target_price {
        g.silver_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.silver_price_min) * (cast::<Ext, _>(1.05f32))));
    }
    if !(g.silver_price_max >= g.silver_target_price) {
        g.silver_target_price = cast::<f32, _>(cast::<f32, _>((cast::<Ext, _>(0.96f32)) * cast::<Ext, _>(g.silver_price_max)));
    }
    g.wheat_price_min = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(1.0) * rng.rnd() + (cast::<Ext, _>(1.9f32))));
    g.wheat_price_max = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(8.0)));
    if 1.9f32 > g.wheat_price {
        g.wheat_price = cast::<f32, _>(cast::<f32, _>((cast::<Ext, _>(0.2f32)) * rng.rnd() + (cast::<Ext, _>(1.95f32))));
    }
    if !(Ext::from_f32(18.0f32) >= g.wheat_price) {
        g.wheat_price = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(1.0) * rng.rnd() + Ext::from_f64(17.0)));
    }
    if !(Ext::from_f32(18.0f32) >= g.wheat_price_max) {
        g.wheat_price_max = cast::<f32, _>(Ext::from_f32(18.0f32));
    }
    if g.wheat_price_min > g.wheat_target_price {
        g.wheat_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.wheat_price_min) * (cast::<Ext, _>(1.04f32))));
    }
    if !(g.wheat_price_max >= g.wheat_target_price) {
        g.wheat_target_price = cast::<f32, _>(cast::<f32, _>((cast::<Ext, _>(0.97f32)) * cast::<Ext, _>(g.wheat_price_max)));
    }
    g.corn_price_min = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(2.0) * rng.rnd() + (cast::<Ext, _>(1.75f32))));
    g.corn_price_max = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(7.0)));
    if 1.75f32 > g.corn_price {
        g.corn_price = cast::<f32, _>(cast::<f32, _>((cast::<Ext, _>(0.3f32)) * rng.rnd() + (cast::<Ext, _>(1.8f32))));
    }
    if !(Ext::from_f32(17.0f32) >= g.corn_price) {
        g.corn_price = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(1.0) * rng.rnd() + Ext::from_f64(16.0)));
    }
    if !(Ext::from_f32(17.0f32) >= g.corn_price_max) {
        g.corn_price_max = cast::<f32, _>(Ext::from_f32(17.0f32));
    }
    if g.corn_price_min > g.corn_target_price {
        g.corn_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.corn_price_min) * (cast::<Ext, _>(1.02f32))));
    }
    if !(g.corn_price_max >= g.corn_target_price) {
        g.corn_target_price = cast::<f32, _>(cast::<f32, _>((cast::<Ext, _>(0.98f32)) * cast::<Ext, _>(g.corn_price_max)));
    }
}

/// Port of UpdateMetalsPriceBase (0x63a94e).
/// Recomputes `MetalsPriceBase = ROUND(6000 * (1.0147 ^ quarters elapsed - 1))` (years capped at 35;
/// 0 in the first quarter of the game) and returns 20% of the gold price net of the base (at least
/// 1250). When an old game without a base (`MetalsPriceBase = 0`, more than one year played) gets one,
/// the base is added to `GoldPrice` (max +10%, target +3%); a jump of over 20% sets gold-industry (50)
/// forecast 40 and demand x1.15, over 30% posts "GOLD SOARS, …" (ShowQuickMessage + PostNews).
pub fn update_metals_price_base(g: &mut Globals, ui: &mut dyn Ui) -> Ext {
    let __content = crate::content::current();
    if g.start_year == g.cur_year && g.cur_quarter == 1 {
        g.metals_price_base = 0;
        return cast::<Ext, _>(g.gold_price) * (cast::<Ext, _>(0.2f32));
    }
    let mut years_elapsed = g.cur_year - g.start_year;
    let mut old_gold_price = 0;
    if g.metals_price_base == 0 && years_elapsed > 1 {
        old_gold_price = to_long(cast::<Ext, _>(g.gold_price));
    }
    if years_elapsed > 35 {
        years_elapsed = 35;
    }
    let qtrs_elapsed = g.cur_quarter + years_elapsed * 4 - 1;
    let growth_factor: Ext = pow(cast::<Ext, _>(1.0147f32), cast::<Ext, _>(qtrs_elapsed)) - Ext::from_f64(1.0);
    let new_base = to_long(Ext::from_f64(6000.0) * growth_factor);
    g.metals_price_base = new_base;
    let mut base_gold_price = to_long(cast::<Ext, _>(g.gold_price) - cast::<Ext, _>(new_base));
    if base_gold_price < 1250 {
        base_gold_price = 1250;
    }
    let result: Ext = (cast::<Ext, _>(0.2f32)) * cast::<Ext, _>(base_gold_price);
    if old_gold_price > 0 {
        g.gold_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.metals_price_base) + cast::<Ext, _>(g.gold_price)));
        g.gold_price_max = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gold_price) * (cast::<Ext, _>(1.1f32))));
        g.gold_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gold_price) * (cast::<Ext, _>(1.03f32))));
        if !(cast::<Ext, _>(old_gold_price) * (cast::<Ext, _>(1.2f32)) >= cast::<Ext, _>(g.gold_price)) {
            g.ind_growth_forecast[50] = 40;
            g.ind_demand[50] = to_quad((cast::<Ext, _>(1.15f32)) * cast::<Ext, _>(g.ind_demand[50]));
        }
        if !(cast::<Ext, _>(old_gold_price) * (cast::<Ext, _>(1.3f32)) >= cast::<Ext, _>(g.gold_price)) {
            g.news_msg = PbStr::from(__content.string(5697));
            let mut msg = std::mem::take(&mut g.news_msg);
            show_quick_message(g, ui, &mut msg);
            g.news_msg = msg;
            post_news(g, ui, 0, 0);
        }
        // (UI) label 159 = RIGHT$(FORMAT$(GoldPrice, "* #####,.00"), 9)
    }
    result
}
