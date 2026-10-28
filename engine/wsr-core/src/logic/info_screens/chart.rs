//! Price charts: `ShowPriceChart` (0x6141b1) with `DrawPriceChart` (0x615e52), `ChartAxisMaxForValue`
//! (0x617bc3), `SynthesizePriceHistory` (0x619527) and the chart dialog's callbacks.
//!
//! The original draws the five-year monthly chart with DDT controls: bars and ticks are `CONTROL ADD
//! LINE` rectangles, axis/year labels `CONTROL ADD LABEL`s, in pixels of the chart dialog. The port
//! computes exactly the same controls (ids, texts, positions, styles) and hands them, with the
//! underlying monthly series and scale, to [`Ui::chart`] as a [`ChartSpec`]: a host can replay the
//! controls as they are or draw its own chart from the series.
//!
//! The per-month arithmetic runs on the x87 80-bit model ([`Ext80`]) like the original (local EXT
//! arrays), so pixel roundings and the 15-digit `STR$` axis labels match bit for bit.

#![allow(clippy::collapsible_if, clippy::collapsible_else_if, clippy::neg_cmp_op_on_partial_ord)]
use crate::pb::real::{Ext, cast};


use std::cmp::Ordering;

use crate::logic::economy::close_all_dialogs;
use crate::logic::finance::calc_entity_financials;
use crate::logic::holdings::find_holding_slot;
use crate::logic::market::{rebase_stock_index, update_market_display};
use crate::logic::newgame::LocalTime;
use crate::logic::shell::{Env, menu::do_transaction_menu_action};
use crate::logic::trade_screens::commod_crypto::commod_desk::show_commodity_desk;
use crate::logic::trade_screens::ir_swaps::show_ir_swap_dialog;
use crate::logic::report::show_quick_message;
use crate::logic::turn::ticker_sim_steps;
use crate::logic::valuation::{calc_stock_price, update_price_history_tick};
use crate::pb::ext80::Ext80;
use crate::pb::numeric::format;
use crate::pb::numeric::{str_num, str_num_ext};
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{CaseMode, ltrim, mcase, replace, right, rtrim, mid, space};
use crate::pb::numeric::to_long;
use crate::state::Globals;
use crate::ui::Ui;

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

/// SINGLE pool constant (`fld/fmul dword [0x75e…]`).
fn sf(x: f32) -> Ext {
    cast::<Ext, _>(x)
}

fn e80(x: Ext) -> Ext80 {
    Ext80::from_ext(x)
}

fn ei(x: i64) -> Ext80 {
    Ext80::from_i64(x)
}

fn lt(a: Ext80, b: Ext80) -> bool {
    a.cmp_val(b) == Ordering::Less
}

fn trim(x: &[u8]) -> PbStr {
    rtrim(ltrim(x).as_bytes())
}

/// What a chart control is (`CONTROL ADD LINE / LABEL / BUTTON`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChartControlKind {
    /// A filled or framed rectangle (`CONTROL ADD LINE`; `style` 4 = black filled, 6 = the plot frame,
    /// 7 = black frame).
    Line,
    /// A text label (`style` 1 = centered).
    Label,
    /// A push button (1 = CLOSE, 522 = TRADE/BUY).
    Button,
}

/// One control of the chart dialog, in the original's pixel coordinates (dialog client area).
#[derive(Clone, Debug, PartialEq)]
pub struct ChartControl {
    /// Control id: 520 year label, 521 last-value label, 524 month bar (high..low), 525 close tick /
    /// axis tick, 526 plot frame, 528 quarter tick, 530 year-end tick, 531..537 axis labels (537 at the
    /// top), 1 CLOSE, 522 TRADE.
    pub id: i32,
    pub kind: ChartControlKind,
    pub text: PbStr,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub style: i32,
}

/// Everything the chart dialog shows (built by [`show_price_chart_spec`] / [`draw_price_chart`]).
#[derive(Clone, Debug, PartialEq)]
pub struct ChartSpec {
    /// Calendar month at the right edge, for hosts drawing the series themselves.
    pub end_year: i32,
    pub end_month: i32,
    /// `ChartEntity`: 0 stock index, 1..5 a player's net worth, 6..10 crude/gold/silver/wheat/corn,
    /// 11..1600 a stock, 1601..1604 prime rate / long bond / short bond / GDP growth, 1605/1606 crypto.
    pub entity: i32,
    /// `ChartSize`: 1 large, 2 small.
    pub size: i32,
    /// Dialog caption.
    pub title: PbStr,
    /// Dialog size (pixels).
    pub width: i32,
    pub height: i32,
    /// Every control in creation order.
    pub controls: Vec<ChartControl>,
    /// The 60 plotted months, oldest first, as plotted (per share for stocks, shifted by the GDP offset,
    /// divided by 1000^`thousands_shift`); a month with high = low = close = 0 has no bar.
    pub highs: Vec<Ext>,
    pub lows: Vec<Ext>,
    pub closes: Vec<Ext>,
    /// Value at the top of the plot (bar height = value / `scale_max` x plot height).
    pub scale_max: i64,
    /// How often the values were divided by 1000 to fit the scale.
    pub thousands_shift: i64,
    /// GDP chart with negative growth: the lowest axis value (the series are shifted up by
    /// `gdp_offset`); else 0.
    pub min_low: Ext,
    pub gdp_offset: Ext,
    /// Caption of the trade button (`&TRADE` / `&BUY`) if it is shown.
    pub trade_label: Option<PbStr>,
}

