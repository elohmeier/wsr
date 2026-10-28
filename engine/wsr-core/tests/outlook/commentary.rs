//! Differential tests of the commentary builders IndOutlook_GrowthCommentary (0x583605),
//! IndOutlook_SlowEconomyCommentary (0x586441) and IndOutlook_CostCommentary (0x5878e4): every
//! operating industry (3..69) in several economic regimes, comparing the returned text byte for byte,
//! the globals and the RND state with the original's.
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use super::world::{outlook_world, pick};
use std::sync::Mutex;
use wsr_core::logic::outlook::{
    ind_outlook_cost_commentary, ind_outlook_growth_commentary, ind_outlook_slow_economy_commentary,
};
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::Ui;

const GROWTH: u32 = 0x583605;
const SLOW: u32 = 0x586441;
const COST: u32 = 0x5878e4;

type Builder = fn(&mut Globals, &mut dyn Ui, &mut Rng, i16, &mut i32, &mut f32, &mut f32, &mut PbStr) -> PbStr;

/// Industry display names as BuildIndustryOutlookText passes them (mixed case; "tv" for 65).
fn ind_display_name(ind: i32) -> String {
    let __content = wsr_core::content::current();
    match ind {
        3 => "Auto & Truck".into(),
        5 => "Homebuilding".into(),
        6 => "Lumber/Building Materials".into(),
        43 => __content.string(7057).into(),
        65 => "Cable tv & Satellite tv".into(),
        _ => format!("{__wsr_text_13937}{ind}{__wsr_text_13938}",__wsr_text_13937=__content.string(13937), __wsr_text_13938=__content.string(13938)),
    }
}

/// A SINGLE value with a short decimal representation (exact through the harness' `f:` spec).
fn single(r: &mut Rand, lo: Ext, hi: Ext) -> f32 {
    let v = lo + (hi - lo) * cast::<Ext, _>(r.range(0, 10_000)) / Ext::from_f64(10_000.0);
    cast::<f32, _>(cast::<f32, _>((v * Ext::from_f64(4.0)).round()) / Ext::from_f64(4.0))
}

/// Inputs for one run: the world and the BYREF arguments.
struct Case {
    g: Globals,
    ind: i16,
    rating: i32,
    margin: f32,
    proj: f32,
    name: String,
    rng: u32,
}

