//! The scenario ("adventure") story: `MaybeStartScenario` (0x490982), `AdvanceScenarioStory`
//! (0x48fe45) and `RunScenarioEvent` (0x48ec04) with its GOSUBs `Scenario_CheckPriceMove` (0x48eed9),
//! `Scenario_ApplyNewsEffects` (0x48f02c) and `Scenario_PickNext` (0x48f819).
//!
//! Scenarios only run in cheat mode (`CheatMode = "Y"`) for companies the current human player controls:
//! a random proposal (`ScenarioNum` = the company's industry, or one of the general stories 111..120)
//! is shown as an internal memo; an accepted one runs for `ScenarioStage` story ticks (every 213th tick)
//! with headlines, and may end in an outcome (`ScenarioStage = -3` .. 0, then the negated scenario).
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use super::Env;
use crate::logic::control::update_corp_control;
use crate::logic::earnings::calc_projected_earnings;
use crate::logic::market::update_market_display;
use crate::logic::report::show_quick_message;
use crate::logic::valuation::{calc_stock_price, price_pct_change_months, price_trend_direction};
use std::sync::Mutex;

/// Contents of `DataPath + "SCENWIN1.WSR"` .. `"SCENWIN4.WSR"` (the encoded scenario texts).
static SCENWIN: Mutex<[Option<Vec<u8>>; 4]> = Mutex::new([None, None, None, None]);

/// Provides the contents of `SCENWIN<n>.WSR` (`n` = 1..4; `None`: the file is missing, as if OPEN
/// failed).
pub fn set_scenwin_file(n: i32, data: Option<Vec<u8>>) {
    if (1..=4).contains(&n) {
        SCENWIN.lock().unwrap_or_else(|e| e.into_inner())[cast::<usize, _>(n - 1)] = data;
    }
}

/// The contents of `SCENWIN<n>.WSR`, if provided.
pub(super) fn scenwin_file(n: i32) -> Option<Vec<u8>> {
    if !(1..=4).contains(&n) {
        return None;
    }
    SCENWIN.lock().unwrap_or_else(|e| e.into_inner())[cast::<usize, _>(n - 1)].clone()
}

/// Port of MaybeStartScenario (0x490982).
/// Called every tick for `corpId = SimCoCount`: unless a scenario is cooling down or the current
/// player is a computer (and, in the game's last year, unless one is running), for an established
/// company 11..1580 (some earnings in the last four years) it cancels a scenario whose company or player
/// is gone (status 2), cancels the Japanese-takeover story 104 when the target left Japan, and — when
/// the current player controls the company, no scenario runs and the game is in cheat mode — starts one
/// with chance 1/3 (none in the 4th quarter once the stories 111..120 are all done): industries without a
/// story (36..38, 40, 41, 43..45, 47, 49, 52..54, 59, 60, 65, 70) are marked done (`EventStatus = 2`);
/// then `ScenarioCorp = corpId` and [`run_scenario_event`] if the player has voting control. Outside cheat
/// mode the scenario variables are cleared. Draws `RND` (cheat mode only).
pub fn maybe_start_scenario(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime) {
    maybe_start(g, ui, rng, &Env { now, skip: &[] });
}

