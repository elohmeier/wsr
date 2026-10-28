//! Opening futures positions: the contract pick list `SelectFuturesContract` (0x624c1b), the counterparty
//! search `FindFuturesCounterparty` (0x63389c), and the opening paths (`tran_type` 1) of the commodity
//! desk's `FuturesBuyOrCover` (0x61cebb) and `FuturesSellOrShort` (0x620f39), which
//! [`crate::logic::commod_ai::desk`] calls for that mode.

// Keep the original's statement shapes.
#![allow(
    clippy::assign_op_pattern,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord // x87 comparisons: NaN falls to the same side
)]
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use crate::logic::commod_ai::limits::{apply_futures_trade_impact, calc_commod_trade_limit};
use crate::logic::control::update_corp_control;
use crate::logic::earnings::{balance_cash_t_bills, calc_cur_month_of_year};
use crate::logic::finance::{calc_entity_financials, get_controller_type};
use crate::logic::futures::{
    add_or_remove_futures_rec, calc_futures_basis, count_futures_positions, get_commod_data, get_futures_price,
};
use crate::logic::restructure::merge::show_runtime_error;
use crate::logic::sim_entity::quarter_start::pack_holding_arrays;
use crate::pb::ext80::Ext80;
use crate::pb::string::mcase;
use crate::pb::numeric::Cux;

use super::borrow::borrow_to_cover_purchase;

fn mc(x: &PbStr) -> PbStr {
    mcase(x.as_bytes(), CaseMode::Ansi)
}

fn lc(x: &PbStr) -> PbStr {
    lcase(x.as_bytes(), CaseMode::Ansi)
}

fn field(rec: &PbStr, n: i32) -> PbStr {
    parse_any(rec.as_bytes(), b"|", n)
}

fn field_val(rec: &PbStr, n: i32) -> Ext {
    val(field(rec, n).as_bytes())
}

fn is_crypto(x: &PbStr) -> bool {
    *x == "BITCOIN" || *x == "ETHEREUM"
}

/// The label (`STOCK INDEX` for STOCKS) and its lower-case text (crypto: `MCASE$`).
fn labels(commod: &PbStr) -> (PbStr, PbStr) {
    let __content = crate::content::current();
    let mut commod_label = commod.clone();
    if commod_label == "STOCKS" {
        commod_label = s(__content.string(2499));
    }
    let mut commod_lc = lc(&commod_label);
    if is_crypto(commod) {
        commod_lc = mc(commod);
    }
    (commod_label, commod_lc)
}

/// A SINGLE constant as an 80-bit operand.
fn xf(x: f32) -> Ext80 {
    Ext80::from_f32(x)
}

/// Port of SelectFuturesContract (0x624c1b).
///
/// Shows the futures price list of `commod_name` ("GetCorp" pick list, `TextLines`): the spot price line
/// (`@C0001`, marked " [runtime resource wording] " when `CEIL(month position) = ROUND(month
/// position)`) and the next 60 delivery months (`@C0003`..`@C0062`) with the price per unit and the value
/// of one contract (`CUX`: price x 0.1, x0.1 more for 10,000 units / x0.01 for 1,000 units, x
/// `ExchangeRate`). Returns `"month|year|price"` of the picked month, `"1"` for the spot line, `""` when
/// nothing was picked, or `"ERROR"` (unknown commodity or no futures price; the list so far stays in
/// `TextLines`). `trade_mode` (1 buy, 2 sell) only selects an unused word.
pub fn select_futures_contract(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, commod_name: &PbStr, trade_mode: i32) -> PbStr {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError("FutureSelect", …) (error path not ported)
    let mode = trade_mode;
    let commod = commod_name.clone();
    let mut contract_rows: Vec<PbStr> = vec![PbStr::new(); 101];
    let commod_data = get_commod_data(g, ui, &commod, 7);
    if commod_data == "ERROR" {
        return commod_data;
    }
    let units_per_contract = to_quad(field_val(&commod_data, 1));
    let unit_name = field(&commod_data, 2);
    let mut spot_price = Cux(0);
    if commod == "OIL" {
        spot_price.set_ext(Ext80::from_i64(100).fmul(xf(g.oil_price)));
    } else if commod == "GOLD" {
        spot_price.set_ext(xf(g.gold_price));
    } else if commod == "SILVER" {
        spot_price.set_ext(xf(g.silver_price));
    } else if commod == "WHEAT" {
        spot_price.set_ext(xf(g.wheat_price));
    } else if commod == "CORN" {
        spot_price.set_ext(xf(g.corn_price));
    } else if commod == "STOCKS" {
        spot_price.set_ext(Ext80::from_ext(g.stock_index));
    } else if commod == "BITCOIN" {
        spot_price.set_ext(Ext80::from_i64(cast::<i64, _>(g.bitcoin_price)));
    } else if commod == "ETHEREUM" {
        spot_price.set_ext(Ext80::from_i64(cast::<i64, _>(g.ethereum_price)));
    }
    let sim_co_count = g.sim_co_count;
    let month_in_qtr: Ext = crate::pb::numeric::double(calc_cur_month_of_year(g, sim_co_count)); // DOUBLE
    let mut month = to_long(month_in_qtr);
    let mut year = g.cur_year;
    let (commod_label, commod_lc) = labels(&commod);
    g.text_lines[1] = s(__content.string(12176))
        + &mc(&unit_name)
        + " of "
        + &mc(&commod_label)
        + ":  $"
        + &fmt_trim(spot_price.get(), "* ###,.00");
    if ceil(month_in_qtr) == cast::<Ext, _>(month) {
        g.text_lines[1] =
            g.text_lines[1].clone() + __content.string(12177) + &string_fill(80, b" ") + "@C0001";
    } else {
        g.text_lines[1] = g.text_lines[1].clone() + &string_fill(80, b" ") + "@C0001";
    }
    let mut month_ofs = 1;
    while month_ofs <= 60 {
        month = month + 1;
        if 12 < month {
            month = 1;
            year = year + 1;
        }
        let mut line_tag = s("000") + &str_trim(cast::<Ext, _>(2 + month_ofs));
        line_tag = s("@C") + &right(line_tag.as_bytes(), 4);
        let price_str = get_futures_price(g, ui, rng, &commod, month, year, 5);
        if price_str == "ERROR" {
            return price_str;
        }
        let mut fut_price = Cux(0);
        fut_price.set(val(price_str.as_bytes()));
        let mut contract_value = Cux(0);
        contract_value.set_ext(xf(0.1f32).fmul(fut_price.get_ext()));
        if units_per_contract == 10000 {
            contract_value.set_ext(xf(0.1f32).fmul(contract_value.get_ext()));
        }
        if units_per_contract == 1000 {
            contract_value.set_ext(xf(0.01f32).fmul(contract_value.get_ext()));
        }
        contract_value.set_ext(Ext80::from_ext(g.exchange_rate).fmul(contract_value.get_ext()));
        let mut line_text = g.month_name[month].clone() + "," + &str_s(cast::<Ext, _>(year)) + &string_fill(12, b".");
        let mut _cost_word = s("cost");
        if mode == 2 {
            _cost_word = s("price");
        }
        line_text = mid(line_text.as_bytes(), 1, 17)
            + __content.string(12178)
            + &lc(&unit_name)
            + " = $"
            + &trim(price_str.as_bytes())
            + __content.string(12179)
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(contract_value.get(), "#####.00")
            + " "
            + &g.million_prefix_uc
            + "."
            + &g.currency_suffix
            + ")";
        g.text_lines[month_ofs + 2] = line_text + &string_fill(80, b" ") + &line_tag;
        contract_rows[cast::<usize, _>(month_ofs + 2)] = str_s(cast::<Ext, _>(month))
            + "|"
            + &str_trim(cast::<Ext, _>(year))
            + "|"
            + &fmt_trim(fut_price.get(), "#####.00");
        month_ofs += 1;
    }
    g.text_lines[month_ofs + 4] = s(__content.string(12180));
    g.text_lines[month_ofs + 6] = s("@END");
    g.text_box_title = s("     ")
        + &mc(&commod_label)
        + __content.string(12181)
        + &fmt_trim(cast::<Ext, _>(units_per_contract), "* ###,")
        + " "
        + &unit_name
        + "s of "
        + &commod_lc;
    g.entity_pick_mode = s("GetCorp");
    g.list_item_noun = s("item");
    g.text_box_mode = s("64");
    g.target_corp = 0;
    g.text_has_links = 1;
    show_text_report(g, ui, rng);
    let mut result = PbStr::new();
    if g.target_corp != 0 {
        if g.target_corp == 1 {
            result = s("1");
        } else if (1..=100).contains(&g.target_corp) {
            result = contract_rows[cast::<usize, _>(g.target_corp)].clone();
        } else {
            // contractRows(TargetCorp) out of range: error 9, the ON ERROR handler ends the function
            return result;
        }
        g.target_corp = 0;
    }
    result
}