/// Regime `variant` (0..8) for routine `which` (0 growth, 1 slow, 2 cost).
fn case(seed: u64, which: u32) -> Case {
    let ind = 3 + cast::<i32, _>(seed % 67);
    let variant = (seed / 67) % 8;
    let mut g = outlook_world(seed * 3 + cast::<u64, _>(which));
    let mut r = Rand(seed.wrapping_mul(0x9e37_79b9) ^ (cast::<u64, _>(which) + 1) * 0x5555);
    let (gdp, mut margin) = match (which, variant) {
        // growth: A margin > 19.4 & gdp < 3; B margin < 10.4 & gdp >= 3; C margin > 19.4 & gdp >= 3
        (0, 0 | 1 | 2) => (single(&mut r, -Ext::from_f64(2.0), Ext::from_f64(2.99)), single(&mut r, Ext::from_f64(19.5), Ext::from_f64(40.0))),
        (0, 3 | 4 | 5) => (single(&mut r, Ext::from_f64(3.0), Ext::from_f64(7.0)), single(&mut r, -Ext::from_f64(15.0), Ext::from_f64(10.3))),
        (0, 6) => (single(&mut r, Ext::from_f64(3.0), Ext::from_f64(7.0)), single(&mut r, Ext::from_f64(19.5), Ext::from_f64(40.0))),
        // slow: boom (gdp > 4.8, margin > 19.4) / slump (gdp <= 1.8, margin < 10.4)
        (1, 0 | 1 | 2) => (single(&mut r, Ext::from_f64(4.9), Ext::from_f64(8.0)), single(&mut r, Ext::from_f64(19.5), Ext::from_f64(40.0))),
        (1, 3 | 4 | 5 | 6) => (single(&mut r, -Ext::from_f64(3.0), Ext::from_f64(1.8)), single(&mut r, -Ext::from_f64(10.0), Ext::from_f64(10.3))),
        // cost: margin below 10.4 or below proj + 1
        (2, 0..=5) => (single(&mut r, -Ext::from_f64(6.0), Ext::from_f64(6.0)), single(&mut r, -Ext::from_f64(10.0), Ext::from_f64(25.0))),
        _ => (single(&mut r, -Ext::from_f64(6.0), Ext::from_f64(8.0)), single(&mut r, -Ext::from_f64(15.0), Ext::from_f64(40.0))),
    };
    g.gdp_growth = cast::<f32, _>(gdp);
    let mut proj = if r.chance(Ext::from_f64(0.5)) { margin + single(&mut r, -Ext::from_f64(12.0), Ext::from_f64(12.0)) } else { single(&mut r, -Ext::from_f64(15.0), Ext::from_f64(30.0)) };
    g.oil_price = cast::<f32, _>(pick(&mut r, &[Ext::from_f64(0.5), Ext::from_f64(0.6), Ext::from_f64(0.72), Ext::from_f64(0.8), Ext::from_f64(0.85), Ext::from_f64(0.9), Ext::from_f64(0.95), Ext::from_f64(1.0), Ext::from_f64(1.05), Ext::from_f64(1.1), Ext::from_f64(1.15), Ext::from_f64(1.2), Ext::from_f64(1.25), Ext::from_f64(1.3), Ext::from_f64(1.5)]));
    g.corn_price = cast::<f32, _>(single(&mut r, Ext::from_f64(2.0), Ext::from_f64(8.0)));
    g.wheat_price = cast::<f32, _>(single(&mut r, Ext::from_f64(2.0), Ext::from_f64(10.0)));
    g.base_interest_rate = cast::<f32, _>(single(&mut r, Ext::from_f64(2.0), Ext::from_f64(11.0)));
    g.subprime_crisis_state = pick(&mut r, &[0i16, 2, 3, 4, 5, 6, 7]);
    g.corp_tax_rate = pick(&mut r, &[30, 40, 50, 55]);
    if r.chance(Ext::from_f64(0.3)) {
        g.fad_industry = ind;
        g.fad_valuation_pct = pick(&mut r, &[51, 77, 99, 100, 120]);
    }
    // Pin rare combinations so the branch coverage assertions are deterministic.
    if which == 0 && seed == 8 { // Railroads, profitable despite a slowing economy.
        g.oil_price = cast::<f32, _>(Ext::from_f64(1.3));
        g.cap_hist_close[(0, 1604)] = Ext::from_f64(10.0);
        g.cap_hist_close[(10, 1604)] = Ext::from_f64(1.0);
        g.cap_hist_close[(9, 1604)] = Ext::from_f64(2.0);
        g.cap_hist_close[(8, 1604)] = Ext::from_f64(3.0);
        g.cap_hist_close[(7, 1604)] = Ext::from_f64(4.0);
    }
    if which == 2 && seed == 14 { // Packaged foods, high grain and energy costs.
        g.oil_price = cast::<f32, _>(Ext::from_f64(1.3));
        g.corn_price = cast::<f32, _>(Ext::from_f64(7.0));
        g.wheat_price = cast::<f32, _>(Ext::from_f64(8.0));
        margin = 5.0f32;
        proj = 5.0f32;
    }
    let rating = cast::<i32, _>(r.range(0, 10));
    let rng = cast::<u32, _>(r.range(0, 0xff_ffff)) * 0x100 + 7;
    Case { g, ind: cast::<i16, _>(ind), rating, margin, proj, name: ind_display_name(ind), rng }
}

