//! Quarter-end bookkeeping run from `SimProcessNextEntity` when a new quarter starts —
//! `PayAnnualWealthTax` (0x5082ae), `InsiderTradingInvestigation` (0x67fbfa), `ExpireAssetSaleOffers`
//! (0x6694c4), `GovBondMaturityAndAuction` (0x4e6b8c) — and the player-solvency helpers of the
//! `SimPlayer_*` GOSUBs: `RevokeExCeoOptions` (0x5fd937), `CallInPlayerAdvances` (0x5059ac),
//! `ClearInsiderTip` (0x67fb97), `CancelPendingMerger` (0x52962a), `PackHoldingArrays` (0x49117c), with
//! their small callees (`ConvertLongRateSwapsToShort`, `AddRemoveAssetSaleOffer`, `ReduceAssetSaleOffer`,
//! `CreditRatingAfterRepayment`, `RecaptureExcessLoss`).
use crate::pb::real::{Ext, cast};


pub use super::asset_sales::{add_remove_asset_sale_offer, reduce_asset_sale_offer};
use super::prelude::*;
use crate::logic::earnings::sell_t_bills_to_cash;
use crate::logic::economy::update_interest_rates;
use crate::logic::finance::{calc_capital_taxes, calc_entity_financials, get_controller_type};
use crate::logic::holdings::find_stock_holding;
use crate::logic::market::update_market_display;
use crate::logic::options::add_or_remove_option_rec;
use crate::logic::report::show_quick_message;
use crate::pb::numeric::{Cux, fix};

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

fn str_long(x: Ext) -> PbStr {
    PbStr::from(str_num(x, 15).as_str())
}

/// `RTRIM$(LTRIM$(FORMAT$(x, mask)))`.
fn fmt_trim(x: Ext, mask: &str) -> PbStr {
    trim(&format(x, mask.as_bytes()))
}

// ---------------------------------------------------------------------------------------------------------
// PayAnnualWealthTax
// ---------------------------------------------------------------------------------------------------------

/// Port of PayAnnualWealthTax (0x5082ae).
/// For every active player: recomputes the financials, charges `CalcCapitalTaxes` to cash and posts the
/// wealth tax / corporate shares tax (amounts for human players, "undisclosed" for computer players);
/// then refreshes the market display (`UpdateMarketDisplay`, which needs `time_secs`, see
/// [`update_market_display`]). A pending `NewsMsg` is posted first.
pub fn pay_annual_wealth_tax(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    let __content = crate::content::current();
    let default_fmt = "* ###,.00";
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    let n = g.num_players;
    let mut player_id = 1;
    while player_id <= n {
        if g.entity_status[player_id] == 1 {
            calc_entity_financials(g, ui, rng, player_id);
            let tax = calc_capital_taxes(g, player_id);
            g.cash[player_id] = g.cash[player_id] - tax;
            if get_controller_type(g, player_id) == 1 {
                if sf(0.01f32) < g.tmp_capital_tax {
                    let mut mask = default_fmt;
                    if Ext::from_f64(9999.0) < g.tmp_capital_tax {
                        mask = "* ###,";
                    }
                    g.news_msg = g.player_name[player_id].clone()
                        + __content.string(11113)
                        + &trim(g.currency_symbol.as_bytes())
                        + &fmt_trim(g.tmp_capital_tax, mask)
                        + " "
                        + &g.million_prefix_lc
                        + "il.";
                    if !g.news_msg.is_empty() {
                        post_news(g, ui, 0, 0);
                    }
                }
                if sf(0.01f32) < g.corp_shares_tax_due {
                    let mut mask = default_fmt;
                    if Ext::from_f64(9999.0) < g.corp_shares_tax_due {
                        mask = "* ###,";
                    }
                    g.news_msg = g.player_name[player_id].clone()
                        + __content.string(11114)
                        + &trim(g.currency_symbol.as_bytes())
                        + &fmt_trim(g.corp_shares_tax_due, mask)
                        + " "
                        + &g.million_prefix_lc
                        + "il.";
                    if !g.news_msg.is_empty() {
                        post_news(g, ui, 0, 0);
                    }
                }
            } else {
                if sf(0.01f32) < g.tmp_capital_tax {
                    g.news_msg = g.player_name[player_id].clone()
                        + __content.string(11115);
                }
                if !g.news_msg.is_empty() {
                    post_news(g, ui, 0, 0);
                }
                if sf(0.01f32) < g.corp_shares_tax_due {
                    g.news_msg = g.player_name[player_id].clone()
                        + __content.string(11116);
                }
                if !g.news_msg.is_empty() {
                    post_news(g, ui, 0, 0);
                }
            }
        }
        player_id += 1;
    }
    update_market_display(g, ui, rng, time_secs);
}

// ---------------------------------------------------------------------------------------------------------
// Insider trading
// ---------------------------------------------------------------------------------------------------------

/// Port of ClearInsiderTip (0x67fb97).
/// Forgets the current insider tip (earnings / merger tip corp, tippee, event tick and quarter, industry).
pub fn clear_insider_tip(g: &mut Globals) {
    g.insider_earn_tip_corp = 0;
    g.merger_tip_corp = 0;
    g.insider_tippee = 0;
    g.insider_tip_event_tick = 0;
    g.insider_tip_event_qtr = 0;
    g.tip_industry = 0;
}

