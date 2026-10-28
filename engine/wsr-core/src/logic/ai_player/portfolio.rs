//! AiComputerPlayerTurn steps: more stock purchases, idle holding companies, consolidation, selling stock
//! for cash, Ethereum, end-game debt repayment.
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::prelude::*;
use super::buyer::{add_recent_offering, ai_find_affordable_buyer, validate_buyer_corp};
use crate::logic::commod_ai::physical::{ai_trade_phys_commod, buy_phys_commod};
use crate::logic::control::{find_option_positions, update_corp_control};
use crate::logic::earnings::sell_t_bills_to_cash;
use crate::logic::finance::{calc_capital_taxes, calc_entity_financials, get_phys_commod_value};
use crate::logic::futures::count_futures_positions;
use crate::logic::holdings::{find_holding_slot, find_stock_holding};
use crate::logic::options::get_optioned_stock_pct;
use crate::logic::sim_entity::ai_corp::is_recent_offering;
use crate::logic::sim_entity::bonds::issue_stock;
use crate::logic::sim_entity::etf::ai_create_option_position;
use crate::logic::sim_entity::liquidation::liquidate_corporation;
use crate::logic::sim_entity::quarter_start::pack_holding_arrays;
use crate::logic::sim_entity::transactions::sell_stock_holding;
use crate::logic::swaps::cancel_entity_swaps;
use crate::logic::valuation::calc_analyst_rating;

/// `IF Cash(b) < payAmt`: sell T-bills for the difference, then borrow what is still missing from the
/// lending bank (Cash up, the bank's cash down, BankDebt up). Shared by both purchase paths of
/// BuyMoreStock.
fn raise_purchase_cash(g: &mut Globals, f: &mut Frame) {
    let b = f.buyer_corp;
    if g.cash[b] < f.pay_amt {
        let need = f.pay_amt - g.cash[b];
        sell_t_bills_to_cash(g, b, need);
    }
    if g.cash[b] < f.pay_amt {
        f.shortfall = f.pay_amt - g.cash[b];
        g.cash[b] = f.shortfall + g.cash[b];
        let lb = cast::<i32, _>(g.lending_bank[b]);
        g.cash[lb] = g.cash[lb] - f.shortfall;
        g.bank_debt[b] = f.shortfall + g.bank_debt[b];
    }
}

/// Port of AiPlayer_BuyMoreStock (0x5d020b), GOSUB of AiComputerPlayerTurn.
/// When the player controls `AiTargetCorp`: from a random company (11..1599, or 11 with 1/5 chance) on,
/// scans the companies it controls that hold stock and looks at each holding below 51% (not
/// `AiTargetCorp`, not an already controlled >19% stake, not a controlled stake worth more than
/// `ExchangeRate * 1500`): a stake below 14% of a company the analysts rate above 1 is sold; otherwise,
/// if the public float covers the stake needed for control (`LargestHolderPct + 1 - curPct`, at least
/// 20 - curPct) and the player is below `CpuMaxCorps` (or controls the company already), a company with
/// positive quarterly earnings (field 3 of `EntMiscData`) and projected earnings gets bought into:
/// directly by the holder (1..5%, by its liquid funds) or, when the holder's `LiquidFunds` are at most
/// 1.3 x the market cap, through [`ai_find_affordable_buyer`] (5%); the buyer raises 1.2 x the price
/// from T-bills and its bank, and [`validate_buyer_corp`] buys. The first sale or purchase ends the scan.
pub(crate) fn buy_more_stock(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    if g.ai_target_corp == 0 || f.player_id != g.corp_controller[g.ai_target_corp] {
        return;
    }
    pack_holding_arrays(g, ui);
    f.pick_id = rng.rnd_range(11, 1599);
    if rng.rnd() * Ext::from_f64(5.0) < Ext::from_f64(1.0) {
        f.pick_id = 11;
    }
    f.ent_id = f.pick_id;
    while f.ent_id <= 1600 {
        let e = f.ent_id;
        if g.entity_status[e] == 1 && f.player_id == g.corp_controller[e] && g.hold_stock_id[(0, e)] != 0 {
            calc_entity_financials(g, ui, rng, e);
            f.slot = 1;
            while f.slot <= 15 {
                buy_more_stock_slot(g, ui, rng, f);
                f.slot += 1;
            }
        }
        f.ent_id += 1;
    }
}

