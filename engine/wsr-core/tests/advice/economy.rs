//! Differential tests of BuildMenuAdviceText (0x557bbf): economy section.
use wsr_core::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::common::*;

#[test]
fn mode2_economy_cases_match_original() {
    par_seeds(1..=100, |seed| {
        let mut g = advice_world(seed, 2);
        g.sel_industry = if seed % 2 == 0 {
            cast::<i32, _>(seed % 71 + 1)
        } else {
            0
        };
        if seed % 3 == 0 {
            g.subprime_crisis_state = 1 + cast::<i16, _>(seed % 10);
        }
        if seed % 5 == 0 {
            g.pandemic_state = 1;
        }
        if seed % 7 == 0 {
            g.oil_crisis_level = cast::<f32, _>(Ext::from_f64(1.0));
            g.oil_crisis_stage = 5;
        }
        if seed % 11 == 0 {
            g.gdp_growth = cast::<f32, _>(Ext::from_f64(0.0));
        }

        check(seed, &g, cast::<u32, _>(seed));
    });
}

#[test]
fn mode2_market_trends_match_original() {
    par_seeds(1..=60, |seed| {
        let mut g = advice_world(seed, 2);
        g.sel_industry = 0;
        for series in [0, 1601, 1602] {
            g.cap_hist_close[(0, series)] = Ext::from_f64(60.0);
            for month in 1..=60 {
                let direction = match (seed + cast::<u64, _>(series)) % 4 {
                    0 => Ext::from_f64(1.0),
                    1 => -Ext::from_f64(1.0),
                    2 => {
                        if month > 54 {
                            Ext::from_f64(1.0)
                        } else {
                            -Ext::from_f64(1.0)
                        }
                    }
                    _ => Ext::from_f64(0.0),
                };
                let base = if series == 0 { Ext::from_f64(1000.0) } else { Ext::from_f64(10.0) };
                let scale = if series == 0 { Ext::from_f64(10.0) } else { Ext::from_f64(0.05) };
                g.cap_hist_close[(month, series)] = base + direction * cast::<Ext, _>(month) * scale;
            }
        }
        check(seed, &g, cast::<u32, _>(seed));
    });
}

#[test]
fn mode2_full_matches_original() {
    let __content = wsr_core::content::current();
    let exercised = std::sync::atomic::AtomicU32::new(0);
    par_seeds(1..=60, |seed| {
        let mut g = advice_world(seed, 2);
        if seed % 2 == 0 {
            g.sel_industry = cast::<i32, _>(seed % 71 + 1);
        }
        if seed % 3 == 0 {
            g.subprime_crisis_state = 1 + cast::<i16, _>(seed % 10);
        }
        if seed % 5 == 0 {
            g.pandemic_state = 1;
        }
        if seed % 7 == 0 {
            g.oil_crisis_level = cast::<f32, _>(Ext::from_f64(1.0));
            g.oil_crisis_stage = 5;
        }
        if let Some(p) = check(seed, &g, cast::<u32, _>(seed)) {
            if p.help_text.to_display().contains(__content.string(13550)) {
                exercised.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
    });
    if super::diff::unavailable().is_none() {
        assert!(exercised.into_inner() > 0, "economy body not exercised");
    }
}

#[test]
fn mode2_commodity_trends_match_original() {
    par_seeds(1..=40, |seed| {
        let mut g = advice_world(seed, 2);
        g.sel_industry = 0;
        for series in [6, 7, 8, 9, 10, 1605, 1606] {
            g.cap_hist_close[(0, series)] = Ext::from_f64(60.0);
            for month in 1..=60 {
                let direction = match (seed + cast::<u64, _>(series)) % 4 {
                    0 => Ext::from_f64(1.0),
                    1 => -Ext::from_f64(1.0),
                    2 => {
                        if month > 54 {
                            Ext::from_f64(1.0)
                        } else {
                            -Ext::from_f64(1.0)
                        }
                    }
                    _ => Ext::from_f64(0.0),
                };
                g.cap_hist_close[(month, series)] = Ext::from_f64(100.0) + direction * cast::<Ext, _>(month) * Ext::from_f64(0.8);
            }
        }
        check(seed, &g, cast::<u32, _>(seed));
    });
}