/// Locals of FindFuturesCounterparty shared with its GOSUB FindFuturesCounterparty_CheckCandidate.
struct Counterparty {
    qty_needed: i64,
    contract_value: Ext,
    qty: i64,
    pass_no: i32,
    from_id: i32,
    end_id: i32,
}

/// Port of FindFuturesCounterparty (0x63389c) with its GOSUBs FindFuturesCounterparty_CheckCandidate
/// (0x633a12) and FindFuturesCounterparty_Exit (0x633d17).
///
/// A company [runtime resource wording] of a futures trade of `qty` (sign ignored) contracts worth
/// `contract_value` each: up to 10 scans (two per round when the random start `RND(131, 1580)` —
/// STOCKS: `RND(86, 1580)` — is above 132: start..1580, then 131 (86)..start) for the first active
/// company (no fund) with cash + T-bills of at least `200 x ExchangeRate` and
/// `3% x qty x contract_value`, no futures positions, a credit rating of 7+, a net worth of at least a
/// third of the trade, not controlled by a human player (`GetControllerType <> 1`), whose liquid funds
/// (halved in the first scan) cover `qty_needed` contracts on 12% margin x 0.833. Returns its id, else 0.
/// (`contract_value = 0` raises error 151: the ON ERROR handler's runtime-error message, result 0.)
pub fn find_futures_counterparty(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    qty_needed: i64,
    contract_value: Ext,
    qty: i64,
) -> i32 {
    let mut f = Counterparty {
        qty_needed,
        contract_value,
        qty: qty.wrapping_abs(),
        pass_no: 0,
        from_id: 0,
        end_id: 0,
    };
    let commod = g.cur_commodity.clone();
    loop {
        let mut start_id = rng.rnd_range(1580, 131);
        if commod == "STOCKS" {
            start_id = rng.rnd_range(1580, 86);
        }
        f.from_id = start_id;
        f.end_id = 1580;
        f.pass_no += 1;
        if let Some(r) = counterparty_check(g, ui, rng, &mut f) {
            return r;
        }
        if 132 < start_id {
            f.from_id = 131;
            f.end_id = start_id;
            if commod == "STOCKS" {
                f.from_id = 86;
            }
            f.pass_no += 1;
            if let Some(r) = counterparty_check(g, ui, rng, &mut f) {
                return r;
            }
        }
        if 10 <= f.pass_no {
            return 0;
        }
    }
}

/// Port of FindFuturesCounterparty_CheckCandidate (0x633a12), GOSUB of FindFuturesCounterparty: scans
/// `from_id..=end_id`; `Some(result)` when the routine ends (a counterparty, or 0 after error 151).
fn counterparty_check(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Counterparty) -> Option<i32> {
    let mut cand = f.from_id;
    while cand <= f.end_id {
        'next: {
            if g.entity_status[cand] != 1 {
                break 'next;
            }
            let liquid = g.t_bills[cand] + g.cash[cand];
            if !(g.exchange_rate * Ext::from_f64(200.0) <= liquid) {
                break 'next;
            }
            if cand != 1600 && g.industry_num[cand] == 71 {
                break 'next;
            }
            if !(cast::<Ext, _>(f.qty) * (f.contract_value * sf(0.03f32)) <= g.t_bills[cand] + g.cash[cand]) {
                break 'next;
            }
            if 0 < count_futures_positions(g, cand, 0) {
                break 'next;
            }
            calc_entity_financials(g, ui, rng, cand);
            if f.pass_no == 1 {
                g.liquid_funds = sf(0.5f32) * g.liquid_funds;
            }
            if 7 > g.credit_rating_num[cand] {
                break 'next;
            }
            if !(cast::<Ext, _>(f.qty) * f.contract_value <= g.net_worth * Ext::from_f64(3.0)) {
                break 'next;
            }
            update_corp_control(g, ui, rng, cand);
            if get_controller_type(g, cand) == 1 {
                break 'next;
            }
            if f.contract_value == Ext::from_f64(0.0) {
                // ERROR 151 -> ON ERROR: ShowRuntimeError(ERR, FUNCNAME$, "", 0), result 0
                show_runtime_error(g, ui, 151, &s("FINDFUTURESCOUNTERPARTY"), &PbStr::new(), 0);
                return Some(0);
            }
            let max_qty = to_quad(sf(0.833f32) * (g.liquid_funds / (sf(0.12f32) * f.contract_value)));
            if !((cast::<Ext, _>(max_qty)) < cast::<Ext, _>(f.qty_needed)) {
                return Some(cand);
            }
        }
        cand += 1;
    }
    None
}

// ---------------------------------------------------------------------------------------------------------
// The commodity desk's opening trades (tran_type 1)
// ---------------------------------------------------------------------------------------------------------

