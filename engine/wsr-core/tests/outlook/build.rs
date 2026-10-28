//! Differential tests of BuildIndustryOutlookText (0x56f33b).
use wsr_core::pb::real::cast;


use super::common::{Rec, cmp, orig_msgs, run};
use super::diff::par_seeds;
use super::world::outlook_world;
use wsr_core::logic::outlook::build_industry_outlook_text;
use wsr_core::pb::rng::Rng;

#[test]
fn diff_build_industry_outlook() {
    let __content = wsr_core::content::current();
    // Cover every industry, including banks, insurers, holding companies and ETFs,
    // across multiple economic states. The complete state and RNG are compared as
    // well as the generated text because the original also updates scratch values.
    par_seeds(0..71 * 8, |seed| {
        let industry = 1 + cast::<i32, _>(seed % 71);
        let g = outlook_world(seed);
        let rng_state = (cast::<u32, _>(seed)).wrapping_mul(1_103_515_245).wrapping_add(12_345);
        let arg = format!("{industry}");
        let Some(o) = run(0x56f33b, &[&arg], &g, rng_state, &[], &[]) else { return };
        let mut p = g.clone();
        let mut ui = Rec::default();
        let mut rng = Rng::from_state(rng_state);
        build_industry_outlook_text(&mut p, &mut ui, &mut rng, industry);
        let errors: Vec<_> = o.log.iter().filter(|l| l.starts_with("PBERR")).collect();
        assert!(errors.is_empty(), "seed {seed}{__wsr_text_1975}{industry}: original runtime errors: {errors:?}",__wsr_text_1975=__content.string(1975));
        assert_eq!(p.ind_outlook_text, o.globals.ind_outlook_text, "seed {seed}{__wsr_text_1975}{industry}: text",__wsr_text_1975=__content.string(1975));
        let d = cmp().diffs(&p, &o.globals);
        assert!(d.is_empty(), "seed {seed}{__wsr_text_1975}{industry}: {} differences:\n{}", d.len(), d.iter().take(30).cloned().collect::<Vec<_>>().join("\n"),__wsr_text_1975=__content.string(1975));
        assert_eq!(rng.state(), o.rng, "seed {seed}{__wsr_text_1975}{industry}: RND state",__wsr_text_1975=__content.string(1975));
        assert_eq!(ui.msgs.len(), orig_msgs(&o), "seed {seed}{__wsr_text_1975}{industry}: message boxes",__wsr_text_1975=__content.string(1975));
    });
}
