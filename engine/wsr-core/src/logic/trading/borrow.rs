//! `BorrowToCoverPurchase` (0x5246b6): the bank financing question asked before a purchase that the
//! acting entity's cash cannot cover.

// Keep the original's statement shapes.
#![allow(clippy::assign_op_pattern)]
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use crate::logic::finance::calc_credit_rating;

/// Port of BorrowToCoverPurchase (0x5246b6) with its GOSUBs Borrow_ShowCreditImpact (0x524ede) and
/// Borrow_ExitGosub (0x5254cf, the common exit returning the result).
///
/// The acting entity (`ActiveEntity`, must be active) needs `amt_needed` (BYREF EXT, in millions) more
/// cash: its T-bills are cashed in first (`Cash += TBills`, whenever `amt_needed > 0`). A negative cash
/// balance is first paid off by drawing on the credit line when both fit into `UnusedCreditLine` (popup;
/// `BankDebt`, the lending bank's cash, `UnusedCreditLine` and `amt_needed` are adjusted, `Cash = 0`).
/// If the credit line is too small: popup, returns 7 (NO). For 0.51 million or more the player is asked
/// (YES/NO/CANCEL, "Transaction Financing"; the message shows the change of the debt/equity ratio and
/// credit rating computed from `NetWorth`, `TmpTotalDebt`, `TmpCreditNetWorth` of the caller's
/// `CalcEntityFinancials`, with `equity_loss` lowering the net worth and `debt_repaid` (BYREF QUAD)
/// lowering the debt); CANCEL counts as NO. On YES (or below 0.51 million without asking) the entity
/// borrows `amt_needed + 0.01`: Cash and BankDebt up, the lending bank's cash down.
///
/// Returns 6 (YES: borrowed or nothing needed), 7 (NO / cancelled / not possible), or 0 when the
/// acting entity is not active or `amt_needed <= 0` (`result` is never set on those paths).
pub fn borrow_to_cover_purchase(
    g: &mut Globals,
    ui: &mut dyn Ui,
    amt_needed: &mut Ext,
    equity_loss: Ext,
    debt_repaid: i64,
) -> i32 {
    let __content = crate::content::current();
    let mut answer = 6;
    let mut result = 0;
    if g.entity_status[g.active_entity] != 1 {
        return result;
    }
    let ent_id = g.active_entity;
    let mut equity_loss_adj = equity_loss;
    let mut more_funds_text = PbStr::new();
    let mut clause_text;
    if Ext::from_f64(0.0) < *amt_needed && Ext::from_f64(0.0) < g.t_bills[ent_id] {
        g.cash[ent_id] = g.cash[ent_id] + g.t_bills[ent_id];
        g.t_bills[ent_id] = Ext::from_f64(0.0);
    }
    if *amt_needed <= Ext::from_f64(0.0) {
        return result;
    }
    if g.cash[ent_id] < Ext::from_f64(0.0) && *amt_needed <= g.unused_credit_line && g.cash[ent_id].abs() < g.unused_credit_line {
        g.bank_debt[ent_id] = g.bank_debt[ent_id] + g.cash[ent_id].abs();
        let bank = cast::<i32, _>(g.lending_bank[ent_id]);
        g.cash[bank] = g.cash[bank] + g.cash[ent_id];
        g.unused_credit_line = g.unused_credit_line + g.cash[ent_id];
        *amt_needed = *amt_needed + g.cash[ent_id];
        clause_text = s("your ");
        if 10 < g.active_entity {
            clause_text = g.ent_name[ent_id].clone() + "'s ";
        }
        let mut m = s(__content.string(11876))
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(g.cash[ent_id].abs(), "* ###,.00")
            + &g.currency_suffix
            + " in "
            + &clause_text
            + __content.string(11877);
        show_quick_message(g, ui, &mut m);
        g.cash[ent_id] = Ext::from_f64(0.0);
        more_funds_text = s(__content.string(11878));
    }
    if g.unused_credit_line < *amt_needed {
        let mut m = g.active_entity_name.clone()
            + __content.string(11879);
        show_quick_message(g, ui, &mut m);
        result = 7;
        return result;
    }
    if sf(0.51f32) <= *amt_needed {
        clause_text = s("DO YOU");
        let mut possessive = s("YOUR");
        if 10 < ent_id {
            clause_text = s("DOES ") + &g.ent_symbol[ent_id];
            possessive = s("ITS");
        }
        let mut msg_text = g.ent_name[ent_id].clone()
            + __content.string(11880)
            + &more_funds_text
            + __content.string(11881)
            + __content.string(11882)
            + "is "
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(*amt_needed, "###############,.0")
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix
            + "."
            + "\r"
            + "\r"
            + &clause_text
            + __content.string(11883)
            + &possessive
            + __content.string(11884)
            + __content.string(11885)
            + "\r"
            + "\r";
        let mut impact_text = s(__content.string(11886))
            + "of "
            + &g.ent_name[ent_id]
            + " from ";
        borrow_show_credit_impact(
            g,
            ent_id,
            *amt_needed,
            &mut equity_loss_adj,
            debt_repaid,
            &possessive,
            &mut impact_text,
        );
        msg_text = msg_text + &impact_text;
        answer = ui.msgbox(&msg_text, 8227, &s(__content.string(11887)));
        if answer == 2 {
            answer = 7;
        }
    }
    result = answer;
    if answer == 7 {
        return result;
    }
    g.cash[ent_id] = g.cash[ent_id] + (*amt_needed + sf(0.01f32));
    g.bank_debt[ent_id] = g.bank_debt[ent_id] + (*amt_needed + sf(0.01f32));
    let bank = cast::<i32, _>(g.lending_bank[ent_id]);
    g.cash[bank] = g.cash[bank] - (*amt_needed + sf(0.01f32));
    result
}

