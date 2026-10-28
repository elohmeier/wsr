//! Differential tests of BuildMenuAdviceText (0x557bbf): corp section.
use wsr_core::pb::real::cast;


#[allow(unused_imports)]
use super::common::*;

#[test]
fn mode1_corp_intro_and_stock_matches_original() {
    let __content = wsr_core::content::current();
    use wsr_core::logic::advice::build_menu_advice_text_body;
    use wsr_core::pb::rng::Rng;
    let hits = std::sync::atomic::AtomicU32::new(0);
    par_seeds(1..=20, |seed| {
        let mut g = advice_world(seed, 1);
        g.sel_entity = 131 + (cast::<i32, _>(seed) % 100);
        g.active_entity = g.sel_entity;
        g.active_entity_name = g.ent_name[g.sel_entity].clone();
        let Some(o) = run_hooked(
            BUILD_MENU_ADVICE_TEXT,
            &[],
            &g,
            cast::<u32, _>(seed),
            &[],
            &skips(),
            &[],
        ) else {
            return;
        };
        let original = o.globals.help_text.to_display();
        let Some(pos) = original.find(__content.string(13549)) else {
            return;
        };
        hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut p = g.clone();
        let mut ui = Rec::default();
        let mut rng = Rng::from_state(cast::<u32, _>(seed));
        build_menu_advice_text_body(&mut p, &mut ui, &mut rng);
        let port = p.help_text.to_display();
        assert!(
            port.starts_with(&original[..pos]),
            "seed {seed}: port {port:?} vs original prefix {:?}",
            &original[..pos]
        );
    });
    if super::diff::unavailable().is_none() {
        assert!(hits.into_inner() > 0, "no company introductions compared");
    }
}

#[test]
fn mode1_corp_matches_original() {
    par_seeds(1..=80, |seed| {
        let mut g = advice_world(seed, 1);
        g.sel_entity = match seed % 4 {
            0 => 11 + (cast::<i32, _>(seed) % 75),
            1 => 86 + (cast::<i32, _>(seed) % 45),
            2 => 131 + (cast::<i32, _>(seed * 19) % 1449),
            _ => 1581 + (cast::<i32, _>(seed) % 20),
        };
        g.active_entity = g.sel_entity;
        g.active_entity_name = g.ent_name[g.sel_entity].clone();
        let original = run_hooked(
            BUILD_MENU_ADVICE_TEXT,
            &[],
            &g,
            cast::<u32, _>(seed),
            &[],
            &[(UPDATE_MARKET_DISPLAY, 0)],
            &[],
        );
        let Some(o) = original else { return };
        let mut p = g.clone();
        let mut ui = Rec::default();
        let mut rng = wsr_core::pb::rng::Rng::from_state(cast::<u32, _>(seed));
        wsr_core::logic::advice::build_menu_advice_text_body(&mut p, &mut ui, &mut rng);
        assert_eq!(
            p.help_text.to_display(),
            o.globals.help_text.to_display(),
            "seed {seed}"
        );
        let diffs = cmp().diffs(&p, &o.globals);
        assert!(diffs.is_empty(), "seed {seed}: {}", diffs.join("\n"));
        assert_eq!(rng.state(), o.rng, "seed {seed}: RNG");
        assert_eq!(ui.msgs.len(), orig_msgs(&o), "seed {seed}: messages");
    });
}
