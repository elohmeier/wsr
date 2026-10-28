//! Forced and AI sales: `SellStockHolding` (0x4ed93b), `ForcedSellStocks` (0x4ff864) with
//! `RecaptureExcessLoss` (0x499b0b), `ForcedSellBonds` (0x500723), `CoverCashDeficit` (0x6625d7),
//! `LiquidateFuturesPositions` (0x62e730) and `EtfDropOneAdviserContract` (0x64e6ba).
//!
//! SINGLE pool constants are written `sf(..)`; every float → integer store uses the x87 rounding helpers.

// Keep the original's expression shapes (`x * -1`, nested IFs inside SELECT CASE).
#![allow(clippy::neg_multiply, clippy::collapsible_match)]
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::prelude::*;

use super::corp_ops::draw_credit_line_for_cash;
use crate::logic::cashflow::get_etf_adviser_human_status;
use crate::logic::commod_ai::limits::calc_commod_trade_limit;
use crate::logic::commod_ai::physical::deliver_phys_commod_for_short_futures;
use crate::logic::control::update_corp_control;
use crate::logic::earnings::{
    balance_cash_t_bills, calc_accrued_bond_interest, calc_cur_month_of_year,
    calc_projected_earnings,
};
use crate::logic::finance::{
    calc_entity_financials, get_controller_type, get_managing_player, get_phys_commod_value,
};
use crate::logic::futures::{
    add_or_remove_futures_rec, calc_futures_contract_value, get_futures_price,
    get_commod_data, recalc_commod_margin_balances, sum_commodity_holdings,
};
use crate::logic::holdings::{pack_holding_arrays, recapture_excess_loss};
use crate::logic::news::is_in_recent_headlines;
use crate::logic::reports_corp::assign_etf_adviser;
use crate::logic::reports_lists::list_phys_commod_holdings;
use crate::logic::reports_lists::update_control_chain;
use crate::logic::valuation::{calc_stock_price, update_price_history_tick};
use crate::pb::string::{CaseMode, lcase, mcase, remove};
use crate::pb::numeric::Cux;

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

/// `STR$` of a LONG/QUAD value.
fn str_l(x: Ext) -> PbStr {
    PbStr::from(str_num(x, 15).as_str())
}

use crate::logic::holdings::post_pack_events;

/// `PackHoldingArrays` (0x49117c, [`pack_holding_arrays`]) including its excess-loss news.
pub(crate) fn pack_holdings(g: &mut Globals, ui: &mut dyn Ui) {
    let events = pack_holding_arrays(g);
    post_pack_events(g, ui, events);
}

/// Port of EtfDropOneAdviserContract (0x64e6ba).
/// Finds the first active fund (industry 71, 1581..1600) advised by `adviser_id` and has it pick a new
/// adviser ([`assign_etf_adviser`]); with `voluntary_flag = 1` the old adviser is passed as resigning
/// voluntarily (`oldAdviser = adviser_id`, `voluntary = 1`), else both are 0.
pub fn etf_drop_one_adviser_contract(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    adviser_id: i32,
    voluntary_flag: i32,
) {
    let self_id = adviser_id;
    let mut old_adviser = 0;
    let mut voluntary = 0;
    if voluntary_flag == 1 {
        old_adviser = self_id;
        voluntary = 1;
    }
    let mut etf_id = 1581;
    loop {
        if g.industry_num[etf_id] == 71
            && g.fund_adviser[etf_id] == self_id
            && g.entity_status[etf_id] == 1
        {
            assign_etf_adviser(g, ui, rng, etf_id, old_adviser, voluntary);
            return;
        }
        etf_id += 1;
        if etf_id > 1600 {
            return;
        }
    }
}

