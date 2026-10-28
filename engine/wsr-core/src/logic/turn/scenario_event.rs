//! `RunScenarioEvent_689361` (0x689361): the scripted scenario ("adventure") events. One call runs one
//! step of scenario `ScenarioNum` for company `ScenarioCorp`: a positive number is the proposal (an
//! "internal memo" asking the controlling player YES/NO), the negated number the later outcome ("trading
//! halted ... business news update"). The texts come from the encoded files `SCENWIN1..4.WSR`
//! ([`super::set_scenwin_file`]); the case bodies (in `scenario_cases_*`) set up the story, apply the
//! cash / P&L / margin / market-cap effects and leave headlines for the later stages.
//!
//! The routine's locals are the fields of [`Ev`] (its GOSUB blocks run on the same frame).
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use crate::logic::control::update_corp_control;
use crate::logic::report::{number_to_words, show_quick_message, show_report_dialog};
use crate::logic::valuation::update_price_history_tick;
use crate::pb::numeric::InputReader;
use crate::pb::string::{asc, chr, mid_assign};

/// The locals of `RunScenarioEvent_689361` (ebp offsets), shared with its GOSUB blocks.
#[derive(Default)]
pub(super) struct Ev {
    /// `pricePct` (the BYREF parameter): the stock's price change over two months (Scenario_CheckPriceMove).
    pub price_pct: i32,
    /// `textArr(1 TO 200)` (-0x1e4): the event text read from SCENWINn.WSR.
    pub text_arr: Vec<PbStr>,
    /// -0x158: 0 = new proposal, 1 = running (turned into the outcome, 2), 2 = outcome, -1/-2 aborted.
    pub event_state: i32,
    /// -0x154 (QUAD): year for `@YEAR` (0 = keep `detail_text`).
    pub event_year: i64,
    /// -0x14c: the company's market cap (written back at the end).
    pub market_cap: Ext,
    /// -0x140 (QUAD): `ROUND(pricePct)`.
    pub pct_move: i64,
    /// -0x138 (INTEGER): the player's notoriety (written back to `PlayerNotoriety`).
    pub notoriety: i16,
    /// -0x134
    pub country_idx: i32,
    /// -0x130: the new profit margin (to `TmpExt1`, applied by Scenario_ApplyNewsEffects).
    pub margin: i32,
    /// -0x12c (INTEGER)
    pub industry: i16,
    /// -0x128
    pub nation_name: PbStr,
    /// -0x124
    pub tire_word_lc: PbStr,
    /// -0x120 (`@TEXT1`)
    pub tire_word: PbStr,
    /// -0x11c (`@CHEATER`)
    pub player_name: PbStr,
    /// -0x118 (`@MISC`)
    pub misc_text: PbStr,
    /// -0x114 (`@CHEATSYM`)
    pub corp_symbol: PbStr,
    /// -0x110 (`@CHEATNAM`)
    pub corp_name: PbStr,
    /// -0x10c (`@CHEATIND`)
    pub industry_name: PbStr,
    /// -0x108: "Y" if the event player is the company's CEO, else "N".
    pub is_ceo: PbStr,
    /// -0x104: "Y" for the alternative story variant.
    pub show_text: PbStr,
    /// -0xfc (INTEGER): `@IT` section number while reading; the last character code after decoding.
    pub char_code: i16,
    /// -0xf8 (QUAD)
    pub cost_amt: i64,
    /// -0xf0
    pub abs_val: i32,
    /// -0xec
    pub corp_arg: i32,
    /// -0xe8
    pub char_pos: i32,
    /// -0xe4
    pub line_no: i32,
    /// -0xe0 (QUAD, `@LOSS`)
    pub loss_amt: i64,
    /// -0xd8
    pub line_len: i32,
    /// -0xd4
    pub line_count: i32,
    /// -0xd0 (INTEGER): the text section to show.
    pub text_id: i16,
    /// -0xcc
    pub in_section: i32,
    /// -0xc8
    pub file_num: i32,
    /// -0xc4: `VAL(RIGHT$(TIME$, 2))`.
    pub clock_sec: i32,
    /// -0xc0: `VAL(MID$(DATE$, 4, 2))` (day of month).
    pub day_num: i32,
    /// -0xbc
    pub line_idx: i32,
    /// -0xb8 (`@AMOUNT`): the amount charged by ChargeExpense / ChargeExtraordinary.
    pub amount: Ext,
    /// -0xac (`@YEAR`)
    pub detail_text: PbStr,
    /// -0xa8: also the character temp of DecryptText.
    pub nation_adj: PbStr,
    /// -0xa4
    pub out_char: PbStr,
    /// -0xa0
    pub line_text: PbStr,
    /// -0x9c (`@LOSS`)
    pub loss_text: PbStr,
    /// -0x98: `NewsMsg = corpSymbol + " " + newsTail` at the end.
    pub news_tail: PbStr,
    /// -0x94
    pub month_text: PbStr,
    /// -0x90
    pub phrase: PbStr,
    /// -0x8c (`@AMOUNT`)
    pub amount_text: PbStr,
    /// -0x88
    pub aux_text: PbStr,
    /// -0x5c (QUAD)
    pub event_idx: i64,
    /// -0x2c
    pub cmp_arg: i32,
}

