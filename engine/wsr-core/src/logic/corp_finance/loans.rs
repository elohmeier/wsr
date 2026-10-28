//! Bank loans of the acting entity: borrowing on and repaying the line of credit (with the player
//! advances it owes), the "sweep cash" setting, changing the lending bank and prepaying income tax (this
//! file); bank loan trading `BuySellLoans` and its type dialog in [`buy_sell`].
use crate::pb::real::{Ext, cast};


mod buy_sell;

pub use buy_sell::{buy_sell_loans, show_loan_trade_type_dialog};

#[allow(unused_imports)]
use super::prelude::*;
use crate::logic::control::update_corp_control;
use crate::logic::earnings::balance_cash_t_bills;
use crate::logic::economy::macro_update::sweep_player_cash_to_loan;
use crate::logic::finance::{calc_entity_financials, get_controller_type};
use crate::logic::market::update_market_display;
use crate::logic::newgame_helpers::{calc_loan_sale_price_pct, save_config, sell_loan_to_bank};
use crate::logic::report::show_quick_message;
use crate::logic::sim_entity::corp_ops::bank_allocate_assets;
use crate::logic::sim_entity::fin_inst::bank_system_liquidity_lender;
use crate::logic::sim_entity::quarter_start::credit_rating_after_repayment;
use crate::logic::trading::borrow::borrow_to_cover_purchase;
use crate::logic::trading::select::{get_selected_player_etf, in_box_symbol};
use crate::pb::string::{remove, remove_any};

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

/// `CHR$(34)`.
fn q() -> PbStr {
    s("\"")
}

