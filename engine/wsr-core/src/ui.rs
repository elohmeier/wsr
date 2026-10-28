//! The engine's only window to the outside world.
//!
//! The original calls modal Windows dialogs from the middle of game logic (697 `MSGBOX`/`INPUTBOX`
//! sites). The port keeps that shape: engine code calls these methods synchronously. On the web the
//! engine runs in a Web Worker and the implementation blocks on `Atomics.wait` until the page answers;
//! native headless tests use [`Scripted`].

use crate::pb::pbstr::PbStr;

/// `MSGBOX` style bits that matter to the logic (Win32 `MB_*`).
pub mod mb {
    pub const OK: i32 = 0x0;
    pub const OKCANCEL: i32 = 0x1;
    pub const YESNOCANCEL: i32 = 0x3;
    pub const YESNO: i32 = 0x4;
    pub const ICONSTOP: i32 = 0x10;
    pub const ICONQUESTION: i32 = 0x20;
    pub const ICONEXCLAMATION: i32 = 0x30;
    pub const ICONINFORMATION: i32 = 0x40;
    pub const TASKMODAL: i32 = 0x2000;
}

/// `MSGBOX` results (Win32 `ID*`).
pub mod id {
    pub const OK: i32 = 1;
    pub const CANCEL: i32 = 2;
    pub const YES: i32 = 6;
    pub const NO: i32 = 7;
}

/// How the user left a report dialog (ShowReportDialog 0x41727f and its button callbacks).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReportAction {
    /// OK (or YES when the report asks a YES/NO question).
    Ok,
    /// NO on a YES/NO report.
    No,
    /// The user clicked a line carrying a `@C<id>`/`@I<id>` link and confirmed: the link code, e.g. `"@C0123"`.
    Link(PbStr),
    /// Window closed without pressing a button.
    Closed,
    /// Open the earnings report's stock chart without dismissing the report.
    Chart,
    /// Activate the research report's analyst-rating trade callback.
    Rating,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReportOptions {
    pub analyst_rating: Option<i32>,
    pub rating_trade: bool,
    pub stock_chart: bool,
}

/// Windows that a nested transaction can end while their modal caller is still on the stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogKind {
    CommodityDesk,
    Report,
}