/// `"$" + price` (USA) or `"US$ " + price`, formatted `"* ###,.00"` (crypto: `"* ###,"`).
fn price_text(g: &Globals, commod: &PbStr, price: Ext) -> PbStr {
    let mut t = fmt_trim(price, "* ###,.00");
    if is_crypto(commod) {
        t = fmt_trim(price, "* ###,");
    }
    if g.currency_code == "USA" { s("$") + &t } else { s("US$ ") + &t }
}

/// The contract value of one contract at `price` (CUX) per unit: x0.001 / 0.01 / 0.1 for 1,000 / 10,000 /
/// 100,000 units per contract, x `ExchangeRate` (80-bit, like the original's EXT local).
fn contract_value_of(g: &Globals, price: Cux, units_per_contract: i64) -> Ext80 {
    let mut v = price.get_ext();
    if units_per_contract == 1000 {
        v = xf(0.001f32).fmul(v);
    }
    if units_per_contract == 10000 {
        v = xf(0.01f32).fmul(v);
    }
    if units_per_contract == 100000 {
        v = xf(0.1f32).fmul(v);
    }
    Ext80::from_ext(g.exchange_rate).fmul(v)
}

/// `RTRIM$(LTRIM$(FORMAT$(qty * units, "* ############,")))`, `",000,000"` shortened to `" million"`.
fn total_units_text(qty: i64, units: i64) -> PbStr {
    let __content = crate::content::current();
    let mut t = fmt_trim(cast::<Ext, _>(units) * cast::<Ext, _>(qty), "* ############,");
    if 0 < instr(1, t.as_bytes(), b",000,000") {
        let n = len(t.as_bytes());
        t = mid(t.as_bytes(), 1, n - 8) + __content.string(12182);
    }
    t
}

/// The counterparty's side of a new futures trade (`qty` signed for its record): margin and commission
/// paid (6%, 5% to the margin balance), cash/T-bills rebalanced, [runtime resource wording] borrowed from
/// its bank, and its futures record.
#[allow(clippy::too_many_arguments)]
fn counterparty_books(
    g: &mut Globals,
    cp: i32,
    total_value: Ext80,
    commod: &PbStr,
    qty: i64,
    deliv_month: i32,
    deliv_year: i32,
    value_text: &PbStr,
    contract_price: Cux,
) {
    g.cash[cp] = g.cash[cp] - total_value.fmul(xf(0.06f32)).into_ext();
    g.commod_margin_bal[cp] = total_value.fmul(xf(0.05f32)).into_ext() + g.commod_margin_bal[cp];
    balance_cash_t_bills(g, cp);
    if g.cash[cp] < Ext::from_f64(0.0) {
        let shortfall = g.cash[cp].abs();
        g.bank_debt[cp] = shortfall + g.bank_debt[cp];
        let bank = cast::<i32, _>(g.lending_bank[cp]);
        g.cash[bank] = g.cash[bank] - shortfall;
        g.cash[cp] = Ext::from_f64(0.0);
    }
    let new_rec = futures_rec(commod, cp, qty, deliv_month, deliv_year, value_text, contract_price);
    add_or_remove_futures_rec(g, &new_rec, 0);
}

/// A futures record `commod|owner|qty|month|year|value|price`.
fn futures_rec(commod: &PbStr, owner: i32, qty: i64, month: i32, year: i32, value_text: &PbStr, price: Cux) -> PbStr {
    commod.clone()
        + "|"
        + &str_trim(cast::<Ext, _>(owner))
        + "|"
        + &str_trim(cast::<Ext, _>(qty))
        + "|"
        + &str_trim(cast::<Ext, _>(month))
        + "|"
        + &str_trim(cast::<Ext, _>(year))
        + "|"
        + value_text
        + "|"
        + &str_trim(price.get())
}

/// `NewsMsg` posted when shorter than 75 characters, else cleared.
fn post_short_news(g: &mut Globals, ui: &mut dyn Ui) {
    if len(g.news_msg.as_bytes()) < 75 {
        post_news(g, ui, 0, 0);
    } else {
        g.news_msg = PbStr::new();
    }
}

/// The common exit `LAB_00620dc5` of FuturesBuyOrCover: (UI) DIALOG END DlgCommodDesk, the pending
/// message as a quick message, `PackHoldingArrays`, and the temporary `FuturesBasis` bump undone.
fn buy_exit(g: &mut Globals, ui: &mut dyn Ui, msg_text: &mut PbStr, basis_bumped: bool, commod_idx: i32) {
    ui.end_dialog(crate::ui::DialogKind::CommodityDesk);
    if !msg_text.is_empty() {
        show_quick_message(g, ui, msg_text);
    }
    pack_holding_arrays(g, ui);
    if basis_bumped {
        g.futures_basis[commod_idx] = g.futures_basis[commod_idx].wrapping_sub(20);
    }
}

/// The credit-rating refusal text shared by both opening trades (`verb`: "buy futures" / "sell futures
/// short", `tail`: the closing sentence).
fn credit_text(g: &Globals, ent_id: i32, verb: &str, tail: &str) -> PbStr {
    let __content = crate::content::current();
    let mut info_text = s(__content.string(1506));
    if 10 < ent_id {
        info_text = g.ent_name[ent_id].clone() + " has";
    }
    let q = s("\"");
    s(__content.string(12183))
        + __content.string(12184)
        + &q
        + "BBB"
        + &q
        + verb
        + "\r\n"
        + "\r\n"
        + &info_text
        + " a "
        + &q
        + &g.credit_rating_str
        + &q
        + tail
}

