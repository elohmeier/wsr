//! The trading-desk refresh `UpdateMarketDisplay` (0x4400d7) and its market-tick helpers:
//! `RebaseStockIndex` (0x61a2fc), the main-window selection helpers `SelectTargetCorp` (0x4227d5),
//! `RefreshSelectionLabels` (0x424da3), `UpdateControllerLabel` (0x46d8d5), `UpdateSelectLastButton`
//! (0x473686), `ShowNextEarningsDate` (0x4a070a), and the streaming-quote list `ToggleStreamingQuote`
//! (0x5dda89) / `RebuildStreamingQuotesPanel` (0x5dcee5). `UpdateCommodityPrices` (0x62a6a2) is in
//! [`commodities`].
//!
//! Only state effects are ported; window labels and controls are skipped with `// (UI)`. SINGLE pool
//! constants of the original (`fmul dword [0x75e…]`) are written `sf(0.05)`; SINGLE globals are rounded
//! with `as f32` on every store (`fstp dword`).

// Statements keep the original BASIC shape on purpose.
#![allow(
    clippy::assign_op_pattern,
    clippy::manual_range_contains,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord
)]
use crate::pb::real::{Ext, cast};


pub mod commodities;

use crate::logic::control::update_corp_control;
use crate::logic::earnings::get_next_earnings_date_str;
use crate::logic::finance::calc_entity_financials;
use crate::logic::news::post_news;
use crate::logic::report::show_quick_message;
use crate::logic::reports_lists::update_streaming_quotes;
use crate::logic::valuation::{
    calc_stock_price, check_stock_split, price_trend_direction, price_trend_streak, scale_price_history,
    update_main_commodity_display, update_price_history_tick,
};
use crate::pb::numeric::format;
use crate::pb::numeric::{str_num, val};
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{instr, ltrim, mid, parse_any, replace, rtrim, ucase, CaseMode};
use crate::pb::numeric::{to_long, to_quad};
use crate::state::Globals;
use crate::ui::Ui;

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

/// A SINGLE constant of the original (`dword [0x75e…]`), widened to EXT.
fn sf(x: f32) -> Ext {
    cast::<Ext, _>(x)
}

/// `RTRIM$(LTRIM$(STR$(x)))` (PB_StrNum, 15 digits).
fn str_trim(x: Ext) -> PbStr {
    let t = str_num(x, 15);
    rtrim(ltrim(t.as_bytes()).as_bytes())
}

/// `RTRIM$(LTRIM$(FORMAT$(x, mask)))`.
fn fmt_trim(x: Ext, mask: &str) -> PbStr {
    let t = format(x, mask.as_bytes());
    rtrim(ltrim(&t).as_bytes())
}

