//! The application shell: menu dispatcher, action menu, trading desk, startup, ticker loop and settings menus.
//!
//! Hosts can open and dispatch action menus using [`action_menu`] and [`menu`], and dispatch menu-bar
//! commands through [`desk::desk_command`]. Use [`settings::menu_new_game`] to start a game,
//! [`app::save_game`] / [`app::load_game`] for save data, and [`app::ticker_step`] from the host's
//! event loop. [`Env`] supplies the clock and virtual files and records host requests.

// Statements keep the original BASIC shape on purpose.
#![allow(
    clippy::assign_op_pattern,
    clippy::manual_range_contains,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::too_many_arguments,
    clippy::if_same_then_else,
    clippy::needless_late_init,
    clippy::neg_multiply,
    clippy::manual_clamp
)]


pub mod menu;
pub mod credit;
pub mod action_menu;
pub mod desk;
pub mod settings;
pub mod app;
pub mod dialogs;
pub mod leftovers;
pub mod quotes;
pub mod startup;

use crate::logic::newgame::LocalTime;
use crate::pb::pbstr::PbStr;
use std::cell::RefCell;
use std::collections::BTreeMap;

/// The game's data directory as the shell sees it: files by upper-case base name (`DataPath` prefixes
/// and case don't matter), like the emulator harness's virtual filesystem. Hosts load the files the
/// original reads (`LASTGAME.DAT`, `GAMExx.DAT`, `cfig.wsr`, `GTIME.DAT`, ...) and persist what the shell
/// writes ([`Files::written`]).
#[derive(Clone, Debug, Default)]
pub struct Files {
    /// Current contents.
    pub files: BTreeMap<String, Vec<u8>>,
    /// Files written/created (`Some`) or deleted (`None`) since creation, by upper-case base name.
    pub written: BTreeMap<String, Option<Vec<u8>>>,
}

impl Files {
    /// Upper-case base name of a path (`C:\WSR\game01.dat` → `GAME01.DAT`).
    pub fn key(path: &[u8]) -> String {
        let base = path.rsplit(|&c| c == b'\\' || c == b'/').next().unwrap_or(path);
        String::from_utf8_lossy(base).to_ascii_uppercase()
    }
    /// The contents of `path`, if it exists.
    pub fn read(&self, path: &[u8]) -> Option<&Vec<u8>> {
        self.files.get(&Self::key(path))
    }
    /// `FileExists` / `DIR$` probe.
    pub fn exists(&self, path: &[u8]) -> bool {
        self.files.contains_key(&Self::key(path))
    }
    /// Writes (replaces) `path`.
    pub fn write(&mut self, path: &[u8], data: Vec<u8>) {
        let k = Self::key(path);
        self.written.insert(k.clone(), Some(data.clone()));
        self.files.insert(k, data);
    }
    /// `KILL path` (no error if missing).
    pub fn delete(&mut self, path: &[u8]) {
        let k = Self::key(path);
        if self.files.remove(&k).is_some() {
            self.written.insert(k, None);
        }
    }
}

/// Things the original does outside the game state that a host may want to act on (recorded in
/// [`Env::requests`] in order).
#[derive(Clone, Debug, PartialEq)]
pub enum HostRequest {
    /// `ShellExecute` of a URL or document (web pages, the manual, `WSRBOOK.HTM`).
    OpenUrl(PbStr),
    /// Starts an external program (`SHELL`, e.g. `WSRHELP.EXE`, `CUSTOMIZ.EXE`), with its command line.
    RunProgram(PbStr),
    /// The program ends (`MenuExitGame`, WinMain's exits).
    Exit,
}

/// The clock, files, host requests and the routines to skip (differential tests) of one call into the
/// shell.
///
/// `skip` lists original addresses whose calls are left out (like the harness's `--skip`); every
/// skipped call is recorded in `trace` (in call order), so a test can compare it with the original's
/// `SKIP Name` log lines.
pub struct Env<'a> {
    /// The local time (`TIMER`, `TIME$`, `DATE$`).
    pub now: &'a LocalTime,
    /// Browser safety repair for original bug 67 and invalid dividends in earlier saves.
    /// Disabled in compatibility hosts and original-executable differential tests.
    pub repair_initial_shares: bool,
    /// Game routines (addresses) skipped like the test harness's `--skip`.
    pub skip: &'a [u32],
    /// Skipped calls, in order.
    pub trace: RefCell<Vec<u32>>,
    /// The data directory.
    pub files: RefCell<Files>,
    /// Host requests, in order.
    pub requests: RefCell<Vec<HostRequest>>,
    /// Guarded operation failure, retained by browser hosts even for nested autosaves.
    pub error: RefCell<Option<String>>,
}

impl<'a> Env<'a> {
    /// No skips, no files.
    pub fn new(now: &'a LocalTime) -> Self {
        Self::skipping(now, &[])
    }

