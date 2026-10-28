//!  [runtime resource wording] (`ShowAdvancedOptionsStation` 0x60b44b): an order sheet of eight
//! option legs (rows 1-4 puts, 5-8 calls) on one stock, priced and executed at once
//! (`ProcessOptionOrderSheet` 0x608b95), filled automatically from ten strategy templates
//! (`AutoBuildOptionStrategy` 0x613212 with the picker `ShowOptionStrategyPicker` 0x6108b7).
//!
//! * [`station`]: the station dialog — entry checks, its dialog procedure and button callbacks
//!   (`AdvOpt_*`), and the interaction loop.
//! * [`order_sheet`]: `ProcessOptionOrderSheet` and its GOSUBs, `ClearOptionGridFields` (0x6130f7).
//! * [`strategy`]: `AutoBuildOptionStrategy`, the strategy picker and its callbacks (`OptStrat_*`).
//!
//! The original keeps the order sheet in the dialog's text boxes (control ids `1320 + row` buy/sell,
//! `1330 + row` strike, `1340 + row` expiry month/year, `1350 + row` size in %, `1360 + row` the computed
//! option price, `1436` the status line). The port keeps them in [`OptionGrid`] inside an
//! [`OptionStation`], which a host UI shows and edits directly ([`OptionGrid::set_text`] /
//! [`OptionStation::click`] / [`OptionStation::run`]). [`show_advanced_options_station`] drives the
//! station through [`crate::ui::Ui`]. Browser hosts edit the grid directly; headless hosts fall back to
//! `choose` for the buttons and `inputbox` to edit a line.

// Keep the original's statement shapes.
#![allow(
    clippy::assign_op_pattern,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::manual_range_contains
)]
use crate::pb::real::cast;


pub mod order_sheet;
pub mod station;
pub mod strategy;

pub use order_sheet::{clear_option_grid_fields, process_option_order_sheet};
pub use station::{show_advanced_options_station, show_advanced_options_station_with, StationAction};
pub use strategy::{auto_build_option_strategy, show_option_strategy_picker, STRATEGY_NAMES};

use crate::logic::newgame::LocalTime;
use crate::pb::pbstr::PbStr;
use std::cell::RefCell;

/// Number of order lines (rows 1-4: puts, 5-8: calls).
pub const ROWS: usize = 8;

/// Control ids of the order sheet columns: `base + row` (row 1..8).
pub mod ctl {

    /// "Buy or Sell: B or S".
    pub const SIDE: i32 = 1320;
    /// "Strike Price".
    pub const STRIKE: i32 = 1330;
    /// "Exp. Date" (month/year).
    pub const EXP: i32 = 1340;
    /// " [runtime resource wording] ".
    pub const QTY: i32 = 1350;
    /// "Option Price" (per share; written by the station).
    pub const PRICE: i32 = 1360;
    /// The status line below the buttons.
    pub const STATUS: i32 = 1436;
    /// Buttons.
    pub const CLEAR_ALL: i32 = 1372;
    pub const HELP: i32 = 1376;
    pub const CALC: i32 = 1;
    pub const SIZE_LIMITS: i32 = 1381;
    pub const AUTO_TRADE: i32 = 1382;
    pub const LIST_OPTIONS: i32 = 1379;
    pub const CLOSE_ALL_OPTIONS: i32 = 1380;
    pub const SUBMIT: i32 = 1374;
    pub const CLOSE: i32 = 1378;
}

/// The order sheet: the text of every grid text box (index = row - 1).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OptionGrid {
    /// "B" (buy) or "S" (sell short).
    pub side: [PbStr; ROWS],
    /// Strike price per share.
    pub strike: [PbStr; ROWS],
    /// Expiry as "month/year" (e.g. "01/2032" or "1/32").
    pub exp: [PbStr; ROWS],
    /// Size of the contract in % of the issued shares (1..10).
    pub qty: [PbStr; ROWS],
    /// Option price per share (output of CALC/SUBMIT).
    pub price: [PbStr; ROWS],
}