/// Port of UpdateMarketDisplay (0x4400d7), state effects only.
/// Refreshes the trading desk of the main window and runs a small market tick:
///
/// * fixes the selection (`SelEntity` / `ActiveEntity`; a selected company that no longer exists
///   falls back to the active company via `SelectTargetCorp`, or to the player), recomputes control of
///   the selected company (`UpdateCorpControl`, `VotingController`) and refreshes its streaming quotes;
/// * `CalcEntityFinancials(CurPlayer)` for the cash / assets / debt / net-worth labels;
/// * unless the game has ended: re-values a random fund 1581..1600 (`ValuationMode = 2`) and, with
///   probability 1/150 each, every active company (in the current `ValuationMode`), appending quiet
///   movers (< 2.3 % change, price below a random 150..900) and the bitcoin / ethereum price (once it
///   moved 2 %) to `TickerText` while it is shorter than 150 characters; companies with a market cap
///   <= 0 or >= 1e9 are set to their target value (`ValuationMode = 3`) and checked for a split;
/// * recomputes `StockIndex` (sum of positive market caps of non-funds / `StockIndexDivisor`; in the
///   first quarter of a new game the divisor is calibrated to `StockIndexBase` after a 15 % haircut of all
///   caps; an implausible sum makes the index drift randomly instead), posts "MARKET MOVES SHARPLY …"
///   news for big moves (with a pop-up headline and `LastMarketHeadline`), and rebases the index outside
///   1000..90000 (`RebaseStockIndex`);
/// * rounds `GdpGrowth` to 0.1, nudges one commodity price at random (by `RandSeed MOD 6`), takes the
///   absolute value of all of them and runs `UpdateCommodityPrices` (1 in 5) or
///   `UpdateMainCommodityDisplay`, then records the history tick of the index, players, commodities and
///   rates.
///
/// Draws `RND` about 1,600 times. `time_secs` = `VAL(RIGHT$(TIME$, 2))` for `CalcStockPrice` (see
/// [`calc_stock_price`]). The ON ERROR handler only shows `ShowRuntimeError` (not ported); `SLEEP 1000`
/// after the market news is not ported.
pub fn update_market_display(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    // priceFmt = "* #######,.00" (labels only)
    let mut market_msg = PbStr::new();
    let mut entity_id = g.sel_entity;
    if 0 < g.sel_entity {
        if 1600 < g.sel_entity || 1600 < g.active_entity {
            return; // EntityStatus(): error 9 -> handler
        }
        if g.entity_status[g.sel_entity] != 1 {
            if g.active_entity < 11 || g.entity_status[g.active_entity] != 1 {
                g.sel_entity = g.cur_player;
                g.sel_corp_name = g.cur_player_name.clone();
                g.active_entity = g.cur_player;
                g.active_entity_name = g.cur_player_name.clone();
                refresh_selection_labels(g);
                // (UI) EnableMainControls
            } else {
                g.target_corp = g.active_entity;
                g.lookup_entity_name = g.active_entity_name.clone();
                g.sel_entity = 0;
                g.tmp_symbol = g.ent_symbol[g.active_entity].clone();
                select_target_corp(g, ui, rng, time_secs);
            }
            entity_id = g.sel_entity;
        }
    }
    if 10 < entity_id {
        if g.sel_industry == 0 {
            g.sel_industry = cast::<i32, _>(g.industry_num[g.sel_entity]);
        }
        update_corp_control(g, ui, rng, entity_id);
        if g.cur_player == g.voting_controller {
            g.active_entity = g.sel_entity;
            g.active_entity_name = g.sel_corp_name.clone();
        } else if g.active_entity == entity_id {
            g.active_entity = g.cur_player;
            g.active_entity_name = g.cur_player_name.clone();
            refresh_selection_labels(g);
            // (UI) EnableMainControls
        }
    }
    if 0 < entity_id {
        update_controller_label(g);
    }
    // (UI) label 176: share price of the selected company ("N/A" for players)
    if 10 < entity_id {
        update_streaming_quotes(g, ui, rng, entity_id - 1, entity_id + 1);
    }
    let player_id = g.cur_player;
    calc_entity_financials(g, ui, rng, player_id);
    // (UI) labels 208/211/214/217/218: cash, other assets, total assets, debt, net worth;
    // label 112: "Transactions Remaining: " (5 - TurnActionCount)
    if Ext::from_f64(0.0) < g.stock_index && g.stock_index < Ext::from_f64(100000.0) {
        g.prev_market_index = g.stock_index;
    }
    if !(g.game_end_flag == "Y") {
        market_tick(g, ui, rng, time_secs, &mut market_msg);
    }
    // LAB_00441d4d
    if g.stock_index >= Ext::from_f64(90000.0) || !(g.stock_index >= Ext::from_f64(1000.0)) {
        rebase_stock_index(g, ui);
    }
    // (UI) label 114: RIGHT$(FORMAT$(StockIndex, priceFmt), 11) if changed
    if !market_msg.is_empty() {
        show_quick_message(g, ui, &mut market_msg);
    }
    let tenths = to_long(Ext::from_f64(10.0) * cast::<Ext, _>(g.gdp_growth));
    g.gdp_growth = cast::<f32, _>(cast::<f32, _>(sf(0.1f32) * cast::<Ext, _>(tenths)));
    // (UI) label 153: GDP growth
    let game_over = |g: &Globals| g.game_end_flag == "Y";
    if g.rand_seed % 6 == 1 && !game_over(g) {
        let r = rng.rnd();
        g.oil_price = cast::<f32, _>(cast::<f32, _>((r * sf(0.01f32) + cast::<Ext, _>(g.oil_price)) - sf(0.005f32)));
    }
    g.oil_price = cast::<f32, _>(g.oil_price.abs());
    g.gold_price = cast::<f32, _>(g.gold_price.abs());
    g.silver_price = cast::<f32, _>(g.silver_price.abs());
    g.wheat_price = cast::<f32, _>(g.wheat_price.abs());
    g.corn_price = cast::<f32, _>(g.corn_price.abs());
    if g.rand_seed % 6 == 2 && !game_over(g) {
        let r = rng.rnd();
        g.gold_price = cast::<f32, _>(cast::<f32, _>((r * Ext::from_f64(4.0) + cast::<Ext, _>(g.gold_price)) - Ext::from_f64(2.0)));
    }
    if g.rand_seed % 6 == 3 && !game_over(g) && Ext::from_f64(5.0) < cast::<Ext, _>(g.silver_price) {
        let r = rng.rnd();
        g.silver_price = cast::<f32, _>(cast::<f32, _>((r * sf(0.8f32) + cast::<Ext, _>(g.silver_price)) + sf(0.4f32)));
    }
    if g.rand_seed % 6 == 4 && !game_over(g) {
        let r = rng.rnd();
        g.wheat_price = cast::<f32, _>(cast::<f32, _>((r * sf(0.1f32) + cast::<Ext, _>(g.wheat_price)) - sf(0.05f32)));
    }
    if g.rand_seed % 6 == 5 && !game_over(g) {
        let r = rng.rnd();
        g.corn_price = cast::<f32, _>(cast::<f32, _>((r * sf(0.7f32) + cast::<Ext, _>(g.corn_price)) - sf(0.35f32)));
    }
    if !game_over(g) {
        let r = rng.rnd();
        if r * Ext::from_f64(5.0) >= Ext::from_f64(1.0) {
            update_main_commodity_display(g);
        } else {
            commodities::update_commodity_prices(g, ui, rng);
        }
        if !game_over(g) {
            for e in 0..=10 {
                update_price_history_tick(g, ui, rng, e);
            }
            for e in 1601..=1604 {
                update_price_history_tick(g, ui, rng, e);
            }
        }
    }
}