impl ChartSpec {
    fn add(&mut self, id: i32, kind: ChartControlKind, text: PbStr, x: i32, y: i32, w: i32, h: i32, style: i32) {
        self.controls.push(ChartControl { id, kind, text, x, y, w, h, style });
    }
}

/// Port of ShowPriceChart (0x6141b1).
/// Shows the monthly chart of `ChartEntity` (see [`show_price_chart_spec`]) through [`Ui::chart`]; when
/// the user presses the trade button, runs [`chart_trade_click`], else [`chart_close_click`].
pub fn show_price_chart(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime) {
    let Some(spec) = show_price_chart_spec(g, ui, rng, now) else {
        return;
    };
    // DIALOG SHOW MODAL hChartDlg CALL DlgProcChart
    if ui.chart(&spec) && spec.trade_label.is_some() {
        chart_trade_click(g, ui, rng, now);
    } else {
        chart_close_click(g);
    }
}

/// [`show_price_chart`] up to `DIALOG SHOW MODAL`: everything the original computes before the chart
/// dialog waits for the user. `None` when no chart is shown.
///
/// `ChartSize` 0 becomes 2 (small). Without a game a quick message says so. Else the market display and
/// the series' current month are updated (`UpdateMarketDisplay`, `UpdatePriceHistoryTick`); the stock
/// index is rebased first when it is at or above 90,000 or below 1,000 (`RebaseStockIndex`). A stock
/// whose price (highest monthly high / shares) was above 10,000 gets the "Stock Split Required" box
/// instead of a chart (an ETF, industry 71, then splits 2-for-1: `SharesOutstanding x 2`). The
/// last-value text goes to `StockPriceStr` (players: `CalcEntityFinancials` → `NetWorth`; GDP growth is
/// clamped to -23..9.8 first). The trade button needs: game not over, not GDP / a player / crypto, for a
/// stock a holding of the acting entity and the stock selected (or last selected by the acting player),
/// not the acting company itself; for rates and crypto credit rating >= 5; no commodity desk for banks
/// and insurers, no index trading for banks; and `TurnActionCount < 5` — else (all other conditions met)
/// one ticker step runs (`TickerSimSteps(1)`). For stocks the price is recomputed (`ValuationMode = 0`,
/// `CalcStockPrice`) for the last-price label.
pub fn show_price_chart_spec(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime) -> Option<ChartSpec> {
    let __content = crate::content::current();
    if g.chart_size == 0 {
        g.chart_size = 2;
    }
    if g.game_in_progress == 0 {
        show_quick_message(g, ui, &mut s(__content.string(5702)));
        return None;
    }
    // (UI) DIALOG END hChartDlg / DlgWhoOwnsWhat / hCheatDlg
    update_market_display(g, ui, rng, now.time_secs());
    let series = g.chart_entity;
    update_price_history_tick(g, ui, rng, series);
    let e = g.chart_entity;
    let mut max_high: Ext = Ext::from_f64(0.0);
    for month in 1..=60 {
        let v = g.price_hist_high[(month, e)];
        if Ext::from_f64(0.0) < v && max_high < v {
            max_high = v;
        }
    }
    if e < 11 || 1600 < e {
        if e == 0 && !(g.stock_index < Ext::from_f64(90000.0)) {
            rebase_stock_index(g, ui);
        } else if e == 0 && g.stock_index < Ext::from_f64(1000.0) {
            rebase_stock_index(g, ui);
        }
    } else {
        if Ext::from_f64(0.0) < g.shares_outstanding[e] {
            max_high = max_high / g.shares_outstanding[e];
        }
        if Ext::from_f64(10000.0) < max_high {
            let mut high_price = e80(max_high).to_i64();
            if high_price == 10000 {
                high_price = 10001;
            }
            let mut t = s(__content.string(5703))
                + str_num(cast::<Ext, _>(high_price), 15).as_str()
                + __content.string(5704)
                + __content.string(5705)
                + __content.string(5706)
                + "\r"
                + "\r";
            if g.game_end_flag != "Y" {
                t = t + "Until " + &g.ent_name[e] + __content.string(5707) + __content.string(5708);
            }
            ui.msgbox(&t, 8192, &s(__content.string(5709)));
            if g.industry_num[e] == 71 {
                g.shares_outstanding[e] = g.shares_outstanding[e] * Ext::from_f64(2.0);
                show_quick_message(g, ui, &mut (g.ent_name[e].clone() + __content.string(5710)));
            }
            return None;
        }
    }

    let small = g.chart_size == 2;
    let (mut width, mut height) = (672, 400);
    if small {
        (width, height) = (366, 212);
    }
    let mut title;
    let mut last_value: Ext = Ext::from_f64(0.0);
    if e == 0 {
        title = s(__content.string(5711));
        if small {
            title = mcase(title.as_bytes(), CaseMode::Ansi);
        }
        last_value = g.stock_index;
    } else if e >= 6 {
        if e >= 11 {
            if e < 1601 || 1604 < e {
                if e == 1605 || e == 1606 {
                    let mut series_text = PbStr::new();
                    if e == 1605 {
                        series_text = s("BITCOIN");
                        last_value = cast::<Ext, _>(g.bitcoin_price);
                    }
                    if e == 1606 {
                        series_text = s("ETHEREUM");
                        last_value = cast::<Ext, _>(g.ethereum_price);
                    }
                    if small {
                        series_text = mcase(series_text.as_bytes(), CaseMode::Ansi);
                    }
                    title = s(__content.string(5712)) + &series_text + __content.string(5713);
                    if Ext::from_f64(85000.0) <= last_value {
                        title = replace(title.as_bytes(), b"In U.S.", __content.bytes(5714));
                    }
                    if small {
                        title = mcase(title.as_bytes(), CaseMode::Ansi);
                    }
                } else {
                    // ids 1607..1610 would read EntName out of its bounds (never charted)
                    let (name, sym) = if e <= 1600 { (g.ent_name[e].clone(), g.ent_symbol[e].clone()) } else { (PbStr::new(), PbStr::new()) };
                    title = s(__content.string(5715)) + &name + " (" + &sym + ")";
                    if small {
                        title = name + " (" + &sym + __content.string(5716);
                    }
                }
            } else {
                let mut series_text = PbStr::new();
                if e == 1601 {
                    series_text = s(__content.string(5717));
                    last_value = cast::<Ext, _>(g.prime_rate);
                }
                if e == 1602 {
                    series_text = s(__content.string(5718));
                    last_value = cast::<Ext, _>(g.long_bond_yield);
                }
                if e == 1603 {
                    series_text = s(__content.string(5719));
                    last_value = cast::<Ext, _>(g.short_bond_yield);
                }
                if e == 1604 {
                    series_text = s(__content.string(5720));
                    if !(sf(9.8f32) >= cast::<Ext, _>(g.gdp_growth)) {
                        g.gdp_growth = cast::<f32, _>(Ext::from_f32(9.8f32));
                    }
                    if -Ext::from_f64(23.0) > cast::<Ext, _>(g.gdp_growth) {
                        g.gdp_growth = cast::<f32, _>(-Ext::from_f32(23.0f32));
                    }
                    last_value = cast::<Ext, _>(g.gdp_growth);
                }
                title = s(__content.string(5721)) + &series_text;
                if small {
                    title = mcase(title.as_bytes(), CaseMode::Ansi);
                }
            }
        } else {
            let mut series_text = match e {
                6 => {
                    last_value = cast::<Ext, _>(g.oil_price) * Ext::from_f64(100.0);
                    s(__content.string(5722))
                }
                7 => {
                    last_value = cast::<Ext, _>(g.gold_price);
                    s("GOLD")
                }
                8 => {
                    last_value = cast::<Ext, _>(g.silver_price);
                    s("SILVER")
                }
                9 => {
                    last_value = cast::<Ext, _>(g.wheat_price);
                    s("WHEAT")
                }
                _ => {
                    last_value = cast::<Ext, _>(g.corn_price);
                    s("CORN")
                }
            };
            if small {
                series_text = mcase(series_text.as_bytes(), CaseMode::Ansi);
            }
            title = s(__content.string(5723)) + &series_text + __content.string(5713);
            if small {
                title = mcase(title.as_bytes(), CaseMode::Ansi);
            }
        }
    } else {
        title = s(__content.string(5724)) + &g.player_name[e];
        if small {
            title = s(__content.string(5725)) + &g.player_name[e];
        }
        calc_entity_financials(g, ui, rng, e);
        last_value = g.net_worth;
    }
    if Ext::from_f64(0.0) < last_value {
        g.stock_price_str = trim(&format(last_value, b"############.00"));
        if Ext::from_f64(1000.0) <= last_value {
            g.stock_price_str = trim(&format(last_value, b"* ##,.00"));
        }
        if Ext::from_f64(100000.0) <= last_value || 1604 < e {
            g.stock_price_str = trim(&format(last_value, b"* ##,"));
        }
        if 1600 < e && e < 1605 {
            g.stock_price_str = g.stock_price_str.clone() + "%";
        }
    }
    if e == 1604 {
        g.stock_price_str = trim(&format(last_value, b"##.0")) + "%";
    }
    if g.chart_size == 1 && g.cap_hist_close[(49, e)] != Ext::from_f64(0.0) {
        title = s(__content.string(5726)) + &title;
    }

    // DIALOG NEW hParent, title, 1, 1, width, height; AdjustDialogFrameSize (UI)
    let mut spec = ChartSpec {
        end_year: g.cur_year,
        end_month: 1,
        entity: e,
        size: g.chart_size,
        title,
        width,
        height,
        controls: Vec::new(),
        highs: Vec::new(),
        lows: Vec::new(),
        closes: Vec::new(),
        scale_max: 0,
        thousands_shift: 0,
        min_low: Ext::from_f64(0.0),
        gdp_offset: Ext::from_f64(0.0),
        trade_label: None,
    };
    let (fx, fy, fw, fh) = if small { (13, 10, 320, 164) } else { (25, 20, 615, 327) };
    spec.add(526, ChartControlKind::Line, PbStr::new(), fx, fy, fw, fh, 6);
    draw_price_chart(g, rng, &mut spec);

    let (bx, by, bh, trade_x) = if small { (156, 193, 12, 56) } else { (296, 372, 15, 124) };
    spec.add(1, ChartControlKind::Button, s("CL&OSE"), bx, by, 53, bh, 0x5001_0000);
    let mut can_trade = 1;
    let mut trade_label = s("&TRADE");
    let e = g.chart_entity;
    let act = g.active_entity;
    if 10 < e && e < 1601 {
        trade_label = s("&BUY");
        let hold_slot = find_holding_slot(g, act, e);
        if hold_slot == 0 {
            can_trade = 0;
        }
        if can_trade == 1 && g.last_sel_entity == e && g.cur_player == act {
            can_trade = 1;
        } else if g.sel_entity != e {
            can_trade = 0;
        }
        if e == act {
            can_trade = 0;
        }
    }
    if e < 1601 || 1603 < e {
        if (e == 1605 || e == 1606) && g.credit_rating_num[act] < 5 {
            can_trade = 0;
        }
    } else if g.credit_rating_num[act] < 5 {
        can_trade = 0;
    }
    let industry = |g: &Globals| cast::<i32, _>(g.industry_num[act]);
    if g.game_end_flag != "Y"
        && e != 1604
        && (e < 1 || 5 < e)
        && can_trade != 0
        && (e < 6 || 10 < e || act < 11 || 2 < industry(g))
        && (e != 0 || act < 11 || industry(g) != 1)
        && e != 1605
        && e != 1606
    {
        if g.turn_action_count < 5 {
            spec.add(522, ChartControlKind::Button, trade_label.clone(), trade_x, by, 53, bh, 0x5001_0000);
            spec.trade_label = Some(trade_label);
        } else {
            ticker_sim_steps(g, ui, rng, 1, now);
        }
    }
    let e = g.chart_entity;
    let mut series_text;
    if e < 11 || 1600 < e {
        series_text = s("Last: ") + &g.stock_price_str;
    } else {
        g.valuation_mode = 0;
        calc_stock_price(g, ui, rng, e, now.time_secs());
        series_text = g.ent_symbol[e].clone() + ": " + &g.stock_price_str;
        if g.chart_size == 1 {
            series_text = s(__content.string(5727)) + &series_text;
        }
    }
    let (lx, ly, lw) = if g.chart_size == 1 { (400, 375, 250) } else { (235, 195, 120) };
    spec.add(521, ChartControlKind::Label, series_text, lx, ly, lw, 12, 0);
    // (UI) fonts, CONTROL SET FOCUS
    Some(spec)
}

