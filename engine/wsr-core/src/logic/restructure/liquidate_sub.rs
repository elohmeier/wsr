//! `LiquidateSubIntoParent` (0x512aa9) with its GOSUB blocks, and the transfers it uses
//! (`TransferFuturesOnLiquidation`, `TransferPhysCommodOnLiquidation`, `TransferEtfContracts`).
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::prelude::*;
use crate::logic::futures::recalc_commod_margin_balances;
use crate::logic::reports_corp::consolidate_phys_commod_recs;
use crate::logic::control::{find_option_positions, update_corp_control};
use crate::logic::earnings::balance_cash_t_bills;
use crate::logic::economy::close_all_dialogs;
use crate::logic::etf_derivs::etf_mgmt::etf_liquidate_bonds_on_disqualify;
use crate::logic::finance::{calc_entity_financials, get_controller_type, get_phys_commod_value};
use crate::logic::futures::count_futures_positions;
use crate::logic::holdings::find_holding_slot;
use crate::logic::market::{rebuild_streaming_quotes_panel, select_target_corp};
use crate::logic::newgame_helpers::{clear_entity_record, create_bank, create_insurer};
use crate::logic::options::{close_option_positions, transfer_options_holder};
use crate::logic::report::show_quick_message;
use crate::logic::reports_corp::divest_stock;
use crate::logic::restructure::capital::update_ctrl_since_in_misc_data;
use crate::logic::sim_entity::bonds::{call_bonds, force_regulated_call_sales, normalize_holding_pcts, scale_earnings_history};
use crate::logic::sim_entity::corp_ops::bank_allocate_assets;
use crate::logic::sim_entity::liquidation::{clear_all_holdings, liquidate_all_phys_commod};
use crate::logic::sim_entity::quarter_start::{cancel_pending_merger, pack_holding_arrays};
use crate::logic::sim_entity::transactions::liquidate_futures_positions;
use crate::logic::swaps::transfer_entity_swaps;
use crate::logic::valuation::{adjust_index_divisor, calc_stock_price};
use crate::pb::string::{CaseMode, lcase};

/// Port of TransferEtfContracts (0x64e7d1).
/// Hands every active fund (1581..1600, industry 71) advised by `from_id` to adviser `to_id` and posts a
/// headline if any was moved.
pub fn transfer_etf_contracts(g: &mut Globals, ui: &mut dyn Ui, to_id: i32, from_id: i32) {
    let __content = crate::content::current();
    let mut moved = false;
    for etf_id in 1581..=1600 {
        if g.fund_adviser[etf_id] == from_id && g.industry_num[etf_id] == 71 && g.entity_status[etf_id] == 1 {
            g.fund_adviser[etf_id] = to_id;
            moved = true;
        }
    }
    if moved {
        g.news_msg = s(__content.string(9439))
            + &g.ent_symbol[from_id]
            + &s(__content.string(9440))
            + &g.ent_symbol[to_id];
        post_news(g, ui, to_id, from_id);
    }
}

/// Port of TransferPhysCommodOnLiquidation (0x663592).
/// Re-owns every physical-commodity record of `old_owner` (`"commod|owner|units|cost"`) to `new_owner`
/// (fields trimmed), posts a headline if any moved and consolidates the new owner's records.
pub fn transfer_phys_commod_on_liquidation(g: &mut Globals, ui: &mut dyn Ui, new_owner: i32, old_owner: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut any_moved = 0;
    for rec_idx in 1..=1000 {
        let rec = g.phys_commod_rec[rec_idx].clone();
        if rec.is_empty() {
            continue;
        }
        if val(parse_any(rec.as_bytes(), b"|", 2).as_bytes()) == cast::<Ext, _>(old_owner) {
            any_moved = 1;
            let commod = trim(parse_any(rec.as_bytes(), b"|", 1).as_bytes()) + &s("|");
            let units = trim(parse_any(rec.as_bytes(), b"|", 3).as_bytes()) + &s("|");
            let cost = trim(parse_any(rec.as_bytes(), b"|", 4).as_bytes());
            g.phys_commod_rec[rec_idx] = commod + &trim(str_s(cast::<Ext, _>(new_owner)).as_bytes()) + &s("|") + &units + &cost;
        }
    }
    if any_moved == 1 {
        g.news_msg = s(__content.string(9441))
            + &g.ent_symbol[old_owner]
            + &s(__content.string(9442))
            + &g.ent_symbol[new_owner]
            + &s(__content.string(9443));
        post_news(g, ui, new_owner, old_owner);
    }
    consolidate_phys_commod_recs(g, new_owner);
}

/// Port of TransferFuturesOnLiquidation (0x62f493).
/// Moves the futures contracts of `from_id` to `to_id` (after recomputing `from_id`'s margin balance),
/// together with the margin balance — only if the receiver is no bank and has a credit rating above 6;
/// an insurer (industry 2) takes only stock-index (`STOCKS`) contracts. Returns the number moved.
pub fn transfer_futures_on_liquidation(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, from_id: i32, to_id: i32) -> i32 {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let from_ent = from_id;
    let to_ent = to_id;
    let owner_fld = trim(str_s(cast::<Ext, _>(to_id)).as_bytes()) + &s("|");
    recalc_commod_margin_balances(g, ui, rng, from_ent);
    let mut result = 0;
    if g.industry_num[to_ent] != 1 && 6 < g.credit_rating_num[to_ent] {
        let mut moved_count = 0;
        let mut commod = PbStr::new();
        for rec_idx in 1..=1000 {
            let fut_rec = g.futures_recs[rec_idx].clone();
            if fut_rec.is_empty() {
                continue;
            }
            if val(parse_any(fut_rec.as_bytes(), b"|", 2).as_bytes()) == cast::<Ext, _>(from_ent) {
                if g.industry_num[to_ent] == 2 && parse_any(fut_rec.as_bytes(), b"|", 1) != s("STOCKS") {
                    continue;
                }
                moved_count += 1;
                commod = parse_any(fut_rec.as_bytes(), b"|", 1);
                let p = |n| parse_any(fut_rec.as_bytes(), b"|", n);
                g.futures_recs[rec_idx] = commod.clone()
                    + &s("|")
                    + &owner_fld
                    + &(p(3) + &s("|"))
                    + &(p(4) + &s("|"))
                    + &(p(5) + &s("|"))
                    + &(p(6) + &s("|"))
                    + &p(7);
            }
        }
        if 0 < moved_count {
            let m = g.commod_margin_bal[from_ent];
            g.commod_margin_bal[to_ent] = g.commod_margin_bal[to_ent] + m;
            g.commod_margin_bal[from_ent] = Ext::from_f64(0.0);
            g.news_msg = g.ent_symbol[from_ent].clone()
                + &s(__content.string(9444))
                + &g.ent_symbol[to_ent]
                + &s(__content.string(9443));
            if moved_count == 1 {
                let mut commod_lc = lcase(commod.as_bytes(), CaseMode::Ansi);
                if commod == s("STOCKS") {
                    commod_lc = s(__content.string(2547));
                }
                g.news_msg = replace(g.news_msg.as_bytes(), __content.bytes(9445), (commod_lc + &s(__content.string(2538))).as_bytes());
            }
            post_news(g, ui, to_ent, from_ent);
        }
        result = moved_count;
    }
    result
}

