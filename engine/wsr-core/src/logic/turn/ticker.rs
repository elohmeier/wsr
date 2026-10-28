//! `TickerSimSteps` (0x41ef3f) and the small ticker helpers `RefreshNewsListbox` (0x41e749),
//! `ShowPendingPopup` (0x4b25e8), `TickerWatchListQuotes` (0x4bb959), `UpdateDateDisplay` (0x4a054a) and
//! `PauseTicker` (0x46e001).
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use super::Env;
use crate::logic::cashflow::get_etf_adviser_human_status;
use crate::logic::economy::industry::commodity_demand_effects;
use crate::logic::economy::macro_update::update_economy;
use crate::logic::economy::quarterly::update_economy_52978d;
use crate::logic::economy::show_misc_news_story;
use crate::logic::futures::set_commodity_price_ranges;
use crate::logic::market::commodities::update_commodity_prices;
use crate::logic::market::{show_next_earnings_date, update_market_display};
use crate::logic::reports_corp::show_earnings_report;
use crate::logic::reports_lists::update_streaming_quotes;
use crate::logic::sim_entity::sim_process_next_entity_skipping;

/// Port of TickerSimSteps (0x41ef3f): one batch of simulation ticks (see the module docs of
/// [`super`]).
///
/// Nothing happens when the game is over or not running, a stop was requested (`TickerStopReq`), or the
/// ticker is paused (`TIMER + 0.6 < TickerPauseUntil`). `RandSeed` gets the clock seconds added. The batch
/// has `step_count` ticks (if > 0; else 10..20 at random, `RND`), x1.5 for `TickerSpeed > 75` and again
/// for `> 92`, and only one while `ClockState` is "RUN" or "STOP". Unless the ticker is suppressed
/// (`ActionCode = "SUPP"`), a tick first scrolls `TickerText` by one character (the batch ends at once,
/// without the closing part, when the text is empty; with `ClockState = "STOP"` the tick only scrolls).
///
/// A tick: [`super::sim_scheduled_events`], `SimProcessNextEntity`, [`show_pending_popup`]; for an active
/// company the player follows (selected, last selected, controlled, or a fund with a human adviser) the
/// next-earnings label and — unless suppressed — its earnings report (`ShowEarningsReport`, streaming
/// quotes); `CommodityDemandEffects` for operating companies; `RandSeed + 1`; and while
/// `EconCycleCounter > 22` an economy update (`UpdateEconomy_52978d`, `SetCommodityPriceRanges`,
/// `UpdateCommodityPrices`, `UpdateEconomy`).
///
/// After the batch: a pending `NewsMsg` is posted, `LastNewsLineKey` tracks the newest headline,
/// `UpdateMarketDisplay` runs (every 40th batch while the clock runs or the ticker is suppressed:
/// `IndexRecalcTick`), the turn ends ([`super::end_turn`], with `TickerStopReq = "Y"` meanwhile) once
/// `TurnActionCount >= 5` (not on autopilot), and in the last year the ticker shows "GAME OVER"
/// (`TurnActionCount = 5`, `SoloHumanMode = 0`).
///
/// Skipped UI: minimized-window pause (`IsIconic`), the ticker label, `SLEEP`, the news listbox, the
/// earnings-report labels.
pub fn ticker_sim_steps(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, step_count: i32, now: &LocalTime) {
    ticker_sim_steps_skipping(g, ui, rng, step_count, now, &[]);
}

/// [`ticker_sim_steps`] with routines (game addresses) skipped like the test harness does (`--skip`).
#[doc(hidden)]
pub fn ticker_sim_steps_skipping(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    step_count: i32,
    now: &LocalTime,
    skip: &[u32],
) {
    ticker(g, ui, rng, step_count, &Env { now, skip });
}

