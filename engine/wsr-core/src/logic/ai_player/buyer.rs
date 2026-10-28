//! Helpers shared by several AiComputerPlayerTurn steps: buyer validation (`AiPlayer_ValidateBuyerCorp`,
//! `AiPlayer_GetBuyerHoldingPct`), the pending message box, and the stand-alone `AiFindAffordableBuyer`
//! and `AddRecentOffering`.
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::prelude::*;
use crate::logic::control::update_corp_control;
use crate::logic::finance::calc_entity_financials;
use crate::logic::holdings::find_holding_slot;

/// Port of AiPlayer_ValidateBuyerCorp (0x5c86a9), GOSUB of AiComputerPlayerTurn ("5000").
/// Checks that `buyerCorp` (the player itself or a company it controls) can buy `targetPct` % of
/// `targetId` and buys it: requires liquid funds (`Cash + TBills + UnusedCreditLine / 4`; the player:
/// its cash) above `MarketCap(target) * (1 + targetPct)`, an existing holding slot and a public float
/// with `PublicPct + curPct > 20`; borrows a cash shortfall from the buyer's lending bank and calls
/// `BuyStock(buyerCorp, targetId)` for `MIN(targetPct, PublicPct)` %. On success `boughtFlag = 1` (and
/// `newCorpId` = a weak first-time target), otherwise `buyerCorp = 0`.
pub(crate) fn validate_buyer_corp(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    f.trace_tag = PbStr::from("5000");
    if f.buyer_corp < 1 || f.target_id < 10 || f.target_pct < 1 || f.target_id == f.buyer_corp {
        f.buyer_corp = 0;
        return;
    }
    if ((cast::<Ext, _>(g.asbestos_corp)).abs() == cast::<Ext, _>(f.target_id) || cast::<i32, _>(g.superfund_corp) == f.target_id)
        && f.allow_toxic != 1
    {
        f.buyer_corp = 0;
        return;
    }
    if f.player_id != f.buyer_corp && f.player_id != g.corp_controller[f.buyer_corp] {
        f.buyer_corp = 0;
        return;
    }
    calc_entity_financials(g, ui, rng, f.buyer_corp);
    let b = f.buyer_corp;
    g.liquid_funds = g.t_bills[b] + g.cash[b] + g.unused_credit_line * sf(0.25f32);
    if b == f.player_id {
        g.liquid_funds = g.cash[b];
    }
    if g.liquid_funds <= g.market_cap[f.target_id] * (Ext::from_f64(1.0) + cast::<Ext, _>(f.target_pct)) {
        f.buyer_corp = 0;
        return;
    }
    get_buyer_holding_pct(g, f);
    update_corp_control(g, ui, rng, f.target_id);
    if !(f.found == 1 && 20 < cast::<i32, _>(g.public_pct) + f.cur_pct) {
        f.buyer_corp = 0;
        return;
    }
    f.pay_amt = cast::<Ext, _>(f.target_pct) * g.market_cap[f.target_id] * sf(1.01f32);
    if g.liquid_funds < f.pay_amt {
        f.buyer_corp = 0;
    } else {
        f.shortfall = f.pay_amt - g.cash[b];
        if Ext::from_f64(0.0) < f.shortfall {
            g.cash[b] = f.shortfall + g.cash[b];
            g.bank_debt[b] = f.shortfall + g.bank_debt[b];
            let lb = cast::<i32, _>(g.lending_bank[b]);
            g.cash[lb] = g.cash[lb] - f.shortfall;
        }
    }
    if f.buyer_corp == 0 {
        return;
    }
    g.tmp_ext1 = cast::<Ext, _>(f.target_pct);
    if (cast::<i32, _>(g.public_pct)) < f.target_pct {
        g.tmp_ext1 = cast::<Ext, _>(g.public_pct);
    }
    g.tmp_ext2 = Ext::from_f64(1.0);
    crate::logic::trading::buy_stock(g, ui, rng, f.buyer_corp, f.target_id, f.time_secs);
    f.found = 0;
    f.buy_slot = 1;
    while f.buy_slot < 16 {
        if cast::<i32, _>(g.hold_stock_id[(f.buy_slot, f.buyer_corp)]) == f.target_id
            && cast::<i32, _>(g.hold_stock_pct[(f.buy_slot, f.buyer_corp)]) == f.target_pct + f.cur_pct
        {
            f.found = 1;
        }
        f.buy_slot += 1;
    }
    if f.found == 1 {
        f.bought_flag = 1;
        if f.new_corp_id == 0
            && f.cur_pct == 0
            && g.industry_num[f.target_id] < 70
            && g.profit_margin[f.target_id] < 6
        {
            f.new_corp_id = f.target_id;
        }
    } else {
        f.buyer_corp = 0;
    }
}