/// Port of Borrow_ShowCreditImpact (0x524ede), GOSUB of BorrowToCoverPurchase.
/// Appends to `impact_text` the debt/equity ratio before (`TmpTotalDebt / NetWorth`) and after the loan
/// (`(TmpTotalDebt + amt - debt_repaid) / (NetWorth - equity_loss_adj)`, "negative" if not positive)
/// and the effect on the credit rating (`CalcCreditRating` on the changed figures, which are restored
/// afterwards, as is `CreditRatingNum`; `CreditRatingStr` keeps the new rating). Without a positive net
/// worth the text is cleared. `equity_loss_adj` (the caller's local) gets +0.5 (0 below 0.1).
fn borrow_show_credit_impact(
    g: &mut Globals,
    ent_id: i32,
    amt_needed: Ext,
    equity_loss_adj: &mut Ext,
    debt_repaid: i64,
    possessive: &PbStr,
    impact_text: &mut PbStr,
) {
    let __content = crate::content::current();
    let old_rating_str = g.credit_rating_str.clone();
    let mut de_ratio: Ext = if g.net_worth <= Ext::from_f64(0.0) { -Ext::from_f64(99.0) } else { g.tmp_total_debt / g.net_worth };
    let old_de_text = fmt_trim(de_ratio, "######.00");
    if de_ratio == -Ext::from_f64(99.0) {
        *impact_text = PbStr::new();
        return;
    }
    let mut new_de_text = PbStr::new();
    *equity_loss_adj = *equity_loss_adj + sf(0.5f32);
    if *equity_loss_adj < sf(0.1f32) {
        *equity_loss_adj = Ext::from_f64(0.0);
    }
    if g.net_worth - *equity_loss_adj <= Ext::from_f64(0.0) {
        de_ratio = -Ext::from_f64(1.0);
    } else {
        de_ratio = ((g.tmp_total_debt + amt_needed) - cast::<Ext, _>(debt_repaid)) / (g.net_worth - *equity_loss_adj);
        new_de_text = fmt_trim(de_ratio, "######.00");
    }
    if de_ratio <= Ext::from_f64(0.0) {
        new_de_text = s(__content.string(7177));
    }
    *impact_text = impact_text.clone() + &old_de_text + " to " + &new_de_text;
    let saved_net_worth = g.net_worth;
    let saved_credit_nw = g.tmp_credit_net_worth;
    let saved_total_debt = g.tmp_total_debt;
    let saved_rating_num = g.credit_rating_num[ent_id];
    g.net_worth = g.net_worth - *equity_loss_adj;
    g.tmp_credit_net_worth = g.tmp_credit_net_worth - *equity_loss_adj;
    g.tmp_total_debt = (g.tmp_total_debt + amt_needed) - cast::<Ext, _>(debt_repaid);
    calc_credit_rating(g, ent_id);
    let lc_possessive = lcase(possessive.as_bytes(), CaseMode::Ansi);
    let mut clause_text = s(__content.string(11888))
        + &lc_possessive
        + " "
        + __content.string(11889)
        + &old_rating_str
        + " to "
        + &g.credit_rating_str
        + ".)";
    if g.credit_rating_str == "D" {
        clause_text = replace(clause_text.as_bytes(), __content.bytes(11890), __content.bytes(11891));
        let with = s(__content.string(11892)) + &g.active_entity_name + "!)";
        clause_text = replace(clause_text.as_bytes(), b".)", with.as_bytes());
    }
    if old_rating_str == g.credit_rating_str {
        clause_text = s(__content.string(11893)) + &lc_possessive + " " + &old_rating_str + __content.string(11894);
    }
    *impact_text = impact_text.clone() + &clause_text;
    if saved_rating_num < g.credit_rating_num[ent_id] {
        *impact_text = PbStr::new();
    }
    if 0 < debt_repaid && g.tmp_total_debt < saved_total_debt {
        *impact_text = PbStr::new();
    }
    let mut improved = false;
    if 0 < debt_repaid && val(old_de_text.as_bytes()) < val(new_de_text.as_bytes()) && impact_text.is_empty() {
        *impact_text = s(__content.string(11895)) + &g.ent_symbol[ent_id] + "'s " + __content.string(11896);
        improved = true;
    }
    if !improved && old_de_text == new_de_text {
        *impact_text = s(__content.string(11897))
            + &g.ent_symbol[ent_id]
            + "'s "
            + __content.string(11896);
    }
    g.credit_rating_num[ent_id] = saved_rating_num;
    g.net_worth = saved_net_worth;
    g.tmp_credit_net_worth = saved_credit_nw;
    g.tmp_total_debt = saved_total_debt;
}
