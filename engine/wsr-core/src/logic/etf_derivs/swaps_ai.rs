//! Interest-rate swap pricing and the computer side of the swap market: `CalcSwapFixedRate` (the fixed
//! rate quoted for a proposed swap), `AiCreateSwaps` (computer companies entering swaps with banks and
//! insurers) and `SwapRateShockEvent` (the central-bank shock aimed at players with large net swap
//! positions). Record layout of `SwapContracts()`: see [`crate::logic::swaps`].
//!
//! Numeric conventions as in `logic::finance`: SINGLE pool constants are written `sf(0.28)`, DOUBLE
//! locals are `f64`, CURRENCY locals use [`Cur`]; the association order follows the machine code.
//! The decompiled view of these routines carries misleading `(float)` casts: the DOUBLE locals are
//! never rounded to SINGLE (only constants are SINGLE).

// Statements keep the original BASIC shape on purpose.
#![allow(
    clippy::manual_range_contains,
    clippy::assign_op_pattern,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::if_same_then_else,
    clippy::neg_multiply
)]
use crate::pb::real::{Ext, cast};


use crate::logic::finance::{calc_entity_financials, get_managing_player};
use crate::logic::market::update_market_display;
use crate::logic::news::post_news;
use crate::logic::report::show_quick_message;
use crate::logic::swaps::{count_entity_swaps, find_free_swap_slot, sum_swap_notional};
use crate::pb::numeric::format;
use crate::pb::numeric::{str_num, val};
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{ltrim, mid, parse_any, replace, right, rtrim};
use crate::pb::numeric::{fix, frac, to_long, to_quad, Cur};
use crate::state::Globals;
use crate::ui::Ui;

/// A SINGLE constant of the original's pool (`fmul dword [..]`), widened.
fn sf(x: f32) -> Ext {
    cast::<Ext, _>(x)
}

/// `PARSE$(rec, ANY "|", n)`.
fn field(rec: &PbStr, n: i32) -> PbStr {
    parse_any(rec.as_bytes(), b"|", n)
}

/// `VAL(PARSE$(rec, ANY "|", n))`.
fn fval(rec: &PbStr, n: i32) -> Ext {
    val(field(rec, n).as_bytes())
}

/// `RTRIM$(LTRIM$(s))`.
fn trim(s: &[u8]) -> PbStr {
    rtrim(ltrim(s).as_bytes())
}

/// `RTRIM$(LTRIM$(STR$(x)))` with `digits` significant digits (15, or 7 for SINGLE).
fn str_trim(x: Ext, digits: u32) -> PbStr {
    trim(str_num(x, digits).as_bytes())
}

