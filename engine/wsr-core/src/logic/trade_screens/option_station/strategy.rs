//! `AutoBuildOptionStrategy` (0x613212) and the strategy picker `ShowOptionStrategyPicker` (0x6108b7).
use crate::pb::real::{Ext, cast};


use super::order_sheet::{clear_option_grid_fields, strike_range};
use super::{ctl, OptionGrid};
use crate::logic::earnings::calc_cur_month_of_year;
use crate::logic::trading::prelude::*;
use crate::pb::string::{parse_count, parse_count_any};
use crate::pb::ext80::Ext80;
use crate::pb::numeric::fix;

/// Captions of the picker's option buttons 1403, 1406, …, 1430 (`TmpExt1` 1..10).
#[allow(non_snake_case)]
pub fn STRATEGY_NAMES(__content: &crate::content::ContentPack) -> [&str; 10] { [
    __content.string(11747),
    __content.string(11748),
    __content.string(11749),
    __content.string(11750),
    __content.string(11751),
    __content.string(11752),
    __content.string(11753),
    __content.string(11754),
    __content.string(11755),
    __content.string(11756),
] }

/// Port of AutoBuildOptionStrategy (0x613212) with its GOSUBs AutoBuildOptionStrategy_InitTemplates
/// (0x613eee) and AutoBuildOptionStrategy_Exit (0x614183).
///
/// Lets the player pick one of ten strategies ([`show_option_strategy_picker`]), asks for the expiry
/// month/year (INPUTBOX, repeated until valid; empty = cancel) and the contract size in % (1..10, default
/// 10), then clears the order sheet and fills the strategy's legs: per leg the line, B/S, the strike
/// `ROUND(multiplier x share price)` within the sheet's strike range, the expiry and the size. Returns the
/// strategy's name ("" when cancelled).
pub fn auto_build_option_strategy(g: &mut Globals, ui: &mut dyn Ui, grid: &mut OptionGrid, stock_id: i32) -> PbStr {
    let __content = crate::content::current();
    let mut templates: Vec<PbStr> = vec![PbStr::new(); 11];
    let mut strat_names: Vec<PbStr> = vec![PbStr::new(); 11];
    let stock = stock_id;
    let share_price = to_long(fix(g.market_cap[stock] / g.shares_outstanding[stock]).abs());
    show_option_strategy_picker(g, ui, 0);
    let strategy = to_long(g.tmp_ext1);
    if g.tmp_ext1 == Ext::from_f64(0.0) {
        return PbStr::new();
    }
    g.tmp_ext1 = Ext::from_f64(0.0);
    init_templates(&mut templates, &mut strat_names);
    let template = templates.get(cast::<usize, _>(strategy)).cloned().unwrap_or_default();
    let strat_name = strat_names.get(cast::<usize, _>(strategy)).cloned().unwrap_or_default();
    let progress = g.sim_co_count;
    // The original keeps FLD1 below CalcCurMonthOfYear's return, then FADDP after CEIL
    // (0x613330..0x613358): the earliest expiry is the following month.
    let min_month = 1 + to_long(ceil(calc_cur_month_of_year(g, progress)));
    let mut min_month = min_month;
    let mut year_text = str_trim(cast::<Ext, _>(g.cur_year));
    if min_month == 13 {
        min_month = 1;
        year_text = str_trim(cast::<Ext, _>(1 + g.cur_year));
    }
    let mut range_text = s("0") + &str_trim(cast::<Ext, _>(min_month));
    range_text = right(range_text.as_bytes(), 2) + "/" + &year_text + " to ";
    let mut max_month = min_month - 1;
    if max_month == 0 {
        max_month = 12;
    }
    let mut max_month_text = s("0") + &str_trim(cast::<Ext, _>(max_month));
    max_month_text = right(max_month_text.as_bytes(), 2);
    let max_year_text = str_trim(cast::<Ext, _>(2 + g.cur_year));
    range_text = range_text + &max_month_text + "/" + &max_year_text;
    let min_exp_date: Ext = val(year_text.as_bytes()) + cast::<Ext, _>(min_month) / Ext::from_f64(12.0);
    let max_exp_date: Ext = val(max_year_text.as_bytes()) + cast::<Ext, _>(max_month) / Ext::from_f64(12.0);
    let mut prompt_text = s(__content.string(11757))
        + &range_text
        + __content.string(11758)
        + &max_year_text
        + " or 9/"
        + &right(max_year_text.as_bytes(), 2)
        + ".";
    loop {
        let mut input_ok = 1;
        let input_text = ui.inputbox(&prompt_text, &s(__content.string(11759)), &PbStr::new());
        if input_text.is_empty() {
            show_quick_message(g, ui, &mut s(__content.string(3956)));
            return PbStr::new();
        }
        if parse_count(input_text.as_bytes(), b"/") == 2
            && !(len(input_text.as_bytes()) < 4)
            && !(7 < len(input_text.as_bytes()))
        {
            let mut range_err = s(__content.string(11760));
            let mut parsed_year = to_long(val(parse_any(input_text.as_bytes(), b"/", 1).as_bytes()));
            if parsed_year < 1 || 12 < parsed_year {
                input_ok = 0;
            }
            parsed_year = to_long(val(parse_any(input_text.as_bytes(), b"/", 2).as_bytes()));
            if 2 < parsed_year && parsed_year < 100 {
                let qty_text = str_trim(cast::<Ext, _>(g.cur_year));
                parsed_year = to_long(val(mid(qty_text.as_bytes(), 1, 2).as_bytes()) * Ext::from_f64(100.0) + cast::<Ext, _>(parsed_year));
            }
            if parsed_year < g.cur_year || g.cur_year + 2 < parsed_year {
                input_ok = 0;
            }
            let mut exp_date: Ext = cast::<Ext, _>(parsed_year);
            if input_ok == 1 {
                exp_date = val(parse_any(input_text.as_bytes(), b"/", 1).as_bytes()) / Ext::from_f64(12.0) + exp_date;
                if sf(0.01f32) + exp_date < min_exp_date || max_exp_date < exp_date - sf(0.01f32) {
                    input_ok = 0;
                }
            }
            if input_ok == 0 {
                show_quick_message(g, ui, &mut range_err);
            }
        } else {
            show_quick_message(g, ui, &mut (s(__content.string(11761)) + &max_year_text));
            input_ok = 0;
        }
        if input_ok != 1 {
            continue;
        }
        let exp_text = input_text;
        prompt_text = s(__content.string(11762))
            + &g.ent_name[stock]
            + ". "
            + __content.string(11763);
        let mut pct_size;
        loop {
            let input_text = ui.inputbox(&prompt_text, &s(__content.string(11764)), &s("10"));
            if input_text.is_empty() {
                show_quick_message(g, ui, &mut s(__content.string(3956)));
                return PbStr::new();
            }
            pct_size = to_long(val(input_text.as_bytes()));
            if pct_size < 1 || 10 < pct_size {
                show_quick_message(g, ui, &mut s(__content.string(11765)));
            }
            if !(pct_size < 1 || 10 < pct_size) {
                break;
            }
        }
        let (min_strike, max_strike) = strike_range(share_price);
        clear_option_grid_fields(grid);
        let field_count = parse_count_any(template.as_bytes(), b"|");
        let leg_count = to_long(cast::<Ext, _>(field_count) / Ext::from_f64(3.0));
        let mut field_idx = 0;
        for _leg in 1..=leg_count {
            field_idx = field_idx + 1;
            let row = to_long(val(parse_any(template.as_bytes(), b"|", field_idx).as_bytes()));
            if row < 1 || 8 < row {
                ui.msgbox(&s(__content.string(11766)), 0, &PbStr::new());
            }
            field_idx = field_idx + 1;
            let side_text = trim(parse_any(template.as_bytes(), b"|", field_idx).as_bytes());
            if !(side_text == "B") && !(side_text == "S") {
                ui.msgbox(&s(__content.string(11767)), 0, &PbStr::new());
            }
            grid.set_text(ctl::SIDE + row, &side_text);
            field_idx = field_idx + 1;
            let strike_mult: Ext = val(parse_any(template.as_bytes(), b"|", field_idx).as_bytes());
            if cast::<f32, _>(strike_mult) <= Ext::from_f32(0.74f32) || Ext::from_f32(1.26f32) <= cast::<f32, _>(strike_mult) {
                ui.msgbox(
                    &(s(__content.string(11768)) + &str_s(strike_mult)),
                    0,
                    &PbStr::new(),
                );
            }
            // fld qword; fimul dword; fistp: the product in 80 bits decides x.5 ties
            let mut strike = Ext80::from_ext(strike_mult).fmul(Ext80::from_i64(cast::<i64, _>(share_price))).to_i32();
            if (cast::<Ext, _>(strike)) < min_strike {
                strike = to_long(min_strike);
            }
            if max_strike < cast::<Ext, _>(strike) {
                strike = to_long(max_strike);
            }
            grid.set_text(ctl::STRIKE + row, &str_trim(cast::<Ext, _>(strike)));
            grid.set_text(ctl::EXP + row, &exp_text);
            grid.set_text(ctl::QTY + row, &str_trim(cast::<Ext, _>(pct_size)));
        }
        prompt_text = s(__content.string(11769))
            + &strat_name
            + __content.string(11770)
            + __content.string(11771)
            + __content.string(11772);
        if 10 < g.active_entity {
            prompt_text =
                replace(prompt_text.as_bytes(), __content.bytes(11595), (g.active_entity_name.clone() + " has").as_bytes());
        }
        show_quick_message(g, ui, &mut prompt_text);
        return strat_name;
    }
}

