//! The insider earnings tip: `CheatInsiderEarningsTip` (0x678965, the cheat menu's "earnings tip" offer)
//! with its GOSUB `InsiderEarningsTip_StoryText` (0x67aa49), and `InsiderEarnTipEvent` (0x67cbdd), which
//! makes the tipped earnings surprise happen (called from `InsiderTradingInvestigation` at the start of
//! the event quarter and from `SimScheduledEvents` at the tipped company's tick).
//!
//! SINGLE pool constants of the original (`fmul dword [..]`) are written `sf(0.9)`.

// Statements keep the original BASIC shape on purpose.
#![allow(clippy::collapsible_if, clippy::collapsible_else_if, clippy::neg_cmp_op_on_partial_ord)]
use crate::pb::real::{Ext, cast};


use crate::logic::control::update_corp_control;
use crate::logic::earnings::calc_projected_earnings;
use crate::logic::finance::{calc_entity_financials, get_controller_type};
use crate::logic::market::select_target_corp;
use crate::logic::news::post_news;
use crate::logic::report::{number_to_words, show_quick_message};
use crate::logic::sim_entity::asset_sales::calc_group_stake_pct;
use crate::logic::sim_entity::quarter_start::clear_insider_tip;
use crate::logic::valuation::{
    calc_analyst_rating, calc_stock_price, price_pct_change_months, price_trend_direction, price_trend_streak,
};
use crate::pb::numeric::{str_num, val};
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{CaseMode, chr, instr, lcase, len, ltrim, parse_any, replace, right, rtrim};
use crate::pb::numeric::to_long;
use crate::state::Globals;
use crate::ui::Ui;

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

/// A SINGLE constant of the original's pool: the f32 value, widened.
fn sf(x: f32) -> Ext {
    cast::<Ext, _>(x)
}

/// `REPLACE x WITH y IN v` (in place).
fn repl(v: &mut PbStr, x: &str, y: &str) {
    *v = replace(v.as_bytes(), x.as_bytes(), y.as_bytes());
}

/// `STR$(x)` of a numeric expression.
fn str15(x: Ext) -> PbStr {
    PbStr::from(str_num(x, 15).as_str())
}

// ---------------------------------------------------------------------------------------------------------
// InsiderEarnTipEvent
// ---------------------------------------------------------------------------------------------------------

