//! Holding-array maintenance: `ClearAllHoldings` (0x49261c) and `PackHoldingArrays` (0x49117c).
//!
//! Layout of the holding arrays (second subscript = owning entity):
//! * `HoldStockId/HoldStockPct/HoldStockBasis(0 TO 15, 0 TO 1600)`: slot 0 of `HoldStockId` holds the
//!   number of used slots, slots 1..15 the held corp id, percentage and cost basis.
//! * `HoldBondId/HoldBondFace/BondHoldCost(0 TO 15, 0 TO 1600)`: same for bonds (entities 1..130 and the
//!   funds 1581..1583).
//! * The record lists `OptionRecs`, `FuturesRecs`, `SwapContracts`, `PhysCommodRec` (1..1000) and
//!   `AssetSaleOffers`, `PriceAlerts` (1..100) keep their count as a string in element 0.

// Statements keep the original BASIC shape (`x = x + y`, `a < 1 OR 5 < a`) on purpose.
#![allow(clippy::manual_range_contains, clippy::assign_op_pattern)]
use crate::pb::real::{Ext, cast};


use crate::pb::numeric::to_quad;
use crate::state::Globals;

/// `ClearAllHoldings(mode)` (0x49261c): for every entity 1..1600 whose `EntityStatus` is nonzero, zero all
/// stock-holding slots 0..15 (and bond slots for entities < 131) — except active entities (status 1)
/// when `mode = 2`, i.e. mode 2 only wipes entities in a non-active, non-empty state (status 2, …).
pub fn clear_all_holdings(g: &mut Globals, mode: i32) {
    // entityId = 1 … 1600 (do/while at 0x49264b)
    for entity_id in 1..=1600 {
        let st = g.entity_status[entity_id];
        if st == 0 {
            continue;
        }
        if st == 1 && mode == 2 {
            continue; // LAB_004927df
        }
        for slot in 0..=15 {
            g.hold_stock_id[(slot, entity_id)] = 0;
            g.hold_stock_pct[(slot, entity_id)] = 0;
            g.hold_stock_basis[(slot, entity_id)] = Ext::from_f64(0.0);
            if entity_id < 131 {
                g.hold_bond_id[(slot, entity_id)] = 0;
                g.hold_bond_face[(slot, entity_id)] = 0; // (longlong)ROUND(0)
                g.bond_hold_cost[(slot, entity_id)] = Ext::from_f64(0.0);
            }
        }
    }
}

/// News the original posts from inside [`pack_holding_arrays`] (via `RecaptureExcessLoss`).
#[derive(Debug, Clone, PartialEq)]
pub enum PackEvent {
    /// `RecaptureExcessLoss` (0x499b0b) recaptured an excess loss of `amount` (> 1) on `owner_id`'s holding
    /// of `corp_id`. The original sets `NewsMsg` to " [runtime resource wording] <sym> holding of <sym> falls
    /// below 80%." and to "<sym> [runtime resource wording] <cur><FORMAT$(x, "* ###,")> …"
    /// and calls `PostNews(ownerId, corpId)` after each.
    ExcessLossRecaptured { owner_id: i32, corp_id: i32, amount: Ext },
}

/// Posts the headlines `RecaptureExcessLoss` (0x499b0b) posts in the original for each recapture
/// reported by the holdings port.
pub fn post_pack_events(g: &mut Globals, ui: &mut dyn crate::ui::Ui, events: Vec<PackEvent>) {
    let __content = crate::content::current();
    use crate::logic::news::post_news;
    use crate::pb::pbstr::PbStr;
    fn trim(bytes: &[u8]) -> PbStr {
        crate::pb::string::rtrim(crate::pb::string::ltrim(bytes).as_bytes())
    }
    for ev in events {
        let PackEvent::ExcessLossRecaptured {
            owner_id,
            corp_id,
            amount,
        } = ev;
        g.news_msg = PbStr::from(__content.string(5698))
            + &g.ent_symbol[owner_id]
            + __content.string(5699)
            + &g.ent_symbol[corp_id]
            + __content.string(5700);
        post_news(g, ui, owner_id, corp_id);
        g.news_msg = g.ent_symbol[owner_id].clone()
            + __content.string(5701)
            + &trim(g.currency_symbol.as_bytes())
            + &trim(&crate::pb::numeric::format(amount, b"* ###,"))
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix;
        post_news(g, ui, owner_id, corp_id);
    }
}

