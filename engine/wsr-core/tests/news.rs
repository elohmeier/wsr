//! Unit tests for the news ticker: PostNews (0x41e7d5), IsInRecentHeadlines (0x49e149),
//! LoadCurrencyHeadlines (0x41eddc).

#![allow(clippy::field_reassign_with_default)]
use wsr_core::pb::real::{Ext, cast};


use wsr_core::logic::news::{is_in_recent_headlines, load_currency_headlines, post_news};
use wsr_core::pb::pbstr::PbStr;
use wsr_core::state::Globals;
use wsr_core::ui::Scripted;

fn p(s: &str) -> PbStr {
    PbStr::from(s)
}

fn game() -> Globals {
    let __content = wsr_core::content::current();
    let mut g = Globals::default();
    g.num_players = 2;
    g.cur_player = 1;
    g.is_computer_player[1] = 0;
    g.is_computer_player[2] = 1;
    g.sim_co_count = 777;
    for i in 1..=60 {
        g.news_headline[i] = p(&format!("{__wsr_text_13928}{i}.",__wsr_text_13928=__content.string(13928)));
    }
    g
}

#[test]
fn empty_or_repeated_message_only_marks_ticker() {
    let mut g = game();
    let mut ui = Scripted::default();
    post_news(&mut g, &mut ui, 0, 0);
    assert_eq!(g.ticker_dirty, p("Y"));
    g.ticker_dirty = PbStr::new();
    g.news_msg = p("old 60.");
    post_news(&mut g, &mut ui, 0, 0);
    assert_eq!(g.ticker_dirty, p("Y"));
    assert_eq!(g.news_msg, p("old 60."), "NewsMsg is kept in this path");
    assert!(ui.log.is_empty());
}

#[test]
fn stores_cleaned_headline_and_shifts() {
    let mut g = game();
    let mut ui = Scripted::default();
    g.news_msg = p("  ABC says \"hi\" | bye  ");
    post_news(&mut g, &mut ui, 0, 0);
    assert_eq!(g.news_headline[60], p("ABC says 'hi'   bye."));
    assert_eq!(g.news_headline[59], p("old 60."));
    assert_eq!(g.news_headline[1], p("old 2."));
    assert!(g.news_msg.is_empty());
    assert_eq!(g.last_news_co_cnt, 777);
    assert_eq!(ui.log, vec!["NEWS ABC says 'hi'   bye.".to_string()]);
}

#[test]
fn keeps_terminal_punctuation() {
    let mut g = game();
    let mut ui = Scripted::default();
    for (msg, stored) in [("Up!", "Up!"), ("Why?", "Why?"), ("(see below)", "(see below)"), ("x:", "x:"), ("ab", "ab.")] {
        g.news_msg = p(msg);
        post_news(&mut g, &mut ui, 0, 0);
        assert_eq!(g.news_headline[60], p(stored));
    }
}

#[test]
fn duplicate_is_dropped() {
    let mut g = game();
    let mut ui = Scripted::default();
    g.news_msg = p("old 5");
    g.last_news_co_cnt = 1;
    post_news(&mut g, &mut ui, 1, 0); // would pop up for player 1, but duplicates never do
    assert!(g.news_msg.is_empty());
    assert_eq!(g.news_headline[60], p("old 60."));
    assert_eq!(g.last_news_co_cnt, 1);
    assert!(ui.log.is_empty());
}

#[test]
fn popup_for_human_player_encodes_line_breaks() {
    let mut g = game();
    let mut ui = Scripted::default();
    g.news_msg = p("First\r\n\r\nSecond\r\nThird.");
    post_news(&mut g, &mut ui, 0, 1);
    // ShowQuickMessage changed NewsMsg in place before it was stored
    assert_eq!(g.news_headline[60], p("First@@Second##Third."));
    assert_eq!(ui.log.len(), 2);
    assert!(ui.log[0].starts_with("MSGBOX [] First\r\n"));
}

