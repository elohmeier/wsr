//! The player's option sales and early exercise: `SellOptions` (0x5e95d8, writing new calls/puts or
//! selling long ones) and `ExerciseOptionsMenu` (0x600b42).

// Keep the original's statement shapes.
#![allow(
    clippy::assign_op_pattern,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::if_same_then_else,
    clippy::manual_clamp,
    clippy::manual_range_contains
)]
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use super::options_menu::{check_circular_option_holding, show_option_chain};
use super::select::{get_selected_player_etf, in_box_symbol};
use crate::logic::control::{find_option_positions, update_corp_control};
use crate::logic::earnings::calc_projected_earnings;
use crate::logic::etf_derivs::corp_options::force_close_excess_naked_options;
use crate::logic::etf_derivs::option_expiry::exercise_option;
use crate::logic::finance::calc_entity_financials;
use crate::logic::holdings::find_holding_slot;
use crate::logic::market::update_market_display;
use crate::logic::newgame_helpers::pick_etf_holding_stock;
use crate::logic::options::{
    add_or_remove_option_rec, calc_option_price, get_max_options_pct, get_net_stock_pct_after_options,
    get_plr_naked_liab, merge_matching_option_rec,
};
use crate::logic::report::number_to_words;
use crate::logic::reports_lists::show_option_positions;
use crate::logic::research::get_etf_description;
use crate::logic::restructure::merge::show_runtime_error;
use crate::logic::sim_entity::asset_sales::calc_group_stake_pct;
use crate::logic::sim_entity::etf::find_option_counterparty;
use crate::logic::valuation::calc_stock_price;
use crate::pb::ext80::Ext80;
use crate::pb::numeric::{str_num_ext, val_ext};
use crate::pb::string::mcase;

/// `VAL(PARSE$(rec, ANY "|", n))`.
fn fval(rec: &PbStr, n: i32) -> Ext {
    val(parse_any(rec.as_bytes(), b"|", n).as_bytes())
}

/// `RTRIM$(LTRIM$(PARSE$(rec, ANY "|", n)))`.
fn ftrim(rec: &PbStr, n: i32) -> PbStr {
    trim(parse_any(rec.as_bytes(), b"|", n).as_bytes())
}

/// `STR$(x)` of an EXT value held as an 80-bit number.
fn str_x(x: Ext80) -> PbStr {
    PbStr::from(str_num_ext(x, 15).as_str())
}

/// `MCASE$(x)`.
fn mc(x: &PbStr) -> PbStr {
    mcase(x.as_bytes(), CaseMode::Ansi)
}

