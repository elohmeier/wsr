//! Short sales of players: value of the short positions (`CalcShortPositionValue`), the short-sale
//! margin balances (`RecalcShortMarginBalances`) and the scan for short positions / short squeezes
//! (`FindShortPositions`).
//!
//! A short position is a stock-holding slot with a negative `HoldStockPct` (players 1..NumPlayers only);
//! its value is `MarketCap(corp) * |pct|`. `PlayerShortMarginBal(p)` holds the value of player p's shorts,
//! which is kept out of `Cash(p)`.

// Statements keep the original BASIC shape on purpose.
#![allow(clippy::manual_range_contains, clippy::assign_op_pattern, clippy::collapsible_match)]
use crate::pb::real::{Ext, cast};


use crate::logic::sim_entity::mgmt::force_cover_shorts;
use crate::pb::array::Arr1;
use crate::pb::rng::Rng;
use crate::pb::numeric::{int, to_long};
use crate::state::Globals;
use crate::ui::Ui;

/// Port of CalcShortPositionValue (0x5e0cc9).
/// Sum of `MarketCap(corp) * |HoldStockPct|` over `player_id`'s stock slots 1..15 with a nonzero corp
/// and a negative percentage (`player_id` is BYVAL).
pub fn calc_short_position_value(g: &Globals, player_id: i32) -> Ext {
    // ON ERROR GOTO handler: ShowRuntimeError only
    let mut total_value: Ext = Ext::from_f64(0.0);
    for slot in 1..=15 {
        if g.hold_stock_id[(slot, player_id)] == 0 || g.hold_stock_pct[(slot, player_id)] >= 0 {
            continue;
        }
        let corp_id = cast::<i32, _>(g.hold_stock_id[(slot, player_id)]);
        let pct = to_long((cast::<Ext, _>(g.hold_stock_pct[(slot, player_id)])).abs());
        total_value = g.market_cap[corp_id] * cast::<Ext, _>(pct) + total_value;
    }
    total_value
}

/// Port of RecalcShortMarginBalances (0x5e0ade).
/// For every player 1..NumPlayers not named `"...OUT..."`: sets `PlayerShortMarginBal(p)` to the current
/// value of the short positions and credits the change to `Cash(p)` (`Cash += old - new`).
pub fn recalc_short_margin_balances(g: &mut Globals) {
    // ON ERROR GOTO handler: ShowRuntimeError only
    let n = g.num_players;
    for player_id in 1..=n {
        if g.player_name[player_id] == "...OUT..." {
            continue;
        }
        let value = calc_short_position_value(g, player_id);
        if value != g.player_short_margin_bal[player_id] {
            let delta = g.player_short_margin_bal[player_id] - value;
            g.cash[player_id] = delta + g.cash[player_id];
            g.player_short_margin_bal[player_id] = value;
        }
    }
}

/// Locals of [`find_short_positions`] shared with its GOSUBs.
struct Frame {
    corp_id: i32,
    player_id: i32,
    mode: i32,
    match_code: i32,
    squeeze_player: i32,
    squeeze_corp: i32,
    squeeze_pct: i32,
    short_pct_by_corp: Arr1<i32>,
}

/// GOSUB FindShortPositions_Tally (0x5df95b): sums the short percentages per corp over all players' slots
/// (`ShortPosFlag = 1` if there is any), notes a player short more than 20% of a corp (squeeze candidate,
/// the excess over 20%), and sets `match_code` to the mode when a short matches the query.
fn tally(g: &mut Globals, f: &mut Frame) {
    f.short_pct_by_corp.reset();
    let n = g.num_players;
    for holder_idx in 1..=n {
        for slot in 1..=15 {
            if !(g.hold_stock_id[(slot, holder_idx)] > 10 && g.hold_stock_pct[(slot, holder_idx)] < 0) {
                continue;
            }
            g.short_pos_flag = 1;
            let short_corp = cast::<i32, _>(g.hold_stock_id[(slot, holder_idx)]);
            let short_pct = to_long((cast::<Ext, _>(g.hold_stock_pct[(slot, holder_idx)])).abs());
            if short_pct > 20 {
                f.squeeze_player = holder_idx;
                f.squeeze_corp = short_corp;
                f.squeeze_pct = short_pct - 20;
            }
            f.short_pct_by_corp[short_corp] = short_pct + f.short_pct_by_corp[short_corp];
            match f.mode {
                0 => f.match_code = 1,
                2 => {
                    if f.corp_id == short_corp {
                        f.match_code = 2;
                    }
                }
                3 => {
                    if f.player_id == holder_idx {
                        f.match_code = 3;
                    }
                }
                4 => {
                    if f.player_id == holder_idx && f.corp_id == short_corp {
                        f.match_code = 4;
                    }
                }
                _ => {}
            }
        }
    }
}

