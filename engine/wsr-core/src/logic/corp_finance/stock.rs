//! Stock offerings, buybacks (public LBO / greenmail), extraordinary dividends, restructuring
//! write-offs and contingency-reserve adjustments (player menu actions of the active entity).
//!
//! Calls into routines of other modules that are not ported yet are marked `PORT-TODO` and skipped:
//! `GetSelectedPlayerEtf` (0x652b17), `InBox` (0x423170, symbol-entry mode), `LookupSymbol` (0x422b3b),
//! `LboTenderOffer` (0x51f21c), `AddRecentOffering` (0x498ad8). Where the result of
//! `GetSelectedPlayerEtf` / `LookupSymbol` decides the flow, the routine is split into a public wrapper
//! and a public `*_with_*` function taking that result, so the wiring is a one-liner.
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::prelude::*;
use crate::logic::control::{find_option_positions, update_corp_control};
use crate::logic::earnings::{balance_cash_t_bills, calc_projected_earnings, sell_t_bills_to_cash};
use crate::logic::finance::{calc_entity_financials, get_controller_type};
use crate::logic::holdings::find_holding_slot;
use crate::logic::newgame::LocalTime;
use crate::logic::options::get_plr_naked_liab;
use crate::logic::ai_player::buyer::add_recent_offering;
use crate::logic::restructure::lbo_tender_offer;
use crate::logic::trading::select::{get_selected_player_etf, in_box_symbol, lookup_symbol};
use crate::logic::report::{number_to_words, show_quick_message, show_shareholder_list};
use crate::logic::sim_entity::ai_corp::is_recent_offering;
use crate::logic::sim_entity::asset_sales::calc_industry_unmet_demand;
use crate::logic::sim_entity::bonds::{adjust_conversion_price, adjust_price_alerts_for_stock, issue_stock, normalize_holding_pcts};
use crate::logic::sim_entity::dividends::{
    adjust_options_for_corp_action, adjust_price_for_distribution, charge_shorts_for_distribution,
};
use crate::logic::sim_entity::quarter_start::cancel_pending_merger;
use crate::logic::turn::{run_scenario_event, ticker_sim_steps};
use crate::logic::valuation::{
    adjust_index_divisor, calc_stock_price, check_stock_split, price_pct_change_months, update_price_history_tick,
};
use crate::pb::string::{CaseMode, lcase, mcase, remove, remove_any};

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

/// `CHR$(34)`.
fn quote() -> PbStr {
    s("\"")
}

/// `RTRIM$(LTRIM$(CurrencySymbol))`.
fn cur_sym(g: &Globals) -> PbStr {
    trim(g.currency_symbol.as_bytes())
}

/// `VAL(REMOVE$(text, ANY chars))`.
fn val_removed(text: &PbStr, chars: &[u8]) -> Ext {
    val(remove_any(text.as_bytes(), chars).as_bytes())
}

// ------------------------------------------------------------------------------------------------
// AdjustContingencyReserve
// ------------------------------------------------------------------------------------------------

/// Port of AdjustContingencyReserve (0x59fcdd).
/// Earnings management through the active entity's contingency ("slush fund") reserve: `adjust_mode` 6
/// adds to the reserve (max 1% of the loan book for a bank, else 10% of a projected loss / 5% of the
/// projected profit), any other mode (7) draws it down (max the reserve balance). An explanation box is
/// shown, a pending adjustment is canceled, and the amount entered is stored in `ReserveAdjPending`
/// (negative for a draw-down; applied at [runtime resource wording] ). Cancel / <= 0 leaves it at 0.
pub fn adjust_contingency_reserve(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, adjust_mode: i32) {
    let __content = crate::content::current();
    let num_format: &[u8] = b"#################.00";
    let mut max_addition: Ext = Ext::from_f64(0.0);
    let ae = g.active_entity;
    if adjust_mode == 7 && 10 < ae && g.contingency_reserve[ae] < sf(0.005f32) {
        let mut msg = s(__content.string(4070))
            + __content.string(4071)
            + __content.string(4072)
            + "\r\n"
            + "\r\n"
            + __content.string(4025)
            + &g.active_entity_name
            + __content.string(4073)
            + __content.string(4074);
        show_quick_message(g, ui, &mut msg);
        return;
    }
    let mut msg = s(__content.string(4075))
        + &g.active_entity_name
        + __content.string(4076)
        + __content.string(4077)
        + __content.string(4078)
        + __content.string(4079)
        + __content.string(4080)
        + __content.string(4081)
        + __content.string(4082)
        + __content.string(4083)
        + "\r"
        + "\r";
    let mut reserve_text = s("zero.");
    if sf(0.0049f32) < g.contingency_reserve[g.active_entity] {
        let r = g.contingency_reserve[g.active_entity];
        reserve_text = cur_sym(g) + &fmt_trim(r, num_format) + " " + &g.million_prefix_lc + "illion.";
    }
    if 1 < g.industry_num[g.active_entity] {
        msg = msg
            + __content.string(4084)
            + &quote()
            + __content.string(4085)
            + &quote()
            + __content.string(4086)
            + &g.ent_symbol[g.active_entity]
            + " is "
            + &reserve_text
            + "\r"
            + "\r";
    }
    if adjust_mode == 6 {
        if g.active_entity < 86 {
            msg = msg + __content.string(4087);
        } else {
            msg = msg + __content.string(4088);
        }
        if g.active_entity < 86 {
            let ae = g.active_entity;
            calc_entity_financials(g, ui, rng, ae);
            max_addition = g.tmp_bank_total_loans * sf(0.01f32);
            msg = msg
                + __content.string(4089)
                + &g.active_entity_name
                + " (max. "
                + &cur_sym(g)
                + &fmt_trim(max_addition, num_format)
                + " "
                + &g.million_prefix_lc
                + "il.)";
        } else {
            let proj = g.proj_qtr_earnings[g.active_entity];
            if proj < Ext::from_f64(0.0) {
                max_addition = (proj * sf(0.1f32)).abs();
                msg = msg
                    + __content.string(4090)
                    + &cur_sym(g)
                    + &fmt_trim(max_addition, num_format)
                    + " "
                    + &g.million_prefix_lc
                    + "il.)";
            } else {
                max_addition = proj * sf(0.05f32);
                msg = msg
                    + __content.string(4091)
                    + &cur_sym(g)
                    + &fmt_trim(max_addition, num_format)
                    + " "
                    + &g.million_prefix_lc
                    + "il.)";
            }
        }
        msg = msg + "\r" + "\r";
        msg = msg
            + __content.string(4092)
            + __content.string(4093)
            + __content.string(4094)
            + __content.string(4095)
            + "\r"
            + "\r";
    } else {
        msg = msg
            + __content.string(4096)
            + __content.string(4097)
            + __content.string(4098)
            + __content.string(4099)
            + "\r"
            + "\r";
    }
    let max_add_text = fmt_trim(max_addition, num_format);
    let max_draw_text = fmt_trim(g.contingency_reserve[g.active_entity], num_format);
    msg = msg + __content.string(4100);
    if g.reserve_adj_pending[g.active_entity] != Ext::from_f64(0.0) {
        let mut m = s(__content.string(4101));
        show_quick_message(g, ui, &mut m);
    }
    let ae = g.active_entity;
    g.reserve_adj_pending[ae] = Ext::from_f64(0.0);
    ui.msgbox(&msg, 8256, &s(__content.string(4102)));
    if adjust_mode == 6 {
        let mut reserve_kind = s(__content.string(4103));
        if g.industry_num[g.active_entity] == 1 {
            reserve_kind = s(__content.string(4104));
        }
        msg = s(__content.string(4105))
            + &reserve_kind
            + __content.string(4106)
            + "(Max = "
            + &cur_sym(g)
            + &max_add_text
            + " "
            + &g.million_prefix_lc
            + "illion)";
        let amount = loop {
            let title = s(__content.string(4107))
                + &mcase(reserve_kind.as_bytes(), CaseMode::Ansi)
                + __content.string(4108)
                + &g.ent_symbol[g.active_entity];
            let input = ui.inputbox(&msg, &title, &max_add_text);
            let amount = val_removed(&input, b"$ ,");
            if amount <= Ext::from_f64(0.0) {
                return;
            }
            if !(val(max_add_text.as_bytes()) + sf(0.005f32) < amount) {
                break amount;
            }
        };
        let ae = g.active_entity;
        g.reserve_adj_pending[ae] = amount;
    } else {
        msg = s(__content.string(4109))
            + __content.string(4110)
            + &cur_sym(g)
            + &max_draw_text
            + " "
            + &g.million_prefix_lc
            + "illion)";
        let amount = loop {
            let input = ui.inputbox(&msg, &s(__content.string(4111)), &max_draw_text);
            let amount = val_removed(&input, b"$ ,");
            if amount <= Ext::from_f64(0.0) {
                return;
            }
            if !(val(max_draw_text.as_bytes()) + sf(0.005f32) < amount) {
                break amount;
            }
        };
        let ae = g.active_entity;
        g.reserve_adj_pending[ae] = -amount;
    }
    let mut m = g.active_entity_name.clone()
        + __content.string(4112);
    show_quick_message(g, ui, &mut m);
    g.turn_action_count = g.turn_action_count + 1;
}