/// Port of FuturesBuyOrCover (0x61cebb), opening path (`tran_type` 1), called by
/// [`crate::logic::commod_ai::desk::futures_buy_or_cover`] after its common start.
///
/// `buyer_id` buys `CurCommodity` futures: refused (quick message) below the trading limits
/// (`CalcCommodTradeLimit` probes 99/-99: silently), below a "BBB" rating (7) or with 998+ records.
/// `FuturesBasis` is bumped by 20 for the dialog when it lags `CalcFuturesBasis` by more than 30 (undone
/// on the cancel paths, not after a trade or an "ERROR"). The month is picked with
/// [`select_futures_contract`]; a notice (or "Insufficient Funds" when the liquid funds are below 6% of
/// one contract) follows; the quantity is asked (INPUTBOX, at most what the funds cover on 6% x 0.833, or
/// the exchange limit; in the peak-oil crisis, for the player picked by `OilFutBuyLimitPlayer`, 10 minus
/// the group's long oil contracts). Up to 100 contracts get a counterparty
/// ([`find_futures_counterparty`], its short record and news). A cash shortfall of the 6% outlay is
/// borrowed ([`borrow_to_cover_purchase`], NO cancels). Books 6% cash / 5% margin, the record, the
/// confirmation, the market impact and the news.
pub(crate) fn futures_buy_open(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, buyer_id: i32) {
    let __content = crate::content::current();
    let commod = g.cur_commodity.clone();
    let (commod_label, commod_lc) = labels(&commod);
    let mut msg_text: PbStr;
    let mut basis_bumped = false;
    let mut commod_idx = 0;
    g.tmp_ext1 = Ext::from_f64(99.0);
    if calc_commod_trade_limit(g, ui, buyer_id, 0, 175000) != 99 {
        return;
    }
    g.tmp_ext1 = -Ext::from_f64(99.0);
    if calc_commod_trade_limit(g, ui, buyer_id, 0, 175000) != -99 {
        return;
    }
    calc_entity_financials(g, ui, rng, buyer_id);
    let liquid_funds = g.liquid_funds;
    if 7 > g.credit_rating_num[buyer_id] {
        msg_text = credit_text(
            g,
            buyer_id,
            __content.string(12185),
            __content.string(12186),
        );
        if g.exchange_rate * Ext::from_f64(10.0) < g.t_bills[buyer_id] + g.cash[buyer_id] {
            let mut info_text = s("you");
            let mut subj_verb = s(__content.string(11595));
            if 130 < buyer_id {
                info_text = g.ent_symbol[buyer_id].clone();
                subj_verb = g.ent_symbol[buyer_id].clone() + " has";
            }
            msg_text = msg_text
                + "\r\n"
                + "\r\n"
                + __content.string(12187)
                + &info_text
                + __content.string(12188)
                + &subj_verb
                + __content.string(12189);
        }
        if 10 < buyer_id && 1 < g.credit_rating_num[buyer_id] {
            msg_text = msg_text
                + "\r\n"
                + "\r\n"
                + __content.string(12190)
                + &g.ent_symbol[buyer_id]
                + __content.string(12191);
        }
        buy_exit(g, ui, &mut msg_text, basis_bumped, commod_idx);
        return;
    }
    if !(val(g.futures_recs[0].as_bytes()) < Ext::from_f64(998.0)) {
        msg_text = s(__content.string(12192))
            + __content.string(2714)
            + __content.string(12193);
        buy_exit(g, ui, &mut msg_text, basis_bumped, commod_idx);
        return;
    }
    let data = get_commod_data(g, ui, &commod, 3);
    if data == "ERROR" {
        return;
    }
    let units_per_contract = to_quad(field_val(&data, 1));
    let unit_name = field(&data, 2);
    commod_idx = to_long(field_val(&data, 3));
    let calc_basis: Ext = crate::pb::numeric::double(calc_futures_basis(g, rng, commod_idx)); // DOUBLE
    if g.futures_basis[commod_idx] <= 280 && cast::<Ext, _>(g.futures_basis[commod_idx]) + Ext::from_f64(30.0) < calc_basis {
        g.futures_basis[commod_idx] = g.futures_basis[commod_idx].wrapping_add(20);
        basis_bumped = true;
    }
    let contract_sel = select_futures_contract(g, ui, rng, &commod, 1);
    if contract_sel == "ERROR" {
        return;
    }
    if contract_sel == "1" || contract_sel.is_empty() {
        msg_text = s(__content.string(12194));
        if contract_sel.is_empty() {
            msg_text = s(__content.string(12195));
            // (UI) DIALOG END DlgCommodDesk
        }
        buy_exit(g, ui, &mut msg_text, basis_bumped, commod_idx);
        return;
    }
    let deliv_month = to_long(field_val(&contract_sel, 1));
    let deliv_year = to_long(field_val(&contract_sel, 2));
    let mut contract_price = Cux(0);
    contract_price.set(field_val(&contract_sel, 3));
    let cv = contract_value_of(g, contract_price, units_per_contract);
    let contract_value = cv.into_ext();
    let price_str = price_text(g, &commod, contract_price.get());
    let shortfall = liquid_funds < cv.fmul(xf(0.06f32)).into_ext();
    let mut info_text = s("One ")
        + &commod_lc
        + __content.string(12196)
        + &g.month_name[deliv_month]
        + ","
        + &str_s(cast::<Ext, _>(deliv_year))
        + __content.string(12197)
        + &fmt_trim(cast::<Ext, _>(units_per_contract), "* ###,")
        + " "
        + &lc(&unit_name)
        + "s at "
        + &price_str
        + " per "
        + &lc(&unit_name)
        + __content.string(12198)
        + &trim(g.currency_symbol.as_bytes())
        + &fmt_trim(Ext::from(contract_value), "* #####,.00")
        + " "
        + &g.million_prefix_lc
        + "illion"
        + &g.currency_suffix
        + "."
        + "\r"
        + "\r"
        + __content.string(12199)
        + __content.string(12200)
        + __content.string(12201)
        + "\r"
        + "\r";
    if shortfall {
        msg_text = s("you");
        if 10 < buyer_id {
            msg_text = g.ent_name[buyer_id].clone();
        }
        info_text = info_text
            + __content.string(12202)
            + &msg_text
            + __content.string(12203)
            + "\r"
            + "\r"
            + __content.string(12204);
        ui.msgbox(&info_text, 8208, &s(__content.string(12046)));
        msg_text = PbStr::new();
        buy_exit(g, ui, &mut msg_text, basis_bumped, commod_idx);
        return;
    }
    info_text = info_text
        + __content.string(12205)
        + __content.string(12206)
        + &commod_lc
        + "."
        + "\r"
        + "\r"
        + __content.string(12207)
        + __content.string(12208)
        + __content.string(12209)
        + __content.string(12210);
    ui.msgbox(&info_text, 8256, &(s(__content.string(12211)) + &mc(&commod_label) + __content.string(12212)));
    if contract_value == Ext::from_f64(0.0) {
        // ERROR 151 -> ON ERROR: ShowRuntimeError(ERR, "FrontBuyCommod", "CommodNam$ = " + commod, buyerId)
        show_runtime_error(g, ui, 151, &s(__content.string(12213)), &(s(__content.string(12214)) + &commod), buyer_id);
        return;
    }
    let mut max_qty = to_quad(
        xf(0.833f32)
            .fmul(Ext80::from_ext(liquid_funds).fdiv(xf(0.06f32).fmul(cv)))
            .into_ext(),
    );
    if (cast::<Ext, _>(max_qty)) < Ext::from_f64(1.0) {
        max_qty = 1;
    }
    let max_affordable = max_qty;
    let oil_crisis = commod == "OIL" && g.peak_oil_crisis == 1 && contract_price.get() < Ext::from_f64(645.0) && Ext::from_f64(10.0) < cast::<Ext, _>(max_qty);
    if oil_crisis {
        if g.cur_player == g.oil_fut_buy_limit_player {
            g.oil_fut_buy_limit_player = -g.cur_player;
        }
        if (cast::<Ext, _>(g.oil_fut_buy_limit_player)).abs() != cast::<Ext, _>(g.cur_player) {
            if g.rand_seed % 10 == 1 {
                g.oil_fut_buy_limit_player = g.cur_player;
            } else {
                g.oil_fut_buy_limit_player = -g.cur_player;
            }
        }
    }
    let mut regulator_name = PbStr::new();
    if oil_crisis && -g.cur_player == g.oil_fut_buy_limit_player {
        regulator_name = s(__content.string(2720));
        if g.currency_code != "USA" {
            regulator_name = replace(regulator_name.as_bytes(), __content.bytes(2721), __content.bytes(2722));
        }
        // (maxQty = 10, overwritten below)
        msg_text = s(__content.string(12215))
            + __content.string(12216)
            + __content.string(12217)
            + &regulator_name
            + ". ";
        let mut held_contracts: i64 = 0;
        for scan_idx in 1..=1000 {
            if g.futures_recs[scan_idx].is_empty() {
                continue;
            }
            let fut_rec = g.futures_recs[scan_idx].clone();
            if commod != trim(field(&fut_rec, 1).as_bytes()) {
                continue;
            }
            let holder_id = to_long(field_val(&fut_rec, 2));
            if buyer_id != holder_id && g.cur_player != g.corp_controller[holder_id] {
                continue;
            }
            if !(field_val(&fut_rec, 3) >= Ext::from_f64(0.0)) {
                continue;
            }
            held_contracts = to_quad(field_val(&fut_rec, 3) + cast::<Ext, _>(held_contracts));
        }
        max_qty = to_quad(Ext::from_f64(10.0) - cast::<Ext, _>(held_contracts));
        if !((cast::<Ext, _>(held_contracts)) < Ext::from_f64(10.0)) {
            msg_text = s(__content.string(12218))
                + __content.string(12219)
                + __content.string(12220)
                + "\r\n"
                + "\r\n"
                + __content.string(12221)
                + __content.string(12222)
                + "\r\n"
                + "\r\n"
                + __content.string(12223)
                + &ucase(regulator_name.as_bytes(), CaseMode::Ansi);
            if g.currency_code != "USA" {
                msg_text = replace(msg_text.as_bytes(), b"DEPARTMENT", b"MINISTRY");
            }
        } else if Ext::from_f64(0.0) < cast::<Ext, _>(held_contracts) {
            msg_text = msg_text
                + "\r\n"
                + "\r\n"
                + __content.string(12224)
                + &str_s(cast::<Ext, _>(held_contracts))
                + __content.string(12225)
                + __content.string(12226)
                + &str_s(cast::<Ext, _>(max_qty))
                + __content.string(12227);
        }
        if !((cast::<Ext, _>(held_contracts)) < Ext::from_f64(10.0)) {
            buy_exit(g, ui, &mut msg_text, basis_bumped, commod_idx);
            return;
        }
        show_quick_message(g, ui, &mut msg_text);
    } else {
        max_qty = calc_commod_trade_limit(g, ui, buyer_id, 0, max_affordable);
        if max_qty == 0 {
            msg_text = s(__content.string(12228)) + &lc(&commod) + __content.string(12229);
            buy_exit(g, ui, &mut msg_text, basis_bumped, commod_idx);
            return;
        }
    }
    let mut counterparty_id = 0;
    let qty: i64;
    loop {
        msg_text = s(__content.string(2508))
            + &commod_lc
            + __content.string(2509)
            + &g.month_name[deliv_month]
            + ","
            + &str_s(cast::<Ext, _>(deliv_year))
            + __content.string(12230);
        if max_qty == max_affordable {
            msg_text = msg_text
                + __content.string(12231)
                + &fmt_trim(cast::<Ext, _>(max_qty), "* ########,")
                + __content.string(12232);
        } else if (cast::<Ext, _>(max_qty)) < cast::<Ext, _>(max_affordable) {
            msg_text = msg_text + __content.string(12233);
            if !regulator_name.is_empty() {
                msg_text = replace(msg_text.as_bytes(), __content.bytes(12234), regulator_name.as_bytes());
            }
        }
        let title = g.ent_symbol[buyer_id].clone() + " -- " + &mc(&commod_label) + __content.string(2513);
        let default = fmt_trim(cast::<Ext, _>(max_qty), "* ###,") + __content.string(2514);
        let input_str = ui.inputbox(&msg_text, &title, &default);
        let n = to_quad(val(remove_any(input_str.as_bytes(), b"$ ,").as_bytes()).abs());
        if (cast::<Ext, _>(n)) < Ext::from_f64(1.0) {
            msg_text = PbStr::new();
            buy_exit(g, ui, &mut msg_text, basis_bumped, commod_idx);
            return;
        }
        if !(cast::<Ext, _>(n) <= cast::<Ext, _>(max_qty)) {
            continue;
        }
        max_qty = n;
        if cast::<Ext, _>(n) <= Ext::from_f64(100.0) {
            counterparty_id = find_futures_counterparty(g, ui, rng, max_qty, Ext::from(contract_value), n);
            calc_entity_financials(g, ui, rng, buyer_id);
            if 10 < counterparty_id {
                let mut m = s(__content.string(12235))
                    + &g.ent_name[counterparty_id]
                    + " ("
                    + &g.ent_symbol[counterparty_id]
                    + ").";
                show_quick_message(g, ui, &mut m);
            }
        }
        qty = n;
        break;
    }
    let total_value = Ext80::from_i64(qty).fmul(cv);
    let outlay = total_value.fmul(xf(0.06f32)).into_ext();
    if g.t_bills[buyer_id] + g.cash[buyer_id] < outlay {
        calc_entity_financials(g, ui, rng, buyer_id);
        let mut shortfall_amt = outlay - (g.t_bills[buyer_id] + g.cash[buyer_id]);
        let commission = total_value.fmul(xf(0.01f32)).into_ext();
        if borrow_to_cover_purchase(g, ui, &mut shortfall_amt, Ext::from(commission), 0) == 7 {
            msg_text = PbStr::new();
            buy_exit(g, ui, &mut msg_text, basis_bumped, commod_idx);
            return;
        }
    }
    g.cash[buyer_id] = g.cash[buyer_id] - outlay;
    g.commod_margin_bal[buyer_id] = total_value.fmul(xf(0.05f32)).into_ext() + g.commod_margin_bal[buyer_id];
    let value_text = fmt_trim(Ext::from(total_value.into_ext()), "##############.00");
    if 10 < counterparty_id {
        counterparty_books(g, counterparty_id, total_value, &commod, -qty, deliv_month, deliv_year, &value_text, contract_price);
        g.news_msg = g.ent_symbol[counterparty_id].clone()
            + __content.string(12236)
            + &g.month_name[deliv_month]
            + ","
            + &str_s(cast::<Ext, _>(deliv_year))
            + " "
            + &commod_lc
            + __content.string(12237)
            + &price_str
            + ".";
        post_short_news(g, ui);
    }
    let new_rec = futures_rec(&commod, buyer_id, qty, deliv_month, deliv_year, &value_text, contract_price);
    add_or_remove_futures_rec(g, &new_rec, 0);
    let total_units_str = total_units_text(qty, units_per_contract);
    msg_text = s(" (");
    if Ext::from_f64(1.0) < cast::<Ext, _>(qty) {
        msg_text = s("s (");
    }
    let mut subj = s(__content.string(1506));
    if 10 < buyer_id {
        subj = s("Buyer (") + &g.ent_symbol[buyer_id] + ") has";
    }
    let mut confirm_text = s(__content.string(12238))
        + "\r"
        + "\r"
        + __content.string(2518)
        + &ucase(g.ent_name[buyer_id].as_bytes(), CaseMode::Ansi)
        + "\r"
        + "\r";
    let bought_text = subj
        + __content.string(2763)
        + &fmt_trim(cast::<Ext, _>(qty), "* ##,")
        + __content.string(2538)
        + &msg_text
        + &fmt_trim(cast::<Ext, _>(units_per_contract), "* ###,")
        + " "
        + &unit_name
        + __content.string(12239)
        + "\r"
        + "\r";
    confirm_text = confirm_text
        + &bought_text
        + __content.string(12240)
        + &commod_lc
        + __content.string(12241)
        + &price_str
        + " per "
        + &unit_name
        + ", "
        + __content.string(12242)
        + &g.month_name[deliv_month]
        + ","
        + &str_s(cast::<Ext, _>(deliv_year))
        + ", "
        + "for "
        + &total_units_str
        + " "
        + &unit_name
        + "s. ";
    if instr(1, commod_label.as_bytes(), b"INDEX") == 0 && g.player_take_delivery[g.cur_player] == 1 {
        confirm_text = confirm_text
            + __content.string(12243)
            + __content.string(12244)
            + __content.string(12245);
    }
    confirm_text = confirm_text
        + "\r"
        + "\r"
        + __content.string(12246)
        + __content.string(12247)
        + "of "
        + &trim(g.currency_symbol.as_bytes())
        + &fmt_trim(Ext::from(outlay), "* ###########,.00")
        + " "
        + &g.million_prefix_lc
        + "illion"
        + &g.currency_suffix
        + ".";
    if commod_label == __content.string(2499) || commod_label == "BITCOIN" || commod_label == "ETHEREUM" {
        confirm_text = replace(confirm_text.as_bytes(), b"COMMODITY", commod_label.as_bytes());
    }
    ui.msgbox(&confirm_text, 8256, &(commod_label.clone() + __content.string(2524)));
    g.turn_action_count = g.turn_action_count + 1;
    apply_futures_trade_impact(g, ui, rng, &commod, 1, qty);
    g.news_msg = g.ent_symbol[buyer_id].clone()
        + " buys "
        + &g.month_name[deliv_month]
        + ","
        + &str_s(cast::<Ext, _>(deliv_year))
        + " "
        + &commod_lc
        + __content.string(12248)
        + &price_str
        + ".";
    post_short_news(g, ui);
    pack_holding_arrays(g, ui);
}