/// Port of InsiderEarnTipEvent (0x67cbdd).
/// Plays out the insider earnings tip on `|InsiderEarnTipCorp|` (positive: upside, negative: downside)
/// given to player `InsiderTippee`. The tip is dropped (`ClearInsiderTip`) if the company is gone, is
/// industry 70, or (tipster state > 2) changed industry.
///
/// * In the event quarter (`InsiderTipEventTick = CurQuarter`): the surprise happens — profit margin
///   pushed to 17..27 (up) or -22..-15 (down), growth rate / discretionary spending floors for
///   non-player-controlled companies, market cap x1.1 / x0.9, investor sentiment 45 / -35 — and the
///   projections and analyst rating are refreshed. At the company's own tick (`|tip| = SimCoCount`) the
///   story breaks: a headline depending on the tipster state (1: turnaround / bad prospects; 2: "ace
///   investor" / "smart money"; 3: industry story, x1.05 cap for good news; 4: restated earnings, frauds,
///   discoveries...), the possible end of the insider probe of a tippee who sold out (`InsiderProbePlayer`,
///   margin correction, notoriety -1) and the tip is cleared. Otherwise the quarter's rumour headline is
///   posted (analyst meeting, guidance, insider trading suspicion, price trend).
/// * Before (at the company's tick in the quarter before): profit margin +/-5, an "insiders are
///   accumulating/selling" headline if the tippee holds a position, and no discretionary spending for an
///   upside tip on a company not controlled by a player.
///
/// Headlines pop up (`ShowQuickMessage`) and are posted (`PostNews`), except the pre-event one, which
/// is only posted (with the tippee and the company as news entities).
pub fn insider_earn_tip_event(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    let corp_id = to_long((cast::<Ext, _>(g.insider_earn_tip_corp)).abs());
    if g.insider_earn_tip_corp == 0 {
        return;
    }
    if g.entity_status[corp_id] != 1 || g.industry_num[corp_id] == 70 {
        clear_insider_tip(g);
    }
    if g.insider_earn_tip_corp == 0 || g.insider_tippee == 0 {
        return;
    }
    let tip_industry = g.tip_industry;
    if 2 < g.tipster_state[cast::<i32, _>(g.insider_tippee)] && g.industry_num[corp_id] != tip_industry {
        clear_insider_tip(g);
    }
    if g.insider_earn_tip_corp == 0 || g.insider_tippee == 0 {
        clear_insider_tip(g);
        return;
    }
    let old_rating = calc_analyst_rating(g, ui, rng, corp_id);
    let industry = g.industry_num[corp_id];
    let tippee_id = cast::<i32, _>(g.insider_tippee);
    let mut tippee_holds = 1;
    let mut trade_verb = PbStr::new();
    let mut phrase = PbStr::new();
    if g.insider_earn_tip_corp < 11 {
        if g.insider_earn_tip_corp < -10 {
            let stake = calc_group_stake_pct(g, tippee_id, -corp_id);
            if stake == 0 && g.tmp_ext1 == Ext::from_f64(0.0) {
                tippee_holds = 0;
            }
            trade_verb = s("sell");
            if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                trade_verb = s("dump");
            }
        }
    } else {
        let stake = calc_group_stake_pct(g, tippee_id, corp_id);
        if stake == 0 && g.tmp_ext1 == Ext::from_f64(0.0) {
            tippee_holds = 0;
        }
        trade_verb = s(__content.string(6226));
    }
    g.tmp_ext1 = Ext::from_f64(0.0);
    if cast::<i32, _>(g.insider_tip_event_tick) == g.cur_quarter {
        let mut tone_text: PbStr;
        if g.insider_earn_tip_corp < 1 {
            if industry < 70 {
                let r = rng.rnd_range(5, 1);
                g.profit_margin[corp_id] = g.profit_margin[corp_id].wrapping_sub((cast::<i16, _>(r)).wrapping_add(15));
                if g.profit_margin[corp_id] < -22 {
                    g.profit_margin[corp_id] = -22;
                } else if -15 < g.profit_margin[corp_id] {
                    g.profit_margin[corp_id] = -15;
                }
                if g.disc_spend_pct[corp_id] < 10 && get_controller_type(g, corp_id) != 1 {
                    g.disc_spend_pct[corp_id] = 10;
                }
            }
            tone_text = s(__content.string(6227));
            if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                repl(&mut tone_text, "neg", "comb");
            }
            g.market_cap[corp_id] = g.market_cap[corp_id] * sf(0.9f32);
            g.investor_sentiment[corp_id] = -35;
        } else {
            if industry < 70 {
                let base = cast::<i32, _>(g.profit_margin[corp_id]) + 15;
                let r = rng.rnd_range(5, 1);
                g.profit_margin[corp_id] = (cast::<i16, _>(r)).wrapping_add(cast::<i16, _>(base));
                if g.profit_margin[corp_id] < 28 {
                    if g.profit_margin[corp_id] < 17 {
                        g.profit_margin[corp_id] = 17;
                    }
                } else {
                    g.profit_margin[corp_id] = 27;
                }
                if g.growth_rate[corp_id] < 20 && get_controller_type(g, corp_id) != 1 {
                    g.growth_rate[corp_id] = 20;
                }
            }
            tone_text = s(__content.string(6228));
            g.market_cap[corp_id] = g.market_cap[corp_id] * sf(1.1f32);
            g.investor_sentiment[corp_id] = 45;
        }
        calc_projected_earnings(g, ui, rng, corp_id);
        let new_rating = calc_analyst_rating(g, ui, rng, corp_id);
        let rating_change = old_rating - new_rating;
        g.news_msg = PbStr::new();
        let mut trend_dir = price_trend_direction(g, corp_id, 2, 1);
        let mut trend_streak = price_trend_streak(g, corp_id);
        if trend_dir < 1 && 10 < g.insider_earn_tip_corp {
            trend_dir = 0;
        }
        if -1 < trend_dir && g.insider_earn_tip_corp < -10 {
            trend_dir = 0;
        }
        let tip = g.insider_earn_tip_corp;
        let tippee = cast::<i32, _>(g.insider_tippee);
        if (cast::<i32, _>(tip)).abs() == g.sim_co_count || cast::<i32, _>(g.insider_tip_event_tick) != g.cur_quarter {
            story_breaks(g, rng, corp_id, industry, tippee_holds, trend_dir, &mut phrase);
            if !g.news_msg.is_empty() {
                let mut m = std::mem::take(&mut g.news_msg);
                show_quick_message(g, ui, &mut m);
                g.news_msg = m;
                post_news(g, ui, 0, 0);
            }
            if 0 < g.insider_tippee
                && g.tipster_state[cast::<i32, _>(g.insider_tippee)] == 1
                && (cast::<i32, _>(g.insider_earn_tip_corp)).abs() == g.sim_co_count
                && tippee_holds == 0
            {
                if cast::<i32, _>(g.insider_tippee) == g.insider_probe_player {
                    if 10 < (cast::<i32, _>(g.insider_earn_tip_corp)).abs() && Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(4.0) {
                        g.insider_probe_player = 0;
                    }
                    if g.insider_probe_player == 0 && Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(3.0) {
                        g.news_msg = s(__content.string(6229))
                            + &g.ent_name[cast::<i32, _>(g.insider_tippee)]
                            + __content.string(6230);
                        if g.currency_code == "USA" {
                            repl(&mut g.news_msg, __content.string(2879), "S.E.C.");
                        }
                        let mut m = g.news_msg.clone()
                            + __content.string(6231)
                            + __content.string(6232);
                        show_quick_message(g, ui, &mut m);
                        post_news(g, ui, 0, 0);
                        // BUG 410: indexes ProfitMargin with the signed tip (error 9 for a downside tip).
                        let t = cast::<i32, _>(g.insider_earn_tip_corp);
                        if g.insider_earn_tip_corp < 11 {
                            if g.insider_earn_tip_corp < -10 {
                                g.profit_margin[t] = g.profit_margin[t].wrapping_add(20);
                            }
                        } else {
                            g.profit_margin[t] = g.profit_margin[t].wrapping_sub(20);
                        }
                    }
                }
                let p = cast::<i32, _>(g.insider_tippee);
                g.player_notoriety[p] = g.player_notoriety[p].wrapping_sub(1);
            }
            clear_insider_tip(g);
        } else {
            let sym = g.ent_symbol[corp_id].clone();
            match g.cur_quarter {
                1 => {
                    g.news_msg = sym.clone() + __content.string(6233) + &tone_text + __content.string(6234);
                    if 10 < tip && Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                        g.news_msg =
                            sym.clone() + __content.string(6235) + &g.ent_name[corp_id];
                    }
                    if tip < -10 && Ext::from_f64(2.0) < rng.rnd() * Ext::from_f64(3.0) {
                        g.news_msg = s(__content.string(6236)) + &sym;
                    }
                }
                2 => {
                    g.news_msg = s(__content.string(6237)) + &sym;
                    if tip < -10 {
                        tone_text = s(__content.string(6238));
                        g.news_msg = s(__content.string(6239)) + &sym + &tone_text;
                        if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                            g.news_msg = sym.clone() + __content.string(6240) + &tone_text;
                        }
                    } else if 8 < price_pct_change_months(g, corp_id, 2) && trend_dir == 1 && tippee_holds == 1 {
                        g.news_msg =
                            sym.clone() + __content.string(6241) + &g.ent_name[tippee];
                        if Ext::from_f64(2.0) < rng.rnd() * Ext::from_f64(3.0) {
                            repl(&mut g.news_msg, __content.string(6242), __content.string(6243));
                        }
                    }
                }
                3 => {
                    if rating_change < 1 || 2 < new_rating {
                        if rating_change < 0 && 3 < new_rating {
                            phrase = s("bear");
                        }
                    } else {
                        phrase = s("bull");
                    }
                    if phrase.is_empty() {
                        if tippee_holds == 1 {
                            g.news_msg = s(__content.string(6244)) + &sym;
                            if 0 < trend_dir.abs() {
                                g.news_msg = s(__content.string(6245)) + &sym + __content.string(6246);
                                if trend_dir < 0 {
                                    repl(&mut g.news_msg, "rise", "drop");
                                    repl(&mut g.news_msg, "buy", "sell");
                                }
                            }
                            g.news_msg = g.news_msg.clone() + __content.string(6247);
                            if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(3.0) {
                                if tip < -10 {
                                    trade_verb = s("short");
                                }
                                g.news_msg = s(__content.string(6248))
                                    + &g.ent_name[tippee]
                                    + __content.string(6249)
                                    + &trade_verb
                                    + "ing "
                                    + &sym
                                    + " shares";
                                if 10 < price_pct_change_months(g, corp_id, 2)
                                    && trend_dir == 1
                                    && tippee_holds == 1
                                    && 10 < tip
                                {
                                    g.news_msg = sym.clone()
                                        + __content.string(6250)
                                        + &g.ent_name[tippee]
                                        + __content.string(6251);
                                }
                            }
                        } else {
                            g.news_msg = s(__content.string(6252)) + &sym;
                            if tip < -10 {
                                repl(&mut g.news_msg, __content.string(5806), __content.string(6253));
                            }
                        }
                    } else {
                        g.news_msg = s(__content.string(6254)) + &phrase + "ish on " + &sym + " shares";
                    }
                }
                4 => {
                    if trend_streak == 0 && trend_dir != 0 {
                        trend_streak = trend_dir * 2;
                    }
                    if tip < 11 {
                        phrase = s(__content.string(6255));
                    } else {
                        phrase = s(__content.string(6256));
                    }
                    if phrase.is_empty() || rng.rnd() * Ext::from_f64(2.0) <= Ext::from_f64(1.0) {
                        if trend_streak == 0 {
                            if tippee_holds == 1 {
                                g.news_msg = s(__content.string(6257))
                                    + &g.ent_name[tippee]
                                    + __content.string(6258)
                                    + &sym
                                    + __content.string(6259);
                            }
                        } else {
                            phrase = s(__content.string(6260));
                            if 0 < trend_streak && 20 < price_pct_change_months(g, corp_id, trend_streak) {
                                phrase = s(__content.string(6261));
                            }
                            if trend_streak < 0 {
                                phrase = s(__content.string(6262));
                            }
                            g.news_msg = sym.clone()
                                + " stock "
                                + &phrase
                                + __content.string(6263)
                                + &number_to_words(trend_streak, 0, 0)
                                + " months";
                        }
                    } else {
                        g.news_msg = sym.clone() + __content.string(6264) + &phrase;
                    }
                }
                _ => {}
            }
        }
        if !g.news_msg.is_empty() {
            let mut m = std::mem::take(&mut g.news_msg);
            show_quick_message(g, ui, &mut m);
            g.news_msg = m;
            post_news(g, ui, 0, 0);
        }
    } else {
        if g.insider_earn_tip_corp < 11 {
            g.profit_margin[corp_id] = g.profit_margin[corp_id].wrapping_sub(5);
        } else {
            g.profit_margin[corp_id] = g.profit_margin[corp_id].wrapping_add(5);
        }
        if tippee_holds == 1 {
            let sym = g.ent_symbol[corp_id].clone();
            g.news_msg = s(__content.string(6265)) + &trade_verb + __content.string(6266) + &sym + " stock";
            phrase = s("S.E.C.");
            if !(g.currency_code == "USA") {
                phrase = s(__content.string(2879));
            }
            match g.cur_quarter {
                2 => {
                    g.news_msg = phrase.clone() + __content.string(6267) + &sym;
                }
                3 => {
                    g.news_msg = s(__content.string(6268)) + &sym + __content.string(6269) + &phrase + " probe";
                    if g.insider_earn_tip_corp < -10 {
                        repl(&mut g.news_msg, __content.string(6270), __content.string(6271));
                    }
                }
                4 => repl(&mut g.news_msg, __content.string(6272), __content.string(6273)),
                _ => {}
            }
        }
        if 10 < g.insider_earn_tip_corp && get_controller_type(g, corp_id) != 1 {
            let t = cast::<i32, _>(g.insider_earn_tip_corp);
            g.disc_spend_pct[t] = 0;
        }
        // BUG 414: NewsMsg is not cleared on this path — a pending headline is posted when the tippee
        // holds nothing.
        if !g.news_msg.is_empty() {
            post_news(g, ui, tippee_id, corp_id);
        }
    }
}