/// The part of UpdateMarketDisplay that runs while the game is on (0x440701..0x441d4d): random
/// revaluations and ticker, the stock index and the market news.
fn market_tick(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32, market_msg: &mut PbStr) {
    let __content = crate::content::current();
    // a random fund / late company 1581..1600, damped valuation
    let entity_id = rng.rnd_range(1600, 1581);
    if g.entity_status[entity_id] == 1 {
        let prev_market_cap = g.market_cap[entity_id];
        g.valuation_mode = 2;
        calc_stock_price(g, ui, rng, entity_id, time_secs);
        if (g.market_cap[entity_id] - prev_market_cap).abs() < prev_market_cap * sf(0.023f32) && g.ticker_text.len() < 150 {
            let r = cast::<Ext, _>(rng.rnd_range(900, 150));
            if val(g.stock_price_str.as_bytes()) < r {
                append_ticker_quote(g, entity_id);
            }
        }
    }
    let mut crypto_ticker_done = 0;
    for entity_id in 11..=1600 {
        if g.entity_status[entity_id] != 1 {
            continue;
        }
        if crypto_ticker_done == 0 {
            if 0 < instr(1, g.ticker_text.as_bytes(), b"TCOIN") || 0 < instr(1, g.ticker_text.as_bytes(), b"ETHEREUM") {
                crypto_ticker_done = 1;
            } else {
                if g.rand_seed % 10 > 4 {
                    // first sight: just remember the price; else quote a 2 % move
                    if g.btc_price_last_news == 0 || crypto_quote(g, __content.string(6588), g.bitcoin_price, g.btc_price_last_news) {
                        g.btc_price_last_news = g.bitcoin_price;
                    } else {
                        crypto_ticker_done = crypto_ticker_done - 1;
                    }
                } else {
                    // first sight: just remember the price; else quote a 2 % move
                    if g.eth_price_last_news == 0 || crypto_quote(g, __content.string(6589), g.ethereum_price, g.eth_price_last_news) {
                        g.eth_price_last_news = g.ethereum_price;
                    } else {
                        crypto_ticker_done = crypto_ticker_done - 1;
                    }
                }
                crypto_ticker_done = crypto_ticker_done + 1;
            }
        }
        // LAB_00440c5a
        if rng.rnd_range(150, 1) == 7 {
            let prev_market_cap = g.market_cap[entity_id];
            calc_stock_price(g, ui, rng, entity_id, time_secs);
            if g.market_cap[entity_id] >= sf(0.25f32) {
                let r = cast::<Ext, _>(rng.rnd_range(900, 150));
                if val(g.stock_price_str.as_bytes()) < r
                    && (g.market_cap[entity_id] - prev_market_cap).abs() < prev_market_cap * sf(0.023f32)
                    && g.ticker_text.len() < 150
                {
                    let r = cast::<Ext, _>(rng.rnd_range(900, 150));
                    if val(g.stock_price_str.as_bytes()) < r {
                        append_ticker_quote(g, entity_id);
                    }
                }
            }
        }
    }
    g.stock_index = Ext::from_f64(0.0);
    for entity_id in 11..=1600 {
        if g.entity_status[entity_id] != 1 || g.industry_num[entity_id] == 71 {
            continue;
        }
        let cap = g.market_cap[entity_id];
        if !(cap > Ext::from_f64(0.0)) || cap >= Ext::from_f64(1_000_000_000.0) {
            g.valuation_mode = 3;
            calc_stock_price(g, ui, rng, entity_id, time_secs);
            check_stock_split(g, entity_id);
        } else {
            g.stock_index = g.stock_index + g.market_cap[entity_id];
        }
    }
    if g.exchange_rate == Ext::from_f64(0.0) || g.exchange_rate.is_nan() {
        g.exchange_rate = cast::<f64, _>(Ext::from_f64(1.0));
    }
    if g.stock_index_base == 0 {
        g.stock_index_base = 5000;
    }
    if g.stock_index_divisor > Ext::from_f64(0.0) || g.sim_co_count != 6 || g.cur_quarter != 1 || g.start_year != g.cur_year {
        if !(g.prev_market_index > Ext::from_f64(0.0)) || g.prev_market_index >= Ext::from_f64(100000.0) || !(g.stock_index_divisor > Ext::from_f64(0.0)) {
            if !(g.stock_index_divisor > Ext::from_f64(0.0)) {
                g.stock_index_divisor = Ext::from_f64(150.0);
            }
        } else if !(g.stock_index.abs() > Ext::from_f64(0.0)) || g.stock_index / g.stock_index_divisor > Ext::from_f64(150000.0) {
            // implausible sum: the index drifts from its previous value instead
            let r = rng.rnd();
            if r * Ext::from_f64(2.0) <= Ext::from_f64(1.0) {
                let r = rng.rnd();
                g.prev_market_index = (r * sf(0.01f32) + sf(0.999f32)) * g.prev_market_index;
            } else {
                let r = rng.rnd();
                g.prev_market_index = (r * sf(0.012f32) + Ext::from_f64(1.0)) * g.prev_market_index;
            }
            g.stock_index = g.prev_market_index;
            return; // GOTO 0x441d4d
        }
    } else {
        // first update of a new game: calibrate the divisor to StockIndexBase
        g.stock_index = Ext::from_f64(0.0);
        for entity_id in 11..=1600 {
            if g.entity_status[entity_id] != 1 || g.industry_num[entity_id] == 71 {
                continue;
            }
            if g.earnings_y1[entity_id] > Ext::from_f64(0.0) {
                let r = rng.rnd();
                g.earnings_y1[entity_id] = (r * Ext::from_f64(3.0) + Ext::from_f64(2.0)) * g.market_cap[entity_id];
            }
            g.market_cap[entity_id] = sf(0.85f32) * g.market_cap[entity_id];
            if g.market_cap[entity_id] > Ext::from_f64(0.0) {
                g.stock_index = g.stock_index + g.market_cap[entity_id];
            }
            update_price_history_tick(g, ui, rng, entity_id);
        }
        g.stock_index_divisor = g.stock_index / cast::<Ext, _>(g.stock_index_base);
        g.prev_market_index = Ext::from_f64(0.0);
    }
    g.stock_index = g.stock_index / g.stock_index_divisor;
    if !(g.prev_market_index > Ext::from_f64(0.0)) {
        return;
    }
    let prev = g.prev_market_index;
    let mut index_change = g.stock_index - prev;
    if !(index_change.abs() > Ext::from_f64(200.0)) || index_change.abs() < prev * sf(0.05f32) {
        index_change = Ext::from_f64(0.0);
    }
    if index_change.abs() < prev * sf(0.08f32) {
        let r = rng.rnd();
        if r * Ext::from_f64(10.0) < Ext::from_f64(9.0) {
            index_change = Ext::from_f64(0.0);
        }
    }
    if g.skip_index_move_news == 1 {
        index_change = Ext::from_f64(0.0);
        g.skip_index_move_news = 0;
    }
    if g.start_year == g.cur_year && g.cur_quarter == 1 && g.sim_co_count <= 500 {
        return;
    }
    if !(index_change.abs() > Ext::from_f64(200.0)) && !(prev * sf(0.05f32) < index_change.abs()) {
        return;
    }
    market_news(g, ui, rng, index_change, market_msg);
}