/// Port of SellStockHolding (0x4ed93b).
/// Sells shares of stock `corp_ref` on the open market. Mode = `TmpExt3` (0 = a player's sale order:
/// confirmation box, control update, `TurnActionCount + 1`; 1 = silent AI sale; 2+ = AI sale with
/// sentiment hit and headline filtering), seller = `TmpExt1`, percentage points = `TmpExt2` (all three
/// are cleared). With `corp_ref = 0` an overvalued stock (fair value below its cap and projected
/// earnings below the record's field 3) and an AI holder that is not cash-rich are picked at random
/// (up to 30 tries), selling 5..30 points (or the whole holding below 10%).
///
/// Each point sold lowers the cap by 0.8% (funds: once by 1%) down to a floor of 80..90% (records the
/// history tick), credits the cap to the seller's cash and books the gain against the average basis
/// (capital gains for players, extraordinary P&L otherwise); a negative basis (excess loss account) is
/// recaptured when the stake drops below 80%. The cap is then restored to 90..99% of the old cap for
/// player sales and funds. Posts a headline (or, for AI sales in mode 2+, only when not a recent
/// repeat / for the human's company), `TmpExt1` = proceeds (modes 0 and 1), then packs the holding arrays
/// and updates the control chain of the stock.
pub fn sell_stock_holding(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, corp_ref: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError("SellOff", …) (error path not ported)
    let mode = to_long(g.tmp_ext3);
    g.tmp_ext3 = Ext::from_f64(0.0);
    let mut seller_id = 0;
    let mut pct_to_sell = 0;
    if corp_ref != 0 {
        seller_id = to_long(g.tmp_ext1);
        pct_to_sell = to_long(g.tmp_ext2);
        g.tmp_ext1 = Ext::from_f64(0.0);
        g.tmp_ext2 = Ext::from_f64(0.0);
    }
    let mut stock_id = corp_ref;
    let mut slot = 0;
    let mut pct_held = 0;
    let mut public_pct: i16 = 0;
    if stock_id == 0 {
        while slot <= 30 && stock_id == 0 {
            slot += 1;
            let candidate_id = rng.rnd_range(1600, 11);
            if g.entity_status[candidate_id] == 1 {
                g.valuation_mode = 4;
                calc_stock_price(g, ui, rng, candidate_id, 0); // mode 4 does not use TIME$
                if g.calc_stock_price < g.market_cap[candidate_id]
                    && len(g.ent_misc_data[candidate_id].as_bytes()) > 20
                {
                    let msg_text = parse_any(g.ent_misc_data[candidate_id].as_bytes(), b"|", 3);
                    let basis_per_pct = val(msg_text.as_bytes());
                    if basis_per_pct > g.proj_qtr_earnings[candidate_id] {
                        stock_id = candidate_id;
                    }
                }
            }
        }
        if stock_id == 0 {
            return;
        }
        let mut holder_id = 11;
        loop {
            if g.entity_status[holder_id] == 1
                && g.industry_num[holder_id] != 71
                && get_controller_type(g, holder_id) != 1
            {
                slot = 1;
                loop {
                    if cast::<i32, _>(g.hold_stock_id[(slot, holder_id)]) == stock_id
                        && g.hold_stock_pct[(slot, holder_id)] >= 0
                    {
                        seller_id = holder_id;
                        pct_held = cast::<i32, _>(g.hold_stock_pct[(slot, holder_id)]);
                        if get_controller_type(g, seller_id) == 1 {
                            seller_id = 0;
                            pct_held = 0;
                        }
                        if seller_id < 131 && g.cur_year % 5 != g.cur_quarter {
                            seller_id = 0;
                        }
                        if seller_id > 0 {
                            calc_entity_financials(g, ui, rng, seller_id);
                            if g.cash[seller_id] > g.total_assets * sf(0.05f32) {
                                seller_id = 0;
                            }
                        }
                        if seller_id > 0 {
                            slot = 15;
                            holder_id = 1600;
                        }
                    }
                    slot += 1;
                    if slot > 15 {
                        break;
                    }
                }
            }
            holder_id += 1;
            if holder_id > 1600 {
                break;
            }
        }
        if seller_id == 0 || stock_id == seller_id {
            return;
        }
        if get_controller_type(g, seller_id) == 2 && g.ai_target_corp == stock_id {
            return;
        }
        if g.country_idx[stock_id] == 26 && g.country_idx[seller_id] == 26 {
            return;
        }
        if g.country_idx[stock_id] == 27 && g.country_idx[seller_id] == 27 {
            return;
        }
        pct_to_sell = to_long(Ext::from_f64(5.0) + rng.rnd() * Ext::from_f64(25.0));
        if pct_held < 10 || pct_held < pct_to_sell {
            pct_to_sell = pct_held;
        }
    }
    if mode == 0 {
        update_corp_control(g, ui, rng, stock_id);
        public_pct = g.public_pct;
    }
    if pct_to_sell < 1 {
        return;
    }
    let pct_sold = pct_to_sell;
    let mut price_restore = (rng.rnd() * sf(0.09f32) + sf(0.9f32)) * g.market_cap[stock_id];
    let mut price_floor = (rng.rnd() * sf(0.1f32) + sf(0.8f32)) * g.market_cap[stock_id];
    if g.industry_num[stock_id] == 71 {
        price_restore = g.market_cap[stock_id];
        g.market_cap[stock_id] = g.market_cap[stock_id] * sf(0.99f32);
        price_floor = g.market_cap[stock_id];
    }
    let mut ela_recapture = Ext::from_f64(0.0);
    let mut proceeds = Ext::from_f64(0.0);
    let mut gain_total = Ext::from_f64(0.0);
    let mut basis_total = Ext::from_f64(0.0);
    let mut sold_out = 0;
    let sid = seller_id;
    for slot in 1..=15 {
        if cast::<i32, _>(g.hold_stock_id[(slot, sid)]) != stock_id {
            continue;
        }
        if (cast::<i32, _>(g.hold_stock_pct[(slot, sid)])) - pct_to_sell < 80
            && g.hold_stock_basis[(slot, sid)] < Ext::from_f64(0.0)
        {
            ela_recapture = g.hold_stock_basis[(slot, sid)].abs();
            g.extraordinary_pl[sid] = to_quad(cast::<Ext, _>(g.extraordinary_pl[sid]) + ela_recapture);
            g.hold_stock_basis[(slot, sid)] = Ext::from_f64(0.0);
        }
        while pct_to_sell > 0 {
            let mut basis_per_pct = Ext::from_f64(0.0);
            let candidate_id = cast::<i32, _>(g.hold_stock_pct[(slot, sid)]);
            if candidate_id > 0 {
                basis_per_pct = g.hold_stock_basis[(slot, sid)] * (Ext::from_f64(1.0) / cast::<Ext, _>(candidate_id));
            }
            if candidate_id == 1 {
                basis_per_pct = g.hold_stock_basis[(slot, sid)];
            }
            g.hold_stock_pct[(slot, sid)] = g.hold_stock_pct[(slot, sid)].wrapping_sub(1);
            if g.hold_stock_pct[(slot, sid)] == 0 {
                g.hold_stock_basis[(slot, sid)] = Ext::from_f64(0.0);
            } else {
                g.hold_stock_basis[(slot, sid)] = g.hold_stock_basis[(slot, sid)] - basis_per_pct;
            }
            if g.industry_num[stock_id] != 71 {
                g.market_cap[stock_id] = g.market_cap[stock_id] * sf(0.992f32);
                if g.market_cap[stock_id] < price_floor {
                    g.market_cap[stock_id] = price_floor;
                }
            }
            update_price_history_tick(g, ui, rng, stock_id);
            g.cash[sid] = g.market_cap[stock_id] + g.cash[sid];
            proceeds = g.market_cap[stock_id] + proceeds;
            if g.num_players < sid {
                g.extraordinary_pl[sid] = to_quad(
                    (g.market_cap[stock_id] - basis_per_pct) + cast::<Ext, _>(g.extraordinary_pl[sid]),
                );
            } else {
                g.player_cap_gains[sid] =
                    (g.market_cap[stock_id] - basis_per_pct) + g.player_cap_gains[sid];
            }
            gain_total = (g.market_cap[stock_id] - basis_per_pct) + gain_total;
            basis_total = basis_per_pct + basis_total;
            pct_to_sell -= 1;
            if g.hold_stock_pct[(slot, sid)] == 0 {
                g.hold_stock_id[(slot, sid)] = 0;
                pct_to_sell = 0;
                sold_out = 1;
            }
        }
        gain_total = ela_recapture + gain_total;
        if (mode == 0 || g.industry_num[stock_id] == 71) && g.market_cap[stock_id] < price_restore {
            g.market_cap[stock_id] = price_restore;
        }
    }
    if g.industry_num[stock_id] != 71 {
        if pct_sold == 1 {
            g.investor_sentiment[stock_id] = g.investor_sentiment[stock_id].wrapping_sub(1);
        }
        if mode > 1 {
            g.investor_sentiment[stock_id] =
                cast::<i32, _>(to_quad(cast::<Ext, _>(g.investor_sentiment[stock_id]) - sf(0.5f32) * cast::<Ext, _>(pct_sold)));
        }
        if mode < 2 {
            g.investor_sentiment[stock_id] =
                cast::<i32, _>(to_quad(cast::<Ext, _>(g.investor_sentiment[stock_id]) - sf(0.25f32) * cast::<Ext, _>(pct_sold)));
        }
    }
    let mut msg_text = s(" dumps") + &str_l(cast::<Ext, _>(pct_sold)) + __content.string(2163);
    if g.rand_seed % 5 == 1 {
        msg_text = s(__content.string(11155));
    }
    if g.rand_seed % 5 == 2 {
        msg_text = s(__content.string(11156));
    }
    if sold_out == 1 {
        msg_text = s(__content.string(11157));
        if g.rand_seed % 3 == 1 {
            msg_text = s(__content.string(11158));
        }
    }
    let mut price_text = s(" for ")
        + &trim(g.currency_symbol.as_bytes())
        + &trim(&format(proceeds, b"* ###,"))
        + " "
        + &g.million_prefix_lc
        + "il.";
    if instr(1, msg_text.as_bytes(), b"%") == 0 {
        price_text = s(" (") + &trim(str_l(cast::<Ext, _>(pct_sold)).as_bytes()) + "%).";
    }
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    g.news_msg = g.ent_symbol[sid].clone()
        + &msg_text
        + &g.ent_symbol[stock_id]
        + __content.string(11159)
        + &price_text;
    if len(g.ent_symbol[sid].as_bytes()) > 10 {
        g.news_msg = remove(g.news_msg.as_bytes(), __content.bytes(11159));
    }
    msg_text = g.ent_name[stock_id].clone() + " (" + &g.ent_symbol[stock_id] + ").";
    if g.rand_seed % 4 == 1 {
        g.news_msg = s(__content.string(11160)) + &msg_text;
        if rng.rnd() * Ext::from_f64(3.0) <= Ext::from_f64(2.0) {
            if rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) {
                g.news_msg = replace(g.news_msg.as_bytes(), __content.bytes(11161), b"noted");
            }
        } else {
            g.news_msg = replace(g.news_msg.as_bytes(), __content.bytes(11161), b"seen");
        }
    }
    if 10 < sid && sid < 131 && g.rand_seed % 7 > 4 {
        g.news_msg = s(__content.string(11162)) + &msg_text;
    }
    if sid > 1585 && g.industry_num[sid] == 71 && g.rand_seed % 7 > 3 {
        g.news_msg = g.ent_symbol[sid].clone() + __content.string(11163) + &msg_text;
    }
    if mode == 0 {
        price_text = trim(g.currency_symbol.as_bytes())
            + &trim(&format(proceeds, b"* ###,.0"))
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix
            + "....";
        if !(basis_total > Ext::from_f64(0.0)) {
            basis_total = sf(0.1f32);
        }
        let mut gain_pct = (gain_total.abs() / basis_total) * Ext::from_f64(100.0);
        if gain_total < Ext::from_f64(0.0) {
            msg_text = s("loss");
            if gain_pct > Ext::from_f64(100.0) {
                gain_pct = Ext::from_f64(100.0);
            }
        } else {
            msg_text = s("gain");
        }
        let mut detail_text = s(__content.string(11164))
            + &msg_text
            + " of "
            + &trim(g.currency_symbol.as_bytes())
            + &trim(&format(gain_total.abs(), b"* ###,.00"))
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix
            + " on "
            + __content.string(11165);
        if basis_total > sf(0.11f32) {
            detail_text = detail_text
                + __content.string(11166)
                + &msg_text
                + " of "
                + &trim(&format(gain_pct, b"* ###.0"))
                + "%.";
        } else {
            detail_text = detail_text + ".";
        }
        if ela_recapture > Ext::from_f64(1.0) {
            detail_text = detail_text
                + __content.string(11167)
                + __content.string(11168)
                + &g.ent_name[stock_id]
                + __content.string(2965)
                + &trim(g.currency_symbol.as_bytes())
                + &trim(&format(ela_recapture, b"* ###,"))
                + " "
                + &g.million_prefix_lc
                + "illion"
                + &g.currency_suffix
                + __content.string(11169)
                + __content.string(11170);
        }
        detail_text = s("\r") + "\r" + &detail_text;
        let text = s(__content.string(11171))
            + "\r"
            + "\r"
            + &g.ent_symbol[sid]
            + __content.string(11172)
            + &str_l(cast::<Ext, _>(pct_sold))
            + __content.string(2175)
            + " of "
            + &g.ent_name[stock_id]
            + ", "
            + __content.string(10859)
            + &price_text
            + &detail_text;
        ui.msgbox(&text, 0x2000, &s(__content.string(9864)));
        g.turn_action_count += 1;
        msg_text = g.ent_symbol[sid].clone()
            + " sells"
            + &str_l(cast::<Ext, _>(pct_sold))
            + __content.string(11173)
            + &g.ent_symbol[stock_id];
        g.news_msg = msg_text.clone() + " for " + &price_text;
        if public_pct == 0 {
            g.news_msg = msg_text.clone() + __content.string(11174);
        }
    }
    if mode < 2 {
        g.tmp_ext1 = proceeds;
    } else {
        let same_start =
            mid(g.news_msg.as_bytes(), 1, 10) == mid(g.news_headline[60].as_bytes(), 1, 10);
        if (same_start || is_in_recent_headlines(g, __content.bytes(11175)) == 1)
            && g.cur_player != g.corp_controller[stock_id]
        {
            g.news_msg = PbStr::new();
        }
        if sid > 1580 && g.industry_num[sid] == 71 && g.rand_seed % 7 != 3 {
            g.news_msg = PbStr::new();
            g.rand_seed += 1;
        }
    }
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    pack_holdings(g, ui);
    update_control_chain(g, ui, rng, stock_id);
}