/// Port of CancelPendingMerger (0x52962a).
/// Clears the pending merger (proposer, acquirer, target, price, `NextTakeoverTick`); if `entity_id`
/// (> 10) has been liquidated (status 2) a pop-up says the merger was canceled because of that.
pub fn cancel_pending_merger(g: &mut Globals, ui: &mut dyn Ui, entity_id: i32) {
    let __content = crate::content::current();
    let i = entity_id;
    if 10 < i && g.entity_status[i] == 2 {
        let mut msg = s(__content.string(11117))
            + &g.ent_name[g.merger_acquirer]
            + " and "
            + &g.ent_name[g.merger_target]
            + __content.string(11118)
            + &g.ent_name[i]
            + ".";
        show_quick_message(g, ui, &mut msg);
    }
    g.merger_proposer_player = 0;
    g.merger_acquirer = 0;
    g.merger_target = 0;
    g.merger_price_per_share = Cux(0);
    g.next_takeover_tick = 0;
}

/// Port of InsiderTradingInvestigation (0x67fbfa).
/// Quarterly check of the pending merger and the insider tips: cancels an incomplete merger proposal,
/// drops stale tips, plays out a merger tip (rumor news, sentiment +10, maybe a profit-margin boost for
/// the tipped player's target) or an earnings tip whose quarter has come (profit margin +/-5, tip
/// cleared); without a tip, advances the insider-trading investigations of the players (news, indictment
/// `TipsterState = -1`, `InsiderProbePlayer`). A resulting headline pops up (`ShowQuickMessage`) and is
/// posted.
///
/// An earnings tip whose event tick equals the quarter runs `InsiderEarnTipEvent`.
pub fn insider_trading_investigation(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    let mut player_id: i32;
    if 0 < g.merger_acquirer
        || 0 < g.merger_target
        || 0 < g.merger_proposer_player
        || 0 < g.next_takeover_tick
        || Ext::from_f64(0.0) < g.merger_price_per_share.get()
    {
        if g.next_takeover_tick < 50 {
            g.next_takeover_tick = 50;
        }
        if g.merger_acquirer == 0
            || g.merger_target == 0
            || g.merger_proposer_player == 0
            || g.merger_price_per_share.get() == Ext::from_f64(0.0)
        {
            cancel_pending_merger(g, ui, 0);
        }
    }
    if g.insider_tippee == 0 {
        if 0 < g.merger_tip_corp || g.insider_earn_tip_corp != 0 {
            clear_insider_tip(g);
        }
    } else if g.merger_tip_corp == 0 && g.insider_earn_tip_corp == 0 {
        clear_insider_tip(g);
    }
    if 10 < g.merger_tip_corp && g.entity_status[cast::<i32, _>(g.merger_tip_corp)] != 1 {
        clear_insider_tip(g);
    }
    if Ext::from_f64(10.0) < (cast::<Ext, _>(g.insider_earn_tip_corp)).abs() {
        let abs_tip_corp_id = to_quad((cast::<Ext, _>(g.insider_earn_tip_corp)).abs());
        if g.entity_status[cast::<i32, _>(abs_tip_corp_id)] != 1 {
            clear_insider_tip(g);
        }
    }
    if 10 < g.merger_tip_corp {
        let tip = cast::<i32, _>(g.merger_tip_corp);
        if tip == g.merger_target {
            g.insider_tip_event_tick = 0;
        } else {
            if 0 < g.insider_tippee && g.insider_tip_event_tick < 50 {
                g.insider_tip_event_tick = to_integer(rng.rnd() * Ext::from_f64(100.0) + Ext::from_f64(50.0));
            }
            if g.industry_num[tip] < 70
                && cast::<i32, _>(g.insider_tippee) == g.cur_player
                && Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0)
            {
                let cmp_arg = cast::<i32, _>(g.profit_margin[tip]) + 2;
                let r = rng.rnd_range(8, 1);
                g.profit_margin[tip] = cast::<i16, _>(r + cmp_arg);
            }
            if cast::<i32, _>(g.insider_tippee) == g.cur_player {
                g.investor_sentiment[tip] = g.investor_sentiment[tip] + 10;
                g.news_msg = s(__content.string(11119))
                    + &g.ent_symbol[tip]
                    + __content.string(11120);
                if 0 < g.insider_tippee && g.insider_tippee < 11 {
                    player_id = cast::<i32, _>(g.insider_tippee);
                } else {
                    player_id = 0;
                }
                let tip_corp_id = tip;
                if 0 < player_id {
                    let stake = super::asset_sales::calc_group_stake_pct(g, player_id, tip_corp_id);
                    if 0 < stake || Ext::from_f64(0.0) < g.tmp_ext1 {
                        if g.cur_quarter < 3 && player_id != g.corp_controller[tip_corp_id] {
                            g.news_msg = s("Raider ")
                                + &g.ent_name[player_id]
                                + __content.string(11121)
                                + &g.ent_symbol[tip_corp_id]
                                + __content.string(11122);
                        }
                        if g.cur_quarter == 3 {
                            g.news_msg = s(__content.string(11123))
                                + &g.ent_name[player_id]
                                + __content.string(11124)
                                + &g.ent_symbol[tip_corp_id]
                                + __content.string(11125);
                        }
                        if g.cur_quarter == 4 {
                            g.news_msg = s(__content.string(11126))
                                + &g.ent_symbol[tip_corp_id]
                                + __content.string(11127)
                                + &g.ent_name[player_id];
                        }
                    } else if g.cur_quarter == 1 {
                        g.news_msg = s(__content.string(11128))
                            + &g.ent_name[player_id]
                            + __content.string(11129)
                            + &g.ent_symbol[tip_corp_id];
                    } else if g.cur_quarter == 2 {
                        g.news_msg = s(__content.string(4553))
                            + &g.ent_symbol[tip_corp_id]
                            + __content.string(11130);
                    }
                    g.tmp_ext1 = Ext::from_f64(0.0);
                }
            }
        }
    } else if Ext::from_f64(10.0) < (cast::<Ext, _>(g.insider_earn_tip_corp)).abs()
        && cast::<i32, _>(g.insider_tip_event_qtr) == g.cur_quarter
    {
        player_id = to_long((cast::<Ext, _>(g.insider_earn_tip_corp)).abs());
        if 10 < g.insider_earn_tip_corp
            && g.entity_status[player_id] == 1
            && g.profit_margin[player_id] < 8
        {
            g.profit_margin[player_id] = g.profit_margin[player_id] + 5;
        }
        if g.insider_earn_tip_corp < -10
            && g.entity_status[player_id] == 1
            && -8 < g.profit_margin[player_id]
        {
            g.profit_margin[player_id] = g.profit_margin[player_id] - 5;
        }
        clear_insider_tip(g);
    } else if Ext::from_f64(10.0) < (cast::<Ext, _>(g.insider_earn_tip_corp)).abs()
        && cast::<i32, _>(g.insider_tip_event_tick) == g.cur_quarter
    {
        crate::logic::info_screens::insider::insider_earn_tip_event(g, ui, rng);
    } else if g.insider_tippee == 0 {
        let mut agency_name = s(__content.string(2879));
        if g.currency_code == "USA" {
            agency_name = s("S.E.C.");
        }
        let n = g.num_players;
        player_id = 1;
        while player_id <= n {
            let p = player_id;
            if g.player_name[p] == "...OUT..." {
                g.tipster_state[p] = 0;
            } else {
                if g.tipster_state[p] == -1
                    && p == g.insider_probe_player
                    && (g.cur_quarter + g.cur_year) % 2 == 1
                {
                    g.news_msg =
                        s(__content.string(11131)) + &g.ent_name[p] + __content.string(11132);
                    break;
                }
                if g.tipster_state[p] == 4 || p == g.insider_probe_player {
                    if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(3.0) || p == g.insider_probe_player {
                        if (g.cur_quarter + g.cur_year + p) % 3 == 1 {
                            g.news_msg = s(__content.string(11133))
                                + &g.ent_name[p]
                                + __content.string(11132);
                            if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(3.0) {
                                g.news_msg = replace(
                                    g.news_msg.as_bytes(),
                                    __content.bytes(11134),
                                    agency_name.as_bytes(),
                                );
                            }
                            if Ext::from_f64(4.0) < rng.rnd() * Ext::from_f64(5.0) {
                                g.news_msg =
                                    replace(g.news_msg.as_bytes(), __content.bytes(11135), __content.bytes(11136));
                            }
                            if (g.cur_quarter + g.cur_year) % 3 == 1 {
                                g.news_msg = s(__content.string(11137))
                                    + &g.ent_name[p]
                                    + __content.string(11138);
                                if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                                    g.news_msg = replace(
                                        g.news_msg.as_bytes(),
                                        __content.bytes(11139),
                                        __content.bytes(11140),
                                    );
                                }
                            }
                        }
                    } else if g.tipster_state[p] == 4 {
                        g.tipster_state[p] = -1;
                        g.news_msg = agency_name.clone()
                            + __content.string(11141)
                            + &g.ent_name[p]
                            + __content.string(11142);
                        if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) && g.insider_probe_player == 0 {
                            g.insider_probe_player = p;
                            g.news_msg = g.ent_name[p].clone()
                                + __content.string(11143);
                        }
                        if g.insider_probe_player == 0 && Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(3.0) {
                            g.news_msg = replace(
                                g.news_msg.as_bytes(),
                                __content.bytes(11144),
                                __content.bytes(11145),
                            );
                        }
                    }
                }
                if !g.news_msg.is_empty() {
                    break;
                }
            }
            player_id += 1;
        }
    }
    if !g.news_msg.is_empty() {
        let mut msg = std::mem::take(&mut g.news_msg);
        show_quick_message(g, ui, &mut msg);
        g.news_msg = msg;
        post_news(g, ui, 0, 0);
    }
}