impl OptionGrid {
    /// The text box of control `id` (`1321..=1368` without the `..9`/`..0` ids), if it is a grid box.
    pub fn field(&self, id: i32) -> Option<&PbStr> {
        let (col, row) = Self::split(id)?;
        Some(&[&self.side, &self.strike, &self.exp, &self.qty, &self.price][col][row])
    }

    /// Mutable text box of control `id`.
    pub fn field_mut(&mut self, id: i32) -> Option<&mut PbStr> {
        let (col, row) = Self::split(id)?;
        Some(match col {
            0 => &mut self.side[row],
            1 => &mut self.strike[row],
            2 => &mut self.exp[row],
            3 => &mut self.qty[row],
            _ => &mut self.price[row],
        })
    }

    /// `CONTROL GET TEXT` of grid box `id` ("" for other ids).
    pub fn text(&self, id: i32) -> PbStr {
        self.field(id).cloned().unwrap_or_default()
    }

    /// `CONTROL SET TEXT` of grid box `id` (other ids are ignored).
    pub fn set_text(&mut self, id: i32, text: &PbStr) {
        if let Some(f) = self.field_mut(id) {
            *f = text.clone();
        }
    }

    fn split(id: i32) -> Option<(usize, usize)> {
        if !(1321..=1368).contains(&id) {
            return None;
        }
        let row = id % 10;
        if !(1..=8).contains(&row) {
            return None;
        }
        Some((cast::<usize, _>((id - 1320) / 10), cast::<usize, _>(row - 1)))
    }
}

/// One ope [runtime resource wording] dialog (`hDlgOptions`).
#[derive(Clone, Debug, Default)]
pub struct OptionStation {
    /// The stock whose options are traded (`stockId` of ShowAdvancedOptionsStation).
    pub stock_id: i32,
    /// Dialog title: " [runtime resource wording] <ActiveEntity symbol>".
    pub title: PbStr,
    /// Label 1307: "Stock / Price: <name> / <price>".
    pub stock_label: PbStr,
    /// Column header 1313: "Strike Price: (<min> to <max>)".
    pub strike_header: PbStr,
    /// Column header 1316: "Exp. Date: <mm/yyyy> to <mm/yyyy>".
    pub exp_header: PbStr,
    /// Caption of button 1380: "CLOSE ALL <symbol> O&PTIONS".
    pub close_all_caption: PbStr,
    /// The order sheet.
    pub grid: OptionGrid,
    /// Status line 1436 ("Strategy selected: ...").
    pub status: PbStr,
    /// False once the dialog has ended (`DIALOG END hDlgOptions`).
    pub open: bool,
}

/// The clock and the routines to skip (differential tests) of one station session.
///
/// `skip` lists original addresses whose calls are left out (like the harness's `--skip`); every
/// skipped call is recorded in `trace`, in call order.
pub struct StationEnv<'a> {
    /// The local time (`TIMER`, `TIME$`) for `TickerSimSteps`.
    pub now: &'a LocalTime,
    /// Game routines (addresses) skipped like the test harness's `--skip`.
    pub skip: &'a [u32],
    /// Skipped calls, in order.
    pub trace: RefCell<Vec<u32>>,
}

impl<'a> StationEnv<'a> {
    /// No skips.
    pub fn new(now: &'a LocalTime) -> Self {
        StationEnv { now, skip: &[], trace: RefCell::new(Vec::new()) }
    }

    /// Skipping the routines `skip`.
    pub fn skipping(now: &'a LocalTime, skip: &'a [u32]) -> Self {
        StationEnv { now, skip, trace: RefCell::new(Vec::new()) }
    }

    /// True if the routine at `addr` is to be skipped (the call is then recorded in `trace`).
    pub fn skips(&self, addr: u32) -> bool {
        let s = self.skip.contains(&addr);
        if s {
            self.trace.borrow_mut().push(addr);
        }
        s
    }
}