/// Port of RepayBankLoan (0x46feda).
/// Loan repayment by the acting entity, or by an ETF advised by a company of the player
/// (`GetSelectedPlayerEtf`, YES/NO/CANCEL). With a selected company the player does not control, asks
/// whether the acting entity repays instead.
///
/// `mode` <> 2: repays the bank loan (`TmpBankLoan` of `CalcEntityFinancials`) from cash and T-bills,
/// asked by INPUTBOX (capped at the available funds and the loan). At difficulty 3 a company whose
/// lending bank the player does not control pays a 2% prepayment penalty (YES/NO; to the bank's
/// `AccruedPL`). A cash deficit left after `BalanceCashTBills` is borrowed back; the bank reallocates its
/// assets (`BankAllocateAssets` mode 3). Without a loan or funds: popup. `mode = 1` stops there.
///
/// `mode` <> 1: a company (`ActiveEntity` > 10) with liquid funds may repay each player's advances
/// (`PlayerAdvances`, INPUTBOX per player); refused (and the loop ended) while its credit rating stays
/// below "BB" if it is a bank/insurer or owes bank debt or bonds. A bank or company (> 85) short of cash
/// is asked to borrow (`BorrowToCoverPurchase`; NO ends the loop).
pub fn repay_bank_loan(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32, mode: i32) {
    let __content = crate::content::current();
    let mut payer_id = g.active_entity;
    let mut payer_name = g.active_entity_name.clone();
    let mut etf_id = get_selected_player_etf(g, ui, rng);
    let mut msg_text: PbStr;
    let mut amount_text: PbStr;
    if etf_id < 1581 {
        if payer_id != g.sel_entity && 10 < g.sel_entity {
            let sel = g.sel_entity;
            msg_text = s(__content.string(3312))
                + &g.ent_name[sel]
                + "...!"
                + "\r"
                + "\r"
                + __content.string(3313)
                + &payer_name
                + __content.string(3563);
            let title = g.ent_symbol[sel].clone() + __content.string(3564);
            if ui.msgbox(&msg_text, 8227, &title) != 6 {
                return;
            }
        }
    } else {
        amount_text = g.ent_name[etf_id].clone()
            + " ("
            + &g.ent_symbol[etf_id]
            + __content.string(3017)
            + __content.string(3018)
            + &g.ent_name[g.fund_adviser[etf_id]]
            + ". "
            + __content.string(3565)
            + "\r"
            + "\r"
            + __content.string(3020)
            + &g.ent_symbol[etf_id]
            + __content.string(3566)
            + "\r"
            + "\r"
            + __content.string(3567)
            + &g.active_entity_name
            + __content.string(3568);
        let reply = ui.msgbox(
            &amount_text,
            8483,
            &s(__content.string(3569)),
        );
        if reply == 2 {
            return;
        }
        if reply == 7 {
            etf_id = 0;
        } else {
            payer_id = etf_id;
            payer_name = g.sel_corp_name.clone();
        }
    }
    if mode != 2 {
        let p = payer_id;
        calc_entity_financials(g, ui, rng, p);
        if p == 11 {
            g.lender_bank_name = s("the ") + &g.central_bank_name;
        }
        if g.tmp_bank_loan.abs() < sf(0.001f32) {
            g.tmp_bank_loan = Ext::from_f64(0.0);
        }
        if g.tmp_bank_loan <= Ext::from_f64(0.0) || g.cash[p] + g.t_bills[p] <= Ext::from_f64(0.0) {
            amount_text = PbStr::new();
            if 10 < p && g.tmp_player_advances == Ext::from_f64(0.0) && etf_id == 0 {
                amount_text = s(__content.string(3570));
            }
            msg_text = s(__content.string(3571)) + &amount_text;
            if Ext::from_f64(0.0) < g.tmp_bank_loan {
                msg_text = s(
                    __content.string(3572),
                );
                if 10 < p && Ext::from_f64(0.0) < g.tmp_player_advances && g.liquid_funds <= Ext::from_f64(0.0) {
                    msg_text = msg_text + __content.string(3570);
                }
            }
            if g.liquid_funds <= Ext::from_f64(0.0)
                && 10 < p
                && sf(0.01f32) < g.tmp_player_advances
                && g.bank_debt[p] == Ext::from_f64(0.0)
            {
                msg_text = s(__content.string(3573));
            }
            amount_text = payer_name.clone() + &msg_text + __content.string(3436);
            show_quick_message(g, ui, &mut amount_text);
        } else {
            let mut penalty_flag: i64 = 0;
            let mut penalty = Ext::from_f64(0.0);
            if 10 < p
                && g.difficulty_level == 3
                && g.cur_player != g.corp_controller[cast::<i32, _>(g.lending_bank[p])]
            {
                penalty_flag = 1;
            }
            if 1580 < etf_id {
                penalty_flag = 0;
            }
            amount_text = fmt_trim(g.tmp_bank_loan, b"* ###,.00");
            let mut avail_funds = g.cash[p] + g.t_bills[p];
            if avail_funds < Ext::from_f64(0.0) {
                avail_funds = Ext::from_f64(0.0);
            }
            let mut default_text = fmt_trim(avail_funds, b"* ###,.00");
            if g.tmp_bank_loan < avail_funds {
                default_text = amount_text.clone();
            }
            msg_text = g.ent_symbol[p].clone()
                + " owes "
                + &g.lender_bank_name
                + " "
                + &amount_text
                + " "
                + &g.million_prefix_lc
                + __content.string(3574)
                + "\r"
                + "\r";
            let mut hint_text =
                s(__content.string(3575));
            if 10 < p {
                hint_text = replace(
                    hint_text.as_bytes(),
                    b"Your",
                    (g.ent_symbol[p].clone() + "'s").as_bytes(),
                );
                hint_text = replace(hint_text.as_bytes(), b"your", b"its");
            }
            msg_text = msg_text + &hint_text;
            default_text = remove(default_text.as_bytes(), b",");
            let mut repay_amt;
            loop {
                let title = g.app_title.clone();
                let input_text = ui.inputbox(&msg_text, &title, &default_text);
                repay_amt = val(remove_any(input_text.as_bytes(), b"$ ,").as_bytes());
                if avail_funds < repay_amt {
                    repay_amt = avail_funds;
                }
                if g.tmp_bank_loan < repay_amt {
                    repay_amt = g.tmp_bank_loan;
                }
                if !(repay_amt < Ext::from_f64(0.0)) {
                    break;
                }
            }
            if Ext::from_f64(0.0) < repay_amt {
                if penalty_flag != 0 {
                    msg_text = s(__content.string(3576))
                        + &payer_name
                        + __content.string(3577)
                        + "\r"
                        + "\r"
                        + __content.string(3578)
                        + &trim(g.currency_symbol.as_bytes())
                        + &fmt(repay_amt * sf(0.02f32), b"* ###,.00")
                        + " "
                        + &g.million_prefix_lc
                        + "illion."
                        + "\r"
                        + "\r"
                        + "DOES "
                        + &g.ent_symbol[p]
                        + __content.string(3579);
                    if ui.msgbox(&msg_text, 8243, &s(__content.string(3580))) != 6 {
                        return;
                    }
                    penalty = repay_amt * sf(0.02f32);
                    g.accrued_pl[p] = g.accrued_pl[p] - penalty;
                    if 10 < g.lender_id {
                        let l = g.lender_id;
                        g.accrued_pl[l] = g.accrued_pl[l] + penalty;
                    }
                }
                msg_text = s(__content.string(3581)) + &g.lender_bank_name + __content.string(3582);
                if penalty_flag == 0 && 11 < p && g.difficulty_level == 3 && etf_id < 1581 {
                    msg_text = msg_text
                        + "\r\n"
                        + "\r\n"
                        + __content.string(3583)
                        + &payer_name
                        + " and "
                        + &g.lender_bank_name
                        + ".";
                }
                show_quick_message(g, ui, &mut msg_text);
                g.bank_debt[p] = g.bank_debt[p] - repay_amt;
                if g.bank_debt[p] < sf(0.0052f32) {
                    g.bank_debt[p] = Ext::from_f64(0.0);
                }
                g.cash[p] = g.cash[p] - (repay_amt + penalty);
                if 10 < g.lender_id {
                    let l = g.lender_id;
                    g.cash[l] = g.cash[l] + (repay_amt + penalty);
                }
                if g.cash[p] < Ext::from_f64(0.0) && Ext::from_f64(0.0) < g.t_bills[p] {
                    balance_cash_t_bills(g, p);
                }
                if g.cash[p] < Ext::from_f64(0.0) {
                    g.bank_debt[p] = g.bank_debt[p] - g.cash[p];
                    if 10 < g.lender_id {
                        let l = g.lender_id;
                        g.cash[l] = g.cash[l] + g.cash[p];
                    }
                    g.cash[p] = Ext::from_f64(0.0);
                }
                if g.cash[p].abs() < sf(0.001f32) {
                    g.cash[p] = Ext::from_f64(0.0);
                }
                let l = g.lender_id;
                bank_allocate_assets(g, ui, rng, l, 3, 0);
                update_market_display(g, ui, rng, time_secs);
                if 1580 < etf_id && 0 < g.fund_bonus_year[p] {
                    g.fund_bonus_year[p] = g.cur_year;
                }
            }
        }
        if mode == 1 {
            return;
        }
    }
    let ae = g.active_entity;
    calc_entity_financials(g, ui, rng, ae);
    if !(10 < ae && Ext::from_f64(0.0) < g.tmp_player_advances && Ext::from_f64(0.0) < g.liquid_funds) {
        return;
    }
    let n = g.num_players;
    for player_id in 1..=n {
        if g.player_name[player_id] == "...OUT..." || !(Ext::from_f64(0.0) < g.player_advances[(player_id, ae)]) {
            continue;
        }
        let mut advance_amt = g.player_advances[(player_id, ae)];
        if g.liquid_funds < advance_amt + sf(0.01f32) {
            advance_amt = g.liquid_funds;
        }
        amount_text = fmt_trim(advance_amt, b"* ###,.00")
            + " "
            + &g.million_prefix_lc
            + "il."
            + &g.currency_suffix;
        msg_text = s(__content.string(3584))
            + &g.ent_symbol[ae]
            + __content.string(3585)
            + &g.ent_symbol[player_id]
            + ":"
            + "\r\n"
            + "\r\n"
            + &g.ent_symbol[ae]
            + __content.string(3586)
            + &amount_text
            + __content.string(3587);
        if g.cur_player == player_id {
            msg_text = replace(msg_text.as_bytes(), __content.bytes(3588), b" ");
        }
        let mut answer;
        loop {
            let mut hint_text = g.player_name[player_id].clone();
            if 11 < len(hint_text.as_bytes()) {
                hint_text = s("Player");
            }
            answer = ui.inputbox(
                &msg_text,
                &(s(__content.string(3589)) + &hint_text + "?"),
                &amount_text,
            );
            answer = remove_any(answer.as_bytes(), b"$ ,");
            if val(answer.as_bytes()) <= Ext::from_f64(0.0) {
                break;
            }
            if !(advance_amt + sf(0.01f32) < val(answer.as_bytes())) {
                break;
            }
        }
        if val(answer.as_bytes()) <= Ext::from_f64(0.0) {
            continue;
        }
        advance_amt = val(answer.as_bytes());
        if g.player_advances[(player_id, ae)] < advance_amt {
            advance_amt = g.player_advances[(player_id, ae)];
        }
        // defaultText = the rating after repayment (unused)
        credit_rating_after_repayment(g, ui, rng, player_id, ae, advance_amt);
        if g.credit_rating_num[ae] < 6
            && (g.industry_num[ae] < 3 || Ext::from_f64(0.0) < g.bank_debt[ae] || 0 < g.bonds_issued[ae])
        {
            let mut m = g.active_entity_name.clone()
                + __content.string(3590)
                + __content.string(3591);
            show_quick_message(g, ui, &mut m);
            break;
        }
        let mut avail_funds = advance_amt - (g.cash[ae] + g.t_bills[ae]);
        if 85 < ae
            && Ext::from_f64(0.0) < avail_funds
            && borrow_to_cover_purchase(g, ui, &mut avail_funds, Ext::from_f64(0.0), 0) == 7
        {
            break;
        }
        g.player_advances[(player_id, ae)] = g.player_advances[(player_id, ae)] - advance_amt;
        g.cash[ae] = g.cash[ae] - advance_amt;
        g.cash[player_id] = g.cash[player_id] + advance_amt;
        if g.player_advances[(player_id, ae)].abs() < sf(0.01f32) {
            g.player_advances[(player_id, ae)] = Ext::from_f64(0.0);
        }
        msg_text = s(__content.string(3592))
            + &g.ent_name[player_id]
            + __content.string(3431)
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(advance_amt, b"* ###,.00")
            + " "
            + &g.million_prefix_lc
            + "illion.";
        show_quick_message(g, ui, &mut msg_text);
    }
}