/// Port of RunScenarioEvent_689361 (0x689361).
/// Runs scenario `ScenarioNum` (> 0: the proposal memo with YES/NO; < 0: the outcome) for
/// `ScenarioCorp`, `price_pct` = the stock's recent price change (for the "trading halted" comment).
///
/// * An event already finished (`EventStatus = 2`) sets `ScenarioCmd = "ABORT"`; a `ScenarioCorp < 11`
///   clears the scenario.
/// * A running event (`EventStatus = 1`) is turned into its outcome (`ScenarioNum = -ScenarioNum`).
/// * The text section (`@IT` number = `ScenarioNum`, or an alternative story chosen from the clock
///   seconds / `ScenarioTag`, with `showText = "Y"`) is read from `SCENWIN<n>.WSR` and decoded; an
///   outcome for a company that changed industry is aborted.
/// * The case body fills the text placeholders and shows it (ScenarioEvent_SubstTokens: a message box,
///   then the report with YES/NO for a proposal; the answer goes to `ScenarioCmd` = "Y" / "ABORT"), and
///   applies its effects.
/// * Then: `NewsMsg` from the case's news tail, the event status (1 = running, 2 = done), the
///   player's notoriety (`PlayerNotoriety`), `ScenarioStage` (+ notoriety, at least 1), `TmpExt1 = margin`
///   (the new profit margin, applied by Scenario_ApplyNewsEffects), `MarketCap` and its price tick.
///
/// A runtime error (missing text file, no `@END`) goes to the ON ERROR handler: a message box
/// " [runtime resource wording] n  CHEATCO% = <name>" and the routine ends.
pub fn run_scenario_event_689361(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime, price_pct: i32) {
    let __content = crate::content::current();
    let mut e = Ev { price_pct, text_arr: vec![PbStr::new(); 201], ..Ev::default() };
    if let Err(err) = main(&mut e, g, ui, rng, now) {
        // ON ERROR handler (0x69aeb4)
        e.aux_text = PbStr::new();
        if 10 < g.scenario_corp {
            e.aux_text = g.ent_name[cast::<i32, _>(g.scenario_corp)].clone();
        }
        let text = s(__content.string(13488)) + &str_s(cast::<Ext, _>(err)) + __content.string(13489) + &e.aux_text;
        ui.msgbox(&text, 0, &PbStr::new());
    }
}

/// Runtime error number (the ON ERROR handler's `ERR`).
pub(super) type PbErr = i32;

