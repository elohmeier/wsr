//! Callees of the per-entity simulation (see [`super`]): management changes (`CheckMgmtChange`), forced
//! covering of short sales (`ForceCoverShorts`) and the commodity-futures margin call
//! (`CommodityMarginCall`).
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use crate::logic::earnings::{calc_projected_earnings, sell_t_bills_to_cash};
use crate::logic::economy::close_all_dialogs;
use crate::logic::finance::get_controller_type;
use crate::logic::futures::recalc_commod_margin_balances;
use crate::logic::holdings::find_holding_slot;
use crate::logic::report::show_quick_message;
use crate::logic::reports_lists::show_futures_positions;
use crate::logic::shorts::{find_short_positions, recalc_short_margin_balances};
use crate::logic::sim_entity::quarter_start::pack_holding_arrays;
use crate::logic::sim_entity::transactions::liquidate_futures_positions;
use crate::logic::commod_ai::desk::{futures_buy_or_cover, futures_sell_or_short};
use crate::logic::newgame::LocalTime;
use crate::logic::trading::select_acting_entity;
use crate::logic::valuation::{price_trend_streak, update_price_history_tick};

/// Port of CheckMgmtChange (0x4ef5eb).
/// Pops the next corp code (`"nnnn/"`) off `NewMgmtList` and rates the new management of that active
/// non-fund corp: ProfitMargin moves by -15/0/+15 (biased by the margin, `RandSeed` and a controlling
/// player), or a repeat change within `RecentMgmtChangeList` costs 5 points of margin and 10% of the
/// market cap; maintains `RecentMgmtChangeList` and posts the verdict (pop-up for the controlling player).
pub fn check_mgmt_change(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError("  CoCount # " + STR$(corpId), "CheckMgmtChange") and exit
    let corp_code = mid(g.new_mgmt_list.as_bytes(), 1, 4);
    let mut list_len = len(g.new_mgmt_list.as_bytes());
    if list_len > 5 {
        let slash_pos = instr(1, g.new_mgmt_list.as_bytes(), b"/");
        g.new_mgmt_list = right(g.new_mgmt_list.as_bytes(), list_len - slash_pos);
    } else {
        g.new_mgmt_list = PbStr::new();
    }
    let corp_id = to_long(val(corp_code.as_bytes()));
    if corp_id < 11 {
        return;
    }
    if corp_id > 1600 {
        return; // (error 9 in EntSymbol(corpId) -> ShowRuntimeError, exit)
    }
    let corp_sym = g.ent_symbol[corp_id].clone();
    if g.industry_num[corp_id] > 69 || g.entity_status[corp_id] != 1 {
        return;
    }
    let mut repeat_corp = 0;
    if instr(
        1,
        g.recent_mgmt_change_list.as_bytes(),
        corp_code.as_bytes(),
    ) > 0
    {
        repeat_corp = corp_id;
    }
    list_len = len(g.recent_mgmt_change_list.as_bytes());
    if len(g.new_mgmt_list.as_bytes()) > list_len {
        g.recent_mgmt_change_list = g.recent_mgmt_change_list.clone() + "0000/0000/";
    } else if list_len > 5 {
        let slash_pos = instr(1, g.recent_mgmt_change_list.as_bytes(), b"/");
        g.recent_mgmt_change_list =
            right(g.recent_mgmt_change_list.as_bytes(), list_len - slash_pos);
    } else if list_len == 5 {
        g.recent_mgmt_change_list = PbStr::from("0000/");
    }
    if 0 < g.corp_controller[corp_id] && g.corp_controller[corp_id] < 11 {
        if len(g.recent_mgmt_change_list.as_bytes()) < 101 {
            g.recent_mgmt_change_list = g.recent_mgmt_change_list.clone() + &corp_code + "/";
        }
    }
    let mut outcome: i32;
    if repeat_corp == 0 {
        outcome = rng.rnd_range(3, 1);
        outcome = outcome - 2;
        if g.profit_margin[corp_id] < -12 && g.rand_seed % 7 == 1 {
            outcome = 1;
        }
        if g.profit_margin[corp_id] > 7 && g.rand_seed % 4 == 1 {
            outcome = -1;
        }
        if get_controller_type(g, corp_id) == 2 {
            if Ext::from_f64(7.0) * rng.rnd() < Ext::from_f64(1.0) {
                outcome = 1;
            }
        }
        g.profit_margin[corp_id] =
            cast::<i16, _>(15i32.wrapping_mul(outcome) + cast::<i32, _>(g.profit_margin[corp_id]));
    } else {
        g.profit_margin[corp_id] = cast::<i16, _>(cast::<i32, _>(g.profit_margin[corp_id]) - 5);
        g.market_cap[corp_id] = sf(0.9f32) * g.market_cap[corp_id];
        update_price_history_tick(g, ui, rng, corp_id);
        outcome = -1;
    }
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    let mut verdict = PbStr::from(__content.string(11017));
    if outcome == -1 && g.profit_margin[corp_id] < -5 {
        verdict = PbStr::from(__content.string(11018));
    }
    g.news_msg = PbStr::from(__content.string(1857)) + &corp_sym + " is " + &verdict;
    if outcome == 1 {
        g.news_msg =
            PbStr::from(__content.string(11019)) + &corp_sym + __content.string(11020);
    }
    if repeat_corp > 10 && g.profit_margin[corp_id] < -7 {
        g.news_msg = PbStr::from("Latest ") + &corp_sym + __content.string(11021);
    }
    if g.cur_player == g.corp_controller[corp_id] {
        let mut m = g.news_msg.clone();
        show_quick_message(g, ui, &mut m);
        g.news_msg = m;
        post_news(g, ui, 0, 0);
    } else {
        post_news(g, ui, corp_id, 0);
    }
    if outcome != 0 {
        calc_projected_earnings(g, ui, rng, corp_id);
    }
}

