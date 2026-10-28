//! Capital contributions to 100%-owned subsidiaries: `ContributeCapitalToSub` (0x596b50) with its GOSUB
//! `ContributeCapital_RefreshAfterTransfer` (0x59ba3d), the asset-type dialog `ShowCapContribDialog`
//! (0x5dbaeb, its procedure and control callbacks) and `UpdateCtrlSinceInMiscData` (0x5288b9).
use crate::pb::real::{Ext, cast};


use super::dlg_util::{
    Nmhdr, adjust_dialog_frame_size, get_notify_hwnd_from, get_tooltip_ctrl_id, is_tab_sel_change,
    is_tooltip_need_text, set_tooltip_text,
};
use super::liquidate_sub::transfer_etf_contracts;
#[allow(unused_imports)]
use super::prelude::*;
use crate::logic::control::update_corp_control;
use crate::logic::earnings::{calc_projected_earnings, sell_t_bills_to_cash};
use crate::logic::etf_derivs::corp_options::force_close_excess_naked_options;
use crate::logic::finance::calc_entity_financials;
use crate::logic::holdings::{find_bond_holding_slot, find_holding_slot};
use crate::logic::options::get_optioned_stock_pct;
use crate::logic::report::{number_to_words, show_quick_message};
use crate::logic::reports_corp::check_cross_ownership;
use crate::logic::reports_lists::show_portfolio_listing;
use crate::logic::sim_entity::asset_sales::gen_asset_sale_description;
use crate::logic::sim_entity::bonds::{
    adjust_conversion_price, adjust_price_alerts_for_stock, convert_or_redeem_convertibles,
};
use crate::logic::sim_entity::quarter_start::{pack_holding_arrays, recapture_excess_loss};
use crate::logic::valuation::{
    adjust_index_divisor, calc_stock_price, calc_working_capital_ratio, check_stock_split,
};
use crate::pb::string::{CaseMode, mcase, parse_count, remove_any};

/// `VAL(REMOVE$(text, ANY "$ ,"))`: an amount typed into an INPUTBOX.
fn input_val(text: &PbStr) -> Ext {
    val(remove_any(text.as_bytes(), b"$ ,").as_bytes())
}

/// `ShowQuickMessage` of a temporary string.
fn quick(g: &mut Globals, ui: &mut dyn Ui, text: PbStr) {
    let mut t = text;
    show_quick_message(g, ui, &mut t);
}

/// Locals of ContributeCapitalToSub shared with its GOSUB.
#[derive(Default)]
struct CcFrame {
    parent_id: i32,
    sub_id: i32,
    asset_corp_id: i32,
    /// EXT local: the transferred tax basis; also receives `CalcProjectedEarnings` in the GOSUB.
    transfer_basis: Ext,
    /// EXT local (`local_12c`): the value by which the subsidiary's market cap is moved in the GOSUB.
    price_delta: Ext,
}