/// One holding (`slot` of `entId`) of [`buy_more_stock`].
fn buy_more_stock_slot(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let e = f.ent_id;
    f.cur_pct = 0;
    f.other_id = cast::<i32, _>(g.hold_stock_id[(f.slot, e)]);
    f.cur_pct = cast::<i32, _>(g.hold_stock_pct[(f.slot, e)]);
    if f.other_id == 0 || 50 < f.cur_pct || g.ai_target_corp == f.other_id {
        return;
    }
    let o = f.other_id;
    update_corp_control(g, ui, rng, o);
    if f.player_id == g.corp_controller[o] && 19 < f.cur_pct {
        return;
    }
    if f.player_id == g.corp_controller[o] && g.exchange_rate * Ext::from_f64(1500.0) < g.market_cap[o] * cast::<Ext, _>(f.cur_pct) {
        return;
    }
    if g.largest_holder_pct < 20 {
        g.largest_holder_pct = 19;
    }
    f.cash_amt = to_quad((Ext::from_f64(1.0) + cast::<Ext, _>(g.largest_holder_pct)) - cast::<Ext, _>(f.cur_pct));
    if f.cash_amt < 0 {
        f.cash_amt = 0;
    }
    if f.cur_pct < 14 {
        f.tmp_val = 1;
        if f.tmp_val < calc_analyst_rating(g, ui, rng, o) {
            g.tmp_ext1 = cast::<Ext, _>(f.ent_id);
            g.tmp_ext2 = cast::<Ext, _>(f.cur_pct);
            g.tmp_ext3 = Ext::from_f64(1.0);
            sell_stock_holding(g, ui, rng, o);
            g.tmp_ext1 = Ext::from_f64(0.0);
            f.slot = 15;
            f.ent_id = 1600;
        }
        return;
    }
    if !(0 < g.public_pct && f.cash_amt <= cast::<i64, _>(g.public_pct)) {
        return;
    }
    if f.player_id != g.corp_controller[o] && g.cpu_max_corps <= g.player_corps_controlled[f.player_id] {
        return;
    }
    f.pending_msg = PbStr::new();
    f.tmp_val = 20;
    if f.tmp_val < len(g.ent_misc_data[o].as_bytes()) {
        f.pending_msg = parse_any(g.ent_misc_data[o].as_bytes(), b"|", 3);
    }
    if !(Ext::from_f64(0.0) < val(f.pending_msg.as_bytes())) || !(Ext::from_f64(0.0) < g.proj_qtr_earnings[o]) {
        return;
    }
    if g.liquid_funds > g.market_cap[o] * sf(1.3f32) {
        f.buyer_corp = f.ent_id;
        f.target_pct = 1;
        if g.liquid_funds > g.market_cap[o] * Ext::from_f64(4.0) {
            f.target_pct = 3;
        }
        if g.liquid_funds > g.market_cap[o] * Ext::from_f64(7.0) {
            f.target_pct = 5;
        }
    } else {
        f.buyer_corp = ai_find_affordable_buyer(g, ui, rng, o, 5, 0);
        if f.buyer_corp <= 10 {
            calc_entity_financials(g, ui, rng, f.ent_id);
            return;
        }
        calc_entity_financials(g, ui, rng, f.buyer_corp);
        f.target_pct = 5;
    }
    if (cast::<i32, _>(g.public_pct)) < f.target_pct {
        f.target_pct = cast::<i32, _>(g.public_pct);
    }
    f.pay_amt = cast::<Ext, _>(f.target_pct) * g.market_cap[o] * sf(1.2f32);
    raise_purchase_cash(g, f);
    f.target_id = f.other_id;
    validate_buyer_corp(g, ui, rng, f);
    if 0 < f.buyer_corp {
        f.slot = 15;
        f.ent_id = 1600;
    }
}