/// Port of BorrowFromBank (0x46ef51).
/// The acting entity (or an ETF advised by a company of the player, `GetSelectedPlayerEtf`, YES/NO/
/// CANCEL) draws on its line of credit (`UnusedCreditLine` of `CalcEntityFinancials`): amount by INPUTBOX
/// (+0.005 rounding), cash and `BankDebt` up, the lending bank's cash down; over 5 million a notice (with
/// a lowered credit rating) and news. Banks cannot borrow this way (popup). A player with the "sweep"
/// setting on is offered to turn it off ([`menu_toggle_sweep_cash`]; its CFIG.WSR bytes are returned).
/// Without a credit line the loan is denied with the reason (frozen credit line, lending limit, option
/// collateral, ETF debt limit, poor rating).
pub fn borrow_from_bank(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    time_secs: i32,
) -> Option<Vec<u8>> {
    let __content = crate::content::current();
    let mut cfg = None;
    let mut entity_id = g.sel_entity;
    let mut borrower_id = g.active_entity;
    let mut borrower_name = g.active_entity_name.clone();
    let mut etf_id = get_selected_player_etf(g, ui, rng);
    let mut msg_text: PbStr;
    if etf_id < 1581 {
        if borrower_id != g.sel_entity && 10 < g.sel_entity {
            msg_text = s(__content.string(3312))
                + &g.ent_name[entity_id]
                + "...!"
                + "\r"
                + "\r"
                + __content.string(3313)
                + &borrower_name
                + __content.string(3593);
            if ui.msgbox(&msg_text, 8227, &s(__content.string(3594))) != 6 {
                return cfg;
            }
        }
    } else {
        let etf_prompt = g.ent_name[etf_id].clone()
            + " ("
            + &g.ent_symbol[etf_id]
            + __content.string(3017)
            + __content.string(3018)
            + &g.ent_name[g.fund_adviser[etf_id]]
            + ". "
            + __content.string(3595)
            + "\r"
            + "\r"
            + __content.string(3020)
            + &g.ent_name[etf_id]
            + __content.string(3596)
            + "\r"
            + "\r"
            + __content.string(3567)
            + &g.active_entity_name
            + __content.string(3597);
        let reply = ui.msgbox(
            &etf_prompt,
            8483,
            &s(__content.string(3598)),
        );
        if reply == 2 {
            return cfg;
        }
        if reply == 7 {
            etf_id = 0;
        } else {
            borrower_id = etf_id;
            borrower_name = g.ent_name[etf_id].clone();
        }
    }
    entity_id = borrower_id;
    let e = entity_id;
    if 10 < e && e < 86 {
        let mut m = borrower_name
            + __content.string(3599)
            + __content.string(3600)
            + __content.string(3601)
            + "\r\n"
            + "\r\n"
            + __content.string(3602);
        show_quick_message(g, ui, &mut m);
        return cfg;
    }
    calc_entity_financials(g, ui, rng, e);
    if g.unused_credit_line != Ext::from_f64(0.0) {
        if e < 11 && g.sweep_cash_to_loan[e] == 1 {
            msg_text = s(
                __content.string(3603),
            ) + __content.string(3604)
                + "\r"
                + "\r"
                + __content.string(3605);
            let reply = ui.msgbox(&msg_text, 8227, &s(__content.string(3606)));
            if reply == 6 {
                cfg = Some(menu_toggle_sweep_cash(g, ui, rng, time_secs));
                let mut m = s(
                    __content.string(3607),
                ) + __content.string(3608);
                show_quick_message(g, ui, &mut m);
            } else {
                if reply == 2 {
                    return cfg;
                }
                let mut m = s(
                    __content.string(3609),
                ) + __content.string(3610);
                show_quick_message(g, ui, &mut m);
            }
            calc_entity_financials(g, ui, rng, e);
        }
        let mut max_amount_text =
            fmt_trim(g.unused_credit_line, b"* ###,.00") + " " + &g.million_prefix_uc + ".";
        msg_text = g.ent_name[e].clone()
            + __content.string(3611)
            + &max_amount_text
            + " from "
            + &g.ent_name[g.lender_id]
            + __content.string(3612);
        max_amount_text = remove(max_amount_text.as_bytes(), b",");
        let mut amount;
        loop {
            let answer = ui.inputbox(&msg_text, &s(__content.string(3613)), &max_amount_text);
            amount = val(remove_any(answer.as_bytes(), b"$ ,").as_bytes());
            if amount <= Ext::from_f64(0.0) {
                return cfg;
            }
            if !(g.unused_credit_line + sf(0.005f32) <= amount) {
                break;
            }
        }
        amount = amount + sf(0.005f32);
        g.cash[e] = g.cash[e] + amount;
        g.bank_debt[e] = g.bank_debt[e] + amount;
        let l = g.lender_id;
        g.cash[l] = g.cash[l] - amount;
        if amount <= Ext::from_f64(5.0) {
            let mut m = s("Done.");
            show_quick_message(g, ui, &mut m);
        } else {
            g.news_msg = g.ent_symbol[borrower_id].clone()
                + __content.string(3614)
                + &trim(g.currency_symbol.as_bytes())
                + &fmt_trim(amount, b"* #####,.00")
                + " "
                + &g.million_prefix_lc
                + "illion"
                + &g.currency_suffix;
            msg_text = s(__content.string(3615))
                + "\r"
                + "\r"
                + &g.news_msg
                + __content.string(3616);
            if borrower_id < 11 {
                msg_text = replace(msg_text.as_bytes(), b" its ", b" your ");
            }
            let prev_rating = g.credit_rating_num[borrower_id];
            calc_entity_financials(g, ui, rng, borrower_id);
            if g.credit_rating_num[borrower_id] < prev_rating {
                msg_text = msg_text
                    + __content.string(3617)
                    + &q()
                    + &g.credit_rating_str
                    + "."
                    + &q();
            }
            let title = s(__content.string(3618)) + &g.lender_bank_name + ":";
            ui.msgbox(&msg_text, 8256, &title);
            g.news_msg = g.news_msg.clone()
                + " from "
                + &g.lender_bank_name
                + " ("
                + &g.ent_symbol[g.lender_id]
                + ")";
            post_news(g, ui, 0, 0);
        }
        if 1580 < etf_id && 0 < g.fund_bonus_year[borrower_id] {
            g.fund_bonus_year[borrower_id] = g.cur_year;
        }
        // BankSystemLiquidity(LenderId): BUG (original, #311) the BYREF global moves on as banks are processed
        bank_system_liquidity_lender(g, ui, rng);
        update_market_display(g, ui, rng, time_secs);
        return cfg;
    }
    msg_text = s(__content.string(3619));
    if g.bank_debt[e] == Ext::from_f64(0.0) {
        msg_text = s(__content.string(3620));
    }
    if e < 11 || 85 < e {
        if 6 < g.credit_rating_num[e] {
            if g.credit_frozen[e] == 1 && g.cur_player != g.corp_controller[g.lender_id] {
                msg_text = s(__content.string(3621)) + &g.lender_bank_name;
                let bank_ctrl = if 0 < e {
                    g.corp_controller[cast::<i32, _>(g.lending_bank[e])]
                } else {
                    0
                };
                if 0 < e && 0 < bank_ctrl && bank_ctrl < 11 {
                    msg_text =
                        msg_text + __content.string(3622) + &g.ent_name[bank_ctrl] + ".";
                } else {
                    msg_text = msg_text + ".";
                }
            } else if borrower_id == g.bank_largest_debtor[g.lender_id] {
                msg_text = s(__content.string(3623))
                    + &g.lender_bank_name
                    + __content.string(3624)
                    + __content.string(3625)
                    + &g.ent_symbol[e]
                    + __content.string(3626);
            } else if g.long_options_value <= Ext::from_f64(0.0) {
                if 1580 < e
                    && g.industry_num[e] == 71
                    && g.tmp_credit_net_worth < g.tmp_total_debt * Ext::from_f64(3.0)
                {
                    msg_text = s(
                        __content.string(3627),
                    ) + __content.string(3628);
                }
            } else {
                msg_text = s(__content.string(3629))
                    + __content.string(3630)
                    + __content.string(3631)
                    + __content.string(3632);
            }
        }
    } else {
        msg_text = s(__content.string(3633)) + &msg_text;
    }
    msg_text = g.ent_name[e].clone() + &msg_text;
    let title = s(__content.string(3618)) + &g.lender_bank_name + ":";
    ui.msgbox(
        &(s(__content.string(3634)) + "\r" + "\r" + &msg_text),
        8208,
        &title,
    );
    cfg
}