/// Port of ContributeCapitalToSub (0x596b50).
/// Player action: the active entity (`ActiveEntity`) contributes capital to one of its 100%-owned
/// subsidiaries (picked from its portfolio listing when there are several): cash (limited by its liquid
/// funds, for banks by the regulators' stock-investment limit), business assets (the subsidiary takes
/// over the assets — a holding company enters the parent's industry and loses its tax attributes; a parent
/// left without assets [runtime resource wording] ), stock of another company, or bonds (government
/// bonds to banks/insurers, corporate bonds, or the subsidiary's own bonds, which are extinguished). The
/// transferred basis is added to the parent's basis in the subsidiary; then the subsidiary is revalued
/// (GOSUB [`refresh_after_transfer`]). Every transfer counts as a turn action.
///
/// `time_secs`: the seconds of `TIME$` for `CalcStockPrice` (see [`calc_stock_price`]).
///
/// When cash and T-bills don't cover a cash contribution of the player (or of a corp > 85),
/// `BorrowToCoverPurchase` offers a loan; NO cancels.
#[allow(unused_assignments)] // the original overwrites some values unread
pub fn contribute_capital_to_sub(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut f = CcFrame::default();
    let mut lost_tax_attribs: i32 = 0;
    let mut bank_contrib_limit: i64 = 0;
    let mut sub_count: i32 = 0;
    let mut amount: i64;
    let mut max_amount: i64;
    let mut has_assets: i32;
    let mut msg_text: PbStr;
    let mut aux_text: PbStr;
    let mut input_text: PbStr;

    f.parent_id = g.active_entity;
    let parent_id = f.parent_id;
    for i in 1..=15 {
        if g.hold_stock_pct[(i, parent_id)] == 100 {
            sub_count += 1;
            f.sub_id = cast::<i32, _>(g.hold_stock_id[(i, parent_id)]);
        }
    }
    if sub_count == 0 {
        msg_text = s(__content.string(9297))
            + __content.string(9298)
            + __content.string(9299)
            + "\r"
            + "\r"
            + __content.string(4321);
        let t = g.app_title.clone();
        ui.msgbox(&(g.active_entity_name.clone() + &msg_text), 8208, &t);
        return;
    }
    if 1 < sub_count {
        aux_text = number_to_words(sub_count, 0, 0);
        input_text = s(__content.string(9300));
        if parent_id < 11 {
            input_text = s(__content.string(5792));
        }
        msg_text = g.active_entity_name.clone()
            + " has "
            + &aux_text
            + __content.string(9301)
            + &input_text
            + "."
            + "\r"
            + "\r"
            + __content.string(9302)
            + &input_text
            + " that "
            + &g.active_entity_name.clone()
            + __content.string(9303);
        let answer = ui.msgbox(&msg_text, 8228, &s(__content.string(9304)));
        if answer == 7 {
            return;
        }
        g.tmp_symbol = PbStr::new();
        g.action_code = s("CC");
        g.entity_pick_mode = s("GetCorp");
        g.target_corp = 0;
        g.tmp_ext1 = -Ext::from_f64(1.0);
        let ae = g.active_entity;
        show_portfolio_listing(g, ui, rng, ae, 0, 0);
        if g.target_corp < 11 {
            quick(g, ui, s(__content.string(9305)));
            return;
        }
        f.sub_id = g.target_corp;
        g.target_corp = 0;
        for i in 1..=15 {
            if cast::<i32, _>(g.hold_stock_id[(i, parent_id)]) == f.sub_id && g.hold_stock_pct[(i, parent_id)] != 100 {
                quick(g, ui, g.ent_name[f.sub_id].clone() + __content.string(9306) + __content.string(4321));
                return;
            }
        }
    }
    let sub_id = f.sub_id;
    g.tmp_ext1 = cast::<Ext, _>(sub_id);
    show_cap_contrib_dialog(g, ui, 0);
    g.dlg_open_cap_contrib = 0;
    let choice = to_long(g.tmp_ext1);
    g.tmp_ext1 = Ext::from_f64(0.0);
    if choice == 0 {
        return;
    }
    calc_entity_financials(g, ui, rng, parent_id);
    if g.liquid_funds <= Ext::from_f64(0.0) && choice == 1 {
        quick(g, ui,
            g.active_entity_name.clone()
                + __content.string(9307)
                + __content.string(9308),
        );
        return;
    }
    if choice == 4 && 130 < sub_id {
        let mut bond_slot = find_bond_holding_slot(g, g.active_entity, sub_id);
        if 0 < bond_slot && sub_id != g.hold_bond_id[(bond_slot, g.active_entity)] {
            bond_slot = 0;
        }
        if bond_slot == 0 {
            aux_text = g.active_entity_name.clone() + __content.string(9309);
            if g.active_entity < 11 {
                aux_text = s(__content.string(9310));
            }
            if g.bonds_issued[sub_id] == 0 {
                aux_text = s(__content.string(9311));
            }
            msg_text = s(__content.string(9312))
                + &g.ent_name[sub_id]
                + __content.string(9313)
                + __content.string(9314)
                + "\r\n"
                + "\r\n"
                + __content.string(9315)
                + &g.ent_name[sub_id]
                + __content.string(9316)
                + "but "
                + &aux_text
                + ".)";
            show_quick_message(g, ui, &mut msg_text);
            return;
        }
    }
    if 10 < parent_id && g.industry_num[parent_id] == 1 && (choice == 1 || choice == 4) {
        bank_contrib_limit = to_quad(g.net_worth * sf(0.5f32) - g.tmp_subsid_stock_value);
        if (cast::<Ext, _>(bank_contrib_limit)) < Ext::from_f64(0.0) {
            bank_contrib_limit = 0;
        }
        if g.net_worth * sf(0.5f32) < g.tmp_subsid_stock_value {
            let country = g.country_name[g.country_idx[g.active_entity]].clone();
            quick(g, ui,
                s(__content.string(4025))
                    + &g.active_entity_name.clone()
                    + __content.string(9317)
                    + __content.string(9318)
                    + &mcase(country.as_bytes(), CaseMode::Ansi)
                    + __content.string(9319)
                    + __content.string(9320),
            );
            return;
        }
    }
    if choice == 2 && 130 < parent_id && Ext::from_f64(0.0) < g.business_assets[parent_id] {
        // ---- business assets
        if g.industry_num[sub_id] != g.industry_num[parent_id] && g.industry_num[sub_id] != 70 {
            msg_text = s(__content.string(9321))
                + &g.ent_name[sub_id]
                + __content.string(9322);
            show_quick_message(g, ui, &mut msg_text);
            return;
        }
        aux_text = PbStr::new();
        if 1 < g.hold_stock_id[(0, parent_id)] {
            aux_text = s(__content.string(9323));
        }
        msg_text = g.active_entity_name.clone()
            + __content.string(9324)
            + &aux_text
            + "to "
            + &g.ent_name[sub_id]
            + "."
            + "\r"
            + "\r"
            + __content.string(9325)
            + "\r"
            + "\r";
        amount = 6;
        if g.industry_num[sub_id] == 70
            && !(-Ext::from_f64(100.0) * g.exchange_rate <= g.tax_loss_carry[sub_id] && g.tax_credits[sub_id] <= Ext::from_f64(25.0))
        {
            let ind_name = g.ind_name[cast::<i32, _>(g.industry_num[parent_id])].clone();
            if -Ext::from_f64(100.0) * g.exchange_rate <= g.tax_loss_carry[sub_id] {
                msg_text = msg_text
                    + &g.ent_symbol[sub_id]
                    + __content.string(9326)
                    + "the "
                    + &ind_name
                    + __content.string(9327)
                    + "\""
                    + "change "
                    + __content.string(9328)
                    + "\"";
                if 15 < len(ind_name.as_bytes()) {
                    msg_text = replace(msg_text.as_bytes(), __content.bytes(9329), b"be");
                }
            } else {
                msg_text = msg_text
                    + "("
                    + &g.ent_symbol[sub_id]
                    + __content.string(9330)
                    + "the "
                    + &ind_name
                    + __content.string(9331)
                    + "\""
                    + "change "
                    + __content.string(9328)
                    + "\"";
                if 13 < len(ind_name.as_bytes()) {
                    msg_text = replace(msg_text.as_bytes(), __content.bytes(9332), b"be");
                }
                if g.tax_credits[sub_id] <= Ext::from_f64(25.0) {
                    msg_text = msg_text + ")";
                } else {
                    msg_text = msg_text + __content.string(9333);
                }
            }
            amount = cast::<i64, _>(ui.msgbox(&msg_text, 8484, &s(__content.string(9334))));
        }
        if amount == 6 {
            msg_text = s(__content.string(9335))
                + &g.ent_symbol[parent_id]
                + __content.string(9336);
            aux_text = fmt_trim(g.business_assets[parent_id], b"* ###,.00") + " " + &g.million_prefix_uc + ".";
            let most_threshold = g.business_assets[parent_id] * sf(0.6f32);
            let title = s(__content.string(9337)) + &g.ent_symbol[sub_id];
            loop {
                input_text = ui.inputbox(&msg_text, &title, &aux_text);
                f.transfer_basis = input_val(&input_text);
                if f.transfer_basis <= g.business_assets[parent_id] + sf(0.006f32) {
                    break;
                }
                if !(Ext::from_f64(0.0) < f.transfer_basis) {
                    break;
                }
            }
            if Ext::from_f64(0.0) < f.transfer_basis {
                let amt = f.transfer_basis;
                let ae = g.active_entity;
                gen_asset_sale_description(g, rng, ae);
                let dest_slot = find_holding_slot(g, g.active_entity, sub_id);
                g.hold_stock_basis[(dest_slot, parent_id)] = g.hold_stock_basis[(dest_slot, parent_id)] + amt;
                if g.industry_num[sub_id] == 70 {
                    g.business_assets[sub_id] = amt;
                    g.industry_num[sub_id] = g.industry_num[parent_id];
                    g.profit_margin[sub_id] = g.profit_margin[parent_id];
                    g.disc_spend_pct[sub_id] = g.disc_spend_pct[parent_id];
                    g.growth_rate[sub_id] = g.growth_rate[parent_id];
                    if g.tax_loss_carry[sub_id] < -Ext::from_f64(100.0) {
                        lost_tax_attribs = 1;
                    }
                    g.tax_loss_carry[sub_id] = Ext::from_f64(0.0);
                    if Ext::from_f64(25.0) < g.tax_credits[sub_id] {
                        lost_tax_attribs += 2;
                    }
                    g.tax_credits[sub_id] = Ext::from_f64(0.0);
                } else {
                    let ba = g.business_assets[sub_id];
                    let a1 = Ext::from_f64(100.0) * (ba / (ba + amt)) * cast::<Ext, _>(g.profit_margin[sub_id]);
                    let a2 = Ext::from_f64(100.0) * (amt / (ba + amt)) * cast::<Ext, _>(g.profit_margin[parent_id]);
                    let new_margin = to_quad((a1 + a2) / Ext::from_f64(100.0));
                    g.profit_margin[sub_id] = cast::<i16, _>(new_margin);
                    g.business_assets[sub_id] = amt + g.business_assets[sub_id];
                }
                f.price_delta = amt;
                g.business_assets[parent_id] = g.business_assets[parent_id] - amt;
                let x = amt * calc_working_capital_ratio(g, parent_id);
                g.cash[parent_id] = x + g.cash[parent_id];
                g.working_capital[parent_id] = g.working_capital[parent_id] - x;
                let x = amt * calc_working_capital_ratio(g, sub_id);
                g.cash[sub_id] = g.cash[sub_id] - x;
                g.working_capital[sub_id] = x + g.working_capital[sub_id];
                if sf(0.006f32) < g.business_assets[parent_id] {
                    if most_threshold < amt && g.exchange_rate * Ext::from_f64(500.0) < amt {
                        g.asset_sale_desc = s(__content.string(9338));
                    }
                } else {
                    g.business_assets[parent_id] = Ext::from_f64(0.0);
                    if g.industry_num[parent_id] == 37 {
                        transfer_etf_contracts(g, ui, sub_id, g.active_entity);
                    }
                    g.industry_num[parent_id] = 70;
                    if Ext::from_f64(0.0) < g.working_capital[parent_id] {
                        g.cash[parent_id] = g.working_capital[parent_id] + g.cash[parent_id];
                        g.working_capital[parent_id] = Ext::from_f64(0.0);
                    }
                    g.asset_sale_desc = s(__content.string(2245));
                }
                g.news_msg = g.ent_symbol[parent_id].clone() + __content.string(9339) + &g.asset_sale_desc + " to " + &g.ent_symbol[sub_id];
                if len(g.news_msg.as_bytes()) < 60 {
                    g.news_msg = g.news_msg.clone() + __content.string(8933);
                }
                msg_text = s(__content.string(9340))
                    + "\r"
                    + "\r"
                    + __content.string(9341)
                    + &g.active_entity_name
                    + __content.string(9342)
                    + &g.asset_sale_desc
                    + __content.string(9343)
                    + &g.ent_name[sub_id]
                    + " ("
                    + &g.ent_symbol[sub_id]
                    + __content.string(9344)
                    + &trim(g.currency_symbol.as_bytes())
                    + &fmt_trim(amt, b"################,.00")
                    + " "
                    + &g.million_prefix_lc
                    + "illion"
                    + &g.currency_suffix
                    + "."
                    + "\r"
                    + "\r";
                if g.industry_num[parent_id] == 70 {
                    msg_text = msg_text
                        + &g.active_entity_name
                        + __content.string(9345)
                        + __content.string(9346);
                }
                ui.msgbox(&msg_text, 8192, &s(__content.string(9347)));
                post_news(g, ui, 0, 0);
                if 0 < lost_tax_attribs {
                    g.news_msg = g.ent_symbol[sub_id].clone() + __content.string(9348);
                }
                if lost_tax_attribs == 2 {
                    g.news_msg = replace(g.news_msg.as_bytes(), __content.bytes(9349), __content.bytes(2125));
                }
                if lost_tax_attribs == 3 {
                    g.news_msg = replace(g.news_msg.as_bytes(), __content.bytes(9350), __content.bytes(9351));
                }
                if !g.news_msg.is_empty() {
                    post_news(g, ui, 0, 0);
                }
                g.turn_action_count += 1;
                refresh_after_transfer(g, ui, rng, &mut f, time_secs);
                return;
            }
        }
    } else if choice == 2 {
        msg_text = s(__content.string(9352)) + &g.active_entity_name + __content.string(9353) + &g.ent_name[sub_id] + ".";
        show_quick_message(g, ui, &mut msg_text);
        return;
    }
    calc_entity_financials(g, ui, rng, parent_id);
    if choice == 1 {
        // ---- cash
        if 10 < parent_id
            && g.industry_num[parent_id] == 1
            && 0 < bank_contrib_limit
            && (cast::<Ext, _>(bank_contrib_limit)) < g.liquid_funds
        {
            quick(g, ui,
                s(__content.string(9354))
                    + __content.string(9355)
                    + &fmt_trim(cast::<Ext, _>(bank_contrib_limit), b"##################")
                    + " "
                    + &g.million_prefix_lc
                    + "illion "
                    + __content.string(9356),
            );
            g.liquid_funds = cast::<Ext, _>(bank_contrib_limit);
        }
        aux_text = fmt_trim(g.liquid_funds, b"* ###,") + " " + &g.million_prefix_lc + "illion";
        msg_text = g.active_entity_name.clone()
            + __content.string(9357)
            + __content.string(9358)
            + &trim(g.currency_symbol.as_bytes())
            + &aux_text
            + &g.currency_suffix
            + "."
            + "\r"
            + "\r"
            + __content.string(9359)
            + &g.ent_symbol[parent_id]
            + __content.string(9360)
            + &g.ent_name[sub_id]
            + "?";
        let title = s(__content.string(9361)) + &g.ent_symbol[sub_id];
        loop {
            input_text = ui.inputbox(&msg_text, &title, &aux_text);
            amount = to_quad(input_val(&input_text));
            if !(g.liquid_funds + Ext::from_f64(1.0) < cast::<Ext, _>(amount)) {
                break;
            }
        }
        if 0 < amount {
            let shortfall = cast::<Ext, _>(amount) - g.cash[parent_id];
            sell_t_bills_to_cash(g, parent_id, shortfall);
            let mut shortfall = cast::<Ext, _>(amount) - g.cash[parent_id];
            if Ext::from_f64(0.0) < shortfall && (g.cur_player == parent_id || 85 < parent_id) {
                calc_entity_financials(g, ui, rng, parent_id);
                if crate::logic::trading::borrow_to_cover_purchase(g, ui, &mut shortfall, Ext::from_f64(0.0), 0) == 7 {
                    return;
                }
            }
            g.cash[parent_id] = g.cash[parent_id] - cast::<Ext, _>(amount);
            g.cash[sub_id] = cast::<Ext, _>(amount) + g.cash[sub_id];
            msg_text = s(__content.string(9362))
                + &g.currency_symbol
                + &fmt_trim(cast::<Ext, _>(amount), b"###############,")
                + " "
                + &g.million_prefix_lc
                + "il. to ";
            g.news_msg = g.ent_symbol[parent_id].clone() + &msg_text + &g.ent_symbol[sub_id];
            post_news(g, ui, 0, 0);
            ui.msgbox(
                &(s(__content.string(9363))
                    + "\r"
                    + "\r"
                    + &g.active_entity_name
                    + &msg_text
                    + " "
                    + &g.ent_name[sub_id]
                    + "."),
                8192,
                &s("CONFIRMATION"),
            );
            g.turn_action_count += 1;
            for i in 1..=15 {
                if cast::<i32, _>(g.hold_stock_id[(i, parent_id)]) == sub_id {
                    g.hold_stock_basis[(i, parent_id)] = cast::<Ext, _>(amount) + g.hold_stock_basis[(i, parent_id)];
                }
            }
            f.price_delta = cast::<Ext, _>(amount);
            refresh_after_transfer(g, ui, rng, &mut f, time_secs);
        }
        return;
    }
    if choice != 3 && choice != 4 {
        return;
    }
    // ---- stock or bonds
    pack_holding_arrays(g, ui);
    has_assets = 1;
    msg_text = s("stock");
    if choice == 4 {
        msg_text = s("bond");
    }
    if g.hold_stock_id[(0, parent_id)] == 1 && choice == 3 {
        has_assets = 0;
    }
    if choice == 4 {
        if parent_id <= 130 {
            if 0 < parent_id
                && g.hold_bond_id[(0, parent_id)] < 1
                && g.long_gov_bond_face[parent_id] <= 0
                && g.short_gov_bond_face[parent_id] <= 0
            {
                has_assets = 0;
            }
        } else {
            has_assets = 0;
        }
    }
    if has_assets == 0 {
        quick(g, ui,
            g.active_entity_name.clone() + __content.string(3737) + &msg_text + __content.string(9364) + &g.ent_name[sub_id] + __content.string(9365),
        );
        return;
    }
    if choice == 4 {
        has_assets = 0;
        if 0 < g.short_gov_bond_face[parent_id] {
            has_assets = 2;
        }
        if 0 < g.long_gov_bond_face[parent_id] {
            has_assets += 1;
        }
        if 0 < has_assets {
            f.transfer_basis = Ext::from_f64(0.0);
            amount = cast::<i64, _>(ui.msgbox(
                &s(__content.string(9366)),
                8227,
                &s(__content.string(9367)),
            ));
            if amount == 7 {
                has_assets = 0;
            } else {
                if amount == 2 {
                    return;
                }
                amount = 0;
                if has_assets != 2 && 10 < sub_id && sub_id < 131 {
                    aux_text = s(__content.string(9368))
                        + &g.active_entity_name
                        + __content.string(9369)
                        + &g.ent_name[sub_id]
                        + "?";
                    max_amount = g.long_gov_bond_face[parent_id];
                    let title = s(__content.string(9370)) + &g.ent_symbol[sub_id];
                    loop {
                        input_text = ui.inputbox(&aux_text, &title, &str_s(cast::<Ext, _>(max_amount)));
                        amount = to_quad(input_val(&input_text));
                        if !(max_amount < amount) {
                            break;
                        }
                    }
                    if amount <= 0 {
                        amount = 0;
                    } else {
                        f.transfer_basis = g.lt_gov_bond_cost[parent_id] * (cast::<Ext, _>(amount) / cast::<Ext, _>(g.long_gov_bond_face[parent_id]));
                        g.long_gov_bond_face[parent_id] = to_quad(cast::<Ext, _>(g.long_gov_bond_face[parent_id]) - cast::<Ext, _>(amount));
                        g.lt_gov_bond_cost[parent_id] = g.lt_gov_bond_cost[parent_id] - f.transfer_basis;
                        if g.long_gov_bond_face[parent_id] == 0 {
                            g.lt_gov_bond_cost[parent_id] = Ext::from_f64(0.0);
                        }
                        g.long_gov_bond_face[sub_id] = to_quad(cast::<Ext, _>(amount) + cast::<Ext, _>(g.long_gov_bond_face[sub_id]));
                        f.price_delta = sf(0.01f32) * cast::<Ext, _>(amount) * cast::<Ext, _>(g.long_bond_price);
                        if f.transfer_basis < f.price_delta {
                            g.lt_gov_bond_cost[sub_id] = f.transfer_basis + g.lt_gov_bond_cost[sub_id];
                        } else {
                            g.lt_gov_bond_cost[sub_id] = f.price_delta + g.lt_gov_bond_cost[sub_id];
                        }
                    }
                }
                if 1 < has_assets && 10 < sub_id && sub_id < 131 && amount == 0 {
                    aux_text = s(__content.string(9371))
                        + &g.active_entity_name
                        + __content.string(9369)
                        + &g.ent_name[sub_id]
                        + "?";
                    max_amount = g.short_gov_bond_face[parent_id];
                    let title = s(__content.string(9372)) + &g.ent_symbol[sub_id];
                    loop {
                        input_text = ui.inputbox(&aux_text, &title, &str_s(cast::<Ext, _>(max_amount)));
                        amount = to_quad(input_val(&input_text));
                        if !(max_amount < amount) {
                            break;
                        }
                    }
                    if amount <= 0 {
                        amount = 0;
                    } else {
                        f.transfer_basis = g.st_gov_bond_cost[parent_id] * (cast::<Ext, _>(amount) / cast::<Ext, _>(g.short_gov_bond_face[parent_id]));
                        g.short_gov_bond_face[parent_id] = to_quad(cast::<Ext, _>(g.short_gov_bond_face[parent_id]) - cast::<Ext, _>(amount));
                        g.st_gov_bond_cost[parent_id] = g.st_gov_bond_cost[parent_id] - f.transfer_basis;
                        if g.short_gov_bond_face[parent_id] == 0 {
                            g.st_gov_bond_cost[parent_id] = Ext::from_f64(0.0);
                        }
                        g.short_gov_bond_face[sub_id] = to_quad(cast::<Ext, _>(amount) + cast::<Ext, _>(g.short_gov_bond_face[sub_id]));
                        f.price_delta = sf(0.01f32) * cast::<Ext, _>(amount) * cast::<Ext, _>(g.short_bond_price);
                        if f.transfer_basis < f.price_delta {
                            g.st_gov_bond_cost[sub_id] = f.transfer_basis + g.st_gov_bond_cost[sub_id];
                        } else {
                            g.st_gov_bond_cost[sub_id] = f.price_delta + g.st_gov_bond_cost[sub_id];
                        }
                    }
                }
                if 0 < amount {
                    let dest_slot = find_holding_slot(g, parent_id, sub_id);
                    g.hold_stock_basis[(dest_slot, parent_id)] = f.transfer_basis + g.hold_stock_basis[(dest_slot, parent_id)];
                    aux_text = s(__content.string(9373));
                    g.news_msg = g.ent_symbol[parent_id].clone() + &aux_text + &g.ent_symbol[sub_id];
                    post_news(g, ui, 0, 0);
                    ui.msgbox(
                        &(s(__content.string(9374)) + "\r" + "\r" + &g.active_entity_name + &aux_text + " " + &g.ent_name[sub_id]),
                        8192,
                        &s("CONFIRMATION"),
                    );
                    refresh_after_transfer(g, ui, rng, &mut f, time_secs);
                    g.turn_action_count += 1;
                    return;
                }
            }
        }
    }
    if g.tmp_corp_bonds_value == Ext::from_f64(0.0) && choice == 4 && sub_id < 130 {
        quick(g, ui, g.active_entity_name.clone() + __content.string(9375) + &g.ent_name[sub_id] + ".");
        return;
    }
    if 130 < sub_id && choice == 4 {
        has_assets = 0;
        for i in 1..=15 {
            if sub_id == g.hold_bond_id[(i, parent_id)] {
                has_assets = 1;
            }
        }
        if has_assets == 0 {
            quick(g, ui,
                g.ent_name[sub_id].clone() + __content.string(9376) + __content.string(9377),
            );
            return;
        }
    }
    if msg_text == "bond" {
        msg_text = s(__content.string(9378));
    }
    g.target_corp = 0;
    if choice == 4 && g.hold_bond_id[(0, parent_id)] == 1 {
        has_assets = 1;
        g.target_corp = g.hold_bond_id[(1, parent_id)];
        if g.target_corp < 11 {
            g.target_corp = 0;
            has_assets = 0;
        }
    } else {
        quick(g, ui,
            s(__content.string(9379))
                + &msg_text
                + __content.string(9380)
                + &g.ent_name[sub_id]
                + __content.string(9381)
                + &msg_text
                + __content.string(9382)
                + &g.active_entity_name
                + ".",
        );
        g.tmp_symbol = PbStr::new();
        g.action_code = s("CC");
        g.entity_pick_mode = s("GetCorp");
        g.tmp_ext1 = Ext::from_f64(1.0);
        if choice == 4 {
            g.tmp_ext1 = Ext::from_f64(2.0);
        }
        g.tmp_ext1 = -g.tmp_ext1;
        let ae = g.active_entity;
        show_portfolio_listing(g, ui, rng, ae, 1, 0);
        has_assets = cast::<i32, _>(10 < g.target_corp);
        if sub_id == g.target_corp {
            if choice == 3 {
                has_assets = 0;
            }
        } else if choice == 4 && 130 < sub_id {
            aux_text = if 0 < g.bonds_issued[sub_id] { PbStr::new() } else { s(" no") };
            msg_text = g.ent_name[sub_id].clone()
                + __content.string(9376)
                + __content.string(9383)
                + __content.string(9384)
                + "("
                + &g.ent_name[sub_id]
                + __content.string(2412)
                + &aux_text
                + __content.string(9385);
            show_quick_message(g, ui, &mut msg_text);
            return;
        }
    }
    if has_assets == 0 {
        quick(g, ui, s("No ") + &msg_text + __content.string(9386));
        return;
    }
    f.asset_corp_id = g.target_corp;
    let asset = f.asset_corp_id;
    let mut dest_slot = find_holding_slot(g, sub_id, asset);
    if choice == 4 {
        dest_slot = find_bond_holding_slot(g, sub_id, asset);
        if asset == sub_id {
            dest_slot = -1;
            let answer = ui.msgbox(
                &(s(__content.string(9387))
                    + &g.ent_name[sub_id]
                    + ", "
                    + __content.string(9388)
                    + __content.string(9389)),
                8483,
                &s(__content.string(9390)),
            );
            if answer != 6 {
                return;
            }
        }
    }
    if dest_slot == 0 {
        aux_text = g.ent_name[sub_id].clone() + __content.string(9391) + &msg_text + __content.string(9392) + __content.string(4314);
        show_quick_message(g, ui, &mut aux_text);
        return;
    }
    if choice == 3 {
        // ---- stock
        let src_slot = find_holding_slot(g, parent_id, asset);
        max_amount = cast::<i64, _>(g.hold_stock_pct[(src_slot, parent_id)]);
        if max_amount < 0 {
            msg_text = s(__content.string(9393)) + &g.ent_name[asset] + __content.string(9394);
            show_quick_message(g, ui, &mut msg_text);
            return;
        }
        let mut src_basis = g.hold_stock_basis[(src_slot, parent_id)];
        msg_text = g.ent_symbol[parent_id].clone()
            + " owns"
            + &str_s(cast::<Ext, _>(max_amount))
            + "% of "
            + &g.ent_symbol[asset]
            + __content.string(9395)
            + &g.ent_symbol[asset]
            + __content.string(9396)
            + &g.ent_name[sub_id]
            + " (1% to"
            + &str_s(cast::<Ext, _>(max_amount))
            + "%):";
        aux_text = trim(str_s(cast::<Ext, _>(max_amount)).as_bytes());
        if g.cur_player == g.corp_controller[asset] {
            let ae = g.active_entity;
            has_assets = get_optioned_stock_pct(g, ui, ae, asset, 0);
            if 0 < has_assets {
                let mut owner_text = s(__content.string(1506));
                if 10 < parent_id {
                    owner_text = g.active_entity_name.clone() + " has";
                }
                let mut warn_text = s(__content.string(9397))
                    + &owner_text
                    + __content.string(9398)
                    + &str_s(cast::<Ext, _>(has_assets))
                    + __content.string(2163)
                    + &g.ent_name[asset]
                    + __content.string(9399);
                if g.active_entity < 11 {
                    warn_text = warn_text
                        + __content.string(9400)
                        + &g.ent_name[sub_id]
                        + __content.string(9401)
                        + __content.string(9402)
                        + &g.ent_name[asset]
                        + __content.string(9403)
                        + &str_s(cast::<Ext, _>(has_assets))
                        + "%, you "
                        + __content.string(9404)
                        + __content.string(9405)
                        + &g.ent_symbol[asset]
                        + "."
                        + "\r"
                        + "\r";
                } else {
                    warn_text = warn_text
                        + __content.string(9406)
                        + &g.active_entity_name
                        + __content.string(9407)
                        + &g.ent_symbol[asset]
                        + __content.string(9408)
                        + __content.string(9409)
                        + &g.ent_symbol[asset]
                        + "."
                        + "\r"
                        + "\r";
                }
                warn_text = warn_text
                    + __content.string(9410)
                    + __content.string(9411)
                    + __content.string(9412)
                    + "\r"
                    + "\r"
                    + __content.string(9413);
                let answer = ui.msgbox(&warn_text, 8484, &s(__content.string(9414)));
                if answer == 7 {
                    return;
                }
                if max_amount == cast::<i64, _>(has_assets) {
                    quick(g, ui, s(__content.string(9415)));
                }
            }
        }
        let title = s(__content.string(9416)) + &g.ent_symbol[asset] + __content.string(9417) + &g.ent_symbol[sub_id];
        loop {
            input_text = ui.inputbox(&msg_text, &title, &aux_text);
            amount = to_quad(input_val(&input_text));
            if !(max_amount < amount) {
                break;
            }
        }
        if amount < 1 {
            return;
        }
        let pct = to_long(cast::<Ext, _>(amount));
        if g.hold_stock_id[(dest_slot, sub_id)] == 0 {
            g.hold_stock_id[(dest_slot, sub_id)] = cast::<i16, _>(asset);
        }
        g.hold_stock_pct[(dest_slot, sub_id)] = (cast::<i16, _>(pct)).wrapping_add(g.hold_stock_pct[(dest_slot, sub_id)]);
        g.hold_stock_pct[(src_slot, parent_id)] = g.hold_stock_pct[(src_slot, parent_id)].wrapping_sub(cast::<i16, _>(pct));
        if 0 < g.hold_stock_pct[(src_slot, parent_id)]
            && g.hold_stock_pct[(src_slot, parent_id)] < 80
            && g.hold_stock_basis[(src_slot, parent_id)] < Ext::from_f64(0.0)
            && 10 < parent_id
        {
            recapture_excess_loss(g, ui, asset, parent_id);
            src_basis = Ext::from_f64(0.0);
        }
        f.transfer_basis = cast::<Ext, _>(pct) / cast::<Ext, _>(max_amount);
        f.transfer_basis = src_basis * f.transfer_basis;
        g.hold_stock_basis[(src_slot, parent_id)] = g.hold_stock_basis[(src_slot, parent_id)] - f.transfer_basis;
        f.price_delta = g.market_cap[asset] * cast::<Ext, _>(pct);
        if f.transfer_basis < f.price_delta {
            g.hold_stock_basis[(dest_slot, sub_id)] = f.transfer_basis + g.hold_stock_basis[(dest_slot, sub_id)];
        } else {
            g.hold_stock_basis[(dest_slot, sub_id)] = f.price_delta + g.hold_stock_basis[(dest_slot, sub_id)];
        }
        if g.hold_stock_pct[(src_slot, parent_id)] < 1 || cast::<Ext, _>(max_amount) == cast::<Ext, _>(pct) {
            g.hold_stock_id[(src_slot, parent_id)] = 0;
            g.hold_stock_pct[(src_slot, parent_id)] = 0;
            g.hold_stock_basis[(src_slot, parent_id)] = Ext::from_f64(0.0);
        }
        if src_basis < Ext::from_f64(0.0) && g.hold_stock_pct[(src_slot, parent_id)] == 0 {
            g.hold_stock_basis[(src_slot, parent_id)] = Ext::from_f64(0.0);
            f.transfer_basis = Ext::from_f64(0.0);
        }
        let ds = find_holding_slot(g, parent_id, sub_id);
        if 0 < ds {
            g.hold_stock_basis[(ds, parent_id)] = f.transfer_basis + g.hold_stock_basis[(ds, parent_id)];
        }
        msg_text = s(__content.string(9418)) + &g.ent_symbol[asset] + " to ";
        g.news_msg = g.ent_symbol[parent_id].clone() + &msg_text + &g.ent_symbol[sub_id];
        post_news(g, ui, 0, 0);
        ui.msgbox(
            &(g.active_entity_name.clone() + &msg_text + &g.ent_name[sub_id] + "."),
            8192,
            &s(__content.string(9419)),
        );
        g.turn_action_count += 1;
        if g.cur_player == g.corp_controller[asset] {
            let ae = g.active_entity;
            force_close_excess_naked_options(g, ui, rng, ae, asset);
        }
        if g.industry_num[asset] < 70 && 79 < pct {
            update_ctrl_since_in_misc_data(g, ui, rng, asset);
        }
        refresh_after_transfer(g, ui, rng, &mut f, time_secs);
    } else {
        // ---- corporate bonds (or the subsidiary's own bonds)
        let src_slot = find_bond_holding_slot(g, parent_id, asset);
        max_amount = g.hold_bond_face[(src_slot, parent_id)];
        msg_text = g.ent_symbol[parent_id].clone()
            + " owns"
            + &str_s(cast::<Ext, _>(max_amount))
            + " "
            + &g.million_prefix_lc
            + __content.string(9420)
            + &g.ent_symbol[asset]
            + __content.string(9421)
            + "\r\n"
            + "\r\n"
            + __content.string(9422)
            + &g.ent_symbol[asset]
            + __content.string(9423)
            + &g.ent_name[sub_id]
            + ":";
        aux_text = trim(str_s(cast::<Ext, _>(max_amount)).as_bytes());
        let title = s(__content.string(9416)) + &g.ent_symbol[asset] + __content.string(9424) + &g.ent_symbol[sub_id];
        loop {
            input_text = ui.inputbox(&msg_text, &title, &aux_text);
            amount = to_quad(input_val(&input_text));
            if !(max_amount < amount) {
                break;
            }
        }
        if amount < 1 {
            return;
        }
        f.transfer_basis = cast::<Ext, _>(amount) / cast::<Ext, _>(max_amount);
        f.transfer_basis = g.bond_hold_cost[(src_slot, parent_id)] * f.transfer_basis;
        if dest_slot < 1 {
            if dest_slot == -1 {
                g.bonds_issued[sub_id] = to_quad(cast::<Ext, _>(g.bonds_issued[sub_id]) - cast::<Ext, _>(amount));
                g.extraordinary_pl[sub_id] = to_quad(cast::<Ext, _>(amount) + cast::<Ext, _>(g.extraordinary_pl[sub_id]));
                if g.bonds_issued[sub_id] <= 0 {
                    g.bonds_issued[sub_id] = 0;
                    g.bond_issue_info[sub_id] = PbStr::new();
                }
            }
        } else {
            if g.hold_bond_id[(dest_slot, sub_id)] == 0 {
                g.hold_bond_id[(dest_slot, sub_id)] = asset;
            }
            g.hold_bond_face[(dest_slot, sub_id)] = to_quad(cast::<Ext, _>(amount) + cast::<Ext, _>(g.hold_bond_face[(dest_slot, sub_id)]));
            let market = sf(0.01f32) * g.bond_price[asset].get() * cast::<Ext, _>(amount);
            if f.transfer_basis < market {
                g.bond_hold_cost[(dest_slot, sub_id)] = f.transfer_basis + g.bond_hold_cost[(dest_slot, sub_id)];
            } else {
                g.bond_hold_cost[(dest_slot, sub_id)] = market + g.bond_hold_cost[(dest_slot, sub_id)];
            }
        }
        g.hold_bond_face[(src_slot, parent_id)] = to_quad(cast::<Ext, _>(g.hold_bond_face[(src_slot, parent_id)]) - cast::<Ext, _>(amount));
        g.bond_hold_cost[(src_slot, parent_id)] = g.bond_hold_cost[(src_slot, parent_id)] - f.transfer_basis;
        if g.hold_bond_face[(src_slot, parent_id)] < 1 || cast::<Ext, _>(amount) == cast::<Ext, _>(max_amount) {
            g.hold_bond_id[(src_slot, parent_id)] = 0;
            g.hold_bond_face[(src_slot, parent_id)] = 0;
            g.bond_hold_cost[(src_slot, parent_id)] = Ext::from_f64(0.0);
        }
        if g.bond_hold_cost[(src_slot, parent_id)] < Ext::from_f64(0.0) && g.hold_bond_face[(src_slot, parent_id)] == 0 {
            g.bond_hold_cost[(src_slot, parent_id)] = Ext::from_f64(0.0);
        }
        let ds = find_holding_slot(g, parent_id, sub_id);
        if 0 < ds {
            if asset == sub_id {
                g.hold_stock_basis[(ds, parent_id)] = cast::<Ext, _>(amount) + g.hold_stock_basis[(ds, parent_id)];
            } else {
                g.hold_stock_basis[(ds, parent_id)] = g.hold_stock_basis[(ds, parent_id)] + f.transfer_basis;
            }
        }
        msg_text = s(__content.string(9425)) + &g.ent_symbol[asset] + " to ";
        g.news_msg = g.ent_symbol[parent_id].clone() + &msg_text + &g.ent_symbol[sub_id];
        post_news(g, ui, 0, 0);
        aux_text = PbStr::new();
        if asset == sub_id {
            aux_text = s("\r") + "\r" + __content.string(9426) + &g.ent_name[sub_id] + ".)";
        }
        ui.msgbox(
            &(g.active_entity_name.clone() + &msg_text + &g.ent_name[sub_id] + "." + &aux_text),
            8192,
            &s(__content.string(9427)),
        );
        g.turn_action_count += 1;
        f.price_delta = cast::<Ext, _>(amount);
        refresh_after_transfer(g, ui, rng, &mut f, time_secs);
    }
}