/// Port of SellOptions (0x5e95d8).
///
/// The player writes (sells short) `opt_type` options (1 = calls, 2 = puts) or sells long ones, for the
/// acting entity or an ETF he advises ([`get_selected_player_etf`]; YES/NO/CANCEL, the bond and leveraged
/// funds 1581..1585 may not trade options). Blocked in the first quarter of a new game (before tick 503).
/// The stock is entered in the symbol dialog ([`in_box_symbol`]). With long options on it the player
/// may sell those off (closing sale: the record picked from `ShowOptionPositions` when there are
/// several; employee GRANT options are refused, VESTED ones are sold back to the employer); otherwise
/// (or NO) a new short position goes through the rules: own stock, banks/insurers (hedging only), ETF
/// sectors, credit rating 'BBB' (corporations) / collateral (players), share price 5..1000, dire
/// financial condition, merger insiders, circular holdings, the exchange's 100% position limit, the
/// 1,000 record limit, no public float, the insider/hedge limits (with their notices), the strike
/// (75%..125% of the price) and the series (`ShowOptionChain`), margin (naked liability within 2x / 25%
/// of the credit net worth), then the percent (max 10). A bid below 0.01 is refused, or for long
/// options bought back by the broker at 0.001 million ("mercy sale"). Proceeds (net of 1% commission)
/// go to the seller; new short records need a counterparty ([`find_option_counterparty`]); closing
/// sales book the gain (capital gains / extraordinary P&L) and shrink or remove the record. Ends with
/// the confirmation, `UpdateMarketDisplay`, a headline and a valuation (`ValuationMode = 1`).
/// `time_secs` is `VAL(RIGHT$(TIME$, 2))` for `UpdateMarketDisplay` / `CalcStockPrice`.
///
/// ERROR 151 (`SharesOutstanding = 0` of the stock) ends in the ON ERROR handler
/// (`ShowRuntimeError(151, "FrontSellOptions", "", seller)`).
// PB locals start zeroed; the port keeps them initialised like the original.
#[allow(unused_assignments)]
pub fn sell_options(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, opt_type_ref: i32, time_secs: i32) {
    let __content = crate::content::current();
    let mut msg = PbStr::new();
    let mut part;
    let mut part2;
    let mut answer = 0;
    let mut long_rec = 0;
    let mut is_closing = PbStr::new();
    let mut is_vested = 0;
    let mut pos_qty = 0; // posInfo(3): contracts of the long record sold
    let mut max_pct: i64 = 0;
    let mut strike: Ext = Ext::from_f64(0.0);
    let mut strike_x = Ext80::from_ext(Ext::from_f64(0.0));
    let mut exp_month = 0;
    let mut exp_year = 0;
    let mut orig_premium_x = Ext80::from_ext(Ext::from_f64(0.0));
    let mut bid_total_x;
    let mut mercy_sale = 0;
    let mut hedge_limit_pct: i16 = 0;
    let mut net_stock_pct: i16 = 0;
    let mut owned_pct: i16 = 0;
    let mut public_pct: i16 = 0;
    let mut limit_reason = PbStr::new();
    let mut net_pct_cap: i64 = 0;
    let mut afford_pct: i64 = 0;
    let mut ok_flag = 0;

    if g.cur_year == g.start_year && g.cur_quarter == 1 && g.sim_co_count < 503 {
        let mut m = s(__content.string(12330))
            + &str_s(cast::<Ext, _>(g.cur_year))
            + __content.string(11343);
        show_quick_message(g, ui, &mut m);
        return;
    }
    let fmt_str = "* ##############.00";
    let opt_type = opt_type_ref;
    let price_side = s("Bid");
    let mut opt_kind = s("call");
    if opt_type == 2 {
        opt_kind = s("put");
    }
    g.tmp_symbol = PbStr::new();
    let mut size_factor = sf(0.25f32);
    let cp = g.cur_player;
    calc_entity_financials(g, ui, rng, cp);
    if g.exchange_rate * Ext::from_f64(20000.0) < g.net_worth {
        size_factor = sf(0.15f32);
    }
    if g.exchange_rate * Ext::from_f64(50000.0) < g.net_worth {
        size_factor = sf(0.1f32);
    }
    let mut seller_id = g.active_entity;
    let mut seller_name = g.active_entity_name.clone();
    let mut etf_id = get_selected_player_etf(g, ui, rng);
    if 1585 < etf_id {
        msg = g.ent_name[etf_id].clone()
            + " ("
            + &g.ent_symbol[etf_id]
            + __content.string(3017)
            + __content.string(10156)
            + &g.ent_name[g.fund_adviser[etf_id]]
            + ". "
            + __content.string(12332)
            + "\r"
            + "\r"
            + __content.string(3020)
            + &g.ent_name[etf_id]
            + __content.string(12522)
            + &ucase(opt_kind.as_bytes(), CaseMode::Ansi)
            + __content.string(12334)
            + "\r"
            + "\r"
            + __content.string(3567)
            + &g.active_entity_name
            + " sell "
            + &opt_kind
            + "s.)";
        answer = ui.msgbox(&msg, 8483, &s(__content.string(12523)));
        if answer == 2 {
            return;
        }
        if answer == 7 {
            etf_id = 0;
        } else {
            seller_id = etf_id;
            seller_name = g.ent_name[etf_id].clone();
        }
    } else if 1580 < etf_id {
        part = s(__content.string(12335));
        if 1583 < etf_id {
            part = s(__content.string(12336));
        }
        part2 = g.active_entity_name.clone();
        if g.active_entity < 11 {
            part2 = s(__content.string(12337));
        }
        msg = s("While ")
            + &g.ent_name[etf_id]
            + __content.string(12338)
            + &part
            + __content.string(12339)
            + "\r"
            + "\r"
            + __content.string(12524)
            + &part2
            + ".";
        ui.msgbox(&msg, 8208, &s(__content.string(12341)));
        etf_id = 0;
    }
    calc_entity_financials(g, ui, rng, seller_id);
    if 10 < seller_id && g.industry_num[seller_id] < 3 {
        part = s("Banks");
        if g.industry_num[seller_id] == 2 {
            part = s(__content.string(2267));
        }
        if opt_type == 2 {
            if find_option_positions(g, ui, seller_id, 0, 2, 0) < 21 {
                msg = part.clone()
                    + __content.string(12525)
                    + "\r\n"
                    + "\r\n"
                    + __content.string(12345);
                show_quick_message(g, ui, &mut msg);
                return;
            }
        } else if opt_type == 1
            && find_option_positions(g, ui, seller_id, 0, 1, 0) < 11
            && g.tmp_subsid_stock_value == Ext::from_f64(0.0)
        {
            msg = part.clone()
                + __content.string(12526)
                + &seller_name
                + __content.string(12527);
            show_quick_message(g, ui, &mut msg);
            return;
        }
    }
    msg = s(__content.string(11815));
    part = s("you (") + &g.cur_player_name + ") wish";
    if 10 < seller_id {
        part = seller_name.clone() + " wishes";
    }
    msg = msg + __content.string(12348) + &part + __content.string(12528) + &opt_kind + __content.string(12365);
    if seller_id < 11 || 130 < seller_id || opt_type_ref == 1 {
        msg = msg + __content.string(12529);
    } else {
        msg = msg + __content.string(12530) + &opt_kind + " ";
    }
    msg = msg + __content.string(12531) + "\r\n" + "\r\n";
    g.input_dlg_label = PbStr::new();
    g.input_dlg_prompt = msg.clone();
    g.input_dlg_title = s(__content.string(12532)) + &mc(&opt_kind) + __content.string(12353);
    if 10 < seller_id {
        g.input_dlg_title = g.input_dlg_title.clone() + " for " + &g.ent_symbol[seller_id];
    }
    g.lookup_filter = PbStr::new();
    if 1585 < etf_id {
        g.action_code = s("ETF_TRADES");
    }
    in_box_symbol(g, ui, rng);
    g.action_code = PbStr::new();
    msg = PbStr::new();
    if g.tmp_symbol.is_empty() {
        return;
    }
    let corp_id = g.target_corp;
    g.target_corp = 0;
    if opt_type_ref == 1 {
        answer = find_option_positions(g, ui, seller_id, corp_id, 1, 1);
        if answer == 1 || answer == 2 {
            long_rec = 1;
        }
    } else {
        answer = find_option_positions(g, ui, seller_id, corp_id, 2, 0);
        if answer == 31 || answer == 32 {
            long_rec = 1;
        }
    }
    if 1585 < etf_id && (answer < 0 || answer == 2 || answer == 32) {
        long_rec = long_rec + 2;
    }
    if long_rec == 2 {
        msg = seller_name.clone()
            + __content.string(12533)
            + &opt_kind
            + __content.string(12534)
            + &g.ent_name[corp_id]
            + __content.string(12535)
            + &opt_kind
            + " "
            + __content.string(12356)
            + &g.ent_name[corp_id]
            + "."
            + "\r\n"
            + "\r\n"
            + __content.string(3246);
        show_quick_message(g, ui, &mut msg);
        return;
    }
    if seller_id == corp_id && long_rec == 0 {
        let mut m = seller_name.clone() + __content.string(12357);
        show_quick_message(g, ui, &mut m);
        return;
    }
    if long_rec == 0 && opt_type == 1 && 10 < seller_id && g.industry_num[seller_id] < 3 {
        let slot = cast::<i16, _>(find_holding_slot(g, seller_id, corp_id));
        if !(slot != 0 && cast::<i32, _>(g.hold_stock_id[(cast::<i32, _>(slot), seller_id)]) == corp_id) {
            msg = s("Banks");
            part = s(__content.string(12536));
            if g.industry_num[seller_id] == 2 {
                msg = s(__content.string(2267));
                part = s(__content.string(12537));
            }
            msg = msg
                + __content.string(12360)
                + &part
                + " money "
                + __content.string(12538)
                + "\r\n"
                + "\r\n"
                + __content.string(12345);
            show_quick_message(g, ui, &mut msg);
            return;
        }
    }
    // A closing sale of a long record: its bid is priced here and the series selection is skipped.
    let mut closing_bid: Option<Ext80> = None;
    if long_rec == 1 {
        if corp_id == seller_id {
            answer = 6;
            let mut m = seller_name.clone()
                + __content.string(12539);
            show_quick_message(g, ui, &mut m);
        } else {
            part = s("You own");
            if 10 < seller_id {
                part = g.ent_name[seller_id].clone() + " owns";
            }
            msg = part.clone()
                + __content.string(12540)
                + &opt_kind
                + __content.string(12365)
                + __content.string(9409)
                + &g.ent_name[corp_id]
                + "."
                + "\r"
                + "\r"
                + __content.string(12541);
            answer = ui.msgbox(&msg, 8483, &s(__content.string(12367)));
            if answer == 2 {
                return;
            }
            if answer == 7 && 1585 < etf_id {
                msg = s(__content.string(12368))
                    + &seller_name
                    + __content.string(12542)
                    + &opt_kind
                    + __content.string(12370)
                    + __content.string(12371)
                    + &opt_kind
                    + __content.string(12372)
                    + &g.ent_name[corp_id]
                    + __content.string(12373);
                show_quick_message(g, ui, &mut msg);
                return;
            }
        }
        if answer == 6 {
            is_closing = s("Y");
            long_rec = 0;
            let mut long_count = 0;
            for idx in 1..=1000 {
                let rec = g.option_recs[idx].clone();
                if fval(&rec, 1) == cast::<Ext, _>(corp_id)
                    && fval(&rec, 2) == cast::<Ext, _>(opt_type)
                    && fval(&rec, 3) == cast::<Ext, _>(seller_id)
                    && fval(&rec, 4) >= Ext::from_f64(0.0)
                {
                    long_count = long_count + 1;
                    long_rec = idx;
                }
            }
            if 1 < long_count {
                let mode = if opt_type == 2 { 2 } else { 1 };
                long_rec = show_option_positions(g, ui, rng, seller_id, mode, corp_id);
                if long_rec < 1 {
                    return;
                }
            } else if long_count < 1 {
                long_rec = 0;
                let mut m = s(__content.string(12543)) + &opt_kind + __content.string(12544);
                show_quick_message(g, ui, &mut m);
            }
            if 0 < long_rec {
                let opt_rec = g.option_recs[long_rec].clone();
                pos_qty = to_long(fval(&opt_rec, 4));
                max_pct = to_quad((cast::<Ext, _>(pos_qty)).abs());
                strike_x = val_ext(parse_any(opt_rec.as_bytes(), b"|", 5).as_bytes());
                strike = Ext::from(strike_x.into_ext());
                exp_month = to_long(fval(&opt_rec, 6));
                exp_year = to_long(fval(&opt_rec, 7));
                orig_premium_x = val_ext(parse_any(opt_rec.as_bytes(), b"|", 8).as_bytes());
                if ftrim(&opt_rec, 8) == "GRANT" {
                    part = s("current");
                    if seller_id < 11 && 0 < seller_id && corp_id != g.player_ceo_of[seller_id] {
                        part = s("former");
                    }
                    msg = s(__content.string(12545))
                        + __content.string(12546)
                        + &part
                        + __content.string(12547)
                        + __content.string(12548)
                        + &g.month_name[exp_month]
                        + ","
                        // BUG (original, #251): grants vest in expYear - 1.
                        + &str_s(Ext::from_f64(1.0) + cast::<Ext, _>(exp_year))
                        + __content.string(12549);
                    ui.msgbox(&msg, 8208, &s(__content.string(11731)));
                    return;
                }
                if ftrim(&opt_rec, 8) == "VESTED" {
                    is_vested = 1;
                }
                let price = calc_option_price(g, ui, rng, corp_id, strike, exp_year, exp_month, opt_type, 0);
                closing_bid = Some(Ext80::from_ext(price));
            }
        } else {
            long_rec = 0;
        }
    }
    if closing_bid.is_none() {
        if 1585 < etf_id && pick_etf_holding_stock(g, ui, rng, seller_id, corp_id) == 0 {
            msg = s(__content.string(12550))
                + __content.string(12377);
            if seller_id == 1593 {
                msg = msg
                    + &seller_name
                    + __content.string(12551)
                    + __content.string(11930)
                    + &g.ent_name[corp_id]
                    + __content.string(11931)
                    + &g.country_name[g.country_idx[corp_id]]
                    + __content.string(11932);
            } else {
                let desc = get_etf_description(seller_id);
                msg = msg
                    + &g.ent_name[corp_id]
                    + __content.string(11933)
                    + &g.ind_name[cast::<i32, _>(g.industry_num[corp_id])]
                    + __content.string(1037)
                    + __content.string(11934)
                    + &seller_name
                    + __content.string(12552)
                    + "\r"
                    + "\r"
                    + &desc
                    + "\r"
                    + __content.string(11936);
            }
            if g.industry_num[corp_id] == 71 {
                msg = seller_name.clone()
                    + __content.string(12553);
            }
            let text = msg.clone() + "\r" + "\r" + __content.string(12380);
            ui.msgbox(&text, 8208, &s(__content.string(12381)));
            return;
        }
        msg = PbStr::new();
        if long_rec == 0 && 10 < seller_id && opt_type == 2 {
            if g.industry_num[seller_id] < 3 {
                msg = s(__content.string(12433));
                part = s(__content.string(12536));
                if g.industry_num[seller_id] == 2 {
                    msg = s(__content.string(2267));
                    part = s(__content.string(12537));
                }
                msg = msg
                    + __content.string(12360)
                    + &part
                    + " money "
                    + __content.string(12554)
                    + __content.string(12555);
            } else if g.credit_rating_num[seller_id] < 7 {
                msg = s(__content.string(12556))
                    + __content.string(12557)
                    + &seller_name
                    + __content.string(12558)
                    + &g.credit_rating_str
                    + __content.string(12559)
                    + __content.string(12560);
                ui.msgbox(&msg, 8208, &s(__content.string(11731)));
                return;
            }
        }
        ok_flag = 0;
        if g.credit_rating_num[seller_id] < 7 && 10 < seller_id && !(is_closing == "Y") {
            if opt_type_ref == 1 && get_net_stock_pct_after_options(g, ui, seller_id, corp_id, 1) <= 0 {
                ok_flag = 1;
            } else if opt_type_ref == 2 {
                ok_flag = 1;
            }
            if ok_flag == 1 {
                msg = s(__content.string(12561));
                ui.msgbox(&msg, 8208, &s(__content.string(11689)));
                return;
            }
        }
        if g.shares_outstanding[corp_id] == Ext::from_f64(0.0) {
            show_runtime_error(g, ui, 151, &s(__content.string(12562)), &PbStr::new(), seller_id);
            return;
        }
        let share_price = g.market_cap[corp_id] / g.shares_outstanding[corp_id];
        if share_price < Ext::from_f64(5.0) || Ext::from_f64(1000.0) < share_price {
            part = s(__content.string(12395)) + &trim(g.currency_symbol.as_bytes()) + "5";
            if Ext::from_f64(1000.0) < share_price {
                part = s(__content.string(12396)) + &trim(g.currency_symbol.as_bytes()) + "1000";
            }
            msg = s(__content.string(12397))
                + &g.ent_name[corp_id]
                + ", a "
                + &part
                + &g.currency_suffix
                + __content.string(1071);
            part2 = s(__content.string(3311));
            if Ext::from_f64(1000.0) < share_price {
                part2 = PbStr::new();
            }
            if g.cur_player == g.corp_controller[corp_id] {
                msg = msg + __content.string(12398) + &part2 + __content.string(12399);
            }
            show_quick_message(g, ui, &mut msg);
            return;
        }
        ok_flag = 1;
        if g.credit_rating_num[corp_id] == 1 && g.proj_qtr_earnings[corp_id] <= Ext::from_f64(0.0) {
            ok_flag = 0;
        }
        if g.credit_rating_num[corp_id] == 0 {
            ok_flag = 0;
        }
        if ok_flag == 0 {
            msg = s(__content.string(11715))
                + &g.ent_name[corp_id]
                + __content.string(11716)
                + __content.string(11717);
        }
        ok_flag = 0;
        if 10 < g.merger_target && corp_id == g.merger_target && etf_id == 0 {
            let group = calc_group_stake_pct(g, cp, corp_id);
            let insider_pct = to_long(g.tmp_ext1 + cast::<Ext, _>(group));
            if 4 < insider_pct {
                msg = s(__content.string(11718))
                    + &g.ent_name[corp_id]
                    + __content.string(11719)
                    + __content.string(11720)
                    + &g.ent_symbol[corp_id]
                    + __content.string(11721)
                    + __content.string(11722)
                    + &g.ent_name[g.merger_acquirer]
                    + "."
                    + "\r"
                    + "\r"
                    + __content.string(11723)
                    + "\r"
                    + "\r"
                    + __content.string(11724)
                    + __content.string(11725)
                    + &g.ent_symbol[corp_id]
                    + __content.string(11726)
                    + __content.string(11727)
                    + "\r"
                    + "\r"
                    + __content.string(11728)
                    + &number_to_words(insider_pct, 0, 1)
                    + ".";
            }
            g.tmp_ext1 = Ext::from_f64(0.0);
        }
        if opt_type == 2 && msg.is_empty() && check_circular_option_holding(g, ui, rng, seller_id, corp_id) == 1 {
            msg = s(__content.string(12563));
            update_corp_control(g, ui, rng, corp_id);
            if g.public_pct == 0 {
                msg = PbStr::new();
            }
        }
        let max_opt_pct = get_max_options_pct(g, corp_id, opt_type, &s("S"), 0);
        if max_opt_pct == 0 {
            msg = s(__content.string(12564))
                + &opt_kind
                + __content.string(12386)
                + __content.string(12387)
                + __content.string(12388)
                + __content.string(12389)
                + __content.string(12390)
                + "\r"
                + "\r"
                + __content.string(12565)
                + &opt_kind
                + " "
                + __content.string(12392)
                + &g.ent_name[corp_id]
                + __content.string(12393)
                + __content.string(12394);
        }
        if !msg.is_empty() {
            ui.msgbox(&msg, 8208, &s(__content.string(11731)));
            return;
        }
        is_closing = s("N");
        if Ext::from_f64(998.0) <= val(g.option_recs[0].as_bytes()) {
            let mut m = s(__content.string(12400))
                + __content.string(12401)
                + __content.string(11692);
            show_quick_message(g, ui, &mut m);
            return;
        }
        if seller_id < 11 && !(opt_type == 1 && seller_id == g.corp_controller[corp_id]) {
            calc_entity_financials(g, ui, rng, seller_id);
            if !(g.tmp_total_debt * sf(0.35f32) <= g.tmp_credit_net_worth && 5 <= g.credit_rating_num[seller_id]) {
                msg = s(__content.string(12566))
                    + __content.string(12567);
                ui.msgbox(&msg, 8208, &s(__content.string(12568)));
                return;
            }
        }
        update_corp_control(g, ui, rng, corp_id);
        if g.public_pct == 0 {
            let mut m = s(__content.string(12402))
                + &g.ent_name[corp_id]
                + __content.string(12569);
            show_quick_message(g, ui, &mut m);
            return;
        }
        public_pct = g.public_pct;
        max_pct = 10;
        if (cast::<i64, _>(public_pct)) < max_pct && opt_type == 2 {
            max_pct = cast::<i64, _>(public_pct);
        }
        if (cast::<i64, _>(public_pct) - 1) < max_pct && 1585 < etf_id && opt_type == 2 {
            max_pct = cast::<i64, _>(public_pct) - 1;
            if max_pct == 0 {
                let mut m = s(__content.string(12402))
                    + &g.ent_name[seller_id]
                    + __content.string(12570)
                    + __content.string(12405)
                    + &g.ent_name[corp_id]
                    + __content.string(12406)
                    + "of "
                    + &g.ent_name[corp_id]
                    + __content.string(12407);
                show_quick_message(g, ui, &mut m);
                return;
            }
        }
        let mut hedge_check = 0;
        if opt_type_ref == 1 {
            if g.cur_player == g.corp_controller[corp_id] {
                hedge_check = 1;
            }
            if 10 < seller_id && g.industry_num[seller_id] < 3 {
                hedge_check = 1;
            }
            if max_opt_pct < 10 {
                hedge_check = 1;
            }
        } else {
            hedge_limit_pct = 10;
        }
        if 1585 < etf_id {
            hedge_check = 0;
        }
        let slot = cast::<i16, _>(find_holding_slot(g, seller_id, corp_id));
        if 0 < slot {
            owned_pct = g.hold_stock_pct[(cast::<i32, _>(slot), seller_id)];
        }
        if hedge_check == 1 {
            hedge_limit_pct = cast::<i16, _>(get_net_stock_pct_after_options(g, ui, seller_id, corp_id, 0));
            net_stock_pct = hedge_limit_pct;
            if !(10 < seller_id && g.industry_num[seller_id] < 3) && g.cur_player != g.corp_controller[corp_id] {
                hedge_limit_pct = 10;
            }
            if hedge_limit_pct <= 0 {
                msg = s(__content.string(12408))
                    + &g.ent_symbol[corp_id]
                    + __content.string(12409)
                    + __content.string(12571);
                part = s("you own");
                if 10 < seller_id {
                    msg = s(__content.string(12572))
                        + &g.ent_symbol[seller_id]
                        + " and "
                        + &g.ent_symbol[corp_id]
                        + ", "
                        + &g.ent_symbol[seller_id]
                        + __content.string(12573)
                        + __content.string(12574);
                    if g.industry_num[seller_id] < 3 {
                        msg = g.ent_symbol[seller_id].clone()
                            + __content.string(12575)
                            + &g.ent_symbol[corp_id]
                            + " shares";
                    }
                    part = s("it owns");
                }
                msg = msg
                    + __content.string(12576)
                    + __content.string(12577)
                    + __content.string(12578)
                    + &part
                    + " in "
                    + &g.ent_symbol[corp_id]
                    + " ("
                    + &trim(str_num(cast::<Ext, _>(owned_pct), 7).as_bytes())
                    + "%)."
                    + "\r"
                    + "\r";
                if g.cur_player == g.corp_controller[corp_id] {
                    msg = msg
                        + __content.string(12579)
                        + __content.string(12580)
                        + "\r"
                        + "\r";
                }
                if owned_pct == 0 {
                    if g.cur_player == seller_id {
                        msg = msg + __content.string(12422);
                    } else {
                        msg = msg + &seller_name + __content.string(12423);
                    }
                    msg = msg + __content.string(12581) + &g.ent_symbol[corp_id] + "." + "\r" + "\r";
                } else {
                    msg = msg + __content.string(12425) + &g.ent_symbol[corp_id] + __content.string(12426) + "\r" + "\r";
                }
                let ignored = 0 < find_option_positions(g, ui, seller_id, corp_id, 1, 0)
                    || Ext::from_f64(30.0) < (cast::<Ext, _>(find_option_positions(g, ui, seller_id, corp_id, 2, 0))).abs();
                if ignored {
                    if 10 < seller_id && g.industry_num[seller_id] < 3 {
                        part = s("Bank");
                        if g.industry_num[seller_id] == 2 {
                            part = s(__content.string(2878));
                        }
                        msg = msg
                            + &part
                            + __content.string(9835)
                            + &g.ent_symbol[seller_id]
                            + __content.string(12582)
                            + __content.string(12583)
                            + "\r"
                            + "\r";
                    } else {
                        msg = msg + __content.string(12427) + "\r" + "\r";
                    }
                }
                msg = msg + __content.string(12584) + &g.ent_symbol[corp_id] + __content.string(12585);
                part = s(__content.string(3253));
                if g.cur_player != g.corp_controller[corp_id] {
                    part = s(__content.string(12586));
                }
                ui.msgbox(&msg, 8208, &part);
                return;
            }
            if !((max_opt_pct < cast::<i32, _>(hedge_limit_pct) && (cast::<i64, _>(max_opt_pct)) < max_pct)
                || max_pct <= cast::<i64, _>(hedge_limit_pct))
            {
                max_pct = cast::<i64, _>(hedge_limit_pct);
                msg = s(__content.string(12587))
                    + &str_s(cast::<Ext, _>(max_pct))
                    + __content.string(2163)
                    + &g.ent_symbol[corp_id]
                    + __content.string(12430)
                    + __content.string(12431)
                    + __content.string(12432)
                    + "\r"
                    + "\r";
                if 10 < seller_id {
                    msg = seller_name.clone()
                        + __content.string(12588)
                        + &str_s(cast::<Ext, _>(max_pct))
                        + __content.string(2163)
                        + &g.ent_symbol[corp_id]
                        + "."
                        + "\r"
                        + "\r";
                    if seller_id < 131 && g.corp_controller[seller_id] != g.corp_controller[corp_id] {
                        part = s(__content.string(12589));
                        if g.industry_num[seller_id] == 2 {
                            part = s(__content.string(12590));
                        }
                        msg = msg
                            + &part
                            + __content.string(12591)
                            + __content.string(12592)
                            + "\r"
                            + "\r";
                    } else {
                        msg = msg
                            + __content.string(12593)
                            + &g.ent_symbol[seller_id]
                            + __content.string(12594)
                            + __content.string(12595)
                            + __content.string(12596)
                            + "\r"
                            + "\r";
                    }
                }
                if max_pct == cast::<i64, _>(owned_pct) {
                    part = s(__content.string(12440));
                    if 10 < seller_id {
                        part = g.ent_symbol[seller_id].clone() + __content.string(12441);
                    }
                    msg = msg
                        + &part
                        + &PbStr::from(str_num(cast::<Ext, _>(owned_pct), 7).as_str())
                        + __content.string(2163)
                        + &g.ent_symbol[corp_id]
                        + ".";
                } else {
                    part = s("your");
                    if 10 < seller_id {
                        part = g.ent_symbol[seller_id].clone() + "'s";
                    }
                    msg = msg
                        + "All but"
                        + &str_s(cast::<Ext, _>(max_pct))
                        + "% of "
                        + &part
                        + __content.string(9545)
                        + &g.ent_symbol[corp_id]
                        + __content.string(12442);
                }
                let ignored = 0 < find_option_positions(g, ui, seller_id, corp_id, 1, 0)
                    || Ext::from_f64(30.0) < (cast::<Ext, _>(find_option_positions(g, ui, seller_id, corp_id, 2, 0))).abs();
                if ignored && (seller_id < 11 || 130 < seller_id) {
                    msg = msg + "\r" + "\r" + __content.string(12427);
                }
                part = s(__content.string(12597));
                if 10 < seller_id {
                    part = s(__content.string(3253));
                    if seller_id < 131 {
                        part = s(__content.string(12586));
                    }
                }
                ui.msgbox(&msg, 8192, &part);
            }
        } else if opt_type_ref == 1 && seller_id < 11 && g.cur_player != g.corp_controller[corp_id] {
            net_stock_pct = 10;
        }
        if max_opt_pct < cast::<i32, _>(hedge_limit_pct) && (cast::<i64, _>(max_opt_pct)) < max_pct && etf_id == 0 {
            msg = s(__content.string(12598))
                + &opt_kind
                + " "
                + __content.string(12446)
                + &str_s(cast::<Ext, _>(100 - max_opt_pct))
                + __content.string(12599)
                + &g.ent_symbol[corp_id]
                + __content.string(12447)
                + __content.string(12448)
                + &opt_kind
                + __content.string(12600)
                + "\r"
                + "\r"
                + __content.string(12601)
                + &opt_kind
                + __content.string(12365)
                + __content.string(12450)
                + &str_s(cast::<Ext, _>(max_opt_pct))
                + __content.string(2163)
                + &g.ent_name[corp_id]
                + ".";
            max_pct = cast::<i64, _>(max_opt_pct);
            ui.msgbox(&msg, 8256, &s(__content.string(12451)));
        }
        let mut min_strike = (g.market_cap[corp_id] / g.shares_outstanding[corp_id] * sf(0.75f32)).abs();
        if min_strike < Ext::from_f64(5.0) {
            min_strike = Ext::from_f64(5.0);
        }
        let max_strike = (g.market_cap[corp_id] / g.shares_outstanding[corp_id] * sf(1.25f32)).abs();
        msg = s(__content.string(12452))
            + &opt_kind
            + __content.string(12602)
            + &fmt_trim(min_strike, fmt_str)
            + __content.string(12454)
            + &fmt_trim(max_strike, fmt_str)
            + __content.string(12455);
        let entered = loop {
            let price_str = fmt_trim(g.market_cap[corp_id] / g.shares_outstanding[corp_id], fmt_str);
            let title = g.ent_symbol[corp_id].clone() + __content.string(12456) + &price_str;
            let input = ui.inputbox(&msg, &title, &price_str);
            let x = val_ext(remove_any(input.as_bytes(), b"$ ,%").as_bytes());
            let v = x.into_ext();
            if v == Ext::from_f64(0.0) {
                return;
            }
            if !(v < min_strike - sf(0.01f32) || max_strike + sf(0.01f32) < v) {
                break x;
            }
        };
        strike_x = entered.fmul(Ext80::from_ext(g.shares_outstanding[corp_id])).abs();
        strike = Ext::from(strike_x.into_ext());
    }

    // LAB_005ed5cb: the option series (opening sales), then the bid and the size limits.
    let mut bid_per_share;
    loop {
        if let Some(b) = closing_bid.take() {
            bid_total_x = b;
        } else {
            let opt_rec = show_option_chain(g, ui, rng, corp_id, strike, opt_type, &price_side);
            if opt_rec.is_empty() {
                answer = ui.msgbox(
                    &s(__content.string(12603)),
                    8723,
                    &s(__content.string(12458)),
                );
                if answer == 7 {
                    continue;
                }
                return;
            }
            exp_month = to_long(fval(&opt_rec, 1));
            exp_year = to_long(fval(&opt_rec, 2));
            bid_total_x = Ext80::from_ext(g.shares_outstanding[corp_id])
                .fmul(val_ext(parse_any(opt_rec.as_bytes(), b"|", 4).as_bytes()));
        }
        // LAB_005ed70e
        let bid_total = bid_total_x.into_ext();
        bid_per_share = bid_total / g.shares_outstanding[corp_id];
        if val(fmt_trim(bid_per_share, fmt_str).as_bytes()) < sf(0.01f32)
            || val(fmt_trim(Ext::from(bid_total), fmt_str).as_bytes()) < sf(0.01f32)
        {
            msg = s(__content.string(12604))
                + &g.ent_symbol[corp_id]
                + " "
                + &opt_kind
                + __content.string(12605)
                + __content.string(12606)
                + __content.string(12607)
                + "\r"
                + "\r";
            if is_closing == "N" {
                msg = msg
                    + __content.string(12608)
                    + __content.string(12609)
                    + __content.string(12610);
                ui.msgbox(&msg, 8256, &s(__content.string(12611))); // (sic, #250)
                return;
            }
            if is_vested == 1 {
                msg = s(__content.string(12612))
                    + "("
                    + &g.ent_symbol[corp_id]
                    + __content.string(12613)
                    + __content.string(12614);
                ui.msgbox(&msg, 8256, &s(__content.string(12615)));
                return;
            }
            mercy_sale = 1;
            msg = msg
                + __content.string(12616)
                + __content.string(12617)
                + __content.string(12618)
                + "\r"
                + "\r"
                + __content.string(12619);
            part = g.ent_symbol[corp_id].clone()
                + &str_s(cast::<Ext, _>(exp_month))
                + "/"
                + &str_trim(cast::<Ext, _>(exp_year))
                + " "
                + &trim(g.currency_symbol.as_bytes())
                + &fmt_trim(strike / g.shares_outstanding[corp_id], fmt_str)
                + " "
                + &mc(&opt_kind)
                + "s";
            let title = part.clone() + " -- " + "\"" + __content.string(12620) + "\"" + __content.string(12621);
            answer = ui.msgbox(&msg, 8483, &title);
            if answer != 6 {
                return;
            }
            if bid_per_share < sf(0.01f32) {
                bid_per_share = sf(0.01f32);
            }
            if bid_total < sf(0.01f32) {
                bid_total_x = Ext80::from_ext(sf(0.01f32));
            }
        }
        calc_entity_financials(g, ui, rng, seller_id);
        if is_closing == "N" {
            let mut naked_liab = Ext::from_f64(0.0);
            let mut call_liab = Ext::from_f64(0.0);
            let mut put_liab = Ext::from_f64(0.0);
            if Ext::from_f64(0.0) < g.long_options_value - g.options_net_value {
                naked_liab = get_plr_naked_liab(g, ui, rng, seller_id, 0, 0);
                call_liab = get_plr_naked_liab(g, ui, rng, seller_id, corp_id, 1);
                put_liab = get_plr_naked_liab(g, ui, rng, seller_id, corp_id, 2);
            }
            let mut margin_avail = g.tmp_credit_net_worth * Ext::from_f64(2.0) - naked_liab;
            if 10 < seller_id {
                margin_avail = g.tmp_credit_net_worth * sf(0.25f32) - naked_liab;
            }
            let mut liab_relief = Ext::from_f64(0.0);
            if opt_type == 1 && call_liab < put_liab {
                liab_relief = put_liab - call_liab;
            } else if opt_type == 2 && put_liab < call_liab {
                liab_relief = call_liab - put_liab;
            }
            if margin_avail < liab_relief {
                margin_avail = liab_relief;
            }
            if bid_total_x.into_ext() * (cast::<Ext, _>(max_pct)) < sf(0.01f32) {
                bid_total_x = Ext80::from_ext(sf(0.01f32));
            }
            let afford = margin_avail / bid_total_x.into_ext();
            afford_pct = to_quad(afford.trunc());
            if afford_pct < max_pct {
                max_pct = afford_pct;
                limit_reason = s("LIAB");
            }
            if 0 < hedge_limit_pct && (cast::<i64, _>(hedge_limit_pct)) < max_pct {
                max_pct = cast::<i64, _>(hedge_limit_pct);
            }
            if (cast::<i64, _>(public_pct)) < max_pct && opt_type == 2 {
                max_pct = cast::<i64, _>(public_pct);
            }
            if opt_type == 1 && etf_id == 0 {
                if 0 < max_pct {
                    limit_reason = s(__content.string(12622));
                }
                if g.cur_player != g.corp_controller[corp_id] && 130 < seller_id {
                    net_pct_cap = cast::<i64, _>(get_net_stock_pct_after_options(g, ui, seller_id, corp_id, 1));
                    if 10 < net_pct_cap {
                        net_pct_cap = 10;
                    }
                    if net_pct_cap < 0 {
                        net_pct_cap = 0;
                    }
                    if max_pct < net_pct_cap {
                        max_pct = net_pct_cap;
                    } else if net_pct_cap < max_pct && g.credit_rating_num[seller_id] < 7 {
                        max_pct = net_pct_cap;
                    }
                } else if !(g.cur_player == seller_id
                    && g.cur_player != g.corp_controller[corp_id]
                    && 6 < g.credit_rating_num[seller_id])
                {
                    net_pct_cap = cast::<i64, _>(get_net_stock_pct_after_options(g, ui, seller_id, corp_id, 0));
                    if 10 < net_pct_cap {
                        net_pct_cap = 10;
                    }
                    if net_pct_cap < 0 {
                        net_pct_cap = 0;
                    }
                    if (cast::<i64, _>(net_stock_pct)) < net_pct_cap && net_stock_pct < 10 {
                        net_pct_cap = cast::<i64, _>(net_stock_pct);
                    }
                    max_pct = net_pct_cap;
                }
            }
            if max_pct < 1 && limit_reason == "LIAB" {
                msg = s(__content.string(12623))
                    + __content.string(12624)
                    + &opt_kind
                    + __content.string(12464)
                    + &g.ent_name[corp_id]
                    + " stock";
                if 10 < seller_id {
                    let with = seller_name.clone() + " does";
                    msg = replace(msg.as_bytes(), b"You do", with.as_bytes());
                }
                if margin_avail <= Ext::from_f64(0.0) {
                    show_quick_message(g, ui, &mut msg);
                    return;
                }
                msg = msg
                    + __content.string(12625)
                    + __content.string(12626)
                    + "\r"
                    + "\r"
                    + __content.string(12465)
                    + __content.string(12627);
                answer = ui.msgbox(&msg, 8483, &s(__content.string(12628)));
                if answer == 6 {
                    max_pct = 1;
                    continue;
                }
                return;
            }
            if 10 < seller_id && net_pct_cap == 0 {
                if g.credit_rating_num[seller_id] < 7 || afford_pct < 1 {
                    msg = s(__content.string(12629));
                    if 6 < g.credit_rating_num[seller_id] {
                        msg = g.ent_symbol[seller_id].clone()
                            + __content.string(12630)
                            + __content.string(12631)
                            + __content.string(12632);
                        if Ext::from_f64(0.0) < g.long_options_value {
                            msg = msg
                                + __content.string(12633)
                                + __content.string(12634);
                        }
                    }
                    msg = msg + "\r" + "\r" + __content.string(12635);
                    ui.msgbox(&msg, 8208, &s(__content.string(12615)));
                    return;
                }
            }
            if max_pct < 1 {
                msg = s(__content.string(12636));
                ui.msgbox(&msg, 8208, &s(__content.string(12615)));
                return;
            }
        }
        break;
    }
    if 10 < max_pct && is_closing == "N" {
        max_pct = 10;
    }
    msg = s(__content.string(12637))
        + &opt_kind
        + __content.string(12470)
        + &str_s(cast::<Ext, _>(max_pct))
        + __content.string(2163)
        + &g.ent_name[corp_id]
        + ".  ";
    if mercy_sale == 0 {
        msg = msg
            + __content.string(12638)
            + &fmt_trim(bid_per_share, fmt_str)
            + ". ";
    }
    msg = msg + __content.string(12473) + &str_s(cast::<Ext, _>(max_pct)) + __content.string(12639);
    let mut corp_label = g.ent_name[corp_id].clone();
    if 7 < len(corp_label.as_bytes()) {
        corp_label = g.ent_symbol[corp_id].clone();
    }
    let mut pct_to_sell;
    loop {
        let title = s("Sell ")
            + &corp_label
            + &str_s(cast::<Ext, _>(exp_month))
            + "/"
            + &str_trim(cast::<Ext, _>(exp_year))
            + " "
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(strike / g.shares_outstanding[corp_id], fmt_str)
            + " "
            + &mc(&opt_kind)
            + "s";
        let input = ui.inputbox(&msg, &title, &str_s(cast::<Ext, _>(max_pct)));
        pct_to_sell = to_long(val(remove_any(input.as_bytes(), b"$ ,%").as_bytes()));
        if pct_to_sell < 1 {
            return;
        }
        if cast::<i64, _>(pct_to_sell) <= max_pct {
            ok_flag = 1;
        }
        if ok_flag == 1 {
            break;
        }
    }
    let mut proceeds_x = bid_total_x.fmul(Ext80::from_i64(cast::<i64, _>(pct_to_sell)));
    if is_vested == 0 {
        proceeds_x = Ext80::from_ext(sf(0.99f32)).fmul(proceeds_x);
    }
    msg = s(__content.string(12475))
        + "\r"
        + "\r"
        + __content.string(12640)
        + &trim(g.currency_symbol.as_bytes());
    if mercy_sale == 0 {
        msg = msg
            + &fmt_trim(Ext::from(proceeds_x.into_ext()), "* ##############,.00")
            + " "
            + &g.million_prefix_lc
            + "illion."
            + "\r"
            + "\r";
        if is_vested == 1 {
            msg = msg
                + __content.string(12641)
                + &g.ent_symbol[corp_id]
                + ". "
                + __content.string(12642);
            if corp_id != g.player_ceo_of[g.cur_player] {
                msg = replace(msg.as_bytes(), __content.bytes(6389), __content.bytes(12643));
            }
        } else {
            msg = msg + __content.string(12644);
        }
    } else {
        msg = msg + ".001 " + &g.million_prefix_lc + __content.string(9583) + __content.string(12645);
        proceeds_x = Ext80::from_ext(sf(0.001f32));
    }
    let proceeds = proceeds_x.into_ext();
    msg = msg + "\r" + "\r" + __content.string(12478) + "\r" + "\r";
    if is_closing == "Y" && pos_qty != 0 {
        let sell_frac = Ext80::from_i64(cast::<i64, _>(pct_to_sell)).fdiv(Ext80::from_i64((cast::<i64, _>(pos_qty)).abs()));
        let gain = proceeds_x.fsub(sell_frac.fmul(orig_premium_x).abs()).into_ext();
        if Ext::from_f64(0.0) < gain {
            let mut who = s("You");
            if 10 < seller_id {
                who = g.ent_symbol[seller_id].clone();
            }
            msg = msg
                + "("
                + &who
                + __content.string(12646)
                + &trim(g.currency_symbol.as_bytes())
                + &fmt_trim(Ext::from(gain), "* #############,.00")
                + " "
                + &g.million_prefix_lc
                + __content.string(12647);
            if 1585 < seller_id && g.industry_num[seller_id] == 71 {
                msg = remove(msg.as_bytes(), __content.bytes(12481));
            }
        }
    }
    answer = ui.msgbox(&msg, 8227, &s(__content.string(12648)));
    if answer != 6 {
        return;
    }
    let mut new_rec = str_trim(cast::<Ext, _>(corp_id))
        + "|"
        + &str_s(cast::<Ext, _>(opt_type))
        + "|"
        + &str_s(cast::<Ext, _>(seller_id))
        + "|"
        + &str_s(-(cast::<Ext, _>(pct_to_sell)))
        + "|"
        + &str_x(strike_x)
        + "|"
        + &str_s(cast::<Ext, _>(exp_month))
        + "|"
        + &str_s(cast::<Ext, _>(exp_year))
        + "|";
    if !(is_closing == "Y") {
        orig_premium_x = proceeds_x;
        new_rec = new_rec + &str_x(orig_premium_x);
        if find_option_counterparty(g, ui, rng, &new_rec, Ext::from(proceeds), size_factor) < 11 {
            msg = PbStr::new();
            if 1 < pct_to_sell {
                msg = s("\r") + "\r" + __content.string(12483);
            }
            let text = s(__content.string(12649))
                + __content.string(12650)
                + __content.string(12486)
                + &msg
                + "\r"
                + "\r"
                + __content.string(12487);
            ui.msgbox(&text, 8208, &s(__content.string(12488)));
            return;
        }
    }
    g.cash[seller_id] = proceeds + g.cash[seller_id];
    if is_vested == 1 {
        g.accrued_pl[corp_id] = g.accrued_pl[corp_id] - proceeds;
        g.cash[corp_id] = g.cash[corp_id] - proceeds;
    }
    let sold_pct = pct_to_sell;
    let mut sale_gain = Ext::from_f64(0.0);
    if is_closing == "Y" && pos_qty != 0 {
        if (cast::<i64, _>(pos_qty)).abs() == cast::<i64, _>(pct_to_sell) {
            sale_gain = Ext::from(proceeds_x.fsub(orig_premium_x.abs()).into_ext());
            pct_to_sell = 0;
        } else {
            let sell_frac = Ext80::from_i64(cast::<i64, _>(pct_to_sell)).fdiv(Ext80::from_i64((cast::<i64, _>(pos_qty)).abs()));
            sale_gain = Ext::from(proceeds_x.fsub(sell_frac.fmul(orig_premium_x.abs())).into_ext());
            orig_premium_x = Ext80::from_i64(1).fsub(sell_frac).fmul(orig_premium_x);
            pct_to_sell = pos_qty - pct_to_sell;
        }
        if seller_id < 11 {
            g.player_cap_gains[seller_id] = sale_gain + g.player_cap_gains[seller_id];
        } else {
            g.extraordinary_pl[seller_id] = to_quad(sale_gain + cast::<Ext, _>(g.extraordinary_pl[seller_id]));
        }
    }
    let mut new_qty = to_long((cast::<Ext, _>(pct_to_sell)).abs());
    if !(is_closing == "Y") {
        new_qty = new_qty.wrapping_mul(-1);
    }
    part = str_x(orig_premium_x);
    if is_vested == 1 {
        part = s("VESTED");
    }
    msg = str_trim(cast::<Ext, _>(corp_id))
        + "|"
        + &str_s(cast::<Ext, _>(opt_type))
        + "|"
        + &str_s(cast::<Ext, _>(seller_id))
        + "|"
        + &str_s(cast::<Ext, _>(new_qty))
        + "|"
        + &str_x(strike_x)
        + "|"
        + &str_s(cast::<Ext, _>(exp_month))
        + "|"
        + &str_s(cast::<Ext, _>(exp_year))
        + "|"
        + &part;
    if is_closing == "Y" {
        add_or_remove_option_rec(g, ui, &PbStr::new(), long_rec);
        if pct_to_sell != 0 {
            add_or_remove_option_rec(g, ui, &msg, 0);
        }
    } else if merge_matching_option_rec(g, &msg) != 1 {
        add_or_remove_option_rec(g, ui, &msg, 0);
    }
    if 1585 < etf_id && 0 < g.fund_bonus_year[seller_id] {
        g.fund_bonus_year[seller_id] = g.cur_year;
    }
    g.turn_action_count = g.turn_action_count + 1;
    part = s(__content.string(12651));
    if 10 < seller_id {
        part = g.ent_symbol[seller_id].clone() + __content.string(3012);
    }
    let mut opt_rec = s(__content.string(12489))
        + "\r"
        + "\r"
        + &part
        + &opt_kind
        + __content.string(12652)
        + &g.month_name[exp_month]
        + ","
        + &str_s(cast::<Ext, _>(exp_year))
        + ", "
        + __content.string(12653)
        + &fmt_trim(strike / g.shares_outstanding[corp_id], fmt_str)
        + __content.string(6415)
        + __content.string(12654)
        + &number_to_words(sold_pct, 0, 0)
        + __content.string(12655)
        + __content.string(12656)
        + &g.ent_name[corp_id]
        + " ("
        + &g.ent_symbol[corp_id]
        + "), or ";
    part = fmt_trim(cast::<Ext, _>(sold_pct) * g.shares_outstanding[corp_id], fmt_str) + " " + &g.million_prefix_lc + "illion";
    opt_rec = opt_rec + &part + __content.string(12496) + "\r" + "\r";
    if sf(0.01f32) < val(fmt_trim(bid_per_share, fmt_str).as_bytes()) {
        opt_rec = opt_rec
            + __content.string(12657)
            + &fmt_trim(bid_per_share, fmt_str)
            + __content.string(12658)
            + "\r"
            + "\r";
    }
    opt_rec = opt_rec
        + __content.string(12659)
        + &trim(g.currency_symbol.as_bytes())
        + &fmt_trim(Ext::from(proceeds), "* #############,.00")
        + " "
        + &g.million_prefix_lc
        + "illion"
        + &g.currency_suffix
        + "."
        + "\r"
        + "\r";
    if is_closing == "Y" {
        part = s("gain");
        if sale_gain < Ext::from_f64(0.0) {
            part = s("loss");
        }
        opt_rec = opt_rec
            + __content.string(12660)
            + &opt_kind
            + __content.string(12661)
            + &part
            + " of: "
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(sale_gain.abs(), fmt_str)
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix
            + "."
            + "\r"
            + "\r";
        if find_option_positions(g, ui, seller_id, corp_id, opt_type, 0) < 1 {
            msg = s("your");
            if 10 < seller_id {
                msg = g.ent_symbol[seller_id].clone() + "'s";
            }
            opt_rec = opt_rec
                + __content.string(12502)
                + &msg
                + " long "
                + &opt_kind
                + __content.string(12503)
                + &g.ent_symbol[corp_id]
                + ".";
        } else {
            msg = s(__content.string(12504));
            if 10 < seller_id {
                msg = g.ent_symbol[seller_id].clone() + __content.string(12505);
            }
            opt_rec = opt_rec + &msg + " long " + &opt_kind + __content.string(12503) + &g.ent_name[corp_id] + ".";
        }
        if is_vested == 1 {
            opt_rec = s("\r")
                + "\r"
                + __content.string(12662)
                + __content.string(12663)
                + &g.ent_name[corp_id]
                + "."
                + "\r"
                + "\r"
                + __content.string(12664);
            g.proj_qtr_earnings[corp_id] = calc_projected_earnings(g, ui, rng, corp_id);
        }
    } else {
        g.has_options_outstanding[corp_id] = 1;
        opt_rec = opt_rec
            + __content.string(12665)
            + __content.string(12666);
        if opt_type == 1 {
            opt_rec = opt_rec + __content.string(12667);
        } else {
            opt_rec = opt_rec
                + __content.string(12668)
                + __content.string(12669)
                + __content.string(12670);
        }
        opt_rec = opt_rec
            + "\r"
            + "\r"
            + __content.string(12671)
            + __content.string(12672)
            + __content.string(12673)
            + __content.string(12674);
        if g.player_exercise_options[g.cur_player] == 0 {
            opt_rec = opt_rec + __content.string(12518);
        }
    }
    ui.msgbox(&opt_rec, 8256, &s(__content.string(12675)));
    update_market_display(g, ui, rng, time_secs);
    part = s(" shorts");
    if len(g.ent_symbol[seller_id].as_bytes()) < 20 {
        part = s(__content.string(12676));
    }
    part2 = PbStr::new();
    if is_closing == "Y" {
        part = s(" cashes");
        if Ext::from_f64(0.0) < sale_gain {
            part2 = s(__content.string(12677));
        } else if sale_gain < Ext::from_f64(0.0) {
            part2 = s(__content.string(12678));
            part = s(" sells");
        }
    }
    let stock_of = if len(g.ent_symbol[seller_id].as_bytes()) < 13 { s(__content.string(2146)) } else { PbStr::new() };
    g.news_msg = g.ent_symbol[seller_id].clone()
        + &part
        + &str_s(cast::<Ext, _>(exp_month))
        + "/"
        + &str_trim(cast::<Ext, _>(exp_year))
        + " "
        + &opt_kind
        + __content.string(12534)
        + &stock_of
        + &g.ent_symbol[corp_id]
        + &part2;
    post_news(g, ui, 0, 0);
    g.valuation_mode = 1;
    calc_stock_price(g, ui, rng, corp_id, time_secs);
}

