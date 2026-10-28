//! Advances to controlled companies and their recall, calling in bank loans, freezing credit lines
//! (this file); corporate policy (growth rate, R&D spending, dividends) and AutoPilot in [`policy`].
//!
//! Pick lists: the original lets the player pick a borrower in a report (`ShowBankLoanCustomers` /
//! `ShowMyAdvances` with `EntityPickMode = "GetCorp"`); a double click on a line runs `FollowReportLink`
//! (0x46c9f4, called by the report dialog), which sets `TargetCorp`.
use crate::pb::real::{Ext, cast};


mod policy;

pub use policy::{change_corp_policy, set_auto_pilot_menu_text, toggle_auto_pilot};

#[allow(unused_imports)]
use super::prelude::*;
use crate::logic::control::{find_option_positions, update_corp_control};
use crate::logic::etf_derivs::etf_mgmt::cover_negative_cash;
use crate::logic::finance::calc_entity_financials;
use crate::logic::holdings::find_holding_slot;
use crate::logic::report::show_quick_message;
use crate::logic::reports_lists::{show_bank_loan_customers, show_my_advances};
use crate::logic::sim_entity::quarter_start::credit_rating_after_repayment;
use crate::logic::valuation::calc_working_capital_ratio;
use crate::pb::string::{remove, remove_any, ucase, CaseMode};

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

/// `CHR$(34)`.
fn q() -> PbStr {
    s("\"")
}

