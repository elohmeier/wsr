//! The news ticker: `PostNews` (0x41e7d5), `IsInRecentHeadlines` (0x49e149), `LoadCurrencyHeadlines`
//! (0x41eddc).
//!
//! `NewsHeadline(1 TO 60)` is the ticker history, newest in element 60. The main window's news listbox
//! (control 205) shows elements 54..60 (`RefreshNewsListbox`, 0x41e749, UI only); the port reports each
//! newly stored headline through [`Ui::news`] instead.

// Statements keep the original BASIC shape on purpose.
#![allow(clippy::assign_op_pattern)]
use crate::pb::real::cast;


use crate::logic::report::show_quick_message;
use crate::pb::pbstr::PbStr;
use crate::pb::string::{instr, instr_any, ltrim, replace, replace_any, right, rtrim};
use crate::state::Globals;
use crate::ui::Ui;

/// Port of PostNews (0x41e7d5).
/// Posts `NewsMsg` to the ticker: trims it, replaces `"` by `'` and `|` by a blank, appends a `.` unless
/// it already ends in one of `.!?'-_:)`, drops it if it equals one of the 60 stored headlines, else
/// shifts `NewsHeadline` up and stores it in element 60. Shows it as a popup (`ShowQuickMessage`) when it
/// concerns the human current player, a company that player controls, or the selected company (no popup
/// for duplicates, when `SuppressPopups` is set, or — for companies other than `SelEntity` — while
/// `ActionCode = "AUTO"`). `NewsMsg` is cleared in all paths except the first.
///
/// Note: `ShowQuickMessage` takes `NewsMsg` BYREF and encodes CR/LF as `@@`/`##` in place, so a popped-up
/// headline is stored in that encoded form.
pub fn post_news(g: &mut Globals, ui: &mut dyn Ui, entity_a: i32, entity_b: i32) {
    if g.news_msg.is_empty() || g.news_msg == g.news_headline[60] {
        g.ticker_dirty = PbStr::from("Y");
        return;
    }
    g.news_msg = rtrim(ltrim(g.news_msg.as_bytes()).as_bytes());

    // locals (zeroed by the frame setup)
    let mut human_player = 0;
    let mut show_popup = 0;
    let mut dup_idx = 0;

    if g.is_computer_player[g.cur_player] == 0 {
        human_player = g.cur_player;
    }
    if 0 < entity_a && human_player == entity_a {
        show_popup = 1;
    }
    if 0 < entity_b && human_player == entity_b {
        show_popup = 1;
    }
    if 10 < entity_a || 10 < entity_b {
        if 10 < entity_a {
            if g.sel_entity == entity_a || human_player == g.corp_controller[entity_a] {
                show_popup = 1;
            }
            if g.sel_entity != entity_a && g.action_code == "AUTO" {
                show_popup = 0;
            }
        }
        if 10 < entity_b && show_popup == 0 {
            if g.sel_entity == entity_b || human_player == g.corp_controller[entity_b] {
                show_popup = 1;
            }
            if g.sel_entity != entity_b && g.action_code == "AUTO" {
                show_popup = 0;
            }
        }
    }

    g.news_msg = replace_any(g.news_msg.as_bytes(), b"\"", b"'");
    g.news_msg = replace_any(g.news_msg.as_bytes(), b"|", b" ");
    let last_char = right(g.news_msg.as_bytes(), 1);
    if instr_any(1, last_char.as_bytes(), b".!?'-_:)") == 0 {
        g.news_msg = g.news_msg.clone() + ".";
    }
    for headline_idx in 1..=60 {
        if g.news_msg == g.news_headline[headline_idx] {
            show_popup = 0;
            dup_idx = headline_idx;
        }
    }
    if show_popup == 1 && g.suppress_popups == 0 {
        let mut msg = std::mem::take(&mut g.news_msg);
        show_quick_message(g, ui, &mut msg);
        g.news_msg = msg;
    }
    if dup_idx < 1 {
        for shift_idx in 1..=59 {
            g.news_headline[shift_idx] = g.news_headline[shift_idx + 1].clone();
        }
        if !g.news_msg.is_empty() {
            g.news_headline[60] = g.news_msg.clone();
        }
        g.news_msg = PbStr::new();
        g.last_news_co_cnt = g.sim_co_count;
        // (UI) the ticker/listbox redraw happens elsewhere (RefreshNewsListbox 0x41e749); the port
        // reports the new headline here.
        ui.news(&g.news_headline[60]);
    } else {
        g.news_msg = PbStr::new();
    }
}

/// Port of IsInRecentHeadlines (0x49e149).
/// 1 if `search_text` occurs (case-sensitive `INSTR`) in any of `NewsHeadline(1..60)`, else 0.
pub fn is_in_recent_headlines(g: &Globals, search_text: &[u8]) -> i32 {
    for i in 1..=60 {
        if 0 < instr(1, g.news_headline[i].as_bytes(), search_text) {
            return 1;
        }
    }
    0
}

/// The `DATA` block read by [`load_currency_headlines`] (at 0x41ec75: count 7, NUL-separated items).
#[allow(non_snake_case)]
fn CURRENCY_HEADLINES(__content: &crate::content::ContentPack) -> [&str; 7] { [
    __content.string(6817),
    "  ",
    __content.string(6818),
    " ",
    __content.string(6819),
    __content.string(6820),
    __content.string(6821),
] }

/// Port of LoadCurrencyHeadlines (0x41eddc).
/// Fills `NewsHeadline(54..60)` with the start-up help text (`READ$` 1..7), with `XXX` replaced by
/// `PrefCurrency` (or `CurrencyCode` when that is empty).
pub fn load_currency_headlines(g: &mut Globals) {
    let __content = crate::content::current();
    let mut currency = g.pref_currency.clone();
    if currency.is_empty() {
        currency = g.currency_code.clone();
    }
    for line_idx in 1..8 {
        g.news_headline[line_idx + 53] = PbStr::from(CURRENCY_HEADLINES(&__content)[cast::<usize, _>(line_idx - 1)]);
        if 0 < instr(1, g.news_headline[line_idx + 53].as_bytes(), b"XXX") {
            g.news_headline[line_idx + 53] =
                replace(g.news_headline[line_idx + 53].as_bytes(), b"XXX", currency.as_bytes());
        }
    }
}