/// Port of MenuToggleSweepCash (0x40d9d5).
/// Settings menu " [runtime resource wording] ": toggles `CurSweepCash` (the menu item's text is UI) and
/// explains the setting in a box. During a game the current player's `SweepCashToLoan` follows it and
/// cash is swept right away (`SweepPlayerCashToLoan`). Saves the configuration (`SaveConfig`; returns
/// the CFIG.WSR bytes) and sets `ConfigLoaded = "Y"`.
pub fn menu_toggle_sweep_cash(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    time_secs: i32,
) -> Vec<u8> {
    let __content = crate::content::current();
    let mut msg_text = s(
        __content.string(3635),
    ) + __content.string(3636)
        + __content.string(3637)
        + __content.string(3638)
        + __content.string(3639)
        + __content.string(3640)
        + "\r"
        + "\r";
    let mut status_text = s(__content.string(3641));
    if g.cur_sweep_cash == 0 {
        g.cur_sweep_cash = 1;
        // (UI) MENU SET TEXT MenuSettings, 668, " [runtime resource wording] "
    } else {
        g.cur_sweep_cash = 0;
        // (UI) MENU SET TEXT MenuSettings, 668, " [runtime resource wording] "
        status_text = replace(status_text.as_bytes(), b"'ON.'", b"'OFF.'");
        if 0 < g.cur_player && Ext::from_f64(0.0) < g.bank_debt[g.cur_player] {
            status_text = status_text
                + __content.string(3642);
        }
    }
    msg_text = msg_text + &status_text;
    if g.cur_player < 1 {
        msg_text = msg_text
            + "\r"
            + "\r"
            + __content.string(3643);
    } else {
        let cp = g.cur_player;
        g.sweep_cash_to_loan[cp] = g.cur_sweep_cash;
        sweep_player_cash_to_loan(g);
        update_market_display(g, ui, rng, time_secs);
    }
    let cfg = save_config(g); // SaveConfig(PrefCurrency)
    g.config_loaded = s("Y");
    ui.msgbox(
        &msg_text,
        8192,
        &s(__content.string(3644)),
    );
    cfg
}