/// Port of LiquidateSubIntoParent (0x512aa9).
/// Non [runtime resource wording] subsidiary `sub` into its parent `parent`: all assets, liabilities, holdings,
/// tax attributes, bank/insurer books, futures, commodities, swaps, options and ETF advisory contracts move
/// to the parent, `sub` becomes inactive (status 2, record cleared) and a free bank/insurer slot is
/// refilled with a new company.
///
/// The mode is passed in `TmpExt1` like in the original (read and cleared at entry): 0 = interactive
/// (player menu: confirmations and warnings through `ui`), 1/2 = automatic (computer, merger; 2 also
/// does not count a turn action). Exits early (nothing or only some preparatory steps done) when a
/// regulator, a bondholder, the player or the hold-slot limit blocks it.
// PB locals start zeroed; the port keeps them initialised like the original.
#[allow(unused_assignments)]
pub fn liquidate_sub_into_parent(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, parent_arg: i32, sub_arg: i32, time_secs: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError("LiquidateSubIntoParent", …) and exit (error path not ported)
    let mut subsidiary_list = [0i32; 16];
    let mut also_word = PbStr::new();
    let mut succeed_msg = PbStr::new();
    let mut tax_loss_msg = PbStr::new();
    let mut title_prefix = PbStr::new();
    let mut msg_title = PbStr::new();
    let mut msg_part;
    let mut msg_text = PbStr::new();
    let mut synergy = 0;
    let mut conv_in_money = 0;
    let mut is_convertible = 0;
    let mut hold_slot: i16 = 0;
    let mut stock_pct: i16;
    let mut divest_stock_id: i32 = 0;
    let mut answer: i32;
    let mut parent_id: i32;
    let mut benefit_owner_id: i32 = 0;
    let mut bonds_resolved: i32 = 0;
    let mut parent_bond_price: f32 = cast::<f32, _>(Ext::from_f32(0.0f32));
    let mut parent_unused_credit: Ext = Ext::from_f64(0.0);
    let mut parent_bonds_public: i32 = 0;
    let mut sub_bond_price: f32 = cast::<f32, _>(Ext::from_f32(0.0f32));
    let mut lost_tax_flags: i16 = 0;
    let mut sub_unused_credit: Ext = Ext::from_f64(0.0);
    let mut sub_bonds_public: i32 = 0;
    let mut credit_flag: i32 = 0;
    let mut loss_flag: i32;
    let mut sub_id: i32;
    let mut blocking_issuer_id: i32 = 0;

    let auto_mode = to_long(g.tmp_ext1);
    g.tmp_ext1 = Ext::from_f64(0.0);
    let sub_name = g.ent_name[sub_arg].clone();
    let parent_name = g.ent_name[parent_arg].clone();
    let sub_industry = g.industry_num[sub_arg];
    let er = g.exchange_rate;
    if sub_industry < 3 && g.industry_num[parent_arg] != sub_industry {
        return;
    }
    if parent_arg == sub_arg {
        return;
    }
    if sub_industry == 1 {
        calc_entity_financials(g, ui, rng, sub_arg);
        if auto_mode == 0 && g.industry_num[parent_arg] == 1 {
            msg_part = g.country_name[g.country_idx[sub_arg]].clone() + &s(__content.string(9446));
            if g.earnings_y1[sub_arg] == Ext::from_f64(0.0)
                && g.earnings_yr2_ago[sub_arg] == Ext::from_f64(0.0)
                && g.earnings_yr3_ago[sub_arg] == Ext::from_f64(0.0)
                && g.earnings_yr4_ago[sub_arg] == Ext::from_f64(0.0)
            {
                msg_text = msg_part.clone()
                    + &s(__content.string(9447))
                    + &s(__content.string(9448));
            } else if g.bank_merger_player < 0 && g.bank_merger_player.abs() == g.cur_player {
                msg_text = msg_part.clone()
                    + &s(__content.string(9449))
                    + &g.cur_player_name
                    + &s(__content.string(9450));
            }
            if g.country_idx[sub_arg] != g.country_idx[parent_arg] && !msg_text.is_empty() {
                msg_text = msg_text
                    + &s(__content.string(9451))
                    + &s(__content.string(9452))
                    + &parent_name;
            }
            if !msg_text.is_empty() {
                show_quick_message(g, ui, &mut msg_text);
                return;
            }
        }
    }
    if auto_mode == 0 {
        hold_slot = cast::<i16, _>(find_holding_slot(g, parent_arg, sub_arg));
        if g.hold_stock_basis[(cast::<i32, _>(hold_slot), parent_arg)] <= g.market_cap[sub_arg] * Ext::from_f64(120.0) {
            msg_text = PbStr::new();
            'synergy: {
                if 2 < sub_industry && sub_industry < 70 && g.industry_num[parent_arg] == sub_industry {
                    if g.cur_quarter + 4 < cast::<i32, _>(g.profit_margin[sub_arg])
                        && (cast::<i32, _>(g.profit_margin[parent_arg])) + 3 < cast::<i32, _>(g.profit_margin[sub_arg])
                    {
                        break 'synergy;
                    }
                    if g.business_assets[sub_arg] <= g.business_assets[parent_arg]
                        && g.profit_margin[sub_arg] < g.profit_margin[parent_arg]
                    {
                        break 'synergy;
                    }
                    msg_text = s(__content.string(9453))
                        + &sub_name
                        + &s(" into ")
                        + &parent_name
                        + &s(__content.string(9454))
                        + &s(__content.string(9455))
                        + &s(__content.string(9456));
                    synergy = 1;
                }
            }
            msg_text = msg_text + &s(__content.string(9457)) + &sub_name + &s(" INTO ") + &parent_name;
            msg_title = s(__content.string(9458)) + &sub_name;
        } else {
            let sub_sym = g.ent_symbol[sub_arg].clone();
            msg_text = parent_name.clone()
                + &s(__content.string(9459))
                + &s(__content.string(9460))
                + &sub_name
                + &s(" (")
                + &sub_sym
                + &s(")");
            if sub_industry < 3 {
                msg_text = msg_text + &s(".\r\r");
            } else {
                msg_text = msg_text
                    + &s(__content.string(9461))
                    + &sub_sym
                    + &s(__content.string(9462));
                if g.market_cap[sub_arg] * Ext::from_f64(125.0) < g.net_worth
                    && g.hold_stock_basis[(cast::<i32, _>(hold_slot), parent_arg)] < g.market_cap[sub_arg] * Ext::from_f64(130.0)
                {
                    msg_text = msg_text
                        + &s(__content.string(9463))
                        + &sub_sym
                        + &s(__content.string(9464))
                        + &s(__content.string(9465));
                }
            }
            if 2 < sub_industry && sub_industry != 70 {
                let cur = trim(g.currency_symbol.as_bytes());
                msg_part = s(__content.string(9466))
                    + &sub_sym
                    + &s(" can ")
                    + &parent_name
                    + &s(__content.string(9467))
                    + &sub_sym
                    + &s("'s ");
                if -Ext::from_f64(25.0) * er <= g.tax_loss_carry[sub_arg] {
                    if er * Ext::from_f64(10.0) < g.tax_credits[sub_arg] {
                        title_prefix = s("Gain / ");
                        msg_part = msg_part
                            + &s(__content.string(9468))
                            + &cur
                            + &fmt_trim(g.tax_credits[sub_arg], b"* #################")
                            + &s(" ")
                            + &g.million_prefix_lc
                            + &s("illion.\r\r");
                    } else {
                        title_prefix = PbStr::new();
                        msg_part = PbStr::new();
                    }
                } else {
                    title_prefix = s("Gain / ");
                    msg_part = msg_part
                        + &s(__content.string(9469))
                        + &cur
                        + &fmt_trim(g.tax_loss_carry[sub_arg].abs(), b"* #################")
                        + &s(" ")
                        + &g.million_prefix_lc
                        + &s("illion");
                    if er * Ext::from_f64(10.0) < g.tax_credits[sub_arg] {
                        msg_part = msg_part
                            + &s(__content.string(9470))
                            + &cur
                            + &fmt_trim(g.tax_credits[sub_arg], b"* #################")
                            + &s(" ")
                            + &g.million_prefix_lc
                            + &s("illion.\r\r");
                    } else {
                        msg_part = msg_part + &s(".\r\r");
                    }
                }
                if g.industry_num[parent_arg] == 70 {
                    loss_flag = 0;
                    if g.tax_loss_carry[parent_arg] < -Ext::from_f64(25.0) * er {
                        loss_flag = 1;
                    }
                    if er * Ext::from_f64(10.0) < g.tax_credits[parent_arg] {
                        loss_flag += 2;
                    }
                    if 0 < loss_flag {
                        if msg_part.is_empty() {
                            msg_part = msg_part + &s(__content.string(9471));
                        } else {
                            msg_part = msg_part + &s(__content.string(9472));
                        }
                        msg_part = msg_part
                            + &s(__content.string(9473))
                            + &g.ent_symbol[parent_arg]
                            + &s(__content.string(9474));
                        if loss_flag == 1 {
                            msg_part = msg_part + &s(__content.string(9475));
                        }
                        if loss_flag == 2 {
                            msg_part = msg_part + &s(__content.string(9476));
                        }
                        if loss_flag == 3 {
                            msg_part = msg_part + &s(__content.string(9477));
                        }
                        msg_part = msg_part + &s(__content.string(9478));
                        msg_title = PbStr::new();
                        if loss_flag == 1 {
                            msg_title = s(__content.string(9479))
                                + &cur
                                + &fmt_trim(g.tax_loss_carry[parent_arg].abs(), b"* #################")
                                + &s(" ")
                                + &g.million_prefix_lc
                                + &s("illion");
                        }
                        if loss_flag == 2 {
                            msg_title = s(__content.string(9480))
                                + &cur
                                + &fmt_trim(g.tax_credits[parent_arg], b"* #################")
                                + &s(" ")
                                + &g.million_prefix_lc
                                + &s("illion.");
                        }
                        if msg_title.is_empty() {
                            msg_part = msg_part + &s("\r\r");
                        } else {
                            msg_part = msg_part + &g.ent_symbol[parent_arg] + &s(__content.string(8975)) + &msg_title + &s("\r\r");
                        }
                    }
                }
                msg_text = msg_text + &msg_part;
            }
            msg_text = msg_text + &s(__content.string(9481)) + &sub_sym;
            if sub_industry < 3 {
                msg_text = replace(msg_text.as_bytes(), b"NON-tax", b"non-tax");
                msg_title = s(__content.string(9482)) + &sub_sym + &s(__content.string(9483));
            } else {
                msg_title = title_prefix.clone() + &s(__content.string(9484)) + &sub_sym + &s(__content.string(9485));
            }
        }
        answer = ui.msgbox(&(msg_text.clone() + &s("?")), 8484, &msg_title);
        if answer != 6 {
            return;
        }
    } else if rng.rnd() * Ext::from_f64(25.0) + Ext::from_f64(25.0) < g.market_cap[sub_arg] && Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
        return;
    }
    if sub_industry == 70 || g.industry_num[parent_arg] == 70 {
        loss_flag = 0;
        if sub_industry != 70 {
            if -Ext::from_f64(25.0) * er <= g.tax_loss_carry[parent_arg] {
                if er * Ext::from_f64(10.0) < g.tax_credits[parent_arg] {
                    credit_flag = 1;
                    benefit_owner_id = parent_arg;
                }
            } else {
                loss_flag = 1;
                benefit_owner_id = parent_arg;
                if er * Ext::from_f64(10.0) < g.tax_credits[parent_arg] {
                    credit_flag = 1;
                }
            }
        }
        if sub_industry == 70 {
            if -Ext::from_f64(25.0) * er <= g.tax_loss_carry[sub_arg] {
                if er * Ext::from_f64(10.0) < g.tax_credits[sub_arg] {
                    credit_flag = 1;
                    benefit_owner_id = sub_arg;
                }
            } else {
                loss_flag = 1;
                benefit_owner_id = sub_arg;
                if er * Ext::from_f64(10.0) < g.tax_credits[sub_arg] {
                    credit_flag = 1;
                }
            }
        }
        if auto_mode == 0 {
            title_prefix = PbStr::new();
            msg_text = s(__content.string(9486)) + &g.ent_symbol[sub_arg] + &s(__content.string(9487));
            if parent_arg == benefit_owner_id {
                msg_title = g.ent_name[parent_arg].clone();
            }
            if sub_arg == benefit_owner_id {
                msg_title = g.ent_name[sub_arg].clone();
            }
            if loss_flag < 1 || credit_flag != 0 {
                if loss_flag == 0 && 0 < credit_flag {
                    msg_title = msg_title
                        + &s(__content.string(3431))
                        + &g.currency_symbol
                        + &fmt_trim(g.tax_credits[benefit_owner_id].abs(), b"#################")
                        + &s(" ")
                        + &g.million_prefix_lc
                        + &s("illion")
                        + &g.currency_suffix;
                    msg_title = msg_title + &s(".\r\r");
                    msg_text = msg_text + &s(__content.string(9480)) + &msg_title;
                    title_prefix = s("Credits");
                } else if 0 < loss_flag && 0 < credit_flag {
                    msg_title = msg_title + &s(".\r\r");
                    msg_text = msg_text + &s(__content.string(14417)) + &msg_title;
                    title_prefix = s(__content.string(9488));
                }
            } else {
                msg_title = msg_title
                    + &s(__content.string(3431))
                    + &g.currency_symbol
                    + &fmt_trim(g.tax_loss_carry[benefit_owner_id].abs(), b"#################")
                    + &s(" ")
                    + &g.million_prefix_lc
                    + &s("illion")
                    + &g.currency_suffix;
                msg_title = msg_title + &s(".\r\r");
                msg_text = msg_text + &s(__content.string(9479)) + &msg_title;
                title_prefix = s(__content.string(9489));
            }
            if !title_prefix.is_empty() {
                answer = ui.msgbox(
                    &(msg_text.clone() + &s(__content.string(9490))),
                    8483,
                    &(s(__content.string(9491)) + &title_prefix),
                );
                if answer != 6 {
                    g.action_code = s("NOL");
                    return;
                }
            }
        }
    }
    pack_holding_arrays(g, ui);
    if sub_industry < 3 && 15 < g.hold_bond_id[(0, sub_arg)] + g.hold_bond_id[(0, parent_arg)] {
        msg_text = s(__content.string(3198));
        if sub_industry == 2 {
            msg_text = s(__content.string(3199));
        }
        let mut m = s(__content.string(9492))
            + &sub_name
            + &s(__content.string(9493))
            + &s(__content.string(9494))
            + &msg_text
            + &s(".'");
        show_quick_message(g, ui, &mut m);
        if g.hold_bond_id[(0, sub_arg)] + g.hold_bond_id[(0, parent_arg)] == 16 {
            benefit_owner_id = g.hold_bond_id[(1, sub_arg)];
            credit_flag = sub_arg;
            if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                benefit_owner_id = g.hold_bond_id[(1, parent_arg)];
                credit_flag = parent_arg;
            }
            let mut m = s(__content.string(9495))
                + &s(__content.string(9496))
                + &g.ent_symbol[credit_flag]
                + &s(" were ")
                + &s(__content.string(9497))
                + &g.ent_name[benefit_owner_id]
                + &s(__content.string(9498));
            show_quick_message(g, ui, &mut m);
        }
        return;
    }
    let mut hold_count = -1;
    for k in 1..=15 {
        if 0 < g.hold_stock_id[(k, parent_arg)] {
            hold_count += 1;
        }
        if 0 < g.hold_stock_id[(k, sub_arg)] {
            hold_count += 1;
        }
    }
    for i in 1..=15 {
        if 0 < g.hold_stock_id[(i, parent_arg)] {
            let stock_id = g.hold_stock_id[(i, parent_arg)];
            for j in 1..=15 {
                if g.hold_stock_id[(j, sub_arg)] == stock_id {
                    hold_count -= 1;
                }
            }
        }
    }
    if 0 < auto_mode && 15 < hold_count {
        return;
    }
    let mut rd = RequireDivest { auto_mode, also_word: &mut also_word, sub_name: &sub_name, parent_name: &parent_name, sub_arg };
    while 15 < hold_count {
        for k in 1..=15 {
            if 0 < g.hold_stock_id[(k, sub_arg)] {
                divest_stock_id = cast::<i32, _>(g.hold_stock_id[(k, sub_arg)]);
            }
        }
        if rd.run(g, ui, rng, divest_stock_id) {
            return;
        }
        hold_count -= 1;
    }
    benefit_owner_id = find_option_positions(g, ui, sub_arg, parent_arg, 0, 0);
    if auto_mode == 0 && benefit_owner_id != 0 {
        msg_title = PbStr::new();
        if benefit_owner_id == -51 {
            msg_title = s("short ");
        }
        msg_text = sub_name.clone()
            + &s(" has ")
            + &msg_title
            + &s(__content.string(9499))
            + &parent_name
            + &s(__content.string(9500))
            + &sub_name
            + &s(__content.string(9501));
        answer = ui.msgbox(&msg_text, 8484, &s(__content.string(9502)));
        if answer == 7 {
            return;
        }
    }
    if benefit_owner_id != 0 {
        let mut opt_remaining = 1;
        while opt_remaining != 0 {
            opt_remaining = find_option_positions(g, ui, sub_arg, parent_arg, 0, 0);
            g.tmp_ext1 = Ext::from_f64(0.0);
            g.tmp_ext2 = Ext::from_f64(0.0);
            close_option_positions(g, ui, rng, sub_arg, parent_arg, 0, 0);
        }
    }
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    if benefit_owner_id != 0 {
        g.news_msg = g.ent_symbol[sub_arg].clone()
            + &s(__content.string(9503))
            + &g.ent_symbol[parent_arg]
            + &s(__content.string(9504));
    }
    post_news(g, ui, 0, 0);
    if g.industry_num[parent_arg] < 3 && get_phys_commod_value(g, sub_arg, &PbStr::new()) > Ext::from_f64(0.0) {
        if auto_mode == 0 {
            msg_title = s("Banking");
            if g.industry_num[parent_arg] == 2 {
                msg_title = s(__content.string(2878));
            }
            msg_text = sub_name.clone()
                + &s(__content.string(9505))
                + &msg_title
                + &s(__content.string(9506))
                + &g.ent_symbol[parent_arg]
                + &s(__content.string(9507))
                + &s(__content.string(9508));
            answer = ui.msgbox(&msg_text, 8484, &s(__content.string(9509)));
            if answer != 6 {
                return;
            }
        }
        liquidate_all_phys_commod(g, ui, rng, sub_arg);
        if auto_mode == 0 {
            let mut m = s(__content.string(9510)) + &sub_name + &s(__content.string(9511));
            show_quick_message(g, ui, &mut m);
        }
    }
    for k in 1..=15 {
        if cast::<i32, _>(g.hold_stock_id[(k, sub_arg)]) == parent_arg {
            if 2 < g.industry_num[parent_arg] && g.hold_stock_pct[(k, sub_arg)] < 10 {
                continue;
            }
            divest_stock_id = cast::<i32, _>(g.hold_stock_id[(k, sub_arg)]);
            if rd.run(g, ui, rng, divest_stock_id) {
                return;
            }
            break;
        }
    }
    if 1580 < sub_arg && 0 < g.hold_bond_id[(0, sub_arg)] {
        etf_liquidate_bonds_on_disqualify(g, ui, rng, sub_arg);
    }

    // ---- the bonds of the subsidiary (and the parent)
    'bonds: {
        loop {
            parent_id = parent_arg;
            sub_id = sub_arg;
            if g.bonds_issued[sub_id] == 0 {
                break 'bonds;
            }
            calc_entity_financials(g, ui, rng, sub_id);
            sub_bonds_public = to_long(cast::<Ext, _>(g.bonds_public_held));
            sub_bond_price = cast::<f32, _>(g.bond_price[sub_id].get());
            sub_unused_credit = g.unused_credit_line;
            if 0 < instr(1, g.bond_issue_info[sub_id].as_bytes(), b"cv") {
                is_convertible = 1;
                let conv_price = val(parse_any(g.bond_issue_info[sub_id].as_bytes(), b"|", 2).as_bytes());
                if conv_price < g.market_cap[sub_id] && Ext::from_f32(105.0f32) < sub_bond_price {
                    conv_in_money = 1;
                }
            }
            if g.bonds_issued[sub_id] <= 0 {
                bonds_resolved = 1;
            } else {
                bonds_resolved = 0;
                if g.bonds_issued[parent_id] == 0 && is_convertible == 0 {
                    g.bonds_issued[parent_id] = g.bonds_issued[sub_id];
                    g.bond_issue_info[parent_id] = g.bond_issue_info[sub_id].clone();
                    if cast::<Ext, _>(g.bonds_issued[sub_id]) != cast::<Ext, _>(g.bonds_public_held) {
                        for i in 1..=1583 {
                            for j in 1..=15 {
                                if g.hold_bond_id[(j, i)] == sub_id {
                                    g.hold_bond_id[(j, i)] = parent_id;
                                    break;
                                }
                            }
                        }
                    }
                    clear_bonds(g, sub_id);
                    bonds_resolved = 1;
                    let mut coupon = right(g.bond_issue_info[parent_id].as_bytes(), 5);
                    if mid(coupon.as_bytes(), 1, 1) == s("0") {
                        coupon = right(coupon.as_bytes(), 4);
                    }
                    g.news_msg = g.ent_symbol[parent_id].clone()
                        + &s(__content.string(9512))
                        + &coupon
                        + &s(__content.string(9513))
                        + &g.ent_symbol[sub_id]
                        + &s(".");
                    post_news(g, ui, 0, 0);
                } else {
                    credit_flag = g.credit_rating_num[sub_id];
                    calc_entity_financials(g, ui, rng, parent_id);
                    parent_bonds_public = to_long(cast::<Ext, _>(g.bonds_public_held));
                    parent_bond_price = cast::<f32, _>(g.bond_price[parent_id].get());
                    parent_unused_credit = g.unused_credit_line;
                    if g.credit_rating_num[parent_id] < credit_flag {
                        blocking_issuer_id = sub_id;
                    } else {
                        blocking_issuer_id = parent_id;
                    }
                    if g.bonds_issued[sub_id] != cast::<i64, _>(sub_bonds_public) || Ext::from_f32(120.0f32) < sub_bond_price {
                        if conv_in_money == 0 {
                            call_in_bonds(g, ui, rng, parent_id, sub_id, sub_id, sub_bond_price, &mut bonds_resolved, &mut blocking_issuer_id, 1);
                        }
                    } else {
                        if Ext::from_f32(105.0f32) < sub_bond_price && conv_in_money == 0 {
                            sub_bond_price = 105.0f32;
                        }
                        let buyback_cost = sf(0.01f32) * cast::<Ext, _>(sub_bonds_public) * cast::<Ext, _>(sub_bond_price);
                        if g.t_bills[sub_id] + (sub_unused_credit + g.cash[sub_id]) < buyback_cost
                            && buyback_cost
                                <= g.t_bills[parent_id] + g.t_bills[sub_id] + g.cash[parent_id] + (sub_unused_credit + g.cash[sub_id])
                        {
                            g.cash[sub_id] = g.cash[sub_id] + g.cash[parent_id];
                            g.cash[parent_id] = Ext::from_f64(0.0);
                            g.t_bills[sub_id] = g.t_bills[sub_id] + g.t_bills[parent_id];
                            g.t_bills[parent_id] = Ext::from_f64(0.0);
                        }
                        if buyback_cost <= g.t_bills[sub_id] + (sub_unused_credit + g.cash[sub_id]) {
                            buy_back_bonds(g, sub_id, sub_bonds_public, buyback_cost);
                            if bonds_resolved < 2 {
                                g.news_msg = g.ent_symbol[sub_id].clone()
                                    + &s(__content.string(9514))
                                    + &g.ent_symbol[parent_id];
                                post_news(g, ui, 0, 0);
                            }
                            bonds_resolved = 1;
                        }
                    }
                    if g.bonds_issued[parent_id] == cast::<i64, _>(parent_bonds_public) && bonds_resolved == 0 && is_convertible == 0 {
                        if Ext::from_f32(105.0f32) < parent_bond_price {
                            parent_bond_price = 105.0f32;
                        }
                        let buyback_cost = sf(0.01f32) * cast::<Ext, _>(parent_bonds_public) * cast::<Ext, _>(parent_bond_price);
                        if buyback_cost < g.t_bills[parent_id] + (parent_unused_credit + g.cash[parent_id]) {
                            buy_back_bonds(g, parent_id, parent_bonds_public, buyback_cost);
                            g.news_msg = g.ent_symbol[parent_id].clone()
                                + &s(__content.string(9515))
                                + &g.ent_symbol[sub_id];
                            post_news(g, ui, 0, 0);
                            bonds_resolved = 2;
                        }
                    } else if bonds_resolved == 0 && is_convertible == 0 {
                        call_in_bonds(g, ui, rng, parent_id, parent_id, sub_id, parent_bond_price, &mut bonds_resolved, &mut blocking_issuer_id, 2);
                    }
                    if bonds_resolved == 0
                        && g.bonds_issued[sub_id] == cast::<i64, _>(sub_bonds_public)
                        && g.bonds_issued[parent_id] == cast::<i64, _>(parent_bonds_public)
                        && sub_bond_price < parent_bond_price
                        && (cast::<Ext, _>(parent_bond_price)) * sf(0.8f32) < cast::<Ext, _>(sub_bond_price)
                    {
                        g.bonds_issued[parent_id] = g.bonds_issued[parent_id] + g.bonds_issued[sub_id];
                        // BUGS.md #226: this headline is never posted (overwritten later)
                        g.news_msg = g.ent_symbol[sub_id].clone()
                            + &s(__content.string(9516))
                            + &g.ent_symbol[parent_id]
                            + &s(__content.string(9517));
                        clear_bonds(g, sub_id);
                        bonds_resolved = 1;
                    }
                }
            }
            if bonds_resolved != 2 {
                break;
            }
        }
        if bonds_resolved == 0
            && 0 < g.bonds_issued[parent_id]
            && 0 < g.bonds_issued[sub_id]
            && Ext::from_f32(0.0f32) < parent_bond_price
            && (cast::<Ext, _>(parent_bonds_public)) < cast::<Ext, _>(g.bonds_issued[parent_id]) * sf(0.5f32)
            && (cast::<Ext, _>(sub_bonds_public)) < cast::<Ext, _>(g.bonds_issued[sub_id]) * sf(0.5f32)
        {
            let mut swap_ratio: f32 = cast::<f32, _>(Ext::from_f32(1.0f32));
            if parent_bond_price < sub_bond_price || (cast::<Ext, _>(parent_bond_price)) > cast::<Ext, _>(sub_bond_price) * sf(1.1f32) {
                swap_ratio = sub_bond_price / parent_bond_price;
            }
            g.bonds_issued[parent_id] = to_quad(cast::<Ext, _>(g.bonds_issued[sub_id]) * cast::<Ext, _>(swap_ratio) + cast::<Ext, _>(g.bonds_issued[parent_id]));
            for h in 1..=1583 {
                for k in 1..=15 {
                    if g.hold_bond_id[(k, h)] == sub_id {
                        g.hold_bond_id[(k, h)] = parent_id;
                        g.hold_bond_face[(k, h)] = to_quad(cast::<Ext, _>(g.hold_bond_face[(k, h)]) * cast::<Ext, _>(swap_ratio));
                    }
                }
            }
            clear_bonds(g, sub_id);
            g.news_msg = g.ent_symbol[parent_id].clone()
                + &s(__content.string(9518))
                + &g.ent_symbol[sub_id]
                + &s(__content.string(9519))
                + &fmt_trim(cast::<Ext, _>(swap_ratio), b"#####.000")
                + &s(" to 1.");
            post_news(g, ui, 0, 0);
            bonds_resolved = 1;
            for h in 1..=1583 {
                hold_slot = 0;
                for k in 1..=15 {
                    if g.hold_bond_id[(k, h)] == parent_id {
                        if hold_slot < 1 {
                            hold_slot = cast::<i16, _>(k);
                        } else {
                            let hs = cast::<i32, _>(hold_slot);
                            g.hold_bond_face[(hs, h)] = g.hold_bond_face[(hs, h)] + g.hold_bond_face[(k, h)];
                            g.bond_hold_cost[(hs, h)] = g.bond_hold_cost[(hs, h)] + g.bond_hold_cost[(k, h)];
                            g.hold_bond_id[(k, h)] = 0;
                            g.hold_bond_face[(k, h)] = 0;
                            g.bond_hold_cost[(k, h)] = Ext::from_f64(0.0);
                        }
                    }
                }
            }
            calc_entity_financials(g, ui, rng, parent_id);
        }
        if bonds_resolved == 0 {
            if blocking_issuer_id == 0 {
                blocking_issuer_id = parent_id;
            }
            if 10 < blocking_issuer_id {
                g.news_msg = s(__content.string(9520))
                    + &g.ent_symbol[blocking_issuer_id]
                    + &s(__content.string(9521))
                    + &g.ent_symbol[sub_id]
                    + &s(__content.string(8933));
                if sub_id == blocking_issuer_id {
                    g.news_msg = g.ent_symbol[sub_id].clone()
                        + &s(__content.string(9522))
                        + &g.ent_symbol[parent_id];
                }
            }
            if is_convertible == 1 {
                g.news_msg = s(__content.string(9523)) + &g.ent_symbol[sub_id] + &s(__content.string(9524));
            }
            if blocking_issuer_id < 0 {
                g.news_msg = s(__content.string(9486))
                    + &g.ent_symbol[sub_id]
                    + &s(" into ")
                    + &g.ent_symbol[parent_arg]
                    + &s(__content.string(9525));
                msg_text = g.ent_symbol[sub_id].clone() + &s(" and ") + &g.ent_symbol[parent_arg] + &s(".");
            } else {
                if is_convertible == 1 {
                    blocking_issuer_id = sub_id;
                }
                msg_text = g.ent_symbol[blocking_issuer_id].clone() + &s(".");
            }
            post_news(g, ui, 0, 0);
            if auto_mode == 0 {
                ui.msgbox(
                    &(s(__content.string(9486)) + &g.ent_symbol[sub_id] + &s(__content.string(9526)) + &msg_text),
                    8208,
                    &s(__content.string(9527)),
                );
            }
            return;
        }
    }

    // ---- LAB_005170c9: the transfer
    parent_id = parent_arg;
    sub_id = sub_arg;
    if auto_mode == 0
        && g.industry_num[parent_id] == 1
        && g.industry_num[sub_id] == 1
        && g.cur_player == g.corp_controller[parent_id]
    {
        g.bank_merger_player = -g.cur_player;
    }
    if cast::<i32, _>(g.scenario_corp) == sub_id {
        if cast::<i32, _>(g.cheat_eq_fund_target) == sub_id {
            g.cheat_eq_fund_target = 0;
            g.scenario_corp = 0;
            g.event_player = 0;
            g.scenario_stage = 0;
            g.scenario_num = 0;
            g.event_status[101] = 2;
        } else if (cast::<i32, _>(g.scenario_num)).abs() == 104 && g.event_status[104] == 1 && g.cheat_japan_target == -1 {
            g.scenario_corp = 0;
            g.event_player = 0;
            g.scenario_stage = 0;
            g.scenario_num = 0;
            g.event_status[104] = 0;
            g.cheat_japan_target = 0;
        }
    }
    let parent_short = cast::<i16, _>(parent_id);
    if cast::<i32, _>(g.scenario_corp) == sub_id && 0 < (cast::<i32, _>(g.scenario_num)).abs() {
        g.scenario_corp = parent_short;
    }
    if sub_id == g.malicious_suit_defendant {
        g.malicious_suit_defendant = parent_id;
    }
    if sub_id == g.malicious_suit_plaintiff {
        g.malicious_suit_plaintiff = parent_id;
    }
    if sub_id == g.antitrust_defendant {
        g.antitrust_defendant = parent_id;
    }
    if sub_id == g.antitrust_plaintiff {
        g.antitrust_plaintiff = parent_id;
    }
    if g.asbestos_corp.abs() == sub_id {
        g.asbestos_corp = parent_id;
    }
    if cast::<i32, _>(g.superfund_corp) == sub_id {
        g.superfund_corp = parent_short;
    }
    if 10000 < g.strike_corp_code && (cast::<i32, _>(g.strike_corp_code)) % 10000 == sub_id {
        g.strike_corp_code = parent_short.wrapping_add(20000);
    }
    if 2 < sub_industry && sub_industry < 70 {
        g.working_capital[parent_id] = g.working_capital[parent_id] + g.working_capital[sub_id];
        g.working_capital[sub_id] = Ext::from_f64(0.0);
    }
    g.cash[parent_id] = g.cash[parent_id] + g.cash[sub_id];
    g.cash[sub_id] = Ext::from_f64(0.0);
    g.t_bills[parent_id] = g.t_bills[parent_id] + g.t_bills[sub_id];
    g.t_bills[sub_id] = Ext::from_f64(0.0);
    g.dividend_per_share[sub_id] = Ext::from_f64(0.0);
    g.ent_misc_data[sub_id] = trim(str_s(cast::<Ext, _>(parent_id)).as_bytes());
    if sub_industry == 70 {
        if g.tax_loss_carry[sub_id] < -Ext::from_f64(25.0) {
            lost_tax_flags = 1;
        }
        g.tax_loss_carry[sub_id] = Ext::from_f64(0.0);
        if Ext::from_f64(10.0) < g.tax_credits[sub_id] {
            lost_tax_flags += 2;
        }
        g.tax_credits[sub_id] = Ext::from_f64(0.0);
    }
    if g.industry_num[parent_id] == 70 && sub_industry != 70 {
        if g.tax_loss_carry[parent_id] < -Ext::from_f64(25.0) {
            lost_tax_flags = 4;
        }
        g.tax_loss_carry[parent_id] = Ext::from_f64(0.0);
        if Ext::from_f64(10.0) < g.tax_credits[parent_id] {
            lost_tax_flags += 5;
        }
        g.tax_credits[parent_id] = Ext::from_f64(0.0);
    }
    if 0 < lost_tax_flags && lost_tax_flags < 4 {
        tax_loss_msg = s(__content.string(9528))
            + &g.ent_symbol[sub_id]
            + &s(__content.string(9529))
            + &g.ent_symbol[parent_id];
        if lost_tax_flags == 2 {
            tax_loss_msg = replace(tax_loss_msg.as_bytes(), __content.bytes(9350), b"credits");
        }
        if lost_tax_flags == 3 {
            tax_loss_msg = replace(tax_loss_msg.as_bytes(), b"overs", __content.bytes(9530));
        }
    }
    if 3 < lost_tax_flags {
        tax_loss_msg = g.ent_symbol[parent_id].clone()
            + &s(__content.string(9531))
            + &g.ent_symbol[sub_id];
        if lost_tax_flags == 5 {
            tax_loss_msg = replace(tax_loss_msg.as_bytes(), __content.bytes(9350), b"credits");
        }
        if lost_tax_flags == 9 {
            tax_loss_msg = replace(tax_loss_msg.as_bytes(), __content.bytes(9349), __content.bytes(9532));
        }
    }
    if -Ext::from_f64(25.0) <= g.tax_loss_carry[sub_id] {
        if Ext::from_f64(10.0) < g.tax_credits[sub_id] {
            succeed_msg = g.ent_symbol[parent_id].clone()
                + &s(__content.string(9533))
                + &g.ent_symbol[sub_id]
                + &s(__content.string(9534));
        }
    } else {
        succeed_msg = g.ent_symbol[parent_id].clone()
            + &s(__content.string(9533))
            + &g.ent_symbol[sub_id]
            + &s(__content.string(9535));
    }
    if (Ext::from_f64(0.0) < g.tax_loss_carry[parent_id] || Ext::from_f64(0.0) < g.tax_loss_carry[sub_id])
        && !(Ext::from_f64(0.0) <= g.tax_loss_carry[parent_id] && Ext::from_f64(0.0) <= g.tax_loss_carry[sub_id])
    {
        if Ext::from_f64(0.0) < g.tax_loss_carry[parent_id] && g.tax_loss_carry[sub_id] < Ext::from_f64(0.0) {
            g.tax_loss_carry[parent_id] = Ext::from_f64(0.0);
        }
        if g.tax_loss_carry[parent_id] < Ext::from_f64(0.0) && Ext::from_f64(0.0) < g.tax_loss_carry[sub_id] {
            g.tax_loss_carry[sub_id] = Ext::from_f64(0.0);
        }
    }
    let futures_count = count_futures_positions(g, sub_arg, 0);
    if 0 < futures_count {
        transfer_futures_on_liquidation(g, ui, rng, sub_arg, parent_arg);
        if 0 < count_futures_positions(g, sub_arg, 0) {
            let futures_closed = liquidate_futures_positions(g, ui, rng, sub_arg, 2, 1);
            msg_text = sub_name.clone() + &s(__content.string(9536)) + &parent_name;
            if futures_closed != futures_count {
                msg_text = replace(msg_text.as_bytes(), b"all", b"some");
            }
            msg_title = s(__content.string(9537));
            if 2 < g.industry_num[parent_arg] {
                msg_title = s(__content.string(9538));
            }
            msg_text = msg_text + &msg_title + &s(__content.string(9539));
            if auto_mode == 0 {
                show_quick_message(g, ui, &mut msg_text);
            }
            g.news_msg = g.ent_symbol[sub_arg].clone()
                + &s(__content.string(9540))
                + &g.ent_symbol[parent_arg];
            post_news(g, ui, 0, 0);
        }
        if g.credit_rating_num[parent_arg] < 5 {
            recalc_commod_margin_balances(g, ui, rng, parent_arg);
            answer = liquidate_futures_positions(g, ui, rng, parent_arg, 2, 1);
            if auto_mode == 0 && 0 < answer {
                let mut m = parent_name.clone() + &s(__content.string(9541));
                show_quick_message(g, ui, &mut m);
            }
        }
    }
    transfer_phys_commod_on_liquidation(g, ui, parent_arg, sub_arg);
    if g.merger_target == sub_arg || g.merger_acquirer == sub_arg {
        cancel_pending_merger(g, ui, sub_arg);
    }
    transfer_entity_swaps(g, ui, sub_id, parent_id);
    g.tax_loss_carry[parent_id] = g.tax_loss_carry[parent_id] + g.tax_loss_carry[sub_id];
    g.tax_loss_carry[sub_id] = Ext::from_f64(0.0);
    g.accrued_pl[parent_id] = g.accrued_pl[parent_id] + g.accrued_pl[sub_id];
    g.extraordinary_pl[parent_id] = g.extraordinary_pl[parent_id] + g.extraordinary_pl[sub_id];
    g.accrued_pl[sub_id] = Ext::from_f64(0.0);
    g.earnings_y1[sub_id] = Ext::from_f64(0.0);
    g.earnings_yr2_ago[sub_id] = Ext::from_f64(0.0);
    g.earnings_yr3_ago[sub_id] = Ext::from_f64(0.0);
    g.earnings_yr4_ago[sub_id] = Ext::from_f64(0.0);
    g.extraordinary_pl[sub_id] = 0;
    g.earnings_ytd[sub_id] = Ext::from_f64(0.0);
    if sub_industry == 1 || sub_industry == 2 {
        if g.market_cap[parent_id] < g.market_cap[sub_id] {
            g.profit_margin[parent_id] = g.profit_margin[sub_id];
        }
        g.long_gov_bond_face[parent_id] = g.long_gov_bond_face[parent_id] + g.long_gov_bond_face[sub_id];
        g.long_gov_bond_face[sub_id] = 0;
        g.lt_gov_bond_cost[parent_id] = g.lt_gov_bond_cost[parent_id] + g.lt_gov_bond_cost[sub_id];
        g.lt_gov_bond_cost[sub_id] = Ext::from_f64(0.0);
        g.short_gov_bond_face[parent_id] = g.short_gov_bond_face[parent_id] + g.short_gov_bond_face[sub_id];
        g.short_gov_bond_face[sub_id] = 0;
        g.st_gov_bond_cost[parent_id] = g.st_gov_bond_cost[parent_id] + g.st_gov_bond_cost[sub_id];
        g.st_gov_bond_cost[sub_id] = Ext::from_f64(0.0);
        if sub_industry == 1 {
            g.bank_demand_deposits[parent_id] = g.bank_demand_deposits[parent_id] + g.bank_demand_deposits[sub_id];
            g.bad_debt_reserve[parent_id] = g.bad_debt_reserve[parent_id] + g.bad_debt_reserve[sub_id];
            g.bank_consumer_loans[parent_id] = g.bank_consumer_loans[parent_id] + g.bank_consumer_loans[sub_id];
            g.bank_consumer_loans[sub_id] = 0;
            g.bank_mortgage_loans[parent_id] = g.bank_mortgage_loans[parent_id] + g.bank_mortgage_loans[sub_id];
            g.bank_mortgage_loans[sub_id] = 0;
            g.subprime_secs[parent_id] = g.subprime_secs[parent_id] + g.subprime_secs[sub_id];
            g.subprime_secs[sub_id] = 0;
            g.bank_c_ds[parent_id] = g.bank_c_ds[parent_id] + g.bank_c_ds[sub_id];
            g.bank_cust_deposits[parent_id] = g.bank_cust_deposits[parent_id] + g.bank_cust_deposits[sub_id];
            for e in 1..=1600 {
                if cast::<i32, _>(g.lending_bank[e]) == sub_id {
                    if e < 11 || 85 < e {
                        g.lending_bank[e] = cast::<i16, _>(parent_id);
                    } else if Ext::from_f64(0.0) < g.bank_debt[e] {
                        // BUGS.md #227: the borrowing bank keeps its loan; the parent is credited with it
                        if parent_id == e {
                            g.bank_debt[parent_id] = Ext::from_f64(0.0);
                        } else {
                            g.cash[parent_id] = g.cash[parent_id] + g.bank_debt[e];
                        }
                    }
                }
            }
            if cast::<i32, _>(g.lending_bank[sub_id]) == parent_id {
                g.bank_debt[sub_id] = Ext::from_f64(0.0);
            }
        } else {
            g.ins_reserves[parent_id] = g.ins_reserves[parent_id] + g.ins_reserves[sub_id];
            g.ins_reserves[sub_id] = Ext::from_f64(0.0);
            g.insur_uw_profit_ytd[parent_id] = g.insur_uw_profit_ytd[parent_id] + g.insur_uw_profit_ytd[sub_id];
            g.insur_uw_profit_ytd[sub_id] = Ext::from_f64(0.0);
            g.subprime_secs[parent_id] = g.subprime_secs[parent_id] + g.subprime_secs[sub_id];
            g.subprime_secs[sub_id] = 0;
        }
        for i in 1..=15 {
            let issuer = g.hold_bond_id[(i, parent_id)];
            if 0 < issuer {
                for j in 1..=15 {
                    if g.hold_bond_id[(j, sub_id)] == issuer {
                        g.hold_bond_face[(i, parent_id)] = g.hold_bond_face[(i, parent_id)] + g.hold_bond_face[(j, sub_id)];
                        g.bond_hold_cost[(i, parent_id)] = g.bond_hold_cost[(i, parent_id)] + g.bond_hold_cost[(j, sub_id)];
                        g.hold_bond_id[(j, sub_id)] = 0;
                        g.hold_bond_face[(j, sub_id)] = 0;
                        g.bond_hold_cost[(j, sub_id)] = Ext::from_f64(0.0);
                    }
                }
            }
        }
        for i in 1..=15 {
            if g.hold_bond_id[(i, parent_id)] < 1 {
                for j in 1..=15 {
                    if g.hold_bond_id[(j, sub_id)] != 0 {
                        g.hold_bond_id[(i, parent_id)] = g.hold_bond_id[(j, sub_id)];
                        g.hold_bond_face[(i, parent_id)] = g.hold_bond_face[(j, sub_id)];
                        g.bond_hold_cost[(i, parent_id)] = g.bond_hold_cost[(j, sub_id)];
                        g.hold_bond_id[(j, sub_id)] = 0;
                        g.hold_bond_face[(j, sub_id)] = 0;
                        g.bond_hold_cost[(j, sub_id)] = Ext::from_f64(0.0);
                        break;
                    }
                }
            }
        }
        for i in 1..=15 {
            if g.hold_bond_id[(i, parent_arg)] == parent_arg {
                g.bonds_issued[parent_arg] = g.bonds_issued[parent_arg] - g.hold_bond_face[(i, parent_arg)];
                g.hold_bond_id[(i, parent_arg)] = 0;
                g.hold_bond_face[(i, parent_arg)] = 0;
                g.bond_hold_cost[(i, parent_arg)] = Ext::from_f64(0.0);
            }
        }
    } else {
        if sub_industry < 70 {
            if g.business_assets[parent_id] <= Ext::from_f64(0.0) || g.business_assets[sub_id] <= Ext::from_f64(0.0) {
                if Ext::from_f64(1.0) < g.business_assets[sub_id] {
                    g.profit_margin[parent_id] = g.profit_margin[sub_id];
                    g.disc_spend_pct[parent_id] = g.disc_spend_pct[sub_id];
                }
            } else {
                if g.business_assets[parent_id] <= g.business_assets[sub_id] {
                    g.profit_margin[parent_id] = g.profit_margin[parent_id].wrapping_add(5);
                } else {
                    g.profit_margin[sub_id] = g.profit_margin[sub_id].wrapping_add(5);
                }
                let ba_p = g.business_assets[parent_id];
                let ba_s = g.business_assets[sub_id];
                let v = to_quad((ba_p * cast::<Ext, _>(g.profit_margin[parent_id]) + ba_s * cast::<Ext, _>(g.profit_margin[sub_id])) / (ba_p + ba_s));
                g.profit_margin[parent_id] = cast::<i16, _>(v);
            }
            if g.business_assets[parent_id] * Ext::from_f64(2.0) < g.business_assets[sub_id] {
                let throttled = get_controller_type(g, parent_id) == 1
                    && g.auto_pilot[parent_id] == 1
                    && g.player_growth_throttle[g.corp_controller[parent_id]] < g.growth_rate[sub_id];
                if !throttled {
                    g.growth_rate[parent_id] = g.growth_rate[sub_id];
                }
            }
            g.business_assets[parent_id] = g.business_assets[parent_id] + g.business_assets[sub_id];
            g.industry_num[parent_id] = sub_industry;
        }
        if parent_id < 131 {
            g.extraordinary_pl[parent_id] = to_quad(cast::<Ext, _>(g.extraordinary_pl[parent_id]) - g.goodwill[sub_id]);
        } else {
            g.goodwill[parent_id] = g.goodwill[parent_id] + g.goodwill[sub_id];
        }
        g.goodwill[sub_id] = Ext::from_f64(0.0);
        g.business_assets[sub_id] = Ext::from_f64(0.0);
        g.growth_rate[sub_id] = 0;
    }
    if g.industry_num[sub_id] == 37 || g.industry_num[sub_id] == 2 {
        transfer_etf_contracts(g, ui, parent_id, sub_id);
    }
    g.profit_margin[sub_id] = 0;
    g.disc_spend_pct[sub_id] = 0;
    g.shares_outstanding[sub_id] = Ext::from_f64(1.0);
    g.market_cap[sub_id] = Ext::from_f64(0.0);
    if 2 < sub_industry {
        g.industry_num[sub_id] = 70;
    }
    g.contingency_reserve[parent_id] = g.contingency_reserve[parent_id] + g.contingency_reserve[sub_id];
    g.contingency_reserve[sub_id] = Ext::from_f64(0.0);
    g.reserve_adj_pending[parent_id] = g.reserve_adj_pending[parent_id] + g.reserve_adj_pending[sub_id];
    g.reserve_adj_pending[sub_id] = Ext::from_f64(0.0);
    g.div_recvd1[parent_id] = g.div_recvd1[parent_id] + g.div_recvd1[sub_id];
    g.div_recvd1[sub_id] = Ext::from_f64(0.0);
    g.div_recvd2[parent_id] = g.div_recvd2[parent_id] + g.div_recvd2[sub_id];
    g.div_recvd2[sub_id] = Ext::from_f64(0.0);
    g.equity_income_ytd[parent_id] = g.equity_income_ytd[parent_id] + g.equity_income_ytd[sub_id];
    g.equity_income_ytd[sub_id] = Ext::from_f64(0.0);
    g.extra_items_ytd[parent_id] = g.extra_items_ytd[parent_id] + g.extra_items_ytd[sub_id];
    g.extra_items_ytd[sub_id] = Ext::from_f64(0.0);
    g.consol_tax_inc_ytd[parent_id] = g.consol_tax_inc_ytd[parent_id] + g.consol_tax_inc_ytd[sub_id];
    g.consol_tax_inc_ytd[sub_id] = Ext::from_f64(0.0);
    g.tax_credits[parent_id] = g.tax_credits[parent_id] + g.tax_credits[sub_id];
    g.tax_credits[sub_id] = Ext::from_f64(0.0);
    if !(g.industry_num[parent_id] == 1 && cast::<i32, _>(g.lending_bank[sub_id]) == parent_id) {
        let parent_lender_id = cast::<i32, _>(g.lending_bank[parent_id]);
        let sub_lender_id = cast::<i32, _>(g.lending_bank[sub_id]);
        g.bank_debt[parent_id] = g.bank_debt[parent_id] + g.bank_debt[sub_id];
        g.tmp_bank_loan = g.bank_debt[sub_id];
        if 10 < parent_lender_id {
            g.cash[parent_lender_id] = g.cash[parent_lender_id] - g.tmp_bank_loan;
        }
        if 10 < sub_lender_id {
            g.cash[sub_lender_id] = g.cash[sub_lender_id] + g.tmp_bank_loan;
        }
    }
    g.bank_debt[sub_id] = Ext::from_f64(0.0);
    for i in 1..=5 {
        g.player_advances[(i, parent_id)] = g.player_advances[(i, parent_id)] + g.player_advances[(i, sub_id)];
        g.player_advances[(i, sub_id)] = Ext::from_f64(0.0);
    }
    for k in 1..=15 {
        if cast::<i32, _>(g.hold_stock_id[(k, parent_arg)]) == sub_arg {
            g.hold_stock_id[(k, parent_arg)] = 0;
            g.hold_stock_pct[(k, parent_arg)] = 0;
            g.hold_stock_basis[(k, parent_arg)] = Ext::from_f64(0.0);
        }
    }
    for k in 1..=15 {
        if g.hold_stock_id[(k, sub_arg)] == 0 {
            continue;
        }
        let stock_id = g.hold_stock_id[(k, sub_arg)];
        stock_pct = g.hold_stock_pct[(k, sub_arg)];
        g.hold_stock_id[(k, sub_arg)] = 0;
        g.hold_stock_pct[(k, sub_arg)] = 0;
        if 79 < stock_pct {
            subsidiary_list[cast::<usize, _>(k)] = cast::<i32, _>(stock_id);
        }
        for i in 1..=15 {
            if g.hold_stock_id[(i, parent_arg)] == stock_id {
                g.hold_stock_pct[(i, parent_arg)] = stock_pct.wrapping_add(g.hold_stock_pct[(i, parent_arg)]);
                g.hold_stock_basis[(i, parent_arg)] = g.hold_stock_basis[(i, parent_arg)] + g.hold_stock_basis[(k, sub_arg)];
                stock_pct = 0;
            }
        }
        if stock_pct != 0 {
            for j in 1..=15 {
                if g.hold_stock_id[(j, parent_arg)] == 0 {
                    g.hold_stock_id[(j, parent_arg)] = stock_id;
                    g.hold_stock_pct[(j, parent_arg)] = stock_pct;
                    g.hold_stock_basis[(j, parent_arg)] = g.hold_stock_basis[(k, sub_arg)];
                    g.hold_stock_id[(0, parent_arg)] = g.hold_stock_id[(0, parent_arg)].wrapping_add(1);
                    break;
                }
            }
        }
    }
    transfer_options_holder(g, ui, sub_arg, parent_arg, 1);
    g.hold_stock_id[(0, sub_arg)] = 0;
    g.corp_controller[sub_arg] = 0;
    if auto_mode < 2 {
        g.turn_action_count += 1;
    }
    g.entity_status[sub_arg] = 2;
    clear_all_holdings(g, 2);
    g.proj_qtr_earnings[sub_arg] = Ext::from_f64(0.0);
    if auto_mode == 0 {
        let mut m = s(__content.string(14418))
            + &g.ent_symbol[sub_arg]
            + &s(" by ")
            + &g.ent_symbol[parent_arg]
            + &s(__content.string(9542))
            + &s(__content.string(9543))
            + &parent_name
            + &s(".");
        show_quick_message(g, ui, &mut m);
    }
    if synergy == 1 && Ext::from_f64(0.0) < g.proj_qtr_earnings[parent_arg] {
        let f = rng.rnd() * sf(0.08f32) + sf(1.02f32);
        g.proj_qtr_earnings[parent_arg] = f * g.proj_qtr_earnings[parent_arg];
    }
    g.news_msg = sub_name.clone() + &s(" (") + &g.ent_symbol[sub_arg] + &s(__content.string(9544)) + &g.ent_symbol[parent_arg];
    post_news(g, ui, 0, 0);
    if !tax_loss_msg.is_empty() {
        g.news_msg = tax_loss_msg;
        post_news(g, ui, 0, 0);
    }
    if !succeed_msg.is_empty() {
        g.news_msg = succeed_msg;
        post_news(g, ui, 0, 0);
    }

    // ---- the parent's own stock received from the subsidiary is retired
    parent_id = parent_arg;
    stock_pct = 0;
    for k in 1..=15 {
        if cast::<i32, _>(g.hold_stock_id[(k, parent_id)]) == parent_id {
            stock_pct = g.hold_stock_pct[(k, parent_id)];
            g.hold_stock_id[(k, parent_id)] = 0;
            g.hold_stock_pct[(k, parent_id)] = 0;
            g.hold_stock_basis[(k, parent_id)] = Ext::from_f64(0.0);
            break;
        }
    }
    if 0 < stock_pct {
        if stock_pct == 100 {
            // ERROR 151 -> ON ERROR handler: ShowRuntimeError and exit (not ported: message box only)
            return;
        }
        let pct_scale: Ext = Ext::from_f64(100.0) / (Ext::from_f64(100.0) - cast::<Ext, _>(stock_pct));
        for e in 1..=1600 {
            for i in 1..=15 {
                if cast::<i32, _>(g.hold_stock_id[(i, e)]) == parent_id {
                    let hold_value = g.market_cap[parent_id] * cast::<Ext, _>(g.hold_stock_pct[(i, e)]);
                    g.hold_stock_pct[(i, e)] = cast::<i16, _>(to_quad(pct_scale * cast::<Ext, _>(g.hold_stock_pct[(i, e)])));
                    if g.hold_stock_pct[(i, e)] == 0 {
                        g.hold_stock_id[(i, e)] = 0;
                        g.cash[e] = g.cash[e] + hold_value;
                        if Ext::from_f64(0.0) < hold_value {
                            g.cash[parent_id] = g.cash[parent_id] - hold_value;
                        }
                        if e < 11 {
                            if hold_value < Ext::from_f64(0.0) {
                                g.hold_stock_basis[(i, e)] = -g.hold_stock_basis[(i, e)];
                            }
                            g.player_cap_gains[e] = g.player_cap_gains[e] + (hold_value - g.hold_stock_basis[(i, e)]);
                        } else {
                            g.extraordinary_pl[e] = to_quad(cast::<Ext, _>(g.extraordinary_pl[e]) + (hold_value - g.hold_stock_basis[(i, e)]));
                        }
                        g.hold_stock_basis[(i, e)] = Ext::from_f64(0.0);
                        g.news_msg = g.ent_name[e].clone() + &s(__content.string(9545)) + &g.ent_symbol[parent_arg] + &s(__content.string(9546));
                        if hold_value < Ext::from_f64(0.0) {
                            g.news_msg = g.ent_name[e].clone() + &s(__content.string(9547)) + &g.ent_symbol[parent_arg] + &s(__content.string(9548));
                        }
                        post_news(g, ui, 0, 0);
                    }
                    break;
                }
            }
        }
        g.shares_outstanding[parent_id] = sf(0.01f32) * cast::<Ext, _>(100 - cast::<i32, _>(stock_pct)) * g.shares_outstanding[parent_id];
        let pct_delta = -(cast::<i32, _>(stock_pct));
        adjust_index_divisor(g, parent_id, pct_delta);
        scale_earnings_history(g, parent_id, pct_delta);
    }
    update_corp_control(g, ui, rng, parent_arg);
    if g.public_pct == stock_pct {
        normalize_holding_pcts(g, ui, rng, parent_arg, 1);
    }
    clear_entity_record(g, ui, rng, sub_arg);
    g.active_corp_count -= 1;
    if sub_arg == g.last_sel_entity {
        // CONTROL SET TEXT hDlgMain, 173, "Se&lect Last" (UI only)
        g.last_sel_entity = 0;
    }
    pack_holding_arrays(g, ui);
    if sub_arg == g.sel_entity {
        // (mode 1 doesn't read the clock)
        crate::logic::trading::select_acting_entity(g, ui, rng, 1, &crate::logic::newgame::LocalTime::default());
        close_all_dialogs(g, ui, rng, 1);
        g.target_corp = parent_arg;
        g.tmp_symbol = g.ent_symbol[parent_arg].clone();
        select_target_corp(g, ui, rng, time_secs);
    }
    rebuild_streaming_quotes_panel(g, ui, 2);
    if sub_industry <= 2 {
        if sub_industry == 1 && sub_arg <= 84 && g.entity_status[sub_arg + 1] == 1 {
            g.tmp_ext1 = Ext::from_f64(0.0);
            g.tmp_ext2 = er * Ext::from_f64(2000.0) + rng.rnd() * Ext::from_f64(8000.0) * er;
            create_bank(g, ui, rng, sub_arg, auto_mode);
            g.bank_alloc_off[sub_arg] = 0;
            bank_allocate_assets(g, ui, rng, sub_arg, 2, 0);
        } else if sub_industry == 2 && sub_arg < 130 && g.entity_status[sub_arg + 1] == 1 {
            g.tmp_ext1 = er * Ext::from_f64(2500.0) + rng.rnd() * Ext::from_f64(5000.0) * er;
            g.tmp_ext2 = er * Ext::from_f64(400.0) + rng.rnd() * Ext::from_f64(1200.0) * er;
            create_insurer(g, ui, rng, sub_arg, 0);
        }
        if g.entity_status[sub_arg] == 1 {
            g.ent_misc_data[sub_arg] = PbStr::new();
            g.valuation_mode = 3;
            calc_stock_price(g, ui, rng, sub_arg, time_secs);
            msg_text = s("bank, ");
            if sub_industry == 2 {
                msg_text = s(__content.string(9549));
            }
            g.news_msg = s("A new ") + &msg_text + &g.ent_name[sub_arg] + &s(" (") + &g.ent_symbol[sub_arg] + &s(__content.string(9550));
            post_news(g, ui, 0, 0);
        }
    }
    fix_subsidiary_ctrl_years(g, ui, rng, &subsidiary_list);
    force_regulated_call_sales(g, ui, rng);
}