pub(super) fn maybe_start(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    // ON ERROR GOTO 0x490f90 (ShowRuntimeError; not ported)
    if 0 < g.adventure_cooldown {
        return;
    }
    if g.is_computer_player[g.cur_player] == 1 {
        return;
    }
    let corp_id = g.sim_co_count;
    if g.game_end_year - 1 == g.cur_year && g.scenario_num == 0 {
        return;
    }
    if 10 < corp_id && g.earnings_yr4_ago[corp_id] == Ext::from_f64(0.0) && g.earnings_yr3_ago[corp_id] == Ext::from_f64(0.0) {
        return;
    }
    if corp_id < 11 || 1580 < corp_id {
        return;
    }
    let mut flag = 1;
    if g.entity_status[corp_id] != 1 && cast::<i32, _>(g.scenario_corp) == corp_id {
        flag = 0;
    }
    if 0 < g.event_player && g.player_name[cast::<i32, _>(g.event_player)] == "...OUT..." {
        flag = 0;
    }
    if flag == 0 {
        g.scenario_corp = 0;
        g.event_player = 0;
        g.scenario_stage = 0;
        if 0 < g.scenario_num {
            g.event_status[cast::<i32, _>(g.scenario_num)] = 2;
        }
        g.scenario_num = 0;
    }
    if g.entity_status[corp_id] != 1 {
        return;
    }
    if g.scenario_num == 104
        && g.cheat_japan_target == -1
        && cast::<i32, _>(g.scenario_corp) == corp_id
        && g.country_idx[corp_id] != 26
    {
        g.scenario_corp = 0;
        g.event_player = 0;
        g.scenario_stage = 0;
        g.scenario_num = 0;
        g.event_status[104] = 0;
        g.cheat_japan_target = 0;
    }
    flag = 0;
    if g.cur_player != g.corp_controller[corp_id] || g.scenario_num != 0 || 0 < g.adventure_cooldown {
        return;
    }
    if !(g.cheat_mode == "Y") {
        g.scenario_corp = 0;
        g.event_player = 0;
        g.scenario_stage = 0;
        g.scenario_num = 0;
        return;
    }
    if g.story_events111to120_done == 1 && g.cur_quarter == 4 {
        return;
    }
    if rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) {
        flag = 1;
    }
    if flag == 1 && g.scenario_num == 0 {
        g.scenario_num = g.industry_num[corp_id];
    }
    if 0 < g.scenario_num && g.scenario_num <= 70 {
        let v = g.scenario_num;
        if (36..=38).contains(&v)
            || v == 40
            || v == 41
            || (43..=45).contains(&v)
            || v == 47
            || v == 49
            || (52..=54).contains(&v)
            || v == 59
            || v == 60
            || v == 65
            || v == 70
        {
            g.event_status[cast::<i32, _>(v)] = 2;
            g.scenario_num = 0;
            flag = 0;
        } else {
            g.scenario_num = 0;
        }
    }
    update_corp_control(g, ui, rng, corp_id);
    if g.cur_player != g.voting_controller {
        return;
    }
    if flag == 1 {
        g.scenario_corp = cast::<i16, _>(corp_id);
        run_event(g, ui, rng, env);
    }
}

/// Port of AdvanceScenarioStory (0x48fe45).
/// Every 213th tick (from 214 on): counts down `AdventureCooldown`; drops a scenario whose player went
/// out (all running events marked done); otherwise advances the running story: builds the "FBI is
/// reportedly investigating ..." headline (variants by country, `RandSeed`, `RND`); while
/// `ScenarioStage > 0` counts it down (the company's profit margin +2, the pending `ScenarioHeadline`
/// shown and posted every 5th stage, +3 more), and at 0 posts the investigation (or the pending
/// headline as a popup) and sets `AdventureCooldown = 3`; a negative stage counts up for the player's
/// turn (sentiment -10, the investigation headline at -1) and at 0 cuts the market cap by 4% and runs the
/// outcome ([`run_scenario_event`]). A finished story is cleared; a running one turns into the outcome
/// (`ScenarioStage = -3`) with chance `ScenarioStage`% (`RND * 100`).
pub fn advance_scenario_story(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime) {
    advance_story(g, ui, rng, &Env { now, skip: &[] });
}