/// Locals of [`force_cover_shorts`] shared with its GOSUB ForceCoverShorts_CoverOnePct.
struct CoverFrame {
    mode: i32,
    /// 1 = each covered percent also lifts the price 0.5% (short squeeze).
    squeeze_flag: i32,
    cover_corp_id: i32,
    holder_id: i32,
    slot: i32,
    /// DOUBLE: realized gain/loss of all covered percents.
    total_gain: Ext,
}

/// GOSUB ForceCoverShorts_CoverOnePct (0x5e0727): covers one percent of the short holding `slot` of
/// `holder_id` in `cover_corp_id` at the market cap (after the price effects), charging the short margin
/// balance and booking the gain/loss as capital gain.
fn cover_one_pct(g: &mut Globals, f: &mut CoverFrame) {
    let (slot, holder) = (f.slot, f.holder_id);
    if g.hold_stock_pct[(slot, holder)] >= 0 {
        return;
    }
    let c = f.cover_corp_id;
    if f.mode != 1 {
        g.market_cap[c] = sf(1.005f32) * g.market_cap[c];
        g.investor_sentiment[c] = 1 + g.investor_sentiment[c];
    }
    if f.squeeze_flag == 1 {
        g.market_cap[c] = sf(1.005f32) * g.market_cap[c];
    }
    let cover_cost = g.market_cap[c];
    g.player_short_margin_bal[holder] = g.player_short_margin_bal[holder] - cover_cost;
    let basis_part: Ext;
    if (cast::<Ext, _>(g.hold_stock_pct[(slot, holder)])).abs() > Ext::from_f64(1.0) {
        basis_part =
            g.hold_stock_basis[(slot, holder)] / (cast::<Ext, _>(g.hold_stock_pct[(slot, holder)])).abs();
        g.hold_stock_basis[(slot, holder)] = g.hold_stock_basis[(slot, holder)] - basis_part;
    } else {
        basis_part = g.hold_stock_basis[(slot, holder)];
        g.hold_stock_id[(slot, holder)] = 0;
        g.hold_stock_basis[(slot, holder)] = Ext::from_f64(0.0);
    }
    g.hold_stock_pct[(slot, holder)] = g.hold_stock_pct[(slot, holder)].wrapping_add(1);
    let gain_loss = basis_part - cover_cost;
    f.total_gain = crate::pb::numeric::double(gain_loss + f.total_gain);
    g.player_cap_gains[holder] = gain_loss + g.player_cap_gains[holder];
}

