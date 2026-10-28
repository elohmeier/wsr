//! Player menus: `StockForStockMerger` (0x5a8815, the "Merge with another company" action) and
//! `LiquidateSubsidiary` (0x5a7387, tax-free or [runtime resource wording] the active company) with its GOSUB
//! blocks (`LiquidateSubsidiary_*`, 0x5a8284..0x5a87e5).
//!
//! `TIME$` / `DATE$` come in through `now` ([`LocalTime`]).
use crate::pb::real::{Ext, cast};


use super::liquidate_sub::liquidate_sub_into_parent;
use super::merge::merge_companies;
use super::prelude::*;
use crate::logic::control::update_corp_control;
use crate::logic::earnings::calc_projected_earnings;
use crate::logic::etf_derivs::corp_options::force_insider_short_cover;
use crate::logic::finance::calc_entity_financials;
use crate::logic::holdings::find_stock_holding;
use crate::logic::market::update_market_display;
use crate::logic::newgame::LocalTime;
use crate::logic::report::show_quick_message;
use crate::logic::reports_corp::divest_stock;
use crate::logic::reports_lists::update_control_chain;
use crate::logic::sim_entity::liquidation::liquidate_corporation;
use crate::logic::trading::select::in_box_symbol;
use crate::logic::turn::run_scenario_event;
use crate::pb::string::remove_any;

