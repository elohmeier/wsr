//! Executive options and corporate option positions: `GrantExecutiveOptions`, `VestExecutiveOptions`,
//! `AiCorpOptionsTrading`, `ForceInsiderShortCover`, `ForceCloseDerivativesBadCredit` and
//! `ForceCloseExcessNakedOptions`.
//!
//! Option records (`OptionRecs()`): `corp|type (1 call, 2 put)|holder|pct (negative = written)|strike
//! (MarketCap units)|exp month|exp year|premium` — executive options carry `GRANT` (unvested) or
//! `VESTED` in the last field.
//!
//! Error handling: the ON ERROR handlers only call ShowRuntimeError (0x46ea48) and leave the routine;
//! the port leaves the routine without UI at the same points.

// Statements keep the original BASIC shape on purpose.
#![allow(
    clippy::assign_op_pattern,
    clippy::manual_range_contains,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::if_same_then_else
)]
use crate::pb::real::{Ext, cast};


use crate::logic::control::{find_option_positions, update_corp_control};
use crate::logic::finance::{calc_entity_financials, get_controller_type, get_managing_player};
use crate::logic::futures::recalc_commod_margin_balances;
use crate::logic::holdings::find_holding_slot;
use crate::logic::options::{
    calc_option_price, check_naked_option_margin, close_option_positions,
    get_net_stock_pct_after_options, get_optioned_stock_pct,
};
use crate::logic::report::show_quick_message;
use crate::logic::sim_entity::etf::ai_create_option_position;
use crate::logic::sim_entity::mgmt::force_cover_shorts;
use crate::logic::sim_entity::prelude::*;
use crate::logic::sim_entity::quarter_start::revoke_ex_ceo_options;
use crate::logic::sim_entity::transactions::liquidate_futures_positions;
use crate::logic::valuation::{calc_analyst_rating, check_stock_split};
use crate::pb::numeric::{Ext80, str_num_ext};

/// `VAL(PARSE$(rec, ANY "|", n))`.
fn fval(rec: &PbStr, n: i32) -> Ext {
    val(parse_any(rec.as_bytes(), b"|", n).as_bytes())
}

/// `RTRIM$(LTRIM$(s))`.
fn trim(s: &[u8]) -> PbStr {
    rtrim(ltrim(s).as_bytes())
}

/// `STR$(x)` (15 digits).
fn str15(x: Ext) -> PbStr {
    PbStr::from(str_num(x, 15).as_str())
}

/// Port of ForceInsiderShortCover (0x5e319d).
/// Every short position (negative `HoldStockPct`) of an active player in a company that player
/// controls (`VotingController` after [`update_corp_control`]) is covered in full
/// (`ForceCoverShorts` mode 4) with an "insider forced to cover" headline.
pub fn force_insider_short_cover(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    let n = g.num_players;
    let mut player_id = 1;
    while player_id <= n {
        if g.player_name[player_id] != "...OUT..." {
            for slot in 1..=15 {
                if g.hold_stock_pct[(slot, player_id)] < 0 {
                    let corp_id = cast::<i32, _>(g.hold_stock_id[(slot, player_id)]);
                    update_corp_control(g, ui, rng, corp_id);
                    if player_id == g.voting_controller {
                        let pct = to_long((cast::<Ext, _>(g.hold_stock_pct[(slot, player_id)])).abs());
                        force_cover_shorts(g, ui, rng, 4, player_id, corp_id, pct);
                        g.news_msg = g.ent_symbol[player_id].clone()
                            + __content.string(4806)
                            + &g.ent_symbol[corp_id];
                        post_news(g, ui, player_id, corp_id);
                    }
                }
            }
        }
        player_id += 1;
    }
}