pub(super) fn ticker(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, step_count: i32, env: &Env) {
    let __content = crate::content::current();
    let ts = time_secs(env.now);
    if g.game_end_flag == "Y" || g.ticker_stop_req == "Y" || g.game_in_progress == 0 {
        return;
    }
    // (UI) IsIconic(hDlgMain) -> PauseTicker(2)
    if sf(0.6f32) + timer(env.now) < cast::<Ext, _>(g.ticker_pause_until) {
        return;
    }
    g.rand_seed = to_long(cast::<Ext, _>(ts) + cast::<Ext, _>(g.rand_seed));
    if 32000 < g.rand_seed {
        g.rand_seed = g.rand_seed - 31000;
    }
    let mut num_steps = rng.rnd_range(20, 10); // -0x90
    if 0 < step_count {
        num_steps = step_count;
    }
    if 75 < g.ticker_speed {
        num_steps = to_long(Ext::from_f64(1.5) * cast::<Ext, _>(num_steps));
        if 92 < g.ticker_speed {
            num_steps = to_long(Ext::from_f64(1.5) * cast::<Ext, _>(num_steps));
        }
    }
    if g.clock_state == "STOP" || g.clock_state == "RUN" {
        num_steps = 1;
    }
    let last = num_steps; // -0xa8
    let mut step = 1; // -0x8c
    while step <= last {
        let mut run_tick = true;
        if !(g.action_code == "SUPP") {
            let mut ticker_len = len(g.ticker_text.as_bytes()); // -0x88
            if ticker_len == 0 {
                return;
            }
            g.ticker_text = right(g.ticker_text.as_bytes(), ticker_len - 1);
            ticker_len -= 1;
            if 60 < ticker_len {
                ticker_len = 60;
            }
            let _ = ticker_len;
            if g.clock_state == "STOP" {
                step = num_steps;
                run_tick = false;
            } else {
                // (UI) CONTROL SET TEXT hMainDlgAlias, 123, MID$(TickerText, 1, tickerLen)
                if g.ticker_speed == 0 {
                    g.ticker_speed = 50;
                }
                // (UI) SLEEP (100 - TickerSpeed) * 2
            }
        }
        if run_tick {
            tick(g, ui, rng, env);
        }
        step += 1;
    }
    post_news(g, ui, 0, 0);
    if g.ticker_dirty.is_empty() {
        refresh_news_listbox(g);
    } else if !(mid(g.news_headline[60].as_bytes(), 1, 9) == g.last_news_line_key) {
        refresh_news_listbox(g);
        g.last_news_line_key = mid(g.news_headline[60].as_bytes(), 1, 9);
    }
    g.ticker_dirty = PbStr::new();
    if g.clock_state == "RUN" || g.action_code == "SUPP" {
        g.index_recalc_tick = g.index_recalc_tick + 1;
        if 40 <= g.index_recalc_tick {
            if !env.skips(0x4400d7) {
                update_market_display(g, ui, rng, ts);
            }
            g.index_recalc_tick = 0;
        }
    } else if !env.skips(0x4400d7) {
        update_market_display(g, ui, rng, ts);
    }
    if 5 <= g.turn_action_count && !(g.action_code == "AUTO") {
        if !env.skips(0x4400d7) {
            update_market_display(g, ui, rng, ts);
        }
        set(&mut g.ticker_stop_req, "Y");
        if !env.skips(0x424fd6) {
            super::end_turn::end_turn_env(g, ui, rng, env);
        }
        if !(g.game_end_flag == "Y") {
            set(&mut g.ticker_stop_req, "N");
        }
    }
    if g.game_end_year == g.cur_year {
        g.turn_action_count = 5;
        g.solo_human_mode = 0;
        g.ticker_text = s(__content.string(13519));
    }
}

/// One simulation tick of TickerSimSteps (0x41f201..0x41f4b3).
fn tick(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let ts = time_secs(env.now);
    if !env.skips(0x4a395a) {
        super::scheduled::scheduled(g, ui, rng, env);
    }
    if !env.skips(0x4c6bc1) {
        sim_process_next_entity_skipping(g, ui, rng, env.now, env.skip);
    }
    if !env.skips(0x4b25e8) {
        show_pending_popup(g, ui, rng);
    }
    let s = g.sim_co_count;
    if 10 < s && g.entity_status[s] == 1 {
        let mut show_earnings = 0;
        if g.sel_entity == s || g.last_sel_entity == s || g.cur_player == g.corp_controller[s] {
            show_earnings = 1;
        }
        if 1580 < s && get_etf_adviser_human_status(g, ui, rng, s) == 2 {
            show_earnings = 1;
        }
        if show_earnings == 1 {
            if g.sel_entity == s {
                show_next_earnings_date(g);
            }
            if g.sel_entity == s || g.cur_suppress_earnings == 0 {
                // (UI) SLEEP 300
                g.chart_entity = s;
                update_date_display(g);
                // (UI) labels 169 (" Click 'GO' to select <symbol>") and 202 (symbol)
                if !env.skips(0x462f1e) {
                    show_earnings_report(g, ui, rng, s);
                }
                // (UI) labels 169 (SelCorpName) and 202 (the selected company's symbol)
                g.chart_entity = 0;
                let s = g.sim_co_count;
                update_streaming_quotes(g, ui, rng, s - 1, s + 1);
            }
        }
    }
    if 130 < g.sim_co_count && !env.skips(0x4bce6a) {
        commodity_demand_effects(g, ui, rng);
    }
    g.rand_seed = g.rand_seed.wrapping_add(1);
    if 22 < g.econ_cycle_counter {
        if !env.skips(0x52978d) {
            update_economy_52978d(g, ui, rng, ts);
        }
        if !env.skips(0x629f0f) {
            set_commodity_price_ranges(g, ui, rng);
        }
        if !env.skips(0x62a6a2) {
            update_commodity_prices(g, ui, rng);
        }
        if !env.skips(0x458a7a) {
            update_economy(g, ui, rng);
        }
    }
}