/// Runs routine `addr` / `port` on `c` in both and compares; returns the text.
fn check(what: &str, addr: u32, port: Builder, c: &Case) -> Option<String> {
    let args = [
        format!("i:{}", c.ind),
        format!("{}", c.rating),
        format!("f:{}", c.margin),
        format!("f:{}", c.proj),
        format!("s:{}", c.name),
    ];
    let args: Vec<&str> = args.iter().map(|a| a.as_str()).collect();
    let o = run(addr, &args, &c.g, c.rng, &[], &[])?;
    let mut g = c.g.clone();
    let mut ui = Rec::default();
    let mut rng = Rng::from_state(c.rng);
    let (mut rating, mut margin, mut proj, mut name) = (c.rating, c.margin, c.proj, PbStr::from(c.name.as_str()));
    let text = port(&mut g, &mut ui, &mut rng, c.ind, &mut rating, &mut margin, &mut proj, &mut name);
    let errs: Vec<&String> = o.log.iter().filter(|l| l.starts_with("PBERR")).collect();
    assert!(errs.is_empty(), "{what}: runtime errors in the original: {errs:?}");
    assert_eq!(Some(&text), o.return_str.as_ref().or(Some(&PbStr::new())), "{what}: text");
    let d = cmp().diffs(&g, &o.globals);
    assert!(d.is_empty(), "{what}: {} differences:\n{}", d.len(), d.iter().take(30).cloned().collect::<Vec<_>>().join("\n"));
    assert_eq!(rng.state(), o.rng, "{what}: RND state");
    assert_eq!(ui.msgs.len(), orig_msgs(&o), "{what}: message boxes");
    Some(text.to_display())
}

fn diff_routine(which: u32, addr: u32, port: Builder, variants: u64, must: &[&str]) {
    let __content = wsr_core::content::current();
    let seen = Mutex::new(Vec::<String>::new());
    par_seeds(0..67 * variants, |seed| {
        let c = case(seed, which);
        let what = format!("{addr:#x} seed {seed} ind {} gdp {}{__wsr_text_13939}{} proj {}", c.ind, c.g.gdp_growth, c.margin, c.proj,__wsr_text_13939=__content.string(13939));
        if let Some(t) = check(&what, addr, port, &c) {
            seen.lock().unwrap().push(t);
        }
    });
    let seen = seen.into_inner().unwrap();
    if seen.is_empty() || std::env::var_os("WSR_ONLY_SEED").is_some() {
        return; // harness unavailable / single seed
    }
    let non_empty = seen.iter().filter(|t| !t.is_empty()).count();
    assert!(non_empty * 4 > seen.len(), "too few texts: {non_empty}{__wsr_text_13578}{}", seen.len(),__wsr_text_13578=__content.string(13578));
    for m in must {
        assert!(seen.iter().any(|t| t.contains(m)), "branch not hit: {m:?}");
    }
}

#[test]
fn diff_growth_commentary() {
    let __content = wsr_core::content::current();
    diff_routine(
        0,
        GROWTH,
        ind_outlook_growth_commentary,
        8,
        &[
            __content.string(13940),
            "the demand for energy remains strong",
            __content.string(6845),
            __content.string(13941),
            __content.string(13942),
            __content.string(13943),
            "rolling along",
            __content.string(13944),
            __content.string(13945),
            __content.string(13946),
            __content.string(7698),
            __content.string(13947),
            __content.string(13948),
            __content.string(13949),
            __content.string(6980),
            __content.string(13950),
            __content.string(13951),
        ],
    );
}

#[test]
fn diff_slow_economy_commentary() {
    let __content = wsr_core::content::current();
    diff_routine(
        1,
        SLOW,
        ind_outlook_slow_economy_commentary,
        8,
        &[
            __content.string(7015),
            __content.string(13952),
            __content.string(13953),
            __content.string(1398),
            __content.string(13954),
            __content.string(13955),
            __content.string(13956),
            __content.string(13957),
            __content.string(13958),
            __content.string(13959),
            "severe",
            __content.string(7039),
            __content.string(13960),
            __content.string(13961),
            __content.string(13962),
            __content.string(13963),
        ],
    );
}

#[test]
fn diff_cost_commentary() {
    let __content = wsr_core::content::current();
    diff_routine(
        2,
        COST,
        ind_outlook_cost_commentary,
        8,
        &[
            __content.string(13964),
            __content.string(13965),
            __content.string(7074),
            __content.string(13966),
            __content.string(7086),
            __content.string(7310),
            __content.string(13967),
            __content.string(13968),
            __content.string(7116),
            "\"big box\"",
            __content.string(13969),
            __content.string(7123),
        ],
    );
}