fn main(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime) -> Result<(), PbErr> {
    e.clock_sec = time_secs(now);
    e.day_num = to_long(val(mid(date_str(now).as_bytes(), 4, 2).as_bytes()));
    g.scenario_cmd = PbStr::new();
    e.pct_move = cast::<i64, _>(e.price_pct);
    e.event_state = cast::<i32, _>(g.event_status[cast::<i32, _>(g.scenario_num)]);
    if e.event_state == 2 {
        set(&mut g.scenario_cmd, "ABORT");
        return Ok(());
    }
    set(&mut e.is_ceo, "N");
    if 0 < g.event_player {
        e.notoriety = g.player_notoriety[cast::<i32, _>(g.event_player)];
        e.player_name = g.ent_name[cast::<i32, _>(g.event_player)].clone();
    }
    if g.scenario_corp < 11 {
        g.scenario_num = 0;
        g.scenario_corp = 0;
        g.event_player = 0;
        g.scenario_stage = 0;
        return Ok(());
    }
    let corp = cast::<i32, _>(g.scenario_corp);
    if 0 < g.event_player && corp == g.player_ceo_of[cast::<i32, _>(g.event_player)] {
        set(&mut e.is_ceo, "Y");
    }
    e.industry = g.industry_num[corp];
    e.industry_name = g.ind_name[cast::<i32, _>(g.industry_num[corp])].clone();
    e.margin = cast::<i32, _>(g.profit_margin[corp]);
    if 69 < e.industry {
        e.margin = 0;
        g.tmp_ext1 = cast::<Ext, _>(e.margin);
    }
    e.corp_name = g.ent_name[corp].clone();
    e.corp_symbol = g.ent_symbol[corp].clone();
    e.country_idx = g.country_idx[corp];
    e.nation_name = g.country_name[e.country_idx].clone();
    e.market_cap = g.market_cap[corp];
    e.event_year = cast::<i64, _>(g.cur_year);
    if e.event_state == 0 {
        g.scenario_tag = PbStr::new();
    }
    if e.event_state == 1 {
        g.scenario_num = -g.scenario_num;
        e.event_state = 2;
    }
    select_text_id(e, g);
    e.aux_text = s("1");
    let abs_id = (cast::<i32, _>(e.text_id)).abs();
    if 24 < abs_id {
        e.aux_text = s("2");
    }
    if 60 < abs_id {
        e.aux_text = s("3");
    }
    if 100 < abs_id {
        e.aux_text = s("4");
    }
    read_text(e)?;
    decrypt_text(e);
    if g.scenario_num < 0 {
        e.event_state = 2;
    }
    e.abs_val = (cast::<i32, _>(g.scenario_num)).abs();
    if g.scenario_num < 0 && 2 < e.abs_val && e.abs_val < 70 && cast::<i32, _>(e.industry) != e.abs_val {
        set(&mut g.scenario_cmd, "ABORT");
    } else {
        let n = cast::<i32, _>(g.scenario_num);
        let mut handled = super::scenario_cases_a::dispatch(e, g, ui, rng, n);
        if !handled {
            // cases that may leave the routine without the common end (an early exit, or error 9)
            match super::scenario_cases_b::dispatch_result(e, g, ui, rng, n) {
                Ok(h) => handled = h,
                Err(super::scenario_cases_b::EXIT_ROUTINE) => return Ok(()),
                Err(code) => return Err(code),
            }
        }
        handled = handled
            || super::scenario_cases_c::dispatch(e, g, ui, rng, n)
            || super::scenario_cases_d::dispatch(e, g, ui, rng, n);
        if !handled {
            // CASE ELSE
            set(&mut g.scenario_cmd, "ABORT");
            e.event_state = -2;
        }
    }
    finish(e, g, ui, rng);
    Ok(())
}