/// Port of VestExecutiveOptions (0x5fd441).
/// Posts a pending `NewsMsg`, revokes the [runtime resource wording] s ([`revoke_ex_ceo_options`]),
/// then turns every `GRANT` record whose grant date (`month * 0.08 + year`, SINGLE) is at most 1.01
/// after the current quarter's into `VESTED`, with one summary headline (posted unless the game is in
/// its last two years).
///
/// Quirk: the per-record emptiness test reads `OptionRecs(0)` (the count) instead of the record.
pub fn vest_executive_options(g: &mut Globals, ui: &mut dyn Ui) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut vest_count = 0;
    let mut holder_id = 0;
    let mut other_holder = 0;
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    revoke_ex_ceo_options(g, ui);
    for rec_idx in 1..=1000 {
        if g.option_recs[0].is_empty() {
            continue;
        }
        if instr(1, g.option_recs[rec_idx].as_bytes(), b"GRANT") == 0 {
            continue;
        }
        let mut opt_rec = trim(g.option_recs[rec_idx].as_bytes());
        if right(opt_rec.as_bytes(), 5) != "GRANT" {
            continue;
        }
        let cur_month = g.cur_quarter * 3 - 2;
        let cur_date = cast::<f32, _>(cast::<Ext, _>(cur_month) * sf(0.08f32) + cast::<Ext, _>(g.cur_year));
        let grant_month = to_long(fval(&opt_rec, 6));
        let grant_year = to_long(fval(&opt_rec, 7));
        let grant_date = cast::<f32, _>(cast::<Ext, _>(grant_month) * sf(0.08f32) + cast::<Ext, _>(grant_year));
        if cast::<Ext, _>(grant_date) - cast::<Ext, _>(cur_date) <= sf(1.01f32) {
            let corp_id = to_long(fval(&opt_rec, 1));
            vest_count += 1;
            if vest_count == 1 {
                holder_id = to_long(fval(&opt_rec, 3));
            }
            if 1 < vest_count {
                other_holder = to_long(fval(&opt_rec, 3));
            }
            if 1 < vest_count && other_holder != holder_id {
                holder_id = 0;
            }
            opt_rec = replace(opt_rec.as_bytes(), b"GRANT", b"VESTED");
            g.option_recs[rec_idx] = opt_rec;
            if holder_id < 1 {
                g.news_msg = PbStr::from(__content.string(4807));
            } else {
                g.news_msg = g.ent_symbol[corp_id].clone()
                    + __content.string(4808)
                    + &g.ent_symbol[holder_id]
                    + __content.string(4809);
            }
        }
    }
    if !g.news_msg.is_empty() && g.cur_year < g.game_end_year - 1 {
        post_news(g, ui, 0, 0);
    }
}

/// Port of GrantExecutiveOptions (0x5fc7c1).
/// Once per quarter (not in the first quarter of the game): every active player who has been CEO of a
/// company since before this year (computer players only from the game's third year) is granted call
/// options on 1..3% of the company (`GRANT` record, strike at the market cap but at least 5 per share,
/// expiring in two years) — when the company is public, the player controls it, a free record exists
/// and the quarter's earnings (`EntMiscData` field 3) are positive and above the previous ones (field
/// 4, nonzero). 2% at +15%, 3% if the player also holds more than 50%. Otherwise the current player
/// gets a "No options granted" headline with the reason. The new option is priced
/// ([`calc_option_price`], value unused) for its side effects.
pub fn grant_executive_options(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    // ON ERROR GOTO -> ShowRuntimeError and exit (ERROR 151 paths below)
    if g.start_year == g.cur_year && g.cur_quarter == 1 {
        return;
    }
    let n = g.num_players;
    let mut player_id = 1;
    while player_id <= n {
        if grant_for_player(g, ui, rng, player_id) {
            return;
        }
        player_id += 1;
    }
}