pub(super) fn advance_story(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    let mut headline = PbStr::new(); // -0x94
    let name_suffix; // -0x98
    if g.sim_co_count <= 213 || g.sim_co_count % 213 != 1 {
        return;
    }
    if 0 < g.adventure_cooldown {
        g.adventure_cooldown -= 1;
        return;
    }
    if 0 < g.event_player && cast::<i32, _>(g.event_player) != g.cur_player {
        if g.entity_status[cast::<i32, _>(g.event_player)] != 1 {
            g.event_player = 0;
            g.scenario_corp = 0;
            for event_id in 1..=200 {
                if g.event_status[event_id] == 1 {
                    g.event_status[event_id] = 2;
                }
            }
            g.scenario_num = 0;
            g.scenario_stage = 0;
        }
        return;
    }
    if g.scenario_num == 0 || !(g.cheat_mode == "Y") {
        g.scenario_stage = 0;
    }
    if g.scenario_num == 0 && g.event_player == 0 && g.scenario_corp == 0 && g.adventure_cooldown == 0 {
        g.scenario_tag = PbStr::new();
    }
    let corp = cast::<i32, _>(g.scenario_corp);
    if 10 < corp && (cast::<i32, _>(g.scenario_stage)).abs() < 3 {
        headline = s(__content.string(12898)) + &g.ent_symbol[corp];
        name_suffix = s(" (") + &g.ent_name[corp] + ").";
        if 0 < g.country_idx[corp] || g.rand_seed % 2 == 1 {
            if rng.rnd() * Ext::from_f64(2.0) <= Ext::from_f64(1.0) {
                repl(&mut headline, "FBI is", __content.string(12899));
                if g.country_idx[corp] == 50 {
                    repl(&mut headline, __content.string(12899), __content.string(12900));
                }
            } else {
                repl(&mut headline, "FBI is", __content.string(12901));
            }
        } else if g.country_idx[corp] == 0 && Ext::from_f64(2.0) < rng.rnd() * Ext::from_f64(5.0) {
            repl(&mut headline, "FBI is", __content.string(12902));
        }
        if 4 < g.rand_seed % 7 && 0 < instr(1, headline.as_bytes(), b"Gov") {
            repl(&mut headline, __content.string(12901), "Police");
        }
        if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) && len(headline.as_bytes()) < 64 {
            repl(&mut headline, "vesti", "terro");
            headline = headline + __content.string(12903);
            if Ext::from_f64(3.0) < rng.rnd() * Ext::from_f64(4.0) {
                repl(&mut headline, __content.string(12904), __content.string(12905));
            }
        }
        if g.scenario_num == 104 {
            headline = s(__content.string(12906)) + &g.ent_symbol[corp];
        }
        if len(name_suffix.as_bytes()) + len(headline.as_bytes()) < 68 {
            headline = headline + &name_suffix;
        }
    }
    if 0 < g.scenario_stage {
        g.scenario_stage -= 1;
        let corp = cast::<i32, _>(g.scenario_corp);
        if 10 < corp && g.industry_num[corp] < 70 {
            g.profit_margin[corp] = g.profit_margin[corp].wrapping_add(2);
        }
        if !g.scenario_headline.is_empty() && 5 < g.scenario_stage && g.scenario_stage % 5 == 1 {
            g.news_msg = g.scenario_headline.clone();
            g.scenario_headline = PbStr::new();
            let mut msg = std::mem::take(&mut g.news_msg);
            show_quick_message(g, ui, &mut msg);
            g.news_msg = msg;
            post_news(g, ui, 0, 0);
            let corp = cast::<i32, _>(g.scenario_corp);
            if 10 < corp && g.industry_num[corp] < 70 {
                g.profit_margin[corp] = g.profit_margin[corp].wrapping_add(3);
            }
        }
        if g.scenario_stage == 0 {
            if 10 < g.scenario_corp && g.rand_seed % 6 < 4 {
                if g.scenario_headline.is_empty() || Ext::from_f64(1.0) <= rng.rnd() * Ext::from_f64(2.0) {
                    g.news_msg = headline.clone();
                } else {
                    let mut h = std::mem::take(&mut g.scenario_headline);
                    show_quick_message(g, ui, &mut h);
                    g.scenario_headline = h;
                    g.news_msg = PbStr::new();
                }
                g.scenario_headline = PbStr::new();
                if !g.news_msg.is_empty() {
                    let news_corp_id = cast::<i32, _>(g.scenario_corp);
                    post_news(g, ui, news_corp_id, 0);
                }
            }
            g.adventure_cooldown = 3;
        }
    } else if g.scenario_stage < 0 {
        if cast::<i32, _>(g.event_player) == g.cur_player {
            g.scenario_stage += 1;
            let corp = cast::<i32, _>(g.scenario_corp);
            if 10 < corp {
                g.investor_sentiment[corp] = g.investor_sentiment[corp].wrapping_sub(10);
                if g.scenario_stage == -1 {
                    g.news_msg = headline.clone();
                    if !g.scenario_headline.is_empty() {
                        g.news_msg = g.scenario_headline.clone();
                        let mut h = std::mem::take(&mut g.scenario_headline);
                        show_quick_message(g, ui, &mut h);
                        g.scenario_headline = h;
                    }
                    g.scenario_headline = PbStr::new();
                    post_news(g, ui, 0, 0);
                }
            }
            if g.scenario_stage != 0 {
                return;
            }
            let corp = cast::<i32, _>(g.scenario_corp);
            if 10 < corp {
                g.market_cap[corp] = g.market_cap[corp] * sf(0.96f32);
            }
            run_event(g, ui, rng, env);
        }
    }
    if g.scenario_stage == 0 && 0 < g.scenario_num && g.event_status[cast::<i32, _>(g.scenario_num)] == 1 {
        g.event_status[cast::<i32, _>(g.scenario_num)] = 2;
    }
    if g.scenario_stage == 0 {
        g.scenario_num = 0;
        g.event_player = 0;
        g.scenario_corp = 0;
        g.scenario_headline = PbStr::new();
        return;
    }
    if g.scenario_num != 0 {
        let idx = to_quad((cast::<Ext, _>(g.scenario_num)).abs());
        if g.event_status[cast::<i32, _>(idx)] != 1 {
            return;
        }
    }
    let roll = to_long(rng.rnd() * Ext::from_f64(100.0));
    if cast::<i32, _>(g.scenario_stage) >= roll {
        g.scenario_stage = -3;
    }
}