    /// Skipping the routines `skip`.
    pub fn skipping(now: &'a LocalTime, skip: &'a [u32]) -> Self {
        Env {
            now,
            repair_initial_shares: false,
            skip,
            trace: RefCell::new(Vec::new()),
            files: RefCell::new(Files::default()),
            requests: RefCell::new(Vec::new()),
            error: RefCell::new(None),
        }
    }

    /// With the data directory `files`.
    pub fn with_files(mut self, files: Files) -> Self {
        self.files = RefCell::new(files);
        self
    }

    /// True if the routine at `addr` is to be skipped (the call is then recorded in `trace`).
    pub fn skips(&self, addr: u32) -> bool {
        let s = self.skip.contains(&addr);
        if s {
            self.trace.borrow_mut().push(addr);
        }
        s
    }

    /// The skipped calls so far.
    pub fn skipped(&self) -> Vec<u32> {
        self.trace.borrow().clone()
    }

    /// Records a host request.
    pub fn request(&self, r: HostRequest) {
        self.requests.borrow_mut().push(r);
    }
}

/// Common imports and helpers of the submodules.
#[allow(unused_imports)]
pub(crate) mod prelude {
use crate::pb::real::{Ext, cast};

    pub(crate) use super::{Env, Files, HostRequest};
    pub(crate) use crate::logic::newgame::LocalTime;
    pub(crate) use crate::logic::news::post_news;
    pub(crate) use crate::logic::report::{show_quick_message, show_text_report};
    pub(crate) use crate::pb::numeric::format;
    pub(crate) use crate::pb::numeric::{str_num, val};
    pub(crate) use crate::pb::pbstr::PbStr;
    pub(crate) use crate::pb::rng::Rng;
    pub(crate) use crate::pb::string::{
        instr, instr_any, lcase, left, len, ltrim, mcase, mid, parse_any, remove, remove_any, replace, right, rtrim,
        space, string_fill, ucase, CaseMode,
    };
    pub(crate) use crate::pb::numeric::{ceil, frac, int, to_integer, to_long, to_quad};
    pub(crate) use crate::state::Globals;
    pub(crate) use crate::ui::{id, mb, ReportAction, Ui};

    /// A string literal as [`PbStr`].
    #[allow(dead_code)]
    pub(crate) fn s(x: &str) -> PbStr {
        PbStr::from(x)
    }

    /// A SINGLE constant of the original's pool (`fmul dword [..]`): the f32 value, widened.
    #[allow(dead_code)]
    pub(crate) fn sf(x: f32) -> Ext {
        cast::<Ext, _>(x)
    }

    /// `RTRIM$(LTRIM$(s))`.
    #[allow(dead_code)]
    pub(crate) fn trim(x: &[u8]) -> PbStr {
        rtrim(ltrim(x).as_bytes())
    }

    /// `STR$(x)` of a LONG/QUAD/EXT value (15 digits).
    #[allow(dead_code)]
    pub(crate) fn str_s(x: Ext) -> PbStr {
        PbStr::from(str_num(x, 15).as_str())
    }

    /// `RTRIM$(LTRIM$(STR$(x)))`.
    #[allow(dead_code)]
    pub(crate) fn str_trim(x: Ext) -> PbStr {
        trim(str_num(x, 15).as_bytes())
    }

    /// `FORMAT$(x, mask)`.
    #[allow(dead_code)]
    pub(crate) fn fmt(x: Ext, mask: &str) -> PbStr {
        PbStr(format(x, mask.as_bytes()))
    }

    /// `UCASE$(x)`.
    #[allow(dead_code)]
    pub(crate) fn uc(x: &PbStr) -> PbStr {
        ucase(x.as_bytes(), CaseMode::Ansi)
    }

    /// `x = "literal"`.
    #[allow(dead_code)]
    pub(crate) fn set(x: &mut PbStr, lit: &str) {
        *x = PbStr::from(lit);
    }

    /// `TIMER`: seconds since midnight.
    #[allow(dead_code)]
    pub(crate) fn timer(now: &LocalTime) -> Ext {
        Ext::from(crate::pb::rng::timer_f64(now.timer_ms()))
    }

    /// `TIME$`: `"hh:mm:ss"`.
    #[allow(dead_code)]
    pub(crate) fn time_str(now: &LocalTime) -> PbStr {
        PbStr::from(format!("{:02}:{:02}:{:02}", now.hour, now.minute, now.second).as_str())
    }

    /// `DATE$`: `"mm-dd-yyyy"`.
    #[allow(dead_code)]
    pub(crate) fn date_str(now: &LocalTime) -> PbStr {
        PbStr::from(format!("{:02}-{:02}-{:04}", now.month, now.day, now.year).as_str())
    }
}