/// Port of LiquidateSub_RequireDivest (0x51af17), a GOSUB of LiquidateSubIntoParent: the subsidiary must
/// first sell its stock of `divestStockId` (interactive mode asks; NO exits the whole routine, returned as
/// `true`). The second question says "also".
struct RequireDivest<'a> {
    auto_mode: i32,
    also_word: &'a mut PbStr,
    sub_name: &'a PbStr,
    parent_name: &'a PbStr,
    sub_arg: i32,
}

impl RequireDivest<'_> {
    fn run(&mut self, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, divest_stock_id: i32) -> bool {
    let __content = crate::content::current();
        if self.auto_mode == 0 {
            let name = g.ent_name[divest_stock_id].clone();
            let text = s(__content.string(9551))
                + self.sub_name
                + &s(__content.string(9552))
                + &*self.also_word
                + &s(__content.string(9553))
                + &name
                + &s(__content.string(9554))
                + &name
                + &s(__content.string(9555))
                + self.parent_name
                + &s(__content.string(9556))
                + self.sub_name
                + &s("?");
            if ui.msgbox(&text, 8484, &s(__content.string(5053))) != 6 {
                return true;
            }
            *self.also_word = s("also ");
        }
        divest_stock(g, ui, rng, divest_stock_id, self.sub_arg, 0);
        false
    }
}