/// Port of DlgProcChart (0x61548f): the chart dialog's callback only colours the labels (ids 520, 521,
/// 531..537) and sets the button tooltips; nothing to port (a host draws [`ChartSpec`] its own way).
pub fn dlg_proc_chart() {}

/// Port of Chart_CloseClick (0x615bbd): ends the chart dialog (and, for a small chart of a company
/// other than the selected one, the report dialog it was opened from). Window plumbing only.
pub fn chart_close_click(_g: &mut Globals) {}

/// Port of Chart_TradeClick (0x615c48).
/// Closes all modeless dialogs (`CloseAllDialogs(1)`) and, unless the game is over, opens the trading
/// screen for the charted series: the commodity desk for commodities and the stock index
/// (`TmpExt1 = -999`), the interest-rate swap dialog for prime/long/short rates (`SwapRateType` "P",
/// "L", "S"), the buy-stock action for a stock (`ActionMenuMode = 4`, `SkipMarginPrompt = "Y"` when the
/// acting entity is the player); then `ChartEntity = 0`.
pub fn chart_trade_click(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime) {
    close_all_dialogs(g, ui, rng, 1);
    if g.game_end_flag == "Y" {
        return;
    }
    // (UI) DIALOG SHOW STATE hDlgMain, 0
    let e = cast::<Ext, _>(g.chart_entity);
    if (Ext::from_f64(6.0) <= e && e <= Ext::from_f64(10.0)) || e == Ext::from_f64(0.0) {
        g.tmp_ext1 = -Ext::from_f64(999.0);
        // (UI) DisableMainControls
        show_commodity_desk(g, ui, rng, 0);
        // (UI) EnableMainControls
    } else if e < Ext::from_f64(11.0) || Ext::from_f64(1600.0) < e {
        if Ext::from_f64(1601.0) <= e && e <= Ext::from_f64(1603.0) {
            g.swap_rate_type = s("P");
            if g.chart_entity == 1602 {
                g.swap_rate_type = s("L");
            }
            if g.chart_entity == 1603 {
                g.swap_rate_type = s("S");
            }
            // (UI) DisableMainControls
            show_ir_swap_dialog(g, ui, rng, 0);
            // (UI) EnableMainControls
        }
    } else {
        g.action_menu_mode = 4;
        if g.cur_player == g.active_entity {
            g.skip_margin_prompt = s("Y");
        }
        do_transaction_menu_action(g, ui, rng, &Env::new(now), 1);
    }
    // (UI) DIALOG SHOW STATE hDlgMain, 5
    g.chart_entity = 0;
}