/// Port of AiPlayer_LiquidateIdleHoldingCo (0x5d0c41), GOSUB of AiComputerPlayerTurn.
/// Liquidates (LiquidateCorporation, auto mode 1) the first holding company (industry 70, 131..1600,
/// not `AiTargetCorp`) the player controls that has no bonds, a tax-loss carry of at most 10 and tax
/// credits of at most 10, no options, futures, physical commodities (value <= 0) or swaps.
pub(crate) fn liquidate_idle_holding_co(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    f.ent_id = 131;
    while f.ent_id <= 1600 {
        let e = f.ent_id;
        if g.ai_target_corp != e
            && g.entity_status[e] == 1
            && f.player_id == g.corp_controller[e]
            && g.industry_num[e] == 70
            && g.bonds_issued[e] <= 0
            && !(g.tax_loss_carry[e] < -Ext::from_f64(10.0))
            && g.tax_credits[e] <= Ext::from_f64(10.0)
        {
            f.tmp_val = 0;
            if find_option_positions(g, ui, e, 0, 0, 0) == f.tmp_val {
                f.tmp_val = 0;
                if count_futures_positions(g, e, 0) <= f.tmp_val
                    && get_phys_commod_value(g, e, &PbStr::new()) <= Ext::from_f64(0.0)
                {
                    f.tmp_val = 1;
                    if cancel_entity_swaps(g, ui, rng, e, 2, 1) != f.tmp_val {
                        liquidate_corporation(g, ui, rng, e, 1, &f.now.clone());
                        f.ent_id = 1600;
                    }
                }
            }
        }
        f.ent_id += 1;
    }
}

/// Port of AiPlayer_ConsolidateTopHoldings (0x5d0eb9), GOSUB of AiComputerPlayerTurn.
/// Re-reads the player's two top holdings (`AiTargetCorp` = slot 1, `pickId` = slot 2 or else slot 1);
/// an uncontrolled or toxic (asbestos/superfund) main holding becomes `pickId` and `AiTargetCorp = 0`.
/// With a single controlled holding nothing happens. If both are non-toxic and distinct, the player
/// holds over 30% of `pickId`, `LargestHolderPct > 50` and both have a credit rating above 1, the one
/// that holds the other merges it (MergeCompanies mode 1). Otherwise sells part of `pickId` (all of a
/// stake up to 20%, else 10..20%).
pub(crate) fn consolidate_top_holdings(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    pack_holding_arrays(g, ui);
    let p = f.player_id;
    f.pick_id = cast::<i32, _>(g.hold_stock_id[(2, p)]);
    f.target_pct = cast::<i32, _>(g.hold_stock_pct[(2, p)]);
    g.ai_target_corp = cast::<i32, _>(g.hold_stock_id[(1, p)]);
    if f.pick_id == 0 {
        f.pick_id = g.ai_target_corp;
    }
    g.voting_controller = 0;
    if 10 < g.ai_target_corp {
        let t = g.ai_target_corp;
        update_corp_control(g, ui, rng, t);
        calc_entity_financials(g, ui, rng, t);
        if is_toxic(g, g.ai_target_corp) || f.player_id != g.voting_controller {
            f.pick_id = g.ai_target_corp;
            g.ai_target_corp = 0;
        }
    }
    if f.pick_id == 0 {
        return;
    }
    if cast::<i32, _>(g.hold_stock_id[(1, p)]) == f.pick_id {
        f.target_pct = cast::<i32, _>(g.hold_stock_pct[(1, p)]);
    }
    if g.hold_stock_id[(0, p)] == 1 && f.player_id == g.voting_controller {
        return;
    }
    let t = g.ai_target_corp;
    if !is_toxic(g, f.pick_id)
        && t != f.pick_id
        && !is_toxic(g, t)
        && 30 < f.target_pct
        && 50 < g.largest_holder_pct
        && 10 < t
        && 1 < g.credit_rating_num[t]
        && 1 < g.credit_rating_num[f.pick_id]
    {
        f.acquirer_id = 0;
        f.target_id = 0;
        f.tmp_val = 0;
        if f.tmp_val < find_holding_slot(g, t, f.pick_id) && f.player_id == g.voting_controller {
            f.acquirer_id = t;
            f.target_id = f.pick_id;
        } else {
            f.tmp_val = 0;
            if f.tmp_val < find_holding_slot(g, f.pick_id, t) && 50 < g.largest_holder_pct {
                f.acquirer_id = f.pick_id;
                f.target_id = t;
            }
        }
        if 10 < f.acquirer_id {
            crate::logic::restructure::merge_companies(g, ui, rng, f.acquirer_id, f.target_id, 1, 0, f.time_secs);
            return;
        }
    }
    if 20 < f.target_pct {
        f.target_pct = to_long(rng.rnd() * Ext::from_f64(10.0) + Ext::from_f64(10.0));
    }
    g.tmp_ext1 = cast::<Ext, _>(f.player_id);
    g.tmp_ext2 = cast::<Ext, _>(f.target_pct);
    g.tmp_ext3 = Ext::from_f64(1.0);
    sell_stock_holding(g, ui, rng, f.pick_id);
    g.tmp_ext1 = Ext::from_f64(0.0);
}

