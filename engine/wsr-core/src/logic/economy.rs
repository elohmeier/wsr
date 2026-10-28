//! The macroeconomy and world events: interest rates, GDP, taxes, the quarterly macro step, scripted
//! crises (monetary policy, peak oil, subprime, pandemic, tech bubble), market cycles, industry outlook
//! and fads, crypto and commodity shocks, and the random news-event generator.
//!
//! Submodules by area (every routine carries its original address):
//! [`macro_update`], [`quarterly`], [`crises`], [`crises2`], [`cycle`], [`industry`], [`commodities`],
//! [`world_events`]. Shared here: `ShowMiscNewsStory` (0x53e1ee) and `CheckIndexHistLow` (0x53c9d5).
//!
//! SINGLE pool constants of the original (`fmul dword [0x75e…]`) are written `sf(0.05)`; SINGLE globals
//! are rounded with `as f32` on every store (`fstp dword`).

// Statements keep the original BASIC shape on purpose.
#![allow(
    clippy::assign_op_pattern,
    clippy::manual_range_contains,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::too_many_arguments
)]
use crate::pb::real::{Ext, cast};


pub mod commodities;
pub mod crises;
pub mod crises2;
pub mod cycle;
pub mod industry;
pub mod macro_update;
pub mod quarterly;
pub mod world_events;

pub use commodities::*;
pub use crises::*;
pub use crises2::*;
pub use cycle::*;
pub use industry::*;
pub use macro_update::*;
pub use quarterly::*;
pub use world_events::*;

pub(crate) use crate::logic::news::post_news;
pub(crate) use crate::logic::report::{show_quick_message, show_text_report};
pub(crate) use crate::pb::numeric::format;
pub(crate) use crate::pb::numeric::{str_num, val};
pub(crate) use crate::pb::pbstr::PbStr;
pub(crate) use crate::pb::rng::Rng;
pub(crate) use crate::pb::string::{instr, ltrim, mid, replace, rtrim};
pub(crate) use crate::pb::numeric::{int, to_integer, to_long, to_quad};
pub(crate) use crate::state::Globals;
pub(crate) use crate::ui::Ui;

use crate::pb::numeric::InputReader;
use crate::pb::string::{chr, mid_assign, right};
use std::sync::Mutex;

pub(crate) fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

/// A SINGLE constant of the original (`dword [0x75e…]`), widened to EXT.
#[allow(dead_code)]
pub(crate) fn sf(x: f32) -> Ext {
    cast::<Ext, _>(x)
}

/// `RTRIM$(LTRIM$(STR$(x)))` (PB_StrNum, 15 digits).
#[allow(dead_code)]
pub(crate) fn str_trim(x: Ext) -> PbStr {
    let t = str_num(x, 15);
    rtrim(ltrim(t.as_bytes()).as_bytes())
}

/// `RTRIM$(LTRIM$(FORMAT$(x, mask)))`.
pub(crate) fn fmt_trim(x: Ext, mask: &str) -> PbStr {
    let t = format(x, mask.as_bytes());
    rtrim(ltrim(&t).as_bytes())
}

/// Contents of `DataPath + "MSCNEWS.DAT"` (the encoded news stories read by ShowMiscNewsStory).
static MSCNEWS_DAT: Mutex<Option<Vec<u8>>> = Mutex::new(None);

/// Provides the contents of `MSCNEWS.DAT` (`None`: the file is missing, as if OPEN failed).
pub fn set_mscnews_dat(data: Option<Vec<u8>>) {
    *MSCNEWS_DAT.lock().unwrap_or_else(|e| e.into_inner()) = data;
}

