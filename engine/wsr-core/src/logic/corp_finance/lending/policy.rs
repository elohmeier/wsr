//! Corporate policy changes (growth rate, productivity spending, dividends) and the AutoPilot toggle.
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::super::prelude::*;
use crate::logic::cashflow::{msg_corp_only_function, show_cash_flow_projection};
use crate::logic::control::find_option_positions;
use crate::logic::finance::{calc_entity_financials, get_industry_category};
use crate::logic::report::show_quick_message;
use crate::logic::valuation::calc_dividend_yield;
use crate::pb::string::{parse_count_any, remove_any};

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

/// Port of ChangeCorpPolicy (0x5acf48).
/// Management menu: sets the active company's growth rate (`policy_code = "GR"`, -10..60%, banks
/// -5..10%, insurers -10..30%; not for industries 70/71), productivity (R&D / marketing) spending
/// (`"RD"`, 0..30%; not for industries >= 70) or annual dividend per share (`"DV"`, up to a maximum
/// from the credit rating: 20% of the share price for "AAA", else (rating + 2)% up to 15%, never below
/// the current dividend; a rating below "B" (2) only allows keeping or cutting an existing dividend).
/// Asks through INPUTBOX until the value is valid; warns about the growth-rate throttle of an
/// AutoPilot company (and offers to turn AutoPilot off), posts the news, shows the cash-flow
/// projection (`ShowCashFlowProjection(…, 1)`) for a non-bank and warns when an increase leads to a
/// deficit. A dividend change of a company with options outstanding that does not favour the player's
/// own option positions is recorded in `RecentDividendChangeList` (last 10).
///
/// Quirk (BUGS.md #313): for an R&D change of at most 3 points, or a growth-rate increase to a value <= 0, `NewsMsg` is
/// not set and whatever it held is posted.
pub fn change_corp_policy(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, policy_code: &PbStr) {
    let __content = crate::content::current();
    let ae = g.active_entity;
    if ae < 11 {
        msg_corp_only_function(g, ui);
        return;
    }
    let pc = policy_code.clone();
    let mut applicable = true;
    if pc == "GR" {
        applicable = g.industry_num[ae] < 70;
    } else if 69 < g.industry_num[ae] && pc == "RD" {
        applicable = false;
    }
    if !applicable {
        let mut m = s(__content.string(3466))
            + &g.ind_name[cast::<i32, _>(g.industry_num[ae])]
            + __content.string(1547);
        show_quick_message(g, ui, &mut m);
        return;
    }
    let mut max_rate_str = s("60");
    let mut min_rate_str = s("-10");
    let mut msg_text = PbStr::new();
    let mut dlg_title = PbStr::new();
    let mut old_value: i16 = 0;
    let mut old_dividend: Ext = Ext::from_f64(0.0);
    let mut max_dividend: Ext = Ext::from_f64(0.0);
    let mut max_dividend_str = PbStr::new();
    let mut answer: PbStr;
    if pc == "GR" {
        answer = s(__content.string(3467));
        if g.industry_num[ae] == 1 {
            answer = s(__content.string(3468));
            max_rate_str = s("10");
            min_rate_str = s("-5");
        }
        if g.industry_num[ae] == 2 {
            answer = s(__content.string(3469));
            max_rate_str = s("30");
        }
        msg_text = s(__content.string(3470))
            + &g.active_entity_name
            + __content.string(3471)
            + &answer
            + ")"
            + "\r"
            + "\r"
            + "Range: "
            + &min_rate_str
            + "% to "
            + &max_rate_str
            + "%";
        dlg_title = s(__content.string(3472)) + &g.ent_symbol[ae];
        old_value = g.growth_rate[ae];
    } else if pc == "RD" {
        let category = get_industry_category(g.industry_num[ae]);
        answer = s(__content.string(3473));
        if category == 2 {
            answer = s(__content.string(3474));
        }
        let mut basis_text = s("sales");
        if g.industry_num[ae] == 1 {
            basis_text = s(__content.string(3475));
        }
        if g.industry_num[ae] == 2 {
            basis_text = s(__content.string(3476));
        }
        msg_text = s(__content.string(3477))
            + &basis_text
            + __content.string(3478)
            + &g.active_entity_name
            + __content.string(3479)
            + __content.string(3480)
            + &answer;
        dlg_title = s(__content.string(3481)) + &g.ent_symbol[ae];
        old_value = g.disc_spend_pct[ae];
    } else if pc == "DV" {
        g.tmp_share_price = g.market_cap[ae] / g.shares_outstanding[ae];
        old_value = to_integer(g.dividend_per_share[ae]);
        old_dividend = g.dividend_per_share[ae];
        let basis_text = fmt_trim(old_dividend, b"* ###,.00");
        calc_entity_financials(g, ui, rng, ae);
        calc_dividend_yield(g, ae);
        let mut credit_rating = g.credit_rating_num[ae];
        if g.industry_num[ae] < 3 {
            if 8 < credit_rating {
                credit_rating = 8;
            }
        } else if credit_rating != 9 || g.tmp_accrued_inc_tax + g.capital_taxes_total < g.tmp_total_debt {
            if credit_rating == 10 && g.tmp_accrued_inc_tax + g.capital_taxes_total < g.tmp_total_debt {
                credit_rating = 9;
            }
        } else {
            credit_rating = 10;
        }
        if 2 <= credit_rating {
            if credit_rating == 10 {
                max_dividend = g.tmp_share_price * sf(0.2f32);
                if g.tmp_share_price * sf(0.2f32) < old_dividend {
                    max_dividend = old_dividend;
                }
            } else {
                let mut payout_pct = g.credit_rating_num[ae] + 2;
                if 15 < payout_pct {
                    payout_pct = 15;
                }
                max_dividend = g.tmp_share_price * cast::<Ext, _>(payout_pct) * sf(0.01f32);
                if max_dividend < old_dividend {
                    max_dividend = old_dividend;
                }
            }
        } else {
            max_dividend = old_dividend;
            if old_dividend <= Ext::from_f64(0.0) {
                let mut m = g.ent_name[ae].clone()
                    + __content.string(3482);
                show_quick_message(g, ui, &mut m);
                return;
            }
        }
        max_dividend_str = fmt_trim(max_dividend - sf(0.005f32), b"* ###.00");
        msg_text = s(__content.string(3483))
            + &g.active_entity_name
            + __content.string(3484)
            + __content.string(3485)
            + "\r"
            + "\r"
            + __content.string(3486)
            + &max_dividend_str
            + "\r"
            + "\r"
            + __content.string(3487)
            + &basis_text
            + __content.string(1071);
        dlg_title = s(__content.string(3488)) + &g.ent_symbol[ae];
    }

    let mut done = false;
    let mut default_text = str_s(cast::<Ext, _>(old_value)) + "%";
    if pc == "DV" {
        default_text = PbStr::new();
    }
    let mut extra_note = PbStr::new();
    let mut dividend_raised = PbStr::new();
    let mut new_int: i32;
    let mut new_value: Ext;
    loop {
        answer = ui.inputbox(&msg_text, &dlg_title, &default_text);
        new_int = to_long(val(remove_any(answer.as_bytes(), b"$ ,%").as_bytes()));
        new_value = val(remove_any(answer.as_bytes(), b"$ ,%").as_bytes());
        if pc == "DV" {
            if answer.is_empty() || new_value == old_dividend {
                let mut m = s(__content.string(3489));
                show_quick_message(g, ui, &mut m);
                return;
            }
            dividend_raised = s("N");
            if old_dividend < new_value {
                dividend_raised = s("Y");
            }
        } else if answer.is_empty() || cast::<i32, _>(old_value) == new_int {
            let mut m = s(__content.string(3490));
            show_quick_message(g, ui, &mut m);
            return;
        }
        if pc == "GR" {
            let cp = g.cur_player;
            if 130 < ae
                && g.industry_num[ae] < 70
                && g.player_growth_throttle[cp] < 60
                && (cast::<i32, _>(g.player_growth_throttle[cp])) < new_int
                && new_int < 61
                && g.auto_pilot[ae] != 0
            {
                let throttle = trim(str_s(cast::<Ext, _>(g.player_growth_throttle[cp])).as_bytes());
                extra_note = s(__content.string(3491))
                    + &throttle
                    + __content.string(3492)
                    + &g.ent_symbol[ae]
                    + " "
                    + __content.string(3493)
                    + &throttle
                    + __content.string(3494)
                    + __content.string(3495)
                    + &g.ent_symbol[ae]
                    + __content.string(3496)
                    + "\r"
                    + "\r"
                    + __content.string(3497)
                    + &g.active_entity_name
                    + __content.string(3498)
                    + "to "
                    + &trim(str_s(cast::<Ext, _>(new_int)).as_bytes())
                    + __content.string(3499);
                if ui.msgbox(&extra_note, 8227, &s(__content.string(3500))) != 6 {
                    return;
                }
                extra_note = s(__content.string(3501)) + &g.active_entity_name + "?";
                if ui.msgbox(&extra_note, 8228, &s(__content.string(3502))) == 6 {
                    g.auto_pilot[ae] = 0;
                    // (UI) CONTROL SET TEXT GetDlgItem(hDlgTransMenu, 470), "&Turn AutoPilot ON"
                    extra_note = s("\r\n")
                        + "\r\n"
                        + __content.string(3503)
                        + &g.active_entity_name
                        + ")";
                } else {
                    extra_note = s(__content.string(3504));
                }
            }
            if val(min_rate_str.as_bytes()) - Ext::from_f64(1.0) < cast::<Ext, _>(new_int) && (cast::<Ext, _>(new_int)) < val(max_rate_str.as_bytes()) + Ext::from_f64(1.0) {
                done = true;
                g.growth_rate[ae] = cast::<i16, _>(new_int);
            } else {
                msg_text = s(__content.string(3505)) + &min_rate_str + " and " + &max_rate_str + "%";
            }
        } else if pc == "RD" {
            if new_int < 0 || 30 < new_int {
                msg_text = s(__content.string(3506));
            } else {
                done = true;
                g.disc_spend_pct[ae] = cast::<i16, _>(new_int);
            }
        } else if pc == "DV" {
            if new_value < Ext::from_f64(0.0) || max_dividend < new_value {
                msg_text = s(__content.string(3507)) + &max_dividend_str;
            } else {
                done = true;
                g.dividend_per_share[ae] = new_value;
            }
        }
        if done {
            break;
        }
    }
    g.turn_action_count += 1;
    msg_text = s(__content.string(3508));
    if pc == "GR" {
        msg_text = s(__content.string(3509));
        if g.industry_num[ae] == 1 {
            msg_text = msg_text + __content.string(3510);
        } else if g.industry_num[ae] == 2 {
            msg_text = msg_text + __content.string(3469);
        } else {
            msg_text = msg_text + __content.string(3511);
        }
    }
    msg_text = msg_text + __content.string(3512) + &trim(str_s(cast::<Ext, _>(new_int)).as_bytes()) + "%." + &extra_note;
    if pc == "DV" {
        if val(fmt(new_value, b"* ######.00").as_bytes()) <= Ext::from_f64(0.0) {
            msg_text = g.ent_symbol[ae].clone() + __content.string(3513);
        } else {
            msg_text = g.ent_symbol[ae].clone()
                + __content.string(3514)
                + &trim(g.currency_symbol.as_bytes())
                + &fmt_trim(new_value, b"* ###,.00")
                + __content.string(2818);
        }
        if Ext::from_f64(0.0) < old_dividend {
            if new_value <= old_dividend * sf(1.2f32) {
                if new_value < old_dividend * sf(0.8f32) {
                    msg_text = replace(msg_text.as_bytes(), b"sets", b"cuts");
                    msg_text = replace(msg_text.as_bytes(), b" at ", b" to ");
                }
            } else {
                msg_text = replace(msg_text.as_bytes(), b"sets", b"raises");
                msg_text = replace(msg_text.as_bytes(), b" at ", b" to ");
            }
        }
    } else if pc == "RD" {
        answer = s("R&D");
        if get_industry_category(g.industry_num[ae]) == 2 {
            answer = s(__content.string(3515));
        }
        let change = new_int - cast::<i32, _>(old_value);
        if 3 < change.abs() {
            g.news_msg = g.ent_symbol[ae].clone()
                + __content.string(3516)
                + &answer
                + " to"
                + &str_s(cast::<Ext, _>(new_int))
                + __content.string(3517);
            if ae < 86 {
                g.news_msg = replace(g.news_msg.as_bytes(), b"sales", __content.bytes(3518));
            }
            if 8 < change.abs() {
                g.news_msg = replace(g.news_msg.as_bytes(), b"boosts", __content.bytes(3519));
            }
        }
        if 3 < -change {
            if new_int == 0 {
                g.news_msg = PbStr::new();
            } else {
                g.news_msg = replace(g.news_msg.as_bytes(), b"boosts", b"cuts");
                if 8 < -change {
                    g.news_msg = replace(g.news_msg.as_bytes(), __content.bytes(3519), b"slashes");
                }
            }
        }
    } else if pc == "GR" {
        if (cast::<i32, _>(old_value)) < new_int {
            if 0 < new_int {
                g.news_msg = g.ent_symbol[ae].clone() + __content.string(3520);
                if g.industry_num[ae] == 1 {
                    g.news_msg = g.ent_symbol[ae].clone()
                        + __content.string(3521);
                }
                if g.industry_num[ae] == 2 {
                    g.news_msg = g.ent_symbol[ae].clone()
                        + __content.string(3522);
                }
            }
        } else {
            g.news_msg = g.ent_symbol[ae].clone() + __content.string(3523);
            if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                g.news_msg = replace(g.news_msg.as_bytes(), __content.bytes(3524), __content.bytes(3525));
            }
            if g.industry_num[ae] == 1 {
                g.news_msg =
                    g.ent_symbol[ae].clone() + __content.string(3526);
            }
            if g.industry_num[ae] == 2 {
                g.news_msg =
                    g.ent_symbol[ae].clone() + __content.string(3527);
            }
        }
    }
    post_news(g, ui, 0, 0);
    if pc == "GR" && g.industry_num[ae] == 1 {
        msg_text = msg_text
            + "\r\n"
            + "\r\n"
            + __content.string(3528)
            + __content.string(3529)
            + __content.string(3530);
    }
    show_quick_message(g, ui, &mut msg_text);
    let mut cash_shortfall = 0;
    if 85 < ae {
        cash_shortfall = to_long(show_cash_flow_projection(g, ui, rng, ae, 1));
    }
    g.tmp_ext1 = Ext::from_f64(0.0);
    let mut is_increase = PbStr::new();
    if pc == "GR" && old_value < g.growth_rate[ae] {
        is_increase = s("Y");
    }
    if pc == "RD" && old_value < g.disc_spend_pct[ae] {
        is_increase = s("Y");
    }
    if dividend_raised == "Y" {
        is_increase = s("Y");
    }
    if 6 < cash_shortfall && is_increase == "Y" {
        let mut warn_text = s(__content.string(3531))
            + __content.string(3532)
            + &g.active_entity_name
            + __content.string(3533)
            + __content.string(3534)
            + __content.string(3535);
        if g.bonds_issued[ae] == 0 {
            warn_text = warn_text + __content.string(3536);
        }
        warn_text = warn_text
            + __content.string(3537)
            + "\r"
            + "\r"
            + __content.string(3538);
        if ui.msgbox(&warn_text, 8244, &s(__content.string(3539))) == 6 {
            show_cash_flow_projection(g, ui, rng, ae, 0);
        }
    }
    if pc == "DV" {
        g.news_msg = msg_text.clone();
        post_news(g, ui, 0, 0);
        dlg_title = s("000") + &trim(str_s(cast::<Ext, _>(ae)).as_bytes());
        dlg_title = right(dlg_title.as_bytes(), 4);
        find_option_positions(g, ui, 0, ae, 0, 0);
        if g.has_options_outstanding[ae] != 0 {
            let cp = g.cur_player;
            let mut pos = find_option_positions(g, ui, cp, ae, 1, 0);
            if 0 < pos && dividend_raised == "Y" {
                return;
            }
            if pos < 0 && dividend_raised == "N" {
                return;
            }
            pos = find_option_positions(g, ui, cp, ae, 2, 0);
            if 0 < pos && dividend_raised == "N" {
                return;
            }
            if pos < 0 && dividend_raised == "Y" {
                return;
            }
            if len(g.recent_dividend_change_list.as_bytes()) < 5 {
                g.recent_dividend_change_list = dlg_title.clone() + "|" + &dlg_title + "|" + &dlg_title;
            } else {
                let n = parse_count_any(g.recent_dividend_change_list.as_bytes(), b"|");
                for i in 1..=n {
                    msg_text = parse_any(g.recent_dividend_change_list.as_bytes(), b"|", i);
                    if val(msg_text.as_bytes()) == cast::<Ext, _>(ae) && 9 < n {
                        return;
                    }
                }
                if 9 < n {
                    let l = len(g.recent_dividend_change_list.as_bytes());
                    g.recent_dividend_change_list = right(g.recent_dividend_change_list.as_bytes(), l - 5);
                }
                g.recent_dividend_change_list = g.recent_dividend_change_list.clone() + "|" + &dlg_title;
            }
        }
    }
}