/// Packs holdings and posts the original's excess-loss recapture headlines.
/// Packing does not read state changed by PostNews, so events can be delivered after the scan.
pub fn pack_holding_arrays_with_news(g: &mut Globals, ui: &mut dyn crate::ui::Ui) {
    let events = pack_holding_arrays(g);
    post_pack_events(g, ui, events);
}

/// `PackHoldingArrays` (0x49117c): compact the stock and bond holding slots of every active entity so the
/// used slots are 1..n (moving the *last* used slot into the *first* empty one until no gap is left) and
/// store n in slot 0; then do the same for the string record lists and store their counts in element 0.
///
/// Returns recapture events. Interactive callers use [`pack_holding_arrays_with_news`].
pub fn pack_holding_arrays(g: &mut Globals) -> Vec<PackEvent> {
    let mut events = Vec::new();

    // ---- stocks: FOR entityId = 1 TO 1600 (0x4911b4)
    for entity_id in 1..=1600 {
        if g.entity_status[entity_id] != 1 {
            continue;
        }
        let mut done = 0;
        g.hold_stock_id[(0, entity_id)] = 0;
        while done == 0 {
            let mut last_used = 0;
            let mut first_empty = 0;
            for slot in 1..=15 {
                if first_empty == 0 && g.hold_stock_id[(slot, entity_id)] == 0 {
                    first_empty = slot;
                }
                if g.hold_stock_id[(slot, entity_id)] != 0 {
                    last_used = slot;
                }
                if g.hold_stock_id[(slot, entity_id)] == 0 {
                    g.hold_stock_pct[(slot, entity_id)] = 0;
                    g.hold_stock_basis[(slot, entity_id)] = Ext::from_f64(0.0);
                }
            }
            if last_used == 0 {
                done = 1;
            } else if first_empty == 0 {
                g.hold_stock_id[(0, entity_id)] = 15;
                done = 1;
            } else if last_used < 1 || first_empty <= last_used {
                if first_empty < last_used {
                    let dest_slot = first_empty;
                    g.hold_stock_id[(dest_slot, entity_id)] = g.hold_stock_id[(last_used, entity_id)];
                    g.hold_stock_id[(last_used, entity_id)] = 0;
                    g.hold_stock_pct[(dest_slot, entity_id)] = g.hold_stock_pct[(last_used, entity_id)];
                    g.hold_stock_pct[(last_used, entity_id)] = 0;
                    g.hold_stock_basis[(dest_slot, entity_id)] = g.hold_stock_basis[(last_used, entity_id)];
                    g.hold_stock_basis[(last_used, entity_id)] = Ext::from_f64(0.0);
                    if g.hold_stock_basis[(dest_slot, entity_id)] < Ext::from_f64(0.0) {
                        let held_corp_id = cast::<i32, _>(g.hold_stock_id[(dest_slot, entity_id)]);
                        recapture_excess_loss(g, held_corp_id, entity_id, &mut events);
                    }
                } else {
                    // MSGBOX " [runtime resource wording] ated result" and loop again (done stays 0):
                    // unreachable, a slot cannot be both the first empty and the last used one.
                    unreachable!("PackHoldingArrays: first empty slot == last used slot");
                }
            } else {
                g.hold_stock_id[(0, entity_id)] = cast::<i16, _>(last_used);
                done = 1;
            }
        }
    }

    // ---- bonds: FOR entityId = 1 TO 1583, only < 131 or > 1580 (0x4915d1)
    for entity_id in 1..=1583 {
        if !((entity_id < 131 || 1580 < entity_id) && g.entity_status[entity_id] == 1) {
            continue;
        }
        let mut done = 0;
        g.hold_bond_id[(0, entity_id)] = 0;
        while done == 0 {
            let mut last_used = 0;
            let mut first_empty = 0;
            for slot in 1..=15 {
                if first_empty == 0 && g.hold_bond_id[(slot, entity_id)] == 0 {
                    first_empty = slot;
                }
                // Note: only *positive* ids count as used (negative ids are neither used nor empty).
                if 0 < g.hold_bond_id[(slot, entity_id)] {
                    last_used = slot;
                }
                if g.hold_bond_id[(slot, entity_id)] == 0 {
                    g.hold_bond_face[(slot, entity_id)] = 0;
                    g.bond_hold_cost[(slot, entity_id)] = Ext::from_f64(0.0);
                }
            }
            if last_used == 0 {
                done = 1;
            } else if first_empty == 0 {
                g.hold_bond_id[(0, entity_id)] = 15;
                done = 1;
            } else if last_used < 1 || first_empty <= last_used {
                if first_empty < last_used {
                    let dest_slot = first_empty;
                    g.hold_bond_id[(dest_slot, entity_id)] = g.hold_bond_id[(last_used, entity_id)];
                    g.hold_bond_id[(last_used, entity_id)] = 0;
                    g.hold_bond_face[(dest_slot, entity_id)] = g.hold_bond_face[(last_used, entity_id)];
                    g.hold_bond_face[(last_used, entity_id)] = 0;
                    g.bond_hold_cost[(dest_slot, entity_id)] = g.bond_hold_cost[(last_used, entity_id)];
                    g.bond_hold_cost[(last_used, entity_id)] = Ext::from_f64(0.0);
                } else {
                    // MSGBOX " [runtime resource wording] ated (bonds) result": unreachable (see above).
                    unreachable!("PackHoldingArrays: first empty bond slot == last used slot");
                }
            } else {
                g.hold_bond_id[(0, entity_id)] = last_used;
                done = 1;
            }
        }
    }

    // ---- string record lists (0x4919f6 …)
    pack_records(&mut g.option_recs, 1000);
    pack_records(&mut g.futures_recs, 1000);
    pack_records(&mut g.swap_contracts, 1000);
    pack_records(&mut g.phys_commod_rec, 1000);
    pack_records(&mut g.asset_sale_offers, 100);
    pack_records(&mut g.price_alerts, 100);

    events
}