/// Port of DemandAdvanceRepayment (0x5be3f5).
/// Player menu "recall advanced funds": the player (`ActiveEntity = CurPlayer`) picks a company with
/// outstanding `PlayerAdvances` (pick list `ShowMyAdvances` when there are several) and enters the
/// amount to be repaid. A borrower rated below "BB" (6) may refuse (loan covenants / bond indenture /
/// bankruptcy risk / bank or insurance regulators); then the player may forgive 1..100% of the advance
/// (blocked by a conflicting long-call / short-put position of the player or a company he controls).
/// Forgiveness raises the stock basis of a 100%-owned borrower, else it is a capital loss of the player
/// and an extraordinary item (and a reduction [runtime resource wording] ) of the borrower.
pub fn demand_advance_repayment(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    if g.cur_player != g.active_entity {
        return;
    }
    let ae = g.active_entity;
    let mut count = 0;
    let mut borrower_id = 0;
    for i in 11..=1600 {
        if g.entity_status[i] == 1 && Ext::from_f64(0.0) < g.player_advances[(ae, i)] {
            count += 1;
            borrower_id = i;
        }
    }
    let mut msg_text: PbStr;
    if count == 0 {
        msg_text = s(__content.string(3331)) + __content.string(3332);
        let title = g.app_title.clone();
        ui.msgbox(&msg_text, 8208, &title);
        return;
    }
    if 1 < count {
        g.target_corp = 0;
        g.entity_pick_mode = s("GetCorp");
        let player = g.active_entity;
        show_my_advances(g, ui, rng, player);
        borrower_id = g.target_corp;
    }
    if borrower_id <= 10 {
        return;
    }
    let b = borrower_id;
    let mut covenant_type = PbStr::new();
    let mut has_conflict = 0;
    let mut option_holder = 0;
    let mut is_wholly_owned = 0;
    loop {
        let advanced = g.player_advances[(ae, b)];
        msg_text = s(__content.string(3333))
            + &g.ent_name[b]
            + __content.string(3334)
            + "to it ("
            + &fmt_trim(advanced, b"* ###,.00")
            + " "
            + &g.million_prefix_uc
            + ".)";
        let title = s(__content.string(3335)) + &g.ent_symbol[b];
        let answer = ui.inputbox(&msg_text, &title, &fmt_trim(advanced, b"* ###,.00"));
        let repay_amt = val(remove_any(answer.as_bytes(), b"$ ,%").as_bytes());
        if repay_amt <= Ext::from_f64(0.0) {
            break;
        }
        if !(repay_amt <= advanced + sf(0.005f32)) {
            continue;
        }
        let rating_after = credit_rating_after_repayment(g, ui, rng, ae, b, repay_amt);
        let mut block_reason: i16 = 0;
        if g.credit_rating_num[b] < 6 {
            if Ext::from_f64(0.0) < g.bank_debt[b] && 85 < b {
                block_reason = 1;
                covenant_type = s(__content.string(3336));
            }
            if 0 < g.bonds_issued[b] && block_reason == 0 {
                block_reason = 2;
                covenant_type = s(__content.string(3337));
            }
            if block_reason == 0 && 85 < b && g.total_assets * sf(0.2f32) < repay_amt {
                if (g.cash[b] + g.t_bills[b]) * sf(1.2f32) < repay_amt {
                    block_reason = 3;
                }
            }
            if block_reason == 0 && g.industry_num[b] == 1 {
                block_reason = 4;
            }
            if block_reason == 0 && g.industry_num[b] == 2 {
                block_reason = 5;
            }
        }
        if block_reason < 1 {
            g.cash[ae] = g.cash[ae] + repay_amt;
            g.cash[b] = g.cash[b] - repay_amt;
            g.player_advances[(ae, b)] = g.player_advances[(ae, b)] - repay_amt;
            if g.player_advances[(ae, b)] < sf(0.01f32) {
                g.cash[ae] = g.cash[ae] + g.player_advances[(ae, b)];
                g.cash[b] = g.cash[b] - g.player_advances[(ae, b)];
                g.player_advances[(ae, b)] = Ext::from_f64(0.0);
            }
            msg_text = s(__content.string(3338));
            calc_entity_financials(g, ui, rng, b);
            if g.cash[b] < Ext::from_f64(0.0) && g.unused_credit_line.abs() < g.cash[b].abs() {
                msg_text = msg_text + " " + &g.ent_name[b] + __content.string(3339);
            }
            show_quick_message(g, ui, &mut msg_text);
            if g.cash[b] < Ext::from_f64(0.0) {
                cover_negative_cash(g, ui, rng, b, 1);
            }
            msg_text = s(__content.string(3340));
            if g.player_advances[(ae, b)] == Ext::from_f64(0.0) {
                msg_text = PbStr::new();
            }
            g.news_msg = g.active_entity_name.clone()
                + " calls"
                + &msg_text
                + __content.string(3341)
                + &g.ent_symbol[b];
            post_news(g, ui, 0, 0);
            break;
        }
        let mut note_text = s("(") + &q() + &rating_after + &q() + ")";
        match block_reason {
            1 | 2 => {
                msg_text = g.ent_symbol[b].clone()
                    + __content.string(3342)
                    + __content.string(3343)
                    + &covenant_type
                    + __content.string(3344)
                    + &q()
                    + "BB"
                    + &q()
                    + " "
                    + &note_text
                    + __content.string(3345)
                    + "\r"
                    + "\r";
            }
            3 => {
                msg_text = g.ent_symbol[b].clone()
                    + __content.string(3346)
                    + __content.string(3347)
                    + __content.string(3348)
                    + __content.string(3349)
                    + "\r"
                    + "\r";
            }
            _ => {
                // CASE 4, 5
                note_text = s("banking");
                if block_reason == 5 {
                    note_text = s(__content.string(2822));
                }
                msg_text = s("Due to ")
                    + &g.ent_symbol[b]
                    + __content.string(3350)
                    + &note_text
                    + __content.string(3351)
                    + __content.string(3352)
                    + __content.string(3353);
                if block_reason == 4 {
                    msg_text = msg_text + __content.string(3354);
                } else {
                    msg_text = msg_text + __content.string(3355);
                }
                msg_text = msg_text + &g.ent_symbol[b] + "." + "\r" + "\r";
            }
        }
        let forgive_prompt = s(__content.string(3356))
            + __content.string(3357)
            + &g.ent_name[b]
            + "?";
        msg_text = msg_text + &forgive_prompt;
        if ui.msgbox(&msg_text, 8483, &s(__content.string(3358))) != 6 {
            break;
        }
        let cp = g.cur_player;
        let mut option_pos = find_option_positions(g, ui, cp, b, 1, 0);
        if 0 < option_pos {
            has_conflict = 1;
        }
        if has_conflict == 0 {
            option_pos = find_option_positions(g, ui, cp, b, 2, 0);
            if option_pos == -31 || option_pos == 32 {
                has_conflict = -1;
            }
        }
        if has_conflict == 0 {
            for i in 11..=1600 {
                if g.entity_status[i] == 1 && b != i && g.cur_player == g.corp_controller[i] {
                    option_pos = find_option_positions(g, ui, i, b, 1, 0);
                    if 0 < option_pos {
                        has_conflict = 1;
                        option_holder = i;
                        break;
                    }
                    option_pos = find_option_positions(g, ui, i, b, 2, 0);
                    if option_pos == -31 || option_pos == 32 {
                        has_conflict = -1;
                        option_holder = i;
                        break;
                    }
                }
            }
        }
        if has_conflict != 0 {
            let mut option_type = s(__content.string(3359));
            if has_conflict == -1 {
                option_type = s(__content.string(3360));
            }
            msg_text = s(__content.string(3361))
                + __content.string(3362)
                + &option_type
                + __content.string(3363)
                + &g.ent_name[b]
                + __content.string(3364)
                + __content.string(3365)
                + "\r"
                + "\r"
                + __content.string(3366)
                + &option_type
                + __content.string(3367)
                + __content.string(3368)
                + &g.ent_name[b]
                + ".";
            if 10 < option_holder {
                note_text = s(__content.string(3369))
                    + &g.ent_symbol[b]
                    + __content.string(3370)
                    + &g.ent_name[option_holder]
                    + ".)";
                msg_text = msg_text + "\r" + "\r" + &note_text;
            }
            ui.msgbox(&msg_text, 8208, &s(__content.string(3253)));
            break;
        }
        let forgive_pct;
        loop {
            msg_text = s(__content.string(3371))
                + &g.ent_symbol[b]
                + __content.string(3372);
            let title = s(__content.string(3373)) + &g.ent_symbol[b];
            let answer = ui.inputbox(&msg_text, &title, &s("10%"));
            let pct = to_long(val(remove_any(answer.as_bytes(), b"$ ,%").as_bytes()));
            if pct < 1 {
                return;
            }
            if !(100 < pct) {
                forgive_pct = pct;
                break;
            }
        }
        let forgive_amt = sf(0.01f32) * cast::<Ext, _>(forgive_pct) * advanced;
        g.player_advances[(ae, b)] = advanced - forgive_amt;
        if forgive_pct == 100 {
            g.player_advances[(ae, b)] = Ext::from_f64(0.0);
        }
        msg_text = trim(str_s(cast::<Ext, _>(forgive_pct)).as_bytes())
            + __content.string(3374)
            + &g.active_entity_name
            + " to "
            + &g.ent_name[b]
            + " "
            + __content.string(3375)
            + &g.active_entity_name
            + "."
            + "\r"
            + "\r";
        let slot = find_holding_slot(g, ae, b);
        if 0 < slot && g.hold_stock_pct[(slot, ae)] == 100 {
            is_wholly_owned = 1;
        }
        if is_wholly_owned == 1 {
            msg_text = msg_text
                + __content.string(3376)
                + &g.active_entity_name
                + __content.string(3377)
                + &g.ent_symbol[b]
                + __content.string(3378)
                + __content.string(3379)
                + &g.active_entity_name
                + ".)";
            g.hold_stock_basis[(slot, ae)] = g.hold_stock_basis[(slot, ae)] + forgive_amt;
        } else {
            g.extra_items_ytd[b] = g.extra_items_ytd[b] + forgive_amt;
            g.player_cap_gains[ae] = g.player_cap_gains[ae] - forgive_amt;
            msg_text = msg_text
                + __content.string(3380)
                + &g.active_entity_name
                + " "
                + __content.string(3381);
            if g.tax_loss_carry[b] < Ext::from_f64(0.0) {
                msg_text = msg_text
                    + __content.string(3382)
                    + &g.ent_symbol[b]
                    + ", but "
                    + __content.string(3383)
                    + &g.ent_symbol[b]
                    + ".";
            }
            msg_text = msg_text + ")";
        }
        ui.msgbox(&msg_text, 8256, &s(__content.string(3384)));
        if g.tax_loss_carry[b] < Ext::from_f64(0.0) && is_wholly_owned == 0 {
            g.tax_loss_carry[b] = g.tax_loss_carry[b] + forgive_amt;
            if Ext::from_f64(0.0) < g.tax_loss_carry[b] {
                g.tax_loss_carry[b] = Ext::from_f64(0.0);
            }
        }
        if forgive_pct == 100 {
            break;
        }
    }
}