/// Port of ShowMiscNewsStory (0x53e1ee) with its GOSUB MiscNews_DecodeSubstitute (0x53e798).
/// Shows news story `story_num` of `MSCNEWS.DAT` (a random one 1..35 drawn with `RandSeed` when
/// `story_num < 1`) as a " [runtime resource wording] " text report. The story lines are decoded and
/// `@YEAR` (`CurYear`; -2 for story 13, -20 for 26, +4 for 130), `@CORP` (`EntName(TmpExt2)`), `@AMT`
/// (`TmpExt1` formatted + `MillionPrefixUc`), `@TEXTSTRING` (`text_arg`) and `@HUMOR` (`TmpNewsStr`) are
/// substituted. `TmpExt1` and `TmpExt2` are cleared.
///
/// The file comes from [`set_mscnews_dat`]. Errors (missing file, no `@END` line before the end of the
/// file, `EntName` out of range) go to the ON ERROR handler, which only shows ShowRuntimeError: the
/// routine stops there.
pub fn show_misc_news_story(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, story_num: i32, text_arg: &[u8]) {
    let __content = crate::content::current();
    let text_string = PbStr::from(text_arg);
    let amount_text = fmt_trim(g.tmp_ext1, "* ######################,");
    g.tmp_ext1 = Ext::from_f64(0.0);
    let mut corp_name = PbStr::new();
    if Ext::from_f64(0.0) < g.tmp_ext2 {
        let corp_id = to_quad(g.tmp_ext2);
        if corp_id < 1 || corp_id > 1600 {
            return; // error 9 -> ON ERROR handler (ShowRuntimeError)
        }
        corp_name = g.ent_name[cast::<i32, _>(corp_id)].clone();
    }
    g.tmp_ext2 = Ext::from_f64(0.0);
    let mut story_id = cast::<i16, _>(story_num); // INTEGER local
    let mut story_year = g.cur_year;
    if story_id == 13 {
        story_year = g.cur_year - 2;
    }
    if story_id == 26 {
        story_year -= 20;
    }
    if story_id == 130 {
        story_year += 4;
    }
    if story_id < 1 {
        let mut rand_story;
        loop {
            g.rand_seed = g.rand_seed.wrapping_add(1);
            rand_story = g.rand_seed % 36;
            if 0 < rand_story && rand_story < 36 {
                break;
            }
        }
        story_id = cast::<i16, _>(rand_story);
    }
    let data = MSCNEWS_DAT.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let Some(data) = data else {
        return; // OPEN fails (error 53) -> ON ERROR handler
    };
    let mut story_lines: Vec<PbStr> = vec![PbStr::new(); 301]; // DIM storyLines(1 TO 300)
    let mut rd = InputReader::new(&data);
    let mut line_idx: i32 = 0;
    let mut line_count: i32 = 0;
    let mut item_id: i16 = 0;
    loop {
        let Ok(line) = rd.try_line_input() else {
            return; // input past end (error 62) -> ON ERROR handler
        };
        let line_text = rtrim(&line);
        if mid(line_text.as_bytes(), 1, 3) == "@IT" {
            item_id = to_integer(val(right(line_text.as_bytes(), 3).as_bytes()));
        }
        if item_id == story_id {
            line_idx += 1;
            line_count = line_idx;
            if line_idx > 300 {
                return; // error 9 -> ON ERROR handler
            }
            story_lines[cast::<usize, _>(line_idx)] = line_text.clone();
            if mid(line_text.as_bytes(), 1, 3) == "@IT" || mid(line_text.as_bytes(), 1, 4) == "@END" {
                story_lines[cast::<usize, _>(line_idx)] = PbStr::new();
                if 1 < line_idx {
                    break;
                }
            }
        }
        if line_text == "@END" {
            break;
        }
    }
    // CLOSE #fileNum
    // GOSUB MiscNews_DecodeSubstitute
    for line_idx in 1..=line_count {
        let mut line_text = story_lines[cast::<usize, _>(line_idx)].clone();
        let line_len = cast::<i32, _>(line_text.len());
        if line_len == 0 {
            line_text = PbStr::new();
        } else {
            for char_pos in 1..=line_len {
                let ch = mid(line_text.as_bytes(), char_pos, 1);
                let code = cast::<i32, _>(ch.as_bytes()[0]);
                let decoded_ch = if code < 40 || 170 < code {
                    ch
                } else if char_pos < 6 {
                    chr(&[code - (char_pos * 2 + 26)])
                } else {
                    chr(&[code - 39])
                };
                mid_assign(&mut line_text, char_pos, 1, decoded_ch.as_bytes());
            }
        }
        if 0 < instr(1, line_text.as_bytes(), b"@YEAR") {
            line_text = replace(line_text.as_bytes(), b"@YEAR", str_num(cast::<Ext, _>(story_year), 15).as_bytes());
        }
        if 0 < instr(1, line_text.as_bytes(), b"@CORP") {
            line_text = replace(line_text.as_bytes(), b"@CORP", corp_name.as_bytes());
        }
        if 0 < instr(1, line_text.as_bytes(), b"@AMT") {
            let amt = amount_text.clone() + " " + &g.million_prefix_uc;
            line_text = replace(line_text.as_bytes(), b"@AMT", amt.as_bytes());
        }
        if 0 < instr(1, line_text.as_bytes(), b"@TEXTSTRING") {
            line_text = replace(line_text.as_bytes(), b"@TEXTSTRING", text_string.as_bytes());
        }
        if 0 < instr(1, line_text.as_bytes(), b"@HUMOR") {
            line_text = replace(line_text.as_bytes(), b"@HUMOR", g.tmp_news_str.as_bytes());
        }
        story_lines[cast::<usize, _>(line_idx)] = line_text;
    }
    if line_count + 1 > 300 {
        return; // error 9 -> ON ERROR handler
    }
    story_lines[cast::<usize, _>(line_count + 1)] = s("@END");
    let last_line = line_count + 1;
    for i in 1..=last_line {
        if story_lines[cast::<usize, _>(i)] == "@END" {
            g.text_lines[i] = story_lines[cast::<usize, _>(i)].clone();
        } else {
            g.text_lines[i] = s("       ") + &story_lines[cast::<usize, _>(i)];
        }
    }
    g.text_box_title = s(__content.string(4369));
    g.entity_pick_mode = PbStr::new();
    close_all_dialogs(g, ui, rng, 1);
    // (UI) DisableMainControls
    g.text_box_mode = s("30");
    show_text_report(g, ui, rng);
    // (UI) EnableMainControls, PauseTicker
}