/// Port of LiquidateSub_ClearBonds (0x51aeab), a GOSUB of LiquidateSubIntoParent: clears the bond issue of
/// entity `j` (`BondsIssued`, `BondIssueInfo`, `BondPrice`).
fn clear_bonds(g: &mut Globals, j: i32) {
    g.bonds_issued[j] = 0;
    g.bond_issue_info[j] = PbStr::new();
    g.bond_price[j] = Default::default();
}

/// Port of LiquidateSub_FixSubsidiaryCtrlYears (0x51b101), a GOSUB of LiquidateSubIntoParent: for every
/// 80%+ subsidiary the liquidated company held (now held by the parent, not a holding company), updates
/// the "controlled since" field of its `EntMiscData` ([`update_ctrl_since_in_misc_data`]).
fn fix_subsidiary_ctrl_years(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, subsidiary_list: &[i32; 16]) {
    for &subsidiary_id in &subsidiary_list[1..=15] {
        if subsidiary_id != 0 && g.industry_num[subsidiary_id] != 70 {
            update_ctrl_since_in_misc_data(g, ui, rng, subsidiary_id);
        }
    }
}

/// Inline block of LiquidateSubIntoParent (no GOSUB in the original; both copies are identical): the
/// bonds of `issuer` are called in (`CallBonds` with `TmpExt1 = 100`, `TmpExt2 = 1`) unless a bondholder
/// controlled by the current player (or the player himself) holds them while they trade below 102
/// (`bonds_resolved = blocking_issuer_id = -1`, then `bonds_resolved = 0`). Fully redeemed: `bonds_resolved`
/// = `done` (1 for the subsidiary, 2 for the parent) with a headline. `UpdateCorpControl(parentId)` first.
fn call_in_bonds(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    parent_id: i32,
    issuer: i32,
    sub_id: i32,
    price: f32,
    bonds_resolved: &mut i32,
    blocking_issuer_id: &mut i32,
    done: i32,
) {
    let __content = crate::content::current();
    update_corp_control(g, ui, rng, parent_id);
    for e in 1..=1583 {
        for i in 1..=15 {
            if g.hold_bond_id[(i, e)] == issuer
                && (g.cur_player == g.corp_controller[e] || g.cur_player == e)
                && price < Ext::from_f32(102.0f32)
            {
                *bonds_resolved = -1;
                *blocking_issuer_id = -1;
            }
        }
    }
    if *bonds_resolved != -1 {
        g.tmp_ext1 = Ext::from_f64(100.0);
        g.tmp_ext2 = Ext::from_f64(1.0);
        call_bonds(g, ui, rng, issuer);
        if g.bonds_issued[issuer] <= 0 {
            *bonds_resolved = done;
            g.news_msg = if done == 1 {
                g.ent_symbol[issuer].clone() + &s(__content.string(9557))
            } else {
                g.ent_symbol[issuer].clone()
                    + &s(__content.string(9558))
                    + &g.ent_symbol[sub_id]
            };
            post_news(g, ui, 0, 0);
        } else {
            *bonds_resolved = -1;
        }
    }
    if *bonds_resolved == -1 {
        *bonds_resolved = 0;
    }
}

/// Inline block of LiquidateSubIntoParent (twice, for the subsidiary and the parent): `id` buys back its
/// publicly held bonds (`bonds_public` face) for `buyback_cost` out of cash (T-bills, then a bank loan for a
/// shortfall), books the discount to `ExtraordinaryPL` and clears its bond issue ([`clear_bonds`]).
fn buy_back_bonds(g: &mut Globals, id: i32, bonds_public: i32, buyback_cost: Ext) {
    g.cash[id] = g.cash[id] - buyback_cost;
    if g.cash[id] < Ext::from_f64(0.0) {
        balance_cash_t_bills(g, id);
    }
    if g.cash[id] < Ext::from_f64(0.0) {
        let shortfall = g.cash[id].abs();
        g.cash[id] = g.cash[id] + shortfall;
        g.bank_debt[id] = g.bank_debt[id] + shortfall;
        let lb = cast::<i32, _>(g.lending_bank[id]);
        g.cash[lb] = g.cash[lb] - shortfall;
    }
    g.extraordinary_pl[id] = to_quad(cast::<Ext, _>(g.extraordinary_pl[id]) + (cast::<Ext, _>(bonds_public) - buyback_cost));
    clear_bonds(g, id);
}