/// One player of [`grant_executive_options`] (the body of the player loop); `true` when the routine
/// is left through its error handler.
fn grant_for_player(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, player_id: i32) -> bool {
    let __content = crate::content::current();
    if g.player_name[player_id] == "...OUT..." {
        return false;
    }
    if g.is_computer_player[player_id] == 1 && g.cur_year < g.start_year + 2 {
        return false;
    }
    let corp_id = g.player_ceo_of[player_id];
    if corp_id <= 10 || g.cur_year == g.player_ceo_since_year[player_id] {
        return false;
    }
    let mut free_slot = 0;
    let mut rec_idx = 1;
    while rec_idx < 1001 {
        if g.option_recs[rec_idx].is_empty() {
            free_slot = rec_idx;
            rec_idx = 1000;
        }
        rec_idx += 1;
    }
    let cur_earnings = val(parse_any(g.ent_misc_data[corp_id].as_bytes(), b"|", 3).as_bytes());
    let prev_earnings = val(parse_any(g.ent_misc_data[corp_id].as_bytes(), b"|", 4).as_bytes());
    update_corp_control(g, ui, rng, corp_id);
    if prev_earnings == Ext::from_f64(0.0) {
        free_slot = 0;
    }
    if Ext::from_f64(0.0) < cur_earnings && cur_earnings <= prev_earnings {
        free_slot = 0;
    }
    if g.public_pct == 0 || free_slot == 0 || player_id != g.voting_controller || cur_earnings <= Ext::from_f64(0.0) {
        let mut msg_text = g.ent_symbol[corp_id].clone() + __content.string(4810);
        if g.public_pct == 0 && !(rng.rnd() * Ext::from_f64(3.0) <= Ext::from_f64(1.0)) {
            msg_text = PbStr::from(__content.string(4811)) + &g.ent_symbol[corp_id];
        } else {
            if g.public_pct == 0 {
                return false;
            }
            if Ext::from_f64(0.0) <= cur_earnings || player_id != g.voting_controller {
                if player_id == g.voting_controller {
                    if Ext::from_f64(0.0) < cur_earnings && cur_earnings < prev_earnings {
                        msg_text = g.ent_symbol[corp_id].clone() + __content.string(4812);
                        if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                            msg_text = replace(msg_text.as_bytes(), b"weaker", b"drop in");
                        }
                    }
                } else {
                    msg_text = g.ent_symbol[corp_id].clone() + __content.string(4813);
                }
            } else {
                msg_text = g.ent_symbol[corp_id].clone() + __content.string(4814);
            }
        }
        if g.cur_player == player_id {
            g.news_msg = PbStr::from(__content.string(4815))
                + &g.player_name[player_id]
                + " by "
                + &msg_text;
            if instr(1, msg_text.as_bytes(), __content.bytes(4816)) < 1 || g.cur_quarter == 1 {
                post_news(g, ui, corp_id, 0);
            } else {
                g.news_msg = PbStr::new();
            }
        }
        return false;
    }
    let mut grant_pct = 1;
    if prev_earnings * sf(1.15f32) <= cur_earnings {
        grant_pct = 2;
        if 50 < g.largest_holder_pct {
            grant_pct = 3;
        }
    }
    if g.shares_outstanding[corp_id] == Ext::from_f64(0.0) {
        // ERROR 151 -> ShowRuntimeError, routine left
        return true;
    }
    let mut strike = Ext80::from_ext(g.market_cap[corp_id]);
    if g.market_cap[corp_id] / g.shares_outstanding[corp_id] < Ext::from_f64(5.0) {
        strike = Ext80::from_ext(g.shares_outstanding[corp_id]).fmul(Ext80::from_ext(Ext::from_f64(5.0)));
    }
    let mut sep = PbStr::from(" |");
    let grant_month = g.cur_quarter * 3 - 2;
    let msg_text = trim(str15(cast::<Ext, _>(corp_id)).as_bytes())
        + &sep
        + " 1|"
        + &str15(cast::<Ext, _>(player_id))
        + &sep
        + &str15(cast::<Ext, _>(grant_pct))
        + &sep
        + &PbStr::from(str_num_ext(strike, 15).as_str())
        + &sep
        + &str15(cast::<Ext, _>(grant_month))
        + &sep
        + &str15(Ext::from_f64(2.0) + cast::<Ext, _>(g.cur_year))
        + &sep
        + "GRANT";
    g.option_recs[free_slot] = msg_text;
    g.has_options_outstanding[corp_id] = 1;
    let _opt_value = calc_option_price(g, ui, rng, 0, Ext::from_f64(0.0), 0, 0, 0, free_slot) * cast::<Ext, _>(grant_pct);
    if g.shares_outstanding[corp_id] == Ext::from_f64(0.0) {
        g.shares_outstanding[corp_id] = Ext::from_f64(2.0);
        check_stock_split(g, corp_id);
        // ERROR 151 ("VarType = PMult()") -> ShowRuntimeError, routine left
        return true;
    }
    let strike_ps = strike.fdiv(Ext80::from_ext(g.shares_outstanding[corp_id])).into_ext().abs();
    sep = PbStr::from(" stock");
    if 20 < len(g.player_name[player_id].as_bytes()) {
        sep = PbStr::new();
    }
    g.news_msg = PbStr::from("C.E.O. ")
        + &g.player_name[player_id]
        + __content.string(4817)
        + &g.ent_symbol[corp_id]
        + &sep
        + __content.string(4818)
        + &trim(g.currency_symbol.as_bytes())
        + &fmt_trim(Ext::from(strike_ps), b"* ##.00");
    post_news(g, ui, player_id, corp_id);
    let rec_count = to_long(val(g.option_recs[0].as_bytes())) + 1;
    g.option_recs[0] = trim(str15(cast::<Ext, _>(rec_count)).as_bytes());
    false
}