/// Port of ContributeCapital_RefreshAfterTransfer (0x59ba3d), GOSUB of ContributeCapitalToSub.
/// Updates parent and subsidiary after a contribution: financials (player parent) or cross-ownership check
/// and projected earnings (corp parent); for the subsidiary cross-ownership, projected earnings, its
/// market cap raised by 0.9% of `price_delta` (`TmpExt3` = `price_delta`, index divisor, conversion price
/// and price alerts moved by it), a forced revaluation (`ValuationMode = 3`), a stock-split check and the
/// forced conversion of convertibles priced over 1,000; finally the cross-ownership check of the
/// transferred stock's/bond's company.
fn refresh_after_transfer(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut CcFrame, time_secs: i32) {
    let __content = crate::content::current();
    if f.parent_id < 11 {
        calc_entity_financials(g, ui, rng, f.parent_id);
    } else {
        check_cross_ownership(g, ui, rng, f.parent_id, 1);
        f.transfer_basis = calc_projected_earnings(g, ui, rng, f.parent_id);
    }
    let sub_id = f.sub_id;
    if 10 < sub_id {
        check_cross_ownership(g, ui, rng, sub_id, 1);
        f.transfer_basis = calc_projected_earnings(g, ui, rng, sub_id);
        g.market_cap[sub_id] = g.market_cap[sub_id] + f.price_delta * Ext::from_f64(0.009);
        g.tmp_ext3 = f.price_delta;
        adjust_index_divisor(g, sub_id, 0);
        adjust_conversion_price(g, sub_id, f.price_delta, Ext::from_f64(0.0));
        adjust_price_alerts_for_stock(g, sub_id, f.price_delta, Ext::from_f64(0.0));
        g.valuation_mode = 3;
        calc_stock_price(g, ui, rng, sub_id, time_secs);
        check_stock_split(g, sub_id);
        if 0 < g.bonds_issued[sub_id]
            && 0 < instr(1, g.bond_issue_info[sub_id].as_bytes(), b"cv")
            && Ext::from_f64(1000.0) < g.bond_price[sub_id].get()
        {
            convert_or_redeem_convertibles(g, ui, rng, sub_id, 0, 1);
            if g.bonds_issued[sub_id] == 0 {
                g.news_msg = s(__content.string(9428)) + &g.ent_symbol[sub_id] + __content.string(9429);
                post_news(g, ui, f.parent_id, sub_id);
            }
        }
    }
    if 10 < f.asset_corp_id {
        check_cross_ownership(g, ui, rng, f.asset_corp_id, 1);
    }
}