/// Port of StockForStockMerger (0x5a8815).
/// The player's merger of the active company (acquirer) with a target picked by stock symbol
/// (`InBox(3)` with `ActionCode = "MERGE"`): refuses a merger with itself or with a fund (industry 71); a
/// bank of the player who already merged banks this year (`BankMergerPlayer`) must first make the target
/// divest its other banks' stock; may start the cheat-mode equity-fund scenario (story 101); shows the
/// estimated merger costs (YES/NO/CANCEL); a nearly bankrupt target ("D") may provoke a shareholder revolt
/// or injunction; for a target the player controls the underwriters' "fairness" premium range is shown
/// (narrower if the holdings suggest a reverse merger, 1..3% for sister subsidiaries); then asks the
/// premium (`INPUTBOX$`, re-asked until in range; 0 / negative cancels) and runs
/// [`merge_companies`] in player mode. Afterwards updates control chains and projected earnings and
/// forces insider short covers.
///
/// Bug (BUGS.md #236): the offer to replace the acquirer's management with the target's better team
/// compares the acquirer's industry with a local that is never assigned (always 0), so it never appears.
pub fn stock_for_stock_merger(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime) {
    let __content = crate::content::current();
    if !(10 < g.active_entity && g.entity_status[g.active_entity] == 1) {
        return;
    }
    // locals (zeroed by the frame setup)
    let mut held_bank_id: i32 = 0;
    let mut target_controlled: i16 = 0;
    let target_industry: i16 = 0; // never assigned in the original
    let mut msg_text: PbStr;
    let mut title_text: PbStr;

    let mut acquirer_id = g.active_entity;
    g.action_code = s("MERGE");
    msg_text = s(__content.string(9559)) + &g.ent_name[g.active_entity];
    msg_text = msg_text + __content.string(9560) + "\r\n" + "\r\n";
    g.input_dlg_label = PbStr::new();
    g.input_dlg_prompt = msg_text;
    g.input_dlg_title = s(__content.string(9561)) + &g.active_entity_name;
    g.lookup_filter = PbStr::new();
    in_box_symbol(g, ui, rng);
    if g.target_corp <= 10 {
        return;
    }
    let mut target_id = g.target_corp;
    g.target_corp = 0;
    g.lookup_entity_name = PbStr::new();
    if g.active_entity == target_id {
        let mut m = g.active_entity_name.clone() + __content.string(9562);
        show_quick_message(g, ui, &mut m);
        return;
    }
    if g.industry_num[target_id] == 71 {
        msg_text = s(__content.string(9563));
        held_bank_id = g.fund_adviser[target_id];
        if g.cur_player == g.corp_controller[held_bank_id] {
            title_text = s(__content.string(9564));
        } else {
            msg_text = s(__content.string(9565))
                + &g.ent_name[held_bank_id]
                + ", "
                + __content.string(9566)
                + &g.active_entity_name
                + __content.string(9567);
            title_text = s(__content.string(9568));
        }
        ui.msgbox(&msg_text, 0x2010, &title_text);
        return;
    }
    while g.industry_num[g.active_entity] == 1 && 0 < g.bank_merger_player && g.cur_player == g.bank_merger_player {
        held_bank_id = 0;
        if g.hold_stock_id[(0, target_id)] == 0 {
            break;
        }
        let mut slot = 1;
        while slot < 16 {
            held_bank_id = cast::<i32, _>(g.hold_stock_id[(slot, target_id)]);
            if 10 < held_bank_id && held_bank_id < 86 && g.active_entity != held_bank_id {
                break;
            }
            slot += 1;
        }
        if held_bank_id < 11 {
            break;
        }
        msg_text = s(__content.string(9569))
            + &g.ent_name[target_id]
            + __content.string(9570)
            + &g.ent_name[held_bank_id]
            + __content.string(9571)
            + &g.ent_name[target_id]
            + " "
            + __content.string(9572)
            + &g.ent_name[held_bank_id]
            + " ("
            + &g.ent_symbol[held_bank_id]
            + "), "
            + __content.string(9573)
            + &g.country_name[g.country_idx[target_id]]
            + __content.string(9574)
            + "\r"
            + "\r";
        if g.cur_player != g.corp_controller[target_id] {
            msg_text = msg_text + __content.string(9575);
            ui.msgbox(&msg_text, 0x2040, &s(__content.string(9576)));
            return;
        }
        msg_text = msg_text + __content.string(1936) + &g.ent_name[target_id] + __content.string(9577) + &g.ent_name[held_bank_id] + "?";
        let answer = ui.msgbox(&msg_text, 0x2124, &(s("Sell ") + &g.ent_symbol[held_bank_id] + __content.string(9578)));
        if answer == 7 {
            return;
        }
        divest_stock(g, ui, rng, held_bank_id, target_id, 0);
        if 0 < find_stock_holding(g, target_id, held_bank_id) {
            let mut m = s(__content.string(9579));
            show_quick_message(g, ui, &mut m);
            return;
        }
    }
    update_corp_control(g, ui, rng, g.active_entity);
    let acq_holder_pct = g.largest_holder_pct;
    let acq_holder = cast::<i32, _>(g.largest_holder);
    update_corp_control(g, ui, rng, target_id);
    let tgt_holder_pct = g.largest_holder_pct;
    let tgt_holder = cast::<i32, _>(g.largest_holder);
    let acq_industry = g.industry_num[g.active_entity];
    if g.cur_player == g.corp_controller[target_id] {
        target_controlled = 1;
    }
    if g.cheat_eq_fund_target == 0 && g.event_status[101] == 0 && g.scenario_num == 0 {
        if Ext::from_f64(4.0) < rng.rnd() * Ext::from_f64(5.0) && g.cheat_mode == "Y" {
            g.cheat_eq_fund_target = cast::<i16, _>(target_id);
        }
        if g.cur_player == g.corp_controller[target_id] {
            g.cheat_eq_fund_target = 0;
        }
        if g.scenario_corp != 0 {
            g.cheat_eq_fund_target = 0;
        }
    }
    if cast::<i32, _>(g.cheat_eq_fund_target) == target_id && g.event_status[101] == 0 && g.cheat_mode == "Y" {
        g.scenario_corp = cast::<i16, _>(target_id);
        g.scenario_num = 101;
        run_scenario_event(g, ui, rng, now);
        if cast::<i32, _>(g.cheat_eq_fund_target) == -target_id {
            g.cheat_eq_fund_target = cast::<i16, _>(target_id);
            return;
        }
    }
    // DOUBLE local: MarketCap * 0.6!, reduced by the part the acquirer already owns
    let mut cost: Ext = crate::pb::numeric::double(g.market_cap[target_id] * sf(0.6f32));
    for slot in 1..=15 {
        if cast::<i32, _>(g.hold_stock_id[(slot, g.active_entity)]) == target_id {
            let factor: Ext = Ext::from_f64(1.0) - sf(0.01f32) * cast::<Ext, _>(g.hold_stock_pct[(slot, g.active_entity)]);
            cost = crate::pb::numeric::double(factor * cost);
        }
    }
    let cost_text = fmt_trim(cost, b"* ###");
    if Ext::from_f64(0.0) < val(cost_text.as_bytes()) {
        msg_text = s(__content.string(9580))
            + __content.string(9581)
            + &g.active_entity_name
            + __content.string(9582)
            + &trim(g.currency_symbol.as_bytes())
            + &cost_text
            + " "
            + &g.million_prefix_lc
            + __content.string(9583)
            + __content.string(9584)
            + "\r"
            + "\r";
        msg_text = msg_text + __content.string(9585) + &g.active_entity_name + " and " + &g.ent_name[target_id] + "?";
        let answer = ui.msgbox(&msg_text, 0x2023, &g.app_title.clone());
        if answer == 2 || answer == 7 {
            return;
        }
    }
    calc_entity_financials(g, ui, rng, target_id);
    if g.credit_rating_str == "D" && (g.cur_quarter + g.cur_year) % 5 != 3 {
        update_corp_control(g, ui, rng, g.active_entity);
        let mut revolt = false;
        if g.credit_rating_num[target_id] == 0 && 0 < g.public_float_pct && g.cur_player == g.corp_controller[target_id] {
            revolt = true;
        }
        if g.cur_year % 4 + g.cur_quarter + 51 < cast::<i32, _>(g.public_float_pct) {
            revolt = true;
        }
        if 0 < g.largest_holder && g.cur_player != g.corp_controller[cast::<i32, _>(g.largest_holder)] && 30 < g.public_pct {
            revolt = true;
        }
        if revolt {
            title_text = s(__content.string(9586));
            msg_text = s(__content.string(9587))
                + &g.active_entity_name
                + " have "
                + __content.string(9588)
                + __content.string(9589)
                + &g.ent_name[target_id]
                + __content.string(9590)
                + __content.string(9591)
                + "\r"
                + "\r"
                + __content.string(9592);
            if g.credit_rating_num[target_id] == 0 && g.public_float_pct < 50 {
                title_text = s(__content.string(9593));
                msg_text = s(__content.string(9594))
                    + __content.string(9595)
                    + &g.ent_symbol[g.active_entity]
                    + __content.string(9596)
                    + __content.string(9597)
                    + __content.string(9598)
                    + &g.cur_player_name
                    + __content.string(9599)
                    + __content.string(9600);
            }
            ui.msgbox(&msg_text, 0x2010, &title_text);
            return;
        }
    }
    let mut min_premium: i16 = 5;
    let mut max_premium: i16 = 100;
    if g.cur_player == g.corp_controller[target_id] {
        min_premium = cast::<i16, _>((g.cur_year + target_id + g.cur_quarter) % 6 + 10);
        max_premium = cast::<i16, _>((g.cur_year + g.active_entity + g.cur_quarter) % 10 + 25);
        if acq_holder_pct < tgt_holder_pct {
            held_bank_id = 1;
        }
        if g.active_entity == tgt_holder {
            held_bank_id = 0;
        }
        if target_id == acq_holder {
            held_bank_id = 1;
        }
        if held_bank_id == 1 {
            min_premium -= 7;
            max_premium -= 10;
            if 50 < tgt_holder_pct && acq_holder_pct < tgt_holder_pct {
                min_premium = 3;
                max_premium = 5;
            }
            max_premium = cast::<i16, _>(g.cur_quarter + cast::<i32, _>(max_premium));
        }
        update_corp_control(g, ui, rng, g.active_entity);
        let acq_owner = g.wholly_owned_by;
        update_corp_control(g, ui, rng, target_id);
        if 0 < g.wholly_owned_by && acq_owner == g.wholly_owned_by {
            min_premium = 1;
            max_premium = 3;
        }
        msg_text = s(__content.string(9601))
            + __content.string(9602)
            + __content.string(9603)
            + &str_s(cast::<Ext, _>(min_premium))
            + "% to"
            + &str_s(cast::<Ext, _>(max_premium))
            + __content.string(9604)
            + __content.string(9605)
            + &g.ent_symbol[target_id]
            + ".";
        show_quick_message(g, ui, &mut msg_text);
    }
    let mut premium_pct: i16 = min_premium - 1;
    loop {
        if min_premium <= premium_pct && premium_pct <= max_premium {
            merge_companies(g, ui, rng, g.active_entity, target_id, 0, premium_pct, now.time_secs());
            if acq_industry < 70
                && acq_industry == target_industry
                && acquirer_id == g.active_entity
                && target_controlled != 1
                && g.entity_status[g.active_entity] == 1
                && g.entity_status[target_id] == 1
            {
                // dead code (target_industry is always 0), kept for fidelity
                let ae = g.active_entity;
                let mut similar = false;
                if acq_industry < 3 {
                    if g.market_cap[ae] * sf(0.4f32) < g.market_cap[target_id] && g.market_cap[target_id] < g.market_cap[ae] * sf(1.5f32) {
                        similar = true;
                    }
                } else {
                    if g.business_assets[ae] * sf(0.4f32) < g.business_assets[target_id]
                        && g.business_assets[target_id] < g.business_assets[ae] * sf(1.5f32)
                    {
                        similar = true;
                    }
                    calc_entity_financials(g, ui, rng, ae);
                    if g.business_assets[ae] < g.total_assets * sf(0.2f32) {
                        similar = false;
                    }
                }
                if similar
                    && g.cur_quarter + 4 < cast::<i32, _>(g.profit_margin[target_id])
                    && (cast::<i32, _>(g.profit_margin[ae])) + 3 < cast::<i32, _>(g.profit_margin[target_id])
                {
                    msg_text = s(__content.string(9606))
                        + &g.ent_name[target_id]
                        + __content.string(9607)
                        + __content.string(9608)
                        + &g.active_entity_name
                        + __content.string(9609)
                        + &g.active_entity_name
                        + " "
                        + __content.string(9610)
                        + &g.ent_name[target_id]
                        + __content.string(9611)
                        + &g.ent_name[target_id]
                        + " "
                        + __content.string(9612)
                        + __content.string(9613);
                    let answer = ui.msgbox(&msg_text, 0x2024, &s(__content.string(9614)));
                    if answer == 6 {
                        let mut m = s(__content.string(9615))
                            + &g.ent_symbol[target_id]
                            + ") is "
                            + __content.string(9616)
                            + &g.active_entity_name;
                        show_quick_message(g, ui, &mut m);
                        g.profit_margin[ae] = g.profit_margin[target_id];
                        g.profit_margin[target_id] = g.profit_margin[target_id] - 2;
                        if 1 < g.disc_spend_pct[target_id] {
                            g.disc_spend_pct[target_id] = g.disc_spend_pct[target_id] - 2;
                        }
                    }
                }
            }
            if g.entity_status[acquirer_id] == 2 {
                std::mem::swap(&mut acquirer_id, &mut target_id);
            }
            update_control_chain(g, ui, rng, g.active_entity);
            update_control_chain(g, ui, rng, target_id);
            calc_projected_earnings(g, ui, rng, g.active_entity);
            calc_projected_earnings(g, ui, rng, target_id);
            g.action_code = PbStr::new();
            force_insider_short_cover(g, ui, rng);
            break;
        }
        msg_text = s(__content.string(9617))
            + &str_s(cast::<Ext, _>(min_premium))
            + "% to"
            + &str_s(cast::<Ext, _>(max_premium))
            + __content.string(9618)
            + __content.string(9619)
            + &g.ent_name[target_id]
            + " ("
            + &g.ent_symbol[target_id]
            + "):";
        let input_text = ui.inputbox(&msg_text, &s(__content.string(9620)), &trim(str_s(cast::<Ext, _>(min_premium)).as_bytes()));
        premium_pct = to_integer(val(remove_any(input_text.as_bytes(), b"$ ,%").as_bytes()));
        if premium_pct < 0 || (premium_pct == 0 && 0 < min_premium) {
            break;
        }
        if premium_pct < min_premium || max_premium < premium_pct {
            let mut m = s(__content.string(9621))
                + &str_s(cast::<Ext, _>(min_premium))
                + "% and"
                + &str_s(cast::<Ext, _>(max_premium))
                + "% !";
            show_quick_message(g, ui, &mut m);
        }
    }
}

