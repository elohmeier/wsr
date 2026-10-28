//! Portable implementations of the original's three PowerBASIC extension DLLs.
use crate::pb::real::cast;

pub mod high_score;
mod law_text;
pub mod remarks;

use crate::{pb::pbstr::PbStr, ui::Ui};

/// LAWDLL.DLL!LawFirm (0x4010ca), including the three selection callbacks and OK (0x401808).
/// Closing retains the current selection; an initial zero defaults to the average firm.
pub fn law_firm(ui: &mut dyn Ui, current: i32, currency: &PbStr, million_prefix: &PbStr) -> i32 {
    let __content = crate::content::current();
    let mut selected = if current == 0 { 2 } else { current };
    let names = ["Cheap", "Average", __content.string(5670)];
    let options = [
        __content.string(5671),
        __content.string(5672),
        __content.string(5673),
        "&OK",
    ]
    .map(PbStr::from);
    loop {
        let name = names.get(cast::<usize, _>(selected - 1)).unwrap_or(&"Average");
        let prompt = [PbStr::from(format!("{__wsr_text_5674}{name}{__wsr_text_5675}",__wsr_text_5674=__content.string(5674), __wsr_text_5675=__content.string(5675)).as_str())];
        match ui.choose(&PbStr::from(__content.string(5676)), &prompt, &options) {
            Some(k @ 0..=2) => selected = cast::<i32, _>(k) + 1,
            Some(3) => {
                let seconds = cast::<usize, _>(ui.local_time().second.rem_euclid(60));
                let k = match selected {
                    1 => seconds % 5,
                    2 => 5 + seconds % 2,
                    3 => law_text::EXPENSIVE_VARIANTS[seconds % 30],
                    _ => return selected,
                };
                let text = law_text::LAW_TEXT(&__content)[k]
                    .replace(__content.string(5677), &currency.to_display())
                    .replace(__content.string(5678), &million_prefix.to_display());
                ui.msgbox(
                    &PbStr::from(text.as_str()),
                    4160,
                    &PbStr::from(__content.string(5679)),
                );
                return selected;
            }
            _ => return selected,
        }
    }
}