/// One of the six identical string-list loops of PackHoldingArrays: elements 1..n, count string in 0.
fn pack_records(recs: &mut crate::pb::array::Arr1<crate::pb::pbstr::PbStr>, n: i32) {
    use crate::pb::pbstr::PbStr;
    let mut done = 0;
    while done != 1 {
        let mut last_used = 0;
        let mut first_empty = 0;
        for slot in 1..=n {
            if first_empty == 0 && recs[slot].is_empty() {
                first_empty = slot;
            }
            if !recs[slot].is_empty() {
                last_used = slot;
            }
        }
        if last_used == 0 {
            recs[0] = PbStr::from("0");
            done = 1;
        } else if first_empty == 0 {
            recs[0] = PbStr::from(n.to_string().as_str()); // "1000" / "100" literal
            done = 1;
        } else if last_used < 1 || first_empty <= last_used {
            if first_empty < last_used {
                let dest_slot = first_empty;
                recs[dest_slot] = recs[last_used].clone();
                recs[last_used] = PbStr::new();
            }
            // (no error message in these loops; first_empty == last_used is impossible)
        } else {
            // RTRIM$(LTRIM$(STR$(lastUsed)))
            recs[0] = PbStr::from(last_used.to_string().as_str());
            done = 1;
        }
    }
}

const MINUS_0_001: f64 = -0.001f32 as f64;

/// `RecaptureExcessLoss(corpId, holderId)` (0x499b0b): when a holder owns less than 80% of `corp_id` but
/// still carries a negative cost basis (an excess loss account), the basis is zeroed and its absolute value
/// added to the holder's `ExtraordinaryPL`.
///
/// Scans holder `holder_id` and `holder_id + 1` (capped at 1600) for holders >= 11; returns immediately for
/// human players 1..NumPlayers; for other ids < 11 it scans all corps 11..1600. After a recapture the
/// owner loop variable is set to 1600 to end the scan. **Original quirk:** the inner slot loop is not
/// left, so its remaining slots (bound = the first owner's count) are then checked on entity 1600's
/// holdings; reproduced here.
///
/// Note: called from PackHoldingArrays after `HoldStockId(0, holder)` was reset to 0, so the holder's own
/// slots are not scanned there (slot loop bound 0); only `holder_id + 1`'s stale count is used.
///
/// Returns events for [`post_pack_events`] without changing `NewsMsg`.
pub fn recapture_excess_loss(g: &mut Globals, corp_id: i32, holder_id: i32, events: &mut Vec<PackEvent>) {
    let mut first_id = 11;
    let mut last_id = 1600;
    if holder_id < 11 {
        if 0 < holder_id && holder_id <= g.num_players {
            return;
        }
    } else {
        first_id = holder_id;
        last_id = holder_id + 1;
        if 1600 < last_id {
            last_id = 1600;
        }
    }
    let mut owner_id = first_id;
    while owner_id <= last_id {
        if g.entity_status[owner_id] == 1 {
            let count = cast::<i32, _>(g.hold_stock_id[(0, owner_id)]); // FOR bound, evaluated once
            let mut slot = 1;
            while slot <= count {
                if cast::<i32, _>(g.hold_stock_id[(slot, owner_id)]) == corp_id {
                    // -0.001 is a SINGLE constant (fld dword [0x75e494])
                    if g.hold_stock_basis[(slot, owner_id)] < Ext::from_f64(0.0) && MINUS_0_001 < g.hold_stock_basis[(slot, owner_id)] {
                        g.hold_stock_basis[(slot, owner_id)] = Ext::from_f64(0.0);
                    }
                    if g.hold_stock_pct[(slot, owner_id)] < 80 && g.hold_stock_basis[(slot, owner_id)] <= MINUS_0_001 {
                        let x = g.hold_stock_basis[(slot, owner_id)].abs();
                        g.hold_stock_basis[(slot, owner_id)] = Ext::from_f64(0.0);
                        g.extraordinary_pl[owner_id] = to_quad(cast::<Ext, _>(g.extraordinary_pl[owner_id]) + x);
                        if Ext::from_f64(1.0) < x {
                            events.push(PackEvent::ExcessLossRecaptured { owner_id, corp_id, amount: x });
                        }
                        owner_id = 1600;
                    }
                }
                slot += 1;
            }
        }
        owner_id += 1;
    }
}

