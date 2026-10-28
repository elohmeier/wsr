//! Credit information dialog compared with InBox mode 1 in the original.
use wsr_core::pb::real::{Ext, cast};

use super::common::{cmp, run, st, world};
use super::diff::{par_seeds, Rand};
use wsr_core::logic::shell::credit::show_credit_info;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::ui::{ReportAction, Ui};

#[derive(Default)]
struct CreditUi { title: PbStr, prompt: PbStr, cancel: bool }
impl Ui for CreditUi {
    fn msgbox(&mut self, _: &PbStr, _: i32, _: &PbStr) -> i32 { 1 }
    fn inputbox(&mut self, _: &PbStr, _: &PbStr, default: &PbStr) -> PbStr { default.clone() }
    fn report(&mut self, _: &PbStr, _: &[PbStr], _: bool) -> ReportAction { ReportAction::Ok }
    fn news(&mut self, _: &PbStr) {}
    fn choose(&mut self, title: &PbStr, prompt: &[PbStr], _: &[PbStr]) -> Option<usize> {
        self.title = title.clone(); self.prompt = prompt[0].clone(); Some(cast::<usize, _>(self.cancel))
    }
}

#[test]
fn credit_dialog_matches_original() {
    let __content = wsr_core::content::current();
    par_seeds(1..=24, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed * 671 + 9);
        let entity = match seed % 4 { 0 => g.cur_player, 1 => 11, 2 => 1590, _ => 131 };
        g.sel_entity = entity;
        g.sel_corp_name = g.ent_name[entity].clone();
        g.action_code = st("CHANGESYM");
        g.input_dlg_title = st(__content.string(10151)) + &g.ent_name[entity];
        if entity == 1590 { g.industry_num[entity] = 71; }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { g.credit_frozen[entity] = 1; }
        let cancel = seed % 2 == 0;
        let mut ui = CreditUi { cancel, ..Default::default() };
        let mut pg = g.clone();
        let mut rng = Rng::from_state(cast::<u32, _>(seed));
        show_credit_info(&mut pg, &mut ui, &mut rng);
        let answer = if cancel { "click:2" } else { "click:1" };
        let Some(o) = run(0x423170, &["1"], &g, cast::<u32, _>(seed), &[answer], &[]) else { return };
        let dialog = o.dialogs.iter().find(|d| d.control(308).is_some()).expect("credit dialog");
        assert_eq!(ui.title, dialog.title, "seed {seed}: title");
        assert_eq!(ui.prompt, dialog.control(308).unwrap().text, "seed {seed}: prompt");
        assert_eq!(rng.state(), o.rng, "seed {seed}: RNG");
        let d = cmp().ignore("DlgSymbolEntry").diffs(&pg, &o.globals);
        assert!(d.is_empty(), "seed {seed}: {}", d.join("\n"));
    });
}
