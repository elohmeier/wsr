//! Closing all option positions on a stock (`CloseAllOptionPositions` 0x611ab6), the options size limits
//! (`ShowOptionSizeLimits` 0x612d58), the government bond trades of the Government Bonds desk
//! (`TradeGovtBonds` 0x5ba0ee) and [runtime resource wording] f a commodity when long futures expire
//! (`TakePhysicalDelivery` 0x664f9f).

// Keep the original's statement shapes.
#![allow(clippy::assign_op_pattern, clippy::collapsible_if, clippy::collapsible_else_if)]
use crate::pb::real::{Ext, cast};


use crate::logic::control::find_option_positions;
use crate::logic::earnings::{balance_cash_t_bills, calc_accrued_bond_interest};
use crate::logic::economy::world_events::add_remove_phys_commod_rec;
use crate::logic::finance::{calc_entity_financials, find_phys_commod_rec};
use crate::logic::options::{add_or_remove_option_rec, calc_option_price, calc_spread_factor, get_max_options_pct};
use crate::logic::sim_entity::corp_ops::check_govt_bond_large_trade;
use crate::logic::sim_entity::quarter_start::pack_holding_arrays;
use crate::logic::trading::borrow::borrow_to_cover_purchase;
use crate::logic::trading::prelude::*;
use crate::pb::ext80::Ext80;
use crate::pb::numeric::format_ext;
use crate::pb::numeric::{str_num_ext, val_ext};
use crate::pb::string::mcase;
use crate::pb::numeric::Cux;

/// `VAL(PARSE$(rec, ANY "|", n))` as the full 80-bit value.
fn fld_ext(rec: &PbStr, n: i32) -> Ext80 {
    val_ext(parse_any(rec.as_bytes(), b"|", n).as_bytes())
}

/// `RTRIM$(LTRIM$(FORMAT$(x, mask)))` of an EXT intermediate.
fn fmt_trim_ext(x: Ext80, mask: &str) -> PbStr {
    trim(&format_ext(x, mask.as_bytes()))
}

/// `" " + MillionPrefixLc + "illion" + CurrencySuffix`.
fn million(g: &Globals) -> PbStr {
    s(" ") + &g.million_prefix_lc + "illion" + &g.currency_suffix
}

/// Locals of CloseAllOptionPositions shared with its GOSUB CloseAllOptions_CalcNetGain.
#[derive(Default)]
struct CloseFrame {
    holder_id: i32,
    stock: i32,
    rec_idx: i32,
    opt_type: i32,
    signed_pct: i64,
    qty: i64,
    /// EXT local; only set by CalcNetGain (raw) and for written options (ABS) — see bug 366.
    strike: Ext80,
    cost_basis: Cux,
    opt_price: Ext,
    unit_price: Cux,
    net_gain: i64,
}

/// `optPrice` of a written option: the spread factor applied, at least 0.01 (as a SINGLE compare).
fn short_price(g: &Globals, f: &mut CloseFrame) {
    let sp = calc_spread_factor(f.opt_type, g.market_cap[f.stock], Ext::from(f.strike.into_ext()));
    f.opt_price = sp * f.opt_price;
    if (cast::<f32, _>(f.opt_price)) < 0.01f32 {
        f.opt_price = cast::<Ext, _>(0.01f32);
    }
}