/// Editable controls of one original modal window. Hosts without forms use the existing
/// scripted choice/input path; interactive hosts submit all controls together.
#[derive(Clone, Debug, Default)]
pub struct FormField {
    pub id: i32,
    pub label: PbStr,
    pub value: PbStr,
    pub hint: PbStr,
    pub numeric: bool,
    pub min: Option<i32>,
    pub max: Option<i32>,
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
    pub choices: Vec<(PbStr, PbStr)>,
    pub refresh_on_change: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Form {
    pub title: PbStr,
    pub prompt: Vec<PbStr>,
    pub fields: Vec<FormField>,
    pub buttons: Vec<(i32, PbStr)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormResponse {
    /// -1 closes the window; other ids are callbacks from `Form::buttons` (0 refreshes fields).
    pub button: i32,
    /// Text in the same order as `Form::fields`, including empty fields.
    pub values: Vec<PbStr>,
}

#[derive(Clone, Debug)]
pub struct SymbolChoice {
    pub id: i32,
    pub name: PbStr,
    pub symbol: PbStr,
}

pub trait Ui {
    /// Opt-in host policy: bound legacy random searches and validate load/save boundaries.
    /// Compatibility hosts retain the original behavior; browser sessions enable these guards.
    fn safety_checks(&self) -> bool { false }
    /// A failed guarded operation. Interactive bindings also retain this as the command error.
    fn game_error(&mut self, text: &PbStr, title: &PbStr) { self.msgbox(text, 8208, title); }
    /// The host's local clock, sampled around modal dialogs for the original's ticker pauses.
    /// Deterministic/headless hosts default to the emulator clock; interactive hosts override this.
    fn local_time(&self) -> crate::logic::newgame::LocalTime {
        crate::logic::newgame::LocalTime::HARNESS
    }
    /// Persist a game file produced by a simulation-triggered save. Interactive hosts retain these
    /// bytes in their virtual data directory; headless hosts can omit persistence.
    fn write_file(&mut self, _name: &PbStr, _data: &[u8]) {}
    /// Original manual-save confirmation. A browser host confirms only after its durable storage
    /// transaction commits; it can replace this modal message with that existing acknowledgement.
    fn save_confirmation(&mut self, text: &PbStr, title: &PbStr) {
        self.msgbox(text, 8192, title);
    }
    /// `DIALOG END`: notify a modal caller that a nested routine destroyed its window.
    fn end_dialog(&mut self, _dialog: DialogKind) {}
    /// Host-owned WSRDLL remark pool. A headless host may omit decorative remarks.
    fn random_remark(&mut self) -> PbStr { PbStr::new() }
    /// The VARICHEK high-score record in the host's data directory; headless hosts may omit it.
    fn high_score(&mut self, _path: &PbStr, _name: &PbStr, _score: i64) -> i32 { 0 }
    /// `MSGBOX text, style, title` → button id.
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32;
    /// `INPUTBOX$(prompt, title, default)` → entered text ("" on cancel).
    fn inputbox(&mut self, prompt: &PbStr, title: &PbStr, default: &PbStr) -> PbStr;
    fn form(&mut self, _form: &Form) -> Option<FormResponse> { None }
    /// Symbol entry with the original LOOK UP list, already filtered by the engine.
    fn symbol_input(&mut self, prompt: &PbStr, title: &PbStr, default: &PbStr, _companies: &[SymbolChoice]) -> PbStr {
        self.inputbox(prompt, title, default)
    }
    /// A text report (`TextLines()` + `ShowTextReport`): lines may carry `@C<id>`/`@I<id>` links.
    /// `yes_no`: the report asks a question and shows YES/NO buttons instead of OK.
    fn report(&mut self, title: &PbStr, lines: &[PbStr], yes_no: bool) -> ReportAction;
    /// Optional report controls. Existing/headless hosts retain the text-only presentation.
    fn report_with_actions(&mut self, title: &PbStr, lines: &[PbStr], yes_no: bool, _options: &ReportOptions) -> ReportAction {
        self.report(title, lines, yes_no)
    }
    /// A headline posted to the news ticker (`PostNews`).
    fn news(&mut self, headline: &PbStr);
    /// Ownership boxes and connecting lines. Headless hosts retain the linked-report presentation.
    fn ownership_diagram(&mut self, spec: &crate::logic::info_screens::ownership::OwnershipDiagram) -> ReportAction {
        self.report(&spec.title, &crate::logic::info_screens::ownership::diagram_report_lines(spec), false)
    }
    /// A host's editable eight-leg order sheet. None uses the generic choice/input fallback.
    fn option_station(&mut self, _spec: &crate::logic::trade_screens::option_station::OptionStation)
        -> Option<Vec<crate::logic::trade_screens::option_station::StationAction>> { None }
    /// The modal price chart of `ShowPriceChart` (0x6141b1): the original's chart controls, series and
    /// scale ([`ChartSpec`](crate::logic::info_screens::chart::ChartSpec)). Returns `true` when the user
    /// pressed the trade button (only offered if `spec.trade_label` is set), `false` for CLOSE. The
    /// default shows nothing and closes.
    fn chart(&mut self, spec: &crate::logic::info_screens::chart::ChartSpec) -> bool {
        let _ = spec;
        false
    }
    /// A custom modal dialog of the original that offers one choice out of several (option buttons or
    /// command buttons, e.g. the capital-contribution or loan-trade dialogs) → index into `options`, or
    /// `None` when cancelled. The default shows it as a report whose option lines carry `@C<index>`
    /// links, so every host that can show reports supports it.
    fn choose(&mut self, title: &PbStr, prompt: &[PbStr], options: &[PbStr]) -> Option<usize> {
        let mut lines: Vec<PbStr> = prompt.to_vec();
        lines.push(PbStr::new());
        for (k, o) in options.iter().enumerate() {
            lines.push(o.clone() + &PbStr::from(format!("{}@C{k:04}", " ".repeat(70)).as_str()));
        }
        match self.report(title, &lines, false) {
            ReportAction::Link(code) => code.to_display().get(2..)?.trim().parse().ok().filter(|&k: &usize| k < options.len()),
            _ => None,
        }
    }
}

/// Forwards UI requests while observing destruction of one enclosing modal window. This keeps
/// Windows' nested-dialog lifetime separate from the original's serialized open-counter globals.
pub struct DialogUi<'a> {
    inner: &'a mut dyn Ui,
    dialog: DialogKind,
    pub open: bool,
}

impl<'a> DialogUi<'a> {
    pub fn new(inner: &'a mut dyn Ui, dialog: DialogKind) -> Self {
        Self { inner, dialog, open: true }
    }
}

impl Ui for DialogUi<'_> {
    fn local_time(&self) -> crate::logic::newgame::LocalTime { self.inner.local_time() }
    fn write_file(&mut self, name: &PbStr, data: &[u8]) { self.inner.write_file(name, data); }
    fn end_dialog(&mut self, dialog: DialogKind) {
        if dialog == self.dialog { self.open = false; }
        self.inner.end_dialog(dialog);
    }
    fn random_remark(&mut self) -> PbStr { self.inner.random_remark() }
    fn high_score(&mut self, path: &PbStr, name: &PbStr, score: i64) -> i32 { self.inner.high_score(path, name, score) }
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32 { self.inner.msgbox(text, style, title) }
    fn inputbox(&mut self, prompt: &PbStr, title: &PbStr, default: &PbStr) -> PbStr { self.inner.inputbox(prompt, title, default) }
    fn form(&mut self, form: &Form) -> Option<FormResponse> { self.inner.form(form) }
    fn symbol_input(&mut self, prompt: &PbStr, title: &PbStr, default: &PbStr, companies: &[SymbolChoice]) -> PbStr { self.inner.symbol_input(prompt, title, default, companies) }
    fn report(&mut self, title: &PbStr, lines: &[PbStr], yes_no: bool) -> ReportAction { self.inner.report(title, lines, yes_no) }
    fn report_with_actions(&mut self, title: &PbStr, lines: &[PbStr], yes_no: bool, options: &ReportOptions) -> ReportAction { self.inner.report_with_actions(title, lines, yes_no, options) }
    fn news(&mut self, headline: &PbStr) { self.inner.news(headline); }
    fn ownership_diagram(&mut self, spec: &crate::logic::info_screens::ownership::OwnershipDiagram) -> ReportAction { self.inner.ownership_diagram(spec) }
    fn option_station(&mut self, spec: &crate::logic::trade_screens::option_station::OptionStation)
        -> Option<Vec<crate::logic::trade_screens::option_station::StationAction>> { self.inner.option_station(spec) }
    fn chart(&mut self, spec: &crate::logic::info_screens::chart::ChartSpec) -> bool { self.inner.chart(spec) }
    fn choose(&mut self, title: &PbStr, prompt: &[PbStr], options: &[PbStr]) -> Option<usize> { self.inner.choose(title, prompt, options) }
}