/// The alternative text sections (`textId`) and their `showText = "Y"` flag.
fn select_text_id(e: &mut Ev, g: &mut Globals) {
    let __content = crate::content::current();
    let n = g.scenario_num;
    let cs = e.clock_sec;
    e.text_id = n;
    let tag_is = |g: &Globals, t: &str| g.scenario_tag == t;
    let alt = |e: &mut Ev, id: i16| {
        e.text_id = id;
        set(&mut e.show_text, "Y");
    };
    if n == -15 && 30 < cs {
        alt(e, -142);
    }
    if n == -23 && tag_is(g, __content.string(13130)) {
        alt(e, -145);
    }
    if n == -50 && tag_is(g, __content.string(13279)) {
        alt(e, -144);
    }
    if n == -111 && cs % 5 < 2 {
        alt(e, -143);
    }
    if n == -116 && 24 < cs {
        alt(e, -141);
    }
    if n == 1 && 25 < cs {
        alt(e, 77);
        set(&mut g.scenario_tag, "ALZHEIMER");
    }
    if n == -1 && tag_is(g, "ALZHEIMER") {
        alt(e, -77);
    }
    if n == 2 && g.cur_quarter < 4 {
        alt(e, 80);
        set(&mut g.scenario_tag, "E.R.");
    }
    if n == -2 && tag_is(g, "E.R.") {
        alt(e, -80);
    }
    if n == 3 && 30 < cs {
        alt(e, 93);
    }
    if n == -3 && tag_is(g, "BRONTO") {
        alt(e, -93);
    }
    if n == 4 && cs < 31 {
        alt(e, 75);
    }
    if n == -4 && tag_is(g, "TLAXACOATL") {
        alt(e, -75);
    }
    if n == 10 && 30 < cs {
        alt(e, 78);
        set(&mut g.scenario_tag, "SEXBOT");
    }
    if n == -10 && tag_is(g, "SEXBOT") {
        alt(e, -78);
    }
    if n == 14 && 20 < cs {
        alt(e, 79);
        set(&mut g.scenario_tag, "KILLERBOT");
    }
    if n == -14 && tag_is(g, "KILLERBOT") {
        alt(e, -79);
    }
    if n == 19 && 30 < cs {
        alt(e, 94);
        set(&mut g.scenario_tag, "SAIPAN");
    }
    if n == -19 && tag_is(g, "SAIPAN") {
        alt(e, -94);
    }
    if n == 21 && 30 < cs && g.start_year + 5 < g.cur_year {
        alt(e, 99);
        set(&mut g.scenario_tag, "ZOMBEX-II");
    }
    if n == -21 && tag_is(g, "ZOMBEX-II") {
        alt(e, -99);
    }
    if n == 22 && cs % 2 == 1 {
        alt(e, 92);
    }
    if n == -22 && tag_is(g, __content.string(4564)) {
        alt(e, -92);
    }
    if n == 25 && 30 < cs {
        alt(e, 91);
    }
    if n == -25 && tag_is(g, "TEAMSTERS") {
        alt(e, -91);
    }
    if n == 26 && 35 < cs {
        alt(e, 97);
        set(&mut g.scenario_tag, "SONDER");
    }
    if n == -26 && tag_is(g, "SONDER") {
        alt(e, -97);
    }
    if n == 27 && 40 < cs {
        alt(e, 74);
        set(&mut g.scenario_tag, "NUKE");
    }
    if n == -27 && tag_is(g, "NUKE") {
        alt(e, -74);
    }
    if n == 28 && 30 < cs && Ext::from_f64(7.5) < cast::<Ext, _>(g.wheat_price) {
        alt(e, 95);
    }
    if n == -28 && tag_is(g, "SAWDUST") {
        alt(e, -95);
    }
    if n == 31 && 30 < cs {
        alt(e, 96);
        set(&mut g.scenario_tag, "THC");
    }
    if n == -31 && tag_is(g, "THC") {
        alt(e, -96);
    }
    if n == 34 && 30 < cs {
        alt(e, 72);
        set(&mut g.scenario_tag, "ZING!");
    }
    if n == -34 && tag_is(g, "ZING!") {
        alt(e, -72);
    }
    if n == 48 && 2 < g.cur_quarter {
        alt(e, 76);
        set(&mut g.scenario_tag, "CHINESETOYS");
    }
    if n == -48 && tag_is(g, "CHINESETOYS") {
        alt(e, -76);
    }
    if n == 56 && e.margin < 4 {
        alt(e, 71);
        set(&mut g.scenario_tag, "HYPO");
    }
    if n == -56 && tag_is(g, "HYPO") {
        alt(e, -71);
    }
    if n == 58 && 27 < cs {
        alt(e, 73);
        set(&mut g.scenario_tag, "PYRO");
    }
    if n == -58 && tag_is(g, "PYRO") {
        alt(e, -73);
    }
    if n == 63 && 30 < cs {
        alt(e, 98);
    }
    if n == -63 && tag_is(g, "Hanford") {
        alt(e, -98);
    }
    if n == -9 && tag_is(g, "BRE-X") {
        alt(e, -140);
    }
    if n == -29 && tag_is(g, "Disney") {
        alt(e, -146);
    }
    if n == -32 && tag_is(g, "CYANIDE") {
        alt(e, -147);
    }
}