/// Port of ForcedSellStocks (0x4ff864).
/// Raises cash for `seller_id` (cash < 0) by dumping stock holdings slot by slot, one percentage point at
/// a time at 99% of the cap (each point lowers the cap by 0.5% except for funds, floored at 80% of the
/// cap — 100% for funds), up to 101 points per holding, until the cash is no longer negative. Gains are
/// booked against the average basis (players: capital gains; others: extraordinary P&L); a stake
/// falling below 80% recaptures a negative basis (`RecaptureExcessLoss`). A computer player's company
/// (controller type 2) is first revalued (mode 3); the human current player's 100% holdings are
/// auctioned at 75..85% of the cap. With `mode = 0` insurers slow their growth by 3 and operating corps
/// with assets may lose growth points. Posts one headline per stock sold and records the history tick.
/// Returns the sold stock ids as `"id^id^…"`.
pub fn forced_sell_stocks(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    seller_id: i32,
    mode: i32,
) -> PbStr {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError("StockLiq", …) (error path not ported)
    let mut sold_list = PbStr::new();
    for slot in 1..=15 {
        let mut pct_sold = 0;
        let mut is_auction = 0;
        if g.hold_stock_id[(slot, seller_id)] == 0 || g.hold_stock_pct[(slot, seller_id)] < 0 {
            continue;
        }
        let stock_id = cast::<i32, _>(g.hold_stock_id[(slot, seller_id)]);
        if get_controller_type(g, seller_id) == 2 {
            g.valuation_mode = 3;
            calc_stock_price(g, ui, rng, stock_id, 0); // mode 3 does not use TIME$
        } else if g.cur_player == seller_id && g.hold_stock_pct[(slot, seller_id)] == 100 {
            g.market_cap[stock_id] = (rng.rnd() * Ext::from_f64(10.0) + Ext::from_f64(75.0)) * g.market_cap[stock_id] * sf(0.01f32);
            is_auction = 1;
        }
        // DOUBLE local
        let price_floor: Ext = crate::pb::numeric::double(if g.industry_num[stock_id] == 71 {
            g.market_cap[stock_id]
        } else {
            g.market_cap[stock_id] * sf(0.8f32)
        });
        if mode == 0
            && 85 < seller_id
            && g.industry_num[seller_id] == 2
            && g.growth_rate[seller_id] > -5
        {
            g.growth_rate[seller_id] = g.growth_rate[seller_id].wrapping_sub(3);
        }
        let mut iter = 0;
        loop {
            if !(g.cash[seller_id] < Ext::from_f64(0.0)) {
                break;
            }
            if g.hold_stock_pct[(slot, seller_id)] < 1 {
                break;
            }
            iter += 1;
            let cost_basis;
            if g.hold_stock_pct[(slot, seller_id)] < 2 {
                g.hold_stock_id[(slot, seller_id)] = 0;
                g.hold_stock_pct[(slot, seller_id)] = 0;
                cost_basis = g.hold_stock_basis[(slot, seller_id)];
                g.hold_stock_basis[(slot, seller_id)] = Ext::from_f64(0.0);
                pct_sold += 1;
                let proceeds = g.market_cap[stock_id] * sf(0.99f32);
                g.cash[seller_id] = proceeds + g.cash[seller_id];
                if g.num_players < seller_id {
                    g.extraordinary_pl[seller_id] =
                        to_quad((proceeds - cost_basis) + cast::<Ext, _>(g.extraordinary_pl[seller_id]));
                } else {
                    g.player_cap_gains[seller_id] =
                        (proceeds - cost_basis) + g.player_cap_gains[seller_id];
                }
                let hold_count = cast::<i32, _>(g.hold_stock_id[(0, seller_id)]) - 1;
                if hold_count >= 0 {
                    g.hold_stock_id[(0, seller_id)] = cast::<i16, _>(hold_count);
                }
            } else {
                let mut cb = Ext::from_f64(0.0);
                let hold_pct = cast::<i32, _>(g.hold_stock_pct[(slot, seller_id)]);
                if hold_pct > 0 {
                    cb = g.hold_stock_basis[(slot, seller_id)] * (Ext::from_f64(1.0) / cast::<Ext, _>(hold_pct));
                }
                cost_basis = cb;
                g.hold_stock_basis[(slot, seller_id)] =
                    g.hold_stock_basis[(slot, seller_id)] - cost_basis;
                g.hold_stock_pct[(slot, seller_id)] =
                    g.hold_stock_pct[(slot, seller_id)].wrapping_sub(1);
                if 10 < seller_id
                    && g.hold_stock_pct[(slot, seller_id)] < 80
                    && g.hold_stock_basis[(slot, seller_id)] < Ext::from_f64(0.0)
                {
                    let mut events = Vec::new();
                    recapture_excess_loss(g, stock_id, seller_id, &mut events);
                    post_pack_events(g, ui, events);
                }
                pct_sold += 1;
                let proceeds = g.market_cap[stock_id] * sf(0.99f32);
                g.cash[seller_id] = proceeds + g.cash[seller_id];
                if g.num_players < seller_id {
                    g.extraordinary_pl[seller_id] =
                        to_quad((proceeds - cost_basis) + cast::<Ext, _>(g.extraordinary_pl[seller_id]));
                } else {
                    g.player_cap_gains[seller_id] =
                        (proceeds - cost_basis) + g.player_cap_gains[seller_id];
                }
            }
            if g.industry_num[stock_id] != 71 {
                g.market_cap[stock_id] = g.market_cap[stock_id] * sf(0.995f32);
            }
            if g.market_cap[stock_id] < price_floor {
                g.market_cap[stock_id] = price_floor;
            }
            if mode == 0
                && 130 < seller_id
                && g.business_assets[seller_id] > Ext::from_f64(50.0)
                && rng.rnd() * Ext::from_f64(20.0) < cast::<Ext, _>(g.growth_rate[seller_id])
            {
                g.growth_rate[seller_id] = g.growth_rate[seller_id].wrapping_sub(1);
            }
            if g.cash[seller_id] < Ext::from_f64(0.0) {
                balance_cash_t_bills(g, seller_id);
            }
            if iter > 100 {
                break;
            }
        }
        if pct_sold != 0 {
            let mut seller_name = g.ent_name[seller_id].clone();
            let mut msg_suffix = s(__content.string(8489));
            if seller_id < 11 {
                msg_suffix = s(".");
            } else {
                seller_name = g.ent_symbol[seller_id].clone();
            }
            g.valuation_mode = 0;
            calc_stock_price(g, ui, rng, stock_id, 0); // mode 0 does not use TIME$
            let mut msg_text = seller_name.clone()
                + __content.string(11176)
                + &str_l(cast::<Ext, _>(pct_sold))
                + "% of "
                + &g.ent_symbol[stock_id]
                + __content.string(8237)
                + &trim(g.currency_symbol.as_bytes())
                + &g.stock_price_str
                + &msg_suffix;
            if is_auction == 1 {
                msg_text = seller_name.clone()
                    + __content.string(11177)
                    + &str_l(cast::<Ext, _>(pct_sold))
                    + "% of "
                    + &g.ent_symbol[stock_id]
                    + __content.string(11178)
                    + &trim(g.currency_symbol.as_bytes())
                    + &g.stock_price_str
                    + &msg_suffix;
            }
            if !g.news_msg.is_empty() {
                post_news(g, ui, 0, 0);
            }
            g.news_msg = msg_text;
            post_news(g, ui, stock_id, seller_id);
            sold_list = sold_list + &trim(str_l(cast::<Ext, _>(stock_id)).as_bytes()) + "^";
            update_price_history_tick(g, ui, rng, stock_id);
        }
    }
    sold_list
}