/// Port of CloseAllOptionPositions (0x611ab6) with its GOSUBs CloseAllOptions_CalcNetGain (0x612945)
/// and CloseAllOptions_Exit (0x612d32, returns the result).
///
/// Closes every option position of the acting entity (`ActiveEntity`) on the stock `corp_id` at the
/// current market price (`CalcOptionPrice`; written options are bought back at the spread,
/// `CalcSpreadFactor`, at least 0.01). Without positions (unvested GRANTs don't count) an error popup.
/// Otherwise the estimated net gain (CalcNetGain, over all records incl. grants, in whole millions) is
/// shown with a YES/NO question; on YES each record is closed: cash credited (long) or debited
/// (written), the gain booked to `PlayerCapGains` (the current player) or `ExtraordinaryPL`, the record
/// deleted (`AddOrRemoveOptionRec`) and a popup shown per contract; unvested grants are skipped with
/// one popup. More than one record closed with a non-zero total: a summary MSGBOX. Finally
/// `PackHoldingArrays`. Returns 1 when the closing ran (YES), else 0.
pub fn close_all_option_positions(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, corp_id: i32) -> i32 {
    let __content = crate::content::current();
    // ON ERROR GOTO 0x612ccd -> ShowRuntimeError (error path not ported)
    let mut f = CloseFrame::default();
    let mut rec_stock = corp_id;
    f.stock = corp_id;
    let stock_name = g.ent_name[rec_stock].clone();
    f.holder_id = g.active_entity;
    let mut holder_name = g.active_entity_name.clone();
    if g.cur_player == g.active_entity {
        holder_name = s("You");
    }
    let mut msg_text;
    if find_option_positions(g, ui, f.holder_id, rec_stock, 0, 1) == 0 {
        msg_text = s("ERROR: ") + &holder_name + __content.string(11574) + &stock_name;
        if 10 < g.active_entity {
            msg_text = replace(msg_text.as_bytes(), b"don't", b"doesn't");
        }
        show_quick_message(g, ui, &mut msg_text);
        return 0;
    }
    close_all_options_calc_net_gain(g, ui, rng, &mut f);
    msg_text = holder_name.clone()
        + __content.string(11575)
        + &trim(g.currency_symbol.as_bytes())
        + &fmt_trim(cast::<Ext, _>(f.net_gain), "* ###,.00")
        + &million(g)
        + __content.string(11576)
        + &stock_name
        + __content.string(11577)
        + "\r"
        + "\r";
    if f.net_gain < 0 {
        msg_text = replace(msg_text.as_bytes(), b"gain", b"loss");
    }
    msg_text = msg_text + __content.string(11578) + &stock_name + "?";
    if 10 < g.active_entity {
        let with = g.active_entity_name.clone() + "'s";
        msg_text = replace(msg_text.as_bytes(), b"your", with.as_bytes());
    }
    let title = s("Close ") + &g.ent_symbol[f.stock] + __content.string(11579);
    if ui.msgbox(&msg_text, 8483, &title) != 6 {
        return 0;
    }
    let mut grant_warned = 0;
    let mut total_gain = Ext80::ZERO;
    let mut closed_count = 0;
    f.rec_idx = 1;
    loop {
        'rec: {
            if g.option_recs[f.rec_idx].is_empty() {
                break 'rec;
            }
            let r0 = g.option_recs[f.rec_idx].clone();
            if fld_ext(&r0, 3).into_ext() != cast::<Ext, _>(f.holder_id) {
                break 'rec;
            }
            if 0 < f.stock && fld_ext(&r0, 1).into_ext() != cast::<Ext, _>(f.stock) {
                break 'rec;
            }
            let rec = r0;
            f.opt_type = fld_ext(&rec, 2).to_i32();
            if f.opt_type == 1 && trim(parse_any(rec.as_bytes(), b"|", 8).as_bytes()) == "GRANT" {
                if grant_warned == 0 {
                    let mut m = s(__content.string(11580));
                    show_quick_message(g, ui, &mut m);
                }
                grant_warned = 1;
                break 'rec;
            }
            f.signed_pct = fld_ext(&rec, 4).to_i64();
            rec_stock = fld_ext(&rec, 1).to_i32();
            let _ = rec_stock;
            f.opt_price = calc_option_price(g, ui, rng, 0, Ext::from_f64(0.0), 0, 0, 0, f.rec_idx);
            if f.signed_pct < 0 {
                f.strike = fld_ext(&rec, 5).abs();
                short_price(g, &mut f);
            }
            f.unit_price.set(f.opt_price);
            f.cost_basis.set_ext(fld_ext(&rec, 8));
            f.qty = Ext80::from_i64(f.signed_pct).abs().to_i64();
            let qty = Ext80::from_i64(f.qty);
            let h = f.holder_id;
            let gain;
            if f.signed_pct <= 0 {
                gain = f.cost_basis.get_ext().fsub(f.unit_price.get_ext().fmul(qty));
                g.cash[h] = Ext::from(Ext80::from_ext(g.cash[h]).fsub(f.unit_price.get_ext().fmul(qty)).into_ext());
            } else {
                gain = f.unit_price.get_ext().fmul(qty).fsub(f.cost_basis.get_ext());
                g.cash[h] = Ext::from(Ext80::from_ext(g.cash[h]).fadd(f.unit_price.get_ext().fmul(qty)).into_ext());
            }
            if g.cur_player == h {
                g.player_cap_gains[h] = Ext::from(Ext80::from_ext(g.player_cap_gains[h]).fadd(gain).into_ext());
            } else {
                g.extraordinary_pl[h] = Ext80::from_i64(g.extraordinary_pl[h]).fadd(gain).to_i64();
            }
            let exp_year = fld_ext(&rec, 7).to_i32();
            let exp_month = fld_ext(&rec, 6).to_i32();
            add_or_remove_option_rec(g, ui, &PbStr::new(), f.rec_idx);
            let mut type_text = s("call");
            if f.opt_type == 2 {
                type_text = s("put");
            }
            let per_share = f.strike.fdiv(Ext80::from_ext(g.shares_outstanding[f.stock]));
            msg_text = holder_name.clone()
                + __content.string(11581)
                + &trim(g.currency_symbol.as_bytes())
                + &fmt_trim_ext(gain, "* ###,.00")
                + &million(g)
                + " on "
                + &g.ent_symbol[f.stock]
                + &str_s(cast::<Ext, _>(exp_month))
                + "/"
                + &str_trim(cast::<Ext, _>(exp_year))
                + __content.string(11582)
                + &trim(g.currency_symbol.as_bytes())
                + &fmt_trim_ext(per_share, " * ###")
                + " "
                + &type_text;
            if gain.is_neg() && !gain.is_zero() {
                msg_text = replace(msg_text.as_bytes(), __content.bytes(11583), __content.bytes(11584));
            }
            if f.signed_pct < 0 {
                msg_text = replace(msg_text.as_bytes(), b"(long)", b"(short)");
            }
            if 1 < f.qty {
                msg_text = msg_text + "s";
            }
            show_quick_message(g, ui, &mut msg_text);
            total_gain = total_gain.fadd(gain);
            closed_count = closed_count + 1;
        }
        f.rec_idx = f.rec_idx + 1;
        if 1000 < f.rec_idx {
            break;
        }
    }
    if !total_gain.is_zero() && 1 < closed_count {
        msg_text = s(__content.string(11585))
            + &stock_name
            + " was "
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim_ext(total_gain, "* ###,.00")
            + &million(g)
            + ".";
        if 10 < g.active_entity {
            let with = g.active_entity_name.clone() + "'s";
            msg_text = replace(msg_text.as_bytes(), b"your", with.as_bytes());
        }
        if total_gain.is_neg() {
            msg_text = replace(msg_text.as_bytes(), b"gain", b"loss");
        }
        let title = s(__content.string(11586)) + &g.ent_symbol[f.stock] + " Closed";
        ui.msgbox(&msg_text, 8256, &title);
    }
    pack_holding_arrays(g, ui);
    1
}