/// Headless UI for tests: answers prompts from a queue and records everything shown.
#[derive(Default)]
pub struct Scripted {
    pub answers: std::collections::VecDeque<PbStr>,
    pub log: Vec<String>,
}

impl Ui for Scripted {
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32 {
        self.log.push(format!("MSGBOX [{title}] {text}"));
        if style == 0 && title.is_empty() { return id::OK; } // timed qm.exe notice takes no queued answer
        match self.answers.pop_front().map(|a| a.to_display()) {
            Some(a) if a.eq_ignore_ascii_case("no") => id::NO,
            Some(a) if a.eq_ignore_ascii_case("cancel") => id::CANCEL,
            Some(_) => if style & 0xf == mb::YESNO || style & 0xf == mb::YESNOCANCEL { id::YES } else { id::OK },
            None => if style & 0xf == mb::YESNO || style & 0xf == mb::YESNOCANCEL { id::YES } else { id::OK },
        }
    }
    fn inputbox(&mut self, prompt: &PbStr, title: &PbStr, default: &PbStr) -> PbStr {
        self.log.push(format!("INPUTBOX [{title}] {prompt}"));
        self.answers.pop_front().unwrap_or_else(|| default.clone())
    }
    fn report(&mut self, title: &PbStr, lines: &[PbStr], yes_no: bool) -> ReportAction {
        self.log.push(format!("REPORT {title} ({} lines)", lines.len()));
        match self.answers.pop_front().map(|a| a.to_display()) {
            Some(a) if yes_no && a.eq_ignore_ascii_case("no") => ReportAction::No,
            Some(a) if a.starts_with("@C") || a.starts_with("@I") => ReportAction::Link(PbStr::from(a.as_str())),
            Some(a) if a.eq_ignore_ascii_case("close") => ReportAction::Closed,
            _ => ReportAction::Ok,
        }
    }
    fn news(&mut self, headline: &PbStr) {
        self.log.push(format!("NEWS {headline}"));
    }
    /// Pops an answer: an option index (`"2"`), or anything else (e.g. `"cancel"`) for cancel.
    fn choose(&mut self, title: &PbStr, _prompt: &[PbStr], options: &[PbStr]) -> Option<usize> {
        self.log.push(format!("CHOOSE [{title}] {} options", options.len()));
        self.answers.pop_front()?.to_display().trim().parse().ok().filter(|&k: &usize| k < options.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Answers every report by clicking its third link line.
    struct Clicker(Vec<PbStr>);

    impl Ui for Clicker {
        fn msgbox(&mut self, _: &PbStr, _: i32, _: &PbStr) -> i32 {
            id::OK
        }
        fn inputbox(&mut self, _: &PbStr, _: &PbStr, d: &PbStr) -> PbStr {
            d.clone()
        }
        fn report(&mut self, _: &PbStr, lines: &[PbStr], _: bool) -> ReportAction {
            self.0 = lines.to_vec();
            let l = lines.iter().filter(|l| l.to_display().contains("@C")).nth(2).unwrap().to_display();
            ReportAction::Link(PbStr::from(&l[l.find("@C").unwrap()..]))
        }
        fn news(&mut self, _: &PbStr) {}
    }

    #[test]
    fn choose_default_uses_report_links() {
        let opts: Vec<PbStr> = ["Cash", "Stock", "Bonds", "Assets"].iter().map(|s| PbStr::from(*s)).collect();
        let mut ui = Clicker(Vec::new());
        assert_eq!(ui.choose(&PbStr::from("T"), &[PbStr::from("Pick one")], &opts), Some(2));
        assert!(ui.0[4].to_display().starts_with("Bonds ") && ui.0[4].to_display().ends_with("@C0002"));
        let mut s = Scripted::default();
        s.answers.extend([PbStr::from("1"), PbStr::from("cancel"), PbStr::from("9")]);
        assert_eq!(s.choose(&PbStr::from("T"), &[], &opts), Some(1));
        assert_eq!(s.choose(&PbStr::from("T"), &[], &opts), None);
        assert_eq!(s.choose(&PbStr::from("T"), &[], &opts), None);
    }
}