#[test]
fn popup_rules_for_companies() {
    let __content = wsr_core::content::current();
    let mut g = game();
    g.corp_controller[50] = 1;
    g.corp_controller[60] = 2;
    g.sel_entity = 60;
    let cases = [
        // (entity_a, entity_b, action_code, suppress, popup?)
        (50, 0, "", 0, true),      // controlled by the human current player
        (60, 0, "", 0, true),      // selected company
        (70, 0, "", 0, false),     // unrelated
        (50, 0, "AUTO", 0, false), // AUTO: only the selected company pops up
        (60, 0, "AUTO", 0, true),
        (70, 50, "", 0, true), // second entity checked when the first gave no popup
        (50, 0, "", 1, false), // SuppressPopups
        (2, 0, "", 0, false),  // the computer player
    ];
    for (k, (a, b, action, sup, popup)) in cases.into_iter().enumerate() {
        let mut ui = Scripted::default();
        g.action_code = p(action);
        g.suppress_popups = sup;
        g.news_msg = p(&format!("{__wsr_text_13924}{k}",__wsr_text_13924=__content.string(13924)));
        post_news(&mut g, &mut ui, a, b);
        let shown = ui.log.iter().any(|l| l.starts_with("MSGBOX"));
        assert_eq!(shown, popup, "{__wsr_text_13924}{k}",__wsr_text_13924=__content.string(13924));
    }
}

#[test]
fn computer_current_player_gets_no_popups() {
    let mut g = game();
    g.cur_player = 2;
    let mut ui = Scripted::default();
    g.news_msg = p("hello");
    post_news(&mut g, &mut ui, 2, 0);
    assert!(!ui.log.iter().any(|l| l.starts_with("MSGBOX")));
}

#[test]
fn recent_headlines() {
    let __content = wsr_core::content::current();
    let mut g = game();
    g.news_headline[17] = p("XYZ Corp declares bankruptcy.");
    assert_eq!(is_in_recent_headlines(&g, __content.bytes(13929)), 1);
    assert_eq!(is_in_recent_headlines(&g, b"BANKRUPTCY"), 0);
    assert_eq!(is_in_recent_headlines(&g, b"old 3"), 1);
}

#[test]
fn currency_headlines() {
    let __content = wsr_core::content::current();
    let mut g = game();
    g.currency_code = p("USA");
    load_currency_headlines(&mut g);
    assert_eq!(g.news_headline[54], p(__content.string(6817)));
    assert_eq!(g.news_headline[55], p("  "));
    assert_eq!(
        g.news_headline[59],
        p(__content.string(14430))
    );
    assert_eq!(g.news_headline[60], p(__content.string(6821)));
    assert_eq!(g.news_headline[53], p("old 53."));
    g.pref_currency = p("UK");
    load_currency_headlines(&mut g);
    assert_eq!(
        g.news_headline[59],
        p(__content.string(14453))
    );
}

// ------------------------------------------------------------------ differential tests (original code)

mod diff;

use diff::{Compare, Opts, Rand, run_original_with};

