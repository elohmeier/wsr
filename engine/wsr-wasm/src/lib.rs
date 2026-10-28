//! Browser binding for `wsr-core`.
//!
//! Plain C-ABI exports, no `wasm-bindgen`: the page (or Web Worker) copies bytes into wasm memory with
//! [`wsr_alloc`], calls an export, and reads the result buffer (`[len: u32 LE][bytes]`), then frees it with
//! [`wsr_free`]. Results are JSON. The engine instance lives in this module (one game per wasm instance).
use wsr_core::pb::real::{Ext, cast};


use std::cell::RefCell;
use wsr_core::logic::report;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::save;
use wsr_core::state::Globals;
use wsr_core::ui::Ui;

pub mod summary;
pub mod observer;
pub mod presentation;
pub mod session;
#[cfg(target_arch = "wasm32")]
pub mod host;

thread_local! {
    static CONTENT: RefCell<Option<std::sync::Arc<wsr_core::content::ContentPack>>> = const { RefCell::new(None) };
    static GAME: RefCell<Option<Globals>> = const { RefCell::new(None) };
    static RNG: RefCell<Rng> = RefCell::new(Rng::default());
    static FILES: RefCell<wsr_core::logic::shell::Files> = RefCell::new(wsr_core::logic::shell::Files::default());
    static REMARKS: RefCell<wsr_core::logic::extensions::remarks::Remarks> = RefCell::new(Default::default());
    static HIGH_SCORES: RefCell<wsr_core::logic::extensions::high_score::HighScores> = RefCell::new(Default::default());
}

/// Allocate `len` bytes in wasm memory for the host to write into.
#[unsafe(no_mangle)]
pub extern "C" fn wsr_alloc(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len.max(1));
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// Free a buffer from [`wsr_alloc`] or a result buffer (pass the capacity/length you got).
///
/// # Safety
/// `ptr`/`cap` must come from this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wsr_free(ptr: *mut u8, cap: usize) {
    if !ptr.is_null() {
        unsafe { drop(Vec::from_raw_parts(ptr, 0, cap.max(1))) };
    }
}

/// Package bytes as a result buffer `[len: u32][bytes]`; the host frees it with `wsr_free(ptr, len + 4)`.
fn result(bytes: &[u8]) -> *mut u8 {
    let mut v = Vec::with_capacity(bytes.len() + 4);
    v.extend_from_slice(&(cast::<u32, _>(bytes.len())).to_le_bytes());
    v.extend_from_slice(bytes);
    let mut v = std::mem::ManuallyDrop::new(v.into_boxed_slice());
    v.as_mut_ptr()
}

/// Load a `GAMExx.DAT` save. `fixed != 0` uses the bug-fixed decoder (BUGS.md #1).
/// Returns JSON: `{"ok":true,"summary":{…}}` or `{"ok":false,"error":"…"}`.
///
/// # Safety
/// `ptr`/`len` must describe a readable buffer in wasm memory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wsr_load_save(ptr: *const u8, len: usize, fixed: u32) -> *mut u8 {
    let data = unsafe { std::slice::from_raw_parts(ptr, len) };
    result(load_save_json(data, fixed != 0).as_bytes())
}

/// Load `CORPNAME.DAT` into the current game (needed when the save asks for it).
///
/// # Safety
/// `ptr`/`len` must describe a readable buffer in wasm memory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wsr_load_corp_names(ptr: *const u8, len: usize) -> *mut u8 {
    let data = unsafe { std::slice::from_raw_parts(ptr, len) };
    let out = GAME.with(|g| match g.borrow_mut().as_mut() {
        Some(g) => match save::load_corp_names(g, data) {
            Ok(()) => format!("{{\"ok\":true,\"summary\":{}}}", summary::summary_json(g)),
            Err(e) => format!("{{\"ok\":false,\"error\":{}}}", summary::json_str(&format!("{e:?}"))),
        },
        None => "{\"ok\":false,\"error\":\"no game loaded\"}".to_string(),
    });
    result(out.as_bytes())
}

/// Save the current game; returns the encoded `GAMExx.DAT` bytes (not JSON).
#[unsafe(no_mangle)]
pub extern "C" fn wsr_save() -> *mut u8 {
    let bytes = GAME.with(|g| {
        RNG.with(|r| {
            g.borrow_mut()
                .as_mut()
                .map(|g| save::write_save_file(g, &mut RecordingUi::default(), &mut r.borrow_mut()))
                .unwrap_or_default()
        })
    });
    result(&bytes)
}