/// GOSUB FindShortPositions_CheckFloat (0x5dfbb1): for every shorted corp, the float is 100% minus the
/// positive holdings of active entities (an INTEGER, wrapping); if more than `INT(float / 2)` percent is
/// short, the excess is covered by `ForceCoverShorts(2, 0, corp, excess)`.
fn check_float(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    for short_corp in 11..=1600 {
        if f.short_pct_by_corp[short_corp] == 0 {
            continue;
        }
        let mut float_pct: i16 = 100;
        for holder_idx in 1..=1600 {
            if g.entity_status[holder_idx] != 1 {
                continue;
            }
            for slot in 1..=15 {
                if cast::<i32, _>(g.hold_stock_id[(slot, holder_idx)]) == short_corp && g.hold_stock_pct[(slot, holder_idx)] > 0 {
                    float_pct = float_pct.wrapping_sub(g.hold_stock_pct[(slot, holder_idx)]);
                }
            }
        }
        let max_short_pct = to_long(int(cast::<Ext, _>(float_pct) * (cast::<Ext, _>(0.5f32))));
        if f.short_pct_by_corp[short_corp] > max_short_pct {
            let excess_pct = f.short_pct_by_corp[short_corp] - max_short_pct;
            force_cover_shorts(g, ui, rng, 2, 0, short_corp, excess_pct);
        }
    }
}

/// Port of FindShortPositions (0x5df7c4) with its GOSUBs FindShortPositions_Tally (0x5df95b) and
/// FindShortPositions_CheckFloat (0x5dfbb1).
/// Query (`corp_id > 10` and/or `player_id > 0`, both BYVAL): returns 2 / 3 / 4 if some player is short
/// `corp_id` / player `player_id` is short anything / `player_id` is short `corp_id`, else -2 / -3 / -4.
/// Without a query (mode 0): returns 1 if any player is short (0 if none) and enforces the limits: a
/// player short more than 20% of a corp is squeezed (`ForceCoverShorts(4, …)`), then shorts above half the
/// float are force-covered (`ForceCoverShorts(2, …)`), and the margins are recalculated.
/// `ShortPosFlag` is -1, or 1 if any short exists. `RecalcShortMarginBalances` runs first in all modes.
pub fn find_short_positions(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, corp_id: i32, player_id: i32) -> i32 {
    // ON ERROR GOTO handler: ShowRuntimeError only
    let mut f = Frame {
        corp_id,
        player_id,
        mode: 0,
        match_code: 0,
        squeeze_player: 0,
        squeeze_corp: 0,
        squeeze_pct: 0,
        short_pct_by_corp: Arr1::new(11, 1600),
    };
    if corp_id > 10 {
        f.mode = 2;
        f.match_code = -2;
    }
    if player_id > 0 {
        f.mode = 3;
        f.match_code = -3;
    }
    if corp_id > 10 && player_id > 0 {
        f.mode = 4;
        f.match_code = -4;
    }
    g.short_pos_flag = -1;
    recalc_short_margin_balances(g);
    tally(g, &mut f);
    if f.match_code == 0 {
        return 0;
    }
    let result = f.match_code;
    if f.mode > 0 {
        return result;
    }
    if f.squeeze_player > 0 {
        let player_count = g.num_players;
        for holder_idx in 1..=player_count {
            if f.squeeze_player > 0 && f.squeeze_player == holder_idx {
                force_cover_shorts(g, ui, rng, 4, f.squeeze_player, f.squeeze_corp, f.squeeze_pct);
            }
        }
        f.squeeze_player = 0;
        f.squeeze_corp = 0;
        f.squeeze_pct = 0;
        tally(g, &mut f);
    }
    check_float(g, ui, rng, &mut f);
    recalc_short_margin_balances(g);
    result
}
