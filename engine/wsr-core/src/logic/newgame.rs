//! The new-game wizard and world builder: `NewGameSetup` (0x473e5b) with its GOSUBs, plus the dialogs it
//! runs (`InBox` mode 2 for the number of players, `ShowStartupDialog` with `Startup_OkBtn`) and the
//! `CreateIndustryCombo` list it fills.
//!
//! Numeric conventions as in `logic::valuation`: `EXT` uses software x87 arithmetic, SINGLE constants of the machine code are
//! written `s(0.8)`, the association order of float expressions follows the x87 code.
//!
//! Host inputs: the data files come from `files` (base name as the original opens it after `DataPath`,
//! e.g. `"WSR101.DAT"`; `None` = the file does not exist); the local time from [`LocalTime`] (the original
//! reads `TIMER`, `TIME$` and `DATE$`).

// Statements keep the original BASIC shape on purpose.
#![allow(
    clippy::manual_range_contains,
    clippy::assign_op_pattern,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::too_many_lines,
    clippy::needless_range_loop,
    clippy::if_same_then_else,
    clippy::neg_multiply,
    clippy::collapsible_match
)]
use crate::pb::real::{Ext, cast};


use crate::logic::control::update_corp_control;
use crate::logic::economy::{update_crypto_prices, update_economy, update_interest_rates};
use crate::logic::finance::{calc_entity_financials, calc_gov_bond_yields};
use crate::logic::futures::init_commodity_markets;
use crate::logic::holdings::pack_holding_arrays_with_news as pack_holding_arrays;
use crate::logic::market::{refresh_selection_labels, show_next_earnings_date};
use crate::logic::newgame_helpers::{
    apply_famous_corp_tweaks, init_convertible_bond_issues, init_earnings_history, init_etfs,
    load_country_names, save_config,
};
use crate::logic::news::post_news;
use crate::logic::report::show_quick_message;
use crate::logic::valuation::{calc_dividend_yield, calc_stock_price, check_stock_split};
use crate::pb::numeric::format;
use crate::pb::numeric::{InputReader, str_num, val};
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{
    compare, instr, len, ltrim, mid, mid_assign, remove_any, replace, replace_any, right, rtrim,
    space,
};
use crate::pb::numeric::{Cur, int, to_integer, to_long, to_quad};
use crate::state::Globals;
use crate::ui::Ui;

/// A SINGLE constant of the original (`fld/fmul dword [const]`) as the x87 sees it.
#[inline]
fn s(x: f32) -> Ext {
    cast::<Ext, _>(x)
}

fn st(x: &str) -> PbStr {
    PbStr::from(x)
}

/// `RTRIM$(LTRIM$(x))`.
fn trim(x: &[u8]) -> PbStr {
    rtrim(ltrim(x).as_bytes())
}

/// `STR$(x)` of a LONG / QUAD / EXT (PB_StrNum, 15 digits).
fn str15(x: Ext) -> PbStr {
    PbStr::from(str_num(x, 15).as_str())
}

/// `STR$(x)` of a SINGLE / INTEGER (PB_StrNum7).
fn str7(x: Ext) -> PbStr {
    PbStr::from(str_num(x, 7).as_str())
}

fn fmt(x: Ext, mask: &str) -> PbStr {
    PbStr(format(x, mask.as_bytes()))
}

/// The local time the original reads through `TIMER`, `TIME$` and `DATE$` (GetLocalTime).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LocalTime {
    pub year: i32,
    pub month: i32,
    pub day: i32,
    pub hour: i32,
    pub minute: i32,
    pub second: i32,
    pub millis: i32,
}

impl LocalTime {
    /// The fixed time of the emulator harness (`tools/emu/game.py`): Mon 2000-01-03 10:00:00.000.
    pub const HARNESS: LocalTime = LocalTime {
        year: 2000,
        month: 1,
        day: 3,
        hour: 10,
        minute: 0,
        second: 0,
        millis: 0,
    };

    /// Milliseconds since midnight (what `TIMER` divides by 1000).
    pub fn timer_ms(&self) -> u32 {
        cast::<u32, _>(self.hour * 3_600_000 + self.minute * 60_000 + self.second * 1000 + self.millis)
    }

    /// `VAL(RIGHT$(TIME$, 2))`: the seconds.
    pub fn time_secs(&self) -> i32 {
        self.second
    }

    /// `VAL(RIGHT$(DATE$, 4))`: the year.
    pub fn date_year(&self) -> i32 {
        self.year
    }
}

/// The locals of NewGameSetup that its GOSUBs share (PB GOSUBs run on the caller's frame).
#[derive(Default)]
struct Frame {
    reply: i32,
    human_count: i32,
    max_humans: i32,
    answer: PbStr,
    year_text: PbStr,
    /// `assetBase` (EXT at ebp-0x12c): asset base of the company being parsed / balance-sheet total.
    asset_base: Ext,
    magnitude: f32,
    coupon_rate: f32,
    rate_input: f32,
    pe_ratio: f32,
    maturity_year: f32,
    is_holding_line: f32,
    industry: i32,
    rating_str: PbStr,
    holding_idx: i32,
    min_rate: f32,
    max_rate: f32,
    max_rate_text: PbStr,
    min_rate_text: PbStr,
    rate_text: PbStr,
    msg_text: PbStr,
    new_year_text: PbStr,
    field_text: PbStr,
    line_text: PbStr,
    j: i32,
    amount: i64,
    rand_val: i64,
    tmp_val: i32,
    pos: i32,
    hold_idx: i32,
    corp_id: i32,
    tmp_long: i64,
    net_equity: Ext,
    cust_deposits: Ext,
    total_funding: Ext,
    sub_amount: i64,
    div_yield: Ext,
    held_corp_id: i32,
    line_len: i32,
}

/// Everything the routine and its GOSUBs work on.
struct Ng<'a> {
    g: &'a mut Globals,
    ui: &'a mut dyn Ui,
    rng: &'a mut Rng,
    now: LocalTime,
    options: NewGameOptions,
    f: Frame,
    /// Files written (SaveConfig).
    written: Vec<(String, Vec<u8>)>,
}

/// Opt-in repairs of original game bugs; compatibility callers retain the original setup.
#[derive(Clone, Copy, Debug, Default)]
pub struct NewGameOptions {
    /// Correct bug 67's comparison-as-assignment before dividends divide by the share count.
    pub repair_initial_shares: bool,
}

/// Port of NewGameSetup (0x473e5b).
/// The new-game wizard and world builder: asks whether some players are computer players and the number of
/// players (`InBox` dialog) and human players (`INPUTBOX$`), configures the currency of `CurrencyCode`
/// (symbol, name, exchange rate — optionally entered), posts the currency news, runs the startup dialog
/// (player names, starting money, game length, difficulty), then builds the world: interest rates, gov
/// bonds, GDP, oil, housing, commodities, the industries and companies of `WSR101.DAT` with their balance
/// sheets and holdings ([`NewGame_FinalizeWorld`](Ng::finalize_world)), the ETFs, the turn order
/// (and a first computer turn), and finally values every company, sets holding costs and control years,
/// the earnings history and convertible bonds.
///
/// `files` supplies `WSR101.DAT` (missing file: the original's ON ERROR handler shows a runtime error and
/// the setup ends). `now` is the local time (`RANDOMIZE TIMER`, `TIME$`, `DATE$`).
///
/// Dialogs: the number-of-players dialog (`InBox` mode 2) is asked through [`Ui::inputbox`] (prompt =
/// `InputDlgPrompt` + "Click 'OK' to enter.", title = `InputDlgLabel`, default = the current number);
/// an empty answer is the dialog's Cancel button. The startup dialog: see [`show_startup_dialog`].
///
/// Returns the files the routine wrote: `("cfig.wsr", contents)` from SaveConfig when the startup dialog
/// was completed.
///
/// When a computer player moves first, its turn runs (`AiComputerPlayerTurn`). Pure window/menu
/// operations are skipped.
///
/// Runtime errors end the routine through its ON ERROR handler (ShowRuntimeError, not ported): the port
/// returns where the original would raise one (missing data file, end of file before `@END`).
pub fn new_game_setup(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    files: &dyn Fn(&str) -> Option<Vec<u8>>,
    now: &LocalTime,
) -> Vec<(String, Vec<u8>)> {
    new_game_setup_with_options(g, ui, rng, files, now, NewGameOptions::default())
}

/// Build a world with the requested original-bug repairs.
pub fn new_game_setup_with_options(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    files: &dyn Fn(&str) -> Option<Vec<u8>>,
    now: &LocalTime,
    options: NewGameOptions,
) -> Vec<(String, Vec<u8>)> {
    let mut ng = Ng {
        g,
        ui,
        rng,
        now: *now,
        options,
        f: Frame::default(),
        written: Vec::new(),
    };
    ng.run(files);
    ng.written
}