/// Port of CheckIndexHistLow (0x53c9d5).
/// 1 if the stock-index history (series 1604) has a monthly low below -2.5 (skipped when
/// `RandSeed MOD 17 = 11`), else 0.
pub fn check_index_hist_low(g: &Globals) -> i32 {
    if g.rand_seed % 17 != 11 {
        for hist_idx in 1..=60 {
            if g.price_hist_low[(hist_idx, 1604)] < -Ext::from_f64(2.5) {
                return 1;
            }
        }
    }
    0
}

/// Port of CloseAllDialogs (0x493e2b): ends the modeless dialogs and clears their open counters.
/// With `close_trans_menu = 1`, ending the action menu also runs its destruction callback before
/// decrementing `ActionMenuOpen`. That callback can advance the simulation, including nested news.
pub fn close_all_dialogs(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, close_trans_menu: i32) {
    let now = ui.local_time();
    let env = crate::logic::shell::Env::new(&now);
    close_all_dialogs_skipping(g, ui, rng, close_trans_menu, &env);
}

#[doc(hidden)]
pub fn close_all_dialogs_skipping(
    g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, close_trans_menu: i32,
    env: &crate::logic::shell::Env,
) {
    if g.dlg_open_news_report > 0 {
        ui.end_dialog(crate::ui::DialogKind::Report);
    }
    if g.dlg_open_commod_desk > 0 {
        ui.end_dialog(crate::ui::DialogKind::CommodityDesk);
    }
    if close_trans_menu == 1 {
        crate::logic::shell::action_menu::dialog_end_trans_menu(g, ui, rng, env);
        g.action_menu_open = g.action_menu_open - 1;
    }
    for flag in [
        &mut g.dlg_open_news_report,
        &mut g.symbol_dlg_open,
        &mut g.dlg_open_alpha_list,
        &mut g.dlg_open_db_search,
        &mut g.dlg_open_holdings_diagram,
        &mut g.dlg_open_gov_bond_desk,
        &mut g.dlg_open_cap_contrib,
        &mut g.dlg_open_commod_desk,
        &mut g.dlg_open_swap_dlg,
        &mut g.dlg_open_who_owns_what,
        &mut g.dlg_open_cheat_menu,
        &mut g.dlg_open_crypto,
    ] {
        if 0 < *flag {
            *flag = 0;
        }
    }
    // (UI) DIALOG END hChartDlg
}