// ---------------------------------------------------------------------------------------------------------
// Asset sale offers
// ---------------------------------------------------------------------------------------------------------

/// Port of ExpireAssetSaleOffers (0x6694c4).
/// Deletes asset-sale offers that expired (year, quarter), whose seller is gone or whose stock is no
/// longer held/active (clearing `AutoPilotRejectedTarget` when it pointed at the deleted offer's
/// stock), trims offers that exceed what the seller still has (`ReduceAssetSaleOffer`), then packs all
/// record arrays (`PackHoldingArrays`).
pub fn expire_asset_sale_offers(g: &mut Globals, ui: &mut dyn Ui) {
    for offer_idx in 1..=100 {
        if g.asset_sale_offers[offer_idx].is_empty() {
            continue;
        }
        let offer_text = g.asset_sale_offers[offer_idx].clone();
        let t = offer_text.as_bytes();
        let exp_year = to_long(val(parse_any(t, b"|", 6).as_bytes()));
        if exp_year < g.cur_year {
            g.asset_sale_offers[offer_idx] = PbStr::new();
        }
        if exp_year == g.cur_year {
            let exp_qtr = to_long(val(parse_any(t, b"|", 5).as_bytes()));
            if exp_qtr <= g.cur_quarter {
                g.asset_sale_offers[offer_idx] = PbStr::new();
            }
        }
        if g.asset_sale_offers[offer_idx].is_empty() {
            if val(parse_any(t, b"|", 2).as_bytes()) == cast::<Ext, _>(g.auto_pilot_rejected_target) {
                g.auto_pilot_rejected_target = 0;
            }
            continue;
        }
        let seller_id = to_long(val(parse_any(t, b"|", 1).as_bytes()));
        if g.entity_status[seller_id] != 1 {
            g.asset_sale_offers[offer_idx] = PbStr::new();
            continue;
        }
        if parse_any(t, b"|", 2) == "CA" {
            if !(Ext::from_f64(0.0) < g.business_assets[seller_id]) {
                g.asset_sale_offers[offer_idx] = PbStr::new();
                continue;
            }
            let offered_amt = val(parse_any(t, b"|", 3).as_bytes());
            if fix(g.business_assets[seller_id]) < offered_amt {
                let mut excess_amt = fix(g.business_assets[seller_id]);
                excess_amt = offered_amt - excess_amt;
                if sf(0.5f32) < excess_amt {
                    let reduce_amt = to_quad(excess_amt);
                    reduce_asset_sale_offer(g, ui, offer_idx, 0, reduce_amt);
                }
            }
        } else {
            let stock_corp_id = to_long(val(parse_any(t, b"|", 2).as_bytes()));
            if g.entity_status[stock_corp_id] != 1 {
                g.asset_sale_offers[offer_idx] = PbStr::new();
                continue;
            }
            let hold_slot = find_stock_holding(g, seller_id, stock_corp_id);
            if hold_slot < 1 {
                g.asset_sale_offers[offer_idx] = PbStr::new();
            } else {
                let held_pct = cast::<i32, _>(g.hold_stock_pct[(hold_slot, seller_id)]);
                let excess_pct = to_long(val(parse_any(t, b"|", 3).as_bytes()) - cast::<Ext, _>(held_pct));
                if 0 < excess_pct {
                    reduce_asset_sale_offer(g, ui, offer_idx, excess_pct, 0);
                }
            }
        }
    }
    pack_holding_arrays(g, ui);
}