/// Gain on a government-bond sale: players → capital gains, ids < 86 → accrued P&L, others →
/// extraordinary P&L.
fn book_gov_bond_gain(g: &mut Globals, entity_id: i32, gain: Ext) {
    if entity_id < 11 {
        g.player_cap_gains[entity_id] = gain + g.player_cap_gains[entity_id];
    } else if entity_id < 86 {
        g.accrued_pl[entity_id] = gain + g.accrued_pl[entity_id];
    } else {
        g.extraordinary_pl[entity_id] = to_quad(gain + cast::<Ext, _>(g.extraordinary_pl[entity_id]));
    }
}

/// Port of ForcedSellBonds (0x500723).
/// Raises cash for `entity_id` after [`balance_cash_t_bills`] left it negative (else returns 0 at once).
/// `bond_mode != 2`: sells government bonds in blocks of up to 100 (face) at 99.8% of the price —
/// long bonds unless none are held, a 1-in-3 chance or more short bonds are held — with accrued
/// interest, until the cash is covered or none are left; insurers without a managing player cut their
/// growth by 5; posts "must sell … of government bonds". The result is the face value sold (negated
/// when `total_sold_in` is nonzero). If cash is still negative and `bond_mode != 1` (and always for
/// `bond_mode = 2`) it then sells corporate bonds one unit at a time at 99.5% of `BondPrice` with
/// accrued interest, posts "must sell … [runtime resource wording] " and returns that face value
/// instead. Gains are booked against the average cost.
///
/// Bug (reproduced, BUGS.md): for banks (ids 11..85) the gain on short gov notes is computed with
/// `LongBondPrice`. `corp_sold_in` is read but never used.
pub fn forced_sell_bonds(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    entity_id: i32,
    bond_mode: i32,
    total_sold_in: Ext,
    corp_sold_in: Ext,
) -> Ext {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError("BondLiq", …) (error path not ported)
    let fmt_mask: &[u8] = b"* ###,";
    let id = entity_id;
    let mut result = Ext::from_f64(0.0);
    balance_cash_t_bills(g, id);
    if !(g.cash[id] < Ext::from_f64(0.0)) {
        return result;
    }
    let mut total_sold = total_sold_in;
    let _ = corp_sold_in; // copied to the corp sale value, which is always overwritten
    let manager_id = get_managing_player(g, id);
    if bond_mode != 2 {
        if !(g.long_gov_bond_face[id] == 0 && g.short_gov_bond_face[id] == 0) {
            let mut sale_value: i64 = 0;
            loop {
                if !(cast::<Ext, _>(g.short_gov_bond_face[id] + g.long_gov_bond_face[id]) > Ext::from_f64(0.0)) {
                    break;
                }
                if !(g.cash[id] < Ext::from_f64(0.0)) {
                    break;
                }
                let mut face_held = g.long_gov_bond_face[id];
                let mut which = 1;
                if face_held < 1 {
                    which = 2;
                }
                if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(2.0) || g.short_gov_bond_face[id] > g.long_gov_bond_face[id] {
                    which = 2;
                }
                let lbp = cast::<Ext, _>(g.long_bond_price);
                let sbp = cast::<Ext, _>(g.short_bond_price);
                if which == 1 {
                    if face_held > 0 {
                        let face_to_sell: i64 = if face_held > 100 { 100 } else { 1 };
                        let cost_basis =
                            g.lt_gov_bond_cost[id] * (cast::<Ext, _>(face_to_sell) / cast::<Ext, _>(face_held));
                        g.long_gov_bond_face[id] =
                            to_quad(cast::<Ext, _>(g.long_gov_bond_face[id]) - cast::<Ext, _>(face_to_sell));
                        g.lt_gov_bond_cost[id] = g.lt_gov_bond_cost[id] - cost_basis;
                        let gain = sf(0.998f32) * lbp * sf(0.01f32) * cast::<Ext, _>(face_to_sell) - cost_basis;
                        book_gov_bond_gain(g, id, gain);
                        let accrued_int = calc_accrued_bond_interest(g, face_to_sell, 1);
                        g.cash[id] = sf(0.998f32) * lbp * sf(0.01f32) * cast::<Ext, _>(face_to_sell)
                            + g.cash[id]
                            + accrued_int;
                        g.accrued_pl[id] = accrued_int + g.accrued_pl[id];
                        sale_value =
                            to_quad(sf(0.01f32) * lbp * cast::<Ext, _>(face_to_sell) + cast::<Ext, _>(sale_value));
                    }
                } else {
                    face_held = g.short_gov_bond_face[id];
                    if face_held > 0 {
                        let face_to_sell: i64 = if face_held > 100 { 100 } else { 1 };
                        let cost_basis =
                            g.st_gov_bond_cost[id] * (cast::<Ext, _>(face_to_sell) / cast::<Ext, _>(face_held));
                        g.short_gov_bond_face[id] =
                            to_quad(cast::<Ext, _>(g.short_gov_bond_face[id]) - cast::<Ext, _>(face_to_sell));
                        g.st_gov_bond_cost[id] = g.st_gov_bond_cost[id] - cost_basis;
                        // banks: LongBondPrice (bug, reproduced)
                        let price = if id >= 11 && id < 86 { lbp } else { sbp };
                        let gain = sf(0.998f32) * price * sf(0.01f32) * cast::<Ext, _>(face_to_sell) - cost_basis;
                        book_gov_bond_gain(g, id, gain);
                        let accrued_int = calc_accrued_bond_interest(g, face_to_sell, 2);
                        g.cash[id] = sf(0.998f32) * sbp * sf(0.01f32) * cast::<Ext, _>(face_to_sell)
                            + g.cash[id]
                            + accrued_int;
                        g.accrued_pl[id] = accrued_int + g.accrued_pl[id];
                        sale_value =
                            to_quad(sf(0.01f32) * sbp * cast::<Ext, _>(face_to_sell) + cast::<Ext, _>(sale_value));
                    }
                }
            }
            if total_sold == Ext::from_f64(0.0) {
                total_sold = cast::<Ext, _>(sale_value);
            } else {
                total_sold = -Ext::from_f64(1.0) * cast::<Ext, _>(sale_value);
            }
            if manager_id == 0 && 10 < id && g.industry_num[id] == 2 && g.growth_rate[id] > 3 {
                g.growth_rate[id] = g.growth_rate[id].wrapping_sub(5);
            }
            if sale_value > 0 {
                if !g.news_msg.is_empty() {
                    post_news(g, ui, 0, 0);
                }
                g.news_msg = g.ent_symbol[id].clone()
                    + __content.string(11179)
                    + &trim(g.currency_symbol.as_bytes())
                    + &trim(&format(cast::<Ext, _>(sale_value), fmt_mask))
                    + " "
                    + &g.million_prefix_lc
                    + __content.string(11180);
                post_news(g, ui, id, 0);
            }
        }
        if !(g.long_gov_bond_face[id] > 0) {
            g.lt_gov_bond_cost[id] = Ext::from_f64(0.0);
            g.long_gov_bond_face[id] = 0;
        }
        if !(g.short_gov_bond_face[id] > 0) {
            g.st_gov_bond_cost[id] = Ext::from_f64(0.0);
            g.short_gov_bond_face[id] = 0;
        }
        result = total_sold;
        if !(g.cash[id] < Ext::from_f64(0.0)) || bond_mode == 1 {
            return result;
        }
    }
    // corporate bonds
    let mut sale_value: i64 = 0;
    for slot in 1..=15 {
        let mut units_sold: i64 = 0;
        let bond_id = g.hold_bond_id[(slot, id)];
        if bond_id == 0 {
            continue;
        }
        loop {
            if !(g.hold_bond_face[(slot, id)] > 0) {
                break;
            }
            if !(g.cash[id] < Ext::from_f64(0.0)) {
                break;
            }
            let face_held = g.hold_bond_face[(slot, id)];
            let mut cost_basis = Ext::from_f64(0.0);
            if face_held > 0 {
                cost_basis = g.bond_hold_cost[(slot, id)] * (Ext::from_f64(1.0) / cast::<Ext, _>(face_held));
            }
            g.hold_bond_face[(slot, id)] = to_quad(cast::<Ext, _>(g.hold_bond_face[(slot, id)]) - Ext::from_f64(1.0));
            g.bond_hold_cost[(slot, id)] = g.bond_hold_cost[(slot, id)] - cost_basis;
            let bp = g.bond_price[bond_id].get();
            let gain = bp * sf(0.995f32) * sf(0.01f32) - cost_basis;
            if id < 11 {
                g.player_cap_gains[id] = gain + g.player_cap_gains[id];
            } else if id <= 86 {
                g.extraordinary_pl[id] = to_quad(gain + cast::<Ext, _>(g.extraordinary_pl[id]));
            } else {
                g.accrued_pl[id] = gain + g.accrued_pl[id];
            }
            g.cash[id] = bp * sf(0.995f32) * sf(0.01f32) + g.cash[id];
            sale_value = to_quad(bp * sf(0.01f32) + cast::<Ext, _>(sale_value));
            units_sold = to_quad(cast::<Ext, _>(units_sold) + Ext::from_f64(1.0));
        }
        if units_sold > 0 {
            let accrued_int = calc_accrued_bond_interest(g, units_sold, bond_id);
            g.accrued_pl[id] = accrued_int + g.accrued_pl[id];
            g.cash[id] = accrued_int + g.cash[id];
        }
        if g.hold_bond_face[(slot, id)] == 0 {
            g.hold_bond_id[(slot, id)] = 0;
            g.bond_hold_cost[(slot, id)] = Ext::from_f64(0.0);
        }
    }
    // DOUBLE local
    let corp_sale_value = crate::pb::numeric::double(cast::<Ext, _>(sale_value));
    if corp_sale_value > Ext::from_f64(0.0) {
        if total_sold < Ext::from_f64(0.0) {
            total_sold = total_sold - corp_sale_value;
        } else {
            total_sold = total_sold + corp_sale_value;
        }
        let _ = total_sold; // not returned
        if !g.news_msg.is_empty() {
            post_news(g, ui, 0, 0);
        }
        g.news_msg = g.ent_symbol[id].clone()
            + __content.string(11179)
            + &trim(g.currency_symbol.as_bytes())
            + &trim(&format(corp_sale_value, fmt_mask))
            + " "
            + &g.million_prefix_lc
            + __content.string(11181);
        post_news(g, ui, id, 0);
    }
    corp_sale_value
}