impl Ng<'_> {
    fn post(&mut self) {
        post_news(self.g, self.ui, 0, 0);
    }

    fn msgbox(&mut self, text: &PbStr, style: i32, title: &str) -> i32 {
        self.ui.msgbox(text, style, &st(title))
    }

    fn run(&mut self, files: &dyn Fn(&str) -> Option<Vec<u8>>) {
    let __content = crate::content::current();
        // ON ERROR GOTO 0x47c1fa: ShowRuntimeError("NewGameSetup", ...) (closes the data file on error 62), exit.
        let title = self.g.app_title.clone() + __content.string(6629);
        let text = st(__content.string(6630))
            + __content.string(6631)
            + "\r"
            + "\r"
            + __content.string(6632);
        self.f.reply = self.ui.msgbox(&text, 8227, &title);
        match self.f.reply {
            6 => self.g.has_cpu_players = st("Y"),
            7 => self.g.has_cpu_players = st("N"),
            2 => return, // NewGameSetup_Exit
            _ => {}
        }
        loop {
            self.g.action_code = st("GETDATA");
            self.f.field_text = st(__content.string(6633));
            if self.g.has_cpu_players == "Y" {
                self.f.field_text =
                    self.f.field_text.clone() + "\r\n" + "\r\n" + __content.string(6634);
            }
            self.f.field_text = self.f.field_text.clone() + "\r\n" + "\r\n";
            self.g.input_dlg_label = st(__content.string(6635));
            self.g.input_dlg_prompt = self.f.field_text.clone();
            in_box_num_players(self.g, self.ui);
            if self.g.symbol_dlg_text == "Cancel" {
                self.g.has_cpu_players = PbStr::new();
                return;
            }
            if !(self.g.num_players < 2 || 5 < self.g.num_players) {
                break;
            }
        }
        if self.g.has_cpu_players == "Y" {
            self.g.cpu_max_corps = 80;
            self.g.is_computer_player[1] = 1;
        }
        if self.g.has_cpu_players == "Y" && 2 < self.g.num_players {
            self.f.max_humans = self.g.num_players - 1;
            loop {
                let prompt = st(__content.string(6636))
                    + &str15(cast::<Ext, _>(self.f.max_humans))
                    + ")";
                self.f.answer = self
                    .ui
                    .inputbox(&prompt, &st(__content.string(6637)), &st("1"));
                self.f.human_count =
                    to_long(val(remove_any(self.f.answer.as_bytes(), b"$ ,%").as_bytes()));
                if self.f.human_count == 0 {
                    self.g.has_cpu_players = PbStr::new();
                    return;
                }
                if !(self.f.human_count < 1 || self.f.max_humans < self.f.human_count) {
                    break;
                }
            }
            let hc = self.f.human_count;
            match self.g.num_players {
                3 => {
                    if hc == 1 {
                        self.g.is_computer_player[3] = 1;
                        self.g.cpu_max_corps = 60;
                    }
                }
                4 => {
                    if hc == 1 {
                        self.g.is_computer_player[3] = 1;
                        self.g.is_computer_player[4] = 1;
                        self.g.cpu_max_corps = 40;
                    } else if hc == 2 {
                        self.g.is_computer_player[3] = 1;
                        self.g.cpu_max_corps = 60;
                    }
                }
                5 => match hc {
                    1 => {
                        self.g.is_computer_player[3] = 1;
                        self.g.is_computer_player[4] = 1;
                        self.g.is_computer_player[5] = 1;
                        self.g.cpu_max_corps = 30;
                    }
                    2 => {
                        self.g.is_computer_player[3] = 1;
                        self.g.is_computer_player[5] = 1;
                        self.g.cpu_max_corps = 40;
                    }
                    3 => {
                        self.g.is_computer_player[3] = 1;
                        self.g.cpu_max_corps = 60;
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        // RANDOMIZE TIMER
        self.rng.randomize_timer(self.now.timer_ms());
        self.g.rand_seed = to_long(cast::<Ext, _>(self.now.time_secs()) + cast::<Ext, _>(self.g.rand_seed));
        self.setup_currency();

        // DIALOG SHOW STATE hDlgMain, 0 / 5 around the startup dialog (UI)
        if let Some(cfg) = show_startup_dialog(self.g, self.ui, self.rng) {
            self.written.push(("cfig.wsr".into(), cfg));
        } else {
            return;
        }
        self.f.reply = 7;
        if self.g.cfg_difficulty_raw == 4 {
            self.f.reply = 6;
            let mut note = st(__content.string(6638))
                + "\r\n"
                + "\r\n"
                + __content.string(6639)
                + __content.string(6640)
                + __content.string(6641);
            show_quick_message(self.g, self.ui, &mut note);
        }
        let n = self.g.num_players;
        if self.f.reply == 7 {
            self.g.wealth_tax_rate = -1;
            self.g.carbon_tax_rate = -1;
            self.g.oil_windfall_tax_rate = -1;
            self.g.holding_co_tax_rate = -1;
            self.g.corp_shares_tax_country = -1;
            self.g.health_care_tax_rate = -1;
        }
        self.f.j = 1;
        while self.f.j <= n {
            let j = self.f.j;
            self.g.entity_status[j] = 1;
            self.g.ent_symbol[j] = self.g.ent_name[j].clone();
            self.g.player_law_firm[j] = 2;
            if self.g.is_computer_player[j] == 1 {
                self.g.player_law_firm[j] = 3;
            }
            self.f.j += 1;
        }
        self.g.cur_quarter = 1;
        // fieldText = RIGHT$(DATE$, 4)
        self.f.field_text = right(str15(cast::<Ext, _>(self.now.date_year())).as_bytes(), 4);
        self.g.start_year = to_long(Ext::from_f64(1.0) + val(self.f.field_text.as_bytes()));
        if self.g.start_year <= 2024 {
            self.g.start_year = 2024;
        }
        self.g.cur_year = self.g.start_year;
        self.g.game_end_year += self.g.start_year;
        show_next_earnings_date(self.g);
        self.f.year_text = str15(cast::<Ext, _>(self.g.cur_year));
        // (UI) label 141: "Date:  January 1," + yearText
        self.g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(self.rng.rnd_range(40, 26)));
        self.g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(self.g.base_interest_rate) / Ext::from_f64(4.0)));
        self.g.prime_rate = cast::<f32, _>(self.g.base_interest_rate);
        update_interest_rates(self.g, self.rng);
        self.f.coupon_rate = cast::<f32, _>(int(cast::<Ext, _>(self.g.long_bond_yield) * Ext::from_f64(4.0)));
        self.f.coupon_rate = cast::<f32, _>(s(0.25f32) * cast::<Ext, _>(self.f.coupon_rate));
        self.f.maturity_year = cast::<f32, _>(Ext::from_f64(20.0) + cast::<Ext, _>(self.g.cur_year));
        self.f.field_text = str7(cast::<Ext, _>(self.f.maturity_year));
        self.f.field_text = trim(self.f.field_text.as_bytes());
        self.g.long_bond_desc = self.f.field_text.clone() + "@";
        self.f.field_text = fmt(cast::<Ext, _>(self.f.coupon_rate), "0#.00");
        self.g.long_bond_desc = self.g.long_bond_desc.clone() + &self.f.field_text;
        self.g.long_bond_price = cast::<f32, _>(Ext::from_f32(100.0f32));
        self.f.coupon_rate = cast::<f32, _>(int(cast::<Ext, _>(self.g.short_bond_yield) * Ext::from_f64(4.0)));
        self.f.coupon_rate = cast::<f32, _>(s(0.25f32) * cast::<Ext, _>(self.f.coupon_rate));
        self.f.maturity_year = cast::<f32, _>(Ext::from_f64(10.0) + cast::<Ext, _>(self.g.cur_year));
        self.f.field_text = str7(cast::<Ext, _>(self.f.maturity_year));
        self.f.field_text = trim(self.f.field_text.as_bytes());
        self.g.short_bond_desc = self.f.field_text.clone() + "@";
        self.f.field_text = fmt(cast::<Ext, _>(self.f.coupon_rate), "0#.00");
        self.g.short_bond_desc = self.g.short_bond_desc.clone() + &self.f.field_text;
        self.g.short_bond_price = cast::<f32, _>(Ext::from_f32(100.0f32));
        self.f.rand_val = to_quad(int(Ext::from_f64(20.0) * self.rng.rnd()));
        self.f.rand_val = to_quad(cast::<Ext, _>(self.f.rand_val) - Ext::from_f64(10.0));
        // QUAD: always rounds to 0 (see BUGS.md), so GdpLevel is always 7000
        self.f.rand_val = to_quad(s(0.01f32) * cast::<Ext, _>(self.f.rand_val));
        self.g.gdp_level = to_quad((cast::<Ext, _>(self.f.rand_val) + Ext::from_f64(1.0)) * Ext::from_f64(7000.0));
        self.g.gdp_growth = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(4.0) * self.rng.rnd() + Ext::from_f64(2.0)));
        self.f.rand_val = to_quad(int(Ext::from_f64(30.0) * self.rng.rnd()));
        self.g.oil_price = cast::<f32, _>(cast::<f32, _>(s(0.01f32) * cast::<Ext, _>(self.f.rand_val) + s(0.85f32)));
        if (cast::<Ext, _>(self.g.gdp_level)) < Ext::from_f64(7000.0) {
            self.g.oil_price = cast::<f32, _>(cast::<f32, _>(s(0.75f32) * cast::<Ext, _>(self.g.oil_price)));
        }
        self.f.rand_val = to_quad(int(Ext::from_f64(30.0) * self.rng.rnd()));
        self.f.rand_val = to_quad(cast::<Ext, _>(self.f.rand_val) - Ext::from_f64(10.0));
        self.f.rand_val = to_quad(cast::<Ext, _>(self.f.rand_val) * Ext::from_f64(10.0));
        self.g.housing_starts = to_long(cast::<Ext, _>(self.f.rand_val) + Ext::from_f64(1000.0));
        init_commodity_markets(self.g, self.ui, self.rng);
        update_crypto_prices(self.g, self.rng);
        self.g.corp_tax_rate = 21;
        self.g.cap_gains_tax_rate = 18;
        update_economy(self.g, self.ui, self.rng);
        calc_gov_bond_yields(self.g);
        self.g.monetary_policy = to_integer(int(s(2.99f32) * self.rng.rnd()));

        // OPEN DataPath + "WSR101.DAT" FOR INPUT AS #fileNum
        let Some(data) = files("WSR101.DAT") else {
            return; // error 53: ON ERROR handler (ShowRuntimeError), exit
        };
        let mut rd = InputReader::new(&data);
        loop {
            if self.f.line_text == "@END" {
                // CLOSE #fileNum
                self.finish();
                return;
            }
            match rd.try_line_input() {
                Ok(l) => self.f.line_text = PbStr(l),
                Err(_) => return, // error 62: the handler closes the file and exits
            }
            self.f.line_text = trim(self.f.line_text.as_bytes());
            if mid(self.f.line_text.as_bytes(), 1, 3) == "REM" {
                continue;
            }
            if len(self.f.line_text.as_bytes()) == 0 {
                continue;
            }
            if 0 < instr(1, self.f.line_text.as_bytes(), b"**IND") {
                self.parse_industry_record();
                continue;
            }
            if 0 < instr(1, self.f.line_text.as_bytes(), b"@END") {
                continue;
            }
            if !self.parse_company_line() {
                return;
            }
        }
    }

    /// The currency block: symbol, name, central bank and exchange rate of `CurrencyCode` (default "USA"),
    /// an optional new exchange rate, the currency news and the issue limits.
    fn setup_currency(&mut self) {
    let __content = crate::content::current();
        if self.g.currency_code.is_empty() {
            self.g.currency_code = st("USA");
        }
        self.g.exchange_rate = cast::<f64, _>(Ext::from_f64(1.0));
        self.g.central_bank_name = st(__content.string(6642));
        self.g.million_prefix_uc = st("M");
        self.g.million_prefix_lc = st("m");
        let code = self.g.currency_code.clone();
        let g = &mut *self.g;
        match code.as_bytes() {
            b"USA" => {
                g.currency_symbol = st("$");
                g.currency_name = st(__content.string(6643));
                g.central_bank_name = st(__content.string(4373));
            }
            b"UK" => {
                g.currency_symbol = PbStr(vec![163]);
                g.exchange_rate = cast::<f64, _>(s(0.79f32));
                g.currency_name = st(__content.string(6644));
                g.central_bank_name = st(__content.string(6645));
            }
            b"EUR" => {
                g.currency_suffix = st(" Euros");
                g.exchange_rate = cast::<f64, _>(s(0.93f32));
                g.currency_name = st(__content.string(6646));
                g.central_bank_name = st(__content.string(6647));
            }
            b"JAP" => {
                g.currency_symbol = PbStr(vec![165]);
                g.exchange_rate = cast::<f64, _>(s(0.146f32));
                g.currency_name = st(__content.string(6648));
                g.million_prefix_uc = st("B");
                g.million_prefix_lc = st("b");
                g.central_bank_name = st(__content.string(6649));
            }
            b"KOR" => {
                g.currency_suffix = st(" Won");
                g.exchange_rate = cast::<f64, _>(s(1.324f32));
                g.currency_name = st(__content.string(6650));
                g.million_prefix_uc = st("B");
                g.million_prefix_lc = st("b");
            }
            b"IND" => {
                g.exchange_rate = cast::<f64, _>(s(0.082f32));
                g.currency_suffix = st(" Rupees");
                g.currency_name = st(__content.string(6651));
                g.million_prefix_uc = st("B");
                g.million_prefix_lc = st("b");
                g.central_bank_name = st(__content.string(6652));
            }
            b"NZ" => {
                g.currency_symbol = st("NZ$");
                g.exchange_rate = cast::<f64, _>(s(1.7f32));
                g.currency_name = st(__content.string(6653));
            }
            b"SWE" => {
                g.currency_suffix = st(" Kronor");
                g.exchange_rate = cast::<f64, _>(s(11.05f32));
                g.currency_name = st(__content.string(6654));
            }
            b"SWI" => {
                g.currency_suffix = st(" SwFr.");
                g.exchange_rate = cast::<f64, _>(s(0.88f32));
                g.currency_name = st(__content.string(6655));
                g.central_bank_name = st(__content.string(6656));
            }
            b"BRZ" => {
                g.currency_suffix = st(" Reals");
                g.exchange_rate = cast::<f64, _>(s(4.9f32));
                g.currency_name = st(__content.string(6657));
            }
            b"CAN" => {
                g.currency_symbol = st("C$");
                g.exchange_rate = cast::<f64, _>(s(1.36f32));
                g.currency_name = st(__content.string(6658));
                g.central_bank_name = st(__content.string(6659));
            }
            b"AUS" => {
                g.currency_symbol = st("A$");
                g.exchange_rate = cast::<f64, _>(s(1.56f32));
                g.currency_name = st(__content.string(6660));
                g.central_bank_name = st(__content.string(6661));
            }
            b"HK" => {
                g.currency_symbol = st("HK$");
                g.exchange_rate = cast::<f64, _>(s(7.85f32));
                g.currency_name = st(__content.string(6662));
            }
            b"SNG" => {
                g.currency_symbol = st("S$");
                g.exchange_rate = cast::<f64, _>(s(1.35f32));
                g.currency_name = st(__content.string(6663));
            }
            b"CHN" => {
                g.currency_suffix = st(" Yuan");
                g.exchange_rate = cast::<f64, _>(s(7.3f32));
                g.currency_name = st(__content.string(6664));
                g.central_bank_name = st(__content.string(6665));
            }
            b"ARG" => {
                g.currency_suffix = st(" Pesos");
                g.exchange_rate = cast::<f64, _>(s(350.0f32));
                g.currency_name = st(__content.string(6666));
            }
            b"ISR" => {
                g.currency_suffix = st(__content.string(6667));
                g.exchange_rate = cast::<f64, _>(s(3.8f32));
                g.currency_name = st(__content.string(6668));
            }
            b"MAL" => {
                g.currency_suffix = st(__content.string(6669));
                g.exchange_rate = cast::<f64, _>(s(4.65f32));
                g.currency_name = st(__content.string(6670));
            }
            b"MEX" => {
                g.currency_suffix = st(" Pesos");
                g.exchange_rate = cast::<f64, _>(s(16.75f32));
                g.currency_name = st(__content.string(6671));
                g.central_bank_name = st(__content.string(6672));
            }
            b"DAN" => {
                g.currency_suffix = st(" Kroner");
                g.exchange_rate = cast::<f64, _>(s(6.9f32));
                g.currency_name = st(__content.string(6673));
                g.central_bank_name = st(__content.string(6647));
            }
            b"NOR" => {
                g.currency_suffix = st(" Kroner");
                g.exchange_rate = cast::<f64, _>(s(10.7f32));
                g.currency_name = st(__content.string(6674));
                g.central_bank_name = st(__content.string(6647));
            }
            b"SAF" => {
                g.currency_suffix = st(" Rand");
                g.exchange_rate = cast::<f64, _>(s(18.6f32));
                g.currency_name = st(__content.string(6675));
            }
            b"EGY" => {
                g.currency_symbol = PbStr(vec![b'E', 163]);
                g.currency_name = st(__content.string(6676));
                g.exchange_rate = cast::<f64, _>(s(31.0f32));
            }
            b"ICE" => {
                g.currency_suffix = st(" Kronur");
                g.exchange_rate = cast::<f64, _>(s(0.132f32));
                g.currency_name = st(__content.string(6677));
                g.central_bank_name = st(__content.string(6647));
                g.million_prefix_uc = st("B");
                g.million_prefix_lc = st("b");
            }
            _ => {}
        }
        if !self.g.currency_symbol.is_empty() {
            self.g.currency_symbol = self.g.currency_symbol.clone() + " ";
        }
        // (UI) UpdateCurrencyMenuText (menu item 651); label 122 " [runtime resource wording] " for "B"
        if !(self.g.currency_code == "USA") {
            self.f.field_text = self.g.currency_suffix.clone() + " ";
            self.f.rate_input = cast::<f32, _>(self.g.exchange_rate);
            if self.g.million_prefix_lc == "b" {
                self.f.rate_input = cast::<f32, _>(Ext::from_f64(1000.0) * cast::<Ext, _>(self.f.rate_input));
            }
            self.f.msg_text = st(__content.string(6678))
                + &self.g.currency_name
                + ", with "
                + __content.string(6679)
                + &trim(fmt(cast::<Ext, _>(self.f.rate_input), "###.00").as_bytes())
                + &self.f.field_text
                + __content.string(6680)
                + "\r"
                + "\r"
                + __content.string(6681)
                + __content.string(6682);
            let msg = self.f.msg_text.clone();
            self.f.reply = self.msgbox(&msg, 8228, __content.string(6683));
            if self.f.reply == 6 {
                let mut cancelled = false;
                loop {
                    self.f.rate_input = cast::<f32, _>(self.g.exchange_rate);
                    self.f.min_rate = 0.5f32;
                    self.f.min_rate_text = st("0.5");
                    self.f.max_rate = 100.0f32;
                    self.f.max_rate_text = st("100");
                    if self.g.currency_code == "ARG" {
                        self.f.min_rate = 10.0f32;
                        self.f.min_rate_text = st("10");
                        self.f.max_rate = 500.0f32;
                        self.f.max_rate_text = st("500");
                    }
                    if self.g.million_prefix_lc == "b" {
                        self.f.rate_input = cast::<f32, _>(Ext::from_f64(1000.0) * cast::<Ext, _>(self.f.rate_input));
                        self.f.min_rate = 50.0f32;
                        self.f.min_rate_text = st("50");
                        self.f.max_rate = 2500.0f32;
                        self.f.max_rate_text = st("2500");
                    }
                    self.f.msg_text = st(__content.string(6684))
                        + &self.g.currency_name
                        + __content.string(6685)
                        + &self.f.min_rate_text
                        + " and "
                        + &self.f.max_rate_text
                        + ").";
                    let default = trim(fmt(cast::<Ext, _>(self.f.rate_input), "###.00").as_bytes());
                    self.f.rate_text = self.ui.inputbox(
                        &self.f.msg_text,
                        &st(__content.string(6686)),
                        &default,
                    );
                    self.f.rate_input = cast::<f32, _>(val(self.f.rate_text.as_bytes()));
                    if self.f.rate_input == Ext::from_f32(0.0f32) {
                        cancelled = true;
                        break;
                    }
                    if !(self.f.rate_input < self.f.min_rate || self.f.max_rate < self.f.rate_input)
                    {
                        break;
                    }
                }
                if !cancelled {
                    if self.g.million_prefix_lc == "b" {
                        self.g.exchange_rate = cast::<f64, _>(cast::<Ext, _>(self.f.rate_input) * s(0.001f32));
                    } else {
                        self.g.exchange_rate = cast::<f64, _>(cast::<Ext, _>(self.f.rate_input));
                    }
                }
            }
        }
        // LAB_00475703
        self.g.news_msg = st(__content.string(6687)) + &self.g.currency_name;
        if self.g.currency_symbol.is_empty() {
            self.g.news_msg = self.g.news_msg.clone() + ".";
        } else {
            self.g.news_msg =
                self.g.news_msg.clone() + " (" + &trim(self.g.currency_symbol.as_bytes()) + ")....";
        }
        self.post();
        if !(self.g.currency_code == "USA") {
            self.f.rate_input = cast::<f32, _>(self.g.exchange_rate);
            if self.g.million_prefix_lc == "b" {
                self.f.rate_input = cast::<f32, _>(Ext::from_f64(1000.0) * cast::<Ext, _>(self.f.rate_input));
            }
            self.g.news_msg = st(__content.string(6688));
            self.post();
            self.f.field_text = self.g.currency_suffix.clone() + " ";
            self.g.news_msg = st(__content.string(6689))
                + &trim(fmt(cast::<Ext, _>(self.f.rate_input), "###.00").as_bytes())
                + &self.f.field_text
                + __content.string(6690);
            self.post();
        }
        self.g.news_msg = st(__content.string(6691))
            + &self.g.currency_symbol
            + &self.g.million_prefix_lc
            + "illions"
            + &self.g.currency_suffix
            + ".) ";
        self.post();
        self.g.news_msg =
            st(__content.string(6692));
        self.post();
        self.g.max_bond_issue = to_quad(self.g.exchange_rate * Ext::from_f64(150000.0));
        self.g.max_stock_offering_amt = to_quad(self.g.exchange_rate * Ext::from_f64(100000.0));
    }

    /// A company line (`#nnnn ST=…`) or a holding line (`nnn>pp%`) of WSR101.DAT. Returns false where the
    /// original leaves NewGameSetup (no `ST=` item).
    fn parse_company_line(&mut self) -> bool {
    let __content = crate::content::current();
        self.f.is_holding_line = 0.0f32;
        self.f.pos = instr(1, self.f.line_text.as_bytes(), b"#");
        if self.f.pos < 1 {
            self.parse_stock_holding();
            self.f.is_holding_line = 1.0f32;
        } else {
            self.f.holding_idx = 0;
            self.f.field_text = mid(self.f.line_text.as_bytes(), self.f.pos + 1, 4);
            self.f.corp_id = to_long(val(self.f.field_text.as_bytes()));
            if 0 < self.f.industry {
                self.g.industry_num[self.f.corp_id] = cast::<i16, _>(self.f.industry);
            }
        }
        if self.f.is_holding_line == Ext::from_f32(1.0f32) {
            return true;
        }
        let c = self.f.corp_id;
        let line = self.f.line_text.clone();
        let line = line.as_bytes();
        self.f.pos = instr(1, line, b"ST=");
        if self.f.pos < 1 {
            if 0 < self.f.industry {
                self.msgbox(&st(__content.string(6693)), 0, "");
            }
            return false;
        }
        self.f.field_text = mid(line, self.f.pos + 3, 1);
        self.f.tmp_long = to_quad(val(self.f.field_text.as_bytes()));
        self.g.entity_status[c] = cast::<i16, _>(self.f.tmp_long);
        if self.g.entity_status[c] == 1 {
            self.g.price_hist_high[(0, c)] = -Ext::from_f64(1.0);
        }
        if self.g.entity_status[c] < 0 || 1 < self.g.entity_status[c] {
            let m =
                st(__content.string(6694)) + &str15(cast::<Ext, _>(c));
            self.msgbox(&m, 0, "");
        }
        if self.f.industry < 1 {
            let m = st(__content.string(6695)) + &str15(cast::<Ext, _>(c));
            self.msgbox(&m, 0, "");
        } else {
            self.g.industry_num[c] = cast::<i16, _>(self.f.industry);
        }
        if self.g.entity_status[c] == 0 {
            return true;
        }
        self.f.pos = instr(1, line, b"CR=");
        if self.f.pos < 1 {
            let ind = self.g.industry_num[c];
            if 1 < ind && ind < 70 {
                if ind == 2 {
                    self.f.rating_str = st("AA");
                    self.f.tmp_val = to_long(int(Ext::from_f64(10.0) * self.rng.rnd()));
                    if self.f.tmp_val == 0 {
                        self.f.rating_str = st("AAA");
                    }
                    if 5 < self.f.tmp_val {
                        self.f.rating_str = st("A");
                    }
                    if self.f.tmp_val == 9 {
                        self.f.rating_str = st("BBB");
                    }
                } else {
                    self.f.tmp_val = to_long(int(Ext::from_f64(40.0) * self.rng.rnd()));
                    self.f.rating_str = st(match self.f.tmp_val {
                        0 => "AAA",
                        1 | 2 => "AA",
                        3..=9 => "A",
                        10..=21 => "BBB",
                        22..=28 => "BB",
                        29..=32 => "B",
                        33 | 34 => "CCC",
                        35 | 36 => "CC",
                        37 | 38 | 40 => "C",
                        39 => "D",
                        _ => "BBB",
                    });
                }
            }
            // (industry 1 or >= 70: ratingStr keeps the previous company's value)
        } else {
            self.f.field_text = mid(line, self.f.pos + 3, 3);
            self.f.field_text = trim(self.f.field_text.as_bytes());
            self.f.rating_str = self.f.field_text.clone();
        }
        self.f.pos = instr(1, line, b"JBOS=");
        if 0 < self.f.pos {
            self.f.field_text = mid(line, self.f.pos + 5, 4);
            self.g.bonds_issued[c] = to_quad(val(self.f.field_text.as_bytes()));
            self.f.pos = instr(1, line, b"BCOUP=");
            if self.f.pos < 1 {
                let m = st(__content.string(6696))
                    + &str15(cast::<Ext, _>(c));
                self.msgbox(&m, 0, "");
            } else {
                self.g.bond_issue_info[c] = mid(line, self.f.pos + 6, 10);
                self.f.field_text = self.g.bond_issue_info[c].clone();
                if val(mid(self.f.field_text.as_bytes(), 1, 4).as_bytes())
                    < Ext::from_f64(2.0) + cast::<Ext, _>(self.g.start_year)
                {
                    self.f.tmp_val = to_long(cast::<Ext, _>(self.g.start_year) + int(Ext::from_f64(5.0) * self.rng.rnd()));
                    self.f.new_year_text = trim(str15(cast::<Ext, _>(self.f.tmp_val)).as_bytes());
                    self.f.field_text = mid(self.f.field_text.as_bytes(), 1, 4);
                    self.g.bond_issue_info[c] = replace(
                        self.g.bond_issue_info[c].as_bytes(),
                        self.f.field_text.as_bytes(),
                        self.f.new_year_text.as_bytes(),
                    );
                }
            }
        }
        self.f.pos = instr(1, line, b"PE=");
        if self.f.pos < 1 {
            self.f.pe_ratio = 12.0f32;
        } else {
            self.f.field_text = mid(line, self.f.pos + 3, 2);
            self.f.pe_ratio = cast::<f32, _>(val(self.f.field_text.as_bytes()));
        }
        self.f.pos = instr(1, line, b"BA%=");
        if self.f.pos < 1 {
            if 1 < self.g.industry_num[c] && self.g.entity_status[c] == 1 {
                let m = st(__content.string(6697))
                    + &str15(cast::<Ext, _>(c));
                self.msgbox(&m, 0, "");
            }
        } else {
            self.f.field_text = mid(line, self.f.pos + 4, 2);
            self.f.tmp_long = to_quad(val(self.f.field_text.as_bytes()));
            self.g.lending_bank[c] = cast::<i16, _>(self.f.tmp_long);
        }
        self.f.magnitude = 0.0f32;
        self.f.pos = instr(1, line, b"MAG=");
        if self.f.pos < 1 {
            if 1 < self.g.industry_num[c] && self.g.industry_num[c] < 70 {
                self.g.shares_outstanding[c] = s(0.5f32);
            } else {
                self.g.shares_outstanding[c] = Ext::from_f64(2.0);
            }
        } else {
            self.f.field_text = mid(line, self.f.pos + 4, 2);
            self.f.magnitude = cast::<f32, _>(val(self.f.field_text.as_bytes()));
            if !(Ext::from_f32(6.0f32) < self.f.magnitude) {
                self.g.shares_outstanding[c] = Ext::from_f64(1.0);
            } else {
                self.g.shares_outstanding[c] = cast::<Ext, _>(self.f.magnitude) - Ext::from_f64(4.0);
            }
        }
        // LAB_00477560
        let ind = self.g.industry_num[c];
        if ind != 1 && ind != 70 && ind != 71 {
            self.f.pos = instr(1, line, b"GR%=");
            if self.f.pos < 1 {
                self.f.tmp_long = to_quad(int(Ext::from_f64(13.0) * self.rng.rnd()) - Ext::from_f64(3.0));
                self.g.growth_rate[c] = cast::<i16, _>(self.f.tmp_long);
            } else {
                self.f.field_text = mid(line, self.f.pos + 4, 2);
                self.f.tmp_long = to_quad(val(self.f.field_text.as_bytes()));
                self.g.growth_rate[c] = cast::<i16, _>(self.f.tmp_long);
            }
        }
        self.f.pos = instr(1, line, b"DV%=");
        if self.f.pos < 1 {
            let ind = self.g.industry_num[c];
            if 1 < ind && ind < 70 {
                self.g.dividend_per_share[c] = int(Ext::from_f64(25.0) * self.rng.rnd()) + Ext::from_f64(10.0);
                if self.g.industry_num[c] == 69 {
                    self.g.dividend_per_share[c] = Ext::from_f64(15.0) + self.g.dividend_per_share[c];
                } else if 15 < self.g.growth_rate[c] {
                    self.g.dividend_per_share[c] = Ext::from_f64(0.0);
                }
            } else if ind == 71 {
                self.g.dividend_per_share[c] = Ext::from_f64(0.0);
            } else {
                self.g.dividend_per_share[c] = int(Ext::from_f64(40.0) * self.rng.rnd()) + Ext::from_f64(10.0);
            }
        } else {
            self.f.field_text = mid(line, self.f.pos + 4, 2);
            self.g.dividend_per_share[c] = val(self.f.field_text.as_bytes());
        }
        // LAB_00477918
        let m = self.f.magnitude;
        self.f.asset_base = Ext::from_f64(300.0);
        if m == Ext::from_f32(1.0f32) {
            self.f.asset_base = Ext::from_f64(700.0);
        }
        if m == Ext::from_f32(2.0f32) {
            self.f.asset_base = Ext::from_f64(1000.0);
        }
        if m == Ext::from_f32(3.0f32) {
            self.f.asset_base = Ext::from_f64(2000.0);
        }
        if m == Ext::from_f32(4.0f32) {
            self.f.asset_base = Ext::from_f64(4500.0);
        }
        if m == Ext::from_f32(5.0f32) {
            self.f.asset_base = Ext::from_f64(7500.0);
        }
        if m == Ext::from_f32(6.0f32) {
            self.f.asset_base = Ext::from_f64(15000.0);
        }
        if m == Ext::from_f32(7.0f32) {
            self.f.asset_base = Ext::from_f64(35000.0);
        }
        if m == Ext::from_f32(8.0f32) {
            self.f.asset_base = Ext::from_f64(60000.0);
        }
        if m == Ext::from_f32(9.0f32) {
            self.f.asset_base = Ext::from_f64(100000.0);
        }
        if m == Ext::from_f32(10.0f32) {
            self.f.asset_base = Ext::from_f64(150000.0);
        }
        if m == Ext::from_f32(11.0f32) {
            self.f.asset_base = Ext::from_f64(250000.0);
        }
        if !(Ext::from_f32(12.0f32) > m) {
            self.f.asset_base = Ext::from_f64(500000.0);
        }
        self.f.pos = instr(1, line, b"PL1=");
        if 0 < self.f.pos {
            self.f.field_text = mid(line, self.f.pos + 4, 5);
            self.g.earnings_y1[c] = val(self.f.field_text.as_bytes());
        }
        if self.g.industry_num[c] < 70 {
            self.f.tmp_long = to_quad(Ext::from_f64(9.0) * self.rng.rnd());
            self.g.disc_spend_pct[c] = cast::<i16, _>(self.f.tmp_long);
        }
        match self.g.industry_num[c] {
            1 => self.parse_bank_cash(),
            2 => self.init_insurer_balance(),
            70 | 71 => self.init_holding_co_balance(),
            _ => self.init_operating_corp_balance(),
        }
        true
    }

    /// The end of WSR101.DAT ("@END"): the "Ready to start" box, NewGame_FinalizeWorld, the ETFs, the turn
    /// order and the final pass over all companies.
    fn finish(&mut self) {
    let __content = crate::content::current();
        let m = st(__content.string(6698))
            + &str15(cast::<Ext, _>(self.g.game_number))
            + __content.string(6699)
            + __content.string(6700)
            + __content.string(6701)
            + __content.string(6702);
        self.msgbox(&m, 8192, __content.string(6703));
        self.finalize_world();
        if !init_etfs(self.g, self.ui, self.rng) {
            self.g.game_end_flag = st("Y");
            self.g.game_in_progress = 0;
            return;
        }
        // (UI) label 123 " [runtime resource wording] "
        self.set_turn_order();
        self.g.sel_entity = self.g.active_entity;
        self.g.sel_corp_name = self.g.active_entity_name.clone();
        if self.g.long_game_marker == 0 {
            self.g.long_game_marker = cast::<i16, _>(self.g.rand_seed % 5);
        }
        refresh_selection_labels(self.g);
        self.g.game_in_progress = 1;
        // (UI) EnableMainControls
        // UpdateFileMenuState: menus (UI); SetMenusGameEnded clears AutoAddStreamList once the game has ended
        if self.g.game_end_flag == "Y" {
            self.g.auto_add_stream_list = 0;
        }
        create_industry_combo(self.g);
        let secs = self.now.time_secs();
        self.f.corp_id = 11;
        loop {
            let c = self.f.corp_id;
            if self.g.entity_status[c] == 1 {
                if c == 140
                    || c == 423
                    || c == 620
                    || c == 621
                    || c == 954
                    || c == 1431
                    || c == 1499
                {
                    apply_famous_corp_tweaks(self.g, self.ui, self.rng, c);
                    self.g.corp_prior_net_worth[c] = self.g.net_worth;
                } else {
                    calc_stock_price(self.g, self.ui, self.rng, c, secs);
                    self.g.corp_prior_net_worth[c] = self.g.net_worth;
                    if 60 < c
                        && self.g.credit_rating_num[c] < 8
                        && Ext::from_f64(0.0) < cast::<Ext, _>(self.g.bonds_issued[c])
                        && (cast::<Ext, _>(self.g.bonds_issued[c])) < self.g.bank_debt[c] * s(0.4f32)
                        && Ext::from_f64(1.0) < Ext::from_f64(10.0) * self.rng.rnd()
                    {
                        self.f.amount = to_quad(cast::<Ext, _>(self.g.bonds_issued[c]) * s(0.1f32));
                        self.g.bonds_issued[c] =
                            to_quad(cast::<Ext, _>(self.g.bonds_issued[c]) - cast::<Ext, _>(self.f.amount));
                        self.g.bank_debt[c] = cast::<Ext, _>(self.f.amount) + self.g.bank_debt[c];
                        calc_stock_price(self.g, self.ui, self.rng, c, secs);
                    }
                }
                if 1580 < c {
                    self.g.ctrl_since_year[c] = self.g.start_year - 5;
                }
                if self.g.hold_stock_id[(0, c)] != 0 {
                    let n = cast::<i32, _>(self.g.hold_stock_id[(0, c)]);
                    self.f.hold_idx = 1;
                    while self.f.hold_idx <= n {
                        let h = self.f.hold_idx;
                        self.f.tmp_val = cast::<i32, _>(self.g.hold_stock_id[(h, c)]);
                        let held = cast::<i32, _>(self.g.hold_stock_id[(h, c)]);
                        self.g.hold_stock_basis[(h, c)] =
                            self.g.market_cap[held] * cast::<Ext, _>(self.g.hold_stock_pct[(h, c)]);
                        if !(self.g.hold_stock_pct[(h, c)] < 80)
                            && !(69 < self.g.industry_num[self.f.tmp_val])
                        {
                            self.g.ctrl_since_year[self.f.tmp_val] = 1950;
                        }
                        self.f.hold_idx += 1;
                    }
                }
            }
            self.f.corp_id += 1;
            if 1600 < self.f.corp_id {
                init_earnings_history(self.g, self.ui, self.rng, secs);
                self.g.econ_cycle_counter = 25;
                init_convertible_bond_issues(self.g, self.ui, self.rng);
                return; // NewGameSetup_Exit
            }
        }
    }

    /// Port of NewGame_ParseBankCash (0x477c03), GOSUB of NewGameSetup.
    /// A bank's cash from `CASH=` (plus its bonds issued); its lending bank is the previous bank.
    fn parse_bank_cash(&mut self) {
    let __content = crate::content::current();
        let c = self.f.corp_id;
        if self.g.entity_status[c] == 0 {
            return;
        }
        let line = self.f.line_text.clone();
        self.f.pos = instr(1, line.as_bytes(), b"CASH=");
        if self.f.pos < 1 {
            if self.f.industry == 1 {
                let m = st(__content.string(6704)) + &str15(cast::<Ext, _>(c));
                self.msgbox(&m, 0, "");
            }
        } else {
            self.f.field_text = mid(line.as_bytes(), self.f.pos + 5, 5);
            self.g.cash[c] = val(self.f.field_text.as_bytes());
            if Ext::from_f64(0.0) < cast::<Ext, _>(self.g.bonds_issued[c]) {
                self.g.cash[c] = cast::<Ext, _>(self.g.bonds_issued[c]) + self.g.cash[c];
            }
        }
        self.g.lending_bank[c] = cast::<i16, _>(c - 1);
    }

    /// Port of NewGame_InitInsurerBalance (0x477d9d), GOSUB of NewGameSetup.
    /// An insurer's gov bonds, cash, subprime securities, reserves and debt from the asset base, plus
    /// the first year's earnings if not given; halves the bonds of an insurer with little equity.
    fn init_insurer_balance(&mut self) {
        let c = self.f.corp_id;
        let ab = self.f.asset_base;
        let r1 = self.rng.rnd();
        let r2 = self.rng.rnd();
        self.g.long_gov_bond_face[c] =
            to_quad((ab * (s(0.1f32) * r1) + ab * s(0.25f32)) - ab * (s(0.2f32) * r2));
        self.g.short_gov_bond_face[c] = to_quad(cast::<Ext, _>(self.g.long_gov_bond_face[c]) * s(1.5f32));
        let r1 = self.rng.rnd();
        let r2 = self.rng.rnd();
        self.g.cash[c] = (ab * (s(0.1f32) * r1) + ab * s(0.5f32)) - ab * (s(0.2f32) * r2);
        self.f.asset_base = (cast::<Ext, _>(self.g.short_gov_bond_face[c])
            + cast::<Ext, _>(self.g.long_gov_bond_face[c]))
            + self.g.cash[c];
        let ab = self.f.asset_base;
        let r = self.rng.rnd();
        self.g.subprime_secs[c] = to_quad(ab * (s(0.15f32) * r) + ab * s(0.1f32));
        self.f.asset_base = cast::<Ext, _>(self.g.subprime_secs[c]) + self.f.asset_base;
        let ab = self.f.asset_base;
        let r = self.rng.rnd();
        self.g.ins_reserves[c] = ab * (s(0.2f32) * r) + ab * s(0.6f32);
        self.f.asset_base = self.f.asset_base - self.g.ins_reserves[c];
        self.set_debt_by_rating();
        self.f.asset_base = self.g.ins_reserves[c] + self.f.asset_base;
        let r = self.rng.rnd();
        self.g.bank_debt[c] = (s(0.2f32) * r + s(0.9f32)) * self.g.bank_debt[c];
        if Ext::from_f64(0.0) < cast::<Ext, _>(self.g.bonds_issued[c]) {
            self.g.cash[c] = cast::<Ext, _>(self.g.bonds_issued[c]) + self.g.cash[c];
        }
        self.f.asset_base = cast::<Ext, _>(self.g.bonds_issued[c]) + self.f.asset_base;
        if self.g.earnings_y1[c] == Ext::from_f64(0.0) {
            let ab = self.f.asset_base;
            let r = self.rng.rnd();
            self.g.earnings_y1[c] = ab * (s(0.17f32) * r) + -s(0.02f32) * ab;
        }
        self.f.net_equity = self.f.asset_base
            - ((cast::<Ext, _>(self.g.bonds_issued[c]) + self.g.bank_debt[c]) + self.g.ins_reserves[c]);
        if !(self.f.net_equity >= Ext::from_f64(50.0)) && Ext::from_f64(0.0) < cast::<Ext, _>(self.g.bonds_issued[c]) {
            self.g.bonds_issued[c] = to_quad(s(0.5f32) * cast::<Ext, _>(self.g.bonds_issued[c]));
        }
    }

    /// Port of NewGame_InitHoldingCoBalance (0x47821d), GOSUB of NewGameSetup.
    /// A holding company's or fund's cash from `CASH=` (plus bonds issued); holding companies (not funds)
    /// get debt by rating against their cash, and halved bonds when their equity is small.
    fn init_holding_co_balance(&mut self) {
        let c = self.f.corp_id;
        let line = self.f.line_text.clone();
        self.f.pos = instr(1, line.as_bytes(), b"CASH=");
        if 0 < self.f.pos {
            self.f.field_text = mid(line.as_bytes(), self.f.pos + 5, 5);
            self.g.cash[c] = val(self.f.field_text.as_bytes());
            if Ext::from_f64(0.0) < cast::<Ext, _>(self.g.bonds_issued[c]) {
                self.g.cash[c] = cast::<Ext, _>(self.g.bonds_issued[c]) + self.g.cash[c];
            }
        }
        if self.g.industry_num[c] != 71 {
            self.f.asset_base = self.g.cash[c];
            self.set_debt_by_rating();
            let r = self.rng.rnd();
            self.g.bank_debt[c] = (s(0.2f32) * r + s(0.9f32)) * self.g.bank_debt[c];
            self.f.net_equity =
                self.f.asset_base - (cast::<Ext, _>(self.g.bonds_issued[c]) + self.g.bank_debt[c]);
            if !(self.f.net_equity >= Ext::from_f64(50.0)) && Ext::from_f64(0.0) < cast::<Ext, _>(self.g.bonds_issued[c]) {
                self.g.bonds_issued[c] = to_quad(s(0.5f32) * cast::<Ext, _>(self.g.bonds_issued[c]));
            }
        }
    }

    /// Port of NewGame_InitOperatingCorpBalance (0x478492), GOSUB of NewGameSetup.
    /// An operating company's business assets, cash (from `CASH=` or 5% of the randomized asset base),
    /// working capital, debt by rating and the first year's earnings if not given.
    fn init_operating_corp_balance(&mut self) {
        let c = self.f.corp_id;
        let r = self.rng.rnd();
        self.f.asset_base = (s(0.4f32) * r + s(0.8f32)) * self.f.asset_base;
        let line = self.f.line_text.clone();
        self.f.pos = instr(1, line.as_bytes(), b"CASH=");
        if 0 < self.f.pos {
            self.f.field_text = mid(line.as_bytes(), self.f.pos + 5, 5);
            self.g.cash[c] = val(self.f.field_text.as_bytes());
            if Ext::from_f64(0.0) < cast::<Ext, _>(self.g.bonds_issued[c]) {
                self.g.cash[c] = cast::<Ext, _>(self.g.bonds_issued[c]) + self.g.cash[c];
            }
        }
        let ab = self.f.asset_base;
        let r = self.rng.rnd();
        self.g.business_assets[c] = (ab * s(0.83f32)) * (s(0.4f32) * r + s(0.8f32));
        if self.g.cash[c] == Ext::from_f64(0.0) {
            let r = self.rng.rnd();
            self.g.cash[c] = (ab * s(0.05f32)) * (s(0.4f32) * r + s(0.8f32));
        }
        let r = self.rng.rnd();
        self.g.working_capital[c] = (ab * s(0.12f32)) * (s(0.4f32) * r + s(0.8f32));
        self.f.asset_base =
            (self.g.cash[c] + self.g.business_assets[c]) + self.g.working_capital[c];
        self.set_debt_by_rating();
        let r = self.rng.rnd();
        self.g.bank_debt[c] = (s(0.2f32) * r + s(0.9f32)) * self.g.bank_debt[c];
        if Ext::from_f64(0.0) < cast::<Ext, _>(self.g.bonds_issued[c]) {
            self.g.cash[c] = cast::<Ext, _>(self.g.bonds_issued[c]) + self.g.cash[c];
        }
        self.f.asset_base = cast::<Ext, _>(self.g.bonds_issued[c]) + self.f.asset_base;
        if self.g.earnings_y1[c] == Ext::from_f64(0.0) {
            let ab = self.f.asset_base;
            let r = self.rng.rnd();
            self.g.earnings_y1[c] = ab * (s(0.37f32) * r) + -s(0.02f32) * ab;
        }
    }

    /// Port of NewGame_ParseStockHolding (0x478864), GOSUB of NewGameSetup.
    /// A holding line `id>pct%` of the current company: next holding slot = company `id`, `pct` percent.
    fn parse_stock_holding(&mut self) {
        let line = self.f.line_text.clone();
        let line = line.as_bytes();
        self.f.pos = instr(1, line, b">");
        if 0 < self.f.pos {
            self.f.holding_idx += 1;
            self.f.field_text = mid(line, 1, self.f.pos - 1);
            self.f.held_corp_id = to_long(val(self.f.field_text.as_bytes()));
            self.g.hold_stock_id[(self.f.holding_idx, self.f.corp_id)] = cast::<i16, _>(self.f.held_corp_id);
            self.f.line_len = len(line);
            self.f.field_text = right(line, self.f.line_len - self.f.pos);
            if 0 < instr(1, self.f.field_text.as_bytes(), b"%") {
                self.f.field_text = replace(self.f.field_text.as_bytes(), b"%", b" ");
            }
            self.f.tmp_val = to_long(val(self.f.field_text.as_bytes()));
            self.g.hold_stock_pct[(self.f.holding_idx, self.f.corp_id)] = cast::<i16, _>(self.f.tmp_val);
        }
    }

    /// Port of NewGame_ParseIndustryRecord (0x478a04), GOSUB of NewGameSetup.
    /// An industry line `**IND=nn IDGRO%=gg IDVOL%=vv INDNAM=name`: the current industry number, its
    /// long-term (and current) growth rate, volatility and name.
    fn parse_industry_record(&mut self) {
    let __content = crate::content::current();
        let line = self.f.line_text.clone();
        let lb = line.as_bytes();
        self.f.pos = instr(1, lb, b"**IND=");
        if self.f.pos < 1 {
            self.msgbox(&st(__content.string(6705)), 0, "");
            self.msgbox(&line, 0, "");
        } else {
            self.f.field_text = mid(lb, self.f.pos + 6, 2);
            self.f.industry = to_long(val(self.f.field_text.as_bytes()));
        }
        let ind = self.f.industry;
        self.f.pos = instr(1, lb, b"IDGRO%=");
        if self.f.pos < 1 {
            if 2 < ind && ind < 70 {
                self.msgbox(&st(__content.string(6706)), 0, "");
                self.msgbox(&line, 0, "");
            }
        } else {
            self.f.field_text = mid(lb, self.f.pos + 7, 2);
            self.f.tmp_long = to_quad(val(self.f.field_text.as_bytes()));
            self.g.ind_long_term_growth[ind] = cast::<i16, _>(self.f.tmp_long);
            self.g.ind_growth_rate_now[ind] = self.g.ind_long_term_growth[ind];
        }
        self.f.pos = instr(1, lb, b"IDVOL%=");
        if self.f.pos < 1 {
            if 2 < ind && ind < 70 {
                self.msgbox(&st(__content.string(6707)), 0, "");
                self.msgbox(&line, 0, "");
            }
        } else {
            self.f.field_text = mid(lb, self.f.pos + 7, 2);
            self.f.tmp_long = to_quad(val(self.f.field_text.as_bytes()));
            self.g.ind_volatility[ind] = cast::<i16, _>(self.f.tmp_long);
        }
        self.f.pos = instr(1, lb, b"INDNAM=");
        if self.f.pos < 1 {
            self.msgbox(&st(__content.string(6708)), 0, "");
            self.msgbox(&line, 0, "");
        } else {
            self.f.line_len = len(lb);
            self.f.field_text = right(lb, self.f.line_len - (self.f.pos + 6));
            self.g.ind_name[ind] = self.f.field_text.clone();
        }
    }

    /// Port of NewGame_SetDebtByRating (0x478dce), GOSUB of the balance-sheet GOSUBs.
    /// `BankDebt` = a share of the asset base by credit rating (AAA: 5% with probability ~1/3, else
    /// unchanged; AA 15% … D 107%; another rating leaves it unchanged).
    fn set_debt_by_rating(&mut self) {
        let c = self.f.corp_id;
        let ab = self.f.asset_base;
        match self.f.rating_str.as_bytes() {
            b"AAA" => {
                self.f.rand_val = to_quad(Ext::from_f64(10.0) * self.rng.rnd());
                if Ext::from_f64(6.0) < cast::<Ext, _>(self.f.rand_val) {
                    self.g.bank_debt[c] = ab * s(0.05f32);
                }
            }
            b"AA" => self.g.bank_debt[c] = ab * s(0.15f32),
            b"A" => self.g.bank_debt[c] = ab * s(0.27f32),
            b"BBB" => self.g.bank_debt[c] = ab * s(0.43f32),
            b"BB" => self.g.bank_debt[c] = ab * s(0.56f32),
            b"B" => self.g.bank_debt[c] = ab * s(0.64f32),
            b"CCC" => self.g.bank_debt[c] = ab * s(0.71f32),
            b"CC" => self.g.bank_debt[c] = ab * s(0.8f32),
            b"C" => self.g.bank_debt[c] = ab * s(0.86f32),
            b"D" => self.g.bank_debt[c] = ab * s(1.07f32),
            _ => {}
        }
    }

    /// Port of NewGame_FinalizeWorld (0x4791c2), GOSUB of NewGameSetup.
    /// Completes the world after the data file: controllers and loan offers of the players, earnings
    /// histories, bank balance sheets (loans, deposits, gov bonds, reserves, funding), control and values
    /// of every company, bank asset allocation, bond issues and conversions by credit rating, share
    /// counts, minimum net worth of banks/insurers, country names, industry forecasts/growth/demand, the
    /// players' banks, holdings packing, gov bond costs, the conversion of all amounts into the game's
    /// currency, dividends, a few profit margins and the news-ticker header.
    fn finalize_world(&mut self) {
    let __content = crate::content::current();
        let secs = self.now.time_secs();
        let n = self.g.num_players;
        self.f.corp_id = 1;
        while self.f.corp_id <= n {
            let c = self.f.corp_id;
            self.g.corp_controller[c] = c;
            self.g.player_loan_offers_avail[c] = 6 - self.g.difficulty_level;
            self.f.corp_id += 1;
        }
        self.g.market_val_target = cast::<f64, _>(s(1.2f32));
        self.g.market_valuation = cast::<f32, _>(cast::<f32, _>(s(1.15f32)));
        self.f.corp_id = 11;
        while self.f.corp_id <= 1600 {
            let c = self.f.corp_id;
            // corpId = 1000: (UI) label 123 "    [runtime resource wording] ", SLEEP 100
            if self.g.entity_status[c] == 1 {
                self.finalize_corp(c, secs);
            }
            self.f.corp_id += 1;
        }
        self.f.corp_id = 11;
        while self.f.corp_id <= 85 {
            let c = self.f.corp_id;
            crate::logic::sim_entity::corp_ops::bank_allocate_assets(self.g, self.ui, self.rng, c, 1, 0);
            if self.g.entity_status[c] == 1 && self.g.net_worth < Ext::from_f64(0.0) {
                self.g.t_bills[c] = self.g.bad_debt_reserve[c] * Ext::from_f64(7.0) + self.g.t_bills[c];
                self.g.bad_debt_reserve[c] = Ext::from_f64(3.0) * self.g.bad_debt_reserve[c];
            }
            self.f.corp_id += 1;
        }
        self.f.corp_id = 11;
        while self.f.corp_id <= 1580 {
            let c = self.f.corp_id;
            if self.g.entity_status[c] == 1 {
                self.finalize_bonds_and_shares(c, secs);
            }
            self.f.corp_id += 1;
        }
        load_country_names(self.g);
        self.f.j = 3;
        while self.f.j <= 69 {
            let j = self.f.j;
            let vol = cast::<Ext, _>(self.g.ind_volatility[j]);
            self.f.tmp_long = to_quad(cast::<Ext, _>(self.g.ind_long_term_growth[j]) - vol * s(0.5f32));
            self.g.ind_growth_forecast[j] = cast::<i16, _>(self.f.tmp_long);
            let r = self.rng.rnd();
            self.f.tmp_long =
                to_quad(cast::<Ext, _>(self.g.ind_volatility[j]) * r + cast::<Ext, _>(self.g.ind_growth_forecast[j]));
            self.g.ind_growth_forecast[j] = cast::<i16, _>(self.f.tmp_long);
            let fc = cast::<Ext, _>(self.g.ind_growth_forecast[j]);
            let r = self.rng.rnd();
            self.f.tmp_long = to_quad(fc * (s(0.45f32) * r) + fc * s(0.8f32));
            self.g.ind_growth_rate[j] = cast::<i16, _>(self.f.tmp_long);
            self.f.corp_id = 131;
            while self.f.corp_id <= 1600 {
                let c = self.f.corp_id;
                if self.g.entity_status[c] == 1 && cast::<i32, _>(self.g.industry_num[c]) == j {
                    self.g.ind_demand[j] =
                        to_quad(self.g.business_assets[c] + cast::<Ext, _>(self.g.ind_demand[j]));
                }
                self.f.corp_id += 1;
            }
            let d = cast::<Ext, _>(self.g.ind_demand[j]);
            let r = self.rng.rnd();
            self.g.ind_demand[j] = to_quad(d * (s(0.5f32) * r) + d);
            self.f.j += 1;
        }
        self.f.j = 1;
        while self.f.j <= n {
            let j = self.f.j;
            self.f.rand_val = to_quad(cast::<Ext, _>(self.rng.rnd_range(60, 11)));
            self.g.rand_seed = self.g.rand_seed.wrapping_add(1);
            if (cast::<Ext, _>(self.f.rand_val)) < Ext::from_f64(11.0) {
                self.f.rand_val = 11;
            }
            if Ext::from_f64(60.0) < cast::<Ext, _>(self.f.rand_val) {
                self.f.rand_val = 60;
            }
            self.f.tmp_long = self.f.rand_val;
            self.g.lending_bank[j] = cast::<i16, _>(self.f.tmp_long);
            self.f.j += 1;
        }
        self.f.j = 11;
        while self.f.j <= 1600 {
            let j = self.f.j;
            if self.g.entity_status[j] == 1 && self.g.bonds_issued[j] != 0 {
                calc_entity_financials(self.g, self.ui, self.rng, j);
            }
            self.f.j += 1;
        }
        // The RecaptureExcessLoss news of PackHoldingArrays cannot occur here (no negative basis yet).
        pack_holding_arrays(self.g, self.ui);
        self.f.corp_id = 11;
        while self.f.corp_id <= 130 {
            let c = self.f.corp_id;
            if self.g.entity_status[c] == 1
                && !(self.g.long_gov_bond_face[c] == 0 && self.g.short_gov_bond_face[c] == 0)
            {
                if (cast::<Ext, _>(self.g.long_gov_bond_face[c])) < Ext::from_f64(0.0) {
                    self.msgbox(&st(__content.string(6709)), 0, "");
                }
                if (cast::<Ext, _>(self.g.short_gov_bond_face[c])) < Ext::from_f64(0.0) {
                    self.msgbox(&st(__content.string(6710)), 0, "");
                }
                self.g.lt_gov_bond_cost[c] = cast::<Ext, _>(self.g.long_gov_bond_face[c]);
                self.g.st_gov_bond_cost[c] = cast::<Ext, _>(self.g.short_gov_bond_face[c]);
            }
            self.f.corp_id += 1;
        }
        if !(self.g.currency_code == "USA") {
            self.convert_currency();
        }
        self.finalize_dividends();
        for id in [620, 621, 654, 657] {
            self.f.tmp_long = to_quad(Ext::from_f64(10.0) * self.rng.rnd() + Ext::from_f64(10.0));
            self.g.profit_margin[id] = cast::<i16, _>(self.f.tmp_long);
        }
        self.g.sim_co_count = 6;
        self.g.news_msg = st(
            "----------------------------------------------------------------------------------------------------------",
        );
        self.post();
        // SLEEP 2000
        self.g.news_msg = st(__content.string(6711));
        self.post();
        self.g.news_msg = st(
            "-----------------------------------------------------------------------------------------------------------",
        );
        self.post();
        self.g.world_generated = 1;
    }

    /// NewGame_FinalizeWorld, first company loop (0x47925e): earnings history, bank balance sheet,
    /// control and value of active company `c`.
    fn finalize_corp(&mut self, c: i32, secs: i32) {
        calc_entity_financials(self.g, self.ui, self.rng, c);
        self.f.asset_base = self.g.total_assets;
        if self.g.earnings_y1[c] == Ext::from_f64(0.0) {
            let ab = self.f.asset_base;
            let r = self.rng.rnd();
            self.g.earnings_y1[c] = ab * (s(0.37f32) * r) + -s(0.02f32) * ab;
        }
        if Ext::from_f64(0.0) < self.g.earnings_y1[c] || 70 < self.g.industry_num[c] {
            // LAB_00479577
            if self.g.industry_num[c] == 71 {
                self.g.earnings_y1[c] = Ext::from_f64(128.0) * self.rng.rnd();
                self.g.earnings_yr2_ago[c] = Ext::from_f64(85.0) * self.rng.rnd();
                self.g.earnings_yr3_ago[c] = Ext::from_f64(52.0) * self.rng.rnd();
                self.g.earnings_yr4_ago[c] = Ext::from_f64(170.0) * self.rng.rnd();
            } else {
                self.g.earnings_yr2_ago[c] = self.g.earnings_y1[c] * s(0.9f32);
                let e = self.g.earnings_y1[c];
                let r = self.rng.rnd();
                self.g.earnings_yr2_ago[c] = e * (s(1.2f32) * r) + self.g.earnings_yr2_ago[c] * s(0.4f32);
                self.g.earnings_yr3_ago[c] = self.g.earnings_yr2_ago[c] * s(0.9f32);
                let e = self.g.earnings_yr2_ago[c];
                let r = self.rng.rnd();
                self.g.earnings_yr3_ago[c] = e * (Ext::from_f64(1.0) * r) + self.g.earnings_yr3_ago[c] * s(0.5f32);
                self.g.earnings_yr4_ago[c] = self.g.earnings_yr3_ago[c] * s(0.9f32);
                let e = self.g.earnings_yr3_ago[c];
                let r = self.rng.rnd();
                self.g.earnings_yr4_ago[c] = e * (s(0.8f32) * r) + self.g.earnings_yr4_ago[c] * s(0.6f32);
                if !(Ext::from_f64(3.0) * self.rng.rnd() >= Ext::from_f64(1.0)) {
                    self.g.earnings_yr2_ago[c] =
                        self.g.earnings_yr2_ago[c] - self.g.earnings_yr3_ago[c] * s(1.2f32);
                    let e = self.g.earnings_yr3_ago[c];
                    let r = self.rng.rnd();
                    self.g.earnings_yr3_ago[c] = e * (s(1.3f32) * r) - Ext::from_f64(240.0);
                    let e = self.g.earnings_yr3_ago[c];
                    let r = self.rng.rnd();
                    self.g.earnings_yr4_ago[c] = e * (s(1.2f32) * r) - Ext::from_f64(125.0);
                }
                self.g.earnings_y1[c] =
                    (cast::<Ext, _>(self.g.difficulty_level) * s(0.1f32) + s(1.1f32)) * self.g.earnings_y1[c];
            }
        } else {
            if self.g.earnings_y1[c] == Ext::from_f64(0.0) {
                self.g.earnings_y1[c] = -Ext::from_f64(1.0) * (Ext::from_f64(1000.0) * self.rng.rnd());
            }
            self.g.tax_loss_carry[c] = self.g.earnings_y1[c];
            self.g.earnings_yr2_ago[c] = Ext::from_f64(1000.0) * self.rng.rnd() + self.g.earnings_y1[c];
            let r = self.rng.rnd();
            self.g.earnings_yr3_ago[c] = self.g.earnings_y1[c] - Ext::from_f64(1000.0) * r;
            self.g.earnings_yr4_ago[c] = Ext::from_f64(500.0) - Ext::from_f64(1000.0) * self.rng.rnd();
            if !(self.g.earnings_yr2_ago[c] >= Ext::from_f64(0.0)) {
                self.g.tax_loss_carry[c] = self.g.earnings_yr2_ago[c] + self.g.tax_loss_carry[c];
            }
        }
        if self.g.industry_num[c] == 1 {
            self.finalize_bank(c);
        }
        update_corp_control(self.g, self.ui, self.rng, c);
        self.g.valuation_mode = 3;
        calc_stock_price(self.g, self.ui, self.rng, c, secs);
        if Ext::from_f64(130.0) < self.g.tmp_share_price {
            check_stock_split(self.g, c);
        }
    }

    /// NewGame_FinalizeWorld: a bank's loans, deposits, gov bonds, reserves and funding (0x4799b5).
    fn finalize_bank(&mut self, c: i32) {
        let t = self.g.tmp_bank_total_loans;
        self.g.bad_debt_reserve[c] = self.g.cash[c] * s(0.02f32) + t * s(0.01f32);
        let r = self.rng.rnd();
        self.f.amount = to_quad(t * (s(0.28f32) * r) + t * s(0.9f32));
        let a = cast::<Ext, _>(self.f.amount);
        let r = self.rng.rnd();
        self.g.bank_demand_deposits[c] = a * (s(0.07f32) * r) + a * s(0.05f32);
        self.g.bank_c_ds[c] = cast::<Ext, _>(self.f.amount) - self.g.bank_demand_deposits[c];
        let r = self.rng.rnd();
        self.g.long_gov_bond_face[c] = to_quad(t * (s(0.01f32) * r) + t * s(0.01f32));
        let r = self.rng.rnd();
        self.g.short_gov_bond_face[c] =
            to_quad(cast::<Ext, _>(self.g.long_gov_bond_face[c]) * (Ext::from_f64(1.0) * r + Ext::from_f64(3.0)));
        let r = self.rng.rnd();
        self.g.bank_consumer_loans[c] = to_quad(t * (s(0.4f32) * r) + t * s(0.2f32));
        let r = self.rng.rnd();
        self.g.bank_mortgage_loans[c] = to_quad(t * (s(0.25f32) * r) + self.g.cash[c] * s(0.15f32));
        let r = self.rng.rnd();
        self.g.subprime_secs[c] = to_quad(t * (Ext::from_f64(1.0) * r) + t * s(0.3f32));
        self.g.bad_debt_reserve[c] = ((cast::<Ext, _>(self.g.bank_consumer_loans[c]) * s(0.05f32)
            + self.g.bad_debt_reserve[c])
            + cast::<Ext, _>(self.g.bank_mortgage_loans[c]) * s(0.01f32))
            + cast::<Ext, _>(self.g.subprime_secs[c]) * s(0.03f32);
        self.g.tmp_bank_total_loans = ((cast::<Ext, _>(self.g.bank_consumer_loans[c])
            + self.g.tmp_bank_total_loans)
            + cast::<Ext, _>(self.g.bank_mortgage_loans[c]))
            + cast::<Ext, _>(self.g.subprime_secs[c]);
        let t = self.g.tmp_bank_total_loans;
        if self.g.cash[c] > t * s(0.065f32) {
            let r = self.rng.rnd();
            self.g.cash[c] = t * (s(0.03f32) * r + s(0.05f32));
        }
        self.g.bonds_issued[c] = to_quad(Ext::from_f64(3.0) * cast::<Ext, _>(self.g.bonds_issued[c]));
        calc_entity_financials(self.g, self.ui, self.rng, c);
        self.f.cust_deposits = self.g.bank_cust_deposits[c];
        if !(self.f.cust_deposits >= Ext::from_f64(0.0)) {
            self.f.cust_deposits = Ext::from_f64(0.0);
        }
        self.f.total_funding = ((self.g.bank_demand_deposits[c] + self.g.bank_c_ds[c])
            + self.f.cust_deposits)
            + self.g.tmp_total_debt;
        self.f.coupon_rate = cast::<f32, _>(cast::<Ext, _>(self.g.difficulty_level) * s(0.02f32) + s(0.78f32));
        while !(self.f.total_funding >= self.g.total_assets * cast::<Ext, _>(self.f.coupon_rate)) {
            self.f.amount = to_quad(self.g.total_assets * s(0.1f32));
            self.f.total_funding = cast::<Ext, _>(self.f.amount) + self.f.total_funding;
            let a = cast::<Ext, _>(self.f.amount);
            let r = self.rng.rnd();
            self.f.sub_amount = to_quad(a * (s(0.06f32) * r) + a * s(0.09f32));
            self.g.bank_demand_deposits[c] =
                cast::<Ext, _>(self.f.sub_amount) + self.g.bank_demand_deposits[c];
            self.f.amount = to_quad(cast::<Ext, _>(self.f.amount) - cast::<Ext, _>(self.f.sub_amount));
            self.g.bank_c_ds[c] = cast::<Ext, _>(self.f.amount) + self.g.bank_c_ds[c];
        }
    }

    /// NewGame_FinalizeWorld, third company loop (0x47a0d0): bond conversions/issues by credit rating,
    /// bond maturities, the value (and a share-count quirk), the minimum net worth of financial
    /// companies and the prior net worth of company `c`.
    fn finalize_bonds_and_shares(&mut self, c: i32, secs: i32) {
        calc_entity_financials(self.g, self.ui, self.rng, c);
        self.f.rand_val = to_quad(Ext::from_f64(5.0) * self.rng.rnd());
        if !(Ext::from_f64(2.0) > cast::<Ext, _>(self.f.rand_val)) {
            if !(5 >= self.g.credit_rating_num[c]) {
                // credit rating 6..: issue bonds
                calc_entity_financials(self.g, self.ui, self.rng, c);
                let nw = self.g.net_worth;
                let r = self.rng.rnd();
                self.f.amount = to_quad(nw * (s(0.3f32) * r) + nw * s(0.2f32));
                if self.g.difficulty_level == 2 {
                    self.f.amount = to_quad(cast::<Ext, _>(self.f.amount) * s(0.8f32));
                }
                if self.g.difficulty_level == 1 {
                    self.f.amount = to_quad(cast::<Ext, _>(self.f.amount) * s(0.6f32));
                }
                if self.g.industry_num[c] == 1 {
                    self.f.amount = to_quad(s(1.8f32) * cast::<Ext, _>(self.f.amount));
                }
                if !((cast::<Ext, _>(self.f.amount)) < self.g.exchange_rate * Ext::from_f64(100.0)) {
                    self.g.bonds_issued[c] =
                        to_quad(cast::<Ext, _>(self.f.amount) + cast::<Ext, _>(self.g.bonds_issued[c]));
                    let cy = Ext::from_f64(3.0) + cast::<Ext, _>(self.g.cur_year);
                    self.f.rand_val = to_quad(cy + Ext::from_f64(17.0) * self.rng.rnd());
                    if self.g.credit_rating_num[c] < 8 {
                        let cy = Ext::from_f64(1.0) + cast::<Ext, _>(self.g.cur_year);
                        self.f.rand_val = to_quad(cy + Ext::from_f64(9.0) * self.rng.rnd());
                    }
                    self.f.year_text = trim(str15(cast::<Ext, _>(self.f.rand_val)).as_bytes());
                    self.f.amount = to_quad(Ext::from_f64(70.0) * self.rng.rnd() + Ext::from_f64(110.0));
                    self.f.coupon_rate = cast::<f32, _>(cast::<Ext, _>(self.f.amount) / Ext::from_f64(20.0));
                    let cr = self.g.credit_rating_str.clone();
                    match cr.as_bytes() {
                        b"AAA" | b"AA" | b"A" => {
                            if self.g.bond_issue_info[c].is_empty() {
                                self.g.bonds_issued[c] =
                                    to_quad(s(1.2f32) * cast::<Ext, _>(self.g.bonds_issued[c]));
                                self.f.field_text =
                                    trim(fmt(cast::<Ext, _>(self.f.coupon_rate), "0#.00").as_bytes());
                                self.g.bond_issue_info[c] =
                                    self.f.year_text.clone() + "@" + &self.f.field_text;
                            }
                        }
                        b"BBB" | b"BB" => {
                            if self.g.bond_issue_info[c].is_empty() {
                                self.f.coupon_rate = cast::<f32, _>(s(1.25f32) + cast::<Ext, _>(self.f.coupon_rate));
                                self.f.field_text =
                                    trim(fmt(cast::<Ext, _>(self.f.coupon_rate), "0#.00").as_bytes());
                                self.g.bond_issue_info[c] =
                                    self.f.year_text.clone() + "@" + &self.f.field_text;
                            }
                        }
                        _ => {}
                    }
                    if Ext::from_f64(50000.0) < cast::<Ext, _>(self.g.bonds_issued[c]) {
                        self.g.bonds_issued[c] = to_quad(Ext::from_f64(5000.0) * self.rng.rnd() + Ext::from_f64(40000.0));
                    }
                }
            } else if self.g.credit_rating_num[c] < 5
                && Ext::from_f64(500.0) < cast::<Ext, _>(self.g.bonds_issued[c])
                && 60 < c
                && self.g.bank_debt[c] * s(0.4f32) < cast::<Ext, _>(self.g.bonds_issued[c])
            {
                // good rating: convert 70% of the bonds into bank debt
                self.f.amount = to_quad(cast::<Ext, _>(self.g.bonds_issued[c]) * s(0.7f32));
                self.g.bonds_issued[c] =
                    to_quad(cast::<Ext, _>(self.g.bonds_issued[c]) - cast::<Ext, _>(self.f.amount));
                self.g.bank_debt[c] = cast::<Ext, _>(self.f.amount) + self.g.bank_debt[c];
            }
            if Ext::from_f64(0.0) < cast::<Ext, _>(self.g.bonds_issued[c]) {
                if val(mid(self.g.bond_issue_info[c].as_bytes(), 1, 4).as_bytes())
                    < cast::<Ext, _>(self.g.start_year)
                {
                    let k = cast::<Ext, _>(self.rng.rnd_range(8, 2));
                    self.f.field_text = trim(fmt(k + cast::<Ext, _>(self.g.start_year), "####").as_bytes());
                    mid_assign(
                        &mut self.g.bond_issue_info[c],
                        1,
                        4,
                        self.f.field_text.as_bytes(),
                    );
                }
                self.g.bond_price[c] = Cur::from_f64(Ext::from_f64(100.0));
            }
        }
        self.g.valuation_mode = 3;
        calc_stock_price(self.g, self.ui, self.rng, c, secs);
        if Ext::from_f64(240.0) < self.g.tmp_share_price {
            // BUG (BUGS.md): a comparison instead of `SharesOutstanding * 5`: the share count becomes 0
            let so = self.g.shares_outstanding[c];
            self.g.shares_outstanding[c] = if self.options.repair_initial_shares {
                so * Ext::from_f64(5.0)
            } else if so == so * Ext::from_f64(5.0) { -Ext::from_f64(1.0) } else { Ext::from_f64(0.0) };
        }
        if self.g.industry_num[c] < 3 {
            calc_entity_financials(self.g, self.ui, self.rng, c);
            while !(self.g.net_worth >= Ext::from_f64(500.0)) {
                self.g.cash[c] = Ext::from_f64(500.0) + self.g.cash[c];
                self.g.net_worth = Ext::from_f64(500.0) + self.g.net_worth;
            }
        }
        self.g.corp_prior_net_worth[c] = self.g.net_worth;
    }

    /// NewGame_FinalizeWorld: all amounts from U.S. dollars into the game's currency (0x47afcb).
    fn convert_currency(&mut self) {
        self.f.corp_id = 1;
        while self.f.corp_id <= 1600 {
            let c = self.f.corp_id;
            let er = self.g.exchange_rate;
            if 2 < c && c < 70 {
                self.g.ind_demand[c] = to_quad(er * cast::<Ext, _>(self.g.ind_demand[c]));
            }
            let skip =
                !(c <= self.g.num_players || 10 < c) || (10 < c && self.g.entity_status[c] != 1);
            if !skip {
                self.g.cash[c] = er * self.g.cash[c];
                self.g.bank_debt[c] = er * self.g.bank_debt[c];
                if 10 < c && 0 < self.g.hold_stock_id[(0, c)] {
                    let n = cast::<i32, _>(self.g.hold_stock_id[(0, c)]);
                    self.f.hold_idx = 1;
                    while self.f.hold_idx <= n {
                        let h = self.f.hold_idx;
                        self.g.hold_stock_basis[(h, c)] = er * self.g.hold_stock_basis[(h, c)];
                        self.f.hold_idx += 1;
                    }
                }
                if c < 131 {
                    self.g.long_gov_bond_face[c] =
                        to_quad(er * cast::<Ext, _>(self.g.long_gov_bond_face[c]));
                    self.g.lt_gov_bond_cost[c] = er * self.g.lt_gov_bond_cost[c];
                    self.g.short_gov_bond_face[c] =
                        to_quad(er * cast::<Ext, _>(self.g.short_gov_bond_face[c]));
                    self.g.st_gov_bond_cost[c] = er * self.g.st_gov_bond_cost[c];
                }
                if 10 < c {
                    self.g.market_cap[c] = er * self.g.market_cap[c];
                    self.g.shares_outstanding[c] = er * self.g.shares_outstanding[c];
                    self.g.earnings_y1[c] = er * self.g.earnings_y1[c];
                    self.g.earnings_yr2_ago[c] = er * self.g.earnings_yr2_ago[c];
                    self.g.earnings_yr3_ago[c] = er * self.g.earnings_yr3_ago[c];
                    self.g.earnings_yr4_ago[c] = er * self.g.earnings_yr4_ago[c];
                    self.g.tax_loss_carry[c] = er * self.g.tax_loss_carry[c];
                    self.g.bonds_issued[c] = to_quad(er * cast::<Ext, _>(self.g.bonds_issued[c]));
                    if c < 86 {
                        self.g.bad_debt_reserve[c] = er * self.g.bad_debt_reserve[c];
                        self.g.bank_c_ds[c] = er * self.g.bank_c_ds[c];
                        self.g.bank_cust_deposits[c] = er * self.g.bank_cust_deposits[c];
                        self.g.bank_demand_deposits[c] = er * self.g.bank_demand_deposits[c];
                        self.g.bank_consumer_loans[c] =
                            to_quad(er * cast::<Ext, _>(self.g.bank_consumer_loans[c]));
                        self.g.bank_mortgage_loans[c] =
                            to_quad(er * cast::<Ext, _>(self.g.bank_mortgage_loans[c]));
                        self.g.subprime_secs[c] = to_quad(er * cast::<Ext, _>(self.g.subprime_secs[c]));
                    }
                }
                if 85 < c && c < 131 {
                    self.g.ins_reserves[c] = er * self.g.ins_reserves[c];
                }
                if 130 < c && self.g.industry_num[c] < 70 {
                    self.g.working_capital[c] = er * self.g.working_capital[c];
                    self.g.business_assets[c] = er * self.g.business_assets[c];
                    if !(self.g.business_assets[c] >= Ext::from_f64(100.0)) {
                        self.g.business_assets[c] = Ext::from_f64(20.0) * self.rng.rnd() + Ext::from_f64(100.0);
                    }
                }
            }
            self.f.corp_id += 1;
        }
    }

    /// NewGame_FinalizeWorld: dividends per share of the companies 11..1580 (0x47b813).
    fn finalize_dividends(&mut self) {
        self.f.corp_id = 11;
        while self.f.corp_id <= 1580 {
            let c = self.f.corp_id;
            if self.g.entity_status[c] == 1 && self.g.dividend_per_share[c] != Ext::from_f64(0.0) {
                if Ext::from_f64(10.0) < self.g.earnings_y1[c] {
                    self.f.sub_amount =
                        to_quad(self.g.earnings_y1[c] * (self.g.dividend_per_share[c] * s(0.01f32)));
                    self.f.div_yield = cast::<Ext, _>(self.f.sub_amount) / (self.g.market_cap[c] * Ext::from_f64(100.0));
                    self.g.tmp_share_price = self.g.market_cap[c] / self.g.shares_outstanding[c];
                    self.g.dividend_per_share[c] = self.g.tmp_share_price * self.f.div_yield;
                } else {
                    self.f.div_yield = Ext::from_f64(10.0);
                    while !(Ext::from_f64(6.0) >= self.f.div_yield) {
                        self.f.div_yield = calc_dividend_yield(self.g, c);
                        if !(s(5.95f32) >= self.f.div_yield) {
                            let r = self.rng.rnd();
                            self.g.dividend_per_share[c] =
                                (s(0.3f32) * r + s(0.4f32)) * self.g.dividend_per_share[c];
                        }
                    }
                    if self.g.industry_num[c] == 69 {
                        self.g.dividend_per_share[c] = Ext::from_f64(2.0) * self.g.dividend_per_share[c];
                        if calc_dividend_yield(self.g, c) > Ext::from_f64(10.0) {
                            self.g.dividend_per_share[c] = s(0.7f32) * self.g.dividend_per_share[c];
                        }
                    }
                }
            }
            self.f.corp_id += 1;
        }
    }

    /// Port of NewGame_SetTurnOrder (0x47bcbe), GOSUB of NewGameSetup.
    /// Reseeds RND from `RandSeed`, picks a random first player (player 1 if that one is a computer
    /// player) and the order of the others, announces who plays first and, when that is a computer
    /// player, lets it move (PORT-TODO) and hands over to the next player.
    fn set_turn_order(&mut self) {
    let __content = crate::content::current();
        self.g.rand_seed = to_long(cast::<Ext, _>(self.now.time_secs()) + cast::<Ext, _>(self.g.rand_seed));
        if 32500 < self.g.rand_seed {
            self.g.rand_seed = 10;
        }
        self.rng.randomize(cast::<f64, _>(cast::<Ext, _>(self.g.rand_seed)));
        self.f.amount = cast::<i64, _>(self.g.num_players);
        self.f.j = 0;
        self.f.rand_val = cast::<i64, _>(self.rng.rnd_range(self.g.num_players, 1));
        if (cast::<Ext, _>(self.f.amount)) < cast::<Ext, _>(self.f.rand_val) {
            self.f.rand_val = cast::<i64, _>(self.g.num_players);
        }
        self.f.tmp_long = self.f.rand_val;
        if self.g.is_computer_player[cast::<i32, _>(self.f.tmp_long)] == 1 {
            self.f.rand_val = 1;
        }
        self.f.tmp_long = self.f.rand_val;
        self.g.player_turn_order[1] = cast::<i32, _>(self.f.tmp_long);
        let amount = self.f.amount;
        for k in 2..=5 {
            if k > 2 && amount == cast::<i64, _>(k - 1) {
                break;
            }
            self.f.rand_val += 1;
            if (cast::<Ext, _>(amount)) < cast::<Ext, _>(self.f.rand_val) {
                self.f.rand_val = 1;
            }
            self.f.tmp_long = self.f.rand_val;
            self.g.player_turn_order[k] = cast::<i32, _>(self.f.tmp_long);
        }
        self.g.cur_player = self.g.player_turn_order[1];
        self.g.cur_player_name = self.g.player_name[self.g.cur_player].clone();
        // (UI) label 140 "Player:  " + CurPlayerName
        let m = self.g.cur_player_name.clone() + __content.string(6712);
        let title = self.g.app_title.clone();
        self.ui.msgbox(&m, 8192, &title);
        self.f.corp_id = self.g.cur_player;
        self.g.active_entity_name = self.g.cur_player_name.clone();
        self.g.active_entity = self.f.corp_id;
        if self.g.is_computer_player[self.g.cur_player] == 1 && self.g.has_cpu_players == "Y" {
            let now = self.now;
            crate::logic::ai_player::ai_computer_player_turn(self.g, self.ui, self.rng, &now);
            self.g.turn_action_count = 0;
            self.g.action_code = PbStr::new();
            let next = self.g.player_turn_order[2];
            self.f.field_text = self.g.ent_name[next].clone();
            self.g.active_entity_name = self.f.field_text.clone();
            self.g.active_entity = self.g.player_turn_order[2];
            let m = st(__content.string(6713))
                + &self.g.cur_player_name
                + __content.string(6714)
                + "\r"
                + "\r"
                + __content.string(6715)
                + &self.f.field_text
                + __content.string(6716);
            self.ui.msgbox(&m, 8192, &title);
            self.g.cur_player_name = self.g.active_entity_name.clone();
            self.g.cur_player = self.g.active_entity;
            // (UI) label 140
        }
    }
}