/// Port of RefreshNewsListbox (0x41e749): refills the main window's news listbox (control 205) with
/// `NewsHeadline(54..60)`. UI only (the port reports headlines through [`Ui::news`] when posted).
pub fn refresh_news_listbox(g: &mut Globals) {
    let _ = g;
}

/// Port of ShowPendingPopup (0x4b25e8).
/// Shows a news story queued during the tick (`PendingPopupEntity > 0`): `TmpExt1` = the amount,
/// `TmpExt2` = the company, the queue is cleared, and the story `PendingPopupPic` with text
/// `PendingPopupText` is shown (ShowMiscNewsStory) — unless earnings reports are suppressed and it
/// concerns a company (>= 11), which drops it (`TmpExt1 = TmpExt2 = 0`).
pub fn show_pending_popup(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    if g.pending_popup_entity < 1 {
        return;
    }
    g.tmp_ext1 = cast::<Ext, _>(g.pending_event_amount);
    g.tmp_ext2 = cast::<Ext, _>(g.pending_popup_entity);
    let popup_pic = g.pending_popup_pic;
    let popup_text = g.pending_popup_text.clone();
    g.pending_popup_entity = 0;
    g.pending_event_amount = 0;
    g.pending_popup_pic = 0;
    g.pending_popup_text = PbStr::new();
    if g.cur_suppress_earnings == 0 || g.tmp_ext2 < Ext::from_f64(11.0) {
        show_misc_news_story(g, ui, rng, popup_pic, popup_text.as_bytes());
    } else {
        g.tmp_ext1 = Ext::from_f64(0.0);
        g.tmp_ext2 = Ext::from_f64(0.0);
    }
}

/// Port of TickerWatchListQuotes (0x4bb959).
/// Refreshes some streaming quotes of the (human) current player: 5 times in 6 a few random entries of
/// the watch list (`ROUND((100 - TickerSpeed) / 7 + RND * 3 + 1)` picks, each quoted with its
/// neighbours), else one of the three blocks of 530 companies chosen by `RandSeed % 3`.
pub fn ticker_watch_list_quotes(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    // ON ERROR GOTO 0x4bbbb4 (not ported)
    if 0 < g.cur_player && g.is_computer_player[g.cur_player] == 1 {
        return;
    }
    let mut first_id: i32; // -0x88
    let mut last_id: i32; // -0x8c
    if rng.rnd() * Ext::from_f64(6.0) <= Ext::from_f64(5.0) {
        if g.watch_list[(0, g.cur_player)] == 0 {
            return;
        }
        let n = to_long((rng.rnd() * Ext::from_f64(3.0) + Ext::from_f64(1.0)) + cast::<Ext, _>(100 - g.ticker_speed) / Ext::from_f64(7.0));
        let mut i = 1;
        while i <= n {
            let mut list_slot = to_long(cast::<Ext, _>(g.watch_list[(0, g.cur_player)]) * rng.rnd());
            if 15 < list_slot {
                list_slot = 15;
            }
            if list_slot < 1 {
                list_slot = 1;
            }
            let id = g.watch_list[(list_slot, g.cur_player)];
            if 11 <= id {
                first_id = id - 1;
                last_id = id + 1;
                update_streaming_quotes(g, ui, rng, first_id, last_id);
            }
            i += 1;
        }
    } else {
        last_id = 541;
        if g.rand_seed % 3 == 0 {
            last_id = 1070;
        }
        if g.rand_seed % 3 == 1 {
            last_id = 1600;
        }
        first_id = last_id - 530;
        update_streaming_quotes(g, ui, rng, first_id, last_id);
    }
}

/// Port of UpdateDateDisplay (0x4a054a), state effects only: the game date label ("<Month> <day>,
/// <year> -- Ends in <year>") of the main window is UI; the calendar helpers `CalcCurMonthOfYear` /
/// `CalcDayOfMonth` run for their effects (`TmpExt1`, cleared afterwards).
pub fn update_date_display(g: &mut Globals) {
    crate::logic::earnings::current_game_date(g);
    // (UI) CONTROL SET TEXT hDlgMain, 141, dateText
}

/// Port of PauseTicker (0x46e001): pauses the ticker for `seconds` (0 = 0.4):
/// `TickerPauseUntil = ROUND(TIMER + seconds)`, 0 near midnight.
pub fn pause_ticker(g: &mut Globals, now: &LocalTime, seconds: f32) {
    let mut seconds = seconds;
    if seconds == Ext::from_f32(0.0f32) {
        seconds = 0.4f32;
    }
    g.ticker_pause_until = to_long(timer(now) + cast::<Ext, _>(seconds));
    if 86390 < g.ticker_pause_until {
        g.ticker_pause_until = 0;
    }
}