/// Port of CoverCashDeficit (0x6625d7) with its GOSUB CoverCashDeficit_SellCommods (0x662d72).
/// Only for entities hol [runtime resource wording] or crypto (`GetPhysCommodValue(entity, "") <> 0`,
/// else returns 0 at once): draws on the credit line if cash + T-bills is negative, sells T-bills
/// (news), and if the cash is still below -0.00001 sells physical holdings — for the human current
/// player with subsidiaries interactively (MSGBOX loop, pick a holding from the list), otherwise OIL,
/// GOLD, SILVER, WHEAT, CORN, BITCOIN, ETHEREUM in steps of 20/25/33/50/100% until the cash is covered.
/// Returns 1 if a commodity sale covered the deficit, else the "sold something" flag (AI path) or 0.
/// `now`: the local time (for `SelectActingEntity`).
pub fn cover_cash_deficit(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, entity_id: i32, now: &crate::logic::newgame::LocalTime) -> i32 {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let ent_id = entity_id;
    let mut result = 0;
    let mut sold_flag = 0;
    if get_phys_commod_value(g, ent_id, &PbStr::new()) == Ext::from_f64(0.0) {
        return result;
    }
    if g.t_bills[ent_id] + g.cash[ent_id] < Ext::from_f64(0.0) {
        let _ = draw_credit_line_for_cash(g, ui, rng, ent_id);
    }
    if g.cash[ent_id] < Ext::from_f64(0.0) && g.t_bills[ent_id] > Ext::from_f64(0.0) {
        balance_cash_t_bills(g, ent_id);
        g.news_msg = g.ent_symbol[ent_id].clone() + __content.string(11182);
        if g.cash[ent_id] < Ext::from_f64(0.0) {
            g.news_msg = replace(g.news_msg.as_bytes(), b"cover", __content.bytes(11183));
        }
        post_news(g, ui, ent_id, 0);
    }
    if !(g.cash[ent_id] < -sf(1e-5f32)) {
        return result;
    }
    if g.cur_player != ent_id || !(g.tmp_subsid_stock_value > Ext::from_f64(0.0)) {
        for name in [
            "OIL", "GOLD", "SILVER", "WHEAT", "CORN", "BITCOIN", "ETHEREUM",
        ] {
            let commod_name = s(name);
            let commod_value = get_phys_commod_value(g, ent_id, &commod_name);
            if commod_value > Ext::from_f64(0.0) {
                // GOSUB CoverCashDeficit_SellCommods
                sold_flag = 1;
                g.cur_commodity = commod_name.clone();
                for pct_to_sell in [20.0f32, 25.0f32, 33.0f32, 50.0f32, 100.0f32] {
                    crate::logic::commod_ai::physical::sell_phys_commod(g, ui, rng, ent_id, 0, 2, pct_to_sell, 0);
                    if !(g.cash[ent_id] < Ext::from_f64(0.0)) {
                        return 1;
                    }
                }
            }
        }
        result = sold_flag;
        return result;
    }
    if g.cur_player != g.active_entity {
        crate::logic::trading::select_acting_entity(g, ui, rng, 1, now);
    }
    loop {
        loop {
            if !(g.cash[ent_id] < Ext::from_f64(0.0)) {
                return result;
            }
            let text = s(__content.string(11032))
                + "\r"
                + "\r"
                + __content.string(11184)
                + __content.string(11185)
                + &trim(g.currency_symbol.as_bytes())
                + &trim(&format(g.cash[ent_id].abs(), b"* ######,.00"))
                + " "
                + &g.million_prefix_lc
                + "illion"
                + &g.currency_suffix
                + "."
                + "\r"
                + "\r"
                + __content.string(11186);
            let answer = ui.msgbox(
                &text,
                0x2024,
                &s(__content.string(11187)),
            );
            if answer == 7 {
                return result;
            }
            if g.cur_player == g.active_entity {
                break;
            }
        }
        let active = g.active_entity;
        let selected_rec = list_phys_commod_holdings(g, ui, rng, active, 1);
        if selected_rec < 1 {
            break;
        }
        if val(g.phys_commod_rec[0].as_bytes()) < cast::<Ext, _>(selected_rec) {
            break;
        }
        if val(parse_any(g.phys_commod_rec[selected_rec].as_bytes(), b"|", 2).as_bytes())
            != cast::<Ext, _>(g.active_entity)
        {
            return result;
        }
        let commod_name = g.phys_commod_rec[selected_rec].clone();
        // (UI) DIALOG END hDlgTransMenu
        g.action_menu_open = 0;
        g.cur_commodity = trim(parse_any(commod_name.as_bytes(), b"|", 1).as_bytes());
        let active = g.active_entity;
        crate::logic::commod_ai::physical::sell_phys_commod(g, ui, rng, active, selected_rec, 0, 0.0f32, 0);
        // (UI) EnableMainControls
    }
    result
}