/// Port of ToggleAutoPilot (0x5b65b9).
/// Management menu: turns `AutoPilot` of the active company on or off (with an explanation box), except
/// for the company of which the player is CEO (`PlayerCeoOf`): there it can only be turned off. Updates
/// the menu button text ([`set_auto_pilot_menu_text`]).
pub fn toggle_auto_pilot(g: &mut Globals, ui: &mut dyn Ui) {
    let __content = crate::content::current();
    let entity_id = g.active_entity;
    if !(10 < g.active_entity && g.entity_status[g.active_entity] == 1) {
        return;
    }
    let button_state;
    if g.player_ceo_of[g.cur_player] == entity_id {
        if g.auto_pilot[entity_id] == 0 {
            let mut m = s(__content.string(3540))
                + &g.active_entity_name
                + "."
                + "\r\n"
                + "\r\n"
                + __content.string(3541)
                + __content.string(3542)
                + __content.string(3543);
            show_quick_message(g, ui, &mut m);
        } else {
            let mut m = s(__content.string(3544))
                + &g.active_entity_name
                + " ("
                + &g.ent_symbol[g.active_entity]
                + ").";
            show_quick_message(g, ui, &mut m);
            g.auto_pilot[entity_id] = 0;
        }
        button_state = s("ON");
    } else {
        let mut msg_text = s(__content.string(3545));
        if g.auto_pilot[entity_id] == 0 {
            msg_text = msg_text
                + __content.string(3546)
                + &g.active_entity_name
                + " ("
                + &g.ent_symbol[g.active_entity]
                + ")."
                + "\r"
                + "\r"
                + __content.string(3547)
                + __content.string(3548);
            g.auto_pilot[entity_id] = 1;
        } else {
            msg_text = msg_text
                + __content.string(3549)
                + &g.active_entity_name
                + " ("
                + &g.ent_symbol[g.active_entity]
                + ")."
                + "\r"
                + "\r"
                + __content.string(3550)
                + __content.string(3551)
                + __content.string(3552);
            g.auto_pilot[entity_id] = 0;
        }
        let mut new_state = s("ON");
        let mut bs = s("OFF");
        if g.auto_pilot[entity_id] == 0 {
            new_state = s("OFF");
            bs = s("ON");
        }
        let mut note_text = s(__content.string(3553))
            + &new_state
            + " until "
            + __content.string(3554)
            + &bs
            + __content.string(3555)
            + __content.string(3556)
            + __content.string(3557)
            + &bs
            + __content.string(3558)
            + __content.string(3559);
        if g.auto_pilot[entity_id] == 0 {
            note_text = note_text
                + __content.string(3560)
                + __content.string(3561);
        }
        msg_text = msg_text + "\r" + "\r" + &note_text;
        let title = s(__content.string(3562)) + &g.active_entity_name;
        ui.msgbox(&msg_text, 8192, &title);
        g.turn_action_count += 1;
        button_state = bs;
    }
    set_auto_pilot_menu_text(g, entity_id, &button_state);
}

/// Port of SetAutoPilotMenuText (0x5b6b90).
/// Sets the caption "&Turn AutoPilot " + `state` of the AutoPilot button of the transaction menu
/// (control 469 for banks and insurers, 466 for industry 70, else 470). Pure window plumbing: no game
/// state, so a no-op in the port.
pub fn set_auto_pilot_menu_text(_g: &mut Globals, _entity_id: i32, _state: &PbStr) {}