/// Port of ForceCoverShorts (0x5dfe3a) with its GOSUB ForceCoverShorts_CoverOnePct (0x5e0727).
/// Forced covering of players' short sales in `short_corp_id` (percent by percent at the market cap):
/// mode 1 covers all shorts of every player (or `target_player`) before a transaction; mode 2 is a short
/// squeeze covering `pct_count` percents of random players short more than 20%; mode 3 covers one percent
/// of `target_player`'s first short position (margin call); mode 4 covers `pct_count` percents of
/// `target_player`'s short and leaves the realized gain in `TmpExt1`; mode 5 closes the shorts at a 100%
/// gain (the stock became worthless). Each covered percent lifts the price (except mode 1).
pub fn force_cover_shorts(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    mode: i32,
    target_player: i32,
    short_corp_id: i32,
    pct_count: i32,
) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError("", "ForceCoverShorts") and exit
    let corp_id = short_corp_id;
    let mut f = CoverFrame {
        mode,
        squeeze_flag: 1,
        cover_corp_id: 0,
        holder_id: 0,
        slot: 0,
        total_gain: crate::pb::numeric::double(Ext::from_f64(0.0)),
    };
    match mode {
        1 => {
            f.squeeze_flag = 0;
            f.cover_corp_id = corp_id;
            let n = g.num_players;
            for idx in 1..=n {
                if target_player > 0 && target_player != idx {
                    continue;
                }
                let mut covered = false;
                for slot_idx in 1..=15 {
                    if cast::<i32, _>(g.hold_stock_id[(slot_idx, idx)]) == corp_id
                        && g.hold_stock_pct[(slot_idx, idx)] < 0
                    {
                        f.holder_id = idx;
                        f.slot = slot_idx;
                        loop {
                            cover_one_pct(g, &mut f);
                            if g.hold_stock_pct[(slot_idx, idx)] == 0 {
                                break;
                            }
                        }
                        covered = true;
                    }
                }
                if covered {
                    g.news_msg = g.ent_symbol[idx].clone()
                        + __content.string(11022)
                        + &g.ent_symbol[corp_id]
                        + __content.string(11023);
                    post_news(g, ui, f.cover_corp_id, idx);
                }
                recalc_short_margin_balances(g);
            }
        }
        2 => {
            let mut pct_left = pct_count;
            let mut tries = 0;
            loop {
                let rand_player = rng.rnd_range(g.num_players, 1);
                if find_short_positions(g, ui, rng, corp_id, rand_player) == 4 {
                    f.cover_corp_id = corp_id;
                    f.holder_id = rand_player;
                    f.slot = find_holding_slot(g, rand_player, corp_id);
                    cover_one_pct(g, &mut f);
                    pct_left = pct_left.wrapping_sub(1);
                    g.news_msg = g.ent_symbol[f.holder_id].clone()
                        + __content.string(11024)
                        + &g.ent_symbol[corp_id]
                        + __content.string(11025);
                    if !(g.news_headline[59] == g.news_msg) && !(g.news_headline[60] == g.news_msg)
                    {
                        post_news(g, ui, corp_id, f.holder_id);
                    }
                }
                tries += 1;
                if tries == 999 || pct_left == 0 {
                    break;
                }
            }
        }
        3 => {
            f.holder_id = target_player;
            let holder = f.holder_id;
            let mut idx = 1;
            while idx <= 15 {
                if g.hold_stock_id[(idx, holder)] != 0 && g.hold_stock_pct[(idx, holder)] < 0 {
                    f.cover_corp_id = cast::<i32, _>(g.hold_stock_id[(idx, holder)]);
                    f.slot = idx;
                    cover_one_pct(g, &mut f);
                    recalc_short_margin_balances(g);
                    if g.hold_stock_pct[(idx, holder)] == 0 {
                        g.news_msg = g.ent_symbol[holder].clone()
                            + __content.string(11026)
                            + &g.ent_symbol[f.cover_corp_id]
                            + __content.string(11027);
                        post_news(g, ui, f.cover_corp_id, holder);
                    } else {
                        g.news_msg = g.ent_symbol[holder].clone()
                            + __content.string(11028)
                            + &g.ent_symbol[f.cover_corp_id]
                            + __content.string(11029);
                    }
                    idx = 15;
                }
                idx += 1;
            }
        }
        4 => {
            if !(price_trend_streak(g, corp_id) > 0) {
                f.squeeze_flag = 0;
            }
            g.tmp_ext1 = Ext::from_f64(0.0);
            f.holder_id = target_player;
            f.cover_corp_id = corp_id;
            f.slot = find_holding_slot(g, f.holder_id, corp_id);
            for _ in 1..=pct_count {
                cover_one_pct(g, &mut f);
            }
            recalc_short_margin_balances(g);
            g.tmp_ext1 = f.total_gain;
        }
        5 => {
            // coverCorpId = shortCorpId (unused here)
            let n = g.num_players;
            for idx in 1..=n {
                if g.player_name[idx] == "...OUT..." {
                    continue;
                }
                if target_player > 0 && idx != target_player {
                    continue;
                }
                let holder = idx;
                let slot = find_holding_slot(g, holder, corp_id);
                if slot > 0 && cast::<i32, _>(g.hold_stock_id[(slot, holder)]) == corp_id {
                    g.player_cap_gains[holder] =
                        g.hold_stock_basis[(slot, holder)] + g.player_cap_gains[holder];
                    g.hold_stock_id[(slot, holder)] = 0;
                    g.hold_stock_pct[(slot, holder)] = 0;
                    g.hold_stock_basis[(slot, holder)] = Ext::from_f64(0.0);
                    recalc_short_margin_balances(g);
                    g.news_msg = g.ent_symbol[holder].clone()
                        + __content.string(11030)
                        + &g.ent_symbol[corp_id]
                        + __content.string(11031);
                    post_news(g, ui, corp_id, holder);
                }
            }
        }
        _ => {}
    }
}