/// `TickerText = TickerText + "   " + EntName(e) + " (" + EntSymbol(e) + ") " + StockPriceStr + TickerChangeStr`.
fn append_ticker_quote(g: &mut Globals, e: i32) {
    g.ticker_text = g.ticker_text.clone()
        + "   "
        + &g.ent_name[e]
        + " ("
        + &g.ent_symbol[e]
        + ") "
        + &g.stock_price_str
        + &g.ticker_change_str;
}

/// The bitcoin / ethereum ticker quote of UpdateMarketDisplay: sets `TickerChangeStr` to the change
/// since the last quote and, if it is at least 2 % of the price, appends `label + price + "_[+]change"`
/// to `TickerText` and returns `true`.
fn crypto_quote(g: &mut Globals, label: &str, price: i32, last: i32) -> bool {
    let change = cast::<Ext, _>(price) - cast::<Ext, _>(last); // fild / fisubr
    g.ticker_change_str = str_trim(change);
    if !(cast::<Ext, _>(price) * sf(0.02f32) <= change.abs()) {
        return false;
    }
    if mid(g.ticker_change_str.as_bytes(), 1, 1) == "-" {
        g.ticker_change_str = s("_") + &g.ticker_change_str;
    } else {
        g.ticker_change_str = s("_+") + &g.ticker_change_str;
    }
    g.ticker_text = g.ticker_text.clone() + label + &str_trim(cast::<Ext, _>(price)) + &g.ticker_change_str;
    true
}