/// Port of InBox (0x423170) in mode 2 (the number of players), with the state effects of
/// CreateSymbolEntryDialog (0x419674), InBox_SetupPrompt (0x42351f) and the OK / Cancel buttons
/// (0x41a85f / 0x41a961).
///
/// The dialog is asked through [`Ui::inputbox`]: prompt = `InputDlgPrompt` + "Click 'OK' to enter.",
/// title = `InputDlgLabel`, default = the current `NumPlayers` (2 unless 1..5). An empty answer is Cancel
/// (`SymbolDlgText = "Cancel"`); otherwise `NumPlayers = VAL(answer)`, with a message unless 2..5.
pub fn in_box_num_players(g: &mut Globals, ui: &mut dyn Ui) {
    let __content = crate::content::current();
    // ON ERROR GOTO: ShowRuntimeError (no error possible in this mode)
    g.in_box_mode = 2;
    // dlgMode = 2 (ActionCode = "GETDATA")
    g.symbol_dlg_text = PbStr::new();
    if g.symbol_dlg_open == 1 {
        // DIALOG END DlgSymbolEntry
        g.symbol_dlg_open = 0;
    }
    // CreateSymbolEntryDialog: with ActionCode "GETDATA" no symbol prefill state changes; InBox_SetupPrompt:
    let mut players_text = st("2");
    if 0 < g.num_players && g.num_players < 6 {
        players_text = trim(str15(cast::<Ext, _>(g.num_players)).as_bytes());
    }
    let prompt = g.input_dlg_prompt.clone() + __content.string(6717);
    g.symbol_dlg_open = 1;
    let answer = ui.inputbox(&prompt, &g.input_dlg_label, &players_text);
    if answer.is_empty() {
        // SymbolEntry_CancelBtn
        g.symbol_dlg_text = st("Cancel");
        g.target_corp = 0;
        g.lookup_entity_name = PbStr::new();
        g.tmp_symbol = PbStr::new();
    } else {
        // SymbolEntry_OkBtn
        g.symbol_dlg_text = answer;
    }
    g.symbol_dlg_open = 0;
    if g.ticker_on == 1 {
        g.clock_state = st("RUN");
    }
    g.symbol_dlg_open = 0;
    if !(g.symbol_dlg_text == "Cancel") {
        g.num_players = to_long(val(g.symbol_dlg_text.as_bytes()));
        if g.num_players < 2 || 5 < g.num_players {
            let mut m = st(__content.string(6718));
            show_quick_message(g, ui, &mut m);
        }
    }
    g.in_box_mode = 0;
}