/// Port of ChangeLendingBank (0x59bd8a).
/// Moves the acting entity's banking relationship (and loan) to another bank, asked by stock symbol
/// (`InBox` mode 3, `ActionCode = "CHANGEBANK"`, `LookupFilter = "BANKS"`). An outstanding loan
/// (`BankDebt` > 0.01) is refused when the credit line is frozen or the lender is controlled by someone
/// else than the player; the new bank refuses large "hot money" deposits, a suspicious loan of another
/// player's company, or lacks the liquid funds to buy the loan at `CalcLoanSalePricePct`. Otherwise the
/// loan is sold to the new bank (`SellLoanToBank`), `LendingBank` changes, news is posted and the turn's
/// action count rises. Banks themselves (11..85) cannot change.
pub fn change_lending_bank(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let ae = g.active_entity;
    if 10 < ae && ae < 86 {
        return;
    }
    let mut msg_text: PbStr;
    let mut lender_refuses = 0;
    calc_entity_financials(g, ui, rng, ae);
    g.lender_id = cast::<i32, _>(g.lending_bank[ae]);
    if g.credit_frozen[ae] == 1 {
        lender_refuses = 1;
    }
    let lender = g.lender_id;
    update_corp_control(g, ui, rng, lender);
    if 0 < get_controller_type(g, lender) {
        lender_refuses = 1;
    }
    if g.cur_player == g.voting_controller {
        lender_refuses = 0;
    }
    if sf(0.01f32) < g.bank_debt[ae] && lender_refuses == 1 {
        let pronoun = if ae < 11 { s(__content.string(3645)) } else { s("its ") };
        msg_text = g.active_entity_name.clone()
            + " owes "
            + &g.ent_name[g.lender_id]
            + " "
            + &g.currency_symbol
            + &fmt_trim(g.bank_debt[ae], b"###############")
            + " "
            + &g.million_prefix_uc
            + "."
            + "\r"
            + "\r"
            + &g.active_entity_name
            + __content.string(3646)
            + __content.string(3647)
            + &pronoun
            + __content.string(3648)
            + __content.string(3649)
            + "\r"
            + "\r";
        if g.lender_id != g.voting_controller && 0 < g.voting_controller {
            msg_text = msg_text
                + __content.string(3650)
                + &g.ent_name[g.voting_controller]
                + __content.string(3651);
        }
        if 10 < ae && g.bonds_issued[ae] == 0 {
            msg_text = msg_text
                + "\r"
                + "\r"
                + __content.string(3652);
        }
        ui.msgbox(&msg_text, 8208, &s(__content.string(3653)));
        return;
    }
    g.tmp_symbol = PbStr::new();
    g.action_code = s("CHANGEBANK");
    g.input_dlg_label = PbStr::new();
    g.input_dlg_title = s(__content.string(3654)) + &g.active_entity_name;
    g.input_dlg_prompt = s(__content.string(3655))
        + &g.active_entity_name
        + __content.string(3656)
        + __content.string(3392)
        + __content.string(3393)
        + "\r\n"
        + "\r\n";
    g.lookup_filter = s("BANKS");
    in_box_symbol(g, ui, rng);
    g.lookup_filter = PbStr::new();
    if g.tmp_symbol.is_empty() {
        return;
    }
    let new_bank_id = g.target_corp;
    if cast::<i32, _>(g.lending_bank[ae]) == new_bank_id || new_bank_id < 11 || 85 < new_bank_id {
        no_change(g, ui, ae);
        return;
    }
    calc_entity_financials(g, ui, rng, new_bank_id);
    if g.exchange_rate * Ext::from_f64(3000.0) < g.cash[ae]
        && g.total_assets * sf(0.2f32) < g.cash[ae]
        && g.bank_debt[ae] * sf(1.5f32) < g.cash[ae]
    {
        msg_text = s(__content.string(3657))
            + __content.string(3658)
            + __content.string(3659)
            + __content.string(3660)
            + __content.string(3661)
            + &g.ent_name[new_bank_id]
            + ".";
        let title = s(__content.string(3662)) + &g.ent_name[new_bank_id];
        ui.msgbox(&msg_text, 8256, &title);
        no_change(g, ui, ae);
        return;
    }
    if sf(0.01f32) < g.bank_debt[ae] {
        calc_entity_financials(g, ui, rng, ae);
        update_corp_control(g, ui, rng, new_bank_id);
        let vc = g.voting_controller;
        if 10 < ae
            && 0 < vc
            && vc < 11
            && g.cur_player != vc
            && g.exchange_rate * Ext::from_f64(300.0) < g.bank_debt[ae]
        {
            msg_text = g.ent_name[new_bank_id].clone()
                + __content.string(1121)
                + &g.ent_name[vc]
                + " "
                + __content.string(3663)
                + __content.string(3664);
            ui.msgbox(&msg_text, 8256, &s(__content.string(3665)));
            return;
        }
        let sale_price_pct = calc_loan_sale_price_pct(g, ui, rng, ae);
        let sale_price_frac = cast::<Ext, _>(sale_price_pct) * sf(0.01f32);
        if g.cash[ae] + g.cash[new_bank_id] + g.t_bills[new_bank_id]
            <= g.bank_debt[ae] * sale_price_frac
        {
            msg_text = g.ent_name[g.target_corp].clone()
                + __content.string(3666)
                + &g.active_entity_name
                + __content.string(3667)
                + &g.ent_name[g.lender_id]
                + __content.string(3436)
                + "\r"
                + "\r"
                + __content.string(3668);
            if g.cur_player == g.corp_controller[new_bank_id] {
                msg_text = msg_text
                    + "\r"
                    + "\r"
                    + __content.string(3669)
                    + &g.ent_name[new_bank_id]
                    + __content.string(3670);
            }
            ui.msgbox(&msg_text, 8256, &s(__content.string(3665)));
            no_change(g, ui, ae);
            return;
        }
        g.lender_id = cast::<i32, _>(g.lending_bank[ae]);
        let lender = g.lender_id;
        sell_loan_to_bank(g, ui, rng, new_bank_id, lender, ae);
    }
    g.lending_bank[ae] = cast::<i16, _>(new_bank_id);
    msg_text = g.active_entity_name.clone() + __content.string(3671) + &g.ent_name[new_bank_id];
    ui.msgbox(&msg_text, 8192, &s(__content.string(3672)));
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    g.turn_action_count = g.turn_action_count + 1;
}