/// The market-move news of UpdateMarketDisplay (0x441416..0x441d48): headline (varied by `RandSeed MOD 4`,
/// trend phrases when it repeats `LastMarketHeadline`), the pop-up text for the caller, the reason
/// phrase and `PostNews`.
fn market_news(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, index_change: Ext, market_msg: &mut PbStr) {
    let __content = crate::content::current();
    let direction;
    let mut headline;
    if index_change > Ext::from_f64(0.0) {
        direction = s("higher");
        headline = s(__content.string(6590));
        if g.rand_seed % 4 == 1 {
            headline = s(__content.string(6591));
        }
        if g.rand_seed % 4 == 2 {
            headline = s(__content.string(6592));
        }
        if g.rand_seed % 4 == 3 {
            headline = s(__content.string(6593));
        }
        if headline == g.last_market_headline && price_trend_direction(g, 0, 3, 1) == 1 {
            headline = s(__content.string(6594));
            if price_trend_direction(g, 0, 6, 1) == 1 {
                headline = replace(headline.as_bytes(), __content.bytes(6595), __content.bytes(6596));
            }
        }
    } else {
        direction = s("lower");
        headline = s(__content.string(6597));
        if g.rand_seed % 4 == 1 {
            headline = s(__content.string(6598));
        }
        if g.rand_seed % 4 == 2 {
            headline = s(__content.string(6599));
        }
        if g.rand_seed % 4 == 3 {
            headline = s(__content.string(6600));
        }
        if headline == g.last_market_headline && price_trend_direction(g, 0, 3, 1) == -1 {
            headline = s(__content.string(6601));
            if price_trend_direction(g, 0, 6, 1) == -1 {
                headline = headline + __content.string(6602);
            }
        }
    }
    if g.suppress_popups == 0 {
        let r = rng.rnd();
        if r * Ext::from_f64(3.0) > Ext::from_f64(2.0) && !(g.last_market_headline == headline) {
            g.last_market_headline = headline.clone();
            *market_msg = headline.clone()
                + __content.string(6603)
                + &fmt_trim(g.stock_index, "* ###,")
                + ", some "
                + &fmt_trim(index_change.abs(), "* ###,")
                + __content.string(6604)
                + &direction
                + ".";
        }
    }
    let bir = cast::<Ext, _>(g.base_interest_rate);
    let gdp = cast::<Ext, _>(g.gdp_growth);
    let gdp_target = cast::<Ext, _>(g.gdp_growth_target);
    let policy = g.monetary_policy;
    headline = PbStr::new();
    if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(2.0) {
        headline = s(__content.string(6605));
    }
    if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0) && index_change < Ext::from_f64(0.0) && sf(8.5f32) > bir && policy == 2 {
        headline = s(__content.string(6606));
    }
    if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0)
        && index_change < Ext::from_f64(0.0)
        && sf(7.8f32) > bir
        && policy > 0
        && price_trend_direction(g, 1602, 3, 1) == 1
    {
        headline = s(__content.string(6607));
    }
    if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0) && index_change > Ext::from_f64(0.0) && sf(7.8f32) < bir && policy == 0 {
        headline = s(__content.string(6608));
    }
    if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0)
        && index_change > Ext::from_f64(0.0)
        && sf(7.2f32) < bir
        && policy < 2
        && price_trend_direction(g, 1601, 4, 1) == -1
    {
        headline = s(__content.string(6609));
    }
    if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0)
        && index_change < Ext::from_f64(0.0)
        && gdp - Ext::from_f64(2.0) > gdp_target
        && sf(2.8f32) > gdp
        && price_trend_streak(g, 1604) < -1
    {
        headline = s(__content.string(6610));
    }
    if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0)
        && index_change > Ext::from_f64(0.0)
        && gdp + Ext::from_f64(2.0) < gdp_target
        && sf(2.1f32) < gdp
        && price_trend_streak(g, 1604) > 1
    {
        headline = s(__content.string(6611));
    }
    g.news_msg = s(__content.string(6612)) + &ucase(direction.as_bytes(), CaseMode::Ansi) + &headline;
    post_news(g, ui, 0, 0);
    // SLEEP 1000
}

/// Port of RebaseStockIndex (0x61a2fc).
/// Keeps the stock index in range: above 89,000 the divisor is multiplied by 5 (index / 5, history
/// scaled by -80 %, `StockIndexBase` / 5), below 1,100 divided by 5 (index x5, history +400 %, base x5);
/// tells the player (`ShowQuickMessage`) and rescales the open stock-index futures (`FuturesRecs` "STOCKS"
/// records: quantity x5 / x0.2, entry price x0.2 / x5) and the stock-index price alerts (type 3009).
/// An index of exactly 0 is left alone.
pub fn rebase_stock_index(g: &mut Globals, ui: &mut dyn Ui) {
    let __content = crate::content::current();
    if g.stock_index == Ext::from_f64(0.0) || g.stock_index.is_nan() {
        return; // fcompp / jne: also for an unordered (NaN) index
    }
    let qty_factor: Ext;
    let price_factor: Ext;
    let mut msg_text;
    if g.stock_index > Ext::from_f64(89000.0) {
        g.stock_index_divisor = Ext::from_f64(5.0) * g.stock_index_divisor;
        scale_price_history(g, 0, -80);
        g.stock_index = sf(0.2f32) * g.stock_index;
        if g.stock_index_base == 0 {
            g.stock_index_base = 5000;
        }
        g.stock_index_base = to_long(sf(0.2f32) * cast::<Ext, _>(g.stock_index_base));
        msg_text = s(__content.string(6613))
            + __content.string(6614);
        qty_factor = Ext::from_f64(5.0);
        price_factor = sf(0.2f32);
    } else if !(g.stock_index >= Ext::from_f64(1100.0)) {
        g.stock_index_divisor = sf(0.2f32) * g.stock_index_divisor;
        scale_price_history(g, 0, 400);
        g.stock_index = Ext::from_f64(5.0) * g.stock_index;
        if g.stock_index_base == 0 {
            g.stock_index_base = 5000;
        }
        g.stock_index_base = g.stock_index_base.wrapping_mul(5);
        msg_text = s(__content.string(6615))
            + __content.string(6616);
        qty_factor = sf(0.2f32);
        price_factor = Ext::from_f64(5.0);
    } else {
        return;
    }
    show_quick_message(g, ui, &mut msg_text);
    let bar = b"|";
    for rec_idx in 1..=1000 {
        if g.futures_recs[rec_idx].is_empty() {
            continue;
        }
        let rec = g.futures_recs[rec_idx].clone();
        if instr(1, g.futures_recs[rec_idx].as_bytes(), b"STOCKS") == 0 {
            continue;
        }
        let field = |n: i32| val(parse_any(rec.as_bytes(), bar, n).as_bytes());
        let holder_id = to_long(field(2));
        let qty = to_quad(qty_factor * field(3));
        let exp_month = to_long(field(4));
        let exp_year = to_long(field(5));
        let cost_basis = field(6);
        let entry_price = price_factor * field(7);
        g.futures_recs[rec_idx] = s("STOCKS|")
            + &str_trim(cast::<Ext, _>(holder_id))
            + "|"
            + &str_trim(cast::<Ext, _>(qty))
            + "|"
            + &str_trim(cast::<Ext, _>(exp_month))
            + "|"
            + &str_trim(cast::<Ext, _>(exp_year))
            + "|"
            + &str_trim(cost_basis)
            + "|"
            + &str_trim(entry_price);
    }
    for rec_idx in 1..=100 {
        if g.price_alerts[rec_idx].is_empty() {
            continue;
        }
        let rec = g.price_alerts[rec_idx].clone();
        let field = |n: i32| parse_any(rec.as_bytes(), bar, n);
        if val(field(1).as_bytes()) != Ext::from_f64(3009.0) {
            continue;
        }
        let alert_price_text = fmt_trim(price_factor * val(field(4).as_bytes()), "* ###.00");
        g.price_alerts[rec_idx] = s("3009|")
            + &field(2)
            + "|"
            + &field(3)
            + "|"
            + &alert_price_text
            + "|"
            + &field(5)
            + "|"
            + &field(6)
            + "|"
            + &field(7);
    }
}

