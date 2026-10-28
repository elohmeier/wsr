//! List-style report screens and price alerts: the personal financial statement, portfolio, option and
//! futures positions, top-companies / industry / headline lists, player standings, industry growth
//! projections, bank-loan customers, advances, and the price-alert checks.
//!
//! One submodule per group of routines; everything is re-exported here (`logic::reports_lists::*`).

// Statements keep the original BASIC shape on purpose.
#![allow(
    clippy::assign_op_pattern,
    clippy::manual_range_contains,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::manual_clamp,
    clippy::needless_late_init,
    clippy::collapsible_if,
    clippy::collapsible_else_if
)]
use crate::pb::real::{Ext, cast};


mod alerts;
mod futures_pos;
mod industry;
mod pfs;
mod portfolio;
mod projections;
mod standings;

pub use alerts::*;
pub use futures_pos::*;
pub use industry::*;
pub use pfs::*;
pub use portfolio::*;
pub use projections::*;
pub use standings::*;

use crate::pb::numeric::format;
use crate::pb::numeric::str_num;
use crate::pb::pbstr::PbStr;
use crate::pb::string::{ltrim, right, rtrim};

/// `PbStr` from a literal.
#[allow(dead_code)]
pub(crate) fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

/// `RTRIM$(LTRIM$(STR$(x)))` (STR$ with 15 digits, i.e. LONG/QUAD/EXT).
#[allow(dead_code)]
pub(crate) fn str_trim(x: Ext) -> PbStr {
    let t = PbStr::from(str_num(x, 15).as_str());
    rtrim(ltrim(t.as_bytes()).as_bytes())
}

/// `STR$(x)` (15 digits).
#[allow(dead_code)]
pub(crate) fn str15(x: Ext) -> PbStr {
    PbStr::from(str_num(x, 15).as_str())
}

/// `FORMAT$(x, mask)` as a [`PbStr`].
#[allow(dead_code)]
pub(crate) fn fmt(x: Ext, mask: &str) -> PbStr {
    PbStr(format(x, mask.as_bytes()))
}

/// `RTRIM$(LTRIM$(FORMAT$(x, mask)))`.
#[allow(dead_code)]
pub(crate) fn fmt_trim(x: Ext, mask: &str) -> PbStr {
    let t = fmt(x, mask);
    rtrim(ltrim(t.as_bytes()).as_bytes())
}

/// `RIGHT$("000" + RTRIM$(LTRIM$(STR$(id))), 4)`: the 4-digit id of a `@C` link code.
#[allow(dead_code)]
pub(crate) fn link_id(id: i32) -> PbStr {
    let t = s("000") + &str_trim(cast::<Ext, _>(id));
    right(t.as_bytes(), 4)
}

/// `ARRAY SORT` (PB_ArraySort 0x6bf6a9) of `keys` (the elements from the start index on, `FOR n`), with an
/// optional `TAGARRAY` slice moved along (same positions). The runtime's recursive quicksort (Hoare
/// partition around a copy of the middle element) is reproduced exactly, so equal keys end up in the
/// original's order. `less(a, b)` is the element comparison (`a < b`); `descend` = `DESCEND`.
#[allow(dead_code)]
pub(crate) fn pb_array_sort<K: Clone, T>(
    keys: &mut [K],
    mut tags: Option<&mut [T]>,
    descend: bool,
    less: &dyn Fn(&K, &K) -> bool,
) {
    fn sort<K: Clone, T>(
        a: &mut [K],
        tags: &mut Option<&mut [T]>,
        lo: isize,
        hi: isize,
        before: &dyn Fn(&K, &K) -> bool,
    ) {
        if hi < lo {
            return; // (count 0: the partition loop does not run)
        }
        let pivot = a[cast::<usize, _>(lo + (hi - lo) / 2)].clone();
        let (mut i, mut j) = (lo, hi);
        while i <= j {
            while before(&a[cast::<usize, _>(i)], &pivot) && i < hi {
                i += 1;
            }
            while before(&pivot, &a[cast::<usize, _>(j)]) && j > lo {
                j -= 1;
            }
            if i > j {
                break;
            }
            if i != j {
                a.swap(cast::<usize, _>(i), cast::<usize, _>(j));
                if let Some(t) = tags.as_deref_mut() {
                    t.swap(cast::<usize, _>(i), cast::<usize, _>(j));
                }
            }
            i += 1;
            j -= 1;
        }
        if lo < j {
            sort(a, tags, lo, j, before);
        }
        if i < hi {
            sort(a, tags, i, hi, before);
        }
    }
    let n = cast::<isize, _>(keys.len());
    if descend {
        sort(keys, &mut tags, 0, n - 1, &|x: &K, y: &K| less(y, x));
    } else {
        sort(keys, &mut tags, 0, n - 1, less);
    }
}