/// Port of CloseAllOptions_CalcNetGain (0x612945), GOSUB of CloseAllOptionPositions: the net gain (QUAD,
/// whole millions after each record) of closing every option record of the holder on the stock now.
/// Leaves `strike` (raw field 5) and the other record locals of the last matching record in the frame.
fn close_all_options_calc_net_gain(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut CloseFrame) {
    f.net_gain = 0;
    f.rec_idx = 1;
    while f.rec_idx <= 1000 {
        let rec = g.option_recs[f.rec_idx].clone();
        if !rec.is_empty()
            && fld_ext(&rec, 1).into_ext() == cast::<Ext, _>(f.stock)
            && fld_ext(&rec, 3).into_ext() == cast::<Ext, _>(f.holder_id)
        {
            f.opt_type = fld_ext(&rec, 2).to_i32();
            f.signed_pct = fld_ext(&rec, 4).to_i64();
            f.qty = Ext80::from_i64(f.signed_pct).abs().to_i64();
            f.strike = fld_ext(&rec, 5);
            f.cost_basis.set_ext(fld_ext(&rec, 8));
            f.opt_price = calc_option_price(g, ui, rng, 0, Ext::from_f64(0.0), 0, 0, 0, f.rec_idx);
            if f.signed_pct < 0 {
                short_price(g, f);
            }
            f.unit_price.set(f.opt_price);
            let pos_value = f.unit_price.get_ext().fmul(Ext80::from_i64(f.qty)).to_i64();
            let net = Ext80::from_i64(f.net_gain);
            if f.signed_pct <= 0 {
                f.net_gain = net.fadd(f.cost_basis.get_ext().fsub(Ext80::from_i64(pos_value))).to_i64();
            } else {
                f.net_gain = net.fadd(Ext80::from_i64(pos_value).fsub(f.cost_basis.get_ext())).to_i64();
            }
        }
        f.rec_idx = f.rec_idx + 1;
    }
}

/// Port of ShowOptionSizeLimits (0x612d58).
/// MSGBOX with [runtime resource wording]  on new long/short puts and calls on `stock_id` (percent of
/// the shares, `GetMaxOptionsPct` with the 10% cap lifted); "you have" becomes "<acting entity> has" for
/// a company.
pub fn show_option_size_limits(g: &mut Globals, ui: &mut dyn Ui, stock_id: i32) {
    let __content = crate::content::current();
    let stock = stock_id;
    let max_long_put = get_max_options_pct(g, stock, 2, &s("L"), 1);
    let max_short_put = get_max_options_pct(g, stock, 2, &s("S"), 1);
    let max_long_call = get_max_options_pct(g, stock, 1, &s("L"), 1);
    let max_short_call = get_max_options_pct(g, stock, 1, &s("S"), 1);
    let mut msg_text = s(__content.string(11587))
        + __content.string(11588)
        + &g.ent_name[stock]
        + __content.string(11589)
        + "\r"
        + "\r"
        + __content.string(11590)
        + &str_s(cast::<Ext, _>(max_long_put))
        + "%"
        + "\r"
        + __content.string(11591)
        + &str_s(cast::<Ext, _>(max_short_put))
        + "%"
        + "\r"
        + __content.string(11592)
        + &str_s(cast::<Ext, _>(max_long_call))
        + "%"
        + "\r"
        + __content.string(11593)
        + &str_s(cast::<Ext, _>(max_short_call))
        + "%"
        + "\r"
        + "\r"
        + "\r"
        + __content.string(11594);
    if 10 < g.active_entity {
        let with = g.active_entity_name.clone() + " has";
        msg_text = replace(msg_text.as_bytes(), __content.bytes(11595), with.as_bytes());
    }
    ui.msgbox(&msg_text, 8256, &s(__content.string(11596)));
}