/// Port of CalcSwapFixedRate (0x6415dd).
///
/// The fixed rate (text `FORMAT$(rate, "* ##.00")`) quoted to `entity_id` for the proposed swap record
/// `swap_rec` (`long|short|…|rateType|start|end|…`): the current reference rate (long-bond yield, short-bond
/// yield or prime rate) minus a spread for the LONG side (receives fixed), plus it for the SHORT side. The
/// spread grows with the prime rate's distance from 6..9%, the forward start and the term, the number of
/// swaps already in effect (for the player's own quotes) and the difficulty level; when rates are extreme
/// (prime < 6% for a LONG, > 9% for a SHORT) a small, term-decreasing spread is used instead (negative
/// = a better rate than the reference). The rate is at least 1.
pub fn calc_swap_fixed_rate(g: &Globals, rng: &mut Rng, swap_rec: &PbStr, entity_id: i32) -> PbStr {
    let rec_text = swap_rec.clone();
    let _long_party = to_long(fval(&rec_text, 1));
    let short_party = to_long(fval(&rec_text, 2));
    let side: &[u8] = if short_party == entity_id { b"S" } else { b"L" };
    let rate_type = field(&rec_text, 4);
    let mut fixed_rate = Cur(0);
    if rate_type.as_bytes() == b"L" {
        fixed_rate.set(cast::<Ext, _>(g.long_bond_yield));
    } else if rate_type.as_bytes() == b"S" {
        fixed_rate.set(cast::<Ext, _>(g.short_bond_yield));
    } else if rate_type.as_bytes() == b"P" {
        fixed_rate.set(cast::<Ext, _>(g.prime_rate));
    }
    let start_date_text = field(&rec_text, 5);
    let mut tmp_val: Ext = val(start_date_text.as_bytes());
    let start_time: Ext = fix(tmp_val) + frac(tmp_val) * sf(2.5f32);
    let end_date_text = field(&rec_text, 6);
    tmp_val = val(end_date_text.as_bytes());
    let end_time: Ext = fix(tmp_val) + frac(tmp_val) * sf(2.5f32);
    let now_time: Ext = cast::<Ext, _>(g.cur_quarter) * sf(0.25f32) + cast::<Ext, _>(g.cur_year);
    let prime = cast::<Ext, _>(g.prime_rate);
    let mut extreme_mode = 0;
    if side == b"L" && Ext::from_f64(6.0) > prime {
        extreme_mode = 1;
    }
    if side == b"S" && Ext::from_f64(9.0) < prime {
        extreme_mode = 2;
    }
    let mut spread: Ext = Ext::from_f64(0.0);
    let mut start_years: Ext;
    let mut end_years: Ext;
    if !(extreme_mode > 0) {
        let year_digit = val(right(str_num(cast::<Ext, _>(g.cur_year), 15).as_bytes(), 1).as_bytes());
        let end_digit = val(right(end_date_text.as_bytes(), 1).as_bytes());
        spread = (year_digit + end_digit + Ext::from_f64(2.0)) * sf(0.01f32);
        spread = spread + sf(0.28f32);
        let easing_target = !(g.base_interest_rate >= g.interest_rate_target);
        if side == b"L" {
            if g.monetary_policy == 2 {
                spread = spread - sf(0.07f32);
            } else if g.monetary_policy == 0 {
                spread = spread + sf(0.15f32);
            }
            if easing_target {
                spread = spread - sf(0.07f32);
            } else {
                spread = spread + sf(0.11f32);
            }
        } else {
            if g.monetary_policy == 2 {
                spread = spread + sf(0.15f32);
            } else if g.monetary_policy == 0 {
                spread = spread - sf(0.07f32);
            }
            if easing_target {
                spread = spread + sf(0.1f32);
            } else {
                spread = spread - sf(0.06f32);
            }
        }
        // rateSel = PrimeRate (EXT)
        if !(prime >= sf(2.75f32)) {
            spread = spread + Ext::from_f64(3.0);
        } else if !(prime >= sf(3.25f32)) {
            spread = spread + sf(2.8f32);
        } else if !(prime >= Ext::from_f64(4.0)) {
            spread = spread + sf(2.6f32);
        } else if !(prime >= Ext::from_f64(5.0)) {
            spread = spread + sf(2.3f32);
        } else if !(prime >= Ext::from_f64(6.0)) {
            spread = spread + sf(1.5f32);
        } else if !(prime > Ext::from_f64(9.0)) {
            spread = spread + sf(0.5f32);
            tmp_val = if side == b"L" { prime - sf(7.5f32) } else { sf(7.5f32) - prime };
            spread = tmp_val * sf(0.5f32) + spread;
        } else if !(prime >= Ext::from_f64(10.0)) {
            spread = spread + sf(1.5f32);
        } else if !(prime >= sf(13.5f32)) {
            // asm: (spread + 2.5) + (PrimeRate - 10) * 0.1
            spread = (spread + sf(2.5f32)) + (prime - Ext::from_f64(10.0)) * sf(0.1f32);
        } else {
            spread = (spread + Ext::from_f64(3.0)) + (prime - Ext::from_f64(10.0)) * sf(0.13f32);
        }
        if !(sf(0.5f32) >= spread) && !(Ext::from_f64(6.0) > prime) && !(Ext::from_f64(9.0) < prime) {
            spread = sf(0.7f32) * spread + sf(0.075f32);
        }
        start_years = (start_time - now_time) * sf(0.03f32);
        end_years = (end_time - now_time) * sf(0.03f32);
        spread = start_years + spread + end_years;
        if g.active_entity != entity_id {
            spread = spread + sf(0.3f32);
        }
        if !(start_time - now_time > sf(0.26f32)) && end_time - now_time < sf(0.51f32) {
            spread = sf(0.8f32) * spread;
            if end_time - now_time < sf(0.26f32) {
                spread = sf(0.7f32) * spread;
            }
        }
        if sf(0.1f32) > spread {
            spread = Ext::from_f64(0.0);
        }
        if rate_type.as_bytes() == b"L" {
            spread = sf(1.05f32) * spread;
        }
        if rate_type.as_bytes() == b"S" {
            spread = sf(1.1f32) * spread;
        }
        let swap_count = to_long(val(g.swap_contracts[0].as_bytes()));
        if swap_count > 60 && g.active_entity == entity_id {
            let mut crowd_adj = swap_count - 60;
            if swap_count > 80 {
                crowd_adj = 20;
            }
            if swap_count > 90 {
                crowd_adj = swap_count - 80;
            }
            if swap_count > 100 {
                crowd_adj = swap_count - 100;
            }
            if swap_count > 110 {
                crowd_adj = 2 * g.cur_quarter + 8;
            }
            if swap_count > 123 {
                crowd_adj = 5;
            }
            tmp_val = sf(0.1f32) * rng.rnd() + cast::<Ext, _>(crowd_adj) * sf(0.1f32);
            spread = tmp_val + spread;
        }
        if !(sf(2.75f32) >= spread) {
            spread = sf(2.75f32);
        }
        start_years = start_time - now_time;
        end_years = end_time - now_time;
        if !(sf(2.25f32) >= spread) && sf(1.51f32) > start_years {
            if sf(1.51f32) > start_years {
                tmp_val = sf(0.95f32);
            }
            if sf(1.26f32) > start_years {
                tmp_val = sf(0.92f32);
            }
            if sf(1.01f32) > start_years {
                tmp_val = sf(0.88f32);
            }
            if sf(0.76f32) > start_years {
                tmp_val = sf(0.84f32);
            }
            if sf(0.51f32) > start_years {
                tmp_val = sf(0.8f32);
            }
            if sf(0.26f32) > start_years {
                tmp_val = sf(0.95f32);
            }
            spread = tmp_val * spread;
            if sf(1.51f32) >= end_years {
                if sf(1.51f32) > end_years {
                    tmp_val = sf(0.95f32);
                }
                if sf(1.26f32) > end_years {
                    tmp_val = sf(0.92f32);
                }
                if sf(1.01f32) > end_years {
                    tmp_val = sf(0.88f32);
                }
                if sf(0.76f32) > end_years {
                    tmp_val = sf(0.84f32);
                }
                if sf(0.51f32) > end_years {
                    tmp_val = sf(0.95f32);
                }
                spread = tmp_val * spread;
            }
        }
        tmp_val = sf(0.06f32) * end_years + sf(0.1f32) * start_years;
        spread = tmp_val + spread;
        if !(sf(3.4f32) >= spread) {
            spread = sf(3.4f32);
        }
        if Ext::from_f64(6.0) > prime && !(sf(2.25f32) >= spread) {
            spread = sf(2.25f32);
        }
        'fin: {
            if g.active_entity == entity_id && g.active_entity > 10 {
                let ind_short = cast::<i32, _>(g.industry_num[short_party]);
                if !(ind_short < 3) && ind_short != 37 {
                    break 'fin;
                }
                let ind_active = cast::<i32, _>(g.industry_num[g.active_entity]);
                if !(ind_active < 3) && ind_active != 37 {
                    break 'fin;
                }
                let base = cast::<Ext, _>(g.base_interest_rate);
                if !(sf(6.25f32) >= base) && !(Ext::from_f64(8.0) <= base) {
                    spread = sf(0.75f32) * spread;
                } else if !(sf(5.25f32) >= base) && !(sf(8.75f32) <= base) {
                    spread = sf(0.85f32) * spread;
                }
            }
        }
        if !(end_time - now_time <= Ext::from_f64(1.0)) {
            if !(sf(14.5f32) >= prime) {
                spread = (prime - sf(14.5f32)) * sf(0.12f32) + spread;
                if !(sf(3.75f32) >= spread) {
                    spread = sf(3.75f32);
                }
            } else if !(sf(4.75f32) <= prime) {
                spread = (sf(4.75f32) - prime) * sf(0.15f32) + spread;
                if !(sf(2.5f32) >= spread) {
                    spread = sf(2.35f32);
                }
            }
        }
        if !(Ext::from_f64(1.0) >= spread) {
            if g.difficulty_level == 2 {
                spread = spread + sf(0.1f32);
            }
            if g.difficulty_level == 3 {
                spread = spread + sf(0.1f32);
            }
        }
    }
    // LAB_006424e7
    start_years = start_time - now_time;
    end_years = end_time - now_time;
    tmp_val = end_years + start_years;
    if extreme_mode == 1 {
        spread = sf(0.5f32);
        if !(sf(4.4f32) >= prime) {
            if !(sf(5.4f32) >= prime) {
                spread = spread - sf(0.065f32) * tmp_val;
            } else if !(sf(4.9f32) >= prime) {
                spread = spread - sf(0.104f32) * tmp_val;
            } else if !(Ext::from_f64(4.0) >= prime) {
                spread = spread - sf(0.19f32) * tmp_val;
            }
            if sf(0.05f32) > spread {
                spread = -sf(0.1f32);
            }
        } else {
            spread = -sf(0.1f32);
        }
        if fixed_rate.get() < sf(3.1f32) {
            spread = -sf(0.1f32);
        }
    } else if extreme_mode == 2 {
        spread = sf(0.5f32);
        if !(sf(12.4f32) <= prime) {
            if !(Ext::from_f64(11.0) >= prime) {
                spread = spread - sf(0.19f32) * tmp_val;
            } else if !(Ext::from_f64(10.0) >= prime) {
                spread = spread - sf(0.104f32) * tmp_val;
            } else if !(Ext::from_f64(9.0) > prime) {
                spread = spread - sf(0.065f32) * tmp_val;
            }
            if sf(0.05f32) > spread {
                spread = -sf(0.1f32);
            }
        } else {
            spread = -sf(0.1f32);
        }
    }
    if side == b"L" {
        fixed_rate.set(fixed_rate.get() - spread);
    } else {
        fixed_rate.set(fixed_rate.get() + spread);
    }
    if fixed_rate.get() < Ext::from_f64(1.0) {
        fixed_rate.set(Ext::from_f64(1.0));
    }
    PbStr(format(fixed_rate.get(), b"* ##.00"))
}