/// Port of BookFuturesGainLoss (0x626b81).
/// Books a futures (or physical-commodity) gain/loss `gain_loss` of `entity_id` on `commod_name`:
/// players → capital gains; a corporation whose industry hedges that commodity (OIL: 7, 8, 11, 16,
/// 23, 25, 52, 69; WHEAT/CORN: 15, 28; GOLD/SILVER: 50; STOCKS: 37) → accrued P&L and
/// `CorpHedgingGain`; anything else → extraordinary P&L. Posts "… reports hedging profits / trading
/// gain on …" when the amount is at least `ExchangeRate * 10` and the entity is the selected one (or
/// for `book_mode` 1/3); `book_mode` 2 never posts, 4 reports a delivery (with `FuturesBatchGain`).
pub fn book_futures_gain_loss(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    gain_loss: Ext,
    entity_id: i32,
    commod_name: &PbStr,
    book_mode: i32,
) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError("ReclassGain", …) (error path not ported)
    let amount = gain_loss;
    let ent_id = entity_id;
    let commod = commod_name.clone();
    let mut asset_label = commod_name.clone();
    if *commod_name == "STOCKS" {
        asset_label = s(__content.string(2499));
    }
    let mut commod_lc = lcase(asset_label.as_bytes(), CaseMode::Ansi);
    if commod == "BITCOIN" || commod == "ETHEREUM" {
        commod_lc = mcase(commod.as_bytes(), CaseMode::Ansi);
    }
    // assetLabel gets " futures" / "physical " here but is not used afterwards
    let mut news_mode = book_mode;
    if g.entity_status[ent_id] != 1 {
        return;
    }
    let gain_word = if amount < Ext::from_f64(0.0) { s("loss") } else { s("gain") };
    let mut verb_phrase = if amount < Ext::from_f64(0.0) {
        s(__content.string(11188))
    } else {
        s(__content.string(11189))
    };
    let mut is_hedge = 0;
    if ent_id < 11 {
        g.player_cap_gains[ent_id] = amount + g.player_cap_gains[ent_id];
        verb_phrase = s("has ") + &gain_word;
        is_hedge = 1;
    } else {
        let industry = g.industry_num[ent_id];
        let hedges = if commod == "OIL" {
            matches!(industry, 7 | 8 | 11 | 16 | 23 | 25 | 52 | 69)
        } else if commod == "WHEAT" || commod == "CORN" {
            industry == 15 || industry == 28
        } else if commod == "GOLD" || commod == "SILVER" {
            industry == 50
        } else if commod == "STOCKS" {
            industry == 37
        } else {
            false
        };
        if hedges {
            g.accrued_pl[ent_id] = amount + g.accrued_pl[ent_id];
            g.corp_hedging_gain[ent_id] = amount + g.corp_hedging_gain[ent_id];
            is_hedge = 1;
        }
    }
    if is_hedge == 0 {
        g.extraordinary_pl[ent_id] = to_quad(amount + cast::<Ext, _>(g.extraordinary_pl[ent_id]));
        verb_phrase = s(__content.string(11190)) + &gain_word;
        if amount < Ext::from_f64(0.0) {
            verb_phrase = s(__content.string(11191));
            if rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) {
                verb_phrase = replace(verb_phrase.as_bytes(), b"incur", b"suffer");
            }
        }
    }
    let rounded_amt = to_quad(amount);
    g.news_msg = g.ent_symbol[ent_id].clone()
        + " "
        + &verb_phrase
        + " on "
        + &commod_lc
        + ", of "
        + &trim(g.currency_symbol.as_bytes())
        + &trim(&format((cast::<Ext, _>(rounded_amt)).abs(), b"* ###,"))
        + " "
        + &g.million_prefix_lc
        + "il."
        + &g.currency_suffix;
    if len(g.news_msg.as_bytes()) > 75 || amount.abs() < g.exchange_rate * Ext::from_f64(10.0) {
        g.news_msg = PbStr::new();
    }
    if get_controller_type(g, ent_id) == 0 {
        news_mode = 0;
    }
    if g.sel_entity == ent_id || g.last_sel_entity == ent_id {
        news_mode = 1;
    }
    if book_mode == 2 {
        news_mode = 2;
    }
    if book_mode == 3 {
        news_mode = 3;
    }
    let keep = instr(1, g.news_msg.as_bytes(), b"hedging") > 0
        && g.sim_co_count == ent_id
        && get_controller_type(g, ent_id) != 0
        && g.sel_entity == ent_id;
    if !keep {
        if news_mode == 1 {
            if 1583 < ent_id && ent_id < 1586 && g.industry_num[ent_id] == 71 {
                g.news_msg = PbStr::new();
            }
        } else {
            g.news_msg = PbStr::new();
        }
    }
    if book_mode == 2 {
        g.news_msg = PbStr::new();
        return;
    }
    if book_mode == 4 {
        g.news_msg = g.ent_symbol[ent_id].clone()
            + __content.string(11192)
            + &commod_lc
            + __content.string(11193);
        if g.futures_batch_gain.abs() > Ext::from_f64(5.0) {
            g.news_msg = g.ent_symbol[ent_id].clone()
                + __content.string(11192)
                + &commod_lc
                + __content.string(11194)
                + &trim(g.currency_symbol.as_bytes())
                + &trim(&format(g.futures_batch_gain.abs(), b"* ######"))
                + " "
                + &g.million_prefix_lc
                + "il.";
            if g.futures_batch_gain < Ext::from_f64(0.0) {
                g.news_msg = replace(g.news_msg.as_bytes(), b"gain", b"loss");
            }
            if len(g.ent_symbol[ent_id].as_bytes()) > 17 {
                g.news_msg = remove(g.news_msg.as_bytes(), __content.bytes(11195));
            }
        }
    }
    if !g.news_msg.is_empty() {
        if commod == "STOCKS" && instr(1, g.news_msg.as_bytes(), b"hedging") > 0 {
            g.news_msg = replace(g.news_msg.as_bytes(), b"hedg", b"trad");
        }
        post_news(g, ui, 0, 0);
    }
}

/// Port of SettleFuturesContract (0x625ce5).
/// Closes futures record `rec_idx` at the current futures price: credits the contract value minus the
/// cost basis (adjusted by a 1% closing commission unless `no_commission`, sign-flipped for shorts) to
/// the owner's cash, books the gain net of the 1% opening (and, with commission, a further 1%) charge
/// via [`book_futures_gain_loss`] (`news_mode` as its book mode; `FuturesSettleGain` = the gain),
/// deletes the record, recalculates the owner's margin balance, posts "… [runtime resource wording] …"
/// (not for book mode 0) and recomputes the owner's projected earnings. Nothing happens if the
/// futures price is "ERROR".
///
/// With `allow_delivery = 1`, a long position may become physical holdings (subject to the player's
/// preference and exchange limits); a short may deliver existing physical holdings and settle the
/// remaining gain in cash.
/// Reads the host clock for physical-delivery margin calls.
pub fn settle_futures_contract(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    rec_idx: i32,
    news_mode: i32,
    no_commission: i32,
    allow_delivery: i32,
) {
    let now = ui.local_time();
    settle_futures_contract_at(g, ui, rng, rec_idx, news_mode, no_commission, allow_delivery, &now);
}