/// Port of DrawPriceChart (0x615e52).
/// Adds the chart body to `spec`: the year label (520) under the plot with the last five years placed
/// by the current month (`CurQuarter`, `SimCoCount` progress), the quarter ticks (528, year ends 530),
/// the scale (`ChartAxisMaxForValue` of the highest high; for GDP with negative growth a fixed range
/// below zero and all values shifted up), the seven axis ticks and labels (525, 537..531) and per month
/// a high-low bar (524) with a close tick (525).
///
/// Stock values are per share (`SharesOutstanding` <= 0 is set to 1 first); values above 75,000 are
/// divided by 1000 until they fit (not for the index). In the first quarter of the first year a series
/// without history gets two synthetic months: stocks with `PriceHistHigh(0) = -1` through
/// [`synthesize_price_history`], other series (not players, not GDP) from the current value
/// (`RND`: x(1.03 - 0.07 RND), month 2 only if empty, month 3 x0.96 or x1.03, highs x(1.02 + 0.04 RND),
/// lows x(0.93 + 0.04 RND), `CapHistClose(0) = 3`).
pub fn draw_price_chart(g: &mut Globals, rng: &mut Rng, spec: &mut ChartSpec) {
    use ChartControlKind::{Label, Line};
    let small = g.chart_size == 2;
    let e = g.chart_entity;
    let base_month = (g.cur_quarter - 1) * 3;
    let mut month_num = base_month + 1;
    let mut x_offset: i32 = 0;
    let progress = cast::<Ext, _>(g.sim_co_count);
    if progress <= Ext::from_f64(1000.0) {
        if Ext::from_f64(500.0) < progress {
            month_num = base_month + 2;
            x_offset = -10;
        }
    } else {
        month_num = base_month + 3;
        x_offset = -20;
    }
    if small {
        x_offset = to_long(cast::<Ext, _>((cast::<f32, _>(x_offset)) * Ext::from_f32(0.5f32)));
    }
    spec.end_year = g.cur_year;
    spec.end_month = month_num;
    let mut gap_text = space(18);
    let mut pad_text = match month_num {
        1 => space(16),
        2 => space(14),
        3 => space(12),
        4 => space(10),
        5 => space(8),
        6 => space(7),
        7 => space(5),
        8 => space(3),
        9 => space(1),
        10 => space(10),
        11 => space(8),
        12 => space(6),
        _ => PbStr::new(),
    };
    if small {
        gap_text = space(7);
        let mut pad_len = 7 - month_num;
        if 6 < month_num {
            pad_len = 13 - month_num;
        }
        pad_text = space(pad_len);
        if month_num == 1 {
            pad_text = space(6);
        }
    }
    let mut year_label = s("YEAR") + &pad_text + "9005" + &gap_text + " 9006" + &gap_text + " 9007" + &gap_text + "  9008" + &gap_text + "  9009";
    if small {
        year_label = replace(year_label.as_bytes(), b" 9006", b"9006");
        year_label = replace(year_label.as_bytes(), b" 9008 ", b"9008");
        if month_num == 12 {
            year_label = replace(year_label.as_bytes(), b" 9007 ", b"  9007");
        }
    }
    let mut placeholder_year = 9009;
    let mut year = g.cur_year;
    for _ in 0..5 {
        let find = trim(str_num(cast::<Ext, _>(placeholder_year), 15).as_bytes());
        let with = trim(str_num(cast::<Ext, _>(year), 15).as_bytes());
        year_label = replace(year_label.as_bytes(), find.as_bytes(), with.as_bytes());
        placeholder_year -= 1;
        year -= 1;
    }
    year_label = rtrim(year_label.as_bytes()) + " ";
    let step = year_label.len() - 3;
    let (lx, ly, lw) = if g.chart_size == 1 { (24, 350, 640) } else { (13, 178, 350) };
    if right(year_label.as_bytes(), 1) != " " {
        if mid(year_label.as_bytes(), step, 1) == " " {
            year_label = mid(year_label.as_bytes(), 1, step);
        }
    }
    spec.add(520, Label, year_label, lx, ly, lw, 12, 0);

    // quarter ticks
    let mut tick_x = x_offset + if small { 30 } else { 50 };
    let mut quarter = g.cur_quarter;
    let right_edge = if small { 326 } else { 640 };
    let tick_y = if small { 171 } else { 343 };
    for _ in 0..21 {
        if 20 < tick_x && tick_x < right_edge {
            if quarter == 4 {
                spec.add(530, Line, PbStr::new(), tick_x, tick_y - 5, 2, 10, 4);
            } else {
                spec.add(528, Line, PbStr::new(), tick_x, tick_y, 2, 4, 4);
            }
        }
        tick_x += if g.chart_size == 1 { 30 } else { 15 };
        quarter += 1;
        if 4 < quarter {
            quarter = 1;
        }
    }

    let price_divisor = if e < 11 || 1600 < e {
        Ext80::ONE
    } else {
        if g.shares_outstanding[e] <= Ext::from_f64(0.0) {
            g.shares_outstanding[e] = Ext::from_f64(1.0);
        }
        e80(g.shares_outstanding[e])
    };

    if g.cur_quarter == 1 && g.start_year == g.cur_year && g.cap_hist_close[(3, e)] == Ext::from_f64(0.0) {
        if e < 11 || 1600 < e {
            let mut synth_flag = 1;
            if e < 6 || e == 1604 {
                synth_flag = 0;
            }
            if e == 0 {
                synth_flag = 1;
            }
            if synth_flag == 1 {
                let mut seed = match e {
                    0 => g.stock_index,
                    6 => Ext::from_f64(100.0) * cast::<Ext, _>(g.oil_price),
                    7 => cast::<Ext, _>(g.gold_price),
                    8 => cast::<Ext, _>(g.silver_price),
                    9 => cast::<Ext, _>(g.wheat_price),
                    10 => cast::<Ext, _>(g.corn_price),
                    1601 => cast::<Ext, _>(g.prime_rate),
                    1602 => cast::<Ext, _>(g.long_bond_yield),
                    1603 => cast::<Ext, _>(g.short_bond_yield),
                    1605 => cast::<Ext, _>(g.bitcoin_price),
                    1606 => cast::<Ext, _>(g.ethereum_price),
                    _ => Ext::from_f64(0.0),
                };
                // seed*1.03 - (RND*0.07)*seed, in 80 bits, stored as EXT
                let r = rng.rnd();
                let a = e80(sf(0.07f32)) * e80(Ext::from(r)) * e80(seed);
                let b = e80(sf(1.03f32)) * e80(seed);
                let mut seed80 = b - a;
                seed = Ext::from(seed80.into_ext());
                if g.cap_hist_close[(2, e)] == Ext::from_f64(0.0) {
                    g.cap_hist_close[(2, e)] = seed;
                    let r = rng.rnd();
                    g.price_hist_high[(2, e)] = Ext::from(((e80(sf(0.04f32)) * e80(Ext::from(r)) + e80(sf(1.02f32))) * seed80).into_ext());
                    let r = rng.rnd();
                    g.price_hist_low[(2, e)] = Ext::from(((e80(sf(0.04f32)) * e80(Ext::from(r)) + e80(sf(0.93f32))) * seed80).into_ext());
                }
                let r = rng.rnd();
                if !(Ext::from_f64(2.0) * r > Ext::from_f64(1.0)) {
                    seed80 = seed80 * e80(sf(0.96f32));
                } else {
                    seed80 = e80(sf(1.03f32)) * seed80;
                }
                g.cap_hist_close[(3, e)] = Ext::from(seed80.into_ext());
                let r = rng.rnd();
                g.price_hist_high[(3, e)] = Ext::from(((e80(sf(0.04f32)) * e80(Ext::from(r)) + e80(sf(1.02f32))) * seed80).into_ext());
                let r = rng.rnd();
                g.price_hist_low[(3, e)] = Ext::from(((e80(sf(0.04f32)) * e80(Ext::from(r)) + e80(sf(0.93f32))) * seed80).into_ext());
                g.cap_hist_close[(0, e)] = Ext::from_f64(3.0);
            }
        } else if g.price_hist_high[(0, e)] == -Ext::from_f64(1.0) {
            synthesize_price_history(g, rng, e);
        }
    }

    // the 60 months, oldest first (slot after the current one)
    let start = e80(g.cap_hist_close[(0, e)]).to_i32();
    let months = || {
        let mut pos = start;
        (0..60).map(move |_| {
            pos += 1;
            if 60 < pos {
                pos = 1;
            }
            pos
        })
    };
    let mut highs: Vec<Ext80> = months().map(|p| e80(g.price_hist_high[(p, e)]) / price_divisor).collect();
    let mut max_high = Ext80::ZERO;
    for &h in &highs {
        if e == 1604 {
            if lt(max_high, h) {
                max_high = h;
            }
        } else if lt(Ext80::ZERO, h) && lt(max_high, h) {
            max_high = h;
        }
    }
    let mut thousands_shift: i64 = 0;
    if 0 < e {
        while lt(ei(75000), max_high) {
            thousands_shift = (Ext80::ONE + ei(thousands_shift)).to_i64();
            max_high = e80(sf(0.001f32)) * max_high;
        }
    }
    let mut min_low = Ext80::ZERO;
    let mut lows: Vec<Ext80> = Vec::with_capacity(60);
    for p in months() {
        let l = e80(g.price_hist_low[(p, e)]) / price_divisor;
        lows.push(l);
        if e == 1604 && lt(l, min_low) {
            min_low = l;
        }
    }
    let mut closes: Vec<Ext80> = months().map(|p| e80(g.cap_hist_close[(p, e)]) / price_divisor).collect();

    let mut scale_max: i64 = 0;
    let mut gdp_offset = Ext80::ZERO;
    if lt(min_low, Ext80::ZERO) && e == 1604 {
        let ml = min_low;
        let gt = |a: Ext80, b: i64| lt(ei(b), a);
        if !lt(ml, ei(-20)) {
            if !lt(ml, ei(-15)) {
                if !lt(ml, ei(-10)) {
                    if !lt(ml, ei(-5)) {
                        if lt(ml, Ext80::ZERO) {
                            scale_max = 6 + 9;
                            min_low = ei(-6);
                            if gt(ml, -2) {
                                min_low = ei(-2);
                                scale_max = 2 + 8;
                            }
                        }
                    } else {
                        scale_max = 10 + 15;
                        min_low = ei(-10);
                        if gt(ml, -8) {
                            min_low = ei(-8);
                            scale_max = 8 + 12;
                        }
                    }
                } else {
                    scale_max = 15 + 10;
                    min_low = ei(-15);
                    if gt(ml, -12) {
                        min_low = ei(-12);
                        scale_max = 12 + 8;
                    }
                }
            } else {
                scale_max = 20 + 5;
                min_low = ei(-20);
            }
        } else {
            scale_max = 24 + 6;
            min_low = ei(-24);
        }
        gdp_offset = if min_low.abs().is_zero() { e80(sf(0.1f32)) } else { min_low.abs() + e80(sf(0.1f32)) };
        if small {
            gdp_offset = gdp_offset + e80(sf(0.1f32));
        }
        for i in 0..60 {
            if !highs[i].is_zero() || !lows[i].is_zero() || !closes[i].is_zero() {
                highs[i] = highs[i] + gdp_offset;
                lows[i] = lows[i] + gdp_offset;
                closes[i] = closes[i] + gdp_offset;
            }
        }
    }
    let thousandth = e80(sf(0.001f32));
    for _ in 0..thousands_shift.max(0) {
        for i in 0..60 {
            highs[i] = highs[i] * thousandth;
            lows[i] = lows[i] * thousandth;
            closes[i] = closes[i] * thousandth;
        }
    }
    let gdp_scale = lt(min_low, Ext80::ZERO) && e == 1604;
    if !gdp_scale {
        scale_max = chart_axis_max_for_value(max_high);
        // dead: min_low < 0 only for GDP
        if lt(min_low, Ext80::ZERO) {
            scale_max = (min_low.abs() + ei(scale_max)).to_i64();
            if lt(ei(scale_max), ei(6) + min_low) {
                scale_max = (ei(6) + min_low).to_i64();
            }
        }
    }
    let mut scale_factor: Ext = Ext::from_f64(1.0);
    if scale_max != 100 {
        scale_factor = Ext::from((ei(scale_max) / ei(100)).into_ext());
    }
    let (mut pos, axis_x, step, label_x) = if small { (15, 328, 25, 333) } else { (30, 635, 50, 640) };
    let sfx = e80(scale_factor);
    for i in 1..=7 {
        spec.add(525, Line, PbStr::new(), axis_x, pos, 5, 2, 4);
        let v = sfx * ei(140) - sfx * ei(20) * ei(cast::<i64, _>(i));
        let text = if e != 1604 || !lt(min_low, Ext80::ZERO) {
            trim(str_num_ext(v, 15).as_bytes())
        } else {
            let axis_value = (min_low + e80(Ext::from(v.into_ext()))).into_ext();
            trim(&format(axis_value, b"##.0")) + "%"
        };
        spec.add(538 - i, Label, text, label_x, pos - 2, 30, 9, 1);
        pos += step;
    }

    let (mut pos, step, base_y, plot_h, bar_w, tick_w) = if small { (20, 5, 165, 125, 2, 6) } else { (29, 10, 330, 250, 5, 9) };
    let (base80, plot80, scale80) = (ei(base_y), ei(plot_h), ei(scale_max));
    for i in 0..60 {
        pos += step;
        if !highs[i].is_zero() || !lows[i].is_zero() || !closes[i].is_zero() {
            let mut high_y = (base80 - highs[i] / scale80 * plot80).to_i64();
            let low_y = (base80 - lows[i] / scale80 * plot80).to_i64();
            let mut bar_height = (ei(low_y) - ei(high_y)).to_i64();
            if bar_height < 4 {
                bar_height = 4;
            }
            high_y = (ei(high_y) + Ext80::ONE).to_i64();
            let close_y = (ei((base80 - closes[i] / scale80 * plot80).to_i64()) + Ext80::ONE).to_i64();
            let style = if g.chart_size == 1 || e < 11 || 1600 < e { 4 } else { 7 };
            spec.add(524, Line, PbStr::new(), pos, cast::<i32, _>(high_y), bar_w, cast::<i32, _>(bar_height), style);
            spec.add(525, Line, PbStr::new(), pos - 2, cast::<i32, _>(close_y), tick_w, 2, 4);
        }
    }
    spec.highs = highs.iter().map(|x| x.into_ext()).collect();
    spec.lows = lows.iter().map(|x| x.into_ext()).collect();
    spec.closes = closes.iter().map(|x| x.into_ext()).collect();
    spec.scale_max = scale_max;
    spec.thousands_shift = thousands_shift;
    spec.min_low = Ext::from(min_low.into_ext());
    spec.gdp_offset = Ext::from(gdp_offset.into_ext());
}