// ------------------------------------------------------------------------------------------------
// RestructuringWriteOff
// ------------------------------------------------------------------------------------------------

/// Port of RestructuringWriteOff (0x5aa186).
/// The active operating company (industry 3..69, credit rating >= 5, business assets >= 200) writes off
/// up to `min(40% of net worth, 50% of business assets)` [runtime resource wording] asked as a percentage;
/// a proportional share of goodwill is charged to extraordinary P&L). A small write-off (<= 5% or below
/// a random 18..33% of the maximum) is an operating expense, a larger one an extraordinary item. Above
/// 4% the profit margin improves by `0.45 * pct + 2` (max 30). Can trigger the cheat-mode scenario 102.
pub fn restructuring_write_off(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime) {
    let __content = crate::content::current();
    let mut goodwill_write_off: Ext = Ext::from_f64(0.0);
    let mut is_extraordinary = 0;
    let ae = g.active_entity;
    calc_entity_financials(g, ui, rng, ae);
    let i = g.active_entity;
    let mut allowed = 1;
    if g.industry_num[g.active_entity] < 3 || 69 < g.industry_num[i] || g.credit_rating_num[i] < 5 {
        allowed = -1;
    }
    if allowed == 1 && g.business_assets[i] < Ext::from_f64(200.0) {
        allowed = 0;
    }
    if allowed < 1 {
        let msg = g.active_entity_name.clone()
            + __content.string(4113)
            + __content.string(4114);
        let mut input = s(__content.string(4115));
        if allowed == 0 {
            input = s(__content.string(4116));
        }
        ui.msgbox(&(msg + &input), 8256, &s(__content.string(1962)));
        return;
    }
    let mut max_pct: i64 = 0;
    let mut max_write_off = g.net_worth * sf(0.4f32);
    let half_assets = g.business_assets[i] * Ext::from_f64(0.5);
    if half_assets <= max_write_off {
        max_pct = to_quad(Ext::from_f64(50.0));
        max_write_off = half_assets;
    }
    if max_write_off < half_assets {
        max_pct = to_quad((max_write_off / g.business_assets[i]) * Ext::from_f64(100.0));
    }
    let mut msg = s(__content.string(4117))
        + &g.active_entity_name
        + __content.string(4118)
        + __content.string(4119)
        + "\r\n"
        + "\r\n"
        + __content.string(4120)
        + &str_s(cast::<Ext, _>(max_pct))
        + "%]";
    let pct = loop {
        let title = g.active_entity_name.clone() + __content.string(4121);
        let input = ui.inputbox(&msg, &title, &trim(str_s(cast::<Ext, _>(max_pct)).as_bytes()));
        let pct = to_quad(val_removed(&input, b"$ ,%"));
        if pct <= 0 {
            return;
        }
        if !(max_pct < pct) {
            break pct;
        }
    };
    let i16_i = cast::<i16, _>(i);
    if g.adv_restruct_corp == 0 && g.scenario_num == 0 && g.event_status[102] == 0 && Ext::from_f64(14.0) < rng.rnd() * Ext::from_f64(15.0) {
        if g.cheat_mode == "Y" {
            g.adv_restruct_corp = i16_i;
        }
    }
    if cast::<i32, _>(g.adv_restruct_corp) == i && g.event_status[102] == 0 && g.cheat_mode == "Y" {
        g.scenario_num = 102;
        g.scenario_corp = i16_i;
        run_scenario_event(g, ui, rng, now);
        if cast::<i32, _>(g.adv_restruct_corp) == -i {
            g.adv_restruct_corp = i16_i;
            g.event_status[102] = 0;
            let mut m = g.active_entity_name.clone()
                + __content.string(4122);
            show_quick_message(g, ui, &mut m);
            return;
        }
    }
    let x = sf(0.01f32) * cast::<Ext, _>(pct) * g.business_assets[i];
    g.business_assets[i] = g.business_assets[i] - x;
    if 130 < i && Ext::from_f64(10.0) < g.goodwill[i] {
        goodwill_write_off = sf(0.01f32) * cast::<Ext, _>(pct) * g.goodwill[i];
        g.goodwill[i] = g.goodwill[i] - goodwill_write_off;
        g.extraordinary_pl[i] = to_quad(cast::<Ext, _>(g.extraordinary_pl[i]) - goodwill_write_off);
    }
    let threshold = cast::<Ext, _>(max_pct) * (sf(0.15f32) * rng.rnd() + sf(0.18f32));
    if (cast::<Ext, _>(pct)) <= threshold || pct <= 5 {
        g.accrued_pl[i] = g.accrued_pl[i] - x;
    } else {
        g.extraordinary_pl[i] = to_quad(cast::<Ext, _>(g.extraordinary_pl[i]) - x);
        is_extraordinary = 1;
    }
    if 4 < pct {
        let new_margin = to_quad(cast::<Ext, _>(pct) * sf(0.45f32) + cast::<Ext, _>(g.profit_margin[i]));
        g.profit_margin[i] = cast::<i16, _>(new_margin);
        g.profit_margin[i] = g.profit_margin[i].wrapping_add(2);
        if 30 < g.profit_margin[i] {
            g.profit_margin[i] = 30;
        }
    }
    msg = s(__content.string(4123));
    if is_extraordinary == 1 {
        msg = msg
            + "\r\n"
            + "\r\n"
            + &g.ent_symbol[i]
            + __content.string(4124)
            + __content.string(4125)
            + __content.string(4126);
    } else {
        msg = msg + "\r\n" + "\r\n" + &g.ent_symbol[i] + __content.string(4127) + "expense";
        if goodwill_write_off <= Ext::from_f64(0.0) {
            msg = msg + ".";
        } else {
            msg = msg + __content.string(4128);
        }
    }
    show_quick_message(g, ui, &mut msg);
    g.news_msg = g.ent_symbol[i].clone()
        + __content.string(4129)
        + &g.currency_symbol
        + &fmt_trim(x, b"############")
        + " "
        + &g.million_prefix_lc
        + "il."
        + &g.currency_suffix;
    post_news(g, ui, 0, 0);
    let ae = g.active_entity;
    calc_projected_earnings(g, ui, rng, ae);
    g.turn_action_count = g.turn_action_count + 1;
    if cast::<i32, _>(g.event_player) == g.cur_player
        && 0 < g.scenario_num
        && g.scenario_num != 102
        && g.event_status[cast::<i32, _>(g.scenario_num)] == 1
        && 0 < g.scenario_stage
    {
        g.scenario_stage = g.scenario_stage + 10;
    }
    if g.event_status[102] == 2 && Ext::from_f64(2.0) < rng.rnd() * Ext::from_f64(3.0) {
        g.event_status[102] = 0;
    }
}

// ------------------------------------------------------------------------------------------------
// StockBuybackMenu / GreenmailBuyback
// ------------------------------------------------------------------------------------------------