/// The locals of RunScenarioEvent shared with its GOSUBs.
#[derive(Default)]
struct RunLocals {
    /// -0x94 `choice`: never assigned (always 0).
    choice: i32,
    /// -0xb8 `corpId`
    corp_id: i32,
    /// -0xbc `priceChgPct`
    price_chg_pct: i32,
}

/// Port of RunScenarioEvent (0x48ec04).
/// Runs the next step of the scenario of `ScenarioCorp` (cheat mode only; the company must be active
/// and `SimCoCount` 11..1600; not while cooling down without a scenario): updates
/// `StoryEvents111to120Done`, picks a scenario for a new one (Scenario_PickNext), revalues the company and
/// measures its two-month price move (Scenario_CheckPriceMove), runs the event
/// ([`super::run_scenario_event_689361`]) and applies its effects (Scenario_ApplyNewsEffects), then
/// `UpdateMarketDisplay`.
pub fn run_scenario_event(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime) {
    run_event(g, ui, rng, &Env { now, skip: &[] });
}

/// [`run_scenario_event`] with routines (game addresses) skipped like the test harness does.
#[doc(hidden)]
pub fn run_scenario_event_skipping(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime, skip: &[u32]) {
    run_event(g, ui, rng, &Env { now, skip });
}

pub(super) fn run_event(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    // ON ERROR GOTO 0x48fdb1 -> ShowRuntimeError("Ethics", ...) (not ported)
    let mut l = RunLocals::default();
    if !(g.cheat_mode == "Y") {
        g.scenario_corp = 0;
    }
    if g.scenario_corp == 0 {
        return;
    }
    if g.sim_co_count < 11 || 1600 < g.sim_co_count {
        return;
    }
    if g.entity_status[cast::<i32, _>(g.scenario_corp)] != 1 {
        return;
    }
    if 0 < g.adventure_cooldown && g.scenario_num == 0 {
        return;
    }
    g.adventure_cooldown = 0;
    g.story_events111to120_done = 1;
    for event_id in 111..=120 {
        if g.event_status[event_id] != 2 {
            g.story_events111to120_done = 0;
        }
    }
    let skip_pick = 0 < g.scenario_num && g.event_status[cast::<i32, _>(g.scenario_num)] == 0;
    if !skip_pick {
        if g.scenario_num == 0 && 10 < g.scenario_corp && cast::<i32, _>(g.scenario_corp) == g.sim_co_count {
            pick_next(g, ui, rng);
        }
        if g.scenario_corp == 0 {
            return;
        }
        if 0 < g.scenario_num && g.event_status[cast::<i32, _>(g.scenario_num)] == 2 {
            g.scenario_num = 0;
            g.scenario_corp = 0;
            g.event_player = 0;
            g.scenario_stage = 0;
            return;
        }
    }
    if g.event_player == 0 {
        g.event_player = cast::<i16, _>(g.cur_player);
    }
    // (UI) PauseTicker(5)
    check_price_move(g, ui, rng, env, &mut l);
    if !env.skips(0x689361) {
        super::scenario_event::run_scenario_event_689361(g, ui, rng, env.now, l.price_chg_pct);
    }
    apply_news_effects(g, ui, rng, &mut l);
    if g.scenario_cmd == "ABORT" {
        g.scenario_num = 0;
        g.event_player = 0;
        g.scenario_corp = 0;
        g.scenario_stage = 0;
    }
    // (UI) PauseTicker(2)
    if !env.skips(0x4400d7) {
        update_market_display(g, ui, rng, time_secs(env.now));
    }
}