/// Port of SelectTargetCorp (0x4227d5), state effects only.
/// Makes `TargetCorp` the selected entity (unless it already is, or is 0): remembers the previous
/// selection for the "Select Last" button, sets `SelEntity` / `SelCorpName`, prices the company
/// (`ValuationMode = 0`), recomputes its financials and control (`UpdateCorpControl`; a company the
/// current player controls also becomes the `ActiveEntity`), `SelIndustry`, and auto-adds it to the
/// streaming quotes (`AutoAddStreamList = 1`, game not over). Labels and the industry combo are UI.
pub fn select_target_corp(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let target = g.target_corp;
    let prev_sel = g.sel_entity;
    if target == g.sel_entity || target == 0 {
        return;
    }
    update_select_last_button(g, prev_sel);
    g.sel_entity = target;
    if target < 1 || 1600 < target {
        return; // EntName(target): error 9 -> handler
    }
    g.sel_corp_name = g.ent_name[target].clone();
    // (UI) labels 169 (name) and 202 (symbol)
    show_next_earnings_date(g);
    if 10 < target {
        g.valuation_mode = 0;
        calc_stock_price(g, ui, rng, target, time_secs);
        // (UI) label 176: StockPriceStr
    }
    calc_entity_financials(g, ui, rng, target);
    // (UI) credit-line text (UnusedCreditLine), not shown
    if 0 < g.sel_entity {
        update_corp_control(g, ui, rng, target);
        if g.cur_player == g.voting_controller {
            g.active_entity = g.sel_entity;
            g.active_entity_name = g.sel_corp_name.clone();
        }
        update_controller_label(g);
    }
    if 10 < g.sel_entity {
        g.sel_industry = cast::<i32, _>(g.industry_num[g.sel_entity]);
        // (UI) SelectIndustryComboItem (0x46df33), EnableMainControls (0x424754)
        if !(g.game_end_flag == "Y") && g.auto_add_stream_list == 1 {
            toggle_streaming_quote(g, ui, rng, 1, 0);
        }
    }
}

/// Port of UpdateSelectLastButton (0x473686), state effects only.
/// For a company id (> 10): `LastSelEntity` = it if it is still active, else 0 (the "Select Last"
/// button caption is UI).
pub fn update_select_last_button(g: &mut Globals, entity_id: i32) {
    if 10 < entity_id {
        g.last_sel_entity = entity_id;
        if 1600 < entity_id {
            return; // EntityStatus(): error 9 (no handler here; not ported)
        }
        if g.entity_status[g.last_sel_entity] == 1 {
            // (UI) CONTROL ENABLE 173; caption "Se&lect " + EntSymbol
        } else {
            g.last_sel_entity = 0;
        }
    }
}

/// Port of ShowNextEarningsDate (0x4a070a), state effects only: the next-earnings label of the selected
/// company (UI); `GetNextEarningsDateStr` clears `TmpExt1` (and may fix `CurQuarter`).
pub fn show_next_earnings_date(g: &mut Globals) {
    if 11 <= g.sel_entity {
        let sel = g.sel_entity;
        get_next_earnings_date_str(g, sel);
    }
    // (UI) label 120
}

/// Port of RefreshSelectionLabels (0x424da3), state effects only.
/// Fills an empty `SelCorpName` for a selected company, then `UpdateControllerLabel` and
/// `ShowNextEarningsDate`; the name/symbol/price labels are UI.
pub fn refresh_selection_labels(g: &mut Globals) {
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    if g.cur_player == g.sel_entity {
        // (UI) labels 169/202/176, EnableMainControls
    } else if g.sel_corp_name.is_empty() && 10 < g.sel_entity {
        g.sel_corp_name = g.ent_name[g.sel_entity].clone();
        // (UI) labels 169/202
    }
    update_controller_label(g);
    show_next_earnings_date(g);
}