/// The headline of the event's "story breaks" branch (part of InsiderEarnTipEvent 0x67cbdd), by the
/// tippee's tipster state; may change market cap, earnings, business assets and `NewMgmtList`.
fn story_breaks(
    g: &mut Globals,
    rng: &mut Rng,
    corp_id: i32,
    industry: i16,
    tippee_holds: i32,
    trend_dir: i32,
    phrase: &mut PbStr,
) {
    let __content = crate::content::current();
    let tip = g.insider_earn_tip_corp;
    let tippee = cast::<i32, _>(g.insider_tippee);
    let sym = g.ent_symbol[corp_id].clone();
    let state = g.tipster_state[tippee];
    if state == 1 {
        if tip < 11 {
            if tip < -10 && tippee_holds == 1 {
                g.news_msg = s(__content.string(6274)) + &sym;
                *phrase = parse_any(g.ent_misc_data[corp_id].as_bytes(), b"|", 3);
                if Ext::from_f64(0.0) < val(phrase.as_bytes()) && g.proj_qtr_earnings[corp_id] < Ext::from_f64(0.0) && trend_dir == -1 {
                    g.news_msg = s(__content.string(6275)) + &sym + __content.string(6276);
                }
            }
        } else {
            g.news_msg = s(__content.string(6277)) + &sym + __content.string(6278);
            if trend_dir == 1 && 14 < price_pct_change_months(g, corp_id, 2) {
                g.news_msg = s(__content.string(6279)) + &g.ent_name[tippee] + __content.string(6280) + &sym + __content.string(6281);
            }
        }
    } else if state == 2 {
        if tip < 11 {
            if tip < -10 && trend_dir == -1 && price_pct_change_months(g, corp_id, 3) < -12 {
                g.news_msg = sym.clone() + __content.string(6282);
                if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                    let name = g.ent_name[tippee].clone();
                    g.news_msg = replace(g.news_msg.as_bytes(), __content.bytes(6283), name.as_bytes());
                }
            }
        } else if trend_dir == 1 && 15 < price_pct_change_months(g, corp_id, 2) && tippee_holds == 1 {
            g.news_msg = s(__content.string(6284)) + &g.ent_name[tippee] + __content.string(6285) + &sym + __content.string(6281);
            if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                repl(&mut g.news_msg, __content.string(6286), __content.string(6287));
            }
        }
    } else if state == 3 {
        if tip < 11 {
            if tip < -10 {
                match industry {
                    1 | 2 | 6 | 10 | 16 | 22 | 24 | 26 | 27 | 29 | 35 | 38 | 39 | 40 | 59 | 65 => {
                        g.news_msg = sym.clone() + __content.string(6288);
                        let mut corp_key = s("000") + &rtrim(ltrim(str15(cast::<Ext, _>(corp_id)).as_bytes()).as_bytes());
                        corp_key = right(corp_key.as_bytes(), 4);
                        if instr(1, g.new_mgmt_list.as_bytes(), corp_key.as_bytes()) < 1 {
                            if len(g.new_mgmt_list.as_bytes()) < 31 {
                                g.new_mgmt_list = g.new_mgmt_list.clone() + &corp_key + "/";
                            }
                            if len(g.new_mgmt_list.as_bytes()) < 11 {
                                g.new_mgmt_list = s("XXXX/") + &g.new_mgmt_list;
                            }
                        }
                    }
                    3 | 13 | 20 | 49 | 51 | 53 | 54 | 55 | 67 | 68 | 69 => {
                        g.news_msg = sym.clone() + __content.string(6289);
                    }
                    4 | 21 => {
                        g.news_msg = s(__content.string(6290)) + &sym + __content.string(6291);
                        if g.currency_code == "USA" {
                            repl(&mut g.news_msg, __content.string(2879), "FDA");
                        }
                    }
                    5 | 62 | 63 | 64 => {
                        g.news_msg = sym.clone() + __content.string(6292);
                    }
                    14 => {
                        g.news_msg = s(__content.string(6293)) + &sym;
                    }
                    36 => {
                        g.news_msg =
                            s(__content.string(6294)) + &g.ent_name[corp_id] + " (" + &sym + __content.string(6295);
                    }
                    37 => {
                        g.news_msg = sym.clone() + __content.string(6296);
                    }
                    56 => {
                        g.news_msg = s(__content.string(6297)) + &sym + __content.string(6298);
                        g.accrued_pl[corp_id] = g.accrued_pl[corp_id] - g.business_assets[corp_id] * sf(0.1f32);
                        g.business_assets[corp_id] = g.business_assets[corp_id] * sf(0.9f32);
                    }
                    _ => {}
                }
            }
        } else {
            match industry {
                3 | 13 | 20 | 49 | 51 | 53 | 54 | 55 | 67 | 68 | 69 => {
                    g.news_msg = s(__content.string(4426)) + &sym + __content.string(6299);
                }
                4 | 21 => {
                    g.news_msg = sym.clone() + __content.string(6300);
                }
                5 => {
                    g.news_msg = sym.clone() + __content.string(6301);
                }
                62..=64 => {
                    g.news_msg = s("Huge ") + &sym + __content.string(6302);
                }
                14 => {
                    g.news_msg = sym.clone() + __content.string(6303);
                }
                36 => {
                    // BUG 411 (cosmetic): no blank between the symbol and "Lands".
                    g.news_msg = sym.clone() + __content.string(6304);
                }
                _ => {
                    g.news_msg = sym.clone() + __content.string(6305);
                }
            }
            g.market_cap[corp_id] = g.market_cap[corp_id] * sf(1.05f32);
        }
    } else if state == 4 {
        if tip < 11 {
            if tip < -10 {
                let c = industry;
                if (1..6).contains(&c) || (12..21).contains(&c) || (26..32).contains(&c) || (34..40).contains(&c) || (51..69).contains(&c) {
                    let f = rng.rnd() * sf(0.1f32) + sf(0.2f32);
                    g.earnings_y1[corp_id] = g.earnings_y1[corp_id] * f;
                    *phrase = s("S.E.C.");
                    if !(g.currency_code == "USA") {
                        *phrase = s("Govt.");
                    }
                    g.news_msg = phrase.clone()
                        + __content.string(6306)
                        + &sym
                        + __content.string(6307)
                        + &str15(cast::<Ext, _>(g.cur_year) - Ext::from_f64(1.0))
                        + __content.string(6308);
                    if rng.rnd() * Ext::from_f64(3.0) <= Ext::from_f64(1.0) {
                        g.earnings_y1[corp_id] = g.earnings_y1[corp_id] * -sf(0.41f32);
                        g.news_msg = g.news_msg.clone() + __content.string(6309);
                    } else {
                        g.news_msg = g.news_msg.clone() + __content.string(6310);
                    }
                } else if c == 7 || c == 8 || c == 9 || c == 50 {
                    *phrase = s(__content.string(6311));
                    if industry == 9 {
                        *phrase = s(__content.string(6312));
                    }
                    if industry == 50 {
                        *phrase = s(__content.string(6313));
                    }
                    g.news_msg = s(__content.string(6314)) + &sym + " " + &*phrase + __content.string(6315);
                } else if c == 11 || c == 25 {
                    g.news_msg = sym.clone() + __content.string(6316);
                } else if c == 22 {
                    g.news_msg = s(__content.string(6317)) + &sym + __content.string(6318);
                } else if c == 33 {
                    g.news_msg = s(__content.string(6319)) + &sym + __content.string(6320);
                    if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                        repl(&mut g.news_msg, __content.string(4683), "Peru");
                        repl(&mut g.news_msg, __content.string(6321), __content.string(6322));
                    }
                } else if (41..=48).contains(&c) {
                    g.news_msg = sym.clone() + __content.string(6323);
                }
            }
        } else {
            match industry {
                7 | 8 | 9 | 12 | 50 => {
                    *phrase = s(__content.string(6324));
                    if industry == 9 {
                        *phrase = s(__content.string(6325));
                    }
                    if industry == 12 {
                        *phrase = s(__content.string(6326));
                    }
                    if industry == 50 {
                        *phrase = s(__content.string(6327));
                    }
                    g.news_msg = sym.clone() + __content.string(6328) + &*phrase;
                }
                10 | 24 | 26 | 35 | 39 => {
                    g.news_msg = s(__content.string(6329)) + &sym;
                }
                30 | 38 | 40 | 61 => {
                    g.news_msg = sym.clone() + __content.string(6330);
                }
                59 => {
                    g.news_msg = s(__content.string(6331)) + &sym + __content.string(6332);
                    g.market_cap[corp_id] = g.market_cap[corp_id] * sf(1.07f32);
                }
                _ => {}
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------
// CheatInsiderEarningsTip
// ---------------------------------------------------------------------------------------------------------

/// The locals of `CheatInsiderEarningsTip` that its GOSUB `InsiderEarningsTip_StoryText` shares
/// (zeroed at entry; they keep their values across the candidate loop).
#[derive(Default)]
struct TipFrame {
    story_text: PbStr,
    position_side: PbStr,
    hook_text: PbStr,
    industry: i16,
    tip_corp_id: i32,
}

/// Port of CheatInsiderEarningsTip (0x678965).
/// The cheat menu's insider earnings tip: an informant (tipster state `TipsterState(CurPlayer)` 0..3: old
/// college chum, junior accountant, audit/tax manager, senior partner) offers a tip on a mid-sized
/// company (market cap 0.5%..20% of the player's net worth, at least 50 currency millions; doubled range
/// after 1500 tries) that is publicly held (> 39% public, controlled by a company), with a big operating
/// business and either poor margins (upside tip, 3 in 5) or healthy earnings (downside tip). Tipster
/// states 2 and 3 tell a story about the company ([`InsiderEarningsTip_StoryText`]).
///
/// If the player accepts (`MSGBOX` YES) and pays `state + 1` million: `InsiderEarnTipCorp` (signed),
/// `InsiderTippee`, `TipIndustry`, event tick/quarter (next quarter / the one after), tiny share prices
/// are lifted (shares x0.25 if below 15), the company is selected (`SelectTargetCorp`), earnings
/// smoothing reserves are set up, the cap moves 3% against the coming news, notoriety +1, the tipster
/// state advances and may trigger an insider probe (`InsiderProbePlayer`, state -1 / 4). The player is
/// told the company, its price or trend, and that the fee was paid (pop-up). Without a candidate (or with
/// a tip already pending) a "nothing for you now" message is shown.
///
/// `time_secs` is passed to `SelectTargetCorp` / `CalcStockPrice` (`TIME$` seconds).
pub fn cheat_insider_earnings_tip(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    let __content = crate::content::current();
    let mut f = TipFrame::default();
    let mut msg_text: PbStr;
    let mut msg_title = PbStr::new();
    let mut target_id: i32 = 0;
    let mut aux_text: PbStr;
    let cp = g.cur_player;
    if g.informant_role.is_empty() {
        g.informant_role = s("audit");
        if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
            g.informant_role = s("tax");
        }
    }
    calc_entity_financials(g, ui, rng, cp);
    let mut min_cap = g.net_worth * sf(0.005f32);
    if min_cap < g.exchange_rate * Ext::from_f64(50.0) {
        min_cap = g.exchange_rate * Ext::from_f64(50.0);
    }
    let mut max_cap = min_cap * Ext::from_f64(40.0);
    let mut direction = 1;
    if rng.rnd() * Ext::from_f64(5.0) < Ext::from_f64(2.0) {
        direction = -1;
    }
    if 0 < (cast::<i32, _>(g.insider_earn_tip_corp)).abs() && g.insider_tippee == 0 {
        clear_insider_tip(g);
    }
    if 0 < g.merger_tip_corp && g.insider_tippee == 0 {
        clear_insider_tip(g);
    }
    if 0 < g.insider_tippee && g.merger_tip_corp == 0 && g.insider_earn_tip_corp == 0 {
        clear_insider_tip(g);
    }
    let mut tries = 1;
    'tries: while tries <= 2000 {
        if 10 < g.merger_tip_corp || 10 < (cast::<i32, _>(g.insider_earn_tip_corp)).abs() {
            break;
        }
        if tries == 1500 {
            max_cap = max_cap * Ext::from_f64(2.0);
        }
        let mut cand_id = rng.rnd_range(1600, 131);
        'cand: {
            if !(g.industry_num[cand_id] < 70 && g.entity_status[cand_id] == 1) {
                break 'cand;
            }
            if !(min_cap <= g.market_cap[cand_id] && g.market_cap[cand_id] <= max_cap) {
                break 'cand;
            }
            update_corp_control(g, ui, rng, cand_id);
            if !(39 < g.public_pct && 10 < g.voting_controller) {
                break 'cand;
            }
            if -3 < g.profit_margin[cand_id] && g.profit_margin[cand_id] < 5 {
                break 'cand;
            }
            calc_entity_financials(g, ui, rng, cand_id);
            aux_text = parse_any(g.ent_misc_data[cand_id].as_bytes(), b"|", 30);
            let c = cand_id;
            if direction == -1 {
                if !(val(aux_text.as_bytes()) <= Ext::from_f64(30.0)) {
                    break 'cand;
                }
                f.industry = g.industry_num[c];
                let ok = g.ind_demand_gap[cast::<i32, _>(f.industry)] <= Ext::from_f64(30.0)
                    && 4 < g.profit_margin[c]
                    && g.profit_margin[c] < 22
                    && Ext::from_f64(0.0) < g.earnings_y1[c]
                    && Ext::from_f64(0.0) <= g.earnings_ytd[c]
                    && g.net_worth * sf(0.014f32) <= g.market_cap[c]
                    && -1 < g.investor_sentiment[c];
                if !ok {
                    break 'cand;
                }
                cand_id = -cand_id;
            } else {
                // direction = 1
                if !(-Ext::from_f64(15.0) <= val(aux_text.as_bytes())) {
                    break 'cand;
                }
                f.industry = g.industry_num[c];
                if !(g.ind_demand_gap[cast::<i32, _>(f.industry)] <= Ext::from_f64(30.0) && g.profit_margin[c] < -2 && -19 < g.profit_margin[c]) {
                    break 'cand;
                }
                if g.net_worth * sf(0.012f32) < g.market_cap[c] && 7 < g.credit_rating_num[c] {
                    break 'cand;
                }
                if !(g.market_cap[c] <= g.net_worth * sf(0.03f32) && g.investor_sentiment[c] < 6) {
                    break 'cand;
                }
            }
            let abs_cand_id = to_long((cast::<Ext, _>(cand_id)).abs());
            if !(g.total_assets * sf(0.6f32) <= g.business_assets[abs_cand_id]) {
                break 'cand;
            }
            f.position_side = s("long");
            if cand_id < 0 {
                f.position_side = s("short");
            }
            f.tip_corp_id = to_long((cast::<Ext, _>(cand_id)).abs());
            let state = g.tipster_state[cp];
            if state == 2 || state == 3 {
                // BUG 412: the story (and its side effects) is produced again below.
                insider_earnings_tip_story_text(g, rng, &mut f);
                if f.story_text.is_empty() && tries < 1500 {
                    break 'cand;
                }
            }
            msg_text = PbStr::new();
            if g.tipster_state[cp] == 0 {
                f.hook_text = s(__content.string(6333));
                if f.position_side == "short" {
                    f.hook_text = s(__content.string(6334));
                }
                msg_text = s(__content.string(6335))
                    + __content.string(6336)
                    + __content.string(6337)
                    + &f.hook_text
                    + ". "
                    + __content.string(6338)
                    + __content.string(6339)
                    + __content.string(6340);
                if f.position_side == "long" {
                    msg_text = msg_text + __content.string(6341);
                } else {
                    msg_text = msg_text
                        + __content.string(6342);
                }
                msg_text = msg_text
                    + __content.string(6343)
                    + "\r"
                    + "\r"
                    + __content.string(6344)
                    + __content.string(6345)
                    + &f.position_side
                    + __content.string(6346)
                    + __content.string(6347)
                    + __content.string(6348)
                    + __content.string(6349)
                    + __content.string(6350)
                    + "\r"
                    + "\r";
            }
            if g.tipster_state[cp] == 1 {
                let mut accountant_title = s(__content.string(6351));
                if g.currency_code == "USA" {
                    accountant_title = s("CPAs");
                }
                msg_text = s(__content.string(6352))
                    + "\r"
                    + "\r"
                    + __content.string(6353)
                    + __content.string(6354)
                    + &accountant_title
                    + __content.string(6355)
                    + __content.string(6356)
                    + __content.string(6357)
                    + "\r"
                    + "\r";
                msg_text = msg_text
                    + __content.string(6358)
                    + __content.string(6359);
                if f.position_side == "long" {
                    msg_text = msg_text + __content.string(6360);
                } else {
                    msg_text = msg_text + __content.string(6361);
                }
                msg_text = msg_text
                    + __content.string(6362)
                    + __content.string(6363)
                    + "\r"
                    + "\r";
            }
            if g.tipster_state[cp] == 2 {
                msg_text = s(__content.string(6364))
                    + &g.informant_role
                    + __content.string(6365)
                    + __content.string(6366)
                    + __content.string(6367)
                    + __content.string(6368)
                    + __content.string(6369);
                if f.position_side == "long" {
                    msg_text = msg_text + __content.string(6370);
                } else {
                    msg_text = msg_text + __content.string(6371);
                }
                if g.sim_co_count < 1100 {
                    msg_text = msg_text + ". ";
                } else {
                    msg_text = msg_text + __content.string(6372);
                }
                insider_earnings_tip_story_text(g, rng, &mut f);
                if f.story_text.is_empty() {
                    if f.position_side == "long" {
                        msg_text = msg_text + __content.string(6373);
                    } else {
                        msg_text = msg_text + __content.string(6374);
                    }
                } else {
                    msg_text = msg_text + "\r" + "\r" + &f.story_text;
                }
                msg_text = msg_text + "\r" + "\r";
            }
            if g.tipster_state[cp] == 3 {
                msg_text = s(__content.string(6375))
                    + &g.informant_role
                    + __content.string(6376)
                    + __content.string(6377)
                    + "to go '"
                    + &f.position_side
                    + __content.string(6378)
                    + __content.string(6379);
                insider_earnings_tip_story_text(g, rng, &mut f);
                if f.story_text.is_empty() {
                    msg_text = msg_text + __content.string(6380);
                } else {
                    msg_text = msg_text + "\r" + "\r" + &f.story_text + "\r" + "\r";
                }
                if g.sim_co_count < 1100 {
                    msg_text = msg_text
                        + __content.string(6381)
                        + __content.string(6382)
                        + "\r"
                        + "\r";
                    if f.position_side == "short" {
                        msg_text = replace(msg_text.as_bytes(), b"buy", f.position_side.as_bytes());
                    }
                } else {
                    msg_text = msg_text
                        + __content.string(6383)
                        + __content.string(6384);
                    if f.position_side == "long" {
                        msg_text = msg_text + "buy ";
                    } else {
                        msg_text = msg_text + "short ";
                    }
                    msg_text = msg_text + __content.string(6385) + "\r" + "\r";
                }
            }
            f.hook_text = PbStr::new();
            if g.tipster_state[cp] < 2 {
                f.hook_text = s(__content.string(6386))
                    + __content.string(6387)
                    + __content.string(6388)
                    + "\r"
                    + "\r";
                if g.tipster_state[cp] != 0 {
                    repl(&mut f.hook_text, __content.string(6389), "client");
                }
            }
            if g.tipster_state[cp] < 2 {
                msg_text = msg_text
                    + &f.hook_text
                    + __content.string(6390)
                    + "\r"
                    + "\r";
            } else {
                msg_text = msg_text + __content.string(6391) + "\r" + "\r";
            }
            msg_text = msg_text
                + __content.string(6392)
                + &rtrim(ltrim(g.currency_symbol.as_bytes()).as_bytes())
                + &rtrim(ltrim(str15(cast::<Ext, _>(cast::<i32, _>(g.tipster_state[cp]) + 1)).as_bytes()).as_bytes())
                + " "
                + &g.million_prefix_lc
                + "illion"
                + &g.currency_suffix
                + ".)";
            msg_title = s(__content.string(6393));
            if g.tipster_state[cp] != 0 {
                msg_title = s(__content.string(5820));
            }
            let answer = ui.msgbox(&msg_text, 8483, &msg_title);
            if answer != 6 {
                // CheatInsiderEarningsTip_Exit
                return;
            }
            if !g.news_msg.is_empty() {
                post_news(g, ui, 0, 0);
            }
            g.insider_earn_tip_corp = cast::<i16, _>(cand_id);
            let st = g.tipster_state[cp];
            g.cash[cp] = g.cash[cp] - cast::<Ext, _>(cast::<i32, _>(st) + 1);
            target_id = to_long((cast::<Ext, _>(cand_id)).abs());
            let t = target_id;
            g.tip_industry = g.industry_num[t];
            if Ext::from_f64(0.0) < g.shares_outstanding[t] && g.market_cap[t] / g.shares_outstanding[t] < Ext::from_f64(15.0) {
                g.shares_outstanding[t] = g.shares_outstanding[t] * sf(0.25f32);
            }
            g.insider_tippee = cast::<i16, _>(cp);
            g.target_corp = t;
            select_target_corp(g, ui, rng, time_secs);
            // (UI) CONTROL SET TEXT hDlgMain, 202, EntSymbol(SelEntity)
            if 10 < g.insider_earn_tip_corp && Ext::from_f64(0.0) < g.contingency_reserve[t] {
                g.reserve_adj_pending[t] = g.contingency_reserve[t] * -Ext::from_f64(1.0);
            } else if g.insider_earn_tip_corp < -10 && g.reserve_adj_pending[t] == Ext::from_f64(0.0) {
                g.reserve_adj_pending[t] = g.proj_qtr_earnings[t].abs() * sf(0.05f32);
            }
            if g.insider_earn_tip_corp < 1 {
                g.market_cap[t] = g.market_cap[t] * sf(1.03f32);
            } else {
                g.market_cap[t] = g.market_cap[t] * sf(0.97f32);
                g.disc_spend_pct[t] = 0;
            }
            g.player_notoriety[cp] = g.player_notoriety[cp].wrapping_add(1);
            g.insider_tip_event_tick = cast::<i16, _>(g.cur_quarter + 1);
            if 4 < g.insider_tip_event_tick {
                g.insider_tip_event_tick = 1;
            }
            g.insider_tip_event_qtr = g.insider_tip_event_tick + 1;
            if 4 < g.insider_tip_event_qtr {
                g.insider_tip_event_qtr = 1;
            }
            g.tipster_state[cp] = g.tipster_state[cp].wrapping_add(1);
            if (cast::<Ext, _>(8 - cast::<i32, _>(g.tipster_state[cp]))) < rng.rnd() * Ext::from_f64(10.0) {
                if g.tipster_state[cp] == 1 {
                    if g.insider_probe_player == 0 && Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                        g.insider_probe_player = cp;
                    }
                    if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                        g.tipster_state[cp] = 4;
                    }
                } else if g.insider_probe_player == 0 {
                    if Ext::from_f64(2.0) < rng.rnd() * Ext::from_f64(5.0) {
                        g.insider_probe_player = cp;
                        if g.tipster_state[cp] == 4 {
                            g.tipster_state[cp] = -1;
                        }
                    }
                }
            }
            break 'tries;
        }
        tries += 1;
    }
    if target_id < 11 {
        calc_entity_financials(g, ui, rng, cp);
        aux_text = PbStr::new();
        if g.exchange_rate * Ext::from_f64(30000.0) < g.net_worth {
            aux_text = s(__content.string(6394))
                + __content.string(6395);
        }
        f.hook_text = s("law");
        if g.tipster_state[cp] != 0 {
            f.hook_text = s(__content.string(5977));
        }
        msg_text = s(__content.string(6396))
            + &f.hook_text
            + __content.string(6397)
            + __content.string(6398);
        if 1 < g.tipster_state[cp] {
            repl(&mut msg_text, __content.string(6399), "Your");
        }
        if g.tipster_state[cp] == 0 {
            if !aux_text.is_empty() {
                repl(&mut aux_text, __content.string(6400), "she");
                msg_text = msg_text + &aux_text + "\r" + "\r";
            }
            msg_text = msg_text
                + __content.string(6401)
                + __content.string(6402)
                + __content.string(6403)
                + __content.string(6404)
                + "\r"
                + "\r"
                + __content.string(6405);
        } else {
            repl(&mut msg_text, " she", " he");
            repl(&mut msg_text, " her", " him");
            repl(&mut msg_text, __content.string(5992), __content.string(5993));
            if g.exchange_rate * Ext::from_f64(30000.0) < g.net_worth {
                msg_text = aux_text;
            }
        }
        ui.msgbox(&msg_text, 8256, &s(__content.string(5994)));
    } else {
        let t = f.tip_corp_id;
        msg_text = s(__content.string(6406))
            + &g.ent_name[t]
            + __content.string(6407)
            + &g.ent_symbol[t]
            + ". "
            + __content.string(6408);
        if g.tipster_state[cp] != 1 {
            msg_text = s(__content.string(6409)) + &g.ent_name[t] + __content.string(6410) + &g.ent_symbol[t] + ". ";
        }
        let trend_streak = price_trend_streak(g, target_id);
        f.hook_text = PbStr::new();
        if trend_streak < -4 && 0 < g.insider_earn_tip_corp {
            f.hook_text = s("\r") + "\r" + __content.string(6411);
        } else if 3 < trend_streak && g.insider_earn_tip_corp < 0 {
            f.hook_text = s("\r")
                + "\r"
                + __content.string(6412)
                + __content.string(6413);
        }
        if f.hook_text.is_empty() {
            g.valuation_mode = 0;
            calc_stock_price(g, ui, rng, target_id, time_secs);
            msg_text = msg_text + "\r" + "\r" + __content.string(6414) + &g.stock_price_str + __content.string(6415);
        } else {
            f.hook_text = f.hook_text.clone() + "for";
            if trend_streak == 12 {
                f.hook_text = f.hook_text.clone() + __content.string(6416);
            } else if trend_streak == 24 {
                f.hook_text = f.hook_text.clone() + __content.string(6417);
            } else {
                f.hook_text = f.hook_text.clone() + &number_to_words(trend_streak, 0, 0) + __content.string(1095);
            }
            if g.sim_co_count < 1100 {
                f.hook_text = f.hook_text.clone() + __content.string(6418);
            }
        }
        ui.msgbox(&(msg_text + &f.hook_text), 8256, &msg_title);
        msg_text = s(__content.string(6419));
        if 1 < g.tipster_state[cp] {
            msg_text = s(__content.string(6420));
        }
        let mut m = s(__content.string(6421)) + &chr(&[34]) + "fee" + &chr(&[34]) + __content.string(6422) + &msg_text;
        show_quick_message(g, ui, &mut m);
    }
    cheat_insider_earnings_tip_exit();
}

