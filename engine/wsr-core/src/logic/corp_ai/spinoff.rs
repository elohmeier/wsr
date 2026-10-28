//! Spin-offs of subsidiary stock to the parent's shareholders: the core transaction
//! (`SpinOffSubsidiary`), the player's dialog flow around it (`SpinOffSubsidiaryMenu`, also used by the
//! AI with a negative subsidiary id), the AI's scheduled tax-free spin-off (`AiTaxFreeSpinoff`), the
//! pending-merger cancellation it may trigger and the asset-sale price helper `CalcAssetSalePricePct`.
//!
//! SINGLE pool constants of the original (`fmul dword [0x75e…]`) are written `sf(0.67)`.

#![allow(
    clippy::assign_op_pattern,
    clippy::manual_range_contains,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::too_many_arguments,
    clippy::if_same_then_else,
    clippy::needless_late_init,
    clippy::neg_multiply
)]
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::prelude::*;

use crate::logic::control::{find_option_positions, update_corp_control};
use crate::logic::earnings::calc_projected_earnings;
use crate::logic::finance::{calc_entity_financials, get_controller_type};
use crate::logic::holdings::{find_holding_slot, find_stock_holding};
use crate::logic::news::is_in_recent_headlines;
use crate::logic::options::check_naked_option_margin;
use crate::logic::report::show_quick_message;
use crate::logic::reports_corp::check_cross_ownership;
use crate::logic::reports_lists::{show_portfolio_listing, update_control_chain};
use crate::logic::shorts::find_short_positions;
use crate::logic::sim_entity::bonds::{
    adjust_conversion_price, adjust_price_alerts_for_stock, force_regulated_call_sales,
    normalize_holding_pcts, scale_earnings_history,
};
use crate::logic::sim_entity::dividends::adjust_options_for_corp_action;
use crate::logic::sim_entity::mgmt::force_cover_shorts;
use crate::logic::sim_entity::quarter_start::{
    cancel_pending_merger, pack_holding_arrays, recapture_excess_loss,
};
use crate::logic::valuation::{
    adjust_index_divisor, calc_industry_outlook, calc_stock_price, check_stock_split,
    update_price_history_tick,
};
use crate::pb::string::{CaseMode, mcase, parse_count, remove_any};

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

/// Port of SpinOffSubsidiary_Exit (0x5c1948).
/// The empty exit trampoline of `SpinOffSubsidiaryMenu` (every exit jumps here); nothing to do.
pub fn spin_off_subsidiary_exit() {}

/// Port of SpinOff_CancelMergerOnViolation (0x5c17df).
/// The selected company broke the terms of a pending merger by a spin-off: with `ActiveEntity`,
/// `MergerTarget` and `MergerAcquirer` all companies (> 10), a pop-up and the headline "… cancels offer
/// to merge with …"; then the pending merger is cleared (`CancelPendingMerger(0)`) in any case.
pub fn spin_off_cancel_merger_on_violation(g: &mut Globals, ui: &mut dyn Ui) {
    let __content = crate::content::current();
    if 10 < g.active_entity && 10 < g.merger_target && 10 < g.merger_acquirer {
        let mut msg = g.ent_symbol[g.active_entity].clone()
            + __content.string(2845)
            + &g.ent_name[g.merger_acquirer]
            + ". "
            + __content.string(1997)
            + &g.ent_name[g.merger_acquirer]
            + __content.string(2846);
        show_quick_message(g, ui, &mut msg);
        g.news_msg = g.ent_symbol[g.merger_acquirer].clone()
            + __content.string(2847)
            + &g.ent_symbol[g.active_entity];
        post_news(g, ui, 0, 0);
    }
    cancel_pending_merger(g, ui, 0);
}

/// Port of CalcAssetSalePricePct (0x5db84b).
/// Premium/discount in % of cost at which operating company `corp_id` (>= 131, industry 3..69; else 0)
/// can sell business assets: the industry's projected demand gap (`CalcIndustryOutlook`, `TmpExt2`)
/// plus its profit margin, minus 8; floor -50, values over 25 scaled by 1.25, cap 75, |x| < 3 -> 0.
/// Clears `TmpExt1..3`.
pub fn calc_asset_sale_price_pct(g: &mut Globals, rng: &mut Rng, corp_id: i32) -> Ext {
    let mut result: i64 = 0;
    if 130 < corp_id {
        let ind_num = g.industry_num[corp_id];
        if 2 < ind_num && ind_num < 70 {
            calc_industry_outlook(g, rng, ind_num);
            let mut price_pct = to_long(g.tmp_ext2 + cast::<Ext, _>(g.profit_margin[corp_id]));
            g.tmp_ext1 = Ext::from_f64(0.0);
            g.tmp_ext2 = Ext::from_f64(0.0);
            g.tmp_ext3 = Ext::from_f64(0.0);
            price_pct = price_pct - 8;
            if price_pct < -50 {
                price_pct = -50;
            }
            if 25 < price_pct {
                price_pct = to_long(sf(1.25f32) * cast::<Ext, _>(price_pct));
            }
            if 75 < price_pct {
                price_pct = 75;
            }
            if (cast::<Ext, _>(price_pct)).abs() < Ext::from_f64(3.0) {
                price_pct = 0;
            }
            result = cast::<i64, _>(price_pct);
        }
    }
    cast::<Ext, _>(result)
}