/// Port of UpdateControllerLabel (0x46d8d5), state effects only.
/// An unset `SelEntity` becomes `CurPlayer`; unless the selection is the active entity,
/// `VotingController` = `CorpController(SelEntity)`; an unset `ActiveEntity` is reset to the player (with
/// `CurPlayerName`, `SelEntity`, `SelCorpName`). The "controlled by" / "acting as" labels are UI.
pub fn update_controller_label(g: &mut Globals) {
    if g.sel_entity < 1 {
        g.sel_entity = g.cur_player;
    }
    if g.active_entity == g.sel_entity {
        // (UI) label "Me" / player name / "N/A"
    } else {
        // CorpController(0) would be error 9 (no handler in this routine): not ported
        if 1 <= g.sel_entity && g.sel_entity <= 1600 {
            g.voting_controller = g.corp_controller[g.sel_entity];
        }
        // (UI) label: controller's name / symbol / "PLAYER n" / "No One"
    }
    // (UI) label 181
    if g.active_entity < 1 {
        g.active_entity = g.cur_player;
        g.cur_player_name = g.ent_name[g.cur_player].clone();
        g.sel_entity = g.cur_player;
        g.sel_corp_name = g.cur_player_name.clone();
    }
    // (UI) label 190: the active entity
}

/// Port of ToggleStreamingQuote (0x5dda89), state effects only.
/// Adds the selected company (`use_active = 1`: the active company, if one is active; manual use falls
/// back to `LastSelEntity`) to the current player's streaming-quote list `WatchList(1..15, CurPlayer)`,
/// or removes it if it is listed. `auto_flag = 1` (auto-add on selection) only adds, and only when
/// `AutoAddStreamList` is on and the company is not `LastAutoAddedQuote` / `PrevPickedEntity`.
/// A removal records the company as `LastAutoAddedQuote` (the previous one as `PrevPickedEntity`).
/// The list is then rebuilt ([`rebuild_streaming_quotes_panel`] mode 2).
/// When the list is full a manual add asks which quote to drop ([`crate::logic::trading::pick_from_streaming_list`]).
pub fn toggle_streaming_quote(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, auto_flag: i32, use_active: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    if g.cur_player == 0 || g.game_in_progress == 0 || g.game_end_flag == "Y" {
        return;
    }
    let is_auto = auto_flag;
    if is_auto == 0 {
        // (UI) CloseAllDialogs(1) (0x493e2b)
    }
    let mut corp_id = g.sel_entity;
    if use_active == 1 && 10 < g.active_entity {
        corp_id = g.active_entity;
    }
    if is_auto == 1
        && (g.auto_add_stream_list == 0 || g.last_auto_added_quote == corp_id || g.prev_picked_entity == corp_id)
    {
        return;
    }
    if corp_id < 11 && is_auto == 0 && 10 < g.last_sel_entity {
        corp_id = g.last_sel_entity;
    }
    if corp_id < 11 && is_auto == 0 {
        let label_text = s(__content.string(6617))
            + __content.string(6618)
            + "\r"
            + "\r"
            + __content.string(6619)
            + __content.string(6620);
        let title = g.app_title.clone();
        ui.msgbox(&label_text, 8208, &title);
        return;
    }
    if corp_id < 1 || 1600 < corp_id || g.cur_player < 0 || 5 < g.cur_player {
        return; // EntityStatus / WatchList: error 9 -> handler
    }
    if g.entity_status[corp_id] != 1 {
        return;
    }
    let player = g.cur_player;
    let mut slot = 0;
    let mut retry = 0;
    loop {
        let state;
        loop {
            // 0x5ddca2
            let mut st = 1;
            let mut slot_idx = 1;
            while slot_idx <= 15 {
                let listed_id = g.watch_list[(slot_idx, player)];
                if slot == 0 && listed_id == 0 {
                    slot = slot_idx;
                }
                if corp_id == listed_id {
                    st = -1;
                    slot = slot_idx;
                    slot_idx = 15;
                }
                slot_idx = slot_idx + 1;
            }
            if st == 1 && slot == 0 {
                st = 0;
            }
            if is_auto == 1 && st < 1 {
                return;
            }
            if st == 0 {
                let label_text = s(__content.string(6621))
                    + __content.string(6622)
                    + &g.ent_name[corp_id]
                    + "."
                    + "\r"
                    + "\r"
                    + __content.string(6623);
                let answer = ui.msgbox(&label_text, 8209, &s(__content.string(6624)));
                if answer == 2 {
                    return;
                }
                corp_id = crate::logic::trading::pick_from_streaming_list(g, ui, rng, 0);
                if corp_id < 10 {
                    return;
                }
                if is_auto == 0 && g.auto_add_stream_list == 1 {
                    if 10 < g.last_auto_added_quote {
                        g.prev_picked_entity = g.last_auto_added_quote;
                    }
                    g.last_auto_added_quote = corp_id;
                }
            }
            if st == 0 && 10 < corp_id {
                retry = 1;
            }
            if !(st == 0 && 10 < corp_id) {
                state = st;
                break;
            }
        }
        if state == 1 {
            g.valuation_mode = 0;
            calc_stock_price(g, ui, rng, corp_id, 0); // mode 0 does not use TIME$
            g.watch_list[(slot, player)] = corp_id;
            g.stream_prev_price_str[(slot, player)] = g.stock_price_str.clone();
            g.stream_cur_price[(slot, player)] = g.stock_price_str.clone();
            g.stream_price_dir[(slot, player)] = 0;
            // (UI) CONTROL KILL / ADD LABEL slot + 800 (name), slot + 900 (price)
        } else if state == -1 {
            // GOSUB ToggleStreamingQuote_RemoveSlot (0x5de4ea)
            g.watch_list[(slot, player)] = 0;
            g.stream_prev_price_str[(slot, player)] = PbStr::new();
            g.stream_cur_price[(slot, player)] = PbStr::new();
            g.stream_price_dir[(slot, player)] = 0;
            // (UI) CONTROL KILL slot + 800, slot + 900
            g.prev_picked_entity = g.last_auto_added_quote;
            g.last_auto_added_quote = corp_id;
        } else {
            return;
        }
        rebuild_streaming_quotes_panel(g, ui, 2);
        if retry != 1 {
            return;
        }
        corp_id = g.sel_entity;
        retry = 0;
        slot = 0;
    }
}