/// Library entry used by the export above and by native tools.
pub fn load_save_json(data: &[u8], fixed: bool) -> String {
    let mut g = session::fresh_game(&wsr_core::logic::newgame::LocalTime::HARNESS, &mut Rng::default());
    let mut ui = RecordingUi::default();
    let mut rng = RNG.with(|r| *r.borrow());
    let res = save::read_save_file(&mut g, &mut ui, &mut rng, data, fixed);
    match res {
        Ok(info) => {
            let notices: Vec<String> = info.notices.iter().map(|n| summary::json_str(&n.to_string())).collect();
            let s = format!(
                "{{\"ok\":true,\"version\":{},\"needsCorpNames\":{},\"notices\":[{}],\"summary\":{}}}",
                info.version,
                info.needs_corp_names,
                notices.join(","),
                summary::summary_json(&g)
            );
            GAME.with(|cell| *cell.borrow_mut() = Some(g));
            RNG.with(|r| *r.borrow_mut() = rng);
            s
        }
        Err(e) => format!("{{\"ok\":false,\"error\":{}}}", summary::json_str(&format!("{e:?}"))),
    }
}

/// Records what the engine shows; prompts get the default answer (OK/YES, default text).
#[derive(Default)]
pub struct RecordingUi {
    pub title: String,
    pub lines: Vec<String>,
    pub messages: Vec<String>,
    pub news: Vec<String>,
}

impl Ui for RecordingUi {
    fn msgbox(&mut self, text: &PbStr, style: i32, _title: &PbStr) -> i32 {
        self.messages.push(text.to_display());
        if style & 0xf == wsr_core::ui::mb::YESNO { wsr_core::ui::id::YES } else { wsr_core::ui::id::OK }
    }
    fn inputbox(&mut self, prompt: &PbStr, _title: &PbStr, default: &PbStr) -> PbStr {
        self.messages.push(prompt.to_display());
        default.clone()
    }
    fn report(&mut self, title: &PbStr, lines: &[PbStr], _yes_no: bool) -> wsr_core::ui::ReportAction {
        self.title = title.to_display();
        self.lines = lines.iter().map(|l| l.to_display()).collect();
        wsr_core::ui::ReportAction::Ok
    }
    fn news(&mut self, headline: &PbStr) {
        self.news.push(headline.to_display());
    }
}

/// Report kinds for [`wsr_report`].
pub const REPORT_SHAREHOLDERS: u32 = 1;
pub const REPORT_INTEREST_RATES: u32 = 2;
pub const REPORT_ECONOMIC_STATS: u32 = 3;
pub const REPORT_BALANCE_SHEET: u32 = 4;
pub const REPORT_EARNINGS: u32 = 5;
pub const REPORT_RESEARCH: u32 = 6;
pub const REPORT_PORTFOLIO: u32 = 7;
pub const REPORT_PERSONAL_STATEMENT: u32 = 8;
pub const REPORT_TOP_COMPANIES: u32 = 9;
pub const REPORT_INDUSTRY: u32 = 10;
pub const REPORT_STANDINGS: u32 = 11;
pub const REPORT_CASH_FLOW: u32 = 12;
pub const REPORT_INDUSTRY_PROJECTIONS: u32 = 13;

/// Run one of the ported report screens: `entity` is the company (or player, industry, list type).
pub fn run_report(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, kind: u32, entity: i32) {
    use wsr_core::logic::{cashflow, reports_corp, reports_lists, research};
    match kind {
        REPORT_SHAREHOLDERS => report::show_shareholder_list(g, ui, rng, entity, 0),
        REPORT_INTEREST_RATES => report::show_interest_rates(g, ui, rng),
        REPORT_ECONOMIC_STATS => report::show_economic_stats(g, ui, rng),
        REPORT_BALANCE_SHEET => reports_corp::show_balance_sheet(g, ui, rng, entity),
        REPORT_EARNINGS => reports_corp::show_earnings_report(g, ui, rng, entity),
        REPORT_RESEARCH => research::show_research_report(g, ui, rng, entity),
        REPORT_PORTFOLIO => {
            g.tmp_ext1 = Ext::from_f64(3.0); // stocks and bonds
            reports_lists::show_portfolio_listing(g, ui, rng, entity, 0, 1)
        }
        REPORT_PERSONAL_STATEMENT => {
            reports_lists::show_personal_financial_statement(g, ui, rng, 0)
        }
        REPORT_TOP_COMPANIES => reports_lists::show_top_companies_list(g, ui, rng, entity),
        REPORT_INDUSTRY => reports_lists::show_industry_report(g, ui, rng, entity),
        REPORT_STANDINGS => reports_lists::show_player_standings(g, ui, rng),
        REPORT_CASH_FLOW => {
            cashflow::show_cash_flow_projection(g, ui, rng, entity, 0);
        }
        REPORT_INDUSTRY_PROJECTIONS => reports_lists::show_industry_growth_projections(g, ui, rng),
        _ => {}
    }
}