/// [`settle_futures_contract`] with the host's clock for cash-deficit and margin-call handling after
/// physical delivery. The scheduled expiry path supplies its current local time here.
pub fn settle_futures_contract_at(
    g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, rec_idx: i32, news_mode: i32,
    no_commission: i32, allow_delivery: i32, now: &crate::logic::newgame::LocalTime,
) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError("SettleFutures", …) (error path not ported)
    let fut_rec = g.futures_recs[rec_idx].clone();
    let commod = parse_any(fut_rec.as_bytes(), b"|", 1);
    let mut commod_label = commod.clone();
    if commod_label == "STOCKS" {
        commod_label = s(__content.string(2499));
    }
    let mut commod_lc = lcase(commod_label.as_bytes(), CaseMode::Ansi);
    if commod == "BITCOIN" || commod == "ETHEREUM" {
        commod_lc = mcase(commod.as_bytes(), CaseMode::Ansi);
    }
    let owner_id = to_long(val(parse_any(fut_rec.as_bytes(), b"|", 2).as_bytes()));
    recalc_commod_margin_balances(g, ui, rng, owner_id);
    let deliv_month = to_long(val(parse_any(fut_rec.as_bytes(), b"|", 4).as_bytes()));
    let deliv_year = to_long(val(parse_any(fut_rec.as_bytes(), b"|", 5).as_bytes()));
    let mut cost_basis = val(parse_any(fut_rec.as_bytes(), b"|", 6).as_bytes());
    let contract_value = calc_futures_contract_value(g, ui, rng, &fut_rec);
    let controller_id = g.corp_controller[owner_id];
    let mut delivered = false;
    if allow_delivery == 1 && commod != "STOCKS" {
        if contract_value > Ext::from_f64(0.0) && 0 < controller_id && controller_id <= g.num_players
            && g.player_take_delivery[controller_id] == 1
        {
            g.cur_commodity = commod.clone();
            g.tmp_ext1 = Ext::from_f64(99.0);
            if calc_commod_trade_limit(g, ui, 0, 1, 20000) == 99 {
                let data = get_commod_data(g, ui, &commod, 8);
                let limit = to_quad(val(parse_any(data.as_bytes(), b"|", 4).as_bytes()));
                let quantity = to_quad(val(parse_any(fut_rec.as_bytes(), b"|", 3).as_bytes()));
                let held = sum_commodity_holdings(g, owner_id, 0, 2);
                if (limit as i128) < quantity as i128 + held as i128 {
                    g.news_msg = s(__content.string(11196)) + &lcase(commod.as_bytes(), CaseMode::Ansi)
                        + " to " + &g.ent_symbol[owner_id] + __content.string(11197);
                    if commod == "BITCOIN" || commod == "ETHEREUM" {
                        g.news_msg = s(__content.string(11198)) + &mcase(commod.as_bytes(), CaseMode::Ansi)
                            + " to " + &g.ent_symbol[owner_id] + __content.string(11197);
                    }
                    post_news(g, ui, owner_id, 0);
                } else {
                    let result = crate::logic::trade_screens::option_misc::take_physical_delivery(g, ui, rng, &fut_rec);
                    if result > 0 {
                        add_or_remove_futures_rec(g, &PbStr::new(), rec_idx);
                        recalc_commod_margin_balances(g, ui, rng, owner_id);
                        if g.cash[owner_id] + g.t_bills[owner_id] < Ext::from_f64(0.0) {
                            cover_cash_deficit(g, ui, rng, owner_id, now);
                            if g.cash[owner_id] + g.t_bills[owner_id] < Ext::from_f64(0.0) {
                                super::mgmt::commodity_margin_call(g, ui, rng, owner_id, now);
                            }
                            if result == 2 {
                                g.news_msg = g.ent_symbol[owner_id].clone()
                                    + __content.string(11199) + &lcase(commod.as_bytes(), CaseMode::Ansi);
                                if commod == "BITCOIN" || commod == "ETHEREUM" {
                                    g.news_msg = g.ent_symbol[owner_id].clone() + " dumps "
                                        + &mcase(commod.as_bytes(), CaseMode::Ansi) + __content.string(11200);
                                }
                                if len(g.news_msg.as_bytes()) > 78 {
                                    g.news_msg = PbStr::new();
                                } else {
                                    post_news(g, ui, owner_id, 0);
                                }
                            }
                        }
                        return;
                    }
                }
            }
        } else if contract_value < Ext::from_f64(0.0)
            && (get_controller_type(g, owner_id) != 1 || g.player_make_delivery[controller_id] == 1)
        {
            g.futures_batch_gain = Ext::from_f64(0.0);
            delivered = deliver_phys_commod_for_short_futures(g, ui, rng, &fut_rec) > 0;
        }
    }
    let open_commission = cost_basis * sf(0.01f32);
    let mut close_commission = Ext::from_f64(0.0);
    if no_commission == 0 {
        close_commission = contract_value.abs() * sf(0.01f32);
    }
    if contract_value > Ext::from_f64(0.0) {
        cost_basis = close_commission + cost_basis;
    } else {
        cost_basis = cost_basis - close_commission;
    }
    if contract_value < Ext::from_f64(0.0) {
        cost_basis = -Ext::from_f64(1.0) * cost_basis;
    }
    let mut gain_loss = contract_value - cost_basis;
    g.cash[owner_id] = gain_loss + g.cash[owner_id];
    gain_loss = gain_loss - open_commission;
    let settle_price_str = get_futures_price(g, ui, rng, &commod, deliv_month, deliv_year, 6);
    if settle_price_str == "ERROR" {
        return;
    }
    if no_commission == 0 {
        gain_loss = gain_loss - ((cost_basis + contract_value) * sf(0.01f32)).abs();
    }
    let book_mode = if delivered { 4 } else { news_mode };
    if delivered && g.futures_batch_gain.abs() > Ext::from_f64(0.0) {
        g.futures_batch_gain += gain_loss;
    }
    g.futures_settle_gain = gain_loss;
    book_futures_gain_loss(g, ui, rng, gain_loss, owner_id, &commod, book_mode);
    g.futures_batch_gain = Ext::from_f64(0.0);
    add_or_remove_futures_rec(g, &PbStr::new(), rec_idx);
    recalc_commod_margin_balances(g, ui, rng, owner_id);
    let mut short_tag = PbStr::new();
    if contract_value < Ext::from_f64(0.0) {
        short_tag = s(__content.string(11201));
    }
    g.news_msg = g.ent_symbol[owner_id].clone()
        + &str_l(cast::<Ext, _>(deliv_month))
        + "/"
        + &trim(str_l(cast::<Ext, _>(deliv_year)).as_bytes())
        + " "
        + &short_tag
        + &commod_lc
        + __content.string(11202)
        + &settle_price_str
        + ".";
    if book_mode == 0 || delivered {
        g.news_msg = PbStr::new();
    }
    if len(g.news_msg.as_bytes()) > 75 || owner_id < 11 || g.corp_controller[owner_id] < 11 {
        g.news_msg = replace(g.news_msg.as_bytes(), __content.bytes(11203), b"");
        if len(g.news_msg.as_bytes()) > 75 {
            g.news_msg = PbStr::new();
        }
    }
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    let _ = calc_projected_earnings(g, ui, rng, owner_id);
}