/// Port of AiCreateSwaps (0x646a2d).
///
/// Called per simulated entity (`SimCoCount`): with a 10% chance a computer- [runtime resource wording]
/// fund 1581..1585, not an S&L) without swaps, a positive net worth and a credit rating of 7+ enters an
/// interest-rate swap while rates are outside 6.5..9% and fewer than 50 swaps exist. The notional is 10x
/// its net worth (30..100 billion when that exceeds 100 billion, adjusted for the exchange rate; at least
/// 10 billion). Up to 500 random counterparties (banks, insurers, S&Ls; not player-managed, at most 4
/// swaps, rating 7+) are tried; with base rates below 6.5% the company takes the SHORT side (pays fixed),
/// otherwise the LONG side, subject to the counterparty's existing net exposure. The fixed rate comes
/// from [`calc_swap_fixed_rate`]. `time_secs` = the seconds of `TIME$` (with 30..50+ swaps in effect the
/// routine only runs in the first 6 seconds of a minute).
pub fn ai_create_swaps(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    if !(rng.rnd() * Ext::from_f64(10.0) <= Ext::from_f64(1.0)) {
        return;
    }
    let corp_id = g.sim_co_count;
    if corp_id > 1580 && corp_id < 1586 {
        return;
    }
    if g.entity_status[corp_id] != 1 {
        return;
    }
    if !(g.corp_controller[corp_id] > 10) {
        return;
    }
    let base = cast::<Ext, _>(g.base_interest_rate);
    if !(sf(6.5f32) > base) && !(Ext::from_f64(9.0) < base) {
        return;
    }
    if !(val(g.swap_contracts[0].as_bytes()) < Ext::from_f64(50.0)) {
        return;
    }
    if g.industry_num[corp_id] == 37 {
        return;
    }
    let requester_id = g.sim_co_count;
    let limit = rng.rnd() * Ext::from_f64(20.0) + Ext::from_f64(30.0);
    if val(g.swap_contracts[0].as_bytes()) > limit {
        // VAL(RIGHT$(TIME$, 2)) > 5
        if cast::<Ext, _>(time_secs) > Ext::from_f64(5.0) {
            return;
        }
    }
    if count_entity_swaps(g, corp_id, 0) > 0 {
        return;
    }
    calc_entity_financials(g, ui, rng, corp_id);
    if !(g.net_worth > Ext::from_f64(0.0)) {
        return;
    }
    if !(g.credit_rating_num[corp_id] >= 7) {
        return;
    }
    let mut notional: i64 = to_quad(g.net_worth * Ext::from_f64(10.0));
    if cast::<Ext, _>(notional) > g.exchange_rate * Ext::from_f64(100000.0) {
        notional = to_quad((rng.rnd() * Ext::from_f64(70.0) * Ext::from_f64(1000.0) + Ext::from_f64(30000.0)) * g.exchange_rate);
    }
    if !(cast::<Ext, _>(notional) >= g.exchange_rate * Ext::from_f64(10000.0)) {
        return;
    }
    let mut counterparty_id = 0;
    let mut long_party = 0;
    let mut short_party = 0;
    let mut _position = PbStr::new();
    for attempt in 1..=500 {
        let mut candidate_id = rng.rnd_range(1600, 11);
        if g.entity_status[candidate_id] != 1 {
            continue;
        }
        if get_managing_player(g, candidate_id) > 0 {
            continue;
        }
        if candidate_id > 130 && g.industry_num[candidate_id] != 37 {
            // funds: only fund 1595, and only while it has no swaps
            if g.industry_num[candidate_id] != 71 {
                continue;
            }
            if g.entity_status[1595] != 1 || g.industry_num[1595] != 71 {
                continue;
            }
            candidate_id = 1595;
            if sum_swap_notional(g, 1595, 0) != Ext::from_f64(0.0) {
                continue;
            }
        }
        // LAB_00646ec0
        if count_entity_swaps(g, candidate_id, 0) > 4 {
            continue;
        }
        calc_entity_financials(g, ui, rng, candidate_id);
        if !(g.credit_rating_num[candidate_id] >= 7) {
            continue;
        }
        let cp_industry = g.industry_num[candidate_id];
        if attempt < 10 {
            // Bug 178: compares the attempt counter (not the candidate) with 1595, never true.
            if cp_industry == 71 && attempt == 1595 {
                counterparty_id = candidate_id;
                if count_entity_swaps(g, candidate_id, 0) > 0 {
                    counterparty_id = 0;
                }
            }
        } else if attempt < 30 {
            if cp_industry == 37 {
                counterparty_id = candidate_id;
            }
        } else if attempt < 60 {
            if cp_industry == 1 || cp_industry == 37 {
                counterparty_id = candidate_id;
            }
        } else if cp_industry < 3 || cp_industry == 37 {
            counterparty_id = candidate_id;
        }
        if counterparty_id > 0 {
            let sum = to_quad(sum_swap_notional(g, counterparty_id, 1));
            // NetWorth is the candidate's here (last CalcEntityFinancials)
            let nw10 = g.net_worth * Ext::from_f64(10.0);
            if sf(6.5f32) > cast::<Ext, _>(g.base_interest_rate) {
                if !(g.exchange_rate * Ext::from_f64(100000.0) >= cast::<Ext, _>(notional + sum)) {
                    counterparty_id = 0;
                    continue;
                }
                if !(nw10 >= cast::<Ext, _>(notional + sum)) {
                    counterparty_id = 0;
                    continue;
                }
                _position = PbStr::from("SHORT");
                long_party = counterparty_id;
                short_party = requester_id;
            } else {
                if !(sum > 0) && !(g.exchange_rate * Ext::from_f64(100000.0) >= cast::<Ext, _>(notional) + (cast::<Ext, _>(sum)).abs()) {
                    counterparty_id = 0;
                    continue;
                }
                if !(sum > 0) && !(nw10 >= cast::<Ext, _>(notional) + (cast::<Ext, _>(sum)).abs()) {
                    counterparty_id = 0;
                    continue;
                }
                if sum > 0 && !(nw10 >= cast::<Ext, _>(notional - sum)) {
                    counterparty_id = 0;
                    continue;
                }
                _position = PbStr::from("LONG");
                long_party = requester_id;
                short_party = counterparty_id;
            }
        }
        if counterparty_id > 10 {
            break;
        }
    }
    if counterparty_id == 0 {
        return;
    }
    let short_bond_year = to_long(val(mid(g.short_bond_desc.as_bytes(), 1, 4).as_bytes()));
    let mut start_date: f32 = cast::<f32, _>(cast::<f32, _>((sf(0.2f32) + cast::<Ext, _>(g.cur_year)) + cast::<Ext, _>(g.cur_quarter) * sf(0.1f32)));
    if g.cur_quarter > 2 {
        start_date = cast::<f32, _>(sf(1.2f32) + cast::<Ext, _>(g.cur_year));
    }
    let mut end_date: f32 = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(2.0) + cast::<Ext, _>(start_date)));
    if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
        end_date = cast::<f32, _>(Ext::from_f64(1.0) + cast::<Ext, _>(end_date));
    }
    let mut rate_type = PbStr::from("P");
    if rng.rnd() * Ext::from_f64(4.0) < Ext::from_f64(1.0) && cast::<Ext, _>(short_bond_year) > cast::<Ext, _>(end_date) {
        rate_type = PbStr::from("S");
    }
    if rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) {
        rate_type = PbStr::from("L");
    }
    let mut swap_rec = str_trim(cast::<Ext, _>(long_party), 15);
    swap_rec.0.push(b'|');
    swap_rec.0.extend_from_slice(str_trim(cast::<Ext, _>(short_party), 15).as_bytes());
    swap_rec.0.extend_from_slice(b"|XXXX|");
    swap_rec.0.extend_from_slice(rate_type.as_bytes());
    swap_rec.0.push(b'|');
    swap_rec.0.extend_from_slice(str_trim(cast::<Ext, _>(start_date), 7).as_bytes());
    swap_rec.0.push(b'|');
    swap_rec.0.extend_from_slice(str_trim(cast::<Ext, _>(end_date), 7).as_bytes());
    swap_rec.0.push(b'|');
    swap_rec.0.extend_from_slice(str_trim(cast::<Ext, _>(notional), 15).as_bytes());
    let fixed_rate_text = calc_swap_fixed_rate(g, rng, &swap_rec, requester_id);
    swap_rec = replace(swap_rec.as_bytes(), b"XXXX", trim(fixed_rate_text.as_bytes()).as_bytes());
    let slot = find_free_swap_slot(g, ui);
    if slot == 0 {
        return;
    }
    g.swap_contracts[slot] = swap_rec;
    let swap_count = to_long(val(g.swap_contracts[0].as_bytes())) + 1;
    g.swap_contracts[0] = str_trim(cast::<Ext, _>(swap_count), 15);
}