// ---------------------------------------------------------------------------------------------------------
// Government bonds
// ---------------------------------------------------------------------------------------------------------

/// Port of ConvertLongRateSwapsToShort (0x646652).
/// Rewrites every swap contract on the long-bond rate (`|L|`) to the short rate (`|S|`).
pub fn convert_long_rate_swaps_to_short(g: &mut Globals) {
    for slot in 1..=1000 {
        let swap_rec = g.swap_contracts[slot].clone();
        if 0 < instr(1, swap_rec.as_bytes(), b"|L|") {
            g.swap_contracts[slot] = replace(g.swap_contracts[slot].as_bytes(), b"|L|", b"|S|");
        }
    }
}

/// Port of GovBondMaturityAndAuction (0x4e6b8c).
/// In the year the short government note (`ShortBondDesc`) matures: pays it off at par to every holder
/// (banks, insurers, players 1..130 and funds 1581..1583; the gain over cost goes to `PlayerCapGains`
/// for ids < 6, else `AccruedPL`), the long bond becomes the short note (holdings and costs move over),
/// long-rate swaps become short-rate swaps, the interest rates are updated (`UpdateInterestRates`) and a
/// new 20-year long bond is auctioned at 100 with a coupon of `LongBondYield` rounded down to 1/4 %.
pub fn gov_bond_maturity_and_auction(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError("TBondPayoff") (error path not ported)
    if val(mid(g.short_bond_desc.as_bytes(), 1, 4).as_bytes()) != cast::<Ext, _>(g.cur_year) {
        return;
    }
    for e in 1..=1583 {
        if 130 < e && e < 1581 {
            continue;
        }
        if g.short_gov_bond_face[e] == 0 {
            continue;
        }
        let face_amt = g.short_gov_bond_face[e];
        g.cash[e] = cast::<Ext, _>(face_amt) + g.cash[e];
        g.short_gov_bond_face[e] = 0;
        let gain = cast::<Ext, _>(face_amt) - g.st_gov_bond_cost[e];
        if e < 6 {
            g.player_cap_gains[e] = gain + g.player_cap_gains[e];
        } else {
            g.accrued_pl[e] = gain + g.accrued_pl[e];
        }
        g.st_gov_bond_cost[e] = Ext::from_f64(0.0);
    }
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    g.news_msg = s(__content.string(11146))
        + &str_long(cast::<Ext, _>(g.cur_year))
        + __content.string(11147);
    post_news(g, ui, 0, 0);
    g.short_bond_desc = g.long_bond_desc.clone();
    g.short_bond_price = cast::<f32, _>(g.long_bond_price);
    for e in 1..=1583 {
        if 130 < e && e < 1581 {
            continue;
        }
        if g.entity_status[e] == 1 && g.long_gov_bond_face[e] != 0 {
            g.short_gov_bond_face[e] = g.long_gov_bond_face[e];
            g.long_gov_bond_face[e] = 0;
            g.st_gov_bond_cost[e] = g.lt_gov_bond_cost[e];
            g.lt_gov_bond_cost[e] = Ext::from_f64(0.0);
        }
    }
    convert_long_rate_swaps_to_short(g);
    update_interest_rates(g, rng);
    let mut coupon_rate: f32 = cast::<f32, _>(cast::<f32, _>(int(cast::<Ext, _>(g.long_bond_yield) * Ext::from_f64(4.0))));
    coupon_rate = cast::<f32, _>(sf(0.25f32) * cast::<Ext, _>(coupon_rate));
    let face_amt: i64 = to_quad(Ext::from_f64(20.0) + cast::<Ext, _>(g.cur_year));
    let mut rate_text = trim(str_long(cast::<Ext, _>(face_amt)).as_bytes());
    g.long_bond_desc = rate_text.clone() + "@";
    rate_text = PbStr::from(format(cast::<Ext, _>(coupon_rate), b"0#.00"));
    g.long_bond_desc = g.long_bond_desc.clone() + &rate_text;
    g.long_bond_price = cast::<f32, _>(Ext::from_f32(100.0f32));
    if val(rate_text.as_bytes()) < Ext::from_f64(10.0) {
        rate_text = right(rate_text.as_bytes(), 4);
    }
    g.news_msg = s(__content.string(11148))
        + &rate_text
        + __content.string(11149)
        + &str_long(cast::<Ext, _>(face_amt))
        + ".";
    post_news(g, ui, 0, 0);
}