/// `OPEN DataPath + "SCENWIN" + auxText + ".WSR" FOR INPUT`: reads the lines of section `textId` (the
/// lines after `@IT<nnnn>` with that number) into `textArr(1..)`, then `textArr(n + 1) = "@END"`.
fn read_text(e: &mut Ev) -> Result<(), PbErr> {
    let file_no = to_long(val(e.aux_text.as_bytes()));
    let data = super::scenario::scenwin_file(file_no).ok_or(53)?;
    e.file_num = 1; // FREEFILE
    let mut f = InputReader::new(&data);
    e.line_no = 0;
    e.in_section = 0;
    loop {
        e.line_text = PbStr(f.try_line_input().map_err(|code| cast::<i32, _>(code))?);
        if mid(e.line_text.as_bytes(), 1, 4) == "@END" {
            break;
        }
        if mid(e.line_text.as_bytes(), 1, 3) == "@IT" {
            e.char_code = to_integer(val(right(e.line_text.as_bytes(), 4).as_bytes()));
            continue;
        }
        if e.char_code != e.text_id && e.in_section == 1 {
            break;
        }
        if e.char_code == e.text_id {
            e.in_section = 1;
            e.line_no = e.line_no + 1;
            e.line_count = e.line_no;
            let i = cast::<usize, _>(e.line_no);
            if i >= e.text_arr.len() {
                return Err(9);
            }
            e.text_arr[i] = e.line_text.clone();
        }
    }
    let i = cast::<usize, _>(e.line_no + 1);
    if i >= e.text_arr.len() {
        return Err(9);
    }
    e.text_arr[i] = s("@END");
    e.line_count = e.line_no + 1;
    // CLOSE #fileNum
    Ok(())
}

/// Port of ScenarioEvent_DecryptText (0x69a28d).
/// Decodes `textArr(1 .. lineCount - 1)`: characters 40..170 are shifted down by `2 * pos + 26` in
/// columns 1..5 and by 39 from column 6 on. Leaves the last character in `nationAdj` / `charCode`.
pub(super) fn decrypt_text(e: &mut Ev) {
    e.abs_val = 0;
    let last = e.line_count - 1;
    e.line_no = 1;
    while e.line_no <= last {
        e.line_text = e.text_arr[cast::<usize, _>(e.line_no)].clone();
        e.line_len = len(e.line_text.as_bytes());
        if e.line_len == 0 {
            e.line_text = PbStr::new();
        } else {
            let n = e.line_len;
            e.char_pos = 1;
            while e.char_pos <= n {
                e.nation_adj = mid(e.line_text.as_bytes(), e.char_pos, 1);
                e.char_code = cast::<i16, _>(asc(e.nation_adj.as_bytes()));
                if e.char_code < 40 || 170 < e.char_code {
                    e.out_char = e.nation_adj.clone();
                } else if e.char_pos < 6 {
                    e.out_char = chr(&[cast::<i32, _>(e.char_code) - (e.char_pos * 2 + 26)]);
                } else {
                    // caseVal = charCode: (always 40..170 here)
                    e.abs_val = cast::<i32, _>(e.char_code) - 39;
                    e.out_char = chr(&[e.abs_val]);
                }
                let oc = e.out_char.clone();
                mid_assign(&mut e.line_text, e.char_pos, 1, oc.as_bytes());
                e.char_pos += 1;
            }
        }
        e.text_arr[cast::<usize, _>(e.line_no)] = e.line_text.clone();
        e.line_no += 1;
    }
}

impl Ev {
    /// Port of ScenarioEvent_ChargeExpense (0x69a51b): `AccruedPL` and `Cash` of `ScenarioCorp` minus
    /// `amount` (a negative amount is a gain).
    pub(super) fn charge_expense(&self, g: &mut Globals) {
        let c = cast::<i32, _>(g.scenario_corp);
        g.accrued_pl[c] = g.accrued_pl[c] - self.amount;
        g.cash[c] = g.cash[c] - self.amount;
    }

    /// Port of ScenarioEvent_ChargeExtraordinary (0x69a59a): `ExtraordinaryPL` (QUAD, rounded) and
    /// `Cash` of `ScenarioCorp` minus `amount`.
    pub(super) fn charge_extraordinary(&self, g: &mut Globals) {
        let c = cast::<i32, _>(g.scenario_corp);
        g.extraordinary_pl[c] = to_quad(cast::<Ext, _>(g.extraordinary_pl[c]) - self.amount);
        g.cash[c] = g.cash[c] - self.amount;
    }