/// Port of Scenario_CheckPriceMove (0x48eed9).
/// Revalues `ScenarioCorp` (`ValuationMode = 2`) and takes its two-month price change, kept only when the
/// trend agrees (> 10% rising, < -8% falling) and — for a rise of a company that is not the merger-tip
/// target — with chance 1/2. Story 115 records `LastFraudCorp`.
fn check_price_move(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, l: &mut RunLocals) {
    l.corp_id = cast::<i32, _>(g.scenario_corp);
    if 10 < l.corp_id {
        g.valuation_mode = 2;
        calc_stock_price(g, ui, rng, l.corp_id, time_secs(env.now));
        l.price_chg_pct = price_pct_change_months(g, l.corp_id, 2);
        if 10 < l.price_chg_pct && price_trend_direction(g, l.corp_id, 2, 1) <= 0 {
            l.price_chg_pct = 0;
        }
        if l.price_chg_pct < -8 && 0 <= price_trend_direction(g, l.corp_id, 2, 1) {
            l.price_chg_pct = 0;
        }
        if cast::<i32, _>(g.merger_tip_corp) != l.corp_id && 0 < l.price_chg_pct && Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
            l.price_chg_pct = 0;
        }
    }
    if g.scenario_num == 115 {
        g.last_fraud_corp = cast::<i32, _>(g.scenario_corp);
    }
}