/// Port of CommodityMarginCall (0x632988).
/// Margin call on an overdrawn entity holding futures: sells T-bills first; if still overdrawn
/// (below -0.00001) asks (YES/NO when it also has stocks to sell, `TmpSubsidStockValue > 0`) and lists
/// its futures positions to pick one to close; without a valid pick all its [runtime resource wording]
/// (`LiquidateFuturesPositions`, news) and the result is 1, else 0. A picked position is closed
/// (`FuturesBuyOrCover` / `FuturesSellOrShort`, mode 2) and the call repeats while the entity is still
/// overdrawn with futures margin. `now`: the local time (for `SelectActingEntity`).
pub fn commodity_margin_call(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    entity_id: i32,
    now: &LocalTime,
) -> i32 {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError(FUNCNAME$) and exit
    let ent_id = entity_id;
    loop {
        if g.cash[ent_id] < Ext::from_f64(0.0) && g.t_bills[ent_id] > Ext::from_f64(0.0) {
            let amount = g.cash[ent_id].abs();
            sell_t_bills_to_cash(g, ent_id, amount);
        }
        if g.cash[ent_id] >= -sf(1e-5f32) || g.commod_margin_bal[ent_id] == Ext::from_f64(0.0) {
            return 0;
        }
        if g.tmp_subsid_stock_value > Ext::from_f64(0.0) {
            let msg = PbStr::from(
                __content.string(11032),
            ) + "\r"
                + "\r"
                + __content.string(11033)
                + __content.string(11034)
                + &trim(g.currency_symbol.as_bytes())
                + &fmt_trim(g.cash[ent_id].abs(), b"* ######,.00")
                + " "
                + &g.million_prefix_lc
                + "illion"
                + &g.currency_suffix
                + "."
                + "\r"
                + "\r"
                + __content.string(11035);
            let answer = ui.msgbox(&msg, 0x2024, &PbStr::from(__content.string(11036)));
            if answer == 7 {
                return 0;
            }
        } else {
            let msg = PbStr::from(
                __content.string(11037),
            ) + "\r"
                + "\r"
                + __content.string(11038);
            ui.msgbox(
                &msg,
                0x2010,
                &PbStr::from(__content.string(11039)),
            );
        }
        g.commod_sel = 0;
        g.cur_commodity = PbStr::new();
        // (UI) DIALOG END hDlgTransMenu
        g.action_menu_open = 0;
        pack_holding_arrays(g, ui);
        close_all_dialogs(g, ui, rng, 1);
        select_acting_entity(g, ui, rng, 1, now);
        // (UI) DisableMainControls
        let active = g.active_entity;
        let rec_idx = show_futures_positions(g, ui, rng, active, 4);
        if rec_idx < 1 || val(g.futures_recs[0].as_bytes()) < cast::<Ext, _>(rec_idx) {
            // [runtime resource wording] ll futures positions
            let n = liquidate_futures_positions(g, ui, rng, ent_id, 1, 1);
            if n > 0 {
                g.news_msg = g.ent_name[ent_id].clone() + __content.string(10471);
                if n > 1 {
                    g.news_msg = g.news_msg.clone() + "s";
                }
                g.news_msg = g.news_msg.clone() + ".";
                post_news(g, ui, 0, 0);
            }
            // (UI) EnableMainControls
            return 1;
        }
        if val(parse_any(g.futures_recs[rec_idx].as_bytes(), b"|", 2).as_bytes()) != cast::<Ext, _>(ent_id) {
            // (UI) EnableMainControls
            return 0;
        }
        let msg = g.futures_recs[rec_idx].clone();
        g.cur_commodity = trim(parse_any(msg.as_bytes(), b"|", 1).as_bytes());
        if val(parse_any(msg.as_bytes(), b"|", 3).as_bytes()) < Ext::from_f64(0.0) {
            futures_buy_or_cover(g, ui, rng, 2, ent_id, rec_idx);
        } else {
            futures_sell_or_short(g, ui, rng, 2, ent_id, rec_idx);
        }
        // (UI) EnableMainControls
        recalc_commod_margin_balances(g, ui, rng, ent_id);
        if g.commod_margin_bal[ent_id] == Ext::from_f64(0.0) || !(g.cash[ent_id] < Ext::from_f64(0.0)) {
            return 0;
        }
    }
}