/// Port of AutoBuildOptionStrategy_InitTemplates (0x613eee): the ten strategy templates
/// ("row | B/S | strike multiplier" per leg) and their names.
fn init_templates(templates: &mut [PbStr], strat_names: &mut [PbStr]) {
    let __content = crate::content::current();
    let t = [
        __content.string(11773),
        __content.string(11774),
        __content.string(11775),
        __content.string(11776),
        __content.string(11777),
        __content.string(11778),
        __content.string(11779),
        __content.string(11780),
        __content.string(11781),
        __content.string(11782),
    ];
    let n = [
        __content.string(11783),
        __content.string(11784),
        __content.string(11785),
        __content.string(11786),
        __content.string(11787),
        __content.string(11788),
        __content.string(11789),
        __content.string(11790),
        __content.string(11791),
        __content.string(11792),
    ];
    for k in 0..10 {
        templates[k + 1] = s(t[k]);
        strat_names[k + 1] = s(n[k]);
    }
}

/// Port of ShowOptionStrategyPicker (0x6108b7) with its dialog procedure DlgProcOptionStrategy (0x610ed5)
/// and the control callbacks OptStrat_SelLongStraddle (0x6116eb), OptStrat_SelShortStraddle (0x611733),
/// OptStrat_SelLongStrangle (0x61177f), OptStrat_SelShortStrangle (0x6117cb), OptStrat_SelBullCallSpread
/// (0x611817), OptStrat_SelBearCallSpread (0x611863), OptStrat_SelButterfly (0x6118af),
/// OptStrat_SelIronButterfly (0x6118fb), OptStrat_SelCondor (0x611947), OptStrat_SelIronCondor (0x611993),
/// OptStrat_GoClick (0x6119df) and OptStrat_CancelClick (0x611a50).
///
/// The modal " [runtime resource wording] " dialog: ten option buttons ([`STRATEGY_NAMES`]) and
/// GO/CANCEL, shown through [`Ui::select`]. The choice comes back in `TmpExt1` (1..10; 0 = cancelled or
/// GO without a selection). Closing the window after selecting an option keeps that selection (the
/// option callbacks set `TmpExt1` at once); a host maps that onto `Some`. The dialog procedure only
/// colours the controls and sets tooltips (window plumbing, not ported); `hOptionStrategyDlg` is not
/// modelled.
pub fn show_option_strategy_picker(g: &mut Globals, ui: &mut dyn Ui, _h_parent: i32) {
    let __content = crate::content::current();
    g.tmp_ext1 = Ext::from_f64(0.0);
    let options: Vec<PbStr> = STRATEGY_NAMES(&__content).iter().map(|x| s(x)).collect();
    match ui.select(&s(__content.string(11793)), &[s(__content.string(11794))], &options, None, &s("Go")) {
        Some(k) => {
            // OptStrat_Sel*: TmpExt1 = k + 1; then GO
            g.tmp_ext1 = cast::<Ext, _>(k + 1);
            opt_strat_go_click(g);
        }
        None => opt_strat_cancel_click(g),
    }
}

/// Port of OptStrat_GoClick (0x6119df): `TmpExt1` outside 1..10 becomes 0; ends the dialog.
fn opt_strat_go_click(g: &mut Globals) {
    if to_long(g.tmp_ext1) < 1 || 10 < to_long(g.tmp_ext1) {
        g.tmp_ext1 = Ext::from_f64(0.0);
    }
}

/// Port of OptStrat_CancelClick (0x611a50): `TmpExt1 = 0`; ends the dialog.
fn opt_strat_cancel_click(g: &mut Globals) {
    g.tmp_ext1 = Ext::from_f64(0.0);
}