/// Port of LiquidateFuturesPositions (0x62e730) with its GOSUB LiquidateFutures_CloseOne (0x62f3a6).
/// Closes futures positions of `entity_id` (not banks 11..85, active entities only, not funds advised
/// by a human or the three index funds) and returns the number of records closed/changed (0 on the
/// early exits). `liq_mode` 1/2: settles every contract (1: stops once cash + T-bills is no longer
/// negative, moving T-bills to cash); 3: only for AI-managed hedging industries (insurers in 1 of 3
/// calls), closes small positions (< 5 contracts) and trims hedges; 4: closes small positions and
/// trims hedges to 97%. Hedge trimming keeps 70% + 0.5% per month to delivery (50% within 6 months),
/// books the gain on the sold part (always if positive, otherwise in every other quarter) and rewrites
/// the record. `news_mode` is passed on to the settlement. Packs the holding arrays at the end.
pub fn liquidate_futures_positions(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    entity_id: i32,
    liq_mode: i32,
    news_mode: i32,
) -> i32 {
    // ON ERROR GOTO -> ShowRuntimeError("CashInFutures", …) (error path not ported)
    let ent_id = entity_id;
    let mut closed_count = 0;
    if 10 < ent_id && ent_id < 86 {
        return 0;
    }
    if g.entity_status[ent_id] != 1 {
        return 0;
    }
    if get_etf_adviser_human_status(g, ui, rng, ent_id) > 0 && 1585 < ent_id {
        return 0;
    }
    if 1580 < ent_id && ent_id < 1584 && g.industry_num[ent_id] == 71 {
        return 0;
    }
    let news_mode_val = news_mode;
    if liq_mode == 3 {
        if get_managing_player(g, ent_id) > 0 {
            return 0;
        }
        let industry = g.industry_num[ent_id];
        if !matches!(
            industry,
            2 | 7 | 8 | 11 | 15 | 16 | 23 | 25 | 28 | 37 | 50 | 52 | 69
        ) {
            return 0;
        }
        if g.rand_seed % 3 > 0 && g.industry_num[ent_id] == 2 {
            return 0;
        }
    }
    // locals kept across records like the original's frame
    let mut keep_frac: Ext = crate::pb::numeric::double(Ext::from_f64(0.0)); // DOUBLE
    let mut rec_idx = 1;
    // GOSUB LiquidateFutures_CloseOne
    let close_one =
        |g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, rec_idx: i32, closed: &mut i32| {
            let no_commission = if liq_mode == 3 { 1 } else { 0 };
            settle_futures_contract(g, ui, rng, rec_idx, news_mode_val, no_commission, 0);
            *closed += 1;
        };
    'recs: loop {
        let fut_rec = g.futures_recs[rec_idx].clone();
        if val(parse_any(fut_rec.as_bytes(), b"|", 2).as_bytes()) == cast::<Ext, _>(ent_id) {
            let commod_name = parse_any(fut_rec.as_bytes(), b"|", 1);
            let qty = to_long(val(parse_any(fut_rec.as_bytes(), b"|", 3).as_bytes()));
            let mut skip_rest = false;
            if liq_mode < 3 || (cast::<Ext, _>(qty)).abs() < Ext::from_f64(5.0) {
                close_one(g, ui, rng, rec_idx, &mut closed_count);
                rec_idx -= 1;
                if liq_mode == 3 {
                    skip_rest = true;
                }
            }
            if !skip_rest {
                if liq_mode == 1 && !(g.t_bills[ent_id] + g.cash[ent_id] < Ext::from_f64(0.0)) {
                    if g.cash[ent_id] < Ext::from_f64(0.0) {
                        balance_cash_t_bills(g, ent_id);
                    }
                    break 'recs;
                }
                if liq_mode >= 3 {
                    let mut is_hedge = 0;
                    match g.industry_num[ent_id] {
                        7 | 8 | 11 | 16 | 23 | 25 | 52 | 69 => {
                            if commod_name == "OIL" {
                                is_hedge = 1;
                            }
                        }
                        15 | 28 => {
                            if commod_name == "WHEAT" || commod_name == "CORN" {
                                is_hedge = 1;
                            }
                        }
                        2 | 37 | 71 => {
                            if commod_name == "STOCKS" {
                                is_hedge = 1;
                            }
                        }
                        50 => {
                            if commod_name == "GOLD" || commod_name == "SILVER" {
                                is_hedge = 1;
                            }
                        }
                        _ => {}
                    }
                    if is_hedge != 0 {
                        let sim = g.sim_co_count;
                        let month_in_qtr: Ext = crate::pb::numeric::double(calc_cur_month_of_year(g, sim)); // DOUBLE
                        let cur_month = to_long(month_in_qtr.ceil());
                        let deliv_year =
                            to_long(val(parse_any(fut_rec.as_bytes(), b"|", 5).as_bytes()));
                        let cost_basis = val(parse_any(fut_rec.as_bytes(), b"|", 6).as_bytes());
                        if g.cur_year == deliv_year && cost_basis < g.exchange_rate * Ext::from_f64(500.0) {
                            close_one(g, ui, rng, rec_idx, &mut closed_count);
                            rec_idx -= 1;
                        } else {
                            let deliv_month =
                                to_long(val(parse_any(fut_rec.as_bytes(), b"|", 4).as_bytes()));
                            let mut contract_price = Cux::default();
                            contract_price
                                .set(val(parse_any(fut_rec.as_bytes(), b"|", 7).as_bytes()));
                            let price_str = get_futures_price(
                                g,
                                ui,
                                rng,
                                &commod_name,
                                deliv_month,
                                deliv_year,
                                4,
                            );
                            if price_str == "ERROR" {
                                return 0;
                            }
                            let months_to_deliv =
                                (deliv_year - g.cur_year) * 12 + (deliv_month - cur_month);
                            let haircut_pct = cast::<Ext, _>(months_to_deliv) * sf(0.5f32);
                            let mut keep_qty =
                                to_long((haircut_pct * sf(0.01f32) + sf(0.7f32)) * cast::<Ext, _>(qty));
                            if months_to_deliv < 6 {
                                keep_qty = to_long(cast::<Ext, _>(qty) * sf(0.5f32));
                            }
                            if liq_mode == 4 {
                                keep_qty = to_long(cast::<Ext, _>(qty) * sf(0.97f32));
                            }
                            if qty == keep_qty {
                                if qty > 0 {
                                    keep_qty = qty - 1;
                                } else if qty < 0 {
                                    keep_qty = qty + 1;
                                }
                            }
                            if qty != 0 {
                                keep_frac = crate::pb::numeric::double(cast::<Ext, _>(keep_qty) / cast::<Ext, _>(qty));
                            }
                            let sold_cost = (Ext::from_f64(1.0) - keep_frac) * cost_basis;
                            let cur_value = calc_futures_contract_value(g, ui, rng, &fut_rec).abs();
                            let mut gain_loss = if qty < 0 {
                                sold_cost - (Ext::from_f64(1.0) - keep_frac) * cur_value
                            } else {
                                (Ext::from_f64(1.0) - keep_frac) * cur_value - sold_cost
                            };
                            if gain_loss > Ext::from_f64(0.0) || (g.cur_quarter + ent_id) % 2 != 1 {
                                g.cash[ent_id] = gain_loss + g.cash[ent_id];
                                gain_loss = gain_loss
                                    - ((Ext::from_f64(1.0) - keep_frac) * cur_value + sold_cost) * sf(0.01f32);
                                book_futures_gain_loss(
                                    g,
                                    ui,
                                    rng,
                                    gain_loss,
                                    ent_id,
                                    &commod_name,
                                    0,
                                );
                                closed_count = 1;
                                g.futures_recs[rec_idx] = commod_name.clone()
                                    + "|"
                                    + &trim(str_l(cast::<Ext, _>(ent_id)).as_bytes())
                                    + "|"
                                    + &trim(str_l(cast::<Ext, _>(keep_qty)).as_bytes())
                                    + "|"
                                    + &trim(str_l(cast::<Ext, _>(deliv_month)).as_bytes())
                                    + "|"
                                    + &trim(str_l(cast::<Ext, _>(deliv_year)).as_bytes())
                                    + "|"
                                    + &trim(str_l(cost_basis - sold_cost).as_bytes())
                                    + "|"
                                    + &trim(str_l(contract_price.get()).as_bytes());
                            }
                        }
                    }
                }
            }
        }
        rec_idx += 1;
        if rec_idx > 1000 {
            break;
        }
    }
    pack_holdings(g, ui);
    closed_count
}