/// Port of ShowStartupDialog (0x41aa1c) with its OK button Startup_OkBtn (0x41c244).
/// The startup choices: player names (`PrefPlayerNames`, "Wally Raider" for a computer player 1 without
/// one), starting money (100..1000 million US, default 500 or `StartNetWorthCfg`), game length (1..999
/// years, default `CfgGameLengthYears` or 999; over 35 asks to confirm the disqualified score) and
/// difficulty (1..4, default `DifficultyLevel`, set to 2 if 0).
///
/// Interactive hosts show one [`Ui::form`] containing every text box. Scripted hosts retain the
/// inputbox fallback. Invalid submissions preserve every field, including the corrected length
/// after declining a long game. Closing the form is distinct from submitting an empty field.
///
/// Returns the contents of `cfig.wsr` that SaveConfig wrote when OK ended the dialog (None if closed).
pub fn show_startup_dialog(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> Option<Vec<u8>> {
    let __content = crate::content::current();
    let n = g.num_players;
    let cpu = g.has_cpu_players == "Y";
    let mut boxes: Vec<(i32, PbStr, PbStr)> = Vec::new();
    let name1 = if g.is_computer_player[1] == 1 && g.pref_player_names[1].is_empty() {
        st(__content.string(6719))
    } else {
        g.pref_player_names[1].clone()
    };
    let label = |k: i32, comp: bool| {
        let mut l = st("Player ") + &PbStr::from(k.to_string().as_str()) + ":";
        if comp {
            l = l + __content.string(6720);
        }
        l
    };
    boxes.push((342, label(1, cpu), name1));
    boxes.push((344, label(2, false), g.pref_player_names[2].clone()));
    for k in 3..=5 {
        if k <= n {
            boxes.push((
                340 + 2 * k,
                label(k, cpu && g.is_computer_player[k] == 1),
                g.pref_player_names[k].clone(),
            ));
        }
    }
    let mut money = st("500");
    if 0 < g.start_net_worth_cfg {
        money = trim(str15(cast::<Ext, _>(g.start_net_worth_cfg)).as_bytes());
    }
    boxes.push((
        352,
        st(__content.string(6721)),
        money,
    ));
    let mut years = st("999");
    if 0 < g.cfg_game_length_years && !(years == "2") {
        years = trim(str15(cast::<Ext, _>(g.cfg_game_length_years)).as_bytes());
    }
    boxes.push((354, st("Game Length: (1 to 999 years)"), years));
    if g.difficulty_level == 0 {
        g.difficulty_level = 2;
    }
    let level = trim(str15(cast::<Ext, _>(g.difficulty_level)).as_bytes());
    boxes.push((356, st(__content.string(6722)), level));
    let title = st(__content.string(6723));
    loop {
        use crate::ui::{Form, FormField};
        let form = Form {
            title: title.clone(),
            fields: boxes.iter().map(|(id, label, value)| {
                let name = *id <= 350;
                FormField {
                    id: *id, label: label.clone(), value: value.clone(),
                    min_length: name.then_some(5), max_length: name.then_some(20),
                    numeric: !name,
                    min: match id { 352 => Some(100), 354 | 356 => Some(1), _ => None },
                    max: match id { 352 => Some(1000), 354 => Some(999), 356 => Some(4), _ => None },
                    ..Default::default()
                }
            }).collect(),
            buttons: vec![(1, st("OK")), (-1, st("Cancel"))],
            ..Default::default()
        };
        if let Some(response) = ui.form(&form) {
            if response.button != 1 || response.values.len() != boxes.len() { return None; }
            for (b, mut value) in boxes.iter_mut().zip(response.values) {
                if b.0 <= 350 { value.0.truncate(20); }
                b.2 = value;
            }
        } else {
            for b in boxes.iter_mut() {
                let a = ui.inputbox(&b.1, &title, &b.2);
                if a.is_empty() { return None; }
                b.2 = a;
            }
        }
        let text = |id: i32| {
            boxes
                .iter()
                .find(|b| b.0 == id)
                .map(|b| b.2.clone())
                .unwrap_or_default()
        };
        let mut set_354 = None;
        let saved = startup_ok_btn(g, ui, rng, &text, &mut set_354);
        if let Some(t) = set_354 {
            if let Some(b) = boxes.iter_mut().find(|b| b.0 == 354) {
                b.2 = t;
            }
        }
        if saved.is_some() {
            return saved;
        }
    }
}

/// Port of Startup_OkBtn (0x41c244): validates and stores the startup choices read from the dialog's text
/// boxes (`text(id)`). When all are valid it saves the configuration (SaveConfig) and ends the dialog:
/// returns the `cfig.wsr` contents, else None. `set_354` receives a new text for the game-length box
/// ("35" after the user declines a game over 35 years).
fn startup_ok_btn(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    text: &dyn Fn(i32) -> PbStr,
    set_354: &mut Option<PbStr>,
) -> Option<Vec<u8>> {
    let __content = crate::content::current();
    for v in 1..=5 {
        g.ent_name[v] = PbStr::new();
    }
    let prompt_prefix = st(__content.string(6724));
    let prompt_suffix = st(__content.string(6725));
    let n = g.num_players;
    for k in 1..=5 {
        if k > 2 && n < k {
            break;
        }
        g.ent_name[k] = text(340 + 2 * k);
        let name_text = trim(g.ent_name[k].as_bytes());
        if name_text.is_empty() || len(name_text.as_bytes()) < 5 {
            let mut m =
                prompt_prefix.clone() + &PbStr::from(k.to_string().as_str()) + &prompt_suffix;
            show_quick_message(g, ui, &mut m);
            // CONTROL SET FOCUS (UI)
            return None;
        }
    }
    for v in 1..=n {
        if 0 < instr(1, g.ent_name[v].as_bytes(), b"|") {
            g.ent_name[v] = replace_any(g.ent_name[v].as_bytes(), b"|", b" ");
        }
        if 0 < instr(1, g.ent_name[v].as_bytes(), b"\"") {
            g.ent_name[v] = replace_any(g.ent_name[v].as_bytes(), b"\"", b" ");
        }
        if instr(1, g.ent_name[v].as_bytes(), b"  ") != 0 {
            g.ent_name[v] = replace(g.ent_name[v].as_bytes(), b"  ", b" ");
        }
        g.ent_name[v] = trim(g.ent_name[v].as_bytes());
        g.player_name[v] = g.ent_name[v].clone();
        if !g.ent_name[v].is_empty() {
            g.pref_player_names[v] = g.ent_name[v].clone();
        }
    }
    let field_text = text(352);
    g.start_net_worth_cfg = to_long(val(field_text.as_bytes()));
    if g.start_net_worth_cfg < 100 || 1000 < g.start_net_worth_cfg {
        let mut m = st(__content.string(6726));
        show_quick_message(g, ui, &mut m);
        return None;
    }
    g.start_net_worth = g.start_net_worth_cfg;
    for v in 1..=n {
        g.cash[v] = cast::<Ext, _>(g.start_net_worth_cfg);
    }
    let field_text = text(354);
    let value = to_long(val(field_text.as_bytes()));
    let max_years = 999;
    if 35 < value && value < 1000 {
        let confirm = st(
            __content.string(6727),
        ) + __content.string(6728)
            + __content.string(6729)
            + "\r"
            + "\r"
            + __content.string(6730);
        let answer = ui.msgbox(&confirm, 8484, &st(__content.string(6731)));
        if answer == 7 {
            let mut m = st(__content.string(6732));
            show_quick_message(g, ui, &mut m);
            *set_354 = Some(st("35"));
            return None;
        }
    }
    if value < 1 || max_years < value {
        let mut m = st(__content.string(6733)) + &str15(cast::<Ext, _>(max_years)) + " years.";
        show_quick_message(g, ui, &mut m);
        return None;
    }
    g.game_end_year = value;
    g.cfg_game_length_years = value;
    if 35 < value && value < 1000 {
        g.long_game_marker = cast::<i16, _>(g.rand_seed % 5) + 90;
    }
    let field_text = text(356);
    let value = to_long(val(field_text.as_bytes()));
    if value < 1 || 4 < value {
        let mut m =
            st(__content.string(6734));
        show_quick_message(g, ui, &mut m);
        return None;
    }
    g.difficulty_level = value;
    if value == 4 {
        g.difficulty_level = 3;
    }
    g.cfg_difficulty_raw = value;
    let dl = cast::<Ext, _>(g.difficulty_level);
    let r = rng.rnd();
    g.market_valuation = cast::<f32, _>(cast::<f32, _>((dl * s(0.25f32)) * r + s(0.65f32)));
    let cfg = save_config(g); // SaveConfig(PrefCurrency)
    // DIALOG END
    Some(cfg)
}

/// Port of CreateIndustryCombo (0x46dc2b), state effects only.
/// `IndComboItems(1..71)` = the industry names padded with 75 blanks and the 2-digit industry number,
/// sorted (`ARRAY SORT`); `IndComboItems(0)` = the "SELECT INDUSTRY" entry. The label and combo box are UI
/// (SelectIndustryComboItem only selects the item and calls EnableMainControls).
pub fn create_industry_combo(g: &mut Globals) {
    let __content = crate::content::current();
    let mut items: Vec<PbStr> = Vec::with_capacity(71);
    for i in 1..=71 {
        let suffix = space(75) + &fmt(cast::<Ext, _>(i), "0#");
        items.push(g.ind_name[i].clone() + &suffix);
    }
    items.sort_by(|a, b| compare(a.as_bytes(), b.as_bytes()));
    g.ind_combo_items[0] = st(__content.string(6735)) + &space(53) + "00";
    for (k, it) in items.into_iter().enumerate() {
        g.ind_combo_items[cast::<i32, _>(k) + 1] = it;
    }
}