/// Port of TradeGovtBonds (0x5ba0ee) with its GOSUBs TradeGovtBonds_MsgOnlyOwns (0x5bbc19) and
/// TradeGovtBonds_Exit (0x5bbca1, the common exit).
///
/// The acting entity (`ActiveEntity`) trades government bonds, `action` (the Government Bonds desk's
/// choice, GovtBondDlg_BtnGo): 1 = buy long bonds, 2 = sell long bonds, 3 = buy short bonds, 4 = sell
/// short bonds; anything else does nothing. `TmpExt1` is cleared first. Selling needs a holding. Buying
/// needs `LiquidFunds` (CalcEntityFinancials) for one unit at the price + 0.2% commission + accrued
/// interest. The face amount (millions) is asked by INPUTBOX (default: the affordable maximum or the
/// holding, capped at the no-impact size of `CheckGovtBondLargeTrade` 5/6); empty/0 cancels, a sale
/// above the holding is refused with a popup and asked again. A purchase over half the total issue
/// (`CheckGovtBondLargeTrade` 7) is refused. The market impact (`CheckGovtBondLargeTrade` 1..4, may
/// ask; -99 cancels) moves the price. A purchase beyond cash + T-bills is financed through
/// `BorrowToCoverPurchase` (NO cancels) or refused when over `LiquidFunds`. Purchases debit cash and add
/// face and cost; sales credit 99.8% of the value, reduce face and cost pro rata and book the gain
/// (players: `PlayerCapGains`, banks/insurers: `AccruedPL`, others: `ExtraordinaryPL`). The accrued
/// interest is paid/received through `Cash` and `AccruedPL`. Confirmation MSGBOX; `TurnActionCount`
/// + 1; a trade moving the price by more than 5% shifts `InterestRateTarget` (clamped 1.5..27).
#[allow(unused_assignments)]
pub fn trade_govt_bonds(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, action: i32) {
    let __content = crate::content::current();
    g.tmp_ext1 = Ext::from_f64(0.0);
    if action < 1 || 4 < action {
        return;
    }
    let mut verb = s("buy");
    if action == 2 || action == 4 {
        verb = s("sell");
    }
    let term_label;
    let bond_year;
    let mut coupon_text;
    let price_text;
    if action < 3 {
        term_label = s("Long");
        bond_year = mid(g.long_bond_desc.as_bytes(), 1, 4);
        coupon_text = right(g.long_bond_desc.as_bytes(), 5);
        if val(coupon_text.as_bytes()) < Ext::from_f64(10.0) {
            coupon_text = right(coupon_text.as_bytes(), 4);
        }
        price_text = fmt_trim(cast::<Ext, _>(g.long_bond_price), "###.00");
    } else {
        term_label = s("Short");
        bond_year = mid(g.short_bond_desc.as_bytes(), 1, 4);
        coupon_text = right(g.short_bond_desc.as_bytes(), 5);
        if val(coupon_text.as_bytes()) < Ext::from_f64(10.0) {
            coupon_text = right(coupon_text.as_bytes(), 4);
        }
        price_text = fmt_trim(cast::<Ext, _>(g.short_bond_price), "###.00");
    }
    let trader_id = g.active_entity;
    let mut msg_text;
    if verb == "sell" {
        msg_text = s(__content.string(11597))
            + &g.active_entity_name
            + __content.string(11598)
            + __content.string(11599)
            + &term_label
            + " Bonds!";
        if action == 2 && 0 < g.long_gov_bond_face[trader_id] {
            msg_text = PbStr::new();
        }
        if action == 4 && 0 < g.short_gov_bond_face[trader_id] {
            msg_text = PbStr::new();
        }
        if !msg_text.is_empty() {
            ui.msgbox(&msg_text, 8208, &s(__content.string(11600)));
            return;
        }
    }
    let mut price_frac: Ext;
    let unit_accrued;
    if action < 3 {
        price_frac = sf(0.01f32) * cast::<Ext, _>(g.long_bond_price);
        unit_accrued = calc_accrued_bond_interest(g, 1, 1);
    } else {
        price_frac = cast::<Ext, _>(g.short_bond_price) * sf(0.01f32);
        unit_accrued = calc_accrued_bond_interest(g, 1, 2);
    }
    calc_entity_financials(g, ui, rng, trader_id);
    let unit_cost = Ext80::from_ext(price_frac)
        .fmul(Ext80::from_f32(1.002f32))
        .fadd(Ext80::from_ext(unit_accrued))
        .into_ext();
    if g.liquid_funds < unit_cost && (action == 1 || action == 3) {
        let m = g.active_entity_name.clone()
            + __content.string(11601)
            + __content.string(11602)
            + &trim(g.currency_symbol.as_bytes())
            + "1 "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix
            + ").";
        ui.msgbox(&m, 8208, &s(__content.string(1928)));
        return;
    }
    let mut max_face: i64 = 0;
    if Ext::from_f64(0.0) < unit_cost {
        max_face = Ext80::from_ext(g.liquid_funds)
            .fdiv(Ext80::from_ext(unit_cost))
            .fsub(Ext80::from_f32(0.51f32))
            .to_i64();
    }
    let mut bond_kind = 5;
    if 2 < action {
        bond_kind = 6;
    }
    let trade_face: i64;
    if verb == "sell" {
        let mut tf = g.long_gov_bond_face[trader_id];
        if action == 4 {
            tf = g.short_gov_bond_face[trader_id];
        }
        trade_face = tf;
        max_face = tf;
    } else {
        trade_face = max_face;
    }
    let max_no_impact = check_govt_bond_large_trade(g, ui, bond_kind, trade_face);
    let mut impact_note = PbStr::new();
    if max_no_impact != Ext::from_f64(0.0) {
        max_face = to_quad(max_no_impact - Ext::from_f64(1.0));
        impact_note = s(__content.string(11603));
    }
    let mut subject_text = s(__content.string(11604));
    if 10 < g.active_entity {
        subject_text = g.ent_symbol[g.active_entity].clone() + " wishes";
    }
    let mut suffix_text = PbStr::new();
    if !g.currency_suffix.is_empty() {
        suffix_text = s(" of") + &g.currency_suffix;
    }
    let mut default_text = fmt_trim(cast::<Ext, _>(max_face), "* ###,");
    let mut face_amt: i64;
    loop {
        msg_text = s(__content.string(11605))
            + &g.million_prefix_lc
            + "illions"
            + &suffix_text
            + __content.string(11606)
            + __content.string(11607)
            + &term_label
            + __content.string(11608)
            + &bond_year
            + " "
            + &subject_text
            + " to "
            + &verb
            + ":"
            + "\r\n"
            + "\r\n"
            + __content.string(11609)
            + &price_text
            + ")"
            + "\r\n"
            + __content.string(11610)
            + &verb
            + &impact_note
            + " ("
            + &g.million_prefix_lc
            + __content.string(11611);
        let title = ucase(verb.as_bytes(), CaseMode::Ansi)
            + __content.string(11612)
            + &ucase(term_label.as_bytes(), CaseMode::Ansi)
            + "' BONDS";
        let answer = ui.inputbox(&msg_text, &title, &default_text);
        face_amt = val_ext(remove_any(answer.as_bytes(), b"$ ,%").as_bytes()).to_i64();
        if face_amt <= 0 {
            return;
        }
        if action == 2 && g.long_gov_bond_face[trader_id] < face_amt {
            trade_govt_bonds_msg_only_owns(g, ui, trade_face);
            continue;
        }
        if action == 4 && g.short_gov_bond_face[trader_id] < face_amt {
            trade_govt_bonds_msg_only_owns(g, ui, trade_face);
            continue;
        }
        break;
    }
    let trade_face = face_amt;
    let mut kind = 1;
    if 2 < action {
        kind = 2;
    }
    let accrued = calc_accrued_bond_interest(g, face_amt, kind);
    if action == 1 || action == 3 {
        let total_issue = Ext80::from_ext(check_govt_bond_large_trade(g, ui, 7, trade_face))
            .fmul(Ext80::from_f32(0.5f32))
            .to_i64();
        let mut held_face = g.long_gov_bond_face[trader_id];
        if action == 3 {
            held_face = g.short_gov_bond_face[trader_id];
        }
        if Ext80::from_i64(total_issue).cmp_val(Ext80::from_i64(face_amt).fadd(Ext80::from_i64(held_face))).is_lt() {
            msg_text = s(__content.string(11613))
                + &trim(g.currency_symbol.as_bytes())
                + &fmt_trim(cast::<Ext, _>(total_issue), "* ###,")
                + &million(g)
                + ". ";
            if held_face <= 0 {
                msg_text = msg_text + "Y";
            } else {
                msg_text = msg_text + __content.string(11614);
            }
            let sum = Ext80::from_i64(face_amt).fadd(Ext80::from_i64(held_face));
            msg_text = msg_text
                + __content.string(11615)
                + &trim(g.currency_symbol.as_bytes())
                + &fmt_trim_ext(sum, "* ###,")
                + &million(g)
                + ", "
                + __content.string(11616);
            show_quick_message(g, ui, &mut msg_text);
            return;
        }
    }
    let impact = cast::<f32, _>(check_govt_bond_large_trade(g, ui, action, trade_face));
    if impact == -Ext::from_f32(99.0f32) {
        return;
    }
    if impact != Ext::from_f32(0.0f32) {
        price_frac = Ext::from(Ext80::from_f32(0.01f32)
            .fmul(Ext80::from_f32(impact))
            .fmul(Ext80::from_ext(price_frac))
            .fadd(Ext80::from_ext(price_frac))
            .into_ext());
    }
    let face_x = Ext80::from_i64(face_amt);
    let trade_value: Ext80;
    let mut realized_gain = Ext80::ZERO;
    let mut gain_label = PbStr::new();
    if action == 1 || action == 3 {
        trade_value = Ext80::from_f32(1.002f32).fmul(Ext80::from_ext(price_frac)).fmul(face_x);
        let have = Ext80::from_ext(g.cash[trader_id]).fadd(Ext80::from_ext(g.t_bills[trader_id]));
        if have.cmp_val(trade_value).is_lt() {
            if Ext80::from_ext(g.liquid_funds).cmp_val(trade_value.fadd(Ext80::from_ext(accrued))).is_lt() {
                let mut m = g.ent_symbol[trader_id].clone()
                    + __content.string(11617);
                show_quick_message(g, ui, &mut m);
                return;
            }
            let have = Ext80::from_ext(g.cash[trader_id]).fadd(Ext80::from_ext(g.t_bills[trader_id]));
            let mut shortfall = trade_value.fadd(Ext80::from_ext(accrued)).fsub(have).into_ext();
            let commission = trade_value.fsub(face_x.fmul(Ext80::from_ext(price_frac))).into_ext();
            if borrow_to_cover_purchase(g, ui, &mut shortfall, commission, 0) == 7 {
                return;
            }
        }
        g.cash[trader_id] = Ext::from(Ext80::from_ext(g.cash[trader_id]).fsub(trade_value).into_ext());
        if action == 1 {
            g.long_gov_bond_face[trader_id] = Ext80::from_i64(g.long_gov_bond_face[trader_id]).fadd(face_x).to_i64();
            g.lt_gov_bond_cost[trader_id] = Ext::from(Ext80::from_ext(g.lt_gov_bond_cost[trader_id]).fadd(trade_value).into_ext());
        } else {
            g.short_gov_bond_face[trader_id] = Ext80::from_i64(g.short_gov_bond_face[trader_id]).fadd(face_x).to_i64();
            g.st_gov_bond_cost[trader_id] = Ext::from(Ext80::from_ext(g.st_gov_bond_cost[trader_id]).fadd(trade_value).into_ext());
        }
    } else {
        let mut cost_basis_sold = Ext80::ZERO;
        if action == 2 {
            if g.long_gov_bond_face[trader_id] < face_amt {
                face_amt = g.long_gov_bond_face[trader_id];
            }
            cost_basis_sold = Ext80::from_i64(face_amt)
                .fdiv(Ext80::from_i64(g.long_gov_bond_face[trader_id]))
                .fmul(Ext80::from_ext(g.lt_gov_bond_cost[trader_id]));
            g.lt_gov_bond_cost[trader_id] = Ext::from(Ext80::from_ext(g.lt_gov_bond_cost[trader_id]).fsub(cost_basis_sold).into_ext());
            g.long_gov_bond_face[trader_id] =
                Ext80::from_i64(g.long_gov_bond_face[trader_id]).fsub(Ext80::from_i64(face_amt)).to_i64();
            if g.long_gov_bond_face[trader_id] == 0 {
                g.lt_gov_bond_cost[trader_id] = Ext::from_f64(0.0);
            }
        } else if action == 4 {
            if g.short_gov_bond_face[trader_id] < face_amt {
                face_amt = g.short_gov_bond_face[trader_id];
            }
            cost_basis_sold = Ext80::from_i64(face_amt)
                .fdiv(Ext80::from_i64(g.short_gov_bond_face[trader_id]))
                .fmul(Ext80::from_ext(g.st_gov_bond_cost[trader_id]));
            g.st_gov_bond_cost[trader_id] = Ext::from(Ext80::from_ext(g.st_gov_bond_cost[trader_id]).fsub(cost_basis_sold).into_ext());
            g.short_gov_bond_face[trader_id] =
                Ext80::from_i64(g.short_gov_bond_face[trader_id]).fsub(Ext80::from_i64(face_amt)).to_i64();
            if g.short_gov_bond_face[trader_id] == 0 {
                g.st_gov_bond_cost[trader_id] = Ext::from_f64(0.0);
            }
        }
        trade_value = Ext80::from_f32(0.998f32).fmul(Ext80::from_ext(price_frac)).fmul(Ext80::from_i64(face_amt));
        g.cash[trader_id] = Ext::from(Ext80::from_ext(g.cash[trader_id]).fadd(trade_value).into_ext());
        realized_gain = trade_value.fsub(cost_basis_sold);
        gain_label = s(" gain");
        if realized_gain.is_neg() && !realized_gain.is_zero() {
            gain_label = s(" loss");
        }
        if trader_id < 11 {
            g.player_cap_gains[trader_id] = Ext::from(Ext80::from_ext(g.player_cap_gains[trader_id]).fadd(realized_gain).into_ext());
            gain_label = s(__content.string(11618)) + &gain_label;
        } else if trader_id < 86 {
            g.accrued_pl[trader_id] = Ext::from(Ext80::from_ext(g.accrued_pl[trader_id]).fadd(realized_gain).into_ext());
            if realized_gain.is_neg() || realized_gain.is_zero() {
                gain_label = s(__content.string(11619)) + &gain_label;
            } else {
                gain_label = s(" profit");
            }
        } else {
            g.extraordinary_pl[trader_id] = Ext80::from_i64(g.extraordinary_pl[trader_id]).fadd(realized_gain).to_i64();
            gain_label = s(__content.string(11620)) + &gain_label;
        }
    }
    if verb == "buy" {
        subject_text = s(__content.string(11621));
        suffix_text = s(__content.string(11622));
        g.cash[trader_id] = g.cash[trader_id] - accrued;
        g.accrued_pl[trader_id] = g.accrued_pl[trader_id] - accrued;
    } else if verb == "sell" {
        subject_text = s("sold ");
        suffix_text = s(__content.string(11623));
        g.cash[trader_id] = g.cash[trader_id] + accrued;
        g.accrued_pl[trader_id] = g.accrued_pl[trader_id] + accrued;
    }
    default_text = s(__content.string(4950));
    if 10 < trader_id {
        default_text = g.ent_symbol[trader_id].clone() + " has ";
    }
    msg_text = s(__content.string(11624))
        + "\r"
        + "\r"
        + &default_text
        + &subject_text
        + &trim(g.currency_symbol.as_bytes())
        + &fmt_trim(cast::<Ext, _>(face_amt), "* ###,")
        + " "
        + &g.million_prefix_lc
        + __content.string(11625)
        + __content.string(11626)
        + &coupon_text
        + "% "
        + &term_label
        + __content.string(11627)
        + &bond_year
        + ", at a "
        + &suffix_text
        + " of "
        + &trim(g.currency_symbol.as_bytes())
        + &fmt_trim_ext(trade_value, "* ###,.00")
        + &million(g)
        + ", "
        + __content.string(10851)
        + &trim(g.currency_symbol.as_bytes())
        + &fmt_trim(accrued, "* ###,.00")
        + " "
        + &g.million_prefix_lc
        + "illion."
        + "\r"
        + "\r";
    if Ext80::from_f32(0.1f32).cmp_val(realized_gain.abs()).is_lt() {
        msg_text = msg_text
            + "A"
            + &gain_label
            + " of "
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim_ext(realized_gain.abs(), "* ###,.00")
            + &million(g)
            + " "
            + __content.string(11628);
        if realized_gain.is_neg() {
            msg_text = replace(msg_text.as_bytes(), __content.bytes(11629), __content.bytes(9088));
        }
    }
    ui.msgbox(&msg_text, 8192, &s("CONFIRMATION"));
    g.turn_action_count = g.turn_action_count + 1;
    if Ext::from_f32(5.0f32) < impact.abs() {
        // fVar2 * 0.002 * -1 -> priceFrac (DOUBLE), then InterestRateTarget (SINGLE)
        price_frac = crate::pb::numeric::double(Ext80::from_f32(impact).fmul(Ext80::from_f32(0.002f32)).chs().into_ext());
        let irt = cast::<Ext, _>(g.interest_rate_target);
        g.interest_rate_target = cast::<f32, _>(Ext80::from_ext(price_frac)
            .fmul(Ext80::from_ext(irt))
            .fadd(Ext80::from_ext(irt))
            .to_f32());
        if Ext::from_f32(27.0f32) < g.interest_rate_target {
            g.interest_rate_target = cast::<f32, _>(Ext::from_f32(27.0f32));
        }
        if g.interest_rate_target < Ext::from_f32(1.5f32) {
            g.interest_rate_target = cast::<f32, _>(Ext::from_f32(1.5f32));
        }
    }
}

