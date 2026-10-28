use super::{common::st, diff::{Compare, Opts, run_original_with}};
use wsr_core::{logic::{newgame::LocalTime, shell::{Env, Files, quotes::*}}, pb::{codec, pbstr::PbStr}, state::Globals, ui::{ReportAction, Ui}};

#[derive(Default)]
struct QuoteUi { shown: Vec<(PbStr, PbStr)> }
impl Ui for QuoteUi {
    fn msgbox(&mut self, _: &PbStr, _: i32, _: &PbStr) -> i32 { 1 }
    fn inputbox(&mut self, _: &PbStr, _: &PbStr, _: &PbStr) -> PbStr { PbStr::new() }
    fn report(&mut self, _: &PbStr, _: &[PbStr], _: bool) -> ReportAction { ReportAction::Ok }
    fn news(&mut self, _: &PbStr) {}
    fn choose(&mut self, title: &PbStr, text: &[PbStr], _: &[PbStr]) -> Option<usize> {
        self.shown.push((title.clone(), text[0].clone())); Some(0)
    }
}

#[test]
fn quote_calendar_preserves_leap_year_quirk() {
    for (year, month, day, expected) in [(2000,1,3,3),(2000,3,1,61),(2001,3,1,60),(1900,3,1,61),(2100,12,31,366)] {
        assert_eq!(quote_day(&LocalTime { year, month, day, ..LocalTime::HARNESS }), expected);
    }
}

#[test]
fn quotes_match_original_text_dialog_and_flagfile() {
    let __content = wsr_core::content::current();
    for (text, flag, xp, found) in [("An original test quotation.",false,false,true),
        ("Dr. Coyote: Keep your wits.",false,false,true), ("Test",true,true,true),
        ("Test",true,false,true), ("Test",false,false,false)] {
        let mut data = b"@IT001\r\nIgnored\r\n".to_vec();
        data.extend_from_slice(if found { b"@IT003\r\n" } else { b"@IT004\r\n" });
        data.extend(codec::encode_line(text.as_bytes()));
        data.extend_from_slice(b"\r\n\r\n");
        data.extend(codec::encode_line(b"Another line."));
        data.extend_from_slice(b"\r\n@END\r\n");
        let mut g = Globals::default(); g.win_version = st(if xp { "XP" } else { "WINDOWS 10" });
        g.quote_of_day_text = st(__content.string(14135));
        let mut files = Files::default(); files.files.insert("QUOTEWSR.DAT".into(),data.clone());
        if flag { files.files.insert("FLAGFILE.TXT".into(), b"x".to_vec()); }
        let opts = Opts { files: files.files.iter().map(|(n,b)|(n.clone(),b.clone())).collect(), no_default_files:true, ..Opts::default() };
        let Some(o) = run_original_with(0x4a0c7f, &opts, &g) else { return };
        let env = Env::new(&LocalTime::HARNESS).with_files(files);
        let mut port = g.clone(); let mut ui = QuoteUi::default();
        show_quote_of_day(&mut port, &mut ui, &env);
        Compare::default().ignore("hQuoteOfDayDlg").assert(&port, &o.globals);
        if let Some((title, text)) = ui.shown.first() {
            assert_eq!(title, &o.dialogs[0].title);
            assert_eq!(text, &o.dialogs[0].control(519).unwrap().text);
        } else { assert!(o.dialogs.is_empty()); }
        assert_eq!(env.files.borrow().written.get("FLAGFILE.TXT"), o.files.get("FLAGFILE.TXT"));
    }
}