/// Port of ExerciseOptionsMenu (0x600b42).
///
/// Early exercise of a long `opt_type` option (1 = call, 2 = put) of the acting entity: without any
/// (exercisable: no GRANT calls) position a message ends it; else the stock is entered in the symbol
/// dialog ([`in_box_symbol`]), the record picked from `ShowOptionPositions` (long calls / long puts on
/// that stock). GRANT options and [runtime resource wording] (strike vs. `MarketCap`) are refused; the
/// percent (up to the record's) is entered and [`exercise_option`] (manual) does the exercise: result
/// 1 = in full (the record is removed), 2 = partially (limits), else in part as requested. Then the
/// confirmation, a turn action, `UpdateCorpControl` and `ForceCloseExcessNakedOptions` (controller).
///
/// ERROR 151 (`SharesOutstanding = 0`) ends in the ON ERROR handler
/// (`ShowRuntimeError(151, "FrontExercise", "OptType& =" + STR$(optType), corp)`).
// PB locals start zeroed; the port keeps them initialised like the original.
#[allow(unused_assignments)]
pub fn exercise_options_menu(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, opt_type_ref: i32) {
    let __content = crate::content::current();
    let fmt_str = "* ##############.00";
    let mut opt_kind = s("call");
    if opt_type_ref == 2 {
        opt_kind = s("put");
    }
    let opt_type = opt_type_ref;
    let ae = g.active_entity;
    let mut msg = PbStr::new();
    let mut part;
    if opt_type_ref == 1 {
        if find_option_positions(g, ui, ae, 0, 1, 1) < 11 {
            msg = s(" ") + &opt_kind;
            if ae < 11 && 11 <= find_option_positions(g, ui, ae, 0, 1, 0) {
                msg = s(__content.string(12679)) + &msg;
            }
        }
    } else if find_option_positions(g, ui, ae, 0, 2, 0) < 21 {
        msg = s(" ") + &opt_kind;
    }
    if !msg.is_empty() {
        let mut m = g.active_entity_name.clone() + __content.string(9309) + &msg + __content.string(12680);
        show_quick_message(g, ui, &mut m);
        return;
    }
    msg = s(__content.string(11815));
    part = s("you (") + &g.cur_player_name + ") wish";
    if 10 < g.active_entity {
        part = g.active_entity_name.clone() + " wishes";
    }
    msg = msg + __content.string(12348) + &part + __content.string(12681) + &opt_kind + __content.string(12682);
    msg = msg + "\r\n" + "\r\n";
    g.input_dlg_label = PbStr::new();
    g.input_dlg_prompt = msg.clone();
    g.input_dlg_title = s(__content.string(12683)) + &mc(&opt_kind) + __content.string(12353);
    if 10 < g.active_entity {
        g.input_dlg_title = g.input_dlg_title.clone() + " for " + &g.ent_symbol[g.active_entity];
    }
    g.lookup_filter = PbStr::new();
    g.action_code = PbStr::new();
    in_box_symbol(g, ui, rng);
    msg = PbStr::new();
    if g.tmp_symbol.is_empty() {
        return;
    }
    let corp_id = g.target_corp;
    g.target_corp = 0;
    let ae = g.active_entity;
    let mut has_long = 0;
    let mut rec_idx;
    if opt_type_ref == 1 {
        rec_idx = find_option_positions(g, ui, ae, corp_id, 1, 1);
        if rec_idx == 1 || rec_idx == 2 {
            has_long = 1;
        }
    } else {
        rec_idx = find_option_positions(g, ui, ae, corp_id, 2, 0);
        if rec_idx == 31 || rec_idx == 32 {
            has_long = 1;
        }
    }
    if has_long == 0 {
        msg = PbStr::new();
        if opt_type_ref == 1 && g.active_entity < 11 {
            rec_idx = find_option_positions(g, ui, ae, corp_id, 1, 0);
            if rec_idx == 1 || rec_idx == 2 {
                msg = s(__content.string(12679));
            }
        }
        let mut m = g.active_entity_name.clone()
            + __content.string(9309)
            + &msg
            + " "
            + &opt_kind
            + __content.string(12684)
            + &g.ent_name[corp_id]
            + ".";
        show_quick_message(g, ui, &mut m);
        return;
    }
    rec_idx = show_option_positions(g, ui, rng, ae, opt_type, corp_id);
    if rec_idx == 0 {
        return;
    }
    let opt_rec = g.option_recs[rec_idx].clone();
    let rec_pct = to_long(fval(&opt_rec, 4));
    let strike = fval(&opt_rec, 5).abs();
    let exp_month = to_long(fval(&opt_rec, 6));
    let exp_year = to_long(fval(&opt_rec, 7));
    if ftrim(&opt_rec, 8) == "GRANT" {
        part = s("current");
        if g.active_entity < 11 && 0 < g.active_entity && corp_id != g.player_ceo_of[g.active_entity] {
            part = s("former");
        }
        msg = s(__content.string(12685))
            + __content.string(12546)
            + &part
            + __content.string(12686)
            + "mid-"
            + &g.month_name[exp_month]
            + ","
            + &str_s(cast::<Ext, _>(exp_year) - Ext::from_f64(1.0))
            + __content.string(12549);
        ui.msgbox(&msg, 8208, &s(__content.string(11731)));
        return;
    }
    let mut out_of_money = 0;
    if opt_type == 1 && g.market_cap[corp_id] < strike {
        out_of_money = 1;
    }
    if opt_type == 2 && strike < g.market_cap[corp_id] {
        out_of_money = 1;
    }
    if out_of_money == 1 {
        let mut m = s(__content.string(12687));
        show_quick_message(g, ui, &mut m);
        return;
    }
    if g.shares_outstanding[corp_id] == Ext::from_f64(0.0) {
        let diag = s(__content.string(12688)) + &str_s(cast::<Ext, _>(opt_type));
        show_runtime_error(g, ui, 151, &s(__content.string(12689)), &diag, corp_id);
        return;
    }
    msg = s(__content.string(12690))
        + &str_s(cast::<Ext, _>(rec_pct))
        + __content.string(12691)
        + &g.ent_symbol[corp_id]
        + __content.string(12692);
    let mut corp_label = g.ent_name[corp_id].clone();
    if 7 < len(corp_label.as_bytes()) {
        corp_label = g.ent_symbol[corp_id].clone();
    }
    let mut done = 0;
    let mut pct;
    loop {
        let title = s(__content.string(12693))
            + &corp_label
            + &str_s(cast::<Ext, _>(exp_month))
            + "/"
            + &str_trim(cast::<Ext, _>(exp_year))
            + " "
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(strike / g.shares_outstanding[corp_id], fmt_str)
            + " "
            + &mc(&opt_kind)
            + "s";
        let input = ui.inputbox(&msg, &title, &str_s(cast::<Ext, _>(rec_pct)));
        pct = to_long(val(remove_any(input.as_bytes(), b"$ ,%").as_bytes()));
        if pct < 1 {
            return;
        }
        if pct <= rec_pct {
            done = 1;
        }
        if done == 1 {
            break;
        }
    }
    let result = exercise_option(g, ui, rng, rec_idx, 1, pct);
    if result == 0 {
        return;
    }
    part = s("buy");
    if opt_type == 2 {
        part = s("sell");
    }
    msg = s(__content.string(12694)) + "\r" + "\r";
    if result == 1 {
        msg = msg
            + &mc(&opt_kind)
            + __content.string(12695)
            + &part
            + &str_s(cast::<Ext, _>(pct))
            + "% of "
            + __content.string(5234)
            + &g.ent_name[corp_id]
            + ".";
        add_or_remove_option_rec(g, ui, &PbStr::new(), rec_idx);
    } else if result == 2 {
        msg = msg
            + __content.string(12696)
            + __content.string(12697);
    } else {
        msg = msg
            + &mc(&opt_kind)
            + __content.string(12698)
            + &part
            + __content.string(12699)
            + &g.ent_name[corp_id]
            + ", "
            + __content.string(12700);
    }
    part = PbStr::new();
    if result == 2 {
        part = s(__content.string(12701));
    }
    let title = s(__content.string(12702)) + &part + __content.string(12703);
    ui.msgbox(&msg, 8256, &title);
    g.turn_action_count = g.turn_action_count + 1;
    update_corp_control(g, ui, rng, corp_id);
    force_close_excess_naked_options(g, ui, rng, 0, corp_id);
}