/// Port of SpinOffSubsidiary (0x525509).
/// Distributes `pct_to_spin` % (at most the stake held) of subsidiary `sub_id`'s stock held by
/// `parent_id` to `parent_id`'s shareholders, pro rata (stakes under 1% are cashed out). Tax-free when
/// at least 80% is spun off of a subsidiary controlled for 5+ years that is no industry-70 holding
/// company and not mostly cash/T-bills; otherwise a taxable dividend (gain recognised by the parent,
/// dividends received by the holders; non-taxable to an 80% parent). Government blocks: a 15-holding
/// shareholder of the parent that has no slot left for the new stock (with a "Divestment … Required"
/// box when the player controls both; then returns 0 at once), a weak credit rating of the parent
/// (banks/insurers < 7, others < 5) before or after the distribution, or a drop of more than 3 rating
/// notches; a blocked spin-off shows "Government Intervention" (`silent_mode = 0`) and posts a headline.
/// Treasury stock the subsidiary receives is retired. Afterwards shorts are covered, control chains,
/// convertibles, price alerts, options, projected earnings and the parent's price (cut by 10%/20%) are
/// updated and cross-ownership is checked.
///
/// `silent_mode`: 0 = player (confirmation boxes), 1 = silent, 2 = silent without counting a turn action.
/// Returns 1 if the spin-off was done, else 0. `time_secs` is passed on to `CalcStockPrice`.
///
/// Bug (reproduced, BUGS.md #190): the investor-sentiment penalty compares the (negative) option
/// adjustment with 20% of the parent's market cap and never applies.
pub fn spin_off_subsidiary(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    parent_id: i32,
    sub_id: i32,
    silent_mode: i32,
    pct_to_spin: i32,
    time_secs: i32,
) -> i32 {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    if !(10 < parent_id && parent_id < 1601 && 10 < sub_id && sub_id < 1601 && 0 < pct_to_spin) {
        return 0;
    }
    let mut recapture_flag = PbStr::new();
    let mut desc_text: PbStr;
    let mut msg_text: PbStr;
    let mut ratio: Ext;
    let mut blocking_holder: i32 = 0;
    let mut pct_increased: i32 = 0;
    let mut conflict_corp_id: i32 = 0;
    let mut parent80_id: i16 = 0;
    let mut taxable_gain: Ext = Ext::from_f64(0.0);
    let mut sub_basis: Ext = Ext::from_f64(0.0);
    let mut held_pct: i32 = 0;
    let mut is_taxable: i32 = 0;
    let mut cash_rich: i32 = 0;
    let mut pct_received: Ext;
    let mut treasury_pct: i16 = 0;
    let mut parent_slot: i32;
    let mut basis_sold: Ext = Ext::from_f64(0.0);
    let mut distrib_total: Ext = Ext::from_f64(0.0);
    let mut force_to_100: i32 = 0;
    let mut status: i32;
    let mut holder_id: i32;
    let mut spin_pct: i32;
    let mut result: i32 = 0;

    pack_holding_arrays(g, ui);
    status = 1;
    if 2 < g.industry_num[sub_id] {
        calc_entity_financials(g, ui, rng, sub_id);
        if g.total_assets * sf(0.67f32) < g.cash[sub_id] + g.t_bills[sub_id] {
            cash_rich = 1;
        }
    }
    spin_pct = pct_to_spin;
    if spin_pct < 80
        || g.cur_year - 5 < g.ctrl_since_year[sub_id]
        || g.industry_num[sub_id] == 70
        || cash_rich == 1
    {
        is_taxable = 1;
    }
    holder_id = 1;
    loop {
        if g.entity_status[holder_id] == 1 && g.hold_stock_id[(0, holder_id)] >= 15 {
            if find_stock_holding(g, holder_id, parent_id) >= 1 {
                status = find_holding_slot(g, holder_id, sub_id);
                if status < 1 {
                    let slot = (holder_id + g.cur_year) % 15 + 1;
                    conflict_corp_id = cast::<i32, _>(g.hold_stock_id[(slot, holder_id)]);
                    blocking_holder = holder_id;
                    if g.corp_controller[parent_id] == g.corp_controller[holder_id] {
                        desc_text = s("its");
                        if g.cur_player == holder_id {
                            desc_text = s("your");
                        }
                        if g.cur_player == g.corp_controller[parent_id]
                            && g.cur_player == g.corp_controller[blocking_holder]
                        {
                            msg_text = s(__content.string(2848))
                                + &g.ent_name[sub_id]
                                + __content.string(2849);
                            if g.cur_player == holder_id {
                                msg_text = msg_text + "you,";
                            } else {
                                msg_text = msg_text + &g.ent_name[holder_id] + ",";
                            }
                            msg_text = msg_text
                                + __content.string(2850)
                                + &g.ent_name[parent_id]
                                + __content.string(2851)
                                + &desc_text
                                + " stock "
                                + __content.string(2852)
                                + &g.ent_name[conflict_corp_id]
                                + " ("
                                + &g.ent_symbol[conflict_corp_id]
                                + ")."
                                + "\r"
                                + "\r";
                            if g.cur_player != holder_id {
                                msg_text = replace(
                                    msg_text.as_bytes(),
                                    __content.bytes(2853),
                                    __content.bytes(2854),
                                );
                            }
                            msg_text = msg_text
                                + __content.string(2855);
                            let title = s(__content.string(2856)) + &g.ent_name[conflict_corp_id] + __content.string(2857);
                            ui.msgbox(&msg_text, 8208, &title);
                            return result;
                        }
                    }
                    holder_id = 1600;
                } else {
                    status = 1;
                }
            }
        }
        holder_id += 1;
        if holder_id >= 1601 {
            break;
        }
    }
    calc_entity_financials(g, ui, rng, parent_id);
    if g.industry_num[parent_id] < 3 && g.credit_rating_num[parent_id] < 7 {
        status = -1;
    }
    if g.credit_rating_num[parent_id] < 5 {
        status = -1;
    }
    if 0 < status {
        parent_slot = find_holding_slot(g, parent_id, sub_id);
        let prev_rating = g.credit_rating_num[parent_id];
        sub_basis = g.hold_stock_basis[(parent_slot, parent_id)];
        let saved_consol_inc = g.consol_tax_inc_ytd[parent_id];
        held_pct = cast::<i32, _>(g.hold_stock_pct[(parent_slot, parent_id)]);
        if held_pct < spin_pct {
            spin_pct = held_pct;
        }
        ratio = Ext::from_f64(1.0);
        distrib_total = g.market_cap[sub_id] * cast::<Ext, _>(spin_pct);
        if spin_pct < held_pct {
            ratio = cast::<Ext, _>(spin_pct) / cast::<Ext, _>(held_pct);
        }
        if is_taxable == 1 {
            basis_sold = (cast::<Ext, _>(spin_pct) / cast::<Ext, _>(held_pct)) * sub_basis;
            if sub_basis < Ext::from_f64(0.0) && held_pct - spin_pct < 80 {
                basis_sold = sub_basis;
            }
            taxable_gain = distrib_total - basis_sold;
            if Ext::from_f64(0.0) < taxable_gain {
                g.consol_tax_inc_ytd[parent_id] = g.consol_tax_inc_ytd[parent_id] + taxable_gain;
            }
        } else if sub_basis < Ext::from_f64(0.0) {
            taxable_gain = sub_basis.abs();
            g.consol_tax_inc_ytd[parent_id] = g.consol_tax_inc_ytd[parent_id] + taxable_gain;
        }
        g.hold_stock_pct[(parent_slot, parent_id)] =
            g.hold_stock_pct[(parent_slot, parent_id)].wrapping_sub(cast::<i16, _>(spin_pct));
        g.hold_stock_basis[(parent_slot, parent_id)] =
            g.hold_stock_basis[(parent_slot, parent_id)] - ratio * sub_basis;
        if sub_basis < Ext::from_f64(0.0) && g.hold_stock_pct[(parent_slot, parent_id)] < 80 {
            g.hold_stock_basis[(parent_slot, parent_id)] = Ext::from_f64(0.0);
        }
        if held_pct == spin_pct {
            g.hold_stock_id[(parent_slot, parent_id)] = 0;
            g.hold_stock_pct[(parent_slot, parent_id)] = 0;
            g.hold_stock_basis[(parent_slot, parent_id)] = Ext::from_f64(0.0);
        }
        calc_entity_financials(g, ui, rng, parent_id);
        if g.credit_rating_num[parent_id] < 5 || 3 < prev_rating - g.credit_rating_num[parent_id] {
            status = -1;
        }
        if g.industry_num[parent_id] < 3 && g.credit_rating_num[parent_id] < 7 {
            status = -1;
        }
        g.consol_tax_inc_ytd[parent_id] = saved_consol_inc;
        if status == -1 {
            g.hold_stock_id[(parent_slot, parent_id)] = cast::<i16, _>(sub_id);
            g.hold_stock_pct[(parent_slot, parent_id)] = cast::<i16, _>(held_pct);
            g.hold_stock_basis[(parent_slot, parent_id)] = sub_basis;
            calc_entity_financials(g, ui, rng, parent_id);
        } else if sub_basis < Ext::from_f64(0.0) && is_taxable == 0 {
            g.hold_stock_id[(parent_slot, parent_id)] = cast::<i16, _>(sub_id);
            g.hold_stock_basis[(parent_slot, parent_id)] = sub_basis;
            if g.hold_stock_pct[(parent_slot, parent_id)] == 0 {
                g.hold_stock_pct[(parent_slot, parent_id)] = 1;
                recapture_excess_loss(g, ui, sub_id, parent_id);
                recapture_flag = s("Y");
                g.hold_stock_id[(parent_slot, parent_id)] = 0;
                g.hold_stock_pct[(parent_slot, parent_id)] = 0;
            } else {
                recapture_excess_loss(g, ui, sub_id, parent_id);
                recapture_flag = s("Y");
            }
        }
    }
    if 0 < status {
        update_corp_control(g, ui, rng, parent_id);
        if 10 < g.parent80_owner {
            parent80_id = g.parent80_owner;
        }
        // (`IF status > 0` again: always true here)
        if find_short_positions(g, ui, rng, sub_id, 0) == 2 {
            force_cover_shorts(g, ui, rng, 1, 0, sub_id, 0);
        }
        if find_short_positions(g, ui, rng, parent_id, 0) == 2 {
            force_cover_shorts(g, ui, rng, 1, 0, parent_id, 0);
        }
        if is_taxable == 1 && basis_sold < distrib_total {
            g.extraordinary_pl[parent_id] =
                to_quad(cast::<Ext, _>(g.extraordinary_pl[parent_id]) + taxable_gain);
            g.news_msg = g.ent_symbol[parent_id].clone()
                + __content.string(2858)
                + &g.ent_symbol[sub_id]
                + " stock";
            post_news(g, ui, 0, 0);
        }
        for holder_id in 1..=1600 {
            if g.entity_status[holder_id] != 1 || g.hold_stock_id[(0, holder_id)] == 0 {
                continue;
            }
            for slot in 1..=15 {
                if cast::<i32, _>(g.hold_stock_id[(slot, holder_id)]) != parent_id {
                    continue;
                }
                let sub_slot = find_holding_slot(g, holder_id, sub_id);
                let prev_sub_pct = g.hold_stock_pct[(sub_slot, holder_id)];
                pct_received = sf(0.01f32)
                    * cast::<Ext, _>(g.hold_stock_pct[(slot, holder_id)])
                    * cast::<Ext, _>(spin_pct);
                let holder_parent_pct = g.hold_stock_pct[(slot, holder_id)];
                if sub_id == holder_id {
                    treasury_pct = to_integer(pct_received);
                    if sf(0.95f32) < pct_received {
                        if treasury_pct == 0 {
                            treasury_pct = 1;
                        }
                        continue;
                    }
                }
                if sf(0.95f32) < pct_received {
                    if pct_received < Ext::from_f64(1.0) {
                        pct_received = Ext::from_f64(1.0);
                    }
                    g.hold_stock_id[(sub_slot, holder_id)] = cast::<i16, _>(sub_id);
                    g.hold_stock_pct[(sub_slot, holder_id)] =
                        to_integer(pct_received + cast::<Ext, _>(g.hold_stock_pct[(sub_slot, holder_id)]));
                    if is_taxable == 1 && cast::<i32, _>(parent80_id) != holder_id {
                        let distrib_value = g.market_cap[sub_id] * pct_received;
                        g.hold_stock_basis[(sub_slot, holder_id)] =
                            g.hold_stock_basis[(sub_slot, holder_id)] + distrib_value;
                        if holder_id < 11 {
                            g.div_recvd2[holder_id] = g.div_recvd2[holder_id] + distrib_value;
                        } else if holder_parent_pct < 20 {
                            if 0 < g.corp_controller[parent_id]
                                && g.corp_controller[holder_id] == g.corp_controller[parent_id]
                            {
                                g.div_recvd1[holder_id] = g.div_recvd1[holder_id] + distrib_value;
                            } else {
                                g.div_recvd2[holder_id] = g.div_recvd2[holder_id] + distrib_value;
                            }
                        } else {
                            g.div_recvd1[holder_id] = g.div_recvd1[holder_id] + distrib_value;
                        }
                    } else if is_taxable == 1 && cast::<i32, _>(parent80_id) == holder_id {
                        if sub_basis < Ext::from_f64(0.0) {
                            taxable_gain = taxable_gain - sub_basis.abs();
                        }
                        let distrib_value = taxable_gain * (pct_received / cast::<Ext, _>(spin_pct));
                        g.hold_stock_basis[(sub_slot, holder_id)] =
                            g.hold_stock_basis[(sub_slot, holder_id)] + distrib_value;
                        g.hold_stock_basis[(slot, holder_id)] = g.hold_stock_basis[(slot, holder_id)]
                            - g.market_cap[sub_id] * pct_received;
                    } else {
                        let parent_stock_value =
                            g.market_cap[parent_id] * cast::<Ext, _>(g.hold_stock_pct[(slot, holder_id)]);
                        let spun_value = g.market_cap[sub_id] * pct_received;
                        ratio = parent_stock_value + spun_value;
                        let alloc_basis = (spun_value / ratio) * g.hold_stock_basis[(slot, holder_id)];
                        let remaining_basis = g.hold_stock_basis[(slot, holder_id)] - alloc_basis;
                        if sub_id != holder_id {
                            g.hold_stock_basis[(sub_slot, holder_id)] =
                                g.hold_stock_basis[(sub_slot, holder_id)] + alloc_basis;
                        }
                        g.hold_stock_basis[(slot, holder_id)] = remaining_basis;
                    }
                    desc_text = s(__content.string(2859));
                    if is_taxable == 1 {
                        desc_text = s(__content.string(2860));
                        if cast::<i32, _>(parent80_id) == holder_id {
                            desc_text = PbStr::new();
                        }
                    }
                    g.news_msg = g.ent_symbol[holder_id].clone()
                        + __content.string(2861)
                        + &str_s(cast::<Ext, _>(g.hold_stock_pct[(sub_slot, holder_id)]) - cast::<Ext, _>(prev_sub_pct))
                        + "% of "
                        + &g.ent_symbol[sub_id]
                        + " in"
                        + &desc_text
                        + __content.string(2862)
                        + &g.ent_symbol[parent_id];
                    post_news(g, ui, 0, 0);
                    if cast::<i32, _>(parent80_id) == holder_id && is_taxable == 1 && 0 < silent_mode {
                        g.news_msg = s(__content.string(2863))
                            + &g.ent_symbol[sub_id]
                            + __content.string(2864)
                            + &g.ent_symbol[parent_id]
                            + ", "
                            + &g.ent_symbol[cast::<i32, _>(parent80_id)];
                        post_news(g, ui, 0, 0);
                    }
                } else {
                    let cash_in_lieu = g.market_cap[sub_id] * pct_received;
                    g.cash[holder_id] = g.cash[holder_id] + cash_in_lieu;
                    if sub_id == holder_id {
                        if holder_parent_pct < 80 {
                            g.extraordinary_pl[holder_id] =
                                to_quad(cast::<Ext, _>(g.extraordinary_pl[holder_id]) + cash_in_lieu);
                        } else {
                            g.hold_stock_basis[(slot, holder_id)] =
                                g.hold_stock_basis[(slot, holder_id)] - cash_in_lieu;
                        }
                        g.news_msg = g.ent_symbol[holder_id].clone()
                            + __content.string(2865)
                            + &g.ent_symbol[parent_id];
                        treasury_pct = 0;
                    } else {
                        if holder_id < 11 {
                            if is_taxable == 1 {
                                g.accrued_pl[holder_id] = g.accrued_pl[holder_id] + cash_in_lieu;
                            } else {
                                g.player_cap_gains[holder_id] =
                                    g.player_cap_gains[holder_id] + cash_in_lieu;
                            }
                        } else if holder_parent_pct < 80 {
                            g.extraordinary_pl[holder_id] =
                                to_quad(cast::<Ext, _>(g.extraordinary_pl[holder_id]) + cash_in_lieu);
                        } else {
                            g.hold_stock_basis[(slot, holder_id)] =
                                g.hold_stock_basis[(slot, holder_id)] - cash_in_lieu;
                        }
                        g.news_msg = g.ent_symbol[holder_id].clone()
                            + __content.string(2866)
                            + &g.ent_symbol[sub_id]
                            + __content.string(2867);
                    }
                    post_news(g, ui, holder_id, 0);
                }
            }
        }
        desc_text = s(__content.string(2859));
        if recapture_flag == "Y" {
            desc_text = s(__content.string(2868)) + &desc_text;
        }
        if is_taxable == 1 {
            desc_text = s(__content.string(2860));
        }
        msg_text = PbStr::new();
        if recapture_flag == "Y" {
            msg_text = s(__content.string(2869));
        }
        g.news_msg = g.ent_symbol[parent_id].clone()
            + " does"
            + &desc_text
            + __content.string(2870)
            + &str_s(cast::<Ext, _>(spin_pct))
            + __content.string(1909)
            + &g.ent_symbol[sub_id];
        if silent_mode == 0 {
            let text = g.news_msg.clone() + &msg_text;
            ui.msgbox(&text, 8192, &s(__content.string(2871)));
            post_news(g, ui, 0, 0);
        }

        status = if 0 < status { 1 } else { 0 };
        result = status;
        if status == 1 && silent_mode < 2 {
            g.turn_action_count += 1;
        }
        if 0 < treasury_pct {
            update_corp_control(g, ui, rng, sub_id);
            if g.public_pct == treasury_pct {
                force_to_100 = 1;
            }
            ratio = Ext::from_f64(100.0) / (Ext::from_f64(100.0) - cast::<Ext, _>(treasury_pct));
            for holder_id in 1..=1600 {
                for slot in 1..=15 {
                    if cast::<i32, _>(g.hold_stock_id[(slot, holder_id)]) == sub_id {
                        let old_pct = g.hold_stock_pct[(slot, holder_id)];
                        g.hold_stock_pct[(slot, holder_id)] =
                            to_integer(ratio * cast::<Ext, _>(g.hold_stock_pct[(slot, holder_id)]));
                        if old_pct < g.hold_stock_pct[(slot, holder_id)] {
                            pct_increased = 1;
                        }
                    }
                }
            }
            if force_to_100 == 0 && silent_mode == 0 {
                desc_text = g.ent_symbol[sub_id].clone()
                    + __content.string(2872)
                    + __content.string(2873)
                    + "\r"
                    + "\r";
                if pct_increased == 1 {
                    desc_text = desc_text
                        + __content.string(2874)
                        + __content.string(2875)
                        + &g.ent_name[sub_id]
                        + __content.string(2876);
                }
                ui.msgbox(&desc_text, 8256, &s(__content.string(2877)));
            }
            g.shares_outstanding[sub_id] =
                sf(0.01f32) * (Ext::from_f64(100.0) - cast::<Ext, _>(treasury_pct)) * g.shares_outstanding[sub_id];
            let pct_change = -(cast::<i32, _>(treasury_pct));
            adjust_index_divisor(g, sub_id, pct_change);
            adjust_conversion_price(g, sub_id, Ext::from_f64(0.0), cast::<Ext, _>(pct_change));
            adjust_price_alerts_for_stock(g, sub_id, Ext::from_f64(0.0), cast::<Ext, _>(pct_change));
            scale_earnings_history(g, sub_id, pct_change);
            normalize_holding_pcts(g, ui, rng, sub_id, force_to_100);
            update_price_history_tick(g, ui, rng, sub_id);
            parent_slot = find_holding_slot(g, sub_id, parent_id);
            pct_received =
                sf(0.01f32) * cast::<Ext, _>(g.hold_stock_pct[(parent_slot, sub_id)]) * cast::<Ext, _>(spin_pct);
            let spun_value = g.market_cap[sub_id] * pct_received;
            if cast::<i32, _>(parent80_id) == sub_id || is_taxable == 0 {
                g.hold_stock_basis[(parent_slot, sub_id)] =
                    g.hold_stock_basis[(parent_slot, sub_id)] - spun_value;
            } else if g.hold_stock_pct[(parent_slot, sub_id)] < 20 {
                if 0 < g.corp_controller[parent_id]
                    && g.corp_controller[sub_id] == g.corp_controller[parent_id]
                {
                    g.div_recvd1[sub_id] = g.div_recvd1[sub_id] + spun_value;
                } else {
                    g.div_recvd2[sub_id] = g.div_recvd2[sub_id] + spun_value;
                }
            } else {
                g.div_recvd1[sub_id] = g.div_recvd1[sub_id] + spun_value;
            }
        }
        update_control_chain(g, ui, rng, parent_id);
        update_control_chain(g, ui, rng, sub_id);
        find_option_positions(g, ui, 0, parent_id, 0, 0);
        adjust_conversion_price(g, parent_id, distrib_total * -Ext::from_f64(1.0), Ext::from_f64(0.0));
        adjust_price_alerts_for_stock(g, parent_id, distrib_total * -Ext::from_f64(1.0), Ext::from_f64(0.0));
        if g.has_options_outstanding[parent_id] == 1 {
            let option_adj = distrib_total * -sf(0.01f32);
            adjust_options_for_corp_action(g, ui, parent_id, option_adj, sub_id, 3);
            if g.market_cap[parent_id] * sf(0.2f32) < option_adj {
                g.investor_sentiment[parent_id] = g.investor_sentiment[parent_id] - 20;
            }
        }
        if g.proj_qtr_earnings[parent_id] * sf(0.2f32)
            < (g.proj_qtr_earnings[sub_id] * cast::<Ext, _>(spin_pct) * sf(0.01f32)).abs()
        {
            calc_projected_earnings(g, ui, rng, parent_id);
        }
        if distrib_total <= g.market_cap[parent_id] * Ext::from_f64(20.0) {
            g.market_cap[parent_id] = g.market_cap[parent_id] * sf(0.9f32);
        } else {
            g.market_cap[parent_id] = g.market_cap[parent_id] * sf(0.8f32);
        }
        calc_stock_price(g, ui, rng, parent_id, time_secs);
        force_regulated_call_sales(g, ui, rng);
        check_cross_ownership(g, ui, rng, parent_id, 1);
        check_cross_ownership(g, ui, rng, sub_id, 1);
        return result;
    }

    // blocked by the regulators
    if silent_mode == 0 {
        if g.credit_rating_num[parent_id] < 7
            && !(2 < g.industry_num[parent_id] && 4 < g.credit_rating_num[parent_id])
        {
            status = -2;
        }
        msg_text = s("Banking");
        if g.industry_num[parent_id] == 2 {
            msg_text = s(__content.string(2878));
        }
        if status == 0 || 2 < g.industry_num[parent_id] {
            msg_text = s(__content.string(2879));
        }
        let deal_desc = g.ent_name[sub_id].clone() + " by " + &g.ent_name[parent_id];
        desc_text = msg_text + __content.string(2880) + &deal_desc;
        if status == 0 {
            desc_text = desc_text + ". ";
            if 10 < blocking_holder && g.cur_player != g.corp_controller[blocking_holder] {
                desc_text = desc_text
                    + __content.string(2881)
                    + &deal_desc
                    + __content.string(2849)
                    + &g.ent_name[parent_id]
                    + __content.string(2882)
                    + &g.ent_name[blocking_holder]
                    + __content.string(2883)
                    + __content.string(2852)
                    + &g.ent_name[conflict_corp_id]
                    + " ("
                    + &g.ent_symbol[conflict_corp_id]
                    + ")."
                    + "\r"
                    + "\r"
                    + __content.string(2884)
                    + &g.ent_name[blocking_holder]
                    + __content.string(2885)
                    + __content.string(2886)
                    + &g.ent_symbol[conflict_corp_id]
                    + __content.string(2887)
                    + &g.ent_name[blocking_holder]
                    + ".)";
            }
        }
        let mut tax_note = PbStr::new();
        if status == -1 && sub_basis < g.market_cap[sub_id] * cast::<Ext, _>(held_pct) {
            tax_note = s(__content.string(2888));
        }
        if status == -1 {
            desc_text = desc_text
                + __content.string(2889)
                + &tax_note
                + __content.string(2890)
                + &g.ent_name[parent_id]
                + __content.string(2891);
        }
        if status == -2 {
            desc_text = desc_text + __content.string(2892);
        }
        let text = desc_text + "\r" + "\r" + __content.string(2893);
        ui.msgbox(&text, 8208, &s(__content.string(2894)));
    }
    g.news_msg = s(__content.string(2895))
        + &g.ent_symbol[sub_id]
        + " by "
        + &g.ent_symbol[parent_id];
    if is_in_recent_headlines(g, __content.bytes(2896)) == 1
        && rng.rnd() * Ext::from_f64(10.0) > Ext::from_f64(1.0)
        && 0 < silent_mode
    {
        g.news_msg = PbStr::new();
    } else {
        post_news(g, ui, 0, 0);
    }
    0
}