    /// Port of ScenarioEvent_PrefixThe (0x69a619): "the " before the nation name for countries 0, 11,
    /// 30, 35, 50.
    pub(super) fn prefix_the(&mut self) {
        if self.country_idx == 0
            || self.country_idx == 11
            || self.country_idx == 30
            || self.country_idx == 35
            || self.country_idx == 50
        {
            self.nation_name = s("the ") + &self.nation_name;
        }
    }

    /// Port of ScenarioEvent_Nationality (0x69a675): `nationAdj` = the nationality adjective of
    /// `countryIdx` (> 0; unchanged otherwise).
    pub(super) fn nationality(&mut self) {
    let __content = crate::content::current();
        if 0 < self.country_idx {
            self.nation_adj = self.nation_name.clone();
            let lit = match self.country_idx {
                1 => Some(__content.string(13490)),
                2 | 3 | 9 | 10 | 21 | 22 | 27 | 28 | 38 | 39 | 41 | 42 | 44 | 52 => {
                    self.nation_adj = self.nation_adj.clone() + "n";
                    None
                }
                4 => Some("Belgian"),
                5 => Some(__content.string(10517)),
                6 => Some(__content.string(10513)),
                7 => Some("Chilean"),
                8 => Some("Chinese"),
                11 => Some("Czech"),
                12 => Some("Danish"),
                13 => Some(__content.string(13491)),
                14 => Some("Finnish"),
                15 => Some("French"),
                16 => Some("German"),
                17 => Some("Greek"),
                19 => Some(__content.string(13492)),
                20 => Some(__content.string(13493)),
                23 => Some("Irish"),
                24 | 33 => {
                    self.nation_adj = self.nation_adj.clone() + "i";
                    None
                }
                25 => Some("Italian"),
                26 | 47 => {
                    self.nation_adj = self.nation_adj.clone() + "ese";
                    None
                }
                29 => Some("Mexican"),
                30 => Some("Dutch"),
                32 => Some(__content.string(13494)),
                34 => Some(__content.string(5411)),
                35 => Some(__content.string(13495)),
                36 => Some("Polish"),
                37 => Some(__content.string(13496)),
                43 => Some("Spanish"),
                45 => Some("Swedish"),
                46 => Some("Swiss"),
                48 => Some("Thai"),
                49 => Some("Turkish"),
                50 => Some("British"),
                51 => Some(__content.string(13497)),
                _ => None,
            };
            if let Some(t) = lit {
                self.nation_adj = s(t);
            }
        }
    }