/// Port of AdvanceFundsToCorp (0x5bdaaf).
/// Player menu "advance funds": the player (`ActiveEntity = CurPlayer`) lends cash (subordinated) to a
/// company he controls (`PlayerAdvances`). Refreshes the control of every company he controls first
/// (`UpdateCorpControl`); with several of them the company is asked through the symbol-entry dialog
/// `InBox(3)`. Warns about a poor credit rating (YES/NO) or futures positions, asks the amount (up to
/// `LiquidFunds`) and, for an amount over cash + T-bills, `BorrowToCoverPurchase` (NO cancels).
pub fn advance_funds_to_corp(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    if g.cur_player != g.active_entity {
        return;
    }
    let ae = g.active_entity;
    calc_entity_financials(g, ui, rng, ae);
    let mut msg_text = PbStr::new();
    if g.liquid_funds <= Ext::from_f64(0.0) {
        msg_text = s(__content.string(3385));
    }
    let mut ctrl_count = 0;
    let mut corp_id = 0;
    for i in 11..=1600 {
        if g.entity_status[i] == 1 {
            if g.active_entity == g.corp_controller[i] {
                update_corp_control(g, ui, rng, i);
            }
            if g.active_entity == g.corp_controller[i] {
                ctrl_count += 1;
                corp_id = i;
            }
        }
    }
    if ctrl_count == 0 {
        msg_text = s(__content.string(3386))
            + __content.string(3387);
    }
    if !msg_text.is_empty() {
        show_quick_message(g, ui, &mut msg_text);
        return;
    }
    if 1 < ctrl_count {
        g.tmp_symbol = PbStr::new();
        g.input_dlg_label = PbStr::new();
        g.target_corp = 0;
        g.input_dlg_prompt = s(__content.string(3388))
            + __content.string(3389)
            + __content.string(3390)
            + __content.string(3391)
            + __content.string(3392)
            + __content.string(3393)
            + "\r\n"
            + "\r\n";
        g.lookup_filter = s("CTRL");
        g.action_code = s("ADVANCE");
        g.input_dlg_title = s(__content.string(3394));
        crate::logic::trading::select::in_box_symbol(g, ui, rng);
        g.lookup_filter = PbStr::new();
        g.action_code = PbStr::new();
        corp_id = g.target_corp;
    }
    if corp_id <= 10 {
        return;
    }
    let c = corp_id;
    if g.cur_player != g.corp_controller[c] {
        let mut m = s(__content.string(3395)) + &g.ent_name[c] + ".";
        show_quick_message(g, ui, &mut m);
        return;
    }
    calc_entity_financials(g, ui, rng, c);
    if g.credit_rating_num[c] < 6 {
        msg_text = g.ent_symbol[c].clone()
            + __content.string(3396)
            + __content.string(3397)
            + "\r"
            + "\r"
            + __content.string(3398)
            + "only a "
            + &g.credit_rating_str
            + __content.string(3399);
        if ui.msgbox(&msg_text, 8483, &s(__content.string(3400))) != 6 {
            return;
        }
    } else if Ext::from_f64(0.0) < g.futures_net_value {
        let article_text = if g.credit_rating_num[c] < 8 { s(" ") } else { s("n ") };
        msg_text = s(__content.string(3401))
            + &g.ent_name[c]
            + __content.string(3402)
            + __content.string(3403)
            + &q()
            + "BB"
            + &q()
            + __content.string(3404)
            + __content.string(3405)
            + &g.ent_name[c]
            + " "
            + __content.string(3406)
            + &article_text
            + &q()
            + &g.credit_rating_str
            + &q()
            + __content.string(3407);
        ui.msgbox(&msg_text, 8256, &s(__content.string(3408)));
    }
    calc_entity_financials(g, ui, rng, ae);
    msg_text = s(__content.string(3409))
        + &g.ent_name[c]
        + __content.string(3410)
        + &fmt_trim(g.cash[ae], b"* ###,.00")
        + " "
        + &g.million_prefix_lc
        + __content.string(3411);
    let amount;
    loop {
        let title = s(__content.string(3412)) + &g.ent_symbol[c];
        let answer = ui.inputbox(&msg_text, &title, &fmt_trim(g.liquid_funds, b"* ###,.00"));
        let a = val(remove_any(answer.as_bytes(), b"$ ,%").as_bytes());
        if a <= Ext::from_f64(0.0) {
            return;
        }
        if !(g.liquid_funds + sf(0.01f32) < a) {
            amount = a;
            break;
        }
    }
    let mut shortfall = amount - (g.cash[ae] + g.t_bills[ae]);
    if Ext::from_f64(0.0) < shortfall && crate::logic::trading::borrow_to_cover_purchase(g, ui, &mut shortfall, Ext::from_f64(0.0), 0) == 7 {
        return;
    }
    g.cash[ae] = g.cash[ae] - amount;
    g.cash[c] = g.cash[c] + amount;
    g.player_advances[(ae, c)] = g.player_advances[(ae, c)] + amount;
    let mut m = s(__content.string(3413)) + &g.ent_name[c] + "...";
    show_quick_message(g, ui, &mut m);
}

