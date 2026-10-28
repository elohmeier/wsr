//! Tests of the shared helpers in logic::economy: ShowMiscNewsStory (0x53e1ee), CheckIndexHistLow (0x53c9d5).
use wsr_core::pb::real::{Ext, cast};


use super::*;
use wsr_core::logic::economy::{check_index_hist_low, show_misc_news_story};

#[test]
fn check_index_hist_low_unit() {
    let mut g = Globals::default();
    assert_eq!(check_index_hist_low(&g), 0);
    g.price_hist_low[(37, 1604)] = -Ext::from_f64(3.0);
    g.rand_seed = 12;
    assert_eq!(check_index_hist_low(&g), 1);
    g.rand_seed = 28; // 28 MOD 17 = 11
    assert_eq!(check_index_hist_low(&g), 0);
}

/// Stories by number (incl. the year-shifted 13/26/130, a random one, one past the file) with the
/// `@CORP`/`@AMT`/`@TEXTSTRING`/`@HUMOR` substitutions.
#[test]
fn diff_show_misc_news_story() {
    let __content = wsr_core::content::current();
    init_files();
    diff::par_seeds(1..=12, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed * 77 + 3);
        let story = [0, 1, 5, 13, 26, 35, 130, -1, 7, 20, 400, 2][cast::<usize, _>(seed - 1)];
        g.tmp_ext1 = if r.chance(Ext::from_f64(0.5)) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(1e7)) } else { Ext::from_f64(0.0) };
        g.tmp_ext2 = if r.chance(Ext::from_f64(0.5)) { cast::<Ext, _>(r.range(11, 1600)) } else { Ext::from_f64(0.0) };
        g.tmp_news_str = s("a joke");
        g.text_box_mode = s("1");
        g.rand_seed = cast::<i32, _>(r.range(0, 30000));
        let text = format!("{__wsr_text_13768}{seed}",__wsr_text_13768=__content.string(13768));
        let opts = Opts { args: vec![format!("{story}"), format!("s:{text}")], rng: Some(1234), ..Opts::default() };
        let Some(o) = run_original_with(0x53e1ee, &opts, &g) else { return };
        let mut port = g.clone();
        let mut rng = Rng::from_state(1234);
        let mut ui = Scripted::default();
        show_misc_news_story(&mut port, &mut ui, &mut rng, story, text.as_bytes());
        assert!(unexpected(&o.log).is_empty(), "{:?}", unexpected(&o.log));
        cmp().assert(&port, &o.globals);
        assert_eq!(rng.state(), o.rng);
        let titles: Vec<String> = o.dialogs.iter().map(|d| d.title.to_display()).collect();
        let reports = ui.log.iter().filter(|l| l.starts_with("REPORT")).count();
        assert_eq!(reports > 0, !titles.is_empty(), "{__wsr_text_13769}{:?} orig {:?}", ui.log, titles,__wsr_text_13769=__content.string(13769));
    });
}
