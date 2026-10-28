//! The player's short sales: `SellShort` (0x5e0ea7) and `CoverShortPosition` (0x5e2654).

// Keep the original's statement shapes.
#![allow(clippy::assign_op_pattern, clippy::collapsible_if, clippy::collapsible_else_if)]
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use super::select::in_box_symbol;
use super::stock::sell_stock;
use crate::logic::control::update_corp_control;
use crate::logic::finance::calc_entity_financials;
use crate::logic::holdings::find_holding_slot;
use crate::logic::report::number_to_words;
use crate::logic::reports_lists::show_portfolio_listing;
use crate::logic::shorts::{calc_short_position_value, find_short_positions, recalc_short_margin_balances};
use crate::logic::sim_entity::mgmt::force_cover_shorts;
use crate::logic::valuation::{calc_stock_price, price_trend_streak};

/// Port of SellShort (0x5e0ea7).
///
/// The current player (acting as himself, `ActiveEntity <= NumPlayers`, and active) sells stock short:
/// the company is entered in the symbol dialog (`InBox(3)`, `ActionCode = "SELLSHORT"`). Refused (message
/// box) for a company the player controls (an insider; with a long position he may sell it instead,
/// [`sell_stock`]), and for companies rated below 2 that are near bankruptcy (no lendable shares) or under
/// bankruptcy protection (price under 5, assets under 95% of the debt or a projected loss). Refused (quick
/// message, then `FindShortPositions`) without a free holding slot, with a long position, with a short of
/// 20% already, without a public float, when the players' shorts reach half the float, when the net worth
/// is below half of the short liability, or when the 50% margin rule allows no 1%. A stock rising for
/// over 4 months draws the broker's warning (YES/NO/CANCEL, once per stock: `BrokerRiseWarnStock`). The
/// player enters the percent (at most 20% in total, 50% of the float less the players' shorts, and the
/// margin limit); the sale is at `MarketCap * (1 - 0.0043 * DifficultyLevel * pct)` (at least 90%), the
/// proceeds go to `PlayerShortMarginBal` and the (negative) holding's basis, the cap moves halfway to the
/// sale price and the sentiment drops by `pct / 2`; confirmation, news, `ShortPosFlag = 1`.
///
/// `time_secs` (`VAL(RIGHT$(TIME$, 2))`) is passed to [`sell_stock`].
// PB locals start zeroed; the port keeps them initialised like the original.
#[allow(unused_assignments)]
pub fn sell_short(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError("", "FrontSellShort") (error path not ported)
    if g.num_players < g.active_entity || g.entity_status[g.cur_player] != 1 {
        return;
    }
    let mut total_short_pct: i16 = 0;
    let mut aux_text = PbStr::new();
    g.tmp_symbol = PbStr::new();
    g.action_code = s("SELLSHORT");
    let mut msg_text = s(__content.string(11815));
    msg_text = msg_text
        + __content.string(11915)
        + &g.active_entity_name
        + __content.string(12740)
        + __content.string(12741)
        + "\r\n"
        + "\r\n";
    g.input_dlg_label = PbStr::new();
    g.input_dlg_prompt = msg_text.clone();
    g.input_dlg_title = s(__content.string(12742)) + &g.active_entity_name;
    g.lookup_filter = PbStr::new();
    in_box_symbol(g, ui, rng);
    msg_text = PbStr::new();
    if g.tmp_symbol.is_empty() {
        return;
    }
    let corp_id = g.target_corp;
    g.target_corp = 0;
    update_corp_control(g, ui, rng, corp_id);
    let ae = g.active_entity;
    if g.cur_player == g.voting_controller {
        let title_text = s(__content.string(3253));
        msg_text = s(__content.string(3842))
            + &g.ent_symbol[corp_id]
            + __content.string(12743)
            + __content.string(12744);
        let slot = find_holding_slot(g, ae, corp_id);
        if 0 < slot && cast::<i32, _>(g.hold_stock_id[(slot, ae)]) == corp_id && 0 < g.hold_stock_pct[(slot, ae)] {
            msg_text = msg_text
                + "\r"
                + "\r"
                + __content.string(12745)
                + &g.ent_symbol[corp_id]
                + __content.string(12746);
            let answer = ui.msgbox(&msg_text, 8227, &title_text);
            if answer == 6 {
                sell_stock(g, ui, rng, time_secs);
            }
            return;
        }
        ui.msgbox(&msg_text, 8208, &title_text);
        return;
    }
    calc_entity_financials(g, ui, rng, corp_id);
    if g.credit_rating_num[corp_id] < 2 {
        msg_text = PbStr::new();
        let price = g.market_cap[corp_id] / g.shares_outstanding[corp_id];
        if corp_id < 86 || g.total_assets < g.tmp_total_debt * sf(0.875f32) {
            msg_text = s(__content.string(12747))
                + &g.ent_name[corp_id]
                + __content.string(12748)
                + __content.string(12749);
            if price < Ext::from_f64(2.5) {
                msg_text =
                    replace(msg_text.as_bytes(), __content.bytes(12750), __content.bytes(12751));
            }
            aux_text = s(__content.string(12752));
        } else if g.total_assets < g.tmp_total_debt * sf(0.95f32) || price < Ext::from_f64(5.0) || g.proj_qtr_earnings[corp_id] < Ext::from_f64(0.0) {
            msg_text = s(__content.string(12753))
                + &g.ent_name[corp_id]
                + __content.string(12754)
                + __content.string(12755);
            aux_text = s(__content.string(12756)) + &g.ent_symbol[corp_id] + __content.string(12757);
        }
        if !msg_text.is_empty() {
            ui.msgbox(&msg_text, 8256, &aux_text);
            return;
        }
    }
    let slot = find_holding_slot(g, ae, corp_id);
    if slot == 0 {
        msg_text = s(__content.string(12758))
            + __content.string(12759);
        sell_short_exit(g, ui, rng, &mut msg_text);
        return;
    }
    let cur_pct: i16 = g.hold_stock_pct[(slot, ae)];
    msg_text = PbStr::new();
    if 0 < cur_pct {
        msg_text = s(__content.string(12760))
            + &PbStr::from(str_num(cast::<Ext, _>(g.hold_stock_pct[(slot, ae)]), 7).as_str())
            + __content.string(2163)
            + &g.ent_symbol[corp_id]
            + ", "
            + __content.string(12761);
    } else if cur_pct < -19 {
        msg_text = s(__content.string(12762))
            + &g.ent_symbol[corp_id]
            + __content.string(12763)
            + __content.string(12764);
    }
    if !msg_text.is_empty() {
        sell_short_exit(g, ui, rng, &mut msg_text);
        return;
    }
    update_corp_control(g, ui, rng, corp_id);
    if g.public_pct == 0 {
        msg_text = s(__content.string(12765))
            + &g.ent_name[corp_id]
            + ". "
            + __content.string(12766);
        sell_short_exit(g, ui, rng, &mut msg_text);
        return;
    }
    let num_players = g.num_players;
    for player_idx in 1..=num_players {
        if g.player_name[player_idx] == "...OUT..." {
            continue;
        }
        for slot_idx in 1..=15 {
            if cast::<i32, _>(g.hold_stock_id[(slot_idx, player_idx)]) == corp_id && g.hold_stock_pct[(slot_idx, player_idx)] < 0 {
                total_short_pct =
                    to_integer((cast::<Ext, _>(g.hold_stock_pct[(slot_idx, player_idx)])).abs() + cast::<Ext, _>(total_short_pct));
            }
        }
    }
    let mut max_pct: i16 = cast::<i16, _>(50 - cast::<i32, _>(total_short_pct));
    let room_pct = to_long(Ext::from_f64(20.0) - (cast::<Ext, _>(cur_pct)).abs());
    if room_pct < cast::<i32, _>(max_pct) {
        max_pct = cast::<i16, _>(room_pct);
    }
    let half_float = int(cast::<Ext, _>(g.public_pct) * Ext::from_f64(0.5));
    let lendable_pct = to_long(half_float - cast::<Ext, _>(total_short_pct));
    if lendable_pct < cast::<i32, _>(max_pct) {
        max_pct = cast::<i16, _>(lendable_pct);
    }
    if half_float <= cast::<Ext, _>(total_short_pct) || max_pct < 1 {
        msg_text = s(__content.string(12767))
            + "of "
            + &g.ent_name[corp_id]
            + " ("
            + &trim(str_num(cast::<Ext, _>(total_short_pct), 7).as_bytes())
            + __content.string(12768)
            + __content.string(12769)
            + __content.string(12770);
        sell_short_exit(g, ui, rng, &mut msg_text);
        return;
    }
    recalc_short_margin_balances(g);
    let short_liability = calc_short_position_value(g, ae);
    calc_entity_financials(g, ui, rng, ae);
    if g.net_worth < short_liability * Ext::from_f64(0.5) {
        msg_text = s(__content.string(12771))
            + __content.string(12772)
            + __content.string(12773);
        sell_short_exit(g, ui, rng, &mut msg_text);
        return;
    }
    let rise_months = price_trend_streak(g, corp_id);
    if 4 < rise_months && corp_id != g.broker_rise_warn_stock {
        let quote = s("\"");
        msg_text = s(__content.string(1093))
            + &g.ent_symbol[corp_id]
            + __content.string(12774)
            + &number_to_words(rise_months, 0, 0)
            + __content.string(12775)
            + "\r"
            + "\r"
            + __content.string(12776)
            + &quote
            + __content.string(12777)
            + &quote
            + "\r"
            + "\r"
            + __content.string(12778);
        let answer = ui.msgbox(&msg_text, 8483, &s(__content.string(11972)));
        g.broker_rise_warn_stock = corp_id;
        if answer != 6 {
            return;
        }
    }
    g.valuation_mode = 0;
    calc_stock_price(g, ui, rng, corp_id, 0); // mode 0 does not use TIME$
    let mut margin_max_pct = 20;
    let mut pct_try = 1;
    while pct_try <= 20 {
        let pos_value = g.market_cap[corp_id] * cast::<Ext, _>(pct_try);
        if g.net_worth * Ext::from_f64(2.0) < pos_value + g.player_short_margin_bal[ae] {
            margin_max_pct = pct_try - 1;
            pct_try = 20;
        }
        pct_try += 1;
    }
    if margin_max_pct == 0 {
        msg_text = s(__content.string(12779))
            + &g.ent_name[corp_id]
            + __content.string(12780)
            + __content.string(12781);
        sell_short_exit(g, ui, rng, &mut msg_text);
        return;
    }
    if margin_max_pct < cast::<i32, _>(max_pct) {
        max_pct = cast::<i16, _>(margin_max_pct);
    }
    let mut done = 0;
    while done != 1 {
        let max_text = trim(str_num(cast::<Ext, _>(max_pct), 7).as_bytes());
        msg_text = s(__content.string(12782))
            + &g.ent_symbol[corp_id]
            + __content.string(12783)
            + "\r\n"
            + "\r\n"
            + &g.ent_name[corp_id]
            + __content.string(12784)
            + &g.stock_price_str
            + "."
            + "\r\n"
            + "\r\n";
        msg_text = msg_text + "(" + &max_text + __content.string(12785);
        if cast::<i32, _>(max_pct) == margin_max_pct && max_pct < 20 {
            msg_text = msg_text + __content.string(12786);
        }
        let title = g.ent_symbol[ae].clone() + __content.string(12787);
        let input = ui.inputbox(&msg_text, &title, &(max_text.clone() + "%"));
        let pct_to_short = to_long(val(remove_any(input.as_bytes(), b"$ ,%").as_bytes()));
        if (cast::<i32, _>(max_pct)) < pct_to_short {
            let m = s(__content.string(12788))
                + &max_text
                + __content.string(12789)
                + __content.string(4168)
                + &g.ent_name[corp_id]
                + __content.string(12790);
            let title = g.app_title.clone();
            ui.msgbox(&m, 8208, &title);
            continue;
        }
        if pct_to_short < 1 {
            msg_text = s(__content.string(12014));
            sell_short_exit(g, ui, rng, &mut msg_text);
            return;
        }
        let mut sale_price = (Ext::from_f64(1.0) - sf(0.43f32) * sf(0.01f32) * cast::<Ext, _>(g.difficulty_level) * cast::<Ext, _>(pct_to_short))
            * g.market_cap[corp_id];
        if sale_price < g.market_cap[corp_id] * sf(0.9f32) {
            sale_price = g.market_cap[corp_id] * sf(0.9f32);
        }
        let proceeds = cast::<Ext, _>(pct_to_short) * sale_price;
        if g.hold_stock_id[(slot, ae)] == 0 {
            g.hold_stock_id[(slot, ae)] = cast::<i16, _>(corp_id);
        }
        g.hold_stock_pct[(slot, ae)] = cast::<i16, _>(cast::<i32, _>(g.hold_stock_pct[(slot, ae)]) - pct_to_short);
        g.player_short_margin_bal[ae] = proceeds + g.player_short_margin_bal[ae];
        g.hold_stock_basis[(slot, ae)] = proceeds + g.hold_stock_basis[(slot, ae)];
        g.market_cap[corp_id] = (g.market_cap[corp_id] + sale_price) / Ext::from_f64(2.0);
        g.investor_sentiment[corp_id] =
            cast::<i32, _>(to_quad(cast::<Ext, _>(g.investor_sentiment[corp_id]) - sf(0.5f32) * cast::<Ext, _>(pct_to_short)));
        done = 1;
        g.turn_action_count = g.turn_action_count + 1;
        let m = s(__content.string(12791))
            + &ucase(g.active_entity_name.as_bytes(), CaseMode::Ansi)
            + ":  "
            + "\r\n"
            + "\r\n"
            + &g.active_entity_name
            + __content.string(12792)
            + &str_s(cast::<Ext, _>(pct_to_short))
            + __content.string(2163)
            + &g.ent_name[corp_id]
            + ", "
            + __content.string(12793)
            + &g.currency_symbol
            + &fmt_trim(proceeds, "###########.0")
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix
            + "...";
        ui.msgbox(&m, 8192, &s(__content.string(9864)));
        g.news_msg = g.active_entity_name.clone()
            + __content.string(11172)
            + &str_s(cast::<Ext, _>(pct_to_short))
            + __content.string(2163)
            + &g.ent_symbol[corp_id]
            + " short.";
        post_news(g, ui, 0, 0);
        g.short_pos_flag = 1;
    }
    recalc_short_margin_balances(g);
}