/// Port of ChartAxisMaxForValue (0x617bc3).
/// The top of the chart scale for a highest value: the next "round" step at or above it (1, 3, 5, 6, 8,
/// 10, 15, ... 80000, 100000; 400 for 300..400 and 4000 for 3000..4000); 0 above 100,000. Negative values
/// map to -45..-2 (unused by the game: `DrawPriceChart` only passes highs).
pub fn chart_axis_max_for_value(value: Ext80) -> i64 {
    let __content = crate::content::current();
    const STEPS: [(i64, i64); 61] = [
        (-50, -45),
        (-40, -35),
        (-35, -30),
        (-30, -25),
        (-25, -20),
        (-20, -15),
        (-15, -10),
        (-10, -5),
        (-5, -2),
        (0, -5),
        (1, 1),
        (3, 3),
        (5, 5),
        (6, 6),
        (8, 8),
        (10, 10),
        (15, 15),
        (20, 20),
        (25, 25),
        (30, 30),
        (35, 35),
        (40, 40),
        (50, 50),
        (60, 60),
        (75, 75),
        (85, 85),
        (100, 100),
        (125, 125),
        (150, 150),
        (175, 175),
        (200, 200),
        (250, 250),
        (300, 300),
        (350, 400),
        (400, 400),
        (500, 500),
        (600, 600),
        (800, 800),
        (900, 900),
        (1000, 1000),
        (1250, 1250),
        (1500, 1500),
        (2000, 2000),
        (2500, 2500),
        (3000, 3000),
        (3500, 4000),
        (4000, 4000),
        (5000, 5000),
        (6000, 6000),
        (8000, 8000),
        (9000, 9000),
        (10000, 10000),
        (12500, 12500),
        (15000, 15000),
        (20000, 20000),
        (25000, 25000),
        (30000, 30000),
        (40000, 40000),
        (50000, 50000),
        (60000, 60000),
        (80000, 80000),
    ];
    // value < threshold → result (first match)
    for (threshold, result) in STEPS {
        if lt(value, ei(threshold)) {
            return result;
        }
    }
    if !lt(ei(100000), value) { 100000 } else { 0 }
}