/// Port of StockBuybackMenu (0x59e01f).
/// Stock buyback by the active company: `buyback_mode` 1 = buy back a private holder's block above market
/// (greenmail: shareholder pick list, then [`greenmail_buyback`]), 2 = buy back public shares (LBO
/// tender, percentage asked, then `LboTenderOffer`). Banks/insurers rated below 8 are stopped by the
/// regulators (news); a negative net worth, no non-player private holder (mode 1) or no public float
/// (mode 2) cancel with a pop-up.
pub fn stock_buyback_menu(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime, buyback_mode: i32) {
    let __content = crate::content::current();
    let mut msg = PbStr::new();
    let mut private_holder = 0;
    let ae = g.active_entity;
    calc_entity_financials(g, ui, rng, ae);
    if g.industry_num[g.active_entity] < 3 && g.credit_rating_num[g.active_entity] < 8 {
        msg = s("BANKING");
        if g.industry_num[g.active_entity] == 2 {
            msg = s("INSURANCE");
        }
        let mut input = s(" IN ");
        let country_idx = cast::<Ext, _>(g.country_idx[g.active_entity]);
        if country_idx == Ext::from_f64(0.0) || country_idx == Ext::from_f64(11.0) || country_idx == Ext::from_f64(30.0) || country_idx == Ext::from_f64(35.0) || country_idx == Ext::from_f64(50.0) {
            input = input + "THE ";
        }
        input = input + &g.country_name[g.country_idx[g.active_entity]];
        let text = s(__content.string(4130))
            + &msg
            + __content.string(4131)
            + &input
            + " WILL "
            + __content.string(4132)
            + &g.ent_name[g.active_entity]
            + __content.string(4133)
            + __content.string(4134);
        ui.msgbox(&text, 8208, &s(__content.string(4135)));
        g.news_msg = mcase(msg.as_bytes(), CaseMode::Ansi)
            + __content.string(4136)
            + &g.ent_symbol[g.active_entity];
        post_news(g, ui, 0, 0);
        return;
    }
    let mut holder_id = 11;
    loop {
        if g.entity_status[holder_id] == 1 {
            let mut slot = 1;
            loop {
                if cast::<i32, _>(g.hold_stock_id[(slot, holder_id)]) == g.active_entity && 0 < g.hold_stock_pct[(slot, holder_id)] {
                    private_holder = holder_id;
                    update_corp_control(g, ui, rng, private_holder);
                    if get_controller_type(g, private_holder) == 1 {
                        private_holder = 0;
                    } else {
                        slot = 15;
                        holder_id = 1600;
                    }
                }
                slot += 1;
                if !(slot < 16) {
                    break;
                }
            }
        }
        holder_id += 1;
        if !(holder_id < 1601) {
            break;
        }
    }
    let ae = g.active_entity;
    calc_entity_financials(g, ui, rng, ae);
    if g.net_worth < Ext::from_f64(0.0) {
        msg = g.active_entity_name.clone()
            + __content.string(4137)
            + __content.string(4138);
    }
    update_corp_control(g, ui, rng, ae);
    let public_pct = g.public_pct;
    if private_holder == 0 && buyback_mode == 1 {
        msg = s(__content.string(4139))
            + &g.active_entity_name
            + __content.string(4140);
    }
    if public_pct == 0 && buyback_mode == 2 {
        msg = s(__content.string(4141)) + &g.active_entity_name + __content.string(4142) + __content.string(4143);
    }
    if msg.is_empty() {
        if buyback_mode == 1 {
            g.entity_pick_mode = s("GetCorp");
            g.target_corp = 0;
            show_shareholder_list(g, ui, rng, ae, 0);
            greenmail_buyback(g, ui, rng, now);
            g.target_corp = 0;
            g.lookup_entity_name = PbStr::new();
            g.tmp_symbol = PbStr::new();
        } else {
            let mut input;
            loop {
                g.action_code = s("LBO");
                msg = s(__content.string(4144)) + &g.active_entity_name;
                msg = msg + __content.string(4145) + &g.active_entity_name + __content.string(4146);
                msg = msg + &str_s(cast::<Ext, _>(public_pct)) + __content.string(4147);
                let title = g.ent_name[g.active_entity].clone() + " -- LBO";
                input = ui.inputbox(&msg, &title, &(trim(str_s(cast::<Ext, _>(public_pct)).as_bytes()) + "%"));
                if (cast::<Ext, _>(public_pct)) < val_removed(&input, b"$ ,") {
                    let text = s(__content.string(4148))
                        + &g.active_entity_name
                        + __content.string(4149)
                        + &trim(str_s(cast::<Ext, _>(public_pct)).as_bytes())
                        + __content.string(4150)
                        + "!";
                    let title = g.app_title.clone();
                    ui.msgbox(&text, 8208, &title);
                    g.action_code = PbStr::new();
                }
                if g.action_code == "LBO" {
                    break;
                }
            }
            if Ext::from_f64(0.0) < val(input.as_bytes()) {
                g.tmp_ext1 = Ext::from_f64(0.0);
                g.tmp_ext2 = val(input.as_bytes());
                let ae = g.active_entity;
                lbo_tender_offer(g, ui, rng, ae, 0, now);
            } else {
                let mut m = s(__content.string(4151));
                show_quick_message(g, ui, &mut m);
            }
        }
    } else {
        show_quick_message(g, ui, &mut msg);
    }
}

/// Port of GreenmailBuyback (0x521526).
/// The active company buys back a shareholder's block at a premium. Its BYREF parameter is `TargetCorp`
/// at its only call site ([`stock_buyback_menu`]), so `seller_arg` is read from `g.target_corp`: a
/// company id (picked from the shareholder list) or, below 11, the symbol is asked (`InBox(3)`) and
/// looked up (`LookupSymbol`, which sets `TmpSymbol`, `TargetCorp` and `LookupEntityName`).
pub fn greenmail_buyback(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime) {
    let __content = crate::content::current();
    let mut msg = PbStr::new();
    g.tmp_symbol = PbStr::new();
    let seller_arg = g.target_corp;
    let symbol_text;
    if seller_arg < 11 {
        if g.cur_player == seller_arg {
            ui.msgbox(
                &(s(__content.string(4152)) + __content.string(4153)),
                4112,
                &s(__content.string(4154)),
            );
            return;
        }
        if seller_arg <= g.num_players && 0 < seller_arg {
            let text = g.ent_name[seller_arg].clone() + __content.string(4155);
            let title = g.app_title.clone();
            ui.msgbox(&text, 4112, &title);
            return;
        }
        msg = greenmail_symbol_prompt(g);
        in_box_symbol(g, ui, rng);
        symbol_text = g.symbol_dlg_text.clone();
    } else {
        symbol_text = g.ent_symbol[seller_arg].clone();
    }
    lookup_symbol(g, ui, rng, &symbol_text);
    let seller_arg = g.target_corp; // BYREF: LookupSymbol may have set TargetCorp
    greenmail_after_lookup(g, ui, rng, now, seller_arg, msg);
}

/// The `InputDlg*` setup of [`greenmail_buyback`] before `InBox`; returns `msgText`.
fn greenmail_symbol_prompt(g: &mut Globals) -> PbStr {
    let __content = crate::content::current();
    g.action_code = s("GM");
    g.lookup_entity_name = PbStr::new();
    let msg = greenmail_prompt_text(g);
    g.input_dlg_label = PbStr::new();
    g.input_dlg_prompt = msg.clone();
    g.input_dlg_title = s(__content.string(4156)) + &g.active_entity_name;
    g.lookup_filter = PbStr::new();
    msg
}

/// `msgText` as [`greenmail_symbol_prompt`] leaves it (without its global side effects).
fn greenmail_prompt_text(g: &Globals) -> PbStr {
    let __content = crate::content::current();
    let mut msg = s(__content.string(4157));
    msg = msg + &g.active_entity_name + __content.string(4158);
    msg + __content.string(1946) + &g.active_entity_name + __content.string(4159) + "\r\n" + "\r\n"
}

/// The part of GreenmailBuyback (0x521526) after `LookupSymbol`: `seller_arg` is the value of the BYREF
/// parameter at that point, `msg` the routine's `msgText` (the symbol prompt when it was asked for,
/// else "").
///
/// Unless the lookup found a company other than the active one (else `TickerSimSteps(1)`): a human
/// player or a company controlled by one can't be forced to sell, the current player can't be paid
/// greenmail; otherwise the percentage to buy back is asked and `LboTenderOffer(ActiveEntity, seller)`
/// runs with `TmpExt1 = 0`, `TmpExt2 = pct`.
fn greenmail_after_lookup(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    now: &LocalTime,
    seller_arg: i32,
    mut msg: PbStr,
) {
    let __content = crate::content::current();
    if !g.tmp_symbol.is_empty() && g.active_entity != seller_arg {
        let seller_id = seller_arg;
        let mut seller_pct: i16 = 0;
        let mut willing_to_sell = 1;
        if 0 < seller_id && seller_id <= g.num_players && g.is_computer_player[seller_id] == 0 {
            willing_to_sell = 0;
        }
        if 10 < seller_id && get_controller_type(g, seller_id) == 1 {
            willing_to_sell = 0;
        }
        if willing_to_sell == 0 && seller_id <= g.num_players && 0 < seller_id && g.cur_player != seller_id {
            let text = g.ent_name[seller_id].clone() + __content.string(4160);
            let title = g.app_title.clone();
            ui.msgbox(&text, 4112, &title);
            return;
        }
        update_corp_control(g, ui, rng, seller_id);
        if g.cur_player == g.voting_controller || g.cur_player == seller_id {
            let text = s(__content.string(4161))
                + &g.active_entity_name
                + " "
                + __content.string(4162);
            ui.msgbox(&text, 4112, &s(__content.string(3253)));
        } else if willing_to_sell == 0 {
            if 10 < seller_id && seller_id != g.voting_controller {
                msg = s(__content.string(1125)) + &g.ent_symbol[g.voting_controller] + ")";
            }
            let mut m = s(__content.string(4163))
                + &g.ent_name[seller_id]
                + &msg
                + __content.string(4164)
                + __content.string(4165)
                + &g.active_entity_name
                + __content.string(4166);
            show_quick_message(g, ui, &mut m);
        } else {
            for slot in 1..=15 {
                if cast::<i32, _>(g.hold_stock_id[(slot, seller_id)]) == g.active_entity {
                    seller_pct = g.hold_stock_pct[(slot, seller_id)];
                }
            }
            if seller_pct == 0 {
                let text = s(__content.string(4148))
                    + &g.ent_name[seller_id]
                    + __content.string(4167)
                    + __content.string(4168)
                    + &g.active_entity_name
                    + "!";
                let title = g.app_title.clone();
                ui.msgbox(&text, 4112, &title);
            } else {
                let mut pct_text;
                loop {
                    g.action_code = s("GM");
                    msg = s(__content.string(4144)) + &g.active_entity_name;
                    msg = msg + __content.string(4169) + &g.lookup_entity_name + ".";
                    msg = msg + "\r\n" + "\r\n";
                    msg = msg
                        + &g.ent_symbol[seller_id]
                        + " owns"
                        + &str_s(cast::<Ext, _>(seller_pct))
                        + "% of "
                        + &g.active_entity_name
                        + ".";
                    let title = g.ent_symbol[g.active_entity].clone() + __content.string(4170);
                    pct_text = ui.inputbox(&msg, &title, &(trim(str_s(cast::<Ext, _>(seller_pct)).as_bytes()) + "%"));
                    if (cast::<Ext, _>(seller_pct)) < val_removed(&pct_text, b"$ ,%") {
                        let text = s(__content.string(4148))
                            + &g.ent_name[seller_id]
                            + __content.string(4171)
                            + &trim(str_s(cast::<Ext, _>(seller_pct)).as_bytes())
                            + __content.string(4172)
                            + &g.active_entity_name
                            + "!";
                        let title = g.app_title.clone();
                        ui.msgbox(&text, 8208, &title);
                        g.action_code = PbStr::new();
                    }
                    if g.action_code == "GM" {
                        break;
                    }
                }
                if Ext::from_f64(0.0) < val(pct_text.as_bytes()) {
                    g.tmp_ext1 = Ext::from_f64(0.0);
                    g.tmp_ext2 = val(pct_text.as_bytes());
                    let ae = g.active_entity;
                    lbo_tender_offer(g, ui, rng, ae, seller_id, now);
                    g.action_code = PbStr::new();
                } else {
                    let mut m = s(__content.string(4151));
                    show_quick_message(g, ui, &mut m);
                    g.action_code = PbStr::new();
                }
            }
        }
        return;
    }
    ticker_sim_steps(g, ui, rng, 1, now);
}