/// The common refusal exit of SellShort (0x5e25aa): the message as a quick message, then
/// `FindShortPositions(0, 0)`.
fn sell_short_exit(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, msg_text: &mut PbStr) {
    show_quick_message(g, ui, msg_text);
    find_short_positions(g, ui, rng, 0, 0);
}

/// Port of CoverShortPosition (0x5e2654).
///
/// The current player (acting as himself, active) buys back stock he sold short: without short positions
/// (`FindShortPositions(0, ActiveEntity) = -3`) a quick message; else the short positions are listed
/// (`ShowPortfolioListing(ActiveEntity, 0, -1)`, `"GetCorp"` pick) and, for a picked short position, the
/// player enters the percent to cover (default all; more than the short is an error box and asks again,
/// less than 1 cancels). The cover is `ForceCoverShorts(4, ActiveEntity, corp, pct)` (one percent at a
/// time, the realized gain in `TmpExt1`); confirmation with the gain or loss over 0.1, and news.
// PB locals start zeroed; the port keeps them initialised like the original.
#[allow(unused_assignments)]
pub fn cover_short_position(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError("", "FrontShortCover") (error path not ported)
    if g.num_players < g.active_entity || g.entity_status[g.cur_player] != 1 {
        return;
    }
    let ae = g.active_entity;
    let mut msg_text;
    if find_short_positions(g, ui, rng, 0, ae) == -3 {
        msg_text = g.active_entity_name.clone() + __content.string(12794);
        show_quick_message(g, ui, &mut msg_text);
        return;
    }
    g.tmp_symbol = PbStr::new();
    g.entity_pick_mode = s("GetCorp");
    g.target_corp = 0;
    g.tmp_ext1 = -Ext::from_f64(1.0);
    show_portfolio_listing(g, ui, rng, ae, 0, -1);
    if g.target_corp == 0 {
        g.action_code = PbStr::new();
        return;
    }
    let corp_id = g.target_corp;
    g.target_corp = 0;
    let ae = g.active_entity;
    let slot = find_holding_slot(g, ae, corp_id);
    if slot == 0 || 0 <= g.hold_stock_pct[(slot, ae)] {
        msg_text = g.active_entity_name.clone()
            + __content.string(12795)
            + &g.ent_name[corp_id]
            + ".";
        show_quick_message(g, ui, &mut msg_text);
        return;
    }
    let short_pct: i16 = to_integer((cast::<Ext, _>(g.hold_stock_pct[(slot, ae)])).abs());
    recalc_short_margin_balances(g);
    g.valuation_mode = 0;
    calc_stock_price(g, ui, rng, corp_id, 0); // mode 0 does not use TIME$
    let mut basis_per_share = g.hold_stock_basis[(slot, ae)] / cast::<Ext, _>(short_pct);
    basis_per_share = basis_per_share.abs() / g.shares_outstanding[corp_id];
    let basis_str = fmt_trim(basis_per_share, "############.00");
    let mut done = 0;
    while done != 1 {
        msg_text = s(__content.string(4144))
            + &g.ent_name[corp_id]
            + __content.string(12796)
            + "\r\n"
            + "\r\n"
            + __content.string(12797)
            + &g.stock_price_str
            + "\r\n"
            + __content.string(12798)
            + &basis_str;
        let title = g.ent_symbol[ae].clone() + __content.string(12799);
        let default = trim(str_num(cast::<Ext, _>(short_pct), 7).as_bytes()) + "%";
        let answer = ui.inputbox(&msg_text, &title, &default);
        let pct_to_buy = to_long(val(remove_any(answer.as_bytes(), b"$ ,%").as_bytes()));
        if (cast::<i32, _>(short_pct)) < pct_to_buy {
            let m = s(__content.string(12800))
                + &PbStr::from(str_num(cast::<Ext, _>(short_pct), 7).as_str())
                + "% OF "
                + __content.string(11704)
                + &g.ent_name[corp_id]
                + ".";
            let title = g.app_title.clone();
            ui.msgbox(&m, 8208, &title);
            continue;
        }
        if pct_to_buy < 1 {
            msg_text = s(__content.string(12014));
            show_quick_message(g, ui, &mut msg_text);
            return;
        }
        done = 1;
        g.turn_action_count = g.turn_action_count + 1;
        let ae = g.active_entity;
        force_cover_shorts(g, ui, rng, 4, ae, corp_id, pct_to_buy);
        let realized_gain = g.tmp_ext1;
        g.tmp_ext1 = Ext::from_f64(0.0);
        msg_text = s(__content.string(12801))
            + "\r\n"
            + "\r\n"
            + &g.active_entity_name
            + __content.string(12802)
            + &str_s(cast::<Ext, _>(pct_to_buy))
            + __content.string(2163)
            + &g.ent_name[corp_id]
            + __content.string(12803);
        if sf(0.1f32) < realized_gain.abs() {
            let mut kind = s("gain");
            if realized_gain < Ext::from_f64(0.0) {
                kind = s("loss");
            }
            msg_text = msg_text
                + "\r"
                + "\r"
                + "A "
                + &kind
                + " of "
                + &trim(g.currency_symbol.as_bytes())
                + &fmt_trim(realized_gain.abs(), "* ###,.00")
                + " "
                + &g.million_prefix_lc
                + "illion"
                + &g.currency_suffix
                + __content.string(12804);
        }
        ui.msgbox(&msg_text, 8192, &s(__content.string(12805)));
        g.news_msg = g.active_entity_name.clone()
            + __content.string(12806)
            + &str_s(cast::<Ext, _>(pct_to_buy))
            + __content.string(2163)
            + &g.ent_symbol[corp_id]
            + __content.string(12807);
        post_news(g, ui, 0, 0);
    }
}