// ---------------------------------------------------------------------------------------------------------
// Player solvency helpers
// ---------------------------------------------------------------------------------------------------------

/// Port of RevokeExCeoOptions (0x5fd937).
/// For every player: deletes the unvested executive options (`GRANT` records held by the player) on
/// companies the player is no longer CEO of and does not control, with a news headline each. A pending
/// `NewsMsg` is posted first.
pub fn revoke_ex_ceo_options(g: &mut Globals, ui: &mut dyn Ui) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    let n = g.num_players;
    let mut i = 1;
    while i <= n {
        let player_id = i;
        for rec_idx in 1..=1000 {
            if g.option_recs[rec_idx].is_empty()
                || instr(1, g.option_recs[rec_idx].as_bytes(), b"GRANT") == 0
            {
                continue;
            }
            if val(parse_any(g.option_recs[rec_idx].as_bytes(), b"|", 3).as_bytes())
                != cast::<Ext, _>(player_id)
            {
                continue;
            }
            let opt_rec = trim(g.option_recs[rec_idx].as_bytes());
            if right(opt_rec.as_bytes(), 5) != "GRANT" {
                continue;
            }
            let corp_id = to_long(val(
                parse_any(g.option_recs[rec_idx].as_bytes(), b"|", 1).as_bytes()
            ));
            if corp_id != g.player_ceo_of[player_id] && player_id != g.corp_controller[corp_id] {
                g.news_msg = g.ent_symbol[corp_id].clone()
                    + __content.string(11150)
                    + &g.ent_symbol[player_id];
                add_or_remove_option_rec(g, ui, &PbStr::new(), rec_idx);
                post_news(g, ui, corp_id, player_id);
            }
        }
        i += 1;
    }
}

/// Port of CreditRatingAfterRepayment (0x5c2e8f).
/// The credit rating (`CreditRatingStr`) borrower `borrower_id` would have after repaying `amount` of
/// the advances of `lender_id`: computed by `CalcEntityFinancials` on the reduced advance and cash, which
/// are restored afterwards (the other results of `CalcEntityFinancials`, e.g. `CreditRatingNum`, stay).
pub fn credit_rating_after_repayment(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    lender_id: i32,
    borrower_id: i32,
    amount: Ext,
) -> PbStr {
    let borrower = borrower_id;
    let saved_advance = g.player_advances[(lender_id, borrower)];
    g.player_advances[(lender_id, borrower)] = g.player_advances[(lender_id, borrower)] - amount;
    let saved_cash = g.cash[borrower];
    g.cash[borrower] = g.cash[borrower] - amount;
    calc_entity_financials(g, ui, rng, borrower);
    g.player_advances[(lender_id, borrower)] = saved_advance;
    g.cash[borrower] = saved_cash;
    g.credit_rating_str.clone()
}