// ------------------------------------------------------------------------------------------------
// StockOfferingMenu
// ------------------------------------------------------------------------------------------------

/// Port of StockOfferingMenu (0x5b0f50).
/// `offering_type` 1 = public offering, 2 = private placement. The ETF chosen by
/// `GetSelectedPlayerEtf` may be the issuer instead ([`stock_offering_menu_with_etf`]).
pub fn stock_offering_menu(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime, offering_type: i32) {
    let etf_id = get_selected_player_etf(g, ui, rng);
    stock_offering_menu_with_etf(g, ui, rng, now, offering_type, etf_id);
}

/// StockOfferingMenu (0x5b0f50) with `etf_id` = the result of `GetSelectedPlayerEtf()`.
///
/// An ETF (> 1580) managed by the player may do a public offering once a year in December after an
/// 11-month gain of 15%. Otherwise the active company issues 5..25% new stock: publicly (underwriter
/// checks: recent offering, asbestos/superfund litigation, IPO seasoning, 'D' rating, depressed
/// industry; size limit `MaxStockOfferingAmt`) or privately to the player / a controlled company
/// (`InBox(3)` asks its symbol) or a 'White Knight' found among companies 86..1600 controlled by other
/// companies (5% discount). The issue itself is `IssueStock`; may trigger the cheat-mode scenario 103.
/// `AddRecentOffering(issuer, offeringType)` after an underwritten offering.
pub fn stock_offering_menu_with_etf(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    now: &LocalTime,
    offering_type: i32,
    etf_id: i32,
) {
    let __content = crate::content::current();
    let mut etf_id = etf_id;
    let mut issuer_id = g.active_entity;
    let mut issuer_name = g.active_entity_name.clone();
    let mut msg = PbStr::new();
    let mut reason = PbStr::new();
    let mut prompt_text;
    let mut max_shares_str;
    let mut underwriter = PbStr::new();
    let mut pct_gain: i32 = 0;
    let mut recent_offering: i64;
    let mut liquid_before: i64 = 0;
    let mut buyer_type = 0;
    let mut buyer_id = 0;
    let mut was_private = 0;
    let mut old_pct: i16 = 0;
    let mut investor_id = 0;
    let mut slot = 0;
    let mut deal_ok;

    if 1580 < etf_id {
        if offering_type == 2 {
            msg = g.ent_name[etf_id].clone() + __content.string(4173);
        } else {
            recent_offering = to_quad(cast::<Ext, _>(is_recent_offering(g, etf_id)));
            if g.cur_quarter < 4 || g.sim_co_count < 1001 || recent_offering != 0 {
                msg = g.ent_name[etf_id].clone()
                    + __content.string(4174);
            }
            if recent_offering != 0 {
                msg = msg + &g.ent_name[etf_id] + __content.string(4175);
            }
            pct_gain = price_pct_change_months(g, etf_id, 11);
            if pct_gain < 15 && msg.is_empty() {
                msg = s(__content.string(4176))
                    + &g.ent_name[etf_id]
                    + __content.string(4177)
                    + __content.string(4178);
            }
        }
        if g.active_entity < 11 {
            max_shares_str = PbStr::new();
        } else {
            max_shares_str = s("\r\n")
                + "\r\n"
                + __content.string(4179)
                + &g.active_entity_name
                + __content.string(4180);
        }
        if msg.is_empty() {
            let words = number_to_words(pct_gain, 0, 1);
            msg = s(__content.string(4181))
                + &g.ent_name[etf_id]
                + " ("
                + &g.ent_symbol[etf_id]
                + __content.string(4182)
                + __content.string(4183)
                + __content.string(4184)
                + &g.ent_name[g.fund_adviser[etf_id]]
                + ". "
                + __content.string(4185)
                + &words
                + __content.string(4186)
                + __content.string(4187)
                + __content.string(4188)
                + "\r"
                + "\r"
                + __content.string(3020)
                + &g.ent_name[etf_id]
                + __content.string(4189)
                + "\r"
                + "\r"
                + __content.string(4190);
            if g.active_entity < 11 {
                msg = msg + __content.string(3023);
            } else {
                msg = msg + "have " + &g.active_entity_name + __content.string(4191);
            }
            let btn = ui.msgbox(&msg, 8483, &s(__content.string(4192)));
            if btn == 2 {
                return;
            }
            if btn == 7 {
                if g.active_entity < 11 {
                    return;
                }
                etf_id = 0;
            } else {
                issuer_id = etf_id;
                issuer_name = g.ent_name[etf_id].clone();
            }
        } else {
            let mut m = msg.clone() + &max_shares_str;
            show_quick_message(g, ui, &mut m);
            etf_id = 0;
        }
    }
    if issuer_id < 11 {
        let mut m = s(__content.string(3034)) + &g.active_entity_name + __content.string(4193);
        show_quick_message(g, ui, &mut m);
        return;
    }
    if g.entity_status[issuer_id] != 1 {
        return;
    }
    if offering_type == 2 {
        reason = s(__content.string(1936))
            + &g.active_entity_name
            + __content.string(4194)
            + "("
            + &g.cur_player_name
            + __content.string(4195)
            + "\r"
            + "\r"
            + __content.string(4196);
        msg = s(__content.string(4197));
        let btn = ui.msgbox(&reason, 8483, &msg);
        if btn == 2 {
            return;
        }
        if btn == 6 {
            reason = s(__content.string(1936)) + &g.active_entity_name + __content.string(4198);
            msg = s(__content.string(4199)) + &g.cur_player_name + "?";
            let btn = ui.msgbox(&reason, 8484, &msg);
            let mut related_ok = false;
            if btn != 6 {
                buyer_type = 2;
                reason = s(__content.string(4200))
                    + &g.ent_name[g.active_entity];
                reason = reason + __content.string(4201) + "\r\n" + "\r\n";
                g.input_dlg_label = PbStr::new();
                g.input_dlg_prompt = reason.clone();
                g.input_dlg_title = s(__content.string(4202)) + &g.active_entity_name;
                g.lookup_filter = PbStr::new();
                g.action_code = PbStr::new();
                in_box_symbol(g, ui, rng);
                let cur_player = g.cur_player;
                if g.target_corp < 11 {
                    return;
                }
                buyer_id = g.target_corp;
                if g.active_entity != g.target_corp && 85 < g.target_corp && cur_player == g.corp_controller[g.target_corp] {
                    related_ok = true;
                }
                if !related_ok {
                    if g.active_entity == buyer_id {
                        reason = g.active_entity_name.clone() + __content.string(4203);
                    } else if buyer_id < 86 {
                        reason = s(__content.string(4204));
                    } else {
                        reason = s(__content.string(3312)) + &g.ent_name[buyer_id] + ".";
                    }
                    let mut m = reason.clone() + __content.string(4205);
                    show_quick_message(g, ui, &mut m);
                    return;
                }
            } else {
                buyer_type = 1;
                buyer_id = g.cur_player;
            }
            // LAB_005b1a84
            let ae = g.active_entity;
            update_corp_control(g, ui, rng, ae);
            if g.public_pct == 0 && cast::<i32, _>(g.wholly_owned_by) == buyer_id {
                msg = s("ich");
                if buyer_id < 11 {
                    msg = s("o");
                }
                let mut m = g.active_entity_name.clone()
                    + __content.string(4206)
                    + &g.ent_name[buyer_id]
                    + ", wh"
                    + &msg
                    + __content.string(4207);
                show_quick_message(g, ui, &mut m);
                return;
            }
        }
        reason = PbStr::new();
    }
    if buyer_type < 1 {
        pct_gain = rng.rnd_range(4, 1);
        match pct_gain {
            1 => underwriter = s(__content.string(4208)),
            2 => underwriter = s(__content.string(4209)),
            3 => underwriter = s(__content.string(4210)),
            4 => underwriter = s(__content.string(4211)),
            _ => {}
        }
        recent_offering = to_quad(cast::<Ext, _>(is_recent_offering(g, issuer_id)));
        if recent_offering != 0 {
            reason = s(__content.string(4212)) + &underwriter + ") ";
        }
        if offering_type == 1 {
            if recent_offering == 1 {
                reason = reason
                    + "SAY "
                    + &issuer_name
                    + __content.string(4213)
                    + __content.string(4214);
            } else {
                reason = PbStr::new();
            }
            if cast::<i32, _>(g.superfund_corp) == issuer_id || g.asbestos_corp.abs() == issuer_id {
                msg = s("ASBESTOS");
                if cast::<i32, _>(g.superfund_corp) == issuer_id {
                    msg = s(__content.string(4215));
                }
                reason = s(__content.string(4216))
                    + &underwriter
                    + __content.string(4217)
                    + __content.string(4218)
                    + &issuer_name
                    + __content.string(4219)
                    + __content.string(4220)
                    + &msg
                    + __content.string(4221);
            }
        } else {
            if cast::<i32, _>(g.industry_num[issuer_id]) == g.fad_industry
                && 110 < g.fad_valuation_pct
                && g.fad_valuation_pct < 200
                && g.fad_valuation_pct % 2 == 1
                && 10 < price_pct_change_months(g, issuer_id, 6)
            {
                reason = s(__content.string(4222))
                    + &underwriter
                    + __content.string(4223)
                    + __content.string(4224)
                    + &lcase(g.ind_name[cast::<i32, _>(g.industry_num[issuer_id])].as_bytes(), CaseMode::Ansi)
                    + __content.string(4225)
                    + __content.string(4226)
                    + __content.string(4227);
                recent_offering = 0;
            }
            if recent_offering == -1 {
                reason = reason
                    + __content.string(4228)
                    + __content.string(4229)
                    + &issuer_name
                    + ".";
            } else if recent_offering == 1 {
                reason = reason
                    + __content.string(4230)
                    + __content.string(4231)
                    + __content.string(4232);
            }
        }
        update_corp_control(g, ui, rng, issuer_id);
        if etf_id == 0
            && reason.is_empty()
            && offering_type == 1
            && g.earnings_yr3_ago[issuer_id] == Ext::from_f64(0.0)
            && g.earnings_yr4_ago[issuer_id] == Ext::from_f64(0.0)
            && g.public_pct == 0
        {
            reason = s(__content.string(4233))
                + &underwriter
                + "), say "
                + &issuer_name
                + " "
                + __content.string(4234)
                + __content.string(4235)
                + "\r"
                + "\r"
                + __content.string(4236);
            prompt_text = s(".");
            if g.earnings_yr2_ago[issuer_id] == Ext::from_f64(0.0) {
                prompt_text = s(__content.string(4237));
            }
            reason = reason + &prompt_text;
        }
        calc_entity_financials(g, ui, rng, issuer_id);
        liquid_before = to_quad(g.cash[issuer_id] + g.t_bills[issuer_id]);
        if g.credit_rating_str == "D" {
            if offering_type == 1 {
                reason = s(__content.string(4238))
                    + &mcase(underwriter.as_bytes(), CaseMode::Ansi)
                    + __content.string(4239)
                    + "\r"
                    + "\r"
                    + &quote()
                    + __content.string(4240)
                    + __content.string(4241)
                    + &issuer_name
                    + "."
                    + "\r"
                    + "\r"
                    + &quote()
                    + __content.string(4242)
                    + &quote()
                    + "\r"
                    + "\r"
                    + __content.string(4243)
                    + __content.string(4244)
                    + "\r"
                    + "\r"
                    + __content.string(4245);
            } else {
                reason = s(__content.string(4246))
                    + &issuer_name
                    + " "
                    + __content.string(4247)
                    + __content.string(4248)
                    + __content.string(4249)
                    + &g.ent_symbol[issuer_id]
                    + __content.string(4250)
                    + __content.string(4251);
            }
        }
        if 130 < issuer_id && g.industry_num[issuer_id] < 70 {
            let ind = g.industry_num[issuer_id];
            let unmet_demand = calc_industry_unmet_demand(g, ind);
            prompt_text = parse_any(g.ent_misc_data[issuer_id].as_bytes(), b"|", 30);
            let demand_trend = to_long(val(prompt_text.as_bytes()));
            if unmet_demand == 0
                && g.total_assets * sf(0.4f32) < g.business_assets[issuer_id]
                && demand_trend < -2
                && g.ind_demand_gap[cast::<i32, _>(g.industry_num[issuer_id])] < Ext::from_f64(6.0)
            {
                reason = s(__content.string(4252))
                    + &mcase(underwriter.as_bytes(), CaseMode::Ansi)
                    + __content.string(4253)
                    + __content.string(4254)
                    + &g.ind_name[cast::<i32, _>(g.industry_num[issuer_id])]
                    + __content.string(4255)
                    + __content.string(4256)
                    + __content.string(4257);
            }
        }
        if !reason.is_empty() {
            ui.msgbox(&reason, 8208, &s(__content.string(4258)));
            return;
        }
    }
    let shares_out = g.shares_outstanding[issuer_id] * Ext::from_f64(100.0);
    reason = trim(&format(shares_out, b"* ############,.00")) + " ";
    if 0 < instr(1, reason.as_bytes(), b".00") {
        reason = remove(reason.as_bytes(), b".00");
    }
    let min_shares = shares_out * sf(0.05f32);
    msg = trim(&format(min_shares, b"* ###########,.00"));
    if 0 < instr(1, msg.as_bytes(), b".00") {
        msg = remove(msg.as_bytes(), b".00");
    }
    max_shares_str = trim(&format(min_shares * Ext::from_f64(5.0), b"* ##########,.00"));
    if 0 < instr(1, max_shares_str.as_bytes(), b".00") {
        max_shares_str = remove(max_shares_str.as_bytes(), b".00");
    }
    msg = msg + " to " + &max_shares_str;
    let mut shares_to_sell: Ext = Ext::from_f64(0.0);
    while shares_to_sell < min_shares - sf(0.01f32) || min_shares * Ext::from_f64(5.0) < shares_to_sell - sf(0.01f32) {
        prompt_text = issuer_name.clone()
            + " has "
            + &reason
            + &g.million_prefix_lc
            + __content.string(4259)
            + __content.string(2508)
            + &g.million_prefix_lc
            + __content.string(4260)
            + __content.string(4261)
            + &issuer_name
            + __content.string(4262);
        if offering_type == 1 {
            prompt_text = prompt_text + __content.string(4263);
        } else if buyer_type == 0 {
            let mut investor_label = s(__content.string(4264));
            update_corp_control(g, ui, rng, issuer_id);
            if g.earnings_yr3_ago[issuer_id] == Ext::from_f64(0.0) && g.earnings_yr4_ago[issuer_id] == Ext::from_f64(0.0) && 0 < g.wholly_owned_by {
                investor_label = s(__content.string(4265));
            }
            prompt_text = prompt_text + "to a " + &investor_label + __content.string(4266);
        } else {
            let mut investor_label = s("you");
            if buyer_type == 2 {
                investor_label = g.ent_name[buyer_id].clone();
            }
            prompt_text = prompt_text + "to " + &investor_label + "? ";
        }
        prompt_text = prompt_text + "[" + &msg + " " + &g.million_prefix_lc + __content.string(4267);
        let title = s(__content.string(4268)) + &g.ent_symbol[issuer_id];
        let default = max_shares_str.clone() + " " + &g.million_prefix_lc + "il.";
        let answer = ui.inputbox(&prompt_text, &title, &default);
        shares_to_sell = val_removed(&answer, b"$ ,%");
        if shares_to_sell <= Ext::from_f64(0.0) {
            return;
        }
        if shares_to_sell < min_shares - sf(0.01f32) || min_shares * Ext::from_f64(5.0) < shares_to_sell - sf(0.01f32) {
            let mut m = s(__content.string(4269)) + &msg + " " + &g.million_prefix_lc + __content.string(4270);
            show_quick_message(g, ui, &mut m);
        }
    }
    let pct_raw: Ext = (shares_to_sell / shares_out) * Ext::from_f64(100.0);
    let pct_issued = to_integer(pct_raw).clamp(5, 25);
    if offering_type == 1 {
        deal_ok = 1;
        if !(g.market_cap[issuer_id] * cast::<Ext, _>(pct_issued) <= cast::<Ext, _>(g.max_stock_offering_amt)) {
            let text = s(__content.string(4271))
                + &underwriter
                + ", say "
                + __content.string(4272)
                + __content.string(4273)
                + "\r"
                + "\r"
                + __content.string(4274);
            ui.msgbox(&text, 8256, &s(__content.string(4258)));
            return;
        }
    } else {
        update_corp_control(g, ui, rng, issuer_id);
        if g.public_pct == 0 {
            was_private = 1;
        }
        let mut deal_cost = g.market_cap[issuer_id] * cast::<Ext, _>(pct_issued);
        if buyer_type == 0 {
            deal_cost = deal_cost * sf(0.95f32);
        }
        deal_ok = 0;
        if buyer_type < 1 {
            recent_offering = to_quad(cast::<Ext, _>(rng.rnd_range(985, 86)));
            let mut candidate_id = to_long(cast::<Ext, _>(recent_offering));
            while candidate_id <= 1600 {
                if g.entity_status[candidate_id] == 1
                    && 11 <= g.corp_controller[candidate_id]
                    && g.industry_num[candidate_id] != 71
                {
                    g.liquid_funds = Ext::from_f64(0.0);
                    slot = find_holding_slot(g, candidate_id, issuer_id);
                    if !(cast::<i32, _>(g.hold_stock_id[(slot, candidate_id)]) == issuer_id || slot == 0) {
                        calc_entity_financials(g, ui, rng, candidate_id);
                        if deal_cost <= g.liquid_funds {
                            investor_id = candidate_id;
                            deal_ok = 1;
                            candidate_id = 1600;
                        }
                    }
                }
                candidate_id += 1;
            }
        } else {
            investor_id = buyer_id;
            deal_ok = 1;
            slot = find_holding_slot(g, investor_id, issuer_id);
            if slot == 0 {
                deal_ok = -1;
            } else if cast::<i32, _>(g.hold_stock_id[(slot, investor_id)]) == issuer_id && g.hold_stock_pct[(slot, investor_id)] < 0 {
                deal_ok = -2;
            }
            calc_entity_financials(g, ui, rng, investor_id);
            if g.liquid_funds < deal_cost {
                deal_ok = 0;
            }
            if !(0 < deal_ok) {
                reason = s(__content.string(4275)) + &g.ent_name[investor_id];
                if deal_ok == 0 {
                    reason = reason + __content.string(4276);
                } else if deal_ok == -1 {
                    reason = reason + __content.string(4277);
                } else {
                    reason = reason + __content.string(4278) + &issuer_name + " stock.";
                }
                show_quick_message(g, ui, &mut reason);
                return;
            }
        }
        // LAB_005b3408
        if deal_ok == 1 {
            g.cash[investor_id] = g.cash[investor_id] - deal_cost;
            balance_cash_t_bills(g, investor_id);
            if g.cash[investor_id] < Ext::from_f64(0.0) {
                let overdraft = g.cash[investor_id].abs();
                g.bank_debt[investor_id] = g.bank_debt[investor_id] + overdraft;
                g.cash[investor_id] = Ext::from_f64(0.0);
                let lender = g.lender_id;
                g.cash[lender] = g.cash[lender] - overdraft;
            }
            g.hold_stock_id[(slot, investor_id)] = cast::<i16, _>(issuer_id);
            old_pct = g.hold_stock_pct[(slot, investor_id)];
            g.hold_stock_pct[(slot, investor_id)] = pct_issued.wrapping_add(g.hold_stock_pct[(slot, investor_id)]);
            g.hold_stock_basis[(slot, investor_id)] = g.hold_stock_basis[(slot, investor_id)] + deal_cost;
        } else {
            reason = s(__content.string(4279))
                + "\r"
                + "\r"
                + __content.string(4280)
                + __content.string(4281)
                + &underwriter
                + __content.string(4282)
                + __content.string(4283)
                + __content.string(4284)
                + __content.string(4285)
                + &issuer_name
                + ".";
            ui.msgbox(&reason, 8256, &s(__content.string(4286)));
        }
    }
    // LAB_005b3725
    if deal_ok == 0 {
        return;
    }
    if offering_type == 1 && etf_id == 0 {
        if g.adv_offering_corp == 0
            && g.event_status[103] == 0
            && 1 < g.difficulty_level
            && g.scenario_num == 0
            && Ext::from_f64(14.0) < rng.rnd() * Ext::from_f64(15.0)
            && g.cheat_mode == "Y"
        {
            g.adv_offering_corp = cast::<i16, _>(issuer_id);
        }
        if cast::<i32, _>(g.adv_offering_corp) == issuer_id && g.event_status[103] == 0 && g.cheat_mode == "Y" {
            g.scenario_corp = cast::<i16, _>(issuer_id);
            g.scenario_num = 103;
            run_scenario_event(g, ui, rng, now);
            if cast::<i32, _>(g.adv_offering_corp) == -issuer_id {
                g.adv_offering_corp = cast::<i16, _>(issuer_id);
                g.event_status[103] = 0;
                let text = issuer_name.clone() + __content.string(4287) + __content.string(4288);
                ui.msgbox(&text, 8256, &s(__content.string(4289)));
                return;
            }
        }
    }
    g.tmp_ext1 = cast::<Ext, _>(pct_issued);
    issue_stock(g, ui, rng, issuer_id, offering_type);
    if 1580 < etf_id {
        post_news(g, ui, issuer_id, 0);
        if 0 < g.fund_bonus_year[issuer_id] {
            g.fund_bonus_year[issuer_id] = g.cur_year;
        }
        let raised = (g.cash[issuer_id] + g.t_bills[issuer_id]) - cast::<Ext, _>(liquid_before);
        reason = s(__content.string(4290))
            + &g.ent_symbol[issuer_id]
            + __content.string(4291)
            + &cur_sym(g)
            + &trim(&format(raised, b"* ###,"))
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix
            + __content.string(4292);
        ui.msgbox(&reason, 8256, &s(__content.string(4293)));
    }
    if offering_type == 2 {
        normalize_holding_pcts(g, ui, rng, issuer_id, was_private);
    }
    g.turn_action_count = g.turn_action_count + 1;
    if buyer_type == 0 {
        add_recent_offering(g, issuer_id, offering_type);
    }
    if !g.news_msg.is_empty() {
        reason = PbStr::new();
        let now_pct = g.hold_stock_pct[(slot, investor_id)];
        if 3 < cast::<i32, _>(now_pct) - cast::<i32, _>(old_pct) {
            reason = str_s(cast::<Ext, _>(now_pct) - cast::<Ext, _>(old_pct)) + __content.string(4294);
        }
        if offering_type == 2 {
            g.news_msg = g.ent_symbol[issuer_id].clone()
                + " issues"
                + &reason
                + __content.string(4295)
                + &g.ent_symbol[investor_id]
                + __content.string(4296);
        }
        let text = g.news_msg.clone();
        ui.msgbox(&text, 8256, &s(__content.string(4293)));
        post_news(g, ui, 0, 0);
    }
}