    /// Port of ScenarioEvent_SubstTokens (0x69951e).
    /// Fills the placeholders of the event text (`@YEAR` detail text or `STR$(eventYear)`, `@CHEATNAM`,
    /// `@CHEATSYM`, `@CHEATIND`, `@CHEATER`, `@FACT` = `ScenarioTag`, `@AMOUNT` (`amount` formatted;
    /// amounts between -1 and 1 become -1 / 1), `@LOSS`, `@NATION`, `@MISC`, `@TEXT1`), indents the lines
    /// by five blanks, copies them to `TextLines(1..120)` and shows them: for a proposal
    /// (`eventState = 0`) a "DECISION REQUIRED" message box and the memo with YES/NO, for an outcome
    /// (`eventState = 2`) a "Trading Halted" message box (with a commentary on a suspicious price move,
    /// at most twice per game: `HaltCommentCount2`) and the news update. `ScenarioCmd` = the answer
    /// (`ReportDlgYesNo`; "N" becomes "ABORT" with `eventState = -2`); `AdventureCooldown = 10` when the
    /// state is +-2.
    pub(super) fn subst_tokens(&mut self, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
        if self.event_year != 0 && self.detail_text.is_empty() {
            self.detail_text = str_s(cast::<Ext, _>(self.event_year));
            self.detail_text = ltrim(self.detail_text.as_bytes());
        }
        if self.amount != Ext::from_f64(0.0) {
            if Ext::from_f64(0.0) < self.amount && self.amount < Ext::from_f64(1.0) {
                self.amount = Ext::from_f64(1.0);
            }
            if self.amount < Ext::from_f64(0.0) && -Ext::from_f64(1.0) < self.amount {
                self.amount = -Ext::from_f64(1.0);
            }
            self.amount_text = trim(g.currency_symbol.as_bytes())
                + &trim(&format(self.amount, b"#######,"))
                + " "
                + &g.million_prefix_uc;
        }
        if self.loss_amt != 0 && self.loss_text.is_empty() {
            self.loss_text = trim(g.currency_symbol.as_bytes())
                + &trim(&format(cast::<Ext, _>(self.loss_amt), b"########"))
                + " "
                + &g.million_prefix_uc;
        }
        let tokens: [(&str, PbStr); 11] = [
            ("@YEAR", self.detail_text.clone()),
            ("@CHEATNAM", self.corp_name.clone()),
            ("@CHEATSYM", self.corp_symbol.clone()),
            ("@CHEATIND", self.industry_name.clone()),
            ("@CHEATER", self.player_name.clone()),
            ("@FACT", g.scenario_tag.clone()),
            ("@AMOUNT", self.amount_text.clone()),
            ("@LOSS", self.loss_text.clone()),
            ("@NATION", self.nation_name.clone()),
            ("@MISC", self.misc_text.clone()),
            ("@TEXT1", self.tire_word.clone()),
        ];
        let last = self.line_count;
        self.line_no = 1;
        while self.line_no <= last {
            let i = cast::<usize, _>(self.line_no);
            self.line_text = self.text_arr.get(i).cloned().unwrap_or_default();
            for (tok, with) in &tokens {
                if 0 < instr(1, self.line_text.as_bytes(), tok.as_bytes()) {
                    repl_s(&mut self.line_text, tok, with);
                }
            }
            if i < self.text_arr.len() {
                self.text_arr[i] = self.line_text.clone();
            }
            self.line_no += 1;
        }
        self.line_idx = 1;
        while self.line_idx <= 120 {
            let i = cast::<usize, _>(self.line_idx);
            if instr(1, self.text_arr[i].as_bytes(), b"@END") == 0 {
                self.text_arr[i] = s("     ") + &self.text_arr[i];
            }
            g.text_lines[self.line_idx] = self.text_arr[i].clone();
            self.line_idx += 1;
        }
        if self.event_state == 0 {
            set(&mut g.text_box_mode, "YESNO");
        } else if self.event_state == 2 {
            set(&mut g.text_box_mode, "CHEATRESULT");
        }
        g.text_box_title = PbStr::new();
        if !(g.text_box_mode == "YESNO") {
            set(&mut g.text_box_title, __content.string(13498));
        }
        if g.text_box_title.is_empty() {
            g.text_box_title = s(__content.string(13499)) + &self.corp_name;
            if self.is_ceo == "N" {
                g.text_box_title = s(__content.string(13500)) + &self.corp_name;
            }
        }
        if self.event_state == 0 {
            let text = s(__content.string(13501))
                + &self.corp_name
                + "."
                + "\r"
                + "\r"
                + __content.string(13502);
            ui.msgbox(&text, 8192, &s(__content.string(13503)));
        } else if self.event_state == 2 {
            self.aux_text = s(__content.string(5110))
                + &self.corp_name
                + " ("
                + &self.corp_symbol
                + __content.string(5111)
                + __content.string(5112)
                + "\r"
                + "\r"
                + __content.string(5094);
            self.phrase = PbStr::new();
            if 10 < self.pct_move {
                set(&mut self.phrase, " was up");
            }
            if 19 < self.pct_move {
                set(&mut self.phrase, __content.string(13504));
            }
            if self.pct_move < -8 {
                set(&mut self.phrase, __content.string(5219));
            }
            if self.pct_move < -15 {
                repl(&mut self.phrase, "fallen", "plunged");
            }
            if !self.phrase.is_empty() && g.halt_comment_count2 < 2 {
                self.abs_val = to_long((cast::<Ext, _>(self.pct_move)).abs());
                let words = number_to_words(self.abs_val, 0, 1);
                self.aux_text = self.aux_text.clone()
                    + "\r"
                    + "\r"
                    + __content.string(5220)
                    + "the "
                    + &self.corp_symbol
                    + " stock"
                    + &self.phrase
                    + &words
                    + __content.string(5221)
                    + __content.string(5222)
                    + __content.string(5223)
                    + __content.string(13505);
                if 30 < self.clock_sec {
                    repl(&mut self.aux_text, __content.string(5225), __content.string(5226));
                }
                if self.clock_sec < 20 {
                    repl(&mut self.aux_text, __content.string(5227), __content.string(5228));
                }
                g.halt_comment_count2 = g.halt_comment_count2 + 1;
            }
            let title = s(__content.string(5113)) + &self.corp_name;
            ui.msgbox(&self.aux_text, 8208, &title);
        }
        // (UI) DIALOG SHOW STATE hDlgMain, 0
        show_report_dialog(g, ui, rng);
        // (UI) DIALOG SHOW STATE hDlgMain, 5
        g.scenario_cmd = g.report_dlg_yes_no.clone();
        if g.scenario_cmd == "N" {
            set(&mut g.scenario_cmd, "ABORT");
            self.event_state = -2;
        }
        if self.event_state.abs() == 2 {
            g.adventure_cooldown = 10;
        }
    }