/// Port of SpinOffSubsidiaryMenu (0x5bfb37).
/// The spin-off dialog flow for company `parent_id`: `sub_spec` 0..10 = the player picks the
/// subsidiary (the only holding, else from the portfolio listing in "GetCorp" mode) and enters the
/// percentage (INPUTBOX, up to the stake held); `|sub_spec| > 10` = that subsidiary, the whole stake
/// (negative: the AI, tax-free spin-offs only, no questions). Asks to cover the player's own short
/// position in either stock, warns about a pending merger in which the selected company is the target,
/// explains the tax ruling (taxable spin-off: why, the parent's approximate tax; tax-free with a negative
/// basis: the excess-loss recapture) and asks for confirmation; then runs `SpinOffSubsidiary` (silent
/// mode 0 for the player, 2 for the AI). After a spin-off: a merger in violation is cancelled, stock
/// split and naked-option margin of the parent are checked, an 80% grandparent gets a note, and after a
/// tax-free spin-off the subsidiary's control date and the parent field (28) of its earnings record are
/// carried over from its new 80% parent.
///
/// Bug (reproduced, BUGS.md #191): the pending merger is cancelled whenever `MergerTarget = ActiveEntity`,
/// also after an AI company's spin-off.
pub fn spin_off_subsidiary_menu(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    parent_id: i32,
    sub_spec: i32,
    time_secs: i32,
) {
    let __content = crate::content::current();
    if !(10 < parent_id && g.entity_status[parent_id] == 1) {
        return spin_off_subsidiary_exit();
    }
    let mut aux_text: PbStr;
    let mut msg_text: PbStr;
    let mut spin_tax: Ext = Ext::from_f64(0.0);
    let mut parent80_id: i16 = 0;
    let mut short_corp_id: i32 = 0;
    let mut ctrl_since: i32 = 0;
    let mut taxable_gain: Ext;
    let mut basis_spun: Ext;
    let mut mostly_liquid: i32 = 0;
    let mut is_taxable: i32 = 0;
    let mut spin_mode: i32 = 0;
    let mut result: i32;
    let pct_to_spin: i32;

    g.tmp_symbol = PbStr::new();
    g.action_code = s("SELL");
    pack_holding_arrays(g, ui);
    g.target_corp = 0;
    if (cast::<Ext, _>(sub_spec)).abs() <= Ext::from_f64(10.0) {
        if g.hold_stock_id[(0, parent_id)] == 1 {
            g.target_corp = cast::<i32, _>(g.hold_stock_id[(1, parent_id)]);
        } else {
            if g.hold_stock_id[(0, parent_id)] == 0 {
                let mut m = g.ent_name[parent_id].clone()
                    + __content.string(2897)
                    + "\r\n"
                    + "\r\n"
                    + __content.string(2898);
                show_quick_message(g, ui, &mut m);
                return spin_off_subsidiary_exit();
            }
            g.entity_pick_mode = s("GetCorp");
            g.tmp_ext1 = -Ext::from_f64(1.0);
            show_portfolio_listing(g, ui, rng, parent_id, 0, 0);
        }
    } else {
        g.target_corp = to_long((cast::<Ext, _>(sub_spec)).abs());
        if sub_spec < -10 {
            spin_mode = 2;
        }
    }
    if g.hold_stock_id[(0, parent_id)] == 0 || g.target_corp < 11 {
        return spin_off_subsidiary_exit();
    }
    let sub_id = g.target_corp;
    if spin_mode == 0 && 10 < g.target_corp {
        if find_stock_holding(g, g.cur_player, parent_id) < 0 {
            short_corp_id = parent_id;
        }
        if find_stock_holding(g, g.cur_player, sub_id) < 0 {
            short_corp_id = sub_id;
        }
        if 10 < short_corp_id {
            let m = s(__content.string(2899))
                + &g.ent_name[short_corp_id]
                + __content.string(2900)
                + __content.string(2901)
                + "\r"
                + "\r"
                + __content.string(2902);
            result = ui.msgbox(&m, 8227, &s(__content.string(2903)));
            if result != 6 {
                return spin_off_subsidiary_exit();
            }
        }
    }
    if g.merger_target == g.active_entity && 0 <= sub_spec && sub_spec < 11 {
        msg_text = g.ent_name[g.active_entity].clone()
            + __content.string(2904)
            + "by "
            + &g.ent_name[g.merger_acquirer]
            + "."
            + "\r"
            + "\r"
            + __content.string(2905)
            + &g.ent_name[sub_id]
            + __content.string(2906)
            + __content.string(2907)
            + "\r"
            + "\r"
            + __content.string(2908);
        result = ui.msgbox(&msg_text, 8227, &s(__content.string(2909)));
        if result != 6 {
            return spin_off_subsidiary_exit();
        }
    }
    if 130 < sub_id {
        calc_entity_financials(g, ui, rng, sub_id);
        if g.total_assets * sf(0.67f32) < g.cash[sub_id] + g.t_bills[sub_id] {
            mostly_liquid = 1;
        }
    }
    update_corp_control(g, ui, rng, parent_id);
    if 10 < g.parent80_owner {
        parent80_id = g.parent80_owner;
    }
    let mut slot = find_holding_slot(g, parent_id, sub_id);
    let held_pct = cast::<i32, _>(g.hold_stock_pct[(slot, parent_id)]);
    update_corp_control(g, ui, rng, sub_id);
    let mut sub_since_year = g.ctrl_since_year[sub_id];
    if g.industry_num[sub_id] == 70 {
        sub_since_year = 1950;
    }
    let country = g.country_idx[parent_id];
    let mut tax_agency = mcase(g.country_name[country].as_bytes(), CaseMode::Ansi) + __content.string(2910);
    if g.country_idx[parent_id] == 0 {
        tax_agency = s(__content.string(2911));
    }
    if g.country_idx[parent_id] == 50 {
        tax_agency = s(__content.string(2912));
    }
    if (cast::<Ext, _>(sub_spec)).abs() <= Ext::from_f64(10.0) {
        msg_text = s(__content.string(2913))
            + &g.ent_name[sub_id]
            + __content.string(2914)
            + &g.ent_name[parent_id]
            + __content.string(2915)
            + __content.string(2916)
            + &str_s(cast::<Ext, _>(held_pct))
            + "%)"
            + "\r\n"
            + "\r\n";
        if held_pct < 80 {
            msg_text = msg_text + __content.string(2917);
        } else {
            msg_text = msg_text + __content.string(2918);
        }
        msg_text = msg_text + __content.string(2919);
        let default = trim(str_s(cast::<Ext, _>(held_pct)).as_bytes());
        let mut pct;
        loop {
            let answer = ui.inputbox(&msg_text, &s(__content.string(2920)), &default);
            pct = to_long(val(remove_any(answer.as_bytes(), b"$ ,%").as_bytes()));
            if pct < 1 {
                return spin_off_subsidiary_exit();
            }
            if !(held_pct < pct) {
                break;
            }
        }
        pct_to_spin = pct;
    } else {
        pct_to_spin = held_pct;
    }
    msg_text = PbStr::new();
    basis_spun = g.hold_stock_basis[(slot, parent_id)] * (cast::<Ext, _>(pct_to_spin) / cast::<Ext, _>(held_pct));
    if pct_to_spin < 80
        || g.cur_year - 5 < sub_since_year
        || 69 < g.industry_num[sub_id]
        || mostly_liquid == 1
    {
        if sub_spec < -10 {
            return spin_off_subsidiary_exit();
        }
        is_taxable = 1;
        taxable_gain = g.market_cap[sub_id] * cast::<Ext, _>(pct_to_spin);
        let mut gain = taxable_gain - basis_spun;
        if basis_spun < Ext::from_f64(0.0) && held_pct - pct_to_spin < 80 {
            gain = taxable_gain + g.hold_stock_basis[(slot, parent_id)].abs();
        }
        taxable_gain = gain;
        if Ext::from_f64(0.0) < taxable_gain {
            let mut loss_carry = g.tax_loss_carry[parent_id];
            if Ext::from_f64(0.0) < loss_carry {
                loss_carry = Ext::from_f64(0.0);
            }
            spin_tax = taxable_gain + g.consol_tax_inc_ytd[parent_id] + loss_carry;
            let total_tax = sf(0.01f32) * cast::<Ext, _>(g.corp_tax_rate) * spin_tax;
            let mut base_tax =
                sf(0.01f32) * cast::<Ext, _>(g.corp_tax_rate) * (loss_carry + g.consol_tax_inc_ytd[parent_id]);
            if base_tax < Ext::from_f64(0.0) {
                base_tax = Ext::from_f64(0.0);
            }
            spin_tax = total_tax - base_tax;
            if spin_tax < Ext::from_f64(0.0) {
                spin_tax = Ext::from_f64(0.0);
            }
        }
        msg_text = s(__content.string(2921))
            + __content.string(2922)
            + &g.ent_name[parent_id]
            + __content.string(2923)
            + __content.string(2924)
            + __content.string(2925)
            + "\r"
            + "\r"
            + __content.string(2926);
        if pct_to_spin < 80 {
            let with = s(__content.string(2927))
                + &str_s(cast::<Ext, _>(pct_to_spin))
                + __content.string(2928)
                + &g.ent_symbol[sub_id];
            msg_text = replace(msg_text.as_bytes(), __content.bytes(2929), with.as_bytes());
            msg_text = msg_text
                + __content.string(2930)
                + __content.string(2931)
                + __content.string(2932)
                + "\r"
                + "\r";
        } else if g.cur_year - 5 < sub_since_year
            || 69 < g.industry_num[sub_id]
            || mostly_liquid == 1
        {
            msg_text = msg_text + "the " + &tax_agency + __content.string(2933);
            if g.cur_year - 5 < sub_since_year {
                msg_text = msg_text
                    + __content.string(2934)
                    + &g.ent_symbol[sub_id]
                    + __content.string(2935)
                    + __content.string(2936)
                    + __content.string(2937)
                    + &g.ent_symbol[parent_id]
                    + __content.string(2938)
                    + __content.string(2939)
                    + &str_s(cast::<Ext, _>(sub_since_year))
                    + ".";
            } else if mostly_liquid == 1 {
                msg_text = msg_text
                    + __content.string(2940)
                    + &g.ent_symbol[sub_id]
                    + __content.string(2941)
                    + __content.string(2942);
            } else {
                msg_text = msg_text
                    + __content.string(2943)
                    + &g.ent_symbol[sub_id]
                    + __content.string(2944)
                    + __content.string(2945)
                    + __content.string(2946)
                    + &g.ent_name[parent_id]
                    + ".";
            }
            msg_text = msg_text + "\r" + "\r";
        }
        if 10 < parent80_id {
            msg_text = msg_text
                + __content.string(2947)
                + &g.ent_name[cast::<i32, _>(parent80_id)]
                + ", the "
                + __content.string(2948)
                + &g.ent_name[parent_id]
                + __content.string(2949)
                + "of "
                + &g.ent_symbol[parent_id]
                + __content.string(2950)
                + "\r"
                + "\r";
        }
        if Ext::from_f64(0.5) < spin_tax {
            msg_text = msg_text
                + __content.string(2951)
                + &g.ent_symbol[parent_id]
                + " will "
                + __content.string(2952)
                + &g.ent_symbol[sub_id]
                + __content.string(2953)
                + "of "
                + &trim(g.currency_symbol.as_bytes())
                + &trim(fmt(spin_tax, b"* ###,.00").as_bytes())
                + " "
                + &g.million_prefix_lc
                + __content.string(2954)
                + __content.string(2955)
                + __content.string(2956)
                + "\r"
                + "\r";
        }
    } else if basis_spun < Ext::from_f64(0.0) {
        basis_spun = g.hold_stock_basis[(slot, parent_id)];
        msg_text = s(__content.string(2957))
            + &g.ent_symbol[parent_id]
            + ", "
            + "and "
            + &g.ent_symbol[parent_id]
            + __content.string(2958)
            + __content.string(2959)
            + &g.ent_symbol[sub_id]
            + __content.string(2960)
            + "\r"
            + "\r"
            + __content.string(2961)
            + &g.ent_symbol[parent_id]
            + __content.string(2962)
            + __content.string(2963)
            + __content.string(2964)
            + &g.ent_symbol[sub_id]
            + __content.string(2965)
            + &trim(g.currency_symbol.as_bytes())
            + &trim(fmt(basis_spun.abs(), b"* ###,.00").as_bytes())
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix
            + " "
            + __content.string(2966)
            + "\r"
            + "\r";
    }
    if !msg_text.is_empty() && 0 <= sub_spec {
        aux_text = PbStr::new();
        let mut title_prefix = s(__content.string(2967));
        if basis_spun < Ext::from_f64(0.0) && is_taxable == 0 {
            aux_text = s(__content.string(2968));
            title_prefix = PbStr::new();
        }
        msg_text = msg_text + "DOES " + &g.ent_symbol[parent_id] + __content.string(2969) + &aux_text + __content.string(2970);
        let title = title_prefix + __content.string(2971) + &g.ent_symbol[sub_id] + __content.string(2972);
        result = ui.msgbox(&msg_text, 8483, &title);
        if result != 6 {
            return spin_off_subsidiary_exit();
        }
    }
    if Ext::from_f64(0.0) <= basis_spun && is_taxable == 0 && 0 <= sub_spec {
        msg_text = s(__content.string(2973))
            + &tax_agency
            + "."
            + "\r"
            + "\r"
            + __content.string(2974);
        result = ui.msgbox(&msg_text, 8227, &s(__content.string(2975)));
        if result != 6 {
            return spin_off_subsidiary_exit();
        }
    }
    if is_taxable == 0 {
        ctrl_since = g.ctrl_since_year[sub_id];
    }
    result = spin_off_subsidiary(g, ui, rng, parent_id, sub_id, spin_mode, pct_to_spin, time_secs);
    if result == 1 && g.merger_target == g.active_entity {
        spin_off_cancel_merger_on_violation(g, ui);
    }
    if result == 1 {
        g.tmp_ext3 = cast::<Ext, _>(pct_to_spin) * g.market_cap[sub_id] * -Ext::from_f64(1.0);
        check_stock_split(g, parent_id);
        check_naked_option_margin(g, ui, rng, parent_id);
    }
    if result == 1 && is_taxable == 1 && 10 < parent80_id {
        let grand_parent_id = cast::<i32, _>(parent80_id);
        slot = find_holding_slot(g, grand_parent_id, parent_id);
        msg_text = str_s(cast::<Ext, _>(g.hold_stock_pct[(slot, cast::<i32, _>(parent80_id))]));
        g.news_msg = g.ent_symbol[parent_id].clone()
            + __content.string(2976)
            + &g.ent_symbol[sub_id]
            + __content.string(2977)
            + &msg_text
            + __content.string(2978)
            + &g.ent_symbol[cast::<i32, _>(parent80_id)];
        let mut m = s("The ")
            + &g.news_msg
            + __content.string(2979)
            + &g.ent_name[parent_id]
            + ", "
            + __content.string(2980)
            + __content.string(2981)
            + &g.ent_name[parent_id]
            + __content.string(2982)
            + &g.ent_name[sub_id]
            + __content.string(2983)
            + &g.ent_symbol[cast::<i32, _>(parent80_id)]
            + ".";
        show_quick_message(g, ui, &mut m);
        post_news(g, ui, 0, 0);
    }
    if result == 1 && is_taxable == 0 {
        let field_count = parse_count(g.ent_misc_data[sub_id].as_bytes(), b"|");
        if field_count < 32 {
            return spin_off_subsidiary_exit();
        }
        update_corp_control(g, ui, rng, sub_id);
        if 10 < g.parent80_owner {
            g.ctrl_since_year[sub_id] = ctrl_since;
            if ctrl_since < g.ctrl_since_year[parent_id] {
                g.ctrl_since_year[sub_id] = g.ctrl_since_year[parent_id];
            }
            if g.ctrl_since_year[parent_id] == 0 || g.cur_year == g.ctrl_since_year[parent_id] {
                return spin_off_subsidiary_exit();
            }
            msg_text = PbStr::new();
            for i in 1..=41 {
                aux_text = parse_any(g.ent_misc_data[sub_id].as_bytes(), b"|", i);
                if i == 28 {
                    aux_text = trim(str_s(cast::<Ext, _>(g.parent80_owner)).as_bytes());
                }
                msg_text = msg_text + &aux_text + "|";
            }
            g.ent_misc_data[sub_id] = msg_text;
        }
    }
    spin_off_subsidiary_exit()
}