// ------------------------------------------------------------------------------------------------
// ExtraordinaryDividend
// ------------------------------------------------------------------------------------------------

/// Port of ExtraordinaryDividend_Exit (0x5b658f): the routine's common exit label (no state effect).
pub fn extraordinary_dividend_exit() {}

/// Port of CancelMergerOnViolation (0x5b63a3), a GOSUB of ExtraordinaryDividend that only uses globals.
/// The acquirer of the pending merger with the active company cancels its offer (pop-up, news,
/// `CancelPendingMerger(0)`).
pub fn cancel_merger_on_violation(g: &mut Globals, ui: &mut dyn Ui) {
    let __content = crate::content::current();
    if 10 < g.active_entity && 10 < g.merger_target && 10 < g.merger_acquirer {
        let mut m = g.active_entity_name.clone()
            + __content.string(2845)
            + &g.ent_name[g.merger_acquirer]
            + ". "
            + __content.string(1997)
            + &g.ent_name[g.merger_acquirer]
            + __content.string(2846);
        show_quick_message(g, ui, &mut m);
        g.news_msg = g.ent_symbol[g.merger_acquirer].clone() + __content.string(2847) + &g.ent_symbol[g.active_entity];
        post_news(g, ui, 0, 0);
        cancel_pending_merger(g, ui, 0);
    }
}