/// Port of AiPlayer_GetBuyerHoldingPct (0x5c8b9b), GOSUB of AiPlayer_ValidateBuyerCorp.
/// `holdSlot = FindHoldingSlot(buyerCorp, targetId)`; `found = 1` and `curPct` = the slot's percentage
/// when a slot exists (the holding of the target, or else the first free slot, whose percentage is 0).
pub(crate) fn get_buyer_holding_pct(g: &mut Globals, f: &mut Frame) {
    f.found = 0;
    f.cur_pct = 0;
    f.hold_slot = find_holding_slot(g, f.buyer_corp, f.target_id);
    if 0 < f.hold_slot {
        f.found = 1;
        f.cur_pct = cast::<i32, _>(g.hold_stock_pct[(f.hold_slot, f.buyer_corp)]);
    }
}

/// Port of AiPlayer_ShowPendingMsg (0x5cfa91), GOSUB of AiComputerPlayerTurn.
/// `MSGBOX pendingMsg, 8256, AppTitle` and clears `pendingMsg`.
pub(crate) fn show_pending_msg(g: &mut Globals, ui: &mut dyn Ui, f: &mut Frame) {
    ui.msgbox(&f.pending_msg, 8256, &g.app_title);
    f.pending_msg = PbStr::new();
}

/// Port of AiFindAffordableBuyer (0x5d1bc6).
/// Looks for a company (11..1600) that can buy `pct_needed` % of `target_id`: active, controlled by
/// player 1 (sic, see BUGS.md #260), not a bank with stock holdings, banks only for purchases of at
/// least `ExchangeRate * 1200`, fewer than 15 holdings, not `AiTargetCorp` when `allow_toxic = 1`, not
/// the target, with `TBills + Cash + UnusedCreditLine / 4` (+ 3/4 of a positive working capital for
/// operating companies) of at least `MarketCap(target) * (1 + pct_needed)`. An operating company whose
/// `LiquidFunds` fall short moves 3/4 of its working capital to cash. Returns the first such company,
/// 0 if none, if the target is the asbestos/superfund company (unless `allow_toxic = 1`), or if the
/// public float is below `pct_needed`.
///
/// The ON ERROR handler (0x5d2046) only shows ShowRuntimeError (not ported).
pub fn ai_find_affordable_buyer(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    target_id: i32,
    pct_needed: i32,
    allow_toxic: i32,
) -> i32 {
    let mut found_id = 0;
    if !(0 < pct_needed && 10 < target_id && target_id < 1601) {
        return 0;
    }
    if ((cast::<Ext, _>(g.asbestos_corp)).abs() == cast::<Ext, _>(target_id) || cast::<i32, _>(g.superfund_corp) == target_id)
        && allow_toxic != 1
    {
        return 0;
    }
    update_corp_control(g, ui, rng, target_id);
    if (cast::<i32, _>(g.public_pct)) < pct_needed {
        return 0;
    }
    let cost_needed = g.market_cap[target_id] * (Ext::from_f64(1.0) + cast::<Ext, _>(pct_needed));
    let mut cand = 11;
    while cand < 1601 {
        let ok = g.entity_status[cand] == 1
            && g.corp_controller[cand] == 1
            && !(cand < 86 && 0 < g.hold_stock_id[(1, cand)])
            && !(cand < 86 && cost_needed < g.exchange_rate * Ext::from_f64(1200.0))
            && !(14 < g.hold_stock_id[(0, cand)])
            && !(g.ai_target_corp == cand && allow_toxic == 1)
            && target_id != cand;
        if ok {
            calc_entity_financials(g, ui, rng, cand);
            let mut buyer_funds = g.t_bills[cand] + g.cash[cand] + g.unused_credit_line * sf(0.25f32);
            if 130 < cand && Ext::from_f64(0.0) < g.working_capital[cand] {
                buyer_funds = g.working_capital[cand] * sf(0.75f32) + buyer_funds;
            }
            if cost_needed <= buyer_funds {
                if g.liquid_funds < cost_needed && 130 < cand {
                    g.cash[cand] = g.working_capital[cand] * sf(0.75f32) + g.cash[cand];
                    g.working_capital[cand] = g.working_capital[cand] * sf(0.25f32);
                }
                found_id = cand;
                cand = 1600;
            }
        }
        cand += 1;
    }
    found_id
}

/// Port of AddRecentOffering (0x498ad8).
/// Shifts the list `RecentOfferings(1..10)` down by one and appends `corp_id` (`offer_type` 1: a stock
/// offering) or `-corp_id` (2: a buyback); other types leave slot 10 unchanged (a copy of slot 10 stays
/// in 9).
pub fn add_recent_offering(g: &mut Globals, corp_id: i32, offer_type: i32) {
    for i in 1..=9 {
        g.recent_offerings[i] = g.recent_offerings[i + 1];
    }
    if offer_type == 1 {
        g.recent_offerings[10] = corp_id;
    } else if offer_type == 2 {
        g.recent_offerings[10] = -corp_id;
    }
}