/// The locals of LiquidateSubsidiary that its GOSUBs share.
struct LiqFrame {
    sub_id: i32,
    sub_net_worth: Ext,
    parent_name: PbStr,
    country_idx: i32,
    the_article: PbStr,
}

/// Port of LiquidateSubsidiary (0x5a7387).
/// Liquidates the active company `ActiveEntity` (> 10, active): `liq_mode = 1` tries a tax-free
/// liquidation into its 100% parent (Section 332 for U.S. companies; the parent must be in the same
/// industry, or a holding company for an operating subsidiary — else the player may switch to a taxable
/// one), `liq_mode = 2` goes straight to a taxable liquidation (Section 331). Banks and insurers can only be
/// liquidated into a 100% parent of their kind and only if the parent's net worth stays positive; a nearly
/// bankrupt parent (rating < 2) blocks it while the subsidiary has creditors. Warns about a book value far
/// below market value (`WarnBookValue`) and lost loss carryovers (`WarnNolLoss`), each can cancel. The
/// tax-free path runs [`liquidate_sub_into_parent`] (which may leave the company alive, then a taxable
/// liquidation is offered unless `ActionCode = "NOL"`), the taxable one `LiquidateCorporation` (player
/// mode) if the net worth is not negative.
///
/// Error paths (`ShowRuntimeError`) are not ported.
pub fn liquidate_subsidiary(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, liq_mode: i32, now: &LocalTime) {
    let __content = crate::content::current();
    let corp_id = g.active_entity;
    let mut f = LiqFrame {
        sub_id: g.active_entity,
        sub_net_worth: Ext::from_f64(0.0),
        parent_name: PbStr::new(),
        country_idx: 0,
        the_article: PbStr::new(),
    };
    if g.entity_status[g.active_entity] != 1 || corp_id < 11 {
        return;
    }
    let mut is_us_corp = 0;
    let mut sec331 = PbStr::new();
    let mut sec332 = PbStr::new();
    let mut liq_type = 1;
    let sub_industry = g.industry_num[corp_id];
    update_corp_control(g, ui, rng, corp_id);
    let parent80 = g.parent80_owner;
    let wholly_owner = g.wholly_owned_by;
    if g.public_pct != 0 {
        liq_type = 2;
    }
    let parent_id = cast::<i32, _>(g.wholly_owned_by);
    if 0 < parent_id {
        calc_entity_financials(g, ui, rng, parent_id);
    }
    let parent_net_worth = g.net_worth;
    calc_entity_financials(g, ui, rng, f.sub_id);
    f.sub_net_worth = g.net_worth;
    if 10 < g.parent80_owner {
        f.parent_name = g.ent_name[cast::<i32, _>(g.parent80_owner)].clone();
        if g.country_idx[cast::<i32, _>(g.parent80_owner)] == 0 {
            is_us_corp = 1;
        }
    }
    if g.country_idx[f.sub_id] == 0 {
        is_us_corp = 1;
    }
    if is_us_corp == 1 {
        sec331 = s(__content.string(9622));
        sec332 = s(__content.string(9623));
    }
    if parent_id < 11 {
        liq_type = 2;
    } else {
        let parent_industry = g.industry_num[parent_id];
        if sub_industry != parent_industry {
            liq_type = 2;
        }
        if parent_industry == 70 && 2 < sub_industry {
            liq_type = 1;
        }
        if sub_industry == 70 {
            liq_type = 1;
        }
    }
    if liq_type == 2 && 2 < sub_industry && liq_mode == 1 {
        let mut kind_text = s(__content.string(9624))
            + &sec332
            + __content.string(9625)
            + &g.ent_name[f.sub_id]
            + " "
            + __content.string(9626);
        if sub_industry == 70 {
            kind_text = kind_text + ".";
        } else {
            kind_text = kind_text + __content.string(9627);
        }
        kind_text = kind_text + "\r" + "\r";
        let msg_text = s(__content.string(9628)) + &sec331 + __content.string(9629) + "of " + &g.ent_name[f.sub_id] + "?";
        let answer = ui.msgbox(&(kind_text + &msg_text), 0x2123, &g.app_title.clone());
        if answer != 6 {
            if 10 < wholly_owner {
                let mut m = s("NOTE: ")
                    + &g.active_entity_name
                    + __content.string(9630)
                    + &sec332
                    + __content.string(9629)
                    + __content.string(9631);
                show_quick_message(g, ui, &mut m);
            }
            return;
        }
    }
    if liq_mode == 2 {
        liq_type = 2;
    }
    if liq_type == 2 && sub_industry < 3 {
        let mut kind_text = s("bank");
        let mut industry_word = s("banking");
        if sub_industry == 2 {
            kind_text = s(__content.string(3968));
            industry_word = s(__content.string(2822));
        }
        f.country_idx = g.country_idx[f.sub_id];
        liquidate_subsidiary_article_the(&mut f);
        let msg_text = s(__content.string(9632))
            + &g.active_entity_name
            + ", "
            + "("
            + &kind_text
            + __content.string(9633)
            + __content.string(9634)
            + &kind_text
            + __content.string(9635)
            + __content.string(9636)
            + &industry_word
            + __content.string(9637)
            + &f.the_article
            + &g.country_name[g.country_idx[f.sub_id]]
            + ".";
        ui.msgbox(&msg_text, 0x2010, &s(__content.string(9638)));
        return;
    }
    if liq_type == 1 {
        calc_entity_financials(g, ui, rng, parent_id);
        let mut tax_free_ok = true;
        if sub_industry < 3 {
            let mut min_parent_worth = g.market_cap[corp_id] * Ext::from_f64(100.0) - f.sub_net_worth;
            if min_parent_worth < Ext::from_f64(0.0) {
                min_parent_worth = Ext::from_f64(0.0);
            }
            if Ext::from_f64(0.0) < parent_net_worth && parent_net_worth < min_parent_worth {
                let mut m = g.active_entity_name.clone()
                    + __content.string(9639)
                    + __content.string(9640)
                    + __content.string(9641)
                    + &g.ent_name[parent_id]
                    + ") net "
                    + __content.string(9642);
                show_quick_message(g, ui, &mut m);
                return;
            }
        } else if g.credit_rating_num[parent_id] < 2 {
            calc_entity_financials(g, ui, rng, f.sub_id);
            if g.industry_num[parent_id] == 1 && g.bonds_issued[f.sub_id] == 0 && g.tmp_player_advances == Ext::from_f64(0.0) && g.lender_id == parent_id {
                // creditors are the parent bank itself
            } else if Ext::from_f64(0.0) < g.tmp_player_advances + g.bank_debt[f.sub_id] + cast::<Ext, _>(g.bonds_issued[f.sub_id]) || sub_industry < 3 {
                tax_free_ok = false;
            }
            if !tax_free_ok {
                let mut m = g.active_entity_name.clone()
                    + __content.string(9643)
                    + __content.string(9644)
                    + "to "
                    + &f.parent_name
                    + __content.string(9645);
                show_quick_message(g, ui, &mut m);
                return;
            }
        }
        if 10 < wholly_owner && liquidate_subsidiary_warn_book_value(g, ui, &f) {
            return;
        }
        f.country_idx = g.country_idx[corp_id];
        liquidate_subsidiary_article_the(&mut f);
        let mut m = s(__content.string(9646))
            + &sec332
            + __content.string(9647)
            + &g.ent_symbol[corp_id]
            + " into "
            + &g.ent_name[parent_id]
            + __content.string(9648)
            + &f.the_article
            + &g.country_name[g.country_idx[corp_id]]
            + ".";
        show_quick_message(g, ui, &mut m);
        g.tmp_ext1 = Ext::from_f64(0.0);
        g.action_code = PbStr::new();
        liquidate_sub_into_parent(g, ui, rng, parent_id, f.sub_id, now.time_secs());
        if g.entity_status[f.sub_id] != 1 && !(2 < g.industry_num[parent_id] && Ext::from_f64(0.0) <= g.proj_qtr_earnings[parent_id]) {
            calc_projected_earnings(g, ui, rng, parent_id);
        }
        liquidate_subsidiary_refresh_sel_entity(g, ui, rng, &f, now);
        if g.action_code == "NOL" {
            g.action_code = PbStr::new();
            return;
        }
        if g.entity_status[f.sub_id] == 1 && 2 < g.industry_num[f.sub_id] {
            let msg_text = s(__content.string(9628)) + &sec331 + __content.string(9629) + "of " + &g.ent_name[f.sub_id] + "?";
            let answer = ui.msgbox(&msg_text, 0x2123, &s(__content.string(9649)));
            if answer != 6 {
                return;
            }
            liq_type = 2;
        }
    }
    if liq_type != 2 {
        return;
    }
    if Ext::from_f64(0.0) <= f.sub_net_worth {
        if liquidate_subsidiary_warn_nol_loss(g, ui, &f) {
            return;
        }
        if 10 < parent80 && liquidate_subsidiary_warn_book_value(g, ui, &f) {
            return;
        }
        liquidate_corporation(g, ui, rng, f.sub_id, 0, now);
        if g.entity_status[f.sub_id] != 1 && 10 < parent80 {
            let parent80_id = cast::<i32, _>(parent80);
            calc_projected_earnings(g, ui, rng, parent80_id);
        }
        liquidate_subsidiary_refresh_sel_entity(g, ui, rng, &f, now);
        return;
    }
    let mut m = s(__content.string(9650))
        + &g.active_entity_name
        + __content.string(9651)
        + __content.string(9652)
        + __content.string(9653);
    show_quick_message(g, ui, &mut m);
}