/// Headlines drawn from a small pool so that duplicates happen.
fn random_news_state(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let mut r = Rand(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut g = Globals::default();
    g.num_players = cast::<i32, _>(r.range(1, 4));
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    for pl in 1..=5 {
        g.is_computer_player[pl] = cast::<i32, _>(r.range(0, 1));
    }
    for c in 11..=60 {
        g.corp_controller[c] = cast::<i32, _>(r.range(0, 60));
    }
    g.sel_entity = cast::<i32, _>(r.range(0, 60));
    g.action_code = PbStr::from(if r.chance(Ext::from_f64(0.3)) { "AUTO" } else { "BUY" });
    g.sim_co_count = cast::<i32, _>(r.range(1, 1600));
    g.last_news_co_cnt = 3;
    // popups write TMP_QM.DAT, which fails under emulation: keep them off (see popup test below)
    g.suppress_popups = 1;
    for i in 1..=60 {
        g.news_headline[i] = PbStr::from(format!("{__wsr_text_13546}{}.", r.range(0, 80),__wsr_text_13546=__content.string(13546)).as_str());
    }
    g
}

fn random_message(r: &mut Rand) -> PbStr {
    let __content = wsr_core::content::current();
    let m = match r.range(0, 7) {
        0 => String::new(),
        1 => format!("{__wsr_text_13546}{}", r.range(0, 80),__wsr_text_13546=__content.string(13546)),
        2 => format!("{__wsr_text_13546}{}.", r.range(0, 80),__wsr_text_13546=__content.string(13546)),
        3 => format!("  \"Quoted\" | piped {}  ", r.range(0, 9)),
        4 => format!("Ends with {}", [".", "!", "?", "'", "-", "_", ":", ")", ";", ","][cast::<usize, _>(r.range(0, 9))]),
        5 => format!("Two\r\nlines {}", r.range(0, 9)),
        _ => format!("XYZ rises {}%", r.range(0, 99)),
    };
    PbStr::from(m.as_str())
}

#[test]
fn post_news_matches_original() {
    let mut runs = 0;
    for seed in 1u64..=12 {
        let mut g = random_news_state(seed);
        let mut r = Rand(seed);
        g.news_msg = random_message(&mut r);
        if seed == 3 {
            g.news_msg = g.news_headline[60].clone();
        }
        let a = cast::<i32, _>(r.range(-1, 60));
        let b = cast::<i32, _>(r.range(-1, 60));
        let opts = Opts { args: vec![a.to_string(), b.to_string()], ..Opts::default() };
        let Some(o) = run_original_with(0x41e7d5, &opts, &g) else {
            return;
        };
        let mut port = g.clone();
        let mut ui = Scripted::default();
        post_news(&mut port, &mut ui, a, b);
        assert!(o.log.is_empty(), "seed {seed}: {:?}", o.log);
        Compare::default().assert(&port, &o.globals);
        runs += 1;
    }
    eprintln!("{runs} PostNews runs compared");
}

/// With popups on, the original writes TMP_QM.DAT (CreateFileA fails under emulation, a PB error is
/// raised) and runs qm.exe; NewsMsg is CR/LF-encoded in place either way.
#[test]
fn post_news_popup_matches_original() {
    for seed in 20u64..=25 {
        let mut g = random_news_state(seed);
        g.suppress_popups = 0;
        g.is_computer_player[g.cur_player] = 0;
        g.news_msg = PbStr::from(format!("Para one\r\n\r\nline two\r\nline {seed}").as_str());
        let a = g.cur_player;
        let opts = Opts { args: vec![a.to_string(), "0".into()], ..Opts::default() };
        let Some(o) = run_original_with(0x41e7d5, &opts, &g) else {
            return;
        };
        let mut port = g.clone();
        let mut ui = Scripted::default();
        post_news(&mut port, &mut ui, a, 0);
        assert!(o.log.iter().any(|l| l.contains("TMP_QM.DAT")), "{:?}", o.log);
        assert_eq!(ui.log.iter().filter(|l| l.starts_with("MSGBOX")).count(), 1);
        Compare::default().ignore("TickerPauseUntil").assert(&port, &o.globals);
    }
}

#[test]
fn load_currency_headlines_matches_original() {
    for (pref, code) in [("", "USA"), ("EUR", "UK"), ("", "")] {
        let mut g = random_news_state(7);
        g.pref_currency = PbStr::from(pref);
        g.currency_code = PbStr::from(code);
        let Some(o) = run_original_with(0x41eddc, &Opts::default(), &g) else {
            return;
        };
        let mut port = g.clone();
        load_currency_headlines(&mut port);
        Compare::default().assert(&port, &o.globals);
    }
}

#[test]
fn is_in_recent_headlines_matches_original() {
    let __content = wsr_core::content::current();
    let g = random_news_state(9);
    for needle in ["Headline 1", "Headline 79.", __content.string(13930), "", "line 4"] {
        let opts = Opts { args: vec![format!("s:{needle}")], ..Opts::default() };
        let Some(o) = run_original_with(0x49e149, &opts, &g) else {
            return;
        };
        assert_eq!(is_in_recent_headlines(&g, needle.as_bytes()), cast::<i32, _>(o.return_value), "{needle:?}");
    }
}