/// Port of ExtraordinaryDividend (0x5b3dae).
/// The ETF chosen by `GetSelectedPlayerEtf` may pay instead ([`extraordinary_dividend_with_etf`]).
pub fn extraordinary_dividend(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime) {
    let etf_id = get_selected_player_etf(g, ui, rng);
    extraordinary_dividend_with_etf(g, ui, rng, now, etf_id);
}

/// ExtraordinaryDividend (0x5b3dae) with `etf_id` = the result of `GetSelectedPlayerEtf()`.
///
/// A special cash distribution by the active company (or a debt-free ETF the player's adviser manages):
/// regulators/credit checks, a maximum from net worth, debt, cash, credit line and liquid funds, the
/// amount asked; a large or repeated distribution is a taxable partial liquidation (except to an 80%
/// corporate parent). Holders are paid pro rata (dividends received / extraordinary P&L / basis
/// reduction), news per holder, then the price, options, index divisor, convertible and alert
/// adjustments; violating a pending merger as its target cancels it.
pub fn extraordinary_dividend_with_etf(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime, etf_id: i32) {
    let __content = crate::content::current();
    let mut etf_id = etf_id;
    let mut payer_id = g.active_entity;
    let mut payer_name = g.active_entity_name.clone();
    let mut prompt_text = PbStr::new();
    let mut msg = PbStr::new();
    let mut regulatory_max: Ext = Ext::from_f64(0.0);
    let mut is_taxable = 0;
    let mut answer;
    if 1580 < etf_id {
        prompt_text = g.ent_name[etf_id].clone()
            + " ("
            + &g.ent_symbol[etf_id]
            + __content.string(3017)
            + __content.string(3018)
            + &g.ent_name[g.fund_adviser[etf_id]]
            + ". "
            + __content.string(4297)
            + __content.string(4298)
            + __content.string(4299)
            + "\r"
            + "\r"
            + __content.string(3020)
            + &g.ent_name[etf_id]
            + __content.string(4300)
            + "\r"
            + "\r"
            + __content.string(3022);
        if g.active_entity < 11 {
            prompt_text = prompt_text + __content.string(3152);
        } else {
            prompt_text = prompt_text
                + __content.string(3024)
                + &g.active_entity_name
                + __content.string(4301);
        }
        if g.cash[etf_id] < Ext::from_f64(0.0) {
            balance_cash_t_bills(g, etf_id);
        }
        if sf(0.005f32) < g.bank_debt[etf_id] || 0 < g.bonds_issued[etf_id] || g.cash[etf_id] + g.t_bills[etf_id] < Ext::from_f64(0.0) {
            prompt_text = g.ent_name[etf_id].clone()
                + __content.string(4302)
                + __content.string(4303);
            if 10 < g.active_entity {
                prompt_text = prompt_text
                    + "\r\n"
                    + "\r\n"
                    + __content.string(4304)
                    + &g.active_entity_name
                    + "....";
            }
            show_quick_message(g, ui, &mut prompt_text);
            answer = 7;
        } else {
            answer = ui.msgbox(&prompt_text, 8483, &s(__content.string(4305)));
        }
        if answer == 2 {
            return extraordinary_dividend_exit();
        }
        if answer == 7 {
            if g.active_entity < 11 {
                return extraordinary_dividend_exit();
            }
            etf_id = 0;
        } else {
            payer_id = etf_id;
            payer_name = g.ent_name[etf_id].clone();
        }
    }
    if etf_id == 0 && g.merger_target == g.active_entity {
        msg = g.ent_name[g.active_entity].clone()
            + __content.string(2904)
            + "by "
            + &g.ent_name[g.merger_acquirer]
            + "."
            + "\r"
            + "\r"
            + __content.string(4306)
            + __content.string(2907)
            + "\r"
            + "\r"
            + __content.string(4307);
        answer = ui.msgbox(&msg, 8227, &s(__content.string(2909)));
        if answer != 6 {
            return extraordinary_dividend_exit();
        }
        msg = PbStr::new();
    }
    calc_entity_financials(g, ui, rng, payer_id);
    if g.industry_num[payer_id] < 3 && 7 < g.credit_rating_num[payer_id] {
        regulatory_max = g.net_worth * sf(0.1f32);
        if g.liquid_funds < g.net_worth * sf(0.1f32) {
            regulatory_max = Ext::from_f64(0.0);
        }
        prompt_text = s(__content.string(2878));
        if g.industry_num[payer_id] == 1 {
            prompt_text = s("Banking");
        }
        if g.last_extra_div_corp == payer_id || g.prev_extra_div_corp == payer_id {
            msg = s("The ")
                + &prompt_text
                + __content.string(4308)
                + &g.ent_symbol[payer_id]
                + __content.string(4309)
                + __content.string(4310);
        }
    }
    let mut skip_to_check = false;
    if g.industry_num[payer_id] < 3 && Ext::from_f64(1.0) <= g.liquid_funds && msg.is_empty() {
        msg = s(__content.string(4311))
            + __content.string(4312)
            + &prompt_text
            + __content.string(4313)
            + "\r"
            + "\r"
            + __content.string(4314);
        if g.industry_num[payer_id] == 2
            && 7 < g.credit_rating_num[payer_id]
            && g.cash[payer_id] + g.t_bills[payer_id] < g.net_worth * sf(0.667f32)
            && Ext::from_f64(0.0) < regulatory_max
        {
            msg = PbStr::new();
        }
        if Ext::from_f64(0.0) < regulatory_max {
            msg = PbStr::new();
        }
        skip_to_check = true;
    }
    if !skip_to_check {
        if g.liquid_funds < Ext::from_f64(1.0) || g.credit_rating_num[payer_id] < 4 {
            prompt_text = s(__content.string(4315));
            if g.credit_rating_num[payer_id] == 2 || g.credit_rating_num[payer_id] == 3 {
                prompt_text = s(__content.string(4316));
            }
            if g.liquid_funds < Ext::from_f64(1.0) {
                prompt_text = s(__content.string(4317));
            }
            if Ext::from_f64(25.0) < g.tmp_accrued_inc_tax + g.capital_taxes_total
                && g.net_worth * Ext::from_f64(0.5) < g.tmp_accrued_inc_tax + g.capital_taxes_total
            {
                prompt_text = s(__content.string(4318));
            }
            msg = payer_name.clone()
                + " "
                + &prompt_text
                + __content.string(4319)
                + __content.string(4320)
                + "\r"
                + "\r"
                + __content.string(4321);
        }
        if g.industry_num[payer_id] == 71 && msg.is_empty() {
            let epl = cast::<Ext, _>(g.extraordinary_pl[payer_id]);
            regulatory_max = (g.cash[payer_id] + g.t_bills[payer_id]) * sf(0.3f32);
            if epl <= Ext::from_f64(0.0) || epl < regulatory_max {
                regulatory_max = regulatory_max - epl;
            } else {
                msg = s(__content.string(4322))
                    + __content.string(4323)
                    + &payer_name
                    + __content.string(4324)
                    + __content.string(4325)
                    + "\r"
                    + "\r"
                    + __content.string(4326)
                    + __content.string(4327);
            }
        }
    }
    // LAB_005b4a6f
    if !msg.is_empty() {
        ui.msgbox(&msg, 8208, &s(__content.string(4328)));
        return extraordinary_dividend_exit();
    }
    balance_cash_t_bills(g, payer_id);
    if g.cash[payer_id] < Ext::from_f64(0.0) && g.cash[payer_id].abs() < g.unused_credit_line {
        let overdraft = g.cash[payer_id].abs();
        g.cash[payer_id] = Ext::from_f64(0.0);
        let lender = g.lender_id;
        g.cash[lender] = g.cash[lender] - overdraft;
        g.bank_debt[payer_id] = g.bank_debt[payer_id] + overdraft;
        calc_entity_financials(g, ui, rng, payer_id);
    }
    let mut tmp_q: i64 = to_quad(g.tmp_total_debt * sf(0.333f32));
    let mut max_payout: i64 = to_quad(g.net_worth - cast::<Ext, _>(tmp_q));
    if max_payout < 0 {
        max_payout = to_quad(Ext::from_f64(0.0));
    }
    if g.cash[payer_id] < cast::<Ext, _>(max_payout) && 0 < max_payout {
        max_payout = to_quad(g.cash[payer_id]);
        let room = to_quad(g.net_worth - cast::<Ext, _>(max_payout));
        if g.tmp_total_debt < cast::<Ext, _>(room) {
            tmp_q = to_quad((cast::<Ext, _>(room) - g.tmp_total_debt) * Ext::from_f64(0.5));
            if g.unused_credit_line < cast::<Ext, _>(tmp_q) {
                tmp_q = to_quad(g.unused_credit_line);
            }
            max_payout = max_payout + tmp_q;
        }
    }
    if g.total_assets * sf(0.9f32) < cast::<Ext, _>(max_payout) {
        max_payout = to_quad(g.total_assets * sf(0.9f32));
    }
    if Ext::from_f64(0.0) < regulatory_max {
        max_payout = to_quad(regulatory_max);
    }
    if g.liquid_funds < cast::<Ext, _>(max_payout) {
        max_payout = to_quad(g.liquid_funds);
    }
    msg = s(__content.string(4329))
        + &payer_name
        + __content.string(4330)
        + "\r\n"
        + "\r\n"
        + __content.string(4331)
        + &str_s(cast::<Ext, _>(max_payout))
        + " "
        + &g.million_prefix_lc
        + "illion"
        + &g.currency_suffix
        + __content.string(4332);
    let payout = loop {
        let title = g.ent_symbol[payer_id].clone() + __content.string(4333);
        let input = ui.inputbox(&msg, &title, &str_s(cast::<Ext, _>(max_payout)));
        let payout = val_removed(&input, b"$ ,%");
        if payout <= Ext::from_f64(0.0) {
            return extraordinary_dividend_exit();
        }
        if !((cast::<Ext, _>(max_payout)) < payout) {
            break payout;
        }
    };
    let fm = (cast::<Ext, _>(g.cur_year) + cast::<Ext, _>(payer_id) + cast::<Ext, _>(g.cur_quarter)) % Ext::from_f64(16.0);
    let taxable_ratio: f32 = cast::<f32, _>(cast::<f32, _>((fm + Ext::from_f64(10.0)) * sf(0.01f32)));
    if etf_id < 1581 {
        if g.total_assets * (cast::<Ext, _>(taxable_ratio)) < payout
            || payer_id == g.prev_extra_div_corp
            || payer_id == g.last_extra_div_corp
        {
            is_taxable = 1;
            if payout <= g.total_assets * cast::<Ext, _>(taxable_ratio) {
                msg = s(__content.string(4334))
                    + __content.string(4335);
            } else {
                msg = s(__content.string(4336)) + __content.string(4337);
            }
            msg = msg
                + __content.string(4338)
                + __content.string(4339)
                + __content.string(4340)
                + __content.string(4341);
            update_corp_control(g, ui, rng, payer_id);
            if g.parent80_owner < 11 {
                msg = msg + ".";
            } else {
                msg = msg
                    + __content.string(4342)
                    + __content.string(4343)
                    + &g.ent_symbol[cast::<i32, _>(g.parent80_owner)]
                    + ").";
                if 10 < g.wholly_owned_by || g.public_pct == 20 {
                    is_taxable = 0;
                    msg = s(__content.string(4344))
                        + &g.ent_symbol[cast::<i32, _>(g.parent80_owner)]
                        + __content.string(4345)
                        + __content.string(4346)
                        + &g.ent_symbol[payer_id]
                        + ".";
                }
            }
            msg = msg + "\r" + "\r";
            let naked_liab = get_plr_naked_liab(g, ui, rng, payer_id, 0, 0);
            if Ext::from_f64(100.0) < naked_liab {
                calc_entity_financials(g, ui, rng, payer_id);
                if g.tmp_credit_net_worth - payout < naked_liab * Ext::from_f64(0.5) {
                    msg = msg
                        + __content.string(4347)
                        + &payer_name
                        + " will "
                        + __content.string(4348)
                        + __content.string(4349)
                        + "\r"
                        + "\r";
                }
            }
            if is_taxable == 1 {
                prompt_text = s(__content.string(4350));
            } else {
                prompt_text = s(__content.string(4351));
            }
            msg = msg + __content.string(4352);
            answer = ui.msgbox(&msg, 8227, &prompt_text);
            if answer != 6 {
                return extraordinary_dividend_exit();
            }
        }
    } else {
        // msgText = " [runtime resource wording] reduce the
        // [runtime resource wording] " + EntSymbol(payerId) + ". If the distribution
        // [runtime resource wording] " -- never shown.
    }
    let mut player_holds = 0;
    for h_slot in 1..=15 {
        if cast::<i32, _>(g.hold_stock_id[(h_slot, g.cur_player)]) == payer_id && 0 < g.hold_stock_pct[(h_slot, g.cur_player)] {
            player_holds = 1;
        }
    }
    if 0 < player_holds && etf_id == 0 {
        msg = s(__content.string(4353))
            + __content.string(4354)
            + &str_s(cast::<Ext, _>(5 + g.corp_tax_rate))
            + __content.string(4355)
            + __content.string(4356);
        answer = ui.msgbox(&msg, 8227, &s(__content.string(4357)));
        if answer != 6 {
            return extraordinary_dividend_exit();
        }
    }
    let mut shortfall = payout - g.cash[payer_id];
    if Ext::from_f64(0.0) < shortfall {
        sell_t_bills_to_cash(g, payer_id, shortfall);
        shortfall = payout - g.cash[payer_id];
    }
    if Ext::from_f64(0.0) < shortfall {
        g.cash[payer_id] = g.cash[payer_id] + shortfall;
        let lender = g.lender_id;
        g.cash[lender] = g.cash[lender] - shortfall;
        g.bank_debt[payer_id] = g.bank_debt[payer_id] + shortfall;
    }
    g.cash[payer_id] = g.cash[payer_id] - payout;
    if etf_id == 0 {
        if payer_id != g.last_extra_div_corp {
            g.prev_extra_div_corp = g.last_extra_div_corp;
        }
        g.last_extra_div_corp = payer_id;
    }
    for holder_id in 1..=1600 {
        if !((11 <= holder_id || holder_id <= g.num_players) && g.entity_status[holder_id] == 1 && holder_id != payer_id) {
            continue;
        }
        for slot in 1..=15 {
            if cast::<i32, _>(g.hold_stock_id[(slot, holder_id)]) != payer_id {
                continue;
            }
            let hold_pct = cast::<i32, _>(g.hold_stock_pct[(slot, holder_id)]);
            let holder_share: i64 = to_quad(payout * sf(0.01f32) * cast::<Ext, _>(hold_pct));
            if 0 < hold_pct {
                g.cash[holder_id] = g.cash[holder_id] + cast::<Ext, _>(holder_share);
            }
            let cur_player = g.cur_player;
            let is_80_pct_parent = 79 < hold_pct && 10 < holder_id;
            if is_taxable == 1 && !is_80_pct_parent {
                if 0 < holder_share {
                    g.extraordinary_pl[holder_id] = to_quad(cast::<Ext, _>(g.extraordinary_pl[holder_id]) + cast::<Ext, _>(holder_share));
                }
            } else if etf_id < 1581 {
                if hold_pct < 20 {
                    if 11 <= holder_id && cur_player == g.corp_controller[holder_id] {
                        if 0 < holder_share {
                            g.div_recvd1[holder_id] = g.div_recvd1[holder_id] + cast::<Ext, _>(holder_share);
                        }
                    } else if 0 < holder_share {
                        g.div_recvd2[holder_id] = g.div_recvd2[holder_id] + cast::<Ext, _>(holder_share);
                    }
                } else {
                    if (hold_pct < 80 || holder_id < 11) && 0 < holder_share {
                        g.div_recvd1[holder_id] = g.div_recvd1[holder_id] + cast::<Ext, _>(holder_share);
                    }
                    if 79 < hold_pct && 10 < holder_id {
                        g.hold_stock_basis[(slot, holder_id)] = g.hold_stock_basis[(slot, holder_id)] - cast::<Ext, _>(holder_share);
                    }
                }
            } else {
                g.hold_stock_basis[(slot, holder_id)] = g.hold_stock_basis[(slot, holder_id)] - cast::<Ext, _>(holder_share);
                if g.hold_stock_basis[(slot, holder_id)] < Ext::from_f64(0.0) {
                    if holder_id <= 10 {
                        g.player_cap_gains[holder_id] = g.player_cap_gains[holder_id] + g.hold_stock_basis[(slot, holder_id)];
                    } else {
                        g.extraordinary_pl[holder_id] =
                            to_quad(cast::<Ext, _>(g.extraordinary_pl[holder_id]) + g.hold_stock_basis[(slot, holder_id)]);
                    }
                    g.hold_stock_basis[(slot, holder_id)] = Ext::from_f64(0.0);
                }
            }
            // LAB_005b5d8a
            g.news_msg = g.ent_symbol[holder_id].clone()
                + __content.string(4358)
                + &g.currency_symbol
                + &trim(&format(cast::<Ext, _>(holder_share), b"* ###,"))
                + " "
                + &g.million_prefix_lc
                + __content.string(4359)
                + &g.ent_symbol[payer_id];
            if g.hold_stock_pct[(slot, holder_id)] < 0 {
                g.news_msg = g.ent_symbol[holder_id].clone()
                    + __content.string(4360)
                    + &g.ent_symbol[payer_id]
                    + __content.string(4361);
            }
            post_news(g, ui, 0, 0);
        }
    }
    let per_share = payout / g.shares_outstanding[payer_id];
    g.news_msg = g.ent_symbol[payer_id].clone()
        + __content.string(4362)
        + &cur_sym(g)
        + &trim(&format(payout, b"* ###,"))
        + " "
        + &g.million_prefix_lc
        + "il."
        + &g.currency_suffix
        + " ("
        + &trim(&format(per_share * sf(0.01f32), b"* ###,.00"))
        + __content.string(4363);
    let mut m = s(__content.string(3176)) + "\r\n" + "\r\n" + &g.news_msg;
    show_quick_message(g, ui, &mut m);
    post_news(g, ui, 0, 0);
    g.turn_action_count = g.turn_action_count + 1;
    if Ext::from_f64(0.0) < payout {
        charge_shorts_for_distribution(g, ui, rng, payout, payer_id);
    }
    if etf_id < 1581 {
        calc_projected_earnings(g, ui, rng, payer_id);
        g.valuation_mode = 3;
        calc_stock_price(g, ui, rng, payer_id, now.second);
        update_corp_control(g, ui, rng, payer_id);
        if 10 < g.largest_holder {
            let parent_id = cast::<i32, _>(g.largest_holder);
            calc_projected_earnings(g, ui, rng, parent_id);
            g.valuation_mode = 3;
            calc_stock_price(g, ui, rng, parent_id, now.second);
        }
        let r = rng.rnd();
        g.market_cap[payer_id] = g.market_cap[payer_id] * (r * sf(0.1f32) + sf(0.8f32));
    } else {
        g.valuation_mode = 3;
        calc_stock_price(g, ui, rng, payer_id, now.second);
    }
    find_option_positions(g, ui, 0, payer_id, 0, 0);
    if g.has_options_outstanding[payer_id] == 1 {
        let price_adj = payout * -sf(0.01f32);
        adjust_options_for_corp_action(g, ui, payer_id, price_adj, 0, 0);
    }
    if 1580 < etf_id {
        adjust_price_for_distribution(g, ui, rng, payer_id, payout);
    }
    update_price_history_tick(g, ui, rng, payer_id);
    if etf_id == 0 && g.merger_target == g.active_entity {
        cancel_merger_on_violation(g, ui);
    }
    g.tmp_ext3 = -payout;
    adjust_index_divisor(g, payer_id, 0);
    adjust_conversion_price(g, payer_id, -payout, Ext::from_f64(0.0));
    adjust_price_alerts_for_stock(g, payer_id, -payout, Ext::from_f64(0.0));
    g.valuation_mode = 3;
    calc_stock_price(g, ui, rng, payer_id, now.second);
    check_stock_split(g, payer_id);
    extraordinary_dividend_exit()
}