/// Port of SwapRateShockEvent (0x647de7).
///
/// Anti-exploit event, in two phases. While no shock is scheduled (`FedRateShockTick = 0`, or in two of
/// four quarters), with 50+ swaps in effect each human player (1 in 3 chance) whose net LONG notional
/// (own swaps and those of companies he controls directly) is at least 45 billion schedules a
/// tightening (`FedRateShockTick` 1420..1570, base rate below 13%), a net SHORT position an easing
/// (`-1550`, base rate above 4.7%). On a later call the shock fires: tightening raises the base rate by
/// 3..4 points ("Interest rates soar …"), easing cuts it by 37% (2..5.7 points, "Rumors of more Q.E. …");
/// the prime rate and rate target follow, and the market display and news are updated.
/// `time_secs` is passed to `UpdateMarketDisplay`.
pub fn swap_rate_shock_event(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    let __content = crate::content::current();
    if g.fed_rate_shock_tick == 0 || (g.cur_year + g.cur_quarter) % 4 <= 1 {
        if !(val(g.swap_contracts[0].as_bytes()) >= Ext::from_f64(50.0)) {
            return;
        }
        let num_players = g.num_players;
        let mut player_id = 1;
        while player_id <= num_players {
            let mut long_total: i64 = 0;
            let mut short_total: i64 = 0;
            if g.is_computer_player[player_id] == 1 {
                player_id += 1;
                continue;
            }
            if !(rng.rnd() * Ext::from_f64(3.0) <= Ext::from_f64(1.0)) {
                player_id += 1;
                continue;
            }
            let n = to_long(val(g.swap_contracts[0].as_bytes()));
            for slot in 1..=n {
                let swap_rec = g.swap_contracts[slot].clone();
                if swap_rec.is_empty() {
                    continue;
                }
                let long_party = to_long(fval(&swap_rec, 1));
                let short_party = to_long(fval(&swap_rec, 2));
                if player_id == long_party || player_id == g.corp_controller[long_party] {
                    long_total = to_quad(fval(&swap_rec, 7) + cast::<Ext, _>(long_total));
                }
                if player_id == short_party || player_id == g.corp_controller[short_party] {
                    short_total = to_quad(fval(&swap_rec, 7) + cast::<Ext, _>(short_total));
                }
            }
            let net_long = to_quad(cast::<Ext, _>(long_total - short_total));
            if (cast::<Ext, _>(net_long)).abs() >= g.exchange_rate * Ext::from_f64(45_000_000.0) {
                if net_long > 0 && Ext::from_f64(13.0) > cast::<Ext, _>(g.base_interest_rate) {
                    g.fed_rate_shock_tick = to_long(rng.rnd() * Ext::from_f64(150.0) + Ext::from_f64(1420.0));
                } else if net_long < 0 && !(sf(4.7f32) >= cast::<Ext, _>(g.base_interest_rate)) {
                    g.fed_rate_shock_tick = -1550;
                }
                if g.fed_rate_shock_tick != 0 {
                    break;
                }
            }
            player_id += 1;
        }
        return;
    }
    let bank_name = g.central_bank_name.clone();
    let mut rate_change: Ext = Ext::from_f64(0.0);
    let base = cast::<Ext, _>(g.base_interest_rate);
    if g.fed_rate_shock_tick > 0 {
        let gdp = cast::<Ext, _>(g.gdp_growth);
        let target = cast::<Ext, _>(g.interest_rate_target);
        if rng.rnd() * Ext::from_f64(5.0) + -Ext::from_f64(8.0) > gdp && Ext::from_f64(6.0) < target && target + gdp.abs() > Ext::from_f64(14.0) {
            return;
        }
        // Bug 179: three IFs without ELSE; for base rates below 17% the last one always wins.
        if sf(9.3f32) > base {
            rate_change = base * sf(0.4f32);
        }
        if sf(13.2f32) > base {
            rate_change = rng.rnd() * sf(1.5f32) + sf(4.8f32);
        }
        if Ext::from_f64(17.0) > base {
            rate_change = rng.rnd() * Ext::from_f64(1.0) + Ext::from_f64(3.0);
            let mut msg = PbStr::from(__content.string(4955));
            msg.0.extend_from_slice(bank_name.as_bytes());
            msg.0.extend_from_slice(__content.bytes(4956));
            g.news_msg = msg;
            if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                g.news_msg = replace(g.news_msg.as_bytes(), b"soar", b"rise");
            }
            if g.news_msg.len() < 70 && g.monetary_policy == 2 {
                g.news_msg.0.extend_from_slice(__content.bytes(4957));
            }
            g.monetary_policy = 2;
            g.rate_soar_news_count += 1;
            if g.rate_soar_news_count != 1 && g.rate_soar_news_count != 8 {
                g.news_msg = PbStr::new();
            }
        }
    } else if !(sf(4.5f32) > base) {
        rate_change = -sf(0.37f32) * base;
        if rate_change.abs() > sf(6.7f32) {
            rate_change = -Ext::from_f64(1.0) * (sf(0.7f32) * rng.rnd() + Ext::from_f64(5.0));
        }
        if rate_change.abs() < Ext::from_f64(2.0) {
            rate_change = -Ext::from_f64(2.0);
        }
        let mut msg = PbStr::from(__content.string(4958));
        msg.0.extend_from_slice(bank_name.as_bytes());
        msg.0.extend_from_slice(__content.bytes(4959));
        g.news_msg = msg;
        if g.monetary_policy > 0 {
            g.news_msg = replace(g.news_msg.as_bytes(), b"more", b"resumed");
        }
        g.monetary_policy = 0;
        g.qe_rumor_count += 1;
        if g.qe_rumor_count != 1 && g.qe_rumor_count != 10 {
            g.news_msg = PbStr::new();
        }
    }
    g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(rate_change + cast::<Ext, _>(g.base_interest_rate)));
    if sf(2.25f32) > cast::<Ext, _>(g.base_interest_rate) {
        g.base_interest_rate = cast::<f32, _>(Ext::from_f32(2.25f32));
    }
    if rate_change != Ext::from_f64(0.0) {
        g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(sf(0.1f32) + cast::<Ext, _>(g.base_interest_rate)));
        g.prime_rate = cast::<f32, _>(cast::<f32, _>(fix(cast::<Ext, _>(g.base_interest_rate))));
        if g.cur_quarter < 4 {
            g.prime_rate = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.cur_quarter) * sf(0.25f32) + cast::<Ext, _>(g.prime_rate)));
        }
    }
    g.fed_rate_shock_tick = 0;
    if rate_change != Ext::from_f64(0.0) {
        update_market_display(g, ui, rng, time_secs);
        let mut msg = g.news_msg.clone();
        show_quick_message(g, ui, &mut msg);
        g.news_msg = msg;
        post_news(g, ui, 0, 0);
    }
}