/// Port of AiCorpOptionsTrading (0x5fdd2a).
/// For the entity `SimCoCount` (an active, AI-controlled corp 131..1600 with stock holdings, not a
/// fund; not in the first quarter of the game below entity 501; only while `OptionRecs(250)` is free):
/// tries random holdings; for a 45..78% stake in a stock priced 6.5..1000 per share with an analyst
/// rating of 1..2 and more than 1% public float it buys calls on 6..10% (below the float) through
/// [`ai_create_option_position`] if cash + T-bills cover 1.5x their value. Stops at the first stake
/// outside 45..78%, on success, or after as many tries as holdings.
pub fn ai_corp_options_trading(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut tries = 0;
    let sc = g.sim_co_count;
    if !((g.start_year != g.cur_year || g.cur_quarter != 1 || 500 < sc) && 130 < sc && sc < 1601) {
        return;
    }
    if 1580 < sc && g.industry_num[sc] == 71 {
        return;
    }
    if g.entity_status[sc] != 1 || g.hold_stock_id[(0, sc)] == 0 {
        return;
    }
    if !g.option_recs[250].is_empty() {
        return;
    }
    update_corp_control(g, ui, rng, g.sim_co_count);
    if g.voting_controller == 0 {
        return;
    }
    let sc = g.sim_co_count;
    if get_controller_type(g, sc) == 1 {
        return;
    }
    loop {
        let sc = g.sim_co_count;
        tries += 1;
        let slot = rng.rnd_range(cast::<i32, _>(g.hold_stock_id[(0, sc)]), 1);
        if !(g.hold_stock_pct[(slot, sc)] < 1) {
            let corp_id = cast::<i32, _>(g.hold_stock_id[(slot, sc)]);
            if 10 < corp_id && g.shares_outstanding[corp_id] != Ext::from_f64(0.0) {
                let price = g.market_cap[corp_id] / g.shares_outstanding[corp_id];
                if Ext::from_f64(6.5) <= price && price <= Ext::from_f64(1000.0) {
                    let pct = g.hold_stock_pct[(slot, sc)];
                    if pct < 45 || 78 < pct {
                        return;
                    }
                    if calc_analyst_rating(g, ui, rng, corp_id) <= 2 && {
                        update_corp_control(g, ui, rng, corp_id);
                        1 < g.public_pct
                    } {
                        let mut pct_to_buy = rng.rnd_range(10, 6);
                        if cast::<i32, _>(g.public_pct) <= pct_to_buy {
                            pct_to_buy = cast::<i32, _>(g.public_pct) - 1;
                        }
                        let sc = g.sim_co_count;
                        if cast::<Ext, _>(pct_to_buy) * sf(1.5f32) * g.market_cap[corp_id] <= g.cash[sc] + g.t_bills[sc]
                            && ai_create_option_position(g, ui, rng, sc, corp_id, pct_to_buy, 1, 0) == 1
                        {
                            return;
                        }
                    }
                }
            }
        }
        if cast::<i32, _>(g.hold_stock_id[(0, g.sim_co_count)]) <= tries {
            return;
        }
    }
}