/// Port of Scenario_ApplyNewsEffects (0x48f02c).
/// Posts the event's `NewsMsg` (a crop failure raises wheat/corn prices and targets; "loses heavily
/// from trading" lowers them with chance 1/2); a numeric `ScenarioCmd` is a cash payment to the event
/// player (`AccruedPL` +-100); `ScenarioNum = ABS(ScenarioNum)`; "ABORT" clears the scenario
/// (`AdventureCooldown = 1`, `TmpExt1 = 0`); "REDNECK" / "RESTRUC" / "EQFUND" negate their target ids
/// (arming the follow-ups), "STRIKE" starts a strike, "SUPRFUND" a Superfund case; otherwise the new
/// profit margin (`TmpExt1`) is applied (not to holding companies / funds), and a finished event clears
/// the scenario (`CalcProjectedEarnings`, `AdventureCooldown = 2`).
///
/// Bug (#112): the Senator Doofus health-care tax news (industry 56, story 116) tests the caller's local
/// `choice`, which is never assigned: dead code.
fn apply_news_effects(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, l: &mut RunLocals) {
    let __content = crate::content::current();
    if !g.news_msg.is_empty() {
        if instr(1, g.news_msg.as_bytes(), __content.bytes(12907)) < 1 {
            if 0 < instr(1, g.news_msg.as_bytes(), __content.bytes(12908)) && Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                g.wheat_target_price = cast::<f32, _>(cast::<f32, _>(sf(0.83f32) * cast::<Ext, _>(g.wheat_target_price)));
                g.wheat_price = cast::<f32, _>(cast::<f32, _>(sf(0.95f32) * cast::<Ext, _>(g.wheat_price)));
                g.corn_target_price = cast::<f32, _>(cast::<f32, _>(sf(0.8f32) * cast::<Ext, _>(g.corn_target_price)));
                g.corn_price = cast::<f32, _>(cast::<f32, _>(sf(0.93f32) * cast::<Ext, _>(g.corn_price)));
            }
        } else {
            if rng.rnd() * Ext::from_f64(2.0) <= Ext::from_f64(1.0) {
                g.wheat_target_price = cast::<f32, _>(cast::<f32, _>(sf(2.5f32) + cast::<Ext, _>(g.wheat_target_price)));
            } else {
                g.wheat_target_price = cast::<f32, _>(cast::<f32, _>(sf(0.95f32) * cast::<Ext, _>(g.wheat_target_price)));
            }
            if rng.rnd() * Ext::from_f64(2.0) <= Ext::from_f64(1.0) {
                g.corn_target_price = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(2.0) + cast::<Ext, _>(g.corn_target_price)));
            } else {
                g.corn_target_price = cast::<f32, _>(cast::<f32, _>(sf(0.95f32) * cast::<Ext, _>(g.corn_target_price)));
            }
            g.wheat_price = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(1.0) + sf(1.35f32) * cast::<Ext, _>(g.wheat_price)));
            g.corn_price = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(1.0) + sf(1.3f32) * cast::<Ext, _>(g.corn_price)));
        }
        post_news(g, ui, 0, 0);
    }
    let cash_amt = crate::pb::numeric::double(val(g.scenario_cmd.as_bytes())); // DOUBLE
    if cash_amt != Ext::from_f64(0.0) {
        let p = cast::<i32, _>(g.event_player);
        if cash_amt <= Ext::from_f64(0.0) {
            g.accrued_pl[p] = g.accrued_pl[p] - Ext::from_f64(100.0);
        } else {
            g.accrued_pl[p] = Ext::from_f64(100.0) + g.accrued_pl[p];
        }
        g.cash[p] = cash_amt + g.cash[p];
    }
    g.scenario_num = to_integer((cast::<Ext, _>(g.scenario_num)).abs());
    if g.scenario_cmd == "ABORT" {
        g.scenario_num = 0;
        g.scenario_corp = 0;
        g.event_player = 0;
        g.scenario_stage = 0;
        g.adventure_cooldown = 1;
        g.tmp_ext1 = Ext::from_f64(0.0);
        return;
    }
    if g.scenario_cmd == "REDNECK" && 10 < g.adv_offering_corp {
        g.adv_offering_corp = g.adv_offering_corp.wrapping_neg();
        return;
    }
    if g.scenario_cmd == "RESTRUC" && 130 < g.adv_restruct_corp {
        g.adv_restruct_corp = g.adv_restruct_corp.wrapping_neg();
        return;
    }
    if g.scenario_cmd == "STRIKE" {
        g.strike_corp_code = cast::<i16, _>(20000 + cast::<i32, _>(g.scenario_corp));
    }
    if g.scenario_cmd == "SUPRFUND" {
        g.superfund_corp = g.scenario_corp;
        g.superfund_end_year = 4 + g.cur_year;
    }
    if g.scenario_cmd == "EQFUND" && 10 < g.cheat_eq_fund_target {
        g.cheat_eq_fund_target = g.cheat_eq_fund_target.wrapping_neg();
    }
    let new_margin = to_quad(g.tmp_ext1); // -0xac (QUAD)
    g.tmp_ext1 = Ext::from_f64(0.0);
    let corp = cast::<i32, _>(g.scenario_corp);
    if 10 < corp {
        let industry_id = cast::<Ext, _>(g.industry_num[corp]);
        if industry_id == Ext::from_f64(4.0) || industry_id == Ext::from_f64(21.0) || industry_id == Ext::from_f64(22.0) || industry_id == Ext::from_f64(56.0) {
            if g.industry_num[corp] == 56 && g.scenario_num == 116 && l.choice == 2 {
                if (cast::<Ext, _>(new_margin)) <= cast::<Ext, _>(g.profit_margin[corp]) {
                    if g.health_care_tax_rate == 0 && g.difficulty_level == 3 {
                        g.health_care_tax_rate = 3;
                        g.news_msg = s(__content.string(12909));
                    } else {
                        g.news_msg = s(__content.string(12910));
                    }
                } else if 0 < g.health_care_tax_rate {
                    g.health_care_tax_rate = -1;
                    g.news_msg = s(__content.string(12911));
                }
                if !g.news_msg.is_empty() {
                    post_news(g, ui, 0, 0);
                }
            }
            g.profit_margin[corp] = cast::<i16, _>(new_margin);
        } else if industry_id != Ext::from_f64(70.0) && industry_id != Ext::from_f64(71.0) {
            g.profit_margin[corp] = cast::<i16, _>(new_margin);
        }
    }
    if g.event_status[cast::<i32, _>(g.scenario_num)] == 2 {
        g.scenario_num = 0;
        if 10 < g.scenario_corp {
            let earn_corp_id = cast::<i32, _>(g.scenario_corp);
            calc_projected_earnings(g, ui, rng, earn_corp_id);
        }
        g.scenario_corp = 0;
        g.event_player = 0;
        g.scenario_stage = 0;
        g.adventure_cooldown = 2;
    }
}