/// `ABS(AsbestosCorp) = id OR SuperfundCorp = id`.
fn is_toxic(g: &Globals, id: i32) -> bool {
    (cast::<Ext, _>(g.asbestos_corp)).abs() == cast::<Ext, _>(id) || cast::<i32, _>(g.superfund_corp) == id
}

/// Port of AiPlayer_SellStockRaiseCash (0x5d12ad), GOSUB of AiComputerPlayerTurn.
/// For a main holding (`AiTargetCorp` in slot 1) above 25% that owes holding-company tax
/// (`HoldingCoTaxDue <> 0` after CalcCapitalTaxes): a stock offering of `AiTargetCorp` (`TmpExt1 = 20`,
/// IssueStock type 1) when the unoptioned stake exceeds 35% and it had no recent offering; a sale of 2%
/// when it still exceeds 25% (news "<PlayerName(1)> [runtime resource wording]  2% of stock of <sym>.",
/// see BUGS.md #278; sentiment +2); 1..5% (`PublicPct - 10`) of the float bought by an affordable
/// company holding under 15% ([`ai_find_affordable_buyer`] + [`validate_buyer_corp`]); finally writes
/// calls (AiCreateOptionPosition type 2) on up to 10% while keeping at least 25% unoptioned.
pub(crate) fn sell_stock_raise_cash(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    let p = f.player_id;
    if !(10 < g.ai_target_corp && cast::<i32, _>(g.hold_stock_id[(1, p)]) == g.ai_target_corp && 25 < g.hold_stock_pct[(1, p)]) {
        return;
    }
    pack_holding_arrays(g, ui);
    let t = g.ai_target_corp;
    calc_entity_financials(g, ui, rng, t);
    f.amount = calc_capital_taxes(g, t);
    if g.holding_co_tax_due == Ext::from_f64(0.0) {
        return;
    }
    f.target_pct = 5;
    f.other_id = get_optioned_stock_pct(g, ui, p, g.ai_target_corp, 0);
    if 35 < cast::<i32, _>(g.hold_stock_pct[(1, p)]) - f.other_id {
        f.tmp_val = 1;
        if is_recent_offering(g, g.ai_target_corp) != f.tmp_val {
            g.tmp_ext1 = Ext::from_f64(20.0);
            let t = g.ai_target_corp;
            issue_stock(g, ui, rng, t, 1);
            add_recent_offering(g, t, 1);
            g.turn_action_count += 1;
        }
    }
    f.other_id = get_optioned_stock_pct(g, ui, p, g.ai_target_corp, 0);
    if 25 < cast::<i32, _>(g.hold_stock_pct[(1, p)]) - f.other_id {
        f.target_pct = 2;
        g.tmp_ext1 = cast::<Ext, _>(f.player_id);
        g.tmp_ext2 = cast::<Ext, _>(f.target_pct);
        g.tmp_ext3 = Ext::from_f64(1.0);
        let t = g.ai_target_corp;
        sell_stock_holding(g, ui, rng, t);
        if Ext::from_f64(0.0) < g.tmp_ext1 {
            g.news_msg = g.player_name[1].clone()
                + __content.string(1908)
                + &str_s(cast::<Ext, _>(f.target_pct))
                + __content.string(1909)
                + &g.ent_symbol[g.ai_target_corp]
                + ".";
            post_news(g, ui, 0, 0);
            let t = g.ai_target_corp;
            g.investor_sentiment[t] += 2;
            g.tmp_ext1 = Ext::from_f64(0.0);
        }
    }
    f.target_pct = cast::<i32, _>(g.hold_stock_pct[(1, p)]);
    let t = g.ai_target_corp;
    update_corp_control(g, ui, rng, t);
    f.target_pct = cast::<i32, _>(g.public_pct) - 10;
    if 5 < f.target_pct {
        f.target_pct = rng.rnd_range(5, 1);
    }
    if 0 < f.target_pct {
        f.buyer_corp = ai_find_affordable_buyer(g, ui, rng, g.ai_target_corp, f.target_pct, 0);
        if 10 < f.buyer_corp && 0 < f.target_pct {
            f.bond_slot = find_stock_holding(g, f.buyer_corp, g.ai_target_corp);
            if g.hold_stock_pct[(f.bond_slot, f.buyer_corp)] < 15 {
                f.target_id = g.ai_target_corp;
                validate_buyer_corp(g, ui, rng, f);
                if f.buyer_corp != 0 {
                    g.public_pct = cast::<i16, _>(cast::<i32, _>(g.public_pct) - f.target_pct);
                }
            }
        }
    }
    f.defendant_id = cast::<i32, _>(g.hold_stock_pct[(1, p)]) - f.other_id;
    if f.defendant_id < 25 {
        return;
    }
    f.target_pct = f.defendant_id;
    if 10 < f.target_pct {
        f.target_pct = 10;
    }
    if (cast::<i32, _>(g.public_pct)) < f.target_pct {
        f.target_pct = cast::<i32, _>(g.public_pct);
    }
    if f.defendant_id - f.target_pct < 25 {
        f.target_pct = to_long(sf(0.5f32) * cast::<Ext, _>(f.target_pct));
    }
    if f.defendant_id - f.target_pct >= 25 {
        ai_create_option_position(g, ui, rng, f.player_id, g.ai_target_corp, f.target_pct, 2, 0);
    }
}