/// Port of LiquidateSubsidiary_WarnBookValue (0x5a8284), a GOSUB of [`liquidate_subsidiary`].
/// If the subsidiary's net worth is below 80 x its market cap (stock far above book value), warns that the
/// parent's book value will drop and asks whether to proceed (YES/NO/CANCEL). Returns `true` when the
/// routine is to be left (`EXIT SUB` from the GOSUB, LiquidateSubsidiary_Exit 0x5a87e5).
fn liquidate_subsidiary_warn_book_value(g: &mut Globals, ui: &mut dyn Ui, f: &LiqFrame) -> bool {
    let __content = crate::content::current();
    if f.sub_net_worth < g.market_cap[f.sub_id] * Ext::from_f64(80.0) {
        let msg_text = s(__content.string(9654))
            + &g.ent_symbol[f.sub_id]
            + __content.string(9655)
            + __content.string(9656)
            + __content.string(9657)
            + &f.parent_name
            + __content.string(9658)
            + __content.string(9659)
            + &f.parent_name
            + " "
            + __content.string(9660)
            + &g.ent_symbol[f.sub_id]
            + __content.string(9661)
            + &f.parent_name
            + __content.string(9662)
            + "\r"
            + "\r"
            + __content.string(9663)
            + &g.ent_name[f.sub_id]
            + " NOW?";
        let answer = ui.msgbox(&msg_text, 0x2123, &s(__content.string(9664)));
        if answer != 6 {
            return true;
        }
    }
    false
}