/// Port of RebuildStreamingQuotesPanel (0x5dcee5), state effects only.
/// Mode 3 (bankruptcy): clears the current player's whole quote list and shows "… has run out of
/// [runtime resource wording] ". Otherwise (unless `mode < 2` and the list is already shown for this
/// player, `HoldingsListPlr`) sets `HoldingsListPlr = CurPlayer`, clears slots without an active company,
/// closes the gaps (quirk: the compaction runs once per empty slot, so consecutive gaps can remain) and
/// sets `WatchList(0, player)` to the last used slot. The quote labels are UI. Nothing happens for a
/// computer player, or for a player who is out (`mode < 3`).
pub fn rebuild_streaming_quotes_panel(g: &mut Globals, ui: &mut dyn Ui, mode: i32) {
    let __content = crate::content::current();
    // 0x5dcf11..0x5dcf1a: pause even when no quote slots need rebuilding.
    crate::logic::turn::pause_ticker(g, &ui.local_time(), 0.0f32);
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mode_val = mode;
    if g.cur_player < 0 || 5 < g.cur_player {
        return; // IsComputerPlayer / PlayerName: error 9 -> handler
    }
    if 0 < g.cur_player && g.is_computer_player[g.cur_player] == 1 {
        return;
    }
    if g.cur_player == 0 {
        return; // PlayerName(0): error 9 -> handler
    }
    if g.player_name[g.cur_player] == "...OUT..." && mode_val < 3 {
        return;
    }
    let player_id = g.cur_player;
    let clear_slot = |g: &mut Globals, slot: i32| {
        // GOSUB RebuildStreamingQuotes_ClearSlot (0x5dd942)
        g.watch_list[(slot, player_id)] = 0;
        g.stream_prev_price_str[(slot, player_id)] = PbStr::new();
        g.stream_cur_price[(slot, player_id)] = PbStr::new();
        g.stream_price_dir[(slot, player_id)] = 0;
    };
    if mode_val == 3 {
        for slot in 1..=15 {
            // (UI) GOSUB RebuildStreamingQuotes_KillSlotCtrls
            clear_slot(g, slot);
        }
        let text = g.ent_name[g.cur_player].clone() + __content.string(6625);
        ui.msgbox(&text, 8208, &s(__content.string(6626)));
        // (UI) label "YOU ARE BANKRUPT!"
        return;
    }
    if 0 < g.cur_player && g.holdings_list_plr == g.cur_player && mode_val < 2 {
        return;
    }
    g.holdings_list_plr = g.cur_player;
    for slot in 1..=15 {
        let corp_id = g.watch_list[(slot, player_id)];
        if corp_id < 11 {
            clear_slot(g, slot);
        } else {
            if 1600 < corp_id {
                return; // EntityStatus(corpId): error 9 -> handler
            }
            if g.entity_status[corp_id] != 1 {
                clear_slot(g, slot);
            }
        }
    }
    for slot in 1..=15 {
        if g.watch_list[(slot, player_id)] == 0 {
            // GOSUB RebuildStreamingQuotes_CompactList (0x5dd6f8)
            for shift_idx in slot..=14 {
                g.watch_list[(shift_idx, player_id)] = g.watch_list[(shift_idx + 1, player_id)];
                g.watch_list[(shift_idx + 1, player_id)] = 0;
                g.stream_prev_price_str[(shift_idx, player_id)] = g.stream_prev_price_str[(shift_idx + 1, player_id)].clone();
                g.stream_prev_price_str[(shift_idx + 1, player_id)] = PbStr::new();
                g.stream_cur_price[(shift_idx, player_id)] = g.stream_cur_price[(shift_idx + 1, player_id)].clone();
                g.stream_cur_price[(shift_idx + 1, player_id)] = PbStr::new();
                g.stream_price_dir[(shift_idx, player_id)] = g.stream_price_dir[(shift_idx + 1, player_id)];
                g.stream_price_dir[(shift_idx + 1, player_id)] = 0;
            }
        }
        if 10 < g.watch_list[(slot, player_id)] {
            g.watch_list[(0, player_id)] = slot;
        }
    }
    // (UI) slots 1..15: CONTROL KILL / ADD LABEL (name, symbol) and price labels
}