/// Port of ShowCapContribDialog (0x5dbaeb) with its control callbacks CapContribDlg_OptCash (0x5dc4fe),
/// CapContribDlg_OptBusinessAssets (0x5dc546), CapContribDlg_OptStock (0x5dc592), CapContribDlg_OptBonds
/// (0x5dc5de), CapContribDlg_BtnApply (0x5dc62a) and CapContribDlg_BtnCancel (0x5dc6a9).
/// The modal " [runtime resource wording] <subsidiary TmpExt1>" dialog with four option buttons (cash,
/// business assets, stock, bonds) and Apply/Cancel, shown through [`Ui::select`]. The choice comes back in
/// `TmpExt1` (1..4; 0 = cancelled or Apply without a selection); `DlgOpen_CapContrib` is 1 while it is open.
/// The dialog handle `hCapContribDlg` (`DIALOG NEW`) is not modelled.
pub fn show_cap_contrib_dialog(g: &mut Globals, ui: &mut dyn Ui, _h_parent: i32) {
    let __content = crate::content::current();
    let sub_id = to_long(g.tmp_ext1);
    let title = s(__content.string(9430)) + &g.ent_name[sub_id];
    adjust_dialog_frame_size(g.h_cap_contrib_dlg, 0x80c0_0882, 0);
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.dlg_open_cap_contrib = 1;
    let options = [
        s(__content.string(9431)),
        s(__content.string(9432)),
        s(__content.string(9433)),
        s(__content.string(9434)),
    ];
    let prompt = [s(__content.string(9435)), s(__content.string(9436))];
    match ui.select(&title, &prompt, &options, None, &s("Apply")) {
        Some(k) => {
            // option button k (CapContribDlg_Opt*) sets TmpExt1 = k + 1, then Apply
            g.tmp_ext1 = cast::<Ext, _>(k + 1);
            cap_contrib_dlg_btn_apply(g);
        }
        None => cap_contrib_dlg_btn_cancel(g),
    }
    g.dlg_open_cap_contrib = 0;
}