/// Port of Scenario_PickNext (0x48f819).
/// Chooses the scenario for a new story of `ScenarioCorp`: its industry story while that is not done;
/// otherwise (with chance 1/5, none once 111..120 are all done) a random general story 111..120 (113
/// "insider" preferred for a small, low-margin company whose CEO is the player), falling back to the last
/// one not yet used. 113 needs a U.S. game, the player as CEO, cash >= 111, cap <= 90, margin <= 10 and a
/// public float; 119 a loss-making company 3..70 without positive EntMiscData fields 1/3 (and a negative
/// field 3); 120 industries 4 / 21. No scenario clears the story variables.
fn pick_next(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let corp = cast::<i32, _>(g.scenario_corp);
    let industry_event = g.industry_num[corp]; // -0x98 (INTEGER)
    g.scenario_num = industry_event;
    if g.event_status[cast::<i32, _>(industry_event)] < 2 {
        return;
    }
    if !(cast::<Ext, _>(g.story_events111to120_done) + Ext::from_f64(4.0) < rng.rnd() * Ext::from_f64(5.0)) {
        g.scenario_corp = 0;
        g.scenario_num = 0;
        return;
    }
    g.scenario_num = cast::<i16, _>(rng.rnd_range(120, 111));
    let corp = cast::<i32, _>(g.scenario_corp);
    if corp == g.player_ceo_of[g.cur_player]
        && Ext::from_f64(3.0) < rng.rnd() * Ext::from_f64(4.0)
        && g.market_cap[corp] <= Ext::from_f64(90.0)
        && g.profit_margin[corp] < 11
    {
        g.scenario_num = 113;
    }
    if g.event_status[cast::<i32, _>(g.scenario_num)] != 0 {
        g.scenario_num = 0;
        for event_id in 111..=120 {
            if g.event_status[event_id] == 0 {
                g.scenario_num = cast::<i16, _>(event_id);
            }
        }
    }
    if g.scenario_num == 113 && 10 < g.scenario_corp {
        let corp = cast::<i32, _>(g.scenario_corp);
        if corp == g.player_ceo_of[g.cur_player]
            && Ext::from_f64(111.0) <= g.cash[corp]
            && g.market_cap[corp] <= Ext::from_f64(90.0)
            && g.currency_code == "USA"
            && g.profit_margin[corp] <= 10
        {
            let ctrl_corp_id = corp;
            update_corp_control(g, ui, rng, ctrl_corp_id);
            if g.public_pct == 0 {
                g.scenario_num = 0;
            }
        } else {
            g.scenario_num = 0;
        }
    }
    if g.scenario_num == 119 {
        let corp = cast::<i32, _>(g.scenario_corp);
        let misc_field3 = parse_any(g.ent_misc_data[corp].as_bytes(), b"|", 3);
        let misc_field1 = parse_any(g.ent_misc_data[corp].as_bytes(), b"|", 1);
        if Ext::from_f64(0.0) < val(misc_field3.as_bytes()) || Ext::from_f64(0.0) < val(misc_field1.as_bytes()) {
            g.scenario_num = 0;
        }
        if g.earnings_ytd[corp] <= Ext::from_f64(0.0) && g.earnings_y1[corp] <= Ext::from_f64(0.0) && val(misc_field3.as_bytes()) < Ext::from_f64(0.0) {
            g.scenario_num = 119;
        }
        if g.industry_num[corp] < 3 || 70 < g.industry_num[corp] {
            g.scenario_num = 0;
        }
    } else if g.scenario_num == 120 {
        let corp = cast::<i32, _>(g.scenario_corp);
        if g.industry_num[corp] != 4 && g.industry_num[corp] != 21 {
            g.scenario_num = 0;
        }
    }
    if g.scenario_num == 0 {
        g.scenario_corp = 0;
        g.event_player = 0;
        g.scenario_stage = 0;
    }
}