/// ChangeLendingBank's "no change" popup.
fn no_change(g: &mut Globals, ui: &mut dyn Ui, ae: i32) {
    let __content = crate::content::current();
    let mut m = s(__content.string(3673))
        + &g.ent_name[cast::<i32, _>(g.lending_bank[ae])]
        + __content.string(3674);
    show_quick_message(g, ui, &mut m);
}

/// Port of PrepayIncomeTax (0x5c1974).
/// The player (`ActiveEntity = CurPlayer`) prepays estimated income tax (`PlayerPrepaidTax`), at most
/// 20% of the net worth or the liquid funds; an amount over cash + T-bills is borrowed
/// (`BorrowToCoverPurchase`, NO cancels; cash is then 0).
pub fn prepay_income_tax(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    let __content = crate::content::current();
    let player_id = g.active_entity;
    if g.cur_player != g.active_entity {
        return;
    }
    calc_entity_financials(g, ui, rng, player_id);
    if g.liquid_funds < sf(0.01f32) {
        let mut m = s(__content.string(3675));
        show_quick_message(g, ui, &mut m);
        return;
    }
    if g.net_worth * sf(0.2f32) < g.liquid_funds {
        g.liquid_funds = g.net_worth * sf(0.2f32);
    }
    let max_text = trim(g.currency_symbol.as_bytes())
        + &fmt_trim(g.liquid_funds, b"* ###,.00")
        + " "
        + &g.million_prefix_lc
        + "illion";
    let prompt_text = s(
        __content.string(3676),
    ) + __content.string(3677)
        + &max_text
        + &g.currency_suffix
        + ".)";
    let mut answer;
    let mut amount;
    loop {
        let title = g.active_entity_name.clone() + __content.string(3678);
        answer = ui.inputbox(&prompt_text, &title, &PbStr::new());
        amount = val(remove_any(answer.as_bytes(), b"$ ,%").as_bytes());
        if amount < sf(0.01f32) {
            return;
        }
        if !(g.liquid_funds + sf(0.01f32) < amount) {
            break;
        }
    }
    let mut shortfall = Ext::from_f64(0.0);
    if amount <= g.liquid_funds {
        let ae = g.active_entity;
        shortfall = amount - (g.cash[ae] + g.t_bills[ae]);
        if Ext::from_f64(0.0) < shortfall && borrow_to_cover_purchase(g, ui, &mut shortfall, amount, 0) == 7 {
            return;
        }
    }
    g.cash[player_id] = g.cash[player_id] - amount;
    if Ext::from_f64(0.0) < shortfall {
        g.cash[player_id] = Ext::from_f64(0.0);
    }
    g.player_prepaid_tax[player_id] = g.player_prepaid_tax[player_id] + amount;
    let msg_text = s(__content.string(3679))
        + &trim(g.currency_symbol.as_bytes())
        + &answer
        + " "
        + &g.million_prefix_lc
        + "illion"
        + &g.currency_suffix
        + __content.string(3680);
    update_market_display(g, ui, rng, time_secs);
    ui.msgbox(&msg_text, 8192, &s(__content.string(3681)));
}