/// Port of LiquidateSubsidiary_WarnNolLoss (0x5a8469), a GOSUB of [`liquidate_subsidiary`].
/// If the subsidiary has a tax loss carryover above 25 million (x exchange rate), warns that a liquidation
/// loses it (YES/NO/CANCEL). Returns `true` when the routine is to be left.
fn liquidate_subsidiary_warn_nol_loss(g: &mut Globals, ui: &mut dyn Ui, f: &LiqFrame) -> bool {
    let __content = crate::content::current();
    if g.tax_loss_carry[f.sub_id] < -Ext::from_f64(25.0) * g.exchange_rate {
        let msg_text = s(__content.string(9665))
            + &g.ent_name[f.sub_id]
            + __content.string(3431)
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(g.tax_loss_carry[f.sub_id].abs(), b"#################")
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix
            + "."
            + "\r"
            + "\r"
            + __content.string(9666);
        let answer = ui.msgbox(&msg_text, 0x2123, &s(__content.string(9667)));
        if answer != 6 {
            return true;
        }
    }
    false
}

/// Port of LiquidateSubsidiary_ArticleThe (0x5a85f6), a GOSUB of [`liquidate_subsidiary`]:
/// `"the "` before the names of countries 0, 30, 35, 50 (U.S. etc.), else `""`.
fn liquidate_subsidiary_article_the(f: &mut LiqFrame) {
    let c = f.country_idx;
    f.the_article = if c == 0 || c == 30 || c == 35 || c == 50 { s("the ") } else { PbStr::new() };
}

/// Port of LiquidateSubsidiary_RefreshSelEntity (0x5a8681), a GOSUB of [`liquidate_subsidiary`]:
/// `SelCorpName = EntName(SelEntity)` (and the main window's name / symbol controls, UI only); refreshes
/// the market display when the liquidated company is still the active entity.
fn liquidate_subsidiary_refresh_sel_entity(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &LiqFrame, now: &LocalTime) {
    g.sel_corp_name = g.ent_name[g.sel_entity].clone();
    // (UI) CONTROL SET TEXT hDlgMain, 169 / 202: name and symbol ("N/A" for players)
    if g.active_entity == f.sub_id {
        update_market_display(g, ui, rng, now.time_secs());
    }
}

/// Port of LiquidateSubsidiary_Exit (0x5a87e5): the common `EXIT SUB` of [`liquidate_subsidiary`]
/// (nothing to do in the port; the GOSUBs signal it through their return value).
#[allow(dead_code)]
fn liquidate_subsidiary_exit() {}