/// Port of TradeGovtBonds_MsgOnlyOwns (0x5bbc19), GOSUB of TradeGovtBonds: popup "<acting entity> only
/// owns N m [runtime resource wording] …" with N = the frame's `tradeFace` (the holding).
fn trade_govt_bonds_msg_only_owns(g: &mut Globals, ui: &mut dyn Ui, trade_face: i64) {
    let __content = crate::content::current();
    let mut quick_msg = g.active_entity_name.clone()
        + __content.string(11630)
        + &fmt_trim(cast::<Ext, _>(trade_face), "##################,")
        + " "
        + &g.million_prefix_lc
        + __content.string(11631);
    show_quick_message(g, ui, &mut quick_msg);
}

/// Port of TakePhysicalDelivery (0x664f9f).
/// A long futures record `fut_rec` (`commodity|holder|contracts|month|year|cost`) expires into the
/// physical commodity: the holder's `PhysCommodRec` gets the units (new record, or added to an existing
/// one) at the contract cost + 1%, the contract cost is paid in cash (`BalanceCashTBills`; a deficit
/// beyond the T-bills is borrowed from the lending bank, up to the unused credit line). Returns 1, or 2
/// when the cash is still negative; 0 (nothing done) for an inactive holder, zero contracts or a
/// record that cannot be found. A news item ("… [runtime resource wording] …" / "… lacks funds …") is
/// posted for players 1..5, the selected entities and companies of the current player (dropped when
/// longer than 78 characters).
pub fn take_physical_delivery(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, fut_rec: &PbStr) -> i32 {
    let __content = crate::content::current();
    let mut result = 0;
    let commod_name = parse_any(fut_rec.as_bytes(), b"|", 1);
    let holder_id = fld_ext(fut_rec, 2).to_i32();
    if holder_id < 1 || g.entity_status[holder_id] != 1 {
        return result;
    }
    let contracts = fld_ext(fut_rec, 3).to_i32();
    if contracts == 0 {
        return result;
    }
    let delivery_cost = fld_ext(fut_rec, 6);
    if find_phys_commod_rec(g, holder_id, &commod_name) == 0 {
        let total_cost = delivery_cost.fmul(Ext80::from_f32(1.01f32));
        let rec_text = trim(commod_name.as_bytes())
            + "|"
            + &str_trim(cast::<Ext, _>(holder_id))
            + "|"
            + &str_trim(cast::<Ext, _>(contracts))
            + "|"
            + &trim(str_num_ext(total_cost, 15).as_bytes());
        add_remove_phys_commod_rec(g, rec_text.as_bytes(), 0);
    } else {
        let rec_idx = find_phys_commod_rec(g, holder_id, &commod_name);
        if rec_idx == 0 {
            return result;
        }
        let rec_text = g.phys_commod_rec[rec_idx].clone();
        let units_owned = fld_ext(&rec_text, 3).to_i32();
        let new_units = units_owned + contracts;
        let old_cost = fld_ext(&rec_text, 4);
        let total_cost = delivery_cost.fmul(Ext80::from_f32(1.01f32)).fadd(old_cost);
        let new_rec = trim(commod_name.as_bytes())
            + "|"
            + &str_trim(cast::<Ext, _>(holder_id))
            + "|"
            + &str_trim(cast::<Ext, _>(new_units))
            + "|"
            + &trim(str_num_ext(total_cost, 15).as_bytes());
        g.phys_commod_rec[rec_idx] = new_rec;
    }
    g.cash[holder_id] = Ext::from(Ext80::from_ext(g.cash[holder_id]).fsub(delivery_cost).into_ext());
    balance_cash_t_bills(g, holder_id);
    if g.cash[holder_id] + g.t_bills[holder_id] < Ext::from_f64(0.0) {
        calc_entity_financials(g, ui, rng, holder_id);
        let mut loan_amt = g.cash[holder_id].abs();
        if g.unused_credit_line < loan_amt {
            loan_amt = g.unused_credit_line;
        }
        let lender = g.lender_id;
        g.cash[lender] = g.cash[lender] - loan_amt;
        g.bank_debt[holder_id] = g.bank_debt[holder_id] + loan_amt;
        g.cash[holder_id] = g.cash[holder_id] + loan_amt;
    }
    if g.cash[holder_id] < Ext::from_f64(0.0) {
        result = 2;
    } else {
        result = 1;
    }
    if 5 < holder_id
        && g.sel_entity != holder_id
        && g.last_sel_entity != holder_id
        && g.cur_player != g.corp_controller[holder_id]
    {
        return result;
    }
    let crypto = commod_name == "BITCOIN" || commod_name == "ETHEREUM";
    if g.cash[holder_id] + g.t_bills[holder_id] < Ext::from_f64(0.0) {
        g.news_msg = g.ent_symbol[holder_id].clone()
            + __content.string(11632)
            + &lcase(commod_name.as_bytes(), CaseMode::Ansi);
    } else {
        let mut new_rec = commod_name.clone();
        if 10 < holder_id && (new_rec == "GOLD" || new_rec == "SILVER") {
            new_rec = new_rec + __content.string(11633);
        }
        new_rec = lcase(new_rec.as_bytes(), CaseMode::Ansi);
        if crypto {
            new_rec = mcase(new_rec.as_bytes(), CaseMode::Ansi);
        }
        g.news_msg = g.ent_symbol[holder_id].clone() + __content.string(11634) + &new_rec + __content.string(11635);
    }
    if crypto {
        g.news_msg = remove(g.news_msg.as_bytes(), __content.bytes(2640));
    }
    if 78 < len(g.news_msg.as_bytes()) {
        g.news_msg = PbStr::new();
    } else {
        post_news(g, ui, holder_id, 0);
    }
    result
}