/// The common exit `LAB_00624abc` of FuturesSellOrShort: (UI) DIALOG END DlgCommodDesk, the pending
/// message as a quick message, `PackHoldingArrays`.
fn sell_exit(g: &mut Globals, ui: &mut dyn Ui, msg_text: &mut PbStr) {
    ui.end_dialog(crate::ui::DialogKind::CommodityDesk);
    if !msg_text.is_empty() {
        show_quick_message(g, ui, msg_text);
    }
    pack_holding_arrays(g, ui);
}

/// Port of FuturesSellOrShort (0x620f39), opening path (`tran_type` 1), called by
/// [`crate::logic::commod_ai::desk::futures_sell_or_short`] after its common start (`player_net_worth`
/// is the current player's `NetWorth` computed there).
///
/// `seller_id` sells `CurCommodity` futures short: the same checks as the purchase ("BBB", 998 records,
/// trading limits) plus the peak-oil suspension of oil short sales (crisis stage 13+, oil above 2.75 and a
/// player net worth above 40,000 x `ExchangeRate`: MSGBOX). Contract pick list, notice / "Insufficient
/// Funds for Short Sale", quantity (in a falling-oil crisis for the player picked by
/// `OilFutShortLimitPlayer`: 10 plus the group's short oil contracts; this also sets `FuturesBasis(1)`
/// to -100), counterparty for up to 100 contracts (long record, news), the 6% outlay (borrowed if short:
/// NO ends without a message), the short record, confirmation, news, packing and the market impact
/// (direction -1).
pub(crate) fn futures_sell_open(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, seller_id: i32, player_net_worth: Ext) {
    let __content = crate::content::current();
    let commod = g.cur_commodity.clone();
    let (commod_label, commod_lc) = labels(&commod);
    let mut msg_text: PbStr;
    g.tmp_ext1 = Ext::from_f64(99.0);
    if calc_commod_trade_limit(g, ui, seller_id, 0, 175000) != 99 {
        return;
    }
    g.tmp_ext1 = -Ext::from_f64(99.0);
    if calc_commod_trade_limit(g, ui, seller_id, 0, 175000) != -99 {
        return;
    }
    calc_entity_financials(g, ui, rng, seller_id);
    let liquid_funds = g.liquid_funds;
    if 7 > g.credit_rating_num[seller_id] {
        msg_text = credit_text(
            g,
            seller_id,
            __content.string(12249),
            __content.string(12250),
        );
        if 10 < seller_id && 1 < g.credit_rating_num[seller_id] {
            msg_text = msg_text
                + "\r\n"
                + "\r\n"
                + __content.string(12251)
                + &g.ent_symbol[seller_id]
                + __content.string(12191);
        }
        sell_exit(g, ui, &mut msg_text);
        return;
    }
    if !(val(g.futures_recs[0].as_bytes()) < Ext::from_f64(998.0)) {
        msg_text = s(__content.string(12252))
            + __content.string(2714)
            + __content.string(12193);
        sell_exit(g, ui, &mut msg_text);
        return;
    }
    if commod == "OIL"
        && g.peak_oil_crisis == 1
        && 12 < g.oil_crisis_stage
        && sf(2.75f32) < cast::<Ext, _>(g.oil_price)
        && g.exchange_rate * Ext::from_f64(40000.0) < player_net_worth
    {
        msg_text = s(__content.string(12253))
            + __content.string(12254)
            + __content.string(12255)
            + "\r"
            + "\r"
            + __content.string(12256);
        if g.currency_code != "USA" {
            msg_text = replace(msg_text.as_bytes(), b"DEPARTMENT", b"MINISTRY");
        }
        ui.msgbox(&msg_text, 8208, &s(__content.string(12257)));
        return;
    }
    let data = get_commod_data(g, ui, &commod, 5);
    if data == "ERROR" {
        return;
    }
    let units_per_contract = cast::<i64, _>(to_long(field_val(&data, 1)));
    let unit_name = field(&data, 2);
    let contract_sel = select_futures_contract(g, ui, rng, &commod, 2);
    if contract_sel == "ERROR" {
        return;
    }
    if contract_sel == "1" || contract_sel.is_empty() {
        msg_text = s(__content.string(12194));
        if contract_sel.is_empty() {
            msg_text = s(__content.string(12195));
        }
        show_quick_message(g, ui, &mut msg_text);
        if contract_sel.is_empty() {
            ui.end_dialog(crate::ui::DialogKind::CommodityDesk);
        }
        return;
    }
    let deliv_month = to_long(field_val(&contract_sel, 1));
    let deliv_year = to_long(field_val(&contract_sel, 2));
    let mut contract_price = Cux(0);
    contract_price.set(field_val(&contract_sel, 3));
    let cv = contract_value_of(g, contract_price, units_per_contract);
    let contract_value = cv.into_ext();
    let price_str = price_text(g, &commod, contract_price.get());
    let shortfall = liquid_funds < cv.fmul(xf(0.06f32)).into_ext();
    let mut info_text = s("One ")
        + &commod_lc
        + __content.string(12196)
        + &g.month_name[deliv_month]
        + ","
        + &str_s(cast::<Ext, _>(deliv_year))
        + " "
        + __content.string(12258)
        + &fmt_trim(cast::<Ext, _>(units_per_contract), "* ##############,")
        + " "
        + &lc(&unit_name)
        + "s "
        + "at "
        + &price_str
        + " per "
        + &lc(&unit_name)
        + " is "
        + __content.string(12259)
        + &trim(g.currency_symbol.as_bytes())
        + &fmt_trim(Ext::from(contract_value), "* #####,.00")
        + " "
        + &g.million_prefix_lc
        + "illion"
        + &g.currency_suffix
        + "."
        + "\r"
        + "\r"
        + __content.string(12199)
        + __content.string(12200)
        + __content.string(12201)
        + "\r"
        + "\r";
    if shortfall {
        msg_text = s("you");
        if 10 < seller_id {
            msg_text = g.ent_name[seller_id].clone();
        }
        info_text = info_text
            + __content.string(12260)
            + &msg_text
            + __content.string(12203)
            + "\r"
            + "\r"
            + __content.string(12261);
        ui.msgbox(&info_text, 8208, &s(__content.string(12262)));
        msg_text = PbStr::new();
        sell_exit(g, ui, &mut msg_text);
        return;
    }
    info_text = info_text
        + __content.string(12263)
        + __content.string(12206)
        + &commod_lc
        + "."
        + "\r"
        + "\r"
        + __content.string(12264)
        + __content.string(12265)
        + __content.string(12266)
        + "\r"
        + "\r"
        + __content.string(12267)
        + __content.string(12268);
    if 0 < instr(1, info_text.as_bytes(), b"stock") {
        info_text = replace(info_text.as_bytes(), b"stock", __content.bytes(12269));
    }
    if 0 < instr(1, commod_label.as_bytes(), b"INDEX") {
        info_text = replace(info_text.as_bytes(), __content.bytes(2644), __content.bytes(2547));
    }
    ui.msgbox(&info_text, 8256, &(s(__content.string(12270)) + &mc(&commod_label) + __content.string(12212)));
    if contract_value == Ext::from_f64(0.0) {
        // ERROR 151 -> ON ERROR: ShowRuntimeError(ERR, "FrontSellCommod", …, sellerId)
        let diag = s(__content.string(12271))
            + &str_s(cast::<Ext, _>(seller_id))
            + __content.string(12272)
            + &str_s(Ext::from_f64(1.0))
            + __content.string(12273)
            + &str_s(Ext::from_f64(0.0))
            + ")";
        show_runtime_error(g, ui, 151, &s(__content.string(12274)), &diag, seller_id);
        return;
    }
    let mut max_qty = to_quad(
        xf(0.833f32)
            .fmul(Ext80::from_ext(liquid_funds).fdiv(xf(0.06f32).fmul(cv)))
            .into_ext(),
    );
    if (cast::<Ext, _>(max_qty)) < Ext::from_f64(1.0) {
        max_qty = 1;
    }
    let max_affordable = max_qty;
    let oil_crisis = commod == "OIL"
        && g.peak_oil_crisis == -1
        && Ext::from_f64(0.0) < cast::<Ext, _>(g.oil_crisis_level)
        && sf(1.48f32) < cast::<Ext, _>(g.oil_price);
    if oil_crisis {
        if g.cur_player == g.oil_fut_short_limit_player {
            g.oil_fut_short_limit_player = -g.cur_player;
        }
        if (cast::<Ext, _>(g.oil_fut_short_limit_player)).abs() != cast::<Ext, _>(g.cur_player) {
            if g.rand_seed % 10 == 9 {
                g.oil_fut_short_limit_player = g.cur_player;
            } else {
                g.oil_fut_short_limit_player = -g.cur_player;
            }
        }
    }
    let mut regulator_name = PbStr::new();
    if oil_crisis && -g.cur_player == g.oil_fut_short_limit_player {
        regulator_name = s(__content.string(2720));
        if g.currency_code != "USA" {
            regulator_name = replace(regulator_name.as_bytes(), __content.bytes(2721), __content.bytes(2722));
        }
        msg_text = s(__content.string(12275))
            + &regulator_name
            + ", "
            + __content.string(12276)
            + __content.string(12277)
            + __content.string(12278);
        g.futures_basis[1] = -100;
        max_qty = 10;
        let mut held_short_qty: i64 = 0;
        for scan_idx in 1..=1000 {
            if g.futures_recs[scan_idx].is_empty() {
                continue;
            }
            let fut_rec = g.futures_recs[scan_idx].clone();
            if commod != trim(field(&fut_rec, 1).as_bytes()) {
                continue;
            }
            let holder_id = to_long(field_val(&fut_rec, 2));
            if seller_id != holder_id && g.cur_player != g.corp_controller[holder_id] {
                continue;
            }
            if !(field_val(&fut_rec, 3) <= Ext::from_f64(0.0)) {
                continue;
            }
            held_short_qty = to_quad(field_val(&fut_rec, 3) + cast::<Ext, _>(held_short_qty));
        }
        let position_text = s(__content.string(12279));
        if !(cast::<Ext, _>(held_short_qty) > -Ext::from_f64(10.0)) {
            msg_text = msg_text
                + "\r\n"
                + "\r\n"
                + &position_text
                + __content.string(12280)
                + &fmt_trim((cast::<Ext, _>(held_short_qty)).abs(), "* ###,")
                + __content.string(12281)
                + "\r\n"
                + "\r\n"
                + "Thus, "
                + &regulator_name
                + __content.string(12282)
                + __content.string(12283);
            show_quick_message(g, ui, &mut msg_text);
            return;
        }
        if (cast::<Ext, _>(held_short_qty)) < Ext::from_f64(0.0) {
            max_qty = to_quad(cast::<Ext, _>(held_short_qty) + Ext::from_f64(10.0));
            msg_text = msg_text
                + "\r\n"
                + "\r\n"
                + &position_text
                + __content.string(12284)
                + &str_s((cast::<Ext, _>(held_short_qty)).abs())
                + __content.string(12285);
        }
        show_quick_message(g, ui, &mut msg_text);
    } else {
        max_qty = calc_commod_trade_limit(g, ui, seller_id, 0, max_affordable);
        if max_qty == 0 {
            let mut m = s(__content.string(12228)) + &lc(&commod) + __content.string(12229);
            show_quick_message(g, ui, &mut m);
            return;
        }
    }
    let mut counterparty_id = 0;
    let qty: i64;
    loop {
        msg_text = s(__content.string(2508))
            + &commod_lc
            + __content.string(2509)
            + &g.month_name[deliv_month]
            + ","
            + &str_s(cast::<Ext, _>(deliv_year))
            + __content.string(12286);
        if max_qty == max_affordable {
            msg_text = msg_text
                + __content.string(12287)
                + &fmt_trim(cast::<Ext, _>(max_qty), "* #######,")
                + __content.string(12232);
        } else if (cast::<Ext, _>(max_qty)) < cast::<Ext, _>(max_affordable) {
            msg_text = msg_text + __content.string(12288);
            if !regulator_name.is_empty() {
                msg_text = replace(msg_text.as_bytes(), __content.bytes(12234), regulator_name.as_bytes());
            }
        }
        let title = g.ent_symbol[seller_id].clone() + " -- " + &mc(&commod_label) + __content.string(12289);
        let default = fmt_trim(cast::<Ext, _>(max_qty), "* ###,") + __content.string(2514);
        let input_str = ui.inputbox(&msg_text, &title, &default);
        let n = to_quad(val(remove_any(input_str.as_bytes(), b"$ ,").as_bytes()).abs());
        if (cast::<Ext, _>(n)) < Ext::from_f64(1.0) {
            msg_text = PbStr::new();
            sell_exit(g, ui, &mut msg_text);
            return;
        }
        if !(cast::<Ext, _>(n) <= cast::<Ext, _>(max_qty)) {
            continue;
        }
        max_qty = n;
        if cast::<Ext, _>(n) <= Ext::from_f64(100.0) {
            counterparty_id = find_futures_counterparty(g, ui, rng, max_qty, Ext::from(contract_value), -n);
            calc_entity_financials(g, ui, rng, seller_id);
            if 10 < counterparty_id {
                let mut m = s(__content.string(12290))
                    + &g.ent_name[counterparty_id]
                    + " ("
                    + &g.ent_symbol[counterparty_id]
                    + ")";
                show_quick_message(g, ui, &mut m);
            }
        }
        qty = n;
        break;
    }
    let total_value = Ext80::from_i64(qty).fmul(cv);
    let outlay = total_value.fmul(xf(0.06f32)).into_ext();
    if g.t_bills[seller_id] + g.cash[seller_id] < outlay {
        calc_entity_financials(g, ui, rng, seller_id);
        let mut shortfall_amt = outlay - (g.t_bills[seller_id] + g.cash[seller_id]);
        let commission = total_value.fmul(xf(0.01f32)).into_ext();
        if borrow_to_cover_purchase(g, ui, &mut shortfall_amt, Ext::from(commission), 0) == 7 {
            return;
        }
    }
    g.cash[seller_id] = g.cash[seller_id] - outlay;
    g.commod_margin_bal[seller_id] = total_value.fmul(xf(0.05f32)).into_ext() + g.commod_margin_bal[seller_id];
    let value_text = fmt_trim(Ext::from(total_value.into_ext()), "##############.00");
    if 10 < counterparty_id {
        counterparty_books(g, counterparty_id, total_value, &commod, qty, deliv_month, deliv_year, &value_text, contract_price);
    }
    let mut verb_text = s(__content.string(12291));
    if g.rand_seed % 3 == 1 {
        verb_text = s(__content.string(12292));
    }
    if g.rand_seed % 3 == 2 {
        verb_text = s(__content.string(12293));
    }
    if 10 < counterparty_id {
        g.news_msg = g.ent_symbol[counterparty_id].clone()
            + &verb_text
            + &g.month_name[deliv_month]
            + ","
            + &str_s(cast::<Ext, _>(deliv_year))
            + " "
            + &commod_lc
            + __content.string(2520)
            + __content.string(12294)
            + &price_str
            + ".";
        post_short_news(g, ui);
    }
    let new_rec = futures_rec(&commod, seller_id, -qty, deliv_month, deliv_year, &value_text, contract_price);
    add_or_remove_futures_rec(g, &new_rec, 0);
    let total_units_str = total_units_text(qty, units_per_contract);
    msg_text = s(" (");
    if Ext::from_f64(1.0) < cast::<Ext, _>(qty) {
        msg_text = s("s (");
    }
    let mut subj = s(__content.string(1506));
    if 10 < seller_id {
        subj = s(__content.string(2535)) + &g.ent_symbol[seller_id] + ") has";
    }
    let mut confirm_text = s(__content.string(12295))
        + "\r"
        + "\r"
        + __content.string(12296)
        + &ucase(g.ent_name[seller_id].as_bytes(), CaseMode::Ansi)
        + "\r"
        + "\r"
        + &subj
        + __content.string(12297)
        + &fmt_trim(cast::<Ext, _>(qty), "* ##,")
        + __content.string(2538)
        + &msg_text
        + &fmt_trim(cast::<Ext, _>(units_per_contract), "* ###,")
        + " "
        + &unit_name
        + "s "
        + "of "
        + &commod_lc
        + __content.string(12298)
        + "\r"
        + "\r"
        + __content.string(12299)
        + &commod_lc
        + __content.string(12300)
        + &price_str
        + " per "
        + &unit_name
        + __content.string(12301)
        + &g.month_name[deliv_month]
        + ","
        + &str_s(cast::<Ext, _>(deliv_year))
        + ", for "
        + &total_units_str
        + " "
        + &unit_name
        + "s."
        + "\r"
        + "\r"
        + __content.string(12302)
        + __content.string(12303)
        + __content.string(12304)
        + &trim(g.currency_symbol.as_bytes())
        + &fmt_trim(Ext::from(outlay), "* ###########,.00")
        + " "
        + &g.million_prefix_lc
        + "illion"
        + &g.currency_suffix
        + ".";
    if commod_label == __content.string(2499) || commod_label == "BITCOIN" || commod_label == "ETHEREUM" {
        confirm_text = replace(confirm_text.as_bytes(), b"COMMODITY", commod_label.as_bytes());
    }
    ui.msgbox(&confirm_text, 8256, &(commod_label.clone() + __content.string(2524)));
    g.turn_action_count = g.turn_action_count + 1;
    g.news_msg = g.ent_symbol[seller_id].clone()
        + __content.string(12305)
        + &g.month_name[deliv_month]
        + ","
        + &str_s(cast::<Ext, _>(deliv_year))
        + " "
        + &commod_lc
        + __content.string(2520)
        + "@ "
        + &price_str
        + ".";
    post_short_news(g, ui);
    pack_holding_arrays(g, ui);
    apply_futures_trade_impact(g, ui, rng, &commod, -1, qty);
}