/// Port of AiPlayer_BuyEthereum (0x5d177e), GOSUB of AiComputerPlayerTurn.
/// Lets the player (and, if it trades nothing, `AiTargetCorp` above 130) trade physical commodities
/// (AiTradePhysCommod; a trade counts as a turn action). Otherwise a player with id above 2, after the
/// first two years, holding [runtime resource wording] , with crypto not banned (`ABS(CryptoBanState) <> 1`),
/// Ethereum below 4000 and a credit rating above 7 buys Ethereum (BuyPhysCommod, AI mode).
pub(crate) fn buy_ethereum(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    f.pick_id = ai_trade_phys_commod(g, ui, rng, f.player_id);
    if f.pick_id == 0 && 130 < g.ai_target_corp {
        let t = g.ai_target_corp;
        f.pick_id = ai_trade_phys_commod(g, ui, rng, t);
    }
    if f.pick_id != 0 {
        g.turn_action_count += 1;
        return;
    }
    if 2 < f.player_id
        && g.start_year + 2 < g.cur_year
        && get_phys_commod_value(g, f.player_id, &PbStr::new()) == Ext::from_f64(0.0)
        && (cast::<Ext, _>(g.crypto_ban_state)).abs() != Ext::from_f64(1.0)
        && g.ethereum_price < 4000
        && 7 < g.credit_rating_num[f.player_id]
    {
        g.cur_commodity = PbStr::from("ETHEREUM");
        buy_phys_commod(g, ui, rng, f.player_id, 1);
    }
}

/// Port of AiPlayer_EndGamePayDebt (0x5d18dd), GOSUB of AiComputerPlayerTurn.
/// From the year before the game ends: every company the player controls stops discretionary spending
/// (`DiscSpendPct = 0`); non-banks (86..1600) repay their bank debt from cash and T-bills
/// (`MIN(Cash + TBills, BankDebt)`, if at least 10) to their lending bank (`LenderId`).
pub(crate) fn end_game_pay_debt(g: &mut Globals, _ui: &mut dyn Ui, _rng: &mut Rng, f: &mut Frame) {
    if g.cur_year < g.game_end_year - 1 {
        return;
    }
    f.ent_id = 11;
    while f.ent_id <= 1600 {
        let e = f.ent_id;
        if g.entity_status[e] == 1 && f.player_id == g.corp_controller[e] {
            g.disc_spend_pct[e] = 0;
            if 85 < e {
                f.amount = g.t_bills[e] + g.cash[e];
                if f.amount > g.bank_debt[e] {
                    f.amount = g.bank_debt[e];
                }
                if f.amount >= Ext::from_f64(10.0) {
                    g.cash[e] = g.cash[e] - f.amount;
                    g.bank_debt[e] = g.bank_debt[e] - f.amount;
                    g.lender_id = cast::<i32, _>(g.lending_bank[e]);
                    let l = g.lender_id;
                    g.cash[l] = f.amount + g.cash[l];
                }
            }
        }
        f.ent_id += 1;
    }
}