    /// `PostNews(corpArg, 0)` with `corpArg = ScenarioCorp` (the cases' direct news).
    pub(super) fn post_corp_news(&mut self, g: &mut Globals, ui: &mut dyn Ui) {
        self.corp_arg = cast::<i32, _>(g.scenario_corp);
        post_news(g, ui, self.corp_arg, 0);
    }
}

/// The common end of every case (0x699187).
fn finish(e: &mut Ev, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    if !e.news_tail.is_empty() {
        g.news_msg = e.corp_symbol.clone() + " " + &e.news_tail;
    }
    if g.scenario_cmd == "Y" && 2 < g.scenario_num {
        e.notoriety = e.notoriety.wrapping_add(1);
    }
    let an = (cast::<Ext, _>(g.scenario_num)).abs();
    let abs_num = || an;
    if g.scenario_cmd == "ABORT" {
        if e.event_state != -2 {
            e.event_state = -1;
        }
        if e.event_state == -2 && Ext::from_f64(100.0) < abs_num() && abs_num() < Ext::from_f64(111.0) {
            e.event_state = -1;
        }
        if (e.event_state == -2 && abs_num() < Ext::from_f64(101.0)) || Ext::from_f64(110.0) < abs_num() {
            e.event_state = 2;
        }
        if e.event_state == 2 && Ext::from_f64(0.0) < abs_num() {
            e.event_idx = to_quad(abs_num());
            g.event_status[cast::<i32, _>(e.event_idx)] = 2;
        }
    } else if e.event_state == 0 {
        e.event_state = 1;
        if Ext::from_f64(0.0) < abs_num() {
            e.event_idx = to_quad(abs_num());
            g.event_status[cast::<i32, _>(e.event_idx)] = 1;
            g.adventure_cooldown = 2;
        }
    }
    if 0 < g.scenario_stage {
        g.scenario_stage = e.notoriety.wrapping_add(g.scenario_stage);
    }
    if g.scenario_stage < 1 {
        g.scenario_stage = 1;
    }
    if g.scenario_cmd == "ABORT" && e.event_state == 2 {
        set(&mut g.scenario_cmd, "N");
    }
    if e.event_state == 2 {
        e.event_idx = to_quad((cast::<Ext, _>(g.scenario_num)).abs());
        g.event_status[cast::<i32, _>(e.event_idx)] = 2;
    }
    if e.notoriety < 0 {
        e.notoriety = 0;
    }
    if 0 < g.event_player {
        g.player_notoriety[cast::<i32, _>(g.event_player)] = e.notoriety;
    }
    let c = cast::<i32, _>(g.scenario_corp);
    if g.industry_num[c] < 70 {
        g.tmp_ext1 = cast::<Ext, _>(e.margin);
    }
    g.market_cap[c] = e.market_cap;
    e.corp_arg = c;
    update_price_history_tick(g, ui, rng, e.corp_arg);
}

/// Helpers shared by the case files.
impl Ev {
    /// `ShowQuickMessage(NewsMsg)` + `PostNews` used by a few cases.
    #[allow(dead_code)]
    pub(super) fn quick_message(g: &mut Globals, ui: &mut dyn Ui, text: &mut PbStr) {
        show_quick_message(g, ui, text);
    }

    /// `UpdateCorpControl(ScenarioCorp)`.
    #[allow(dead_code)]
    pub(super) fn update_control(&mut self, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
        self.corp_arg = cast::<i32, _>(g.scenario_corp);
        update_corp_control(g, ui, rng, self.corp_arg);
    }
}