/// Port of AiTaxFreeSpinoff (0x4c4e2c).
/// Scheduled AI move for the company `SimCoCount - 70` (>= 86; operating companies only with
/// probability 1/5; industry <= 69, active, 80%-owned by another company and not controlled by a
/// player): unless one of many filters applies (same name prefix as the parent, tax-loss situations
/// that favour keeping the consolidated return, insurer/insurer pairs, controlled for less than 5 years,
/// mostly liquid, negative basis, a parent rated below 7 or short of liquidity, a stake too large for the
/// parent's net worth and rating), the parent spins off its whole stake tax-free
/// (`SpinOffSubsidiaryMenu(parent, -sub)`), with a " [runtime resource wording] " headline when the
/// parent was 100% public.
pub fn ai_tax_free_spinoff(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let subsid_id = g.sim_co_count - 70;
    if subsid_id < 86 {
        return;
    }
    if rng.rnd() * Ext::from_f64(5.0) > Ext::from_f64(1.0) && 130 < subsid_id {
        return;
    }
    if 69 < g.industry_num[subsid_id] || g.entity_status[subsid_id] != 1 {
        return;
    }
    update_corp_control(g, ui, rng, subsid_id);
    if g.parent80_owner < 11 {
        return;
    }
    if 0 < get_controller_type(g, subsid_id) {
        return;
    }
    let parent_id = cast::<i32, _>(g.parent80_owner);
    if mid(g.ent_name[parent_id].as_bytes(), 1, 4) == mid(g.ent_name[subsid_id].as_bytes(), 1, 4)
        && rng.rnd() * Ext::from_f64(20.0) > Ext::from_f64(1.0)
    {
        return;
    }
    if g.exchange_rate * Ext::from_f64(300.0) < g.earnings_y1[subsid_id]
        && Ext::from_f64(0.0) < g.proj_qtr_earnings[subsid_id]
        && g.tax_loss_carry[parent_id] < -Ext::from_f64(1000.0) * g.exchange_rate
    {
        return;
    }
    if g.tax_loss_carry[subsid_id] < -Ext::from_f64(1000.0) * g.exchange_rate
        && -Ext::from_f64(100.0) * g.exchange_rate < g.tax_loss_carry[parent_id]
        && cast::<i32, _>(g.wholly_owned_by) == parent_id
        && g.industry_num[parent_id] == 1
        && Ext::from_f64(0.0) < g.proj_qtr_earnings[subsid_id]
    {
        return;
    }
    let mut checked_ins = false;
    if g.industry_num[parent_id] == 2 && g.industry_num[parent_id] == g.industry_num[subsid_id] {
        if 0 < g.rand_seed % 3 {
            return;
        }
        checked_ins = true;
    }
    if !checked_ins && g.industry_num[subsid_id] == 2 && rng.rnd() * Ext::from_f64(5.0) > Ext::from_f64(1.0) {
        return;
    }
    if g.cur_year - 5 < g.ctrl_since_year[subsid_id] {
        return;
    }
    if 2 < g.industry_num[subsid_id] {
        calc_entity_financials(g, ui, rng, subsid_id);
        if g.total_assets * sf(0.67f32) < g.t_bills[subsid_id] + g.cash[subsid_id] {
            return;
        }
    }
    let slot = find_stock_holding(g, parent_id, subsid_id);
    if slot < 1 {
        return;
    }
    if g.hold_stock_basis[(slot, parent_id)] < Ext::from_f64(0.0) {
        return;
    }
    let held_pct = cast::<i32, _>(g.hold_stock_pct[(slot, parent_id)]);
    let stake_value = to_quad(g.market_cap[subsid_id] * cast::<Ext, _>(held_pct));
    calc_entity_financials(g, ui, rng, parent_id);
    if g.credit_rating_num[parent_id] < 7 {
        return;
    }
    if 85 < parent_id && g.t_bills[parent_id] + g.cash[parent_id] < g.total_assets * sf(0.1f32) {
        return;
    }
    if g.net_worth * sf(0.2f32) < cast::<Ext, _>(stake_value) && g.credit_rating_num[parent_id] == 7 {
        return;
    }
    if g.net_worth * sf(0.25f32) < cast::<Ext, _>(stake_value) && g.credit_rating_num[parent_id] == 8 {
        return;
    }
    if g.net_worth * sf(0.3f32) < cast::<Ext, _>(stake_value) && 8 < g.credit_rating_num[parent_id] {
        return;
    }
    update_corp_control(g, ui, rng, parent_id);
    let public_pct = g.public_pct;
    spin_off_subsidiary_menu(g, ui, rng, parent_id, -subsid_id, time_secs);
    if 0 < find_stock_holding(g, parent_id, subsid_id) {
        return;
    }
    if public_pct == 100 {
        g.news_msg = g.ent_symbol[parent_id].clone()
            + __content.string(2984)
            + &str_s(cast::<Ext, _>(held_pct))
            + __content.string(2985)
            + &g.ent_symbol[subsid_id];
    }
    post_news(g, ui, parent_id, subsid_id);
}