/// Port of CallInLoan (0x595994).
/// Bank menu "call in a loan": the active bank picks a business borrower (`ShowBankLoanCustomers(2)`)
/// and calls 1..50% of its loan unless the borrower is bankrupt, rated "BBB" (7) or better, overdrawn
/// (after `CoverNegativeCash`) or not a customer. If the borrower is controlled by an opponent, offers to
/// call the same percentage of the loans of every bank the player controls to the opponent's companies
/// (positive net worth, rated below "BBB", not overdrawn).
///
/// Quirk (BUGS.md #312): the "instead" offer for a borrower that owes nothing but is controlled by an
/// opponent can never run (the percentage is still 0).
pub fn call_in_loan(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    g.entity_pick_mode = s("GetCorp");
    g.target_corp = 0;
    g.lookup_entity_name = PbStr::new();
    show_bank_loan_customers(g, ui, rng, 2);
    if g.target_corp == 0 {
        return;
    }
    let mut debtor_id = g.target_corp;
    let mut err_text = PbStr::new();
    calc_entity_financials(g, ui, rng, debtor_id);
    let d = debtor_id;
    if !(g.num_players < d) && g.entity_status[d] != 1 {
        err_text = g.ent_name[d].clone() + __content.string(3414);
    } else if g.cur_player == d || g.active_entity == d {
        err_text = s(__content.string(3415));
    } else if cast::<i32, _>(g.lending_bank[d]) == g.active_entity {
        if g.credit_rating_num[d] < 2 && 10 < d {
            err_text = s(__content.string(3416)) + &g.ent_name[d] + __content.string(3417);
        } else if g.credit_rating_num[d] < 7 || d < 11 {
            if g.cash[d] < Ext::from_f64(0.0) && 10 < d {
                cover_negative_cash(g, ui, rng, d, 0);
                if g.cash[d] < Ext::from_f64(0.0) {
                    err_text = g.ent_name[d].clone()
                        + __content.string(3418)
                        + __content.string(3419);
                    let mut reason_text = s(__content.string(3420));
                    if 130 < d && g.industry_num[d] < 70 {
                        let ratio = calc_working_capital_ratio(g, d);
                        if g.working_capital[d] < g.business_assets[d] * ratio * sf(0.8f32) {
                            reason_text =
                                s(__content.string(3421));
                        }
                    }
                    err_text = err_text
                        + &reason_text
                        + __content.string(3422);
                }
            }
        } else {
            err_text = s(__content.string(3423))
                + &g.ent_name[d]
                + __content.string(3424)
                + __content.string(3425);
        }
    } else {
        err_text = g.ent_name[d].clone() + __content.string(3426) + &g.active_entity_name + ".";
    }

    let mut call_pct = 0;
    let mut call_frac: f32 = cast::<f32, _>(Ext::from_f32(0.0f32));
    let mut skip_corp_id = 0;
    let mut msg_text: PbStr;
    if err_text.is_empty() && !(cast::<i32, _>(g.lending_bank[d]) == g.active_entity && g.bank_debt[d] == Ext::from_f64(0.0)) {
        msg_text = s(__content.string(3427)) + &g.ent_name[d] + __content.string(3428);
        loop {
            let title = g.ent_symbol[g.active_entity].clone() + __content.string(3429);
            let input_text = ui.inputbox(&msg_text, &title, &s("50%"));
            call_pct = to_long(val(remove_any(input_text.as_bytes(), b"$ ,").as_bytes()));
            if call_pct < 1 {
                return;
            }
            if !(50 < call_pct) {
                break;
            }
        }
        call_frac = cast::<f32, _>(cast::<Ext, _>(call_pct) * sf(0.01f32));
        let amt = g.bank_debt[d] * cast::<Ext, _>(call_frac);
        g.bank_debt[d] = g.bank_debt[d] - amt;
        g.cash[d] = g.cash[d] - amt;
        let ae = g.active_entity;
        g.cash[ae] = g.cash[ae] + amt;
        g.news_msg = g.ent_symbol[ae].clone()
            + __content.string(3430)
            + &str_s(cast::<Ext, _>(call_pct))
            + "% of "
            + &g.ent_symbol[d]
            + " loan";
        msg_text = s(__content.string(3431))
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(amt, b"##############")
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix;
        let mut popup = g.news_msg.clone() + &msg_text;
        show_quick_message(g, ui, &mut popup);
        if g.credit_frozen[d] == 0 && Ext::from_f64(0.0) < g.unused_credit_line {
            msg_text = s("...but ")
                + &g.ent_name[ae]
                + __content.string(3432)
                + __content.string(3433)
                + &g.ent_symbol[d]
                + __content.string(3434);
            show_quick_message(g, ui, &mut msg_text);
        }
        post_news(g, ui, 0, 0);
        cover_negative_cash(g, ui, rng, d, 1);
        g.turn_action_count += 1;
        if 10 < debtor_id {
            update_corp_control(g, ui, rng, debtor_id);
            let bank_id = g.corp_controller[debtor_id];
            if 0 < bank_id && bank_id <= g.num_players && g.cur_player != bank_id {
                skip_corp_id = debtor_id;
                debtor_id = bank_id;
            }
        }
        err_text = s("also");
    } else {
        msg_text = g.ent_name[d].clone()
            + __content.string(3435)
            + "to "
            + &g.active_entity_name
            + __content.string(3436);
        if !err_text.is_empty() {
            msg_text = err_text.clone();
        }
        show_quick_message(g, ui, &mut msg_text);
        if debtor_id <= g.num_players {
            return;
        }
        let bank_id = g.corp_controller[debtor_id];
        if bank_id != 0 && bank_id <= g.num_players && g.entity_status[bank_id] == 1 && g.cur_player != bank_id {
            err_text = s("instead");
            debtor_id = bank_id;
        } else {
            return;
        }
    }

    // LAB_00596418
    if debtor_id <= g.num_players && 0 < call_pct {
        msg_text = s(__content.string(3437))
            + &err_text
            + __content.string(3438)
            + &str_s(cast::<Ext, _>(call_pct))
            + __content.string(3439)
            + __content.string(3440)
            + &g.ent_name[debtor_id]
            + "?";
        let title = s("Call in") + &str_s(cast::<Ext, _>(call_pct)) + __content.string(3441);
        if ui.msgbox(&msg_text, 8228, &title) != 7 {
            let mut called_any = 0;
            for corp_idx in 86..=1600 {
                if skip_corp_id == corp_idx
                    || g.entity_status[corp_idx] != 1
                    || debtor_id != g.corp_controller[corp_idx]
                {
                    continue;
                }
                let bank_id = cast::<i32, _>(g.lending_bank[corp_idx]);
                if g.cur_player != g.corp_controller[bank_id] || !(Ext::from_f64(0.0) < g.bank_debt[corp_idx]) {
                    continue;
                }
                calc_entity_financials(g, ui, rng, corp_idx);
                if Ext::from_f64(0.0) < g.net_worth && g.credit_rating_num[corp_idx] < 7 {
                    cover_negative_cash(g, ui, rng, corp_idx, 0);
                    if Ext::from_f64(0.0) <= g.cash[corp_idx] {
                        let amt = g.bank_debt[corp_idx] * cast::<Ext, _>(call_frac);
                        g.bank_debt[corp_idx] = g.bank_debt[corp_idx] - amt;
                        g.cash[corp_idx] = g.cash[corp_idx] - amt;
                        g.cash[bank_id] = g.cash[bank_id] + amt;
                        called_any = 1;
                        g.news_msg = g.ent_symbol[bank_id].clone()
                            + __content.string(3430)
                            + &str_s(cast::<Ext, _>(call_pct))
                            + "% of "
                            + &g.ent_symbol[corp_idx]
                            + " loan";
                        post_news(g, ui, 0, 0);
                        cover_negative_cash(g, ui, rng, corp_idx, 1);
                    }
                }
            }
            if called_any == 0 {
                msg_text = s(__content.string(3442));
            } else {
                msg_text = s(__content.string(3443));
                if !(err_text == "also") {
                    g.turn_action_count += 1;
                }
            }
            show_quick_message(g, ui, &mut msg_text);
        }
    }
}

