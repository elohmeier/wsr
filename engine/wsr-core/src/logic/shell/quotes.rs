//! Startup quote reader and its modal dialog.
use crate::pb::real::cast;


use super::prelude::*;

/// Port of QuoteOfDay_DayOfYearOffset (0x4a10de), GOSUB of ShowQuoteOfDay.
/// The original treats every year divisible by four as a leap year, including 1900 and 2100.
pub fn quote_day(now: &LocalTime) -> i32 {
    let mut offset = match now.month {
        1 => 0, 2 => 31, 3 => 59, 4 => 90, 5 => 120, 6 => 151,
        7 => 181, 8 => 212, 9 => 243, 10 => 273, 11 => 304, 12 => 334, _ => 0,
    };
    if now.month > 2 && now.year % 4 == 0 { offset += 1; }
    (offset + now.day).clamp(1, 366)
}

/// Port of QuoteOfDay_Decrypt (0x4a12c6), GOSUB of ShowQuoteOfDay.
/// This decoder has no save codec's below-16 abort; encoded bytes above 170 stay unchanged.
pub fn decrypt_quote_line(line: &[u8]) -> PbStr {
    PbStr(line.iter().enumerate().map(|(k, &c)| {
        if (40..=170).contains(&c) {
            c.wrapping_sub(if k < 5 { cast::<u8, _>(28 + 2 * k) } else { 39 })
        } else { c }
    }).collect())
}

/// Port of ShowQuoteOfDay (0x4a0c7f).
/// Reads the current date's @ITnnn entry from QUOTEWSR.DAT. On XP a flagfile suppresses the quote and
/// is deleted. A missing entry/file leaves the previous quote unchanged.
pub fn show_quote_of_day(g: &mut Globals, ui: &mut dyn Ui, env: &Env) {
    let __content = crate::content::current();
    if env.files.borrow().exists(b"FLAGFILE.TXT") && g.win_version == "XP" {
        env.files.borrow_mut().delete(b"FLAGFILE.TXT");
        return;
    }
    let Some(data) = env.files.borrow().read(b"QUOTEWSR.DAT").cloned() else { return };
    let tag = format!("{__wsr_text_10180}{:03}", quote_day(env.now),__wsr_text_10180=__content.string(10180));
    let mut found = false;
    let mut quote = PbStr::new();
    for line in crate::pb::codec::lines(&data) {
        if !found {
            if line.starts_with(b"@END") { return; }
            if line.starts_with(tag.as_bytes()) { found = true; }
        } else if line.contains(&b'@') {
            if line.starts_with(b"@END") || line.starts_with(b"@IT") {
                g.quote_of_day_text = quote;
                show_quote_of_day_dialog(g, ui);
                return;
            }
        } else {
            let text = decrypt_quote_line(line);
            quote = quote + &text + " ";
            if text.len() < 2 { quote = quote + "\r\n\r\n"; }
        }
    }
}

/// Port of ShowQuoteOfDayDialog (0x4a1f8f), DlgProcQuoteOfDay (0x4a2246) and
/// DlgProcQuoteOfDayOk (0x4a2593): a text box and OK, with no simulation side effects.
pub fn show_quote_of_day_dialog(g: &Globals, ui: &mut dyn Ui) {
    let __content = crate::content::current();
    let title = if instr(1, g.quote_of_day_text.as_bytes(), __content.bytes(10181)) > 0 {
        s(__content.string(10182))
    } else { s(__content.string(10183)) };
    ui.choose(&title, &[g.quote_of_day_text.clone()], &[s("OK")]);
}