/// Port of ForceCloseDerivativesBadCredit (0x4c4033).
/// Up to 11 passes: picks a weak entity (with futures records: a random non-bank, non-human holder
/// rated below 5; else a random corp writer of options, not player-managed, rated below 6, whose
/// written puts are bought back first) and closes its derivatives: written options until the
/// remembered holder's ne [runtime resource wording] s no longer negative, then (if rated 5 or worse)
/// its futures (`LiquidateFuturesPositions` mode 2), each with a headline (one in ten, or always for
/// the current player's companies). `mode` 2 only changes which record count is tested first.
///
/// Bug (reproduced, BUGS.md #174): the covering loop closes the written calls of `SelEntity` (the
/// player's selected entity) instead of the weak option writer, so it usually spins 1,000 times
/// without effect (and may close an unrelated player-selected entity's options).
pub fn force_close_derivatives_bad_credit(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, mode: i32) {
    let __content = crate::content::current();
    let mut pass_count = 0;
    let mut opt_holder = 0;
    loop {
        let mut entity_id = 0;
        // 0 = futures scan, 1 = options scan
        let mut scan_futures = if mode == 2 {
            val(g.option_recs[0].as_bytes()) == Ext::from_f64(0.0) || val(g.futures_recs[0].as_bytes()) != Ext::from_f64(0.0)
        } else {
            val(g.futures_recs[0].as_bytes()) != Ext::from_f64(0.0)
        };
        if scan_futures {
            let n = to_long(val(g.futures_recs[0].as_bytes()));
            let mut i = 1;
            while i <= n {
                let rec_text = g.futures_recs[i].clone();
                let holder = to_long(fval(&rec_text, 2));
                if 85 < holder && entity_id != holder && rng.rnd() * Ext::from_f64(2.0) <= Ext::from_f64(1.0) {
                    if get_controller_type(g, holder) != 1 {
                        calc_entity_financials(g, ui, rng, holder);
                        if g.credit_rating_num[holder] < 5 {
                            entity_id = holder;
                            break;
                        }
                    }
                }
                i += 1;
            }
            if entity_id < 11 {
                scan_futures = false;
            }
        }
        if !scan_futures {
            if val(g.option_recs[0].as_bytes()) == Ext::from_f64(0.0) {
                break;
            }
            let n = to_long(val(g.option_recs[0].as_bytes()));
            let mut i = 1;
            while i <= n {
                entity_id = 0;
                opt_holder = 0;
                let rec_text = g.option_recs[i].clone();
                let holder = to_long(fval(&rec_text, 3));
                if 10 < holder && entity_id != holder && rng.rnd() * Ext::from_f64(2.0) <= Ext::from_f64(1.0) {
                    if fval(&rec_text, 4) <= Ext::from_f64(0.0) && get_managing_player(g, holder) <= 0 {
                        calc_entity_financials(g, ui, rng, holder);
                        if g.credit_rating_num[holder] < 6 {
                            let pos = find_option_positions(g, ui, holder, 0, 2, 0);
                            if pos == -21 || pos == 22 {
                                let mut k = 1;
                                while k < 1001 {
                                    g.tmp_ext1 = Ext::from_f64(0.0);
                                    g.tmp_ext2 = Ext::from_f64(99.0);
                                    if close_option_positions(g, ui, rng, holder, 0, 0, 4) == 0 {
                                        k = 1000;
                                    }
                                    k += 1;
                                }
                            }
                            entity_id = to_long(fval(&rec_text, 1));
                            opt_holder = holder;
                            if get_net_stock_pct_after_options(g, ui, opt_holder, entity_id, 1) < 0 {
                                break;
                            }
                        }
                    }
                }
                i += 1;
            }
            if entity_id < 11 {
                break;
            }
        }
        loop {
            if 0 <= get_net_stock_pct_after_options(g, ui, opt_holder, entity_id, 1) {
                break;
            }
            g.tmp_ext1 = Ext::from_f64(0.0);
            g.tmp_ext2 = Ext::from_f64(99.0);
            let sel = g.sel_entity;
            close_option_positions(g, ui, rng, sel, 0, 0, 3);
            pass_count += 1;
            if pass_count >= 1000 {
                break;
            }
        }
        g.news_msg = PbStr::from(__content.string(4819))
            + &g.ent_symbol[opt_holder]
            + __content.string(4820);
        if rng.rnd() * Ext::from_f64(10.0) <= Ext::from_f64(1.0) || g.cur_player == g.corp_controller[opt_holder] {
            post_news(g, ui, entity_id, opt_holder);
        } else {
            g.news_msg = PbStr::new();
        }
        calc_entity_financials(g, ui, rng, entity_id);
        if 5 < g.credit_rating_num[entity_id] {
            break;
        }
        recalc_commod_margin_balances(g, ui, rng, entity_id);
        let closed = liquidate_futures_positions(g, ui, rng, entity_id, 2, 0);
        if 0 < closed {
            g.news_msg = g.ent_symbol[entity_id].clone()
                + __content.string(4821);
        }
        if rng.rnd() * Ext::from_f64(10.0) <= Ext::from_f64(1.0) || g.cur_player == g.corp_controller[entity_id] {
            post_news(g, ui, entity_id, 0);
        } else {
            g.news_msg = PbStr::new();
        }
        pass_count += 1;
        if pass_count >= 11 {
            break;
        }
    }
}