/// Port of CheatInsiderEarningsTip_Exit (0x67c220).
/// The routine's exit label (frame release only).
fn cheat_insider_earnings_tip_exit() {}

/// Port of InsiderEarningsTip_StoryText (0x67aa49), a GOSUB of CheatInsiderEarningsTip.
/// For tipster states 2 and 3: the inside story about company `tipCorpId` by industry and position side
/// (`storyText`, prefixed "The company "; unchanged when no story fits). Some stories plant a denial
/// headline in `NewsMsg`; the "managers leave" story (state 2, short) props up the company first (margin
/// +7 and cap x1.03 when the margin is below 12); the "superstar CEO" story draws RND for the pronouns.
fn insider_earnings_tip_story_text(g: &mut Globals, rng: &mut Rng, f: &mut TipFrame) {
    let __content = crate::content::current();
    let t = f.tip_corp_id;
    f.industry = g.industry_num[t];
    let ind = f.industry;
    let cp = g.cur_player;
    if g.tipster_state[cp] == 2 {
        if f.position_side == "long" {
            match ind {
                3 | 13 | 20 | 49 | 51 | 53 | 54 | 55 | 67 | 68 | 69 => {
                    f.story_text = s(__content.string(6423))
                        + __content.string(6424)
                        + __content.string(6425)
                        + __content.string(6426)
                        + __content.string(6427)
                        + __content.string(6428);
                }
                4 | 21 => {
                    f.story_text = s(__content.string(6429))
                        + __content.string(6430)
                        + __content.string(6431)
                        + __content.string(6432);
                }
                5 => {
                    f.story_text = s(__content.string(6433))
                        + __content.string(6434)
                        + __content.string(6435)
                        + __content.string(6436)
                        + __content.string(6437);
                }
                62..=64 => {
                    f.story_text = s(__content.string(6438))
                        + __content.string(6439)
                        + __content.string(6440)
                        + __content.string(6441)
                        + &lcase(g.ind_name[cast::<i32, _>(ind)].as_bytes(), CaseMode::Ansi)
                        + __content.string(6442)
                        + __content.string(6443);
                }
                14 => {
                    f.story_text = s(__content.string(6444))
                        + __content.string(6445)
                        + __content.string(6446)
                        + __content.string(6447);
                }
                36 => {
                    f.story_text = s(__content.string(6448))
                        + __content.string(6449)
                        + __content.string(6450)
                        + __content.string(6451);
                }
                _ => {
                    if t != g.player_ceo_of[cp] {
                        f.story_text = s(__content.string(6452))
                            + __content.string(6453)
                            + __content.string(6454)
                            + __content.string(6455)
                            + __content.string(6456)
                            + __content.string(6457)
                            + __content.string(6458)
                            + __content.string(6459);
                        if g.cur_quarter == 4 {
                            repl(&mut f.story_text, "quarter", "year");
                        }
                        if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                            repl(&mut f.story_text, " he", " she");
                            repl(&mut f.story_text, "him", "her");
                            repl(&mut f.story_text, " his", " her");
                        }
                        g.news_msg = g.ent_symbol[t].clone() + __content.string(6460);
                    }
                }
            }
        } else {
            match ind {
                1 | 2 | 6 | 10 | 16 | 22 | 24 | 26 | 27 | 29 | 35 | 38 | 39 | 40 | 59 | 65 => {
                    f.story_text = s(__content.string(6461))
                        + __content.string(6462)
                        + __content.string(6463)
                        + __content.string(6464)
                        + "\r"
                        + "\r"
                        + __content.string(6465)
                        + __content.string(6466)
                        + __content.string(6467)
                        + __content.string(6468)
                        + __content.string(6469);
                    if g.profit_margin[t] < 12 {
                        g.profit_margin[t] = g.profit_margin[t].wrapping_add(7);
                        g.market_cap[t] = g.market_cap[t] * sf(1.03f32);
                    }
                    g.news_msg = s(__content.string(6470)) + &g.ent_symbol[t] + __content.string(6471);
                }
                3 | 13 | 20 | 49 | 51 | 53 | 54 | 55 | 67 | 68 | 69 => {
                    f.story_text = s(__content.string(6472))
                        + __content.string(6473)
                        + __content.string(6474)
                        + __content.string(6475)
                        + __content.string(6476)
                        + __content.string(6477)
                        + __content.string(6478);
                    g.news_msg = s("New ") + &g.ent_symbol[t] + __content.string(6479);
                }
                4 | 21 => {
                    f.story_text = s(__content.string(6480))
                        + __content.string(6481)
                        + __content.string(6482)
                        + __content.string(6483)
                        + __content.string(6484)
                        + __content.string(6485);
                    if g.currency_code == "USA" {
                        repl(&mut f.story_text, __content.string(1550), "FDA");
                    }
                }
                5 | 62 | 63 | 64 => {
                    f.story_text = s(__content.string(6486))
                        + __content.string(6487);
                }
                14 => {
                    f.story_text = s(__content.string(6488))
                        + __content.string(6489)
                        + __content.string(6490)
                        + __content.string(6491);
                    if !(g.currency_code == "USA") {
                        repl(&mut f.story_text, __content.string(6492), __content.string(6493));
                        repl(&mut f.story_text, __content.string(1550), __content.string(6494));
                    }
                }
                36 => {
                    f.story_text = s(__content.string(6495))
                        + __content.string(6496)
                        + __content.string(6497)
                        + __content.string(6498)
                        + __content.string(6499)
                        + __content.string(6500);
                }
                37 => {
                    f.story_text = s(__content.string(6501))
                        + __content.string(6502)
                        + __content.string(6503)
                        + __content.string(6504)
                        + __content.string(6505)
                        + __content.string(6506);
                }
                56 => {
                    f.story_text = s(__content.string(6507))
                        + __content.string(6508)
                        + __content.string(6509)
                        + __content.string(6510);
                }
                _ => {}
            }
        }
    }
    if g.tipster_state[cp] == 3 {
        if f.position_side == "long" {
            match ind {
                7 | 8 | 9 | 12 | 50 => {
                    f.story_text = s(__content.string(6511))
                        + __content.string(6512)
                        + __content.string(6513)
                        + __content.string(6514)
                        + __content.string(6515);
                    if f.industry == 9 || f.industry == 50 {
                        repl(&mut f.story_text, __content.string(6516), __content.string(6517));
                    } else if f.industry == 12 {
                        repl(&mut f.story_text, __content.string(6516), __content.string(6518));
                    }
                    if f.industry < 9 {
                        f.story_text = f.story_text.clone()
                            + __content.string(6519);
                    } else {
                        repl(&mut f.story_text, __content.string(6520), __content.string(6521));
                    }
                }
                10 | 24 | 26 | 35 | 39 => {
                    f.story_text = s(__content.string(6522))
                        + __content.string(6523)
                        + __content.string(6524)
                        + __content.string(6525)
                        + __content.string(6526)
                        + __content.string(6527)
                        + __content.string(6528);
                }
                30 | 38 | 40 | 61 => {
                    f.story_text = s(__content.string(6529))
                        + __content.string(6530)
                        + __content.string(6531)
                        + __content.string(6532)
                        + __content.string(6533)
                        + __content.string(6534);
                }
                59 => {
                    f.story_text = s(__content.string(6535))
                        + __content.string(6536)
                        + __content.string(6537)
                        + __content.string(6538)
                        + __content.string(6539);
                }
                _ => {}
            }
        } else {
            let c = ind;
            if (1..=6).contains(&c) || (12..=21).contains(&c) || (26..=32).contains(&c) || (34..=40).contains(&c) || (51..=69).contains(&c) {
                if g.cur_quarter < 3 && Ext::from_f64(500.0) <= g.earnings_y1[t] {
                    f.hook_text = s(__content.string(6540));
                    if !(g.currency_code == "USA") {
                        f.hook_text = PbStr::new();
                    }
                    f.story_text = s(__content.string(6541))
                        + &f.hook_text
                        + __content.string(6542)
                        + __content.string(6543)
                        + &str15(cast::<Ext, _>(g.cur_year) - Ext::from_f64(1.0))
                        + ", "
                        + __content.string(6544)
                        + __content.string(6545)
                        + __content.string(6546)
                        + __content.string(6547)
                        + __content.string(6548);
                    g.news_msg = s(__content.string(6549))
                        + &g.ent_symbol[t]
                        + &str15(cast::<Ext, _>(g.cur_year) - Ext::from_f64(1.0))
                        + __content.string(6550);
                    if g.currency_code == "USA" {
                        repl(&mut g.news_msg, __content.string(2879), "S.E.C.");
                    }
                }
            } else if c == 7 || c == 8 || c == 9 || c == 50 {
                f.story_text = s(__content.string(6551))
                    + __content.string(6552)
                    + __content.string(6553)
                    + __content.string(6554)
                    + __content.string(6555)
                    + __content.string(6556)
                    + __content.string(6557);
                if f.industry == 9 {
                    repl(&mut f.story_text, __content.string(6520), __content.string(6558));
                }
                if f.industry == 50 {
                    repl(&mut f.story_text, __content.string(6520), __content.string(6313));
                }
            } else if c == 11 || c == 25 {
                f.story_text = s(__content.string(6559))
                    + __content.string(6560)
                    + __content.string(6561)
                    + __content.string(6562)
                    + __content.string(6563);
                if f.industry == 25 {
                    repl(&mut f.story_text, __content.string(6564), __content.string(6565));
                    repl(&mut f.story_text, __content.string(6566), "freeway");
                }
            } else if c == 22 {
                f.story_text = s(__content.string(6567))
                    + __content.string(6568)
                    + __content.string(6569)
                    + __content.string(6570)
                    + __content.string(6571);
            } else if c == 33 {
                f.story_text = s(__content.string(6572))
                    + __content.string(6573)
                    + __content.string(6574)
                    + __content.string(6575);
            } else if (41..=48).contains(&c) {
                f.story_text = s(__content.string(6576))
                    + __content.string(6577)
                    + __content.string(6578)
                    + __content.string(6579)
                    + __content.string(6580)
                    + __content.string(6581)
                    + __content.string(6582)
                    + __content.string(6583);
            }
        }
    }
    if !f.story_text.is_empty() {
        f.story_text = s(__content.string(1927)) + &f.story_text;
    }
}