/// Port of CallInPlayerAdvances (0x5059ac).
/// Player `player_id` is short of cash: walks the companies the player has advanced money to. A company
/// rated below 6 (BB) is skipped in `mode = 1` unless it has no bank debt and no bonds, is an operating
/// company and its assets cover a tenth of the amount; in other modes the advances to it are written off
/// in 10 % steps while it stays below 6 (the write-off lowers its tax-loss carryforward, is booked as an
/// extraordinary item and against the player's capital gains; the company uses its liquid funds to pay
/// down bank debt), with a news headline and a pop-up of the total. Then advances to a company rated 6+
/// are called in (up to the cash shortfall; the whole advance if within 1 %), with news and (for human
/// players, else 1 in 10) a pop-up, until the player's cash is no longer negative.
///
/// Quirk: the pop-up text of a write-off is never reset, so it is shown again for every later company
/// rated below 6.
pub fn call_in_player_advances(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    player_id: i32,
    mode: i32,
) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let p = player_id;
    let mut write_off_msg = PbStr::new();
    let mut _msg_text: PbStr;
    calc_entity_financials(g, ui, rng, p);
    if g.tmp_player_advances == Ext::from_f64(0.0) {
        return;
    }
    for corp_idx in 11..=1600 {
        let corp_id = corp_idx;
        if g.entity_status[corp_id] != 1 {
            continue;
        }
        if !(g.player_advances[(p, corp_id)] > Ext::from_f64(0.0)) {
            continue;
        }
        let cash_needed = g.cash[p].abs();
        let mut repay_amt = cash_needed;
        if repay_amt > g.player_advances[(p, corp_id)] {
            repay_amt = g.player_advances[(p, corp_id)];
        }
        _msg_text = credit_rating_after_repayment(g, ui, rng, p, corp_id, repay_amt);
        let mut call_in = true;
        if g.credit_rating_num[corp_id] < 6 {
            if mode == 1 {
                if g.bank_debt[corp_id] > Ext::from_f64(0.0)
                    || cast::<Ext, _>(g.bonds_issued[corp_id]) > Ext::from_f64(0.0)
                    || corp_id < 131
                    || !(g.total_assets >= repay_amt * sf(0.1f32))
                {
                    call_in = false;
                }
            } else {
                let mut write_off_chunk = g.player_advances[(p, corp_id)] * sf(0.1f32);
                let orig_advance = g.player_advances[(p, corp_id)];
                while g.player_advances[(p, corp_id)] > Ext::from_f64(0.0) && g.credit_rating_num[corp_id] < 6 {
                    if g.player_advances[(p, corp_id)] < orig_advance * sf(0.11f32) {
                        write_off_chunk = sf(0.1f32) * g.player_advances[(p, corp_id)];
                    }
                    if g.player_advances[(p, corp_id)] < orig_advance * sf(0.01f32) {
                        write_off_chunk = g.player_advances[(p, corp_id)];
                    }
                    g.player_advances[(p, corp_id)] =
                        g.player_advances[(p, corp_id)] - write_off_chunk;
                    if g.tax_loss_carry[corp_id] < Ext::from_f64(0.0) {
                        g.tax_loss_carry[corp_id] = write_off_chunk + g.tax_loss_carry[corp_id];
                        if g.tax_loss_carry[corp_id] > Ext::from_f64(0.0) {
                            g.tax_loss_carry[corp_id] = Ext::from_f64(0.0);
                        }
                    }
                    g.extra_items_ytd[corp_id] = write_off_chunk + g.extra_items_ytd[corp_id];
                    g.player_cap_gains[p] = g.player_cap_gains[p] - write_off_chunk;
                    if g.t_bills[corp_id] + g.cash[corp_id] > Ext::from_f64(0.0) && g.bank_debt[corp_id] > Ext::from_f64(0.0) {
                        let mut debt_paydown = g.bank_debt[corp_id];
                        if debt_paydown > g.t_bills[corp_id] + g.cash[corp_id] {
                            debt_paydown = g.t_bills[corp_id] + g.cash[corp_id];
                        }
                        g.cash[corp_id] = g.cash[corp_id] - debt_paydown;
                        g.bank_debt[corp_id] = g.bank_debt[corp_id] - debt_paydown;
                        if g.cash[corp_id] < Ext::from_f64(0.0) {
                            let need = g.cash[corp_id].abs();
                            sell_t_bills_to_cash(g, corp_id, need);
                        }
                        if 11 < corp_id {
                            let bank = cast::<i32, _>(g.lending_bank[corp_id]);
                            g.cash[bank] = debt_paydown + g.cash[bank];
                        }
                    }
                    g.news_msg = g.ent_symbol[p].clone()
                        + __content.string(11151)
                        + &g.ent_symbol[corp_id];
                    write_off_msg = g.news_msg.clone()
                        + __content.string(3431)
                        + &trim(g.currency_symbol.as_bytes())
                        + &fmt_trim(orig_advance - g.player_advances[(p, corp_id)], "* ###,.00")
                        + " "
                        + &g.million_prefix_lc
                        + "illion"
                        + &g.currency_suffix;
                    repay_amt = cash_needed;
                    if repay_amt > g.player_advances[(p, corp_id)] {
                        repay_amt = g.player_advances[(p, corp_id)];
                    }
                    if g.player_advances[(p, corp_id)] > Ext::from_f64(0.0) {
                        _msg_text =
                            credit_rating_after_repayment(g, ui, rng, p, corp_id, repay_amt);
                    }
                }
                if !g.news_msg.is_empty() {
                    post_news(g, ui, 0, 0);
                }
                if !write_off_msg.is_empty() {
                    show_quick_message(g, ui, &mut write_off_msg);
                }
            }
        }
        if !call_in {
            continue;
        }
        if 5 < g.credit_rating_num[corp_id] && g.player_advances[(p, corp_id)] > Ext::from_f64(0.0) {
            repay_amt = cash_needed;
            if repay_amt > g.player_advances[(p, corp_id)] * sf(0.99f32) {
                repay_amt = g.player_advances[(p, corp_id)];
            }
            g.cash[p] = repay_amt + g.cash[p];
            g.cash[corp_id] = g.cash[corp_id] - repay_amt;
            g.player_advances[(p, corp_id)] = g.player_advances[(p, corp_id)] - repay_amt;
            g.news_msg = g.ent_symbol[p].clone()
                + __content.string(11152)
                + &g.ent_symbol[corp_id];
            let mut msg_text = g.news_msg.clone()
                + __content.string(3431)
                + &trim(g.currency_symbol.as_bytes())
                + &fmt_trim(repay_amt, "* #################.00")
                + " "
                + &g.million_prefix_lc
                + "illion"
                + &g.currency_suffix;
            if g.is_computer_player[p] == 0 || Ext::from_f64(9.0) < rng.rnd() * Ext::from_f64(10.0) {
                show_quick_message(g, ui, &mut msg_text);
            }
            post_news(g, ui, 0, 0);
            if !(g.cash[p] < Ext::from_f64(0.0)) {
                return;
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------
// PackHoldingArrays
// ---------------------------------------------------------------------------------------------------------

/// Port of RecaptureExcessLoss (0x499b0b).
/// When a holding of `corp_id` with a negative basis (excess loss account) falls below 80 %, the
/// negative basis is recaptured as extraordinary (taxable) income of the holder, with news if > 1.
/// `holder_id` 1..NumPlayers: nothing; another id < 11: all holders 11..1600 are checked; else
/// holders `holder_id` and `holder_id + 1` (capped at 1600). Stops after the first recapture. Bases in
/// (-0.001, 0) are zeroed.
pub fn recapture_excess_loss(g: &mut Globals, ui: &mut dyn Ui, corp_id: i32, holder_id: i32) {
    let __content = crate::content::current();
    let mut first_id = 11;
    let mut last_id = 1600;
    if holder_id < 11 {
        if 0 < holder_id && holder_id <= g.num_players {
            return;
        }
    } else {
        first_id = holder_id;
        last_id = holder_id + 1;
        if 1600 < last_id {
            last_id = 1600;
        }
    }
    let mut owner_id = first_id;
    while owner_id <= last_id {
        if g.entity_status[owner_id] == 1 {
            let n = cast::<i32, _>(g.hold_stock_id[(0, owner_id)]);
            let mut slot = 1;
            while slot <= n {
                if cast::<i32, _>(g.hold_stock_id[(slot, owner_id)]) == corp_id {
                    if g.hold_stock_basis[(slot, owner_id)] < Ext::from_f64(0.0)
                        && sf(-0.001f32) < g.hold_stock_basis[(slot, owner_id)]
                    {
                        g.hold_stock_basis[(slot, owner_id)] = Ext::from_f64(0.0);
                    }
                    if g.hold_stock_pct[(slot, owner_id)] < 80
                        && g.hold_stock_basis[(slot, owner_id)] <= sf(-0.001f32)
                    {
                        let x = g.hold_stock_basis[(slot, owner_id)].abs();
                        g.hold_stock_basis[(slot, owner_id)] = Ext::from_f64(0.0);
                        g.extraordinary_pl[owner_id] =
                            to_quad(cast::<Ext, _>(g.extraordinary_pl[owner_id]) + x);
                        if Ext::from_f64(1.0) < x {
                            g.news_msg = s(__content.string(5698))
                                + &g.ent_symbol[owner_id]
                                + __content.string(5699)
                                + &g.ent_symbol[corp_id]
                                + __content.string(5700);
                            post_news(g, ui, owner_id, corp_id);
                            g.news_msg = g.ent_symbol[owner_id].clone()
                                + __content.string(5701)
                                + &trim(g.currency_symbol.as_bytes())
                                + &fmt_trim(x, "* ###,")
                                + " "
                                + &g.million_prefix_lc
                                + "illion"
                                + &g.currency_suffix;
                            post_news(g, ui, owner_id, corp_id);
                        }
                        owner_id = 1600;
                    }
                }
                slot += 1;
            }
        }
        owner_id += 1;
    }
}

/// Packs a 1-based record array (`AssetSaleOffers`, `OptionRecs`, …): moves the last record into the
/// first hole until none is left; element 0 gets the count (or `max` when full).
fn pack_records(recs: &mut crate::pb::array::Arr1<PbStr>, max: i32) {
    loop {
        let mut last_used = 0;
        let mut first_empty = 0;
        for slot in 1..=max {
            if first_empty == 0 && recs[slot].is_empty() {
                first_empty = slot;
            }
            if !recs[slot].is_empty() {
                last_used = slot;
            }
        }
        if last_used == 0 {
            recs[0] = s("0");
            return;
        } else if first_empty == 0 {
            recs[0] = trim(str_long(cast::<Ext, _>(max)).as_bytes());
            return;
        } else if 0 < last_used && last_used < first_empty {
            recs[0] = trim(str_long(cast::<Ext, _>(last_used)).as_bytes());
            return;
        } else if first_empty < last_used {
            recs[first_empty] = recs[last_used].clone();
            recs[last_used] = PbStr::new();
        }
    }
}

/// Port of PackHoldingArrays (0x49117c).
/// Compacts the stock holdings of every active entity (slot 0 = number of used slots; free slots get pct
/// and basis 0; a moved holding with a negative basis may be recaptured, `RecaptureExcessLoss`), the bond
/// holdings of active banks, insurers, players and funds, and the record lists `OptionRecs`,
/// `FuturesRecs`, `SwapContracts`, `PhysCommodRec` (1..1000) and `AssetSaleOffers`, `PriceAlerts`
/// (1..100), whose element 0 gets the count.
///
/// The "Unanticipated result" MSGBOX branches of the original (first free slot equal to the last used
/// one) cannot be reached; the original would loop forever there, the port stops.
pub fn pack_holding_arrays(g: &mut Globals, ui: &mut dyn Ui) {
    let __content = crate::content::current();
    for entity_id in 1..=1600 {
        if g.entity_status[entity_id] != 1 {
            continue;
        }
        g.hold_stock_id[(0, entity_id)] = 0;
        loop {
            let mut last_used = 0;
            let mut first_empty = 0;
            for slot in 1..=15 {
                if first_empty == 0 && g.hold_stock_id[(slot, entity_id)] == 0 {
                    first_empty = slot;
                }
                if g.hold_stock_id[(slot, entity_id)] != 0 {
                    last_used = slot;
                }
                if g.hold_stock_id[(slot, entity_id)] == 0 {
                    g.hold_stock_pct[(slot, entity_id)] = 0;
                    g.hold_stock_basis[(slot, entity_id)] = Ext::from_f64(0.0);
                }
            }
            if last_used == 0 {
                break;
            } else if first_empty == 0 {
                g.hold_stock_id[(0, entity_id)] = 15;
                break;
            } else if 0 < last_used && last_used < first_empty {
                g.hold_stock_id[(0, entity_id)] = cast::<i16, _>(last_used);
                break;
            } else if first_empty < last_used {
                let dest = first_empty;
                g.hold_stock_id[(dest, entity_id)] = g.hold_stock_id[(last_used, entity_id)];
                g.hold_stock_id[(last_used, entity_id)] = 0;
                g.hold_stock_pct[(dest, entity_id)] = g.hold_stock_pct[(last_used, entity_id)];
                g.hold_stock_pct[(last_used, entity_id)] = 0;
                g.hold_stock_basis[(dest, entity_id)] = g.hold_stock_basis[(last_used, entity_id)];
                g.hold_stock_basis[(last_used, entity_id)] = Ext::from_f64(0.0);
                if g.hold_stock_basis[(dest, entity_id)] < Ext::from_f64(0.0) {
                    let held_corp_id = cast::<i32, _>(g.hold_stock_id[(dest, entity_id)]);
                    recapture_excess_loss(g, ui, held_corp_id, entity_id);
                }
            } else {
                ui.msgbox(
                    &s(__content.string(11153)),
                    0,
                    &PbStr::new(),
                );
                break;
            }
        }
    }
    for entity_id in 1..=1583 {
        if (130 < entity_id && entity_id < 1581) || g.entity_status[entity_id] != 1 {
            continue;
        }
        g.hold_bond_id[(0, entity_id)] = 0;
        loop {
            let mut last_used = 0;
            let mut first_empty = 0;
            for slot in 1..=15 {
                if first_empty == 0 && g.hold_bond_id[(slot, entity_id)] == 0 {
                    first_empty = slot;
                }
                if 0 < g.hold_bond_id[(slot, entity_id)] {
                    last_used = slot;
                }
                if g.hold_bond_id[(slot, entity_id)] == 0 {
                    g.hold_bond_face[(slot, entity_id)] = 0;
                    g.bond_hold_cost[(slot, entity_id)] = Ext::from_f64(0.0);
                }
            }
            if last_used == 0 {
                break;
            } else if first_empty == 0 {
                g.hold_bond_id[(0, entity_id)] = 15;
                break;
            } else if 0 < last_used && last_used < first_empty {
                g.hold_bond_id[(0, entity_id)] = last_used;
                break;
            } else if first_empty < last_used {
                let dest = first_empty;
                g.hold_bond_id[(dest, entity_id)] = g.hold_bond_id[(last_used, entity_id)];
                g.hold_bond_id[(last_used, entity_id)] = 0;
                g.hold_bond_face[(dest, entity_id)] = g.hold_bond_face[(last_used, entity_id)];
                g.hold_bond_face[(last_used, entity_id)] = 0;
                g.bond_hold_cost[(dest, entity_id)] = g.bond_hold_cost[(last_used, entity_id)];
                g.bond_hold_cost[(last_used, entity_id)] = Ext::from_f64(0.0);
            } else {
                ui.msgbox(
                    &s(__content.string(11154)),
                    0,
                    &PbStr::new(),
                );
                break;
            }
        }
    }
    pack_records(&mut g.option_recs, 1000);
    pack_records(&mut g.futures_recs, 1000);
    pack_records(&mut g.swap_contracts, 1000);
    pack_records(&mut g.phys_commod_rec, 1000);
    pack_records(&mut g.asset_sale_offers, 100);
    pack_records(&mut g.price_alerts, 100);
}