/// Port of ForceCloseExcessNakedOptions (0x5f92e9).
/// After a margin check of `holder_id_ref` ([`check_naked_option_margin`]): when the holder has
/// options on `corp_id_ref` covering more percent than it owns, closes its long puts / written calls
/// on the stock one record at a time (`CloseOptionPositions(holder, corp, 0, 1)`) until the excess is
/// gone, with an "insider must close" (players) or "forced to close" (human-controlled corps)
/// headline. `holder_id_ref = 0` means the corp's controller (only for `SimCoCount` or the current
/// player's corps). Corp holders above 130 are only checked if they share the corp's controller.
pub fn force_close_excess_naked_options(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    holder_id_ref: i32,
    corp_id_ref: i32,
) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let corp_id = corp_id_ref;
    let mut holder_id = holder_id_ref;
    let mut iter = 0;
    check_naked_option_margin(g, ui, rng, holder_id);
    if holder_id == 0 {
        if g.sim_co_count != corp_id && g.cur_player != g.corp_controller[corp_id] {
            return;
        }
        if g.corp_controller[corp_id] == 0 || corp_id == g.corp_controller[corp_id] {
            return;
        }
        holder_id = g.corp_controller[corp_id];
    }
    let optioned_pct = get_optioned_stock_pct(g, ui, holder_id, corp_id, 0);
    if !(0 < optioned_pct) {
        return;
    }
    let slot = find_holding_slot(g, holder_id, corp_id);
    let mut owned_pct = 0;
    if slot != 0 && cast::<i32, _>(g.hold_stock_id[(slot, holder_id)]) == corp_id {
        owned_pct = cast::<i32, _>(g.hold_stock_pct[(slot, holder_id)]);
        if owned_pct < 0 {
            owned_pct = 0;
        }
    }
    let mut excess_pct = optioned_pct - owned_pct;
    if !(0 < excess_pct) {
        return;
    }
    if 130 < holder_id && g.corp_controller[corp_id] != g.corp_controller[holder_id] {
        return;
    }
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    let mut qualifier = PbStr::from(__content.string(4822));
    if owned_pct == 0 {
        qualifier = PbStr::from("all");
    }
    loop {
        if excess_pct < 1 {
            break;
        }
        iter += 1;
        if !(find_option_positions(g, ui, holder_id, corp_id, 2, 0) >= 31)
            && find_option_positions(g, ui, holder_id, corp_id, 1, 0) != -1
            && find_option_positions(g, ui, holder_id, corp_id, 1, 0) != 2
        {
            break;
        }
        g.tmp_ext1 = Ext::from_f64(0.0);
        g.tmp_ext2 = Ext::from_f64(0.0);
        close_option_positions(g, ui, rng, holder_id, corp_id, 0, 1);
        excess_pct -= 1;
        if holder_id < 11 {
            g.news_msg = PbStr::from(__content.string(4823))
                + &g.ent_symbol[holder_id]
                + __content.string(4824)
                + &qualifier
                + __content.string(4825)
                + &g.ent_symbol[corp_id];
        }
        if 10 < holder_id {
            if get_controller_type(g, holder_id) == 1 {
                g.news_msg = g.ent_symbol[holder_id].clone()
                    + __content.string(4826)
                    + &g.ent_symbol[corp_id];
            } else {
                g.news_msg = PbStr::new();
            }
        }
        if iter >= 1001 {
            break;
        }
    }
    if !g.news_msg.is_empty() {
        if iter < 1001 {
            let mut msg = std::mem::take(&mut g.news_msg);
            show_quick_message(g, ui, &mut msg);
            g.news_msg = msg;
        }
        post_news(g, ui, 0, 0);
    }
}
