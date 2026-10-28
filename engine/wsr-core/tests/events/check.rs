//! Shared result check of the events tests.
#![allow(dead_code)]
use wsr_core::pb::real::cast;


use super::common::*;
use super::diff::*;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

/// Port result (globals, RND state, message boxes, news count) vs. the original's; panics on differences.
#[track_caller]
pub fn check_with(what: &str, c: &Compare, port: &Globals, port_rng: &Rng, ui: &Rec, o: &Original) {
    let d = c.diffs(port, &o.globals);
    let log: Vec<&String> = o
        .log
        .iter()
        .filter(|l| !l.starts_with("API"))
        .take(30)
        .collect();
    assert!(
        d.is_empty(),
        "{what}: {} differences:\n{}\nlog: {log:?}",
        d.len(),
        d.iter().take(30).cloned().collect::<Vec<_>>().join("\n")
    );
    assert_eq!(port_rng.state(), o.rng, "{what}: RND state");
    let errs: Vec<&String> = o.log.iter().filter(|l| l.starts_with("PBERR")).collect();
    assert!(
        errs.is_empty(),
        "{what}: runtime errors in the original: {errs:?}"
    );
    assert_eq!(
        ui.msgs.len(),
        orig_msgs(o),
        "{what}: message boxes {:?} vs log {log:?}",
        ui.msgs
    );
}

/// [`check_with`] with the default comparison ([`cmp`]).
#[track_caller]
pub fn check(what: &str, port: &Globals, port_rng: &Rng, ui: &Rec, o: &Original) {
    check_with(what, &cmp(), port, port_rng, ui, o)
}

/// A random active operating corp (131..1580, industry < 70).
pub fn active_corp(g: &Globals, r: &mut Rand) -> i32 {
    for _ in 0..100 {
        let c = cast::<i32, _>(r.range(131, 1580));
        if g.entity_status[c] == 1 && g.industry_num[c] < 70 {
            return c;
        }
    }
    131
}
