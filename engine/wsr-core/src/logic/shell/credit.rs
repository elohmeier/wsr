//! Credit information dialog, the mode-1 path of `InBox` (0x423170).
use crate::pb::real::{Ext, cast};


use crate::logic::finance::calc_entity_financials;
use crate::pb::numeric::format;
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{instr, ltrim, remove, replace, rtrim};
use crate::state::Globals;
use crate::ui::Ui;

fn s(text: &str) -> PbStr { PbStr::from(text) }
fn trim(text: &PbStr) -> PbStr { rtrim(ltrim(text.as_bytes()).as_bytes()) }

/// Port of the credit-information path of InBox (0x423170), CreateSymbolEntryDialog
/// (0x419674), and InBox_SetupPrompt (0x42351f). The dialog has OK and Cancel buttons;
/// its informational text is offered as the prompt to [`Ui::choose`].
pub fn show_credit_info(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    g.in_box_mode = 1;
    g.symbol_dlg_text = PbStr::new();
    if g.symbol_dlg_open == 1 { g.symbol_dlg_open = 0; }
    let title = if g.input_dlg_title.is_empty() { g.app_title.clone() } else { g.input_dlg_title.clone() };
    let entity = g.sel_entity;
    calc_entity_financials(g, ui, rng, entity);
    let mut credit = trim(&g.currency_symbol)
        + &trim(&PbStr(format(g.unused_credit_line, b"##########,.00")))
        + " " + &g.million_prefix_lc + "illion" + &g.currency_suffix + ".";
    if g.unused_credit_line == Ext::from_f64(0.0) {
        credit = s("ZERO.");
        if entity < 11 || entity > 85 {
            if g.credit_frozen[entity] == 1 {
                credit = s("FROZEN.");
            } else if g.bank_largest_debtor[g.lender_id] == entity {
                credit = s(__content.string(10027));
            } else if g.long_options_value > Ext::from_f64(0.0) && g.credit_rating_num[entity] > 6
                && !(entity > 1580 && g.industry_num[entity] == 71 && g.tmp_credit_net_worth < g.tmp_total_debt * Ext::from_f64(3.0))
            {
                credit = s(__content.string(10028));
            }
        }
    }
    let mut note;
    let mut prompt;
    let bank = cast::<i32, _>(g.lending_bank[entity]);
    if entity < 11 {
        note = s(__content.string(10029));
        prompt = s(__content.string(10030)) + &g.credit_rating_str + ".";
        prompt = prompt + __content.string(10031) + &g.ent_name[bank] + " (" + &g.ent_symbol[bank]
            + __content.string(10032) + &PbStr(format(cast::<Ext, _>(g.borrow_rate), b"###.00")) + "%.  "
            + __content.string(10033) + &credit + "\r\n\r\n";
    } else {
        note = s(__content.string(10034));
        if g.industry_num[entity] == 71 { note = replace(note.as_bytes(), b"company", b"fund"); }
        prompt = g.sel_corp_name.clone() + __content.string(10035) + &g.credit_rating_str + ".\r\n\r\n";
        if g.industry_num[entity] < 2 {
            prompt = prompt + __content.string(10036)
                + &PbStr(format(cast::<Ext, _>(g.entity_borrow_rate), b"###.00")) + "%.";
        } else {
            prompt = prompt + __content.string(10037) + &g.ent_name[bank] + " (" + &g.ent_symbol[bank]
                + __content.string(10032) + &PbStr(format(cast::<Ext, _>(g.borrow_rate), b"###.00")) + "%. ";
            if entity > 1580 && g.industry_num[entity] == 71 && g.credit_rating_num[entity] > 6
                && credit != "FROZEN." && g.unused_credit_line == Ext::from_f64(0.0)
            {
                prompt = prompt + __content.string(10038);
            } else {
                prompt = prompt + __content.string(10039) + &credit;
            }
        }
    }
    prompt = prompt + "\r\n\r\n";
    if credit == "FROZEN." {
        note = PbStr::new();
        if instr(1, prompt.as_bytes(), b"unused") > 0 { prompt = remove(prompt.as_bytes(), b" unused"); }
    }
    if g.credit_rating_str == "D" || instr(1, credit.as_bytes(), b"ZERO") > 0 || g.unused_credit_line == Ext::from_f64(0.0) {
        note = PbStr::new();
    }
    prompt = prompt + &note;
    g.input_dlg_title = PbStr::new();
    g.symbol_dlg_open = 1;
    let choice = ui.choose(&title, &[prompt], &[s("OK"), s("Cancel")]);
    if choice == Some(1) {
        g.symbol_dlg_text = s("Cancel");
        g.target_corp = 0;
        g.lookup_entity_name = PbStr::new();
        g.tmp_symbol = PbStr::new();
    }
    if choice.is_some() && g.ticker_on == 1 { g.clock_state = s("RUN"); }
    g.symbol_dlg_open = 0;
    g.in_box_mode = 0;
}