/// Port of FreezeCreditLine (0x59d28a).
/// Bank menu "freeze/unfreeze a credit line": repeatedly picks a borrower of the active bank
/// (`ShowBankLoanCustomers(1)`) and toggles its `CreditFrozen` after a YES/NO confirmation (the player
/// himself can only be unfrozen), while `TurnActionCount < 5` or in solo mode. Picking nothing runs
/// [`freeze_all_lending`] instead.
pub fn freeze_credit_line(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    if g.active_entity < 11 || 85 < g.active_entity {
        let mut m = s(__content.string(3444));
        show_quick_message(g, ui, &mut m);
        return;
    }
    loop {
        g.entity_pick_mode = s("GetCorp");
        g.target_corp = 0;
        g.lookup_entity_name = PbStr::new();
        show_bank_loan_customers(g, ui, rng, 1);
        let i = g.target_corp;
        if g.target_corp == 0 {
            freeze_all_lending(g, ui);
            break;
        }
        update_corp_control(g, ui, rng, i);
        if g.cur_player == i {
            let mut msg_text;
            if g.credit_frozen[i] == 0 {
                msg_text = s(__content.string(3445));
            } else {
                msg_text = s(__content.string(3446)) + &g.ent_name[i] + __content.string(3447);
                g.turn_action_count += 1;
            }
            g.credit_frozen[i] = 0;
            show_quick_message(g, ui, &mut msg_text);
            break;
        }
        let un_prefix = if g.credit_frozen[i] == 1 { s("UN") } else { PbStr::new() };
        let mut msg_text = s(__content.string(3448))
            + &g.active_entity_name
            + " to "
            + &un_prefix
            + __content.string(3449)
            + &g.ent_name[i]
            + "? ";
        if 10 < i && g.cur_player == g.corp_controller[i] && g.credit_frozen[i] == 0 {
            msg_text = msg_text + "\r" + "\r" + __content.string(3450);
        }
        let title = s(__content.string(3451)) + &g.active_entity_name;
        if ui.msgbox(&msg_text, 8484, &title) == 7 {
            break;
        }
        let mut m = s(__content.string(3452))
            + &ucase(g.ent_name[i].as_bytes(), CaseMode::Ansi)
            + __content.string(3453)
            + &un_prefix
            + "FROZEN.";
        show_quick_message(g, ui, &mut m);
        if g.credit_frozen[i] == 1 {
            g.credit_frozen[i] = 0;
        } else {
            g.credit_frozen[i] = 1;
        }
        g.turn_action_count += 1;
        if !(g.turn_action_count < 5 || g.solo_human_mode != 0) {
            break;
        }
    }
}