/// Port of CapContribDlg_BtnApply (0x5dc62a): keeps the selected type (`TmpExt1` 1..4, else 0) and ends
/// the dialog.
fn cap_contrib_dlg_btn_apply(g: &mut Globals) {
    let mut choice = to_long(g.tmp_ext1);
    if choice < 1 || 4 < choice {
        choice = 0;
    }
    g.tmp_ext1 = cast::<Ext, _>(choice);
}

/// Port of CapContribDlg_BtnCancel (0x5dc6a9): `TmpExt1 = 0` and ends the dialog.
fn cap_contrib_dlg_btn_cancel(g: &mut Globals) {
    g.tmp_ext1 = Ext::from_f64(0.0);
}

/// Port of DlgProcCapContribColors (0x5dbf2c).
/// Dialog procedure of the capital-contribution dialog: control colours (`WM_CTLCOLOR*`: the frame 509 in
/// colour 14 on 4, the label 510 in 0 on 15, the option buttons in 15 on 4, the dialog itself in 0 on 4 —
/// returns the background brush `BrushHandles(n)`) and the tooltips of Apply/Cancel (`WM_NOTIFY`). Pure
/// window plumbing; `ctrl_id` is `GetDlgCtrlID(lParam)` for the colour messages, `nm` the notification.
pub fn dlg_proc_cap_contrib_colors(g: &Globals, msg_id: i32, is_dialog: bool, ctrl_id: i32, nm: Option<&mut Nmhdr>) -> i32 {
    let __content = crate::content::current();
    let mut brush_ret = 0;
    match msg_id {
        // WM_CTLCOLORDLG
        310 => {
            if is_dialog {
                brush_ret = g.brush_handles[4];
            }
        }
        // WM_CTLCOLORMSGBOX..WM_CTLCOLORSTATIC
        306..=309 | 311 | 312 => {
            brush_ret = match ctrl_id {
                509 | 513 | 515 | 517 | 518 => g.brush_handles[4],
                510 => g.brush_handles[15],
                _ => 0,
            };
        }
        // WM_NOTIFY
        78 => {
            if let Some(nm) = nm {
                if is_tooltip_need_text(nm) {
                    match get_tooltip_ctrl_id(nm, |h| h) {
                        1 => set_tooltip_text(nm, &s(__content.string(9437))),
                        2 => set_tooltip_text(nm, &s(__content.string(9438))),
                        _ => {}
                    }
                }
                if is_tab_sel_change(nm) {
                    get_notify_hwnd_from(nm);
                }
            }
        }
        _ => {}
    }
    brush_ret
}