/// Port of FindHoldingSlot (0x498c61).
/// Slot 1..15 of `holder_id`'s stock holdings that holds `corp_id`, else the first empty slot, else 0.
pub fn find_holding_slot(g: &Globals, holder_id: i32, corp_id: i32) -> i32 {
    let mut slot = 0;
    let mut i = 1;
    while i < 16 {
        if g.hold_stock_id[(i, holder_id)] == 0 && slot == 0 {
            slot = i;
        }
        if cast::<i32, _>(g.hold_stock_id[(i, holder_id)]) == corp_id {
            slot = i;
            i = 15;
        }
        i += 1;
    }
    slot
}

/// Port of FindStockHolding (0x498d53).
/// Slot of `holder_id`'s holding of `corp_id` (negated for a short position), 0 if not held.
pub fn find_stock_holding(g: &Globals, holder_id: i32, corp_id: i32) -> i32 {
    let mut result = 0;
    let mut slot = find_holding_slot(g, holder_id, corp_id);
    if slot != 0 && cast::<i32, _>(g.hold_stock_id[(slot, holder_id)]) == corp_id {
        if g.hold_stock_pct[(slot, holder_id)] < 0 {
            slot = -slot;
        }
        result = slot;
    }
    result
}

/// Port of FindBondHoldingSlot (0x498e40).
/// Bond slot of `holder_id` holding `issuer_id`, else the *last* empty slot, else 0. Only for holders
/// <= 131 or >= 1582 (so fund 1581 always gets 0 and entity 131 is scanned; see BUGS.md).
pub fn find_bond_holding_slot(g: &Globals, holder_id: i32, issuer_id: i32) -> i32 {
    let mut result = 0;
    let mut slot = 0;
    if holder_id <= 131 || 1582 <= holder_id {
        let mut i = 1;
        while i < 16 {
            if g.hold_bond_id[(i, holder_id)] == 0 {
                slot = i; // no "slot = 0" test: the last empty slot wins
            }
            if issuer_id == g.hold_bond_id[(i, holder_id)] {
                slot = i;
                i = 15;
            }
            i += 1;
        }
        result = slot;
    }
    result
}

/// Port of GetLargestHoldingId (0x49100a).
/// The corp (> 10, long position) with the largest `MarketCap * HoldStockPct` among `entity_id`'s
/// holdings; 0 if none (ties keep the first).
pub fn get_largest_holding_id(g: &Globals, entity_id: i32) -> i32 {
    let mut best_id: i16 = 0;
    let mut best_value: Ext = Ext::from_f64(0.0);
    if 0 < entity_id {
        for slot in 1..=15 {
            if 10 < g.hold_stock_id[(slot, entity_id)] && 0 < g.hold_stock_pct[(slot, entity_id)] {
                let id = g.hold_stock_id[(slot, entity_id)];
                let value = g.market_cap[cast::<i32, _>(id)] * cast::<Ext, _>(g.hold_stock_pct[(slot, entity_id)]);
                if best_value < value {
                    best_value = value;
                    best_id = id;
                }
            }
        }
    }
    cast::<i32, _>(best_id)
}