/// Port of SynthesizePriceHistory (0x619527).
/// Fakes the previous month(s) of a stock that has no history yet: `PriceHistHigh(0) = 0`, the series
/// is shifted back one month (two if month 2 is empty) and month 1 gets a close of
/// `MarketCap x (0.85 + 0.3 RND)`, a high of close + `0.15 RND x MarketCap` and a low of close -
/// `0.12 RND x MarketCap`; then `CapHistClose(0) = 3`. The first fake month caps the high below at and
/// the low above at the previous month's close; the second (bug #405) caps them at the previous month's
/// low and high instead.
pub fn synthesize_price_history(g: &mut Globals, rng: &mut Rng, entity_id: i32) {
    let i = entity_id;
    g.price_hist_high[(0, i)] = Ext::from_f64(0.0);
    let mut months_to_fake = 1;
    if g.cap_hist_close[(2, i)] == Ext::from_f64(0.0) {
        months_to_fake = 2;
    }
    let shift = |g: &mut Globals| {
        g.cap_hist_close[(3, i)] = g.cap_hist_close[(2, i)];
        g.price_hist_high[(3, i)] = g.price_hist_high[(2, i)];
        g.price_hist_low[(3, i)] = g.price_hist_low[(2, i)];
        g.cap_hist_close[(2, i)] = g.cap_hist_close[(1, i)];
        g.price_hist_high[(2, i)] = g.price_hist_high[(1, i)];
        g.price_hist_low[(2, i)] = g.price_hist_low[(1, i)];
    };
    let fake = |g: &mut Globals, rng: &mut Rng| -> Ext80 {
        let mc = e80(g.market_cap[i]);
        let r = rng.rnd();
        let close = mc * (e80(Ext::from(r)) * e80(sf(0.3f32))) + mc * e80(sf(0.85f32));
        g.cap_hist_close[(1, i)] = close.into_ext();
        let r = rng.rnd();
        g.price_hist_high[(1, i)] = (mc * (e80(Ext::from(r)) * e80(sf(0.15f32))) + close).into_ext();
        close
    };
    let fake_low = |g: &mut Globals, rng: &mut Rng, close: Ext80| {
        let mc = e80(g.market_cap[i]);
        let r = rng.rnd();
        g.price_hist_low[(1, i)] = (close - mc * (e80(Ext::from(r)) * e80(sf(0.12f32)))).into_ext();
    };
    shift(g);
    let close = fake(g, rng);
    if g.price_hist_high[(1, i)] < g.cap_hist_close[(2, i)] {
        g.price_hist_high[(1, i)] = g.cap_hist_close[(2, i)];
    }
    fake_low(g, rng, close);
    if g.cap_hist_close[(2, i)] < g.price_hist_low[(1, i)] {
        g.price_hist_low[(1, i)] = g.cap_hist_close[(2, i)];
    }
    if months_to_fake == 2 {
        shift(g);
        let close = fake(g, rng);
        if g.price_hist_high[(1, i)] < g.price_hist_low[(2, i)] {
            g.price_hist_high[(1, i)] = g.price_hist_low[(2, i)];
        }
        fake_low(g, rng, close);
        if g.price_hist_high[(2, i)] < g.price_hist_low[(1, i)] {
            g.price_hist_low[(1, i)] = g.price_hist_high[(2, i)];
        }
    }
    g.cap_hist_close[(0, i)] = Ext::from_f64(3.0);
}