/// Port of UpdateCtrlSinceInMiscData (0x5288b9).
/// For company `entity_id` controlled since an earlier year (`CtrlSinceYear` set and not the current year)
/// with at least 32 `|` fields in `EntMiscData`: recomputes its control ([`update_corp_control`]); if a
/// company now owns 80%+ of it (`Parent80Owner > 10`), the old `CtrlSinceYear` is restored and field 28 of
/// `EntMiscData` set to that owner (the record is rebuilt with 41 fields).
pub fn update_ctrl_since_in_misc_data(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, entity_id: i32) {
    if g.ctrl_since_year[entity_id] == 0 || g.cur_year == g.ctrl_since_year[entity_id] {
        return;
    }
    let ctrl_year = g.ctrl_since_year[entity_id];
    if parse_count(g.ent_misc_data[entity_id].as_bytes(), b"|") < 32 {
        return;
    }
    update_corp_control(g, ui, rng, entity_id);
    if 10 < g.parent80_owner {
        g.ctrl_since_year[entity_id] = ctrl_year;
        let mut new_misc = PbStr::new();
        for field_idx in 1..=41 {
            let mut field = parse_any(g.ent_misc_data[entity_id].as_bytes(), b"|", field_idx);
            if field_idx == 28 {
                field = trim(PbStr::from(str_num(cast::<Ext, _>(g.parent80_owner), 7).as_str()).as_bytes());
            }
            new_misc = new_misc + &field + "|";
        }
        g.ent_misc_data[entity_id] = new_misc;
    }
}