/// Run one of the ported report screens on the loaded game. Returns JSON
/// `{"ok":true,"title":…,"lines":[{"text":…,"link":…}],"messages":[…]}`; `link` is `"C<id>"`/`"I<id>"` from the
/// original's `@C`/`@I` hyperlink codes at the end of a line, or null.
#[unsafe(no_mangle)]
pub extern "C" fn wsr_report(kind: u32, entity: i32) -> *mut u8 {
    result(report_json(kind, entity).as_bytes())
}

pub fn report_json(kind: u32, entity: i32) -> String {
    GAME.with(|cell| {
        let mut cell = cell.borrow_mut();
        let Some(g) = cell.as_mut() else { return "{\"ok\":false,\"error\":\"no game loaded\"}".to_string() };
        let mut ui = RecordingUi::default();
        RNG.with(|r| {
            let rng = &mut *r.borrow_mut();
            run_report(g, &mut ui, rng, kind, entity);
        });
        let lines: Vec<String> = ui
            .lines
            .iter()
            .map(|l| {
                let (text, link) = split_link(l);
                format!("{{\"text\":{},\"link\":{}}}", summary::json_str(text),
                        link.map(|k| summary::json_str(&k)).unwrap_or_else(|| "null".into()))
            })
            .collect();
        let msgs: Vec<String> = ui.messages.iter().map(|m| summary::json_str(m)).collect();
        format!("{{\"ok\":true,\"title\":{},\"lines\":[{}],\"messages\":[{}]}}", summary::json_str(ui.title.trim()),
                lines.join(","), msgs.join(","))
    })
}

/// Split a trailing `@Cnnnn` / `@Innnn` link code off a report line.
fn split_link(line: &str) -> (&str, Option<String>) {
    if let Some(p) = line.rfind('@') {
        let code = &line[p + 1..];
        if code.len() >= 2 && (code.starts_with('C') || code.starts_with('I')) && code[1..].trim().chars().all(|c| c.is_ascii_digit()) {
            let id: i32 = code[1..].trim().parse().unwrap_or(0);
            return (line[..p].trim_end(), Some(format!("{}{}", &code[..1], id)));
        }
    }
    (line, None)
}

/// Interactive version of [`wsr_report`]: shows the report (and any MSGBOX/INPUTBOX on the way) through the
/// host page via the blocking worker bridge, and applies the user's answer (YES/NO, clicked link) to the
/// game state like the original's report dialog. Returns `{"ok":true}` or an error.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn wsr_show_report(kind: u32, entity: i32) -> *mut u8 {
    let out = GAME.with(|cell| {
        let mut cell = cell.borrow_mut();
        let Some(g) = cell.as_mut() else { return "{\"ok\":false,\"error\":\"no game loaded\"}".to_string() };
        let mut ui = host::HostUi;
        RNG.with(|r| {
            let rng = &mut *r.borrow_mut();
            run_report(g, &mut ui, rng, kind, entity);
        });
        "{\"ok\":true}".to_string()
    });
    result(out.as_bytes())
}

#[cfg(test)]
pub(crate) fn install_test_content() {
    use wsr_core::content::{ContentPack, TextId, CONTENT_FILE};
    let entries = TextId::identifiers().enumerate().map(|(i, id)| (id,
        id.binary_length().map(|n| PbStr(vec![0; n])).unwrap_or_else(|| PbStr::from(format!("Synthetic resource {i:06}                         ").as_str()))));
    session::put_file(CONTENT_FILE.as_bytes(), &ContentPack::from_entries(entries).unwrap().to_bytes()).unwrap();
}