/// Port of FreezeAllLending (0x59d7d0).
/// Bank menu "freeze/unfreeze all loans": after a YES/NO question toggles `CreditFrozen(ActiveEntity)`
/// and sets the flag of every borrower of the bank (players 1..10 and companies 86..1600) to it; the
/// player's own credit line and those of his companies are only ever unfrozen. Posts the news when at
/// least two credit lines changed ("… freezes lending to N loan customers"); with none a popup.
///
/// Quirk: the bank range 11..85 is skipped by jumping to 86 from the first active bank, and with exactly
/// one change neither popup nor news appear, but `NewsMsg` is left set.
pub fn freeze_all_lending(g: &mut Globals, ui: &mut dyn Ui) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let ae = g.active_entity;
    let mut un_prefix = s("UN");
    let mut title_prefix = s("F");
    if g.credit_frozen[ae] == 1 {
        un_prefix = PbStr::new();
        title_prefix = s("Un-f");
    }
    let mut msg_text = s(__content.string(3454))
        + &g.active_entity_name
        + __content.string(3455)
        + __content.string(3456);
    if g.credit_frozen[ae] == 0 {
        msg_text = remove(msg_text.as_bytes(), b"un-");
        msg_text = msg_text
            + __content.string(3457)
            + __content.string(3458);
        if cast::<i32, _>(g.lending_bank[g.cur_player]) == ae {
            msg_text = msg_text + __content.string(3459) + &g.active_entity_name;
        }
        if !(right(msg_text.as_bytes(), 1) == ".") {
            msg_text = msg_text + ".";
        }
    } else {
        msg_text = msg_text
            + __content.string(3460)
            + __content.string(3461);
    }
    msg_text = msg_text + "\r" + "\r";
    let mut question_text = s(__content.string(3462));
    if g.credit_frozen[ae] == 0 {
        question_text = remove(question_text.as_bytes(), b"UN-");
    }
    msg_text = msg_text + &question_text;
    if ui.msgbox(&msg_text, 8484, &(title_prefix + __content.string(3463))) == 7 {
        return;
    }
    if g.credit_frozen[ae] == 1 {
        g.credit_frozen[ae] = 0;
        msg_text = s("un");
    } else {
        g.credit_frozen[ae] = 1;
        msg_text = PbStr::new();
    }
    let mut changed_count = 0;
    let mut ent_idx = 1;
    loop {
        if g.entity_status[ent_idx] == 1 {
            if 10 < ent_idx && ent_idx < 86 {
                ent_idx = 86;
            }
            if cast::<i32, _>(g.lending_bank[ent_idx]) == g.active_entity {
                let own = g.cur_player == ent_idx || g.cur_player == g.corp_controller[ent_idx];
                let mut sync = !own;
                if own && un_prefix.is_empty() && g.credit_frozen[ent_idx] == 1 {
                    g.credit_frozen[ent_idx] = 0;
                    changed_count += 1;
                    sync = true;
                }
                if sync && g.credit_frozen[g.active_entity] != g.credit_frozen[ent_idx] {
                    g.credit_frozen[ent_idx] = g.credit_frozen[g.active_entity];
                    changed_count += 1;
                }
            }
        }
        ent_idx += 1;
        if !(ent_idx < 1601) {
            break;
        }
    }
    g.news_msg = g.active_entity_name.clone()
        + " ("
        + &g.ent_symbol[g.active_entity]
        + ") "
        + &msg_text
        + __content.string(3464);
    if changed_count < 2 {
        if changed_count == 0 {
            let mut m = s(__content.string(3465)) + &msg_text + "frozen";
            show_quick_message(g, ui, &mut m);
            g.news_msg = PbStr::new();
        }
    } else {
        g.news_msg = replace(
            g.news_msg.as_bytes(),
            b"to ",
            (s("to") + &str_s(cast::<Ext, _>(changed_count)) + " ").as_bytes(),
        );
        let mut m = ucase(g.news_msg.as_bytes(), CaseMode::Ansi);
        show_quick_message(g, ui, &mut m);
        post_news(g, ui, 0, 0);
    }
    g.turn_action_count += 1;
}
