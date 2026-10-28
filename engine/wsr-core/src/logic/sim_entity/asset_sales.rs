//! Asset-sale offers and corporate expansion: `GenAssetSaleDescription` (0x4e1095), the AI's offers
//! of stock holdings / business assets (`AiOfferAssetsForSale` 0x66b218), the AI buyers
//! (`AiBuyOfferedBusinessAssets` 0x66c796, `AiBuyOfferedStock` 0x66d13a) with the offer-list helpers,
//! and new corporations / industry entries (`StartNewCorpOrExpand` 0x4f21e7).
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::prelude::*;
use crate::pb::string::{CaseMode, mcase};
use crate::pb::numeric::fix;

/// The DATA table read by `GenAssetSaleDescription` with `READ$ 1..20` (code segment 0x4e0fb4: a word
/// count of 20, then NUL-terminated strings): regional place adjectives.
#[allow(non_snake_case)]
pub fn ASSET_PLACE_DATA(__content: &crate::content::ContentPack) -> [&str; 20] { [
    __content.string(10508),
    __content.string(10509),
    "Midwest",
    "sunbelt",
    __content.string(10510),
    __content.string(10511),
    __content.string(10512),
    "Texas",
    __content.string(10513),
    __content.string(10514),
    __content.string(7156),
    __content.string(7069),
    __content.string(10515),
    "Korean",
    "Taiwan",
    __content.string(5834),
    __content.string(10516),
    "Dutch",
    __content.string(10517),
    __content.string(10518),
] }

/// Port of GenAssetSaleDescription (0x4e1095).
/// Builds a random description of business assets of `entity_id`'s industry (e.g. "French chemical
/// plant", "capital assets") into `AssetSaleDesc`; the place comes from the country names (1..52), the
/// country display names (53..104) or the regional adjectives of [`ASSET_PLACE_DATA`] (105..124).
/// Increments `RandSeed`. `AssetSaleDesc` is only cleared (not returned early) for industries outside
/// 3..69, which then get the generic "division"/"unit".
///
/// Quirk: the place rule for industries `3, 5, 6, …, 69` lists `41 - 48` (i.e. -7) instead of the range
/// 41 TO 48, so retail (41..48) keeps any place.
pub fn gen_asset_sale_description(g: &mut Globals, rng: &mut Rng, entity_id: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut place_names: Vec<PbStr> = vec![PbStr::new(); 126]; // DIM (1 TO 125)
    g.rand_seed += 1;
    for i in 1..=124 {
        if i < 53 {
            place_names[cast::<usize, _>(i)] = g.country_name[i].clone();
        } else if i < 105 {
            place_names[cast::<usize, _>(i)] = g.country_display_name[i - 52].clone();
        } else {
            place_names[cast::<usize, _>(i)] = PbStr::from(ASSET_PLACE_DATA(&__content)[cast::<usize, _>(i - 105)]);
        }
    }
    let industry = g.industry_num[entity_id];
    if g.industry_num[entity_id] < 3 || g.industry_num[entity_id] > 69 {
        g.asset_sale_desc = PbStr::new();
    }
    let home_country = g.country_idx[entity_id];
    let mut place_idx = rng.rnd_range(1, 104);
    if home_country == 0 || home_country == 6 {
        place_idx = rng.rnd_range(105, 124);
    } else if home_country == place_idx {
        place_idx += 1;
        if place_idx > 104 {
            place_idx -= 3;
        }
    }
    if home_country == 6 && place_idx == 113 {
        place_idx = to_long(cast::<Ext, _>(place_idx) - (int(rng.rnd() * Ext::from_f64(8.0)) + Ext::from_f64(1.0)));
    }
    let ind = cast::<i32, _>(industry);
    match ind {
        4
        | 7
        | 9
        | 10
        | 12
        | 18..=22
        | 26
        | 30
        | 32..=38
        | 40
        | 49..=51
        | 58
        | 59
        | 60
        | 61
        | 66 => {}
        // (41 - 48 = -7 in the original's list: never matches)
        3 | 5 | 6 | 11 | 13..=15 | 17 | 27..=29 | 31 | 39 | -7 | 55..=57 | 62..=65 | 69 => {
            if place_idx > 112 {
                place_idx -= 8;
            }
            if place_idx > 112 {
                place_idx -= 4;
            }
            if place_idx < 105 {
                place_idx = 0;
            }
        }
        8 | 16 | 23 | 25 | 52..=54 | 67 | 68 => place_idx = 0,
        _ => {}
    }
    let rs = |g: &Globals, n: i32| g.rand_seed % n;
    let mut asset_noun: PbStr;
    match ind {
        3 | 67 | 68 => {
            asset_noun = __content.string(10519).into();
            if rng.rnd() * Ext::from_f64(6.0) > Ext::from_f64(2.0) {
                asset_noun = __content.string(10520).into();
            }
            if rng.rnd() * Ext::from_f64(6.0) > Ext::from_f64(3.0) {
                asset_noun = __content.string(10521).into();
            }
            if rng.rnd() * Ext::from_f64(6.0) > Ext::from_f64(4.0) {
                asset_noun = __content.string(10522).into();
            }
            if rng.rnd() * Ext::from_f64(6.0) > Ext::from_f64(5.0) {
                asset_noun = __content.string(10523).into();
            }
        }
        4 | 21 => {
            asset_noun = __content.string(10524).into();
            if rng.rnd() * Ext::from_f64(4.0) > Ext::from_f64(3.0) {
                asset_noun = __content.string(10525).into();
            }
        }
        5 | 59 => {
            asset_noun = __content.string(10526).into();
            if rng.rnd() * Ext::from_f64(4.0) > Ext::from_f64(2.0) && industry == 5 {
                asset_noun = __content.string(10527).into();
                if rs(g, 4) == 1 {
                    asset_noun = __content.string(10528).into();
                }
                if rs(g, 4) == 2 {
                    asset_noun = __content.string(10529).into();
                }
            }
        }
        6 | 32 | 49 | 60 => {
            asset_noun = "plant".into();
            if rng.rnd() * Ext::from_f64(4.0) > Ext::from_f64(2.0) {
                asset_noun = __content.string(10530).into();
            }
            if industry == 49 {
                if rs(g, 3) == 1 {
                    asset_noun = __content.string(10531).into();
                }
                if rs(g, 3) == 2 {
                    asset_noun = __content.string(10532).into();
                }
            }
        }
        7 => {
            asset_noun = __content.string(10533).into();
            if rng.rnd() * Ext::from_f64(4.0) < Ext::from_f64(2.5) {
                asset_noun = __content.string(10534).into();
                if rs(g, 3) == 1 {
                    asset_noun = __content.string(6311).into();
                }
                if rs(g, 3) == 2 {
                    asset_noun = __content.string(10535).into();
                }
                if place_idx > 52 && place_idx < 105 {
                    place_idx -= 52;
                }
                if matches!(place_idx, 12 | 18 | 30 | 40 | 46 | 50) {
                    place_idx += 1;
                }
            }
        }
        8 => {
            asset_noun = __content.string(10536).into();
            if rs(g, 5) == 1 {
                asset_noun = __content.string(10537).into();
            }
            if rs(g, 5) == 2 {
                asset_noun = __content.string(10538).into();
            }
            if rs(g, 5) == 3 {
                asset_noun = __content.string(10539).into();
            }
            if rs(g, 5) == 4 {
                asset_noun = __content.string(10540).into();
            }
        }
        9 | 50 => {
            asset_noun = __content.string(10541).into();
            if place_idx > 52 && place_idx < 105 {
                place_idx -= 52;
            }
            if matches!(place_idx, 12 | 18 | 30 | 40 | 46 | 50) {
                place_idx -= 1;
            }
        }
        10 | 26 | 63 => asset_noun = __content.string(10542).into(),
        11 => {
            asset_noun = __content.string(10543).into();
            if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0) {
                asset_noun = __content.string(10544).into();
                place_idx = 0;
            }
        }
        12 => asset_noun = __content.string(10545).into(),
        13 | 30 | 39 | 64 => asset_noun = __content.string(10546).into(),
        14 => {
            asset_noun = __content.string(10547).into();
            if g.cur_year > g.start_year + 15 {
                if rs(g, 4) == 1 {
                    asset_noun = __content.string(10548).into();
                    place_idx = 0;
                }
                if rs(g, 4) == 2 {
                    asset_noun = __content.string(10549).into();
                    place_idx = 0;
                }
                if rs(g, 4) == 3 {
                    asset_noun = __content.string(10550).into();
                    place_idx = 0;
                }
            }
        }
        15 | 28 => {
            asset_noun = __content.string(10551).into();
            if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0) || industry == 28 {
                asset_noun = __content.string(10552).into();
            }
        }
        16 => {
            asset_noun = __content.string(10553).into();
            if rs(g, 5) == 1 {
                asset_noun = __content.string(10554).into();
            }
            if rs(g, 5) == 2 {
                asset_noun = __content.string(10555).into();
            }
            if rs(g, 5) == 3 {
                asset_noun = __content.string(10556).into();
            }
            if rs(g, 5) == 4 {
                asset_noun = __content.string(10557).into();
            }
        }
        17 => asset_noun = "brewery".into(),
        18 | 38 | 55 | 66 => asset_noun = "factory".into(),
        19 => asset_noun = __content.string(10558).into(),
        20 | 51 => {
            asset_noun = __content.string(10559).into();
            if rng.rnd() * Ext::from_f64(4.0) > Ext::from_f64(2.5) {
                asset_noun = __content.string(10560).into();
            }
        }
        22 | 40 => asset_noun = __content.string(10530).into(),
        23 => {
            asset_noun = __content.string(10561).into();
            if rng.rnd() * Ext::from_f64(4.0) > Ext::from_f64(3.0) {
                asset_noun = __content.string(10562).into();
            }
            if rng.rnd() * Ext::from_f64(4.0) > Ext::from_f64(3.5) {
                asset_noun = __content.string(10563).into();
            }
            if rng.rnd() * Ext::from_f64(4.0) > Ext::from_f64(1.5) {
                asset_noun = __content.string(10564).into();
            }
        }
        24 => {
            asset_noun = __content.string(10565).into();
            if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0) {
                asset_noun = __content.string(10566).into();
            }
            if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(2.0) {
                asset_noun = __content.string(10567).into();
            }
            if rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) && g.cur_year > 2018 {
                asset_noun = __content.string(10568).into();
                if rs(g, 3) == 1 {
                    asset_noun = __content.string(10569).into();
                }
            }
        }
        25 => {
            asset_noun = __content.string(10555).into();
            if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(2.0) {
                asset_noun = __content.string(10570).into();
            }
        }
        27 => {
            asset_noun = __content.string(10571).into();
            if rs(g, 3) == 1 {
                asset_noun = __content.string(10572).into();
            }
            if rs(g, 3) == 2 {
                asset_noun = __content.string(10573).into();
            }
        }
        31 => asset_noun = __content.string(10574).into(),
        33 => asset_noun = __content.string(10575).into(),
        34 => {
            asset_noun = __content.string(10576).into();
            if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(2.0) {
                asset_noun = __content.string(10577).into();
            }
            if rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) {
                asset_noun = __content.string(10578).into();
            }
        }
        35 => {
            asset_noun = __content.string(10546).into();
            if rs(g, 3) == 1 {
                asset_noun = __content.string(10579).into();
            }
            if rng.rnd() * Ext::from_f64(6.0) > Ext::from_f64(2.0) {
                asset_noun = __content.string(10580).into();
            }
            if rng.rnd() * Ext::from_f64(6.0) > Ext::from_f64(4.0) {
                asset_noun = __content.string(10581).into();
            }
        }
        36 | 37 => asset_noun = "offices".into(),
        41..=48 => {
            asset_noun = "stores".into();
            if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(2.0) {
                asset_noun = "outlets".into();
            }
        }
        52 => {
            asset_noun = __content.string(10582).into();
            if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                asset_noun = __content.string(10583).into();
            }
            if rs(g, 4) == 1 {
                asset_noun = __content.string(10584).into();
            }
            if rs(g, 4) == 2 {
                asset_noun = __content.string(10585).into();
            }
        }
        53 | 54 => {
            asset_noun = __content.string(10586).into();
            if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(2.0) {
                asset_noun = __content.string(10587).into();
            }
            if rs(g, 4) == 1 {
                asset_noun = __content.string(10588).into();
            }
            if rs(g, 4) == 2 {
                asset_noun = __content.string(10589).into();
            }
        }
        58 | 61 => {
            asset_noun = __content.string(10590).into();
            if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(2.0) {
                asset_noun = replace(asset_noun.as_bytes(), b"unit", __content.bytes(10530));
            }
        }
        56 => {
            asset_noun = __content.string(10591).into();
            if rng.rnd() * Ext::from_f64(4.0) > Ext::from_f64(2.0) && place_idx > 0 {
                asset_noun = __content.string(10592).into();
            }
            if rng.rnd() * Ext::from_f64(4.0) > Ext::from_f64(3.0) && place_idx > 0 {
                asset_noun = __content.string(10593).into();
            }
        }
        57 => {
            asset_noun = __content.string(10594).into();
            if place_idx > 0 {
                if rng.rnd() * Ext::from_f64(4.0) > Ext::from_f64(2.0) {
                    asset_noun = __content.string(10542).into();
                }
            }
            if rng.rnd() * Ext::from_f64(4.0) > Ext::from_f64(3.0) {
                asset_noun = "outlets".into();
            }
        }
        62 => asset_noun = "unit".into(),
        65 => asset_noun = __content.string(10595).into(),
        69 => {
            asset_noun = __content.string(10596).into();
            if rs(g, 6) == 1 {
                asset_noun = __content.string(10597).into();
            }
            if rs(g, 6) == 2 {
                asset_noun = __content.string(10598).into();
            }
            if rs(g, 6) == 3 {
                asset_noun = __content.string(10599).into();
            }
            if rs(g, 6) == 4 {
                asset_noun = __content.string(10531).into();
            }
            if rs(g, 6) == 5 {
                asset_noun = __content.string(10600).into();
            }
        }
        _ => {
            asset_noun = __content.string(10542).into();
            if rs(g, 3) > 1 {
                asset_noun = "unit".into();
            }
        }
    }
    let variant_roll = rng.rnd_range(1, 10);
    let mut place_adj = PbStr::new();
    if place_idx < 1 {
        if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
            place_adj = PbStr::new();
            if variant_roll > 5 {
                asset_noun = __content.string(3467).into();
            }
            if variant_roll > 7 {
                asset_noun = __content.string(8294).into();
            }
            if variant_roll > 9 {
                asset_noun = "assets".into();
            }
        }
    } else {
        place_adj = place_names[cast::<usize, _>(place_idx)].clone();
        if place_idx < 105 {
            place_adj = mcase(place_adj.as_bytes(), CaseMode::Ansi);
        }
        for (from, to) in [
            ("Germany", "German"),
            ("France", "French"),
            ("Spain", "Spanish"),
            ("Japan", __content.string(10518)),
            ("Belgium", "Belgian"),
            ("Canada", __content.string(10513)),
            ("Brazil", __content.string(10517)),
        ] {
            if place_adj == from {
                place_adj = to.into();
            }
        }
        if place_adj == __content.string(4564)
            || place_adj == "Austria"
            || place_adj == "Korea"
            || place_adj == "Russia"
            || place_adj == "India"
        {
            place_adj = place_adj + "n";
        }
        if place_adj == "Sweden" {
            place_adj = "Swedish".into();
        }
        if place_adj == __content.string(10601) {
            place_adj = "British".into();
            if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                place_adj = "U.K.".into();
            }
        }
        place_adj = place_adj + " ";
        if variant_roll > 7 {
            asset_noun = __content.string(10602).into();
        }
        if variant_roll > 9 {
            asset_noun = "unit".into();
        }
    }
    g.asset_sale_desc = place_adj + &asset_noun;
    if instr(1, g.asset_sale_desc.as_bytes(), b"  ") > 0 {
        g.asset_sale_desc = replace(g.asset_sale_desc.as_bytes(), b"  ", b" ");
    }
}

// ------------------------------------------------------------------------------------- the offer list

/// `RTRIM$(LTRIM$(STR$(x)))` for a LONG / QUAD / EXT value (15 digits).
fn str15(x: Ext) -> PbStr {
    trim(str_num(x, 15).as_bytes())
}

/// Port of AddRemoveAssetSaleOffer (0x665fe3).
/// `offer_idx < 1`: stores `offer_text` in the first empty slot of `AssetSaleOffers(1..100)` and
/// increments the count in element 0 (nothing if the list is full). Otherwise clears slot `offer_idx`,
/// moves the last counted record (slot = count, if non-empty and count > 1) into it and decrements the
/// count (not below 0).
pub fn add_remove_asset_sale_offer(g: &mut Globals, offer_text: &PbStr, offer_idx: i32) {
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let new_offer = offer_text.clone();
    if offer_idx < 1 {
        for slot in 1..=100 {
            if g.asset_sale_offers[slot].is_empty() {
                g.asset_sale_offers[slot] = new_offer;
                let n = to_long(val(g.asset_sale_offers[0].as_bytes()));
                g.asset_sale_offers[0] = str15(cast::<Ext, _>(n + 1));
                break;
            }
        }
    } else {
        g.asset_sale_offers[offer_idx] = PbStr::new();
        let mut last_idx = to_long(val(g.asset_sale_offers[0].as_bytes()));
        if !g.asset_sale_offers[last_idx].is_empty() && last_idx > 1 {
            g.asset_sale_offers[offer_idx] = g.asset_sale_offers[last_idx].clone();
            g.asset_sale_offers[last_idx] = PbStr::new();
        }
        if last_idx > 0 {
            last_idx -= 1;
        } else {
            last_idx = 0;
        }
        g.asset_sale_offers[0] = str15(cast::<Ext, _>(last_idx));
    }
}

/// Port of ReduceAssetSaleOffer (0x666318).
/// Reduces offer `offer_idx` after a partial sale: a stock offer (`pct_sold > 0`) by `pct_sold` percent
/// (capped by what the seller still holds; removed when nothing is left or all was sold), a business-
/// asset offer by `amount_sold` (capped by the seller's `BusinessAssets`; removed when (almost) all was
/// sold or the seller has no assets). `pct_sold <= 0` with `amount_sold <= 0` shows an error box.
pub fn reduce_asset_sale_offer(
    g: &mut Globals,
    ui: &mut dyn Ui,
    offer_idx: i32,
    pct_sold: i32,
    amount_sold: i64,
) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let offer_slot = offer_idx;
    let offer_text = g.asset_sale_offers[offer_slot].clone();
    let pct = pct_sold;
    let ot = offer_text.as_bytes();
    let seller_field = parse_any(ot, b"|", 1) + "|";
    let stock_field = parse_any(ot, b"|", 2) + "|";
    let flag_field = parse_any(ot, b"|", 4) + "|";
    let exp_qtr_field = parse_any(ot, b"|", 5) + "|";
    let exp_year_field = parse_any(ot, b"|", 6);
    let seller_id = to_long(val(seller_field.as_bytes()));
    let qty_field;
    if pct > 0 {
        let offered_pct = to_long(val(parse_any(ot, b"|", 3).as_bytes()));
        if pct == offered_pct {
            add_remove_asset_sale_offer(g, &PbStr::new(), offer_slot);
            return;
        }
        let stock_corp_id = to_long(val(stock_field.as_bytes()));
        let mut remaining_pct = offered_pct - pct;
        let hold_slot = crate::logic::holdings::find_stock_holding(g, seller_id, stock_corp_id);
        if (cast::<i32, _>(g.hold_stock_pct[(hold_slot, seller_id)])) < remaining_pct {
            remaining_pct = cast::<i32, _>(g.hold_stock_pct[(hold_slot, seller_id)]);
        }
        if remaining_pct < 0 {
            remaining_pct = 0;
        }
        if remaining_pct == 0 {
            add_remove_asset_sale_offer(g, &PbStr::new(), offer_slot);
            return;
        }
        qty_field = str15(cast::<Ext, _>(remaining_pct)) + "|";
    } else {
        if amount_sold <= 0 {
            let msg = PbStr::from(__content.string(10603))
                + &offer_text
                + "\r"
                + __content.string(10604)
                + str_num(cast::<Ext, _>(offer_slot), 15).as_str()
                + "  PCT ="
                + str_num(cast::<Ext, _>(pct), 15).as_str()
                + __content.string(10605)
                + str_num(cast::<Ext, _>(amount_sold), 15).as_str();
            ui.msgbox(&msg, 0, &PbStr::new());
            return;
        }
        let amount = cast::<Ext, _>(amount_sold);
        let offered_amt = val(parse_any(ot, b"|", 3).as_bytes());
        if (offered_amt - amount).abs() < sf(0.001f32) {
            add_remove_asset_sale_offer(g, &PbStr::new(), offer_slot);
            return;
        }
        let mut remaining_amt = offered_amt - amount;
        if g.business_assets[seller_id] < remaining_amt {
            remaining_amt = g.business_assets[seller_id];
        }
        if !(remaining_amt >= Ext::from_f64(0.0)) || !(g.business_assets[seller_id] > Ext::from_f64(0.0)) {
            add_remove_asset_sale_offer(g, &PbStr::new(), offer_slot);
            return;
        }
        qty_field = str15(remaining_amt) + "|";
    }
    g.asset_sale_offers[offer_slot] =
        seller_field + &stock_field + &qty_field + &flag_field + &exp_qtr_field + &exp_year_field;
}

/// Port of ExecuteBusinessAssetPurchase (0x66bdb4) with its GOSUB ExecAssetPurchase_BlendMargin
/// (0x66c5ad).
/// `buyer_id` (an active corp > 130, not a fund) buys `amount` [runtime resource wording] the seller of
/// business-asset offer `offer_idx` (an active operating corp in the buyer's industry, or any industry
/// for a shell, industry 70, which then enters the seller's industry). Price = `amount`, less a 5%
/// discount unless the buyer is `ActiveEntity` and the seller is the current player's or both have the
/// same controller. A buyer short of cash borrows the shortfall (added to its `BankDebt`, the cash
/// deficit taken from `Cash(LenderId)` — the global, not the buyer's bank). The buyer's margin becomes
/// the asset-weighted blend; the discount is booked as an extraordinary loss of the seller and reduces
/// the buyer's goodwill (excess as extraordinary gain). The offer is reduced, or removed when the seller
/// is left with less than 1 of assets (it then withdraws from the industry, with news).
pub fn execute_business_asset_purchase(
    g: &mut Globals,
    ui: &mut dyn Ui,
    buyer_id: i32,
    offer_idx: i32,
    amount: i64,
) {
    let __content = crate::content::current();
    let buyer = buyer_id;
    if buyer < 131 || g.entity_status[buyer] != 1 || g.industry_num[buyer] == 71 {
        return;
    }
    let offer_slot = offer_idx;
    if offer_slot < 1 || offer_slot > 100 {
        return;
    }
    let offer_text = g.asset_sale_offers[offer_slot].clone();
    let seller_id = to_long(val(parse_any(offer_text.as_bytes(), b"|", 1).as_bytes()));
    if seller_id < 131 || g.entity_status[seller_id] != 1 || g.industry_num[seller_id] > 69 {
        return;
    }
    let seller_ind = g.industry_num[seller_id];
    if g.industry_num[buyer] != 70 && seller_ind != g.industry_num[buyer] {
        return;
    }
    let purchase_amt = amount;
    let mut discount = cast::<Ext, _>(purchase_amt) * sf(0.05f32);
    if g.active_entity == buyer && g.cur_player == g.corp_controller[seller_id] {
        discount = Ext::from_f64(0.0);
    } else if g.corp_controller[seller_id] == g.corp_controller[buyer] {
        discount = Ext::from_f64(0.0);
    }
    g.cash[buyer] = g.cash[buyer] - cast::<Ext, _>(purchase_amt);
    g.cash[buyer] = discount + g.cash[buyer];
    if g.cash[buyer] < Ext::from_f64(0.0) {
        g.bank_debt[buyer] = g.cash[buyer].abs() + g.bank_debt[buyer];
        g.cash[g.lender_id] = g.cash[buyer] + g.cash[g.lender_id];
        g.cash[buyer] = Ext::from_f64(0.0);
    }
    // GOSUB ExecAssetPurchase_BlendMargin
    if g.industry_num[buyer] == 70 {
        if g.profit_margin[seller_id] > 5 {
            g.profit_margin[buyer] = g.profit_margin[seller_id] - 2;
        } else if g.profit_margin[seller_id] < -5 {
            g.profit_margin[buyer] = g.profit_margin[seller_id] + 2;
        } else {
            g.profit_margin[buyer] = g.profit_margin[seller_id];
        }
    } else {
        let buyer_wt = cast::<Ext, _>(g.profit_margin[buyer]) * g.business_assets[buyer];
        let seller_wt = cast::<Ext, _>(g.profit_margin[seller_id]) * cast::<Ext, _>(purchase_amt);
        let total = cast::<Ext, _>(purchase_amt) + g.business_assets[buyer];
        g.profit_margin[buyer] = cast::<i16, _>(to_quad((buyer_wt + seller_wt) / total));
    }
    g.business_assets[buyer] = cast::<Ext, _>(purchase_amt) + g.business_assets[buyer];
    if g.industry_num[buyer] == 70 {
        g.tax_credits[buyer] = Ext::from_f64(0.0);
        if g.tax_loss_carry[buyer] < Ext::from_f64(0.0) {
            g.tax_loss_carry[buyer] = Ext::from_f64(0.0);
        }
        g.industry_num[buyer] = g.industry_num[seller_id];
    }
    g.cash[seller_id] = cast::<Ext, _>(purchase_amt) + g.cash[seller_id];
    g.cash[seller_id] = g.cash[seller_id] - discount;
    g.business_assets[seller_id] = g.business_assets[seller_id] - cast::<Ext, _>(purchase_amt);
    g.extraordinary_pl[seller_id] = to_quad(cast::<Ext, _>(g.extraordinary_pl[seller_id]) - discount);
    if g.business_assets[seller_id] >= Ext::from_f64(1.0) {
        reduce_asset_sale_offer(g, ui, offer_slot, 0, purchase_amt);
    } else {
        g.cash[seller_id] = g.business_assets[seller_id] + g.cash[seller_id];
        g.business_assets[seller_id] = Ext::from_f64(0.0);
        g.news_msg = g.ent_symbol[seller_id].clone()
            + __content.string(10606)
            + &g.ind_name[cast::<i32, _>(seller_ind)]
            + __content.string(1157);
        post_news(g, ui, 0, 0);
        g.industry_num[seller_id] = 70;
        g.profit_margin[seller_id] = 0;
        add_remove_asset_sale_offer(g, &PbStr::new(), offer_slot);
    }
    let disc2 = discount;
    g.goodwill[buyer] = g.goodwill[buyer] - disc2;
    if g.goodwill[buyer] < Ext::from_f64(0.0) {
        g.extraordinary_pl[buyer] = to_quad(cast::<Ext, _>(g.extraordinary_pl[buyer]) - g.goodwill[buyer]);
        g.goodwill[buyer] = Ext::from_f64(0.0);
    }
}

/// Port of AiBuyOfferedBusinessAssets (0x66c796).
/// With probability 1/2, the corp `SimCoCount` (active, industry < 70, liquid funds of at least 60
/// (x ExchangeRate) — `LiquidFunds` from the caller's CalcEntityFinancials) buys business assets offered
/// for sale (`"seller|CA|amount|…"` records; needs at least 3 offers) in its own industry when the
/// industry's demand gap is at least RND*10 and its assets are below 30..45% of the industry demand: the
/// first acceptable offer (amount >= 1% of liquid funds and >= 10, reduced to the seller's whole assets;
/// offers of inactive or nearly asset-less sellers are deleted) is bought, up to 60% of liquid funds
/// ([`execute_business_asset_purchase`]), with news and a notice to the seller's player. Returns 1 if a
/// purchase was made, else 0.
pub fn ai_buy_offered_business_assets(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> i32 {
    let __content = crate::content::current();
    let mut result = 0;
    if rng.rnd() * Ext::from_f64(2.0) < Ext::from_f64(1.0) {
        return result;
    }
    let buyer_id = g.sim_co_count;
    if g.entity_status[buyer_id] != 1 {
        return result;
    }
    if !(val(g.asset_sale_offers[0].as_bytes()) >= Ext::from_f64(3.0)) {
        return result;
    }
    if !(g.liquid_funds >= g.exchange_rate * Ext::from_f64(60.0)) {
        return result;
    }
    let industry_id = g.industry_num[buyer_id];
    if industry_id > 69 {
        return result;
    }
    if !(rng.rnd() * Ext::from_f64(10.0) <= g.ind_demand_gap[cast::<i32, _>(industry_id)]) {
        return result;
    }
    let max_spend = to_quad(g.liquid_funds * sf(0.6f32));
    let mut min_amt = to_quad(g.liquid_funds * sf(0.01f32));
    if (cast::<Ext, _>(min_amt)) < g.exchange_rate * Ext::from_f64(10.0) {
        min_amt = to_quad(g.exchange_rate * Ext::from_f64(10.0));
    }
    let x = cast::<Ext, _>(g.ind_demand[cast::<i32, _>(industry_id)]) * (rng.rnd() * sf(0.15f32) + sf(0.3f32));
    if !(g.business_assets[buyer_id] <= x) {
        return result;
    }
    let offer_count = to_long(val(g.asset_sale_offers[0].as_bytes()));
    let mut offer_amt: i64 = 0;
    let mut chosen_seller = 0;
    let mut chosen_offer = 0;
    let mut offer_idx = 1;
    'offers: while offer_idx <= offer_count {
        let offer_text = g.asset_sale_offers[offer_idx].clone();
        let ot = offer_text.as_bytes();
        'next: {
            if g.asset_sale_offers[offer_idx].is_empty() || parse_any(ot, b"|", 2) != "CA" {
                break 'next;
            }
            offer_amt = to_quad(val(parse_any(ot, b"|", 3).as_bytes()));
            if (cast::<Ext, _>(offer_amt)) < cast::<Ext, _>(min_amt) || !(cast::<Ext, _>(offer_amt) >= g.exchange_rate * Ext::from_f64(10.0))
            {
                break 'next;
            }
            let seller_id = to_long(val(parse_any(ot, b"|", 1).as_bytes()));
            if g.entity_status[seller_id] != 1
                || !(g.business_assets[seller_id] >= g.exchange_rate * Ext::from_f64(25.0))
            {
                g.asset_sale_offers[offer_idx] = PbStr::new();
                break 'next;
            }
            if buyer_id == seller_id {
                break 'next;
            }
            if fix(g.business_assets[seller_id]) < cast::<Ext, _>(offer_amt) {
                let excess_amt = cast::<Ext, _>(offer_amt) - fix(g.business_assets[seller_id]);
                if excess_amt > sf(0.5f32) {
                    let reduce_amt = to_quad(excess_amt);
                    reduce_asset_sale_offer(g, ui, offer_idx, 0, reduce_amt);
                    offer_amt = to_quad(fix(g.business_assets[seller_id]));
                }
            }
            if (cast::<Ext, _>(offer_amt)) < cast::<Ext, _>(min_amt) {
                break 'next;
            }
            if g.industry_num[seller_id] == industry_id {
                chosen_seller = seller_id;
                chosen_offer = offer_idx;
                break 'offers;
            }
        }
        offer_idx += 1;
    }
    if chosen_seller == 0 {
        return result;
    }
    if cast::<Ext, _>(offer_amt) > cast::<Ext, _>(max_spend) {
        offer_amt = max_spend;
    }
    execute_business_asset_purchase(g, ui, buyer_id, chosen_offer, offer_amt);
    if g.business_assets[chosen_seller] > Ext::from_f64(10.0) {
        g.profit_margin[chosen_seller] += 2;
    }
    gen_asset_sale_description(g, rng, chosen_seller);
    g.news_msg = g.ent_symbol[buyer_id].clone()
        + __content.string(2062)
        + &g.asset_sale_desc
        + " from "
        + &g.ent_symbol[chosen_seller]
        + __content.string(10607);
    if g.news_msg.len() < 57 {
        g.news_msg = replace(g.news_msg.as_bytes(), b"deal", __content.bytes(10608));
    }
    let mut price = offer_amt;
    if g.corp_controller[chosen_seller] != g.corp_controller[buyer_id] {
        price = to_quad(cast::<Ext, _>(price) * sf(0.95f32));
    }
    if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) && cast::<Ext, _>(price) > Ext::from_f64(10.0) {
        g.news_msg = g.ent_symbol[chosen_seller].clone()
            + " sells "
            + &g.asset_sale_desc
            + " to "
            + &g.ent_symbol[buyer_id]
            + " for "
            + &trim(g.currency_symbol.as_bytes())
            + &trim(&format(cast::<Ext, _>(price), b"* ###,"))
            + " "
            + &g.million_prefix_lc
            + "il";
    }
    let mut seller_news = chosen_seller;
    if g.cur_player == g.corp_controller[seller_news] {
        seller_news = 0;
    }
    post_news(g, ui, buyer_id, seller_news);
    if g.cur_player == g.corp_controller[chosen_seller] {
        g.turn_action_count += 1;
        // (UI) DIALOG SHOW STATE hDlgMain, 0
        let mut msg = PbStr::from(__content.string(10609))
            + "\r\n"
            + "\r\n"
            + __content.string(10610)
            + &g.ent_name[chosen_seller]
            + ", "
            + __content.string(10611)
            + &g.ent_name[buyer_id]
            + " ("
            + &g.ent_symbol[buyer_id]
            + ").";
        crate::logic::report::show_quick_message(g, ui, &mut msg);
        // (UI) DIALOG SHOW STATE hDlgMain, 5
    }
    result = 1;
    result
}

/// Port of CalcGroupStakePct (0x4a25e1).
/// Percentage of `abs(signed_stock_id)` held by `holder_id` itself plus (for a positive id) by every
/// active corp `holder_id` controls; a positive id ignores negative (short) stakes, a negative id
/// positive ones. Also sums the option contracts on that stock held by entities controlled by
/// `holder_id` (positive id: long calls / written puts; negative: written calls / long puts) into
/// `TmpExt1` (negated for a negative id; 0 if there are no option records).
pub fn calc_group_stake_pct(g: &mut Globals, holder_id: i32, signed_stock_id: i32) -> i32 {
    use crate::logic::holdings::find_stock_holding;
    g.tmp_ext1 = Ext::from_f64(0.0);
    let holder = holder_id;
    let stock_id = to_long((cast::<Ext, _>(signed_stock_id)).abs());
    let mut slot = to_long((cast::<Ext, _>(find_stock_holding(g, holder, stock_id))).abs());
    let mut stake_pct = 0;
    if cast::<i32, _>(g.hold_stock_id[(slot, holder)]) == stock_id && g.hold_stock_pct[(slot, holder)] != 0 {
        stake_pct = cast::<i32, _>(g.hold_stock_pct[(slot, holder)]);
    }
    if signed_stock_id > 0 && stake_pct < 0 {
        stake_pct = 0;
    }
    if signed_stock_id < 0 && stake_pct > 0 {
        stake_pct = 0;
    }
    if signed_stock_id > 0 {
        for entity_id in 11..=1600 {
            if g.entity_status[entity_id] == 1 && holder == g.corp_controller[entity_id] {
                slot = find_stock_holding(g, entity_id, stock_id);
                if slot >= 1 && cast::<i32, _>(g.hold_stock_id[(slot, entity_id)]) == stock_id {
                    stake_pct += cast::<i32, _>(g.hold_stock_pct[(slot, entity_id)]);
                }
            }
        }
    }
    let result = stake_pct;
    let mut option_count = to_long(val(g.option_recs[0].as_bytes()));
    if option_count == 0 {
        return result;
    }
    option_count += 50;
    if option_count > 999 {
        option_count = 999;
    }
    let mut option_pct = 0;
    for opt_idx in 1..=option_count {
        let rec = g.option_recs[opt_idx].clone();
        let r = rec.as_bytes();
        if val(parse_any(r, b"|", 1).as_bytes()) != cast::<Ext, _>(stock_id) {
            continue;
        }
        let entity_id = to_long(val(parse_any(r, b"|", 3).as_bytes()));
        if holder != g.corp_controller[entity_id] {
            continue;
        }
        let opt_qty = to_long(val(parse_any(r, b"|", 4).as_bytes()));
        let kind = |r: &[u8]| val(parse_any(r, b"|", 2).as_bytes());
        if signed_stock_id > 0 {
            if kind(r) == Ext::from_f64(1.0) && opt_qty < 0 {
                continue;
            }
            if kind(r) == Ext::from_f64(2.0) && opt_qty > 0 {
                continue;
            }
        } else {
            if kind(r) == Ext::from_f64(1.0) && opt_qty > 0 {
                continue;
            }
            if kind(r) == Ext::from_f64(2.0) && opt_qty < 0 {
                continue;
            }
        }
        option_pct += to_long((cast::<Ext, _>(opt_qty)).abs());
    }
    if signed_stock_id < 0 {
        option_pct = option_pct.wrapping_mul(-1);
    }
    g.tmp_ext1 = cast::<Ext, _>(option_pct);
    result
}

/// Port of NotifyDiscountOffer (0x66e819).
/// Tells the (human) current player about discounted offers. If the current player is `seller_id`:
/// for a stock offer (`offer_type = 1`) shows another seller's competing offer of the same stock
/// (returns that seller); for a business-asset purchase asks whether to look at a cheaper offer of
/// assets in `industry_id` (returns 1 on YES; NO remembers the industry in `AssetWarnDeclinedInd`).
/// Otherwise, for a stock offer by `seller_id` once per quarter (`StockAdQuarter`): if the current
/// player's group owns `stock_corp_id` (and does not control the seller), shows an advertisement
/// (unless `SuppressPopups`) and posts news. Returns 0 in the other cases.
pub fn notify_discount_offer(
    g: &mut Globals,
    ui: &mut dyn Ui,
    seller_id: i32,
    offer_type: i32,
    stock_corp_id: i32,
    industry_id: i16,
) -> i32 {
    let __content = crate::content::current();
    use crate::logic::report::show_quick_message;
    use crate::pb::string::ucase;
    let mut result = 0;
    if g.is_computer_player[g.cur_player] == 1 {
        return result;
    }
    if g.cur_player == seller_id {
        if offer_type == 1 {
            if val(g.asset_sale_offers[0].as_bytes()) == Ext::from_f64(0.0) {
                return result;
            }
            let n = to_long(val(g.asset_sale_offers[0].as_bytes()));
            for offer_idx in 1..=n {
                if g.asset_sale_offers[offer_idx].is_empty() {
                    continue;
                }
                let offer_text = g.asset_sale_offers[offer_idx].clone();
                let ot = offer_text.as_bytes();
                if val(parse_any(ot, b"|", 2).as_bytes()) != cast::<Ext, _>(stock_corp_id) {
                    continue;
                }
                let offeror_id = to_long(val(parse_any(ot, b"|", 1).as_bytes()));
                if g.cur_player == offeror_id || g.cur_player == g.corp_controller[offeror_id] {
                    continue;
                }
                let mut msg = PbStr::from(__content.string(10612))
                    + &ucase(g.ent_name[offeror_id].as_bytes(), CaseMode::Ansi)
                    + __content.string(10613)
                    + &g.ent_symbol[stock_corp_id]
                    + __content.string(10614)
                    + &g.ent_name[offeror_id]
                    + " "
                    + __content.string(10615)
                    + "\r\n"
                    + "\r\n"
                    + __content.string(10616)
                    + __content.string(10617);
                // (UI) DIALOG SHOW STATE hDlgMain, 0
                show_quick_message(g, ui, &mut msg);
                // (UI) DIALOG SHOW STATE hDlgMain, 5
                result = offeror_id;
                break;
            }
        } else {
            if val(g.asset_sale_offers[0].as_bytes()) == Ext::from_f64(0.0) {
                return result;
            }
            if industry_id == g.asset_warn_declined_ind {
                return result;
            }
            let n = to_long(val(g.asset_sale_offers[0].as_bytes()));
            for offer_idx in 1..=n {
                if g.asset_sale_offers[offer_idx].is_empty() {
                    continue;
                }
                let offer_text = g.asset_sale_offers[offer_idx].clone();
                let ot = offer_text.as_bytes();
                if trim(parse_any(ot, b"|", 2).as_bytes()) != "CA" {
                    continue;
                }
                let offeror_id = to_long(val(parse_any(ot, b"|", 1).as_bytes()));
                if offeror_id < 131
                    || g.industry_num[offeror_id] != industry_id
                    || g.cur_player == g.corp_controller[offeror_id]
                {
                    continue;
                }
                let msg =
                    PbStr::from(__content.string(10618))
                        + &g.ind_name[cast::<i32, _>(industry_id)]
                        + __content.string(10619)
                        + &g.ent_name[offeror_id]
                        + __content.string(10620)
                        + __content.string(10621)
                        + "\r"
                        + "\r"
                        + __content.string(10622)
                        + __content.string(10623);
                let title = PbStr::from(__content.string(10624)) + &g.ent_symbol[offeror_id];
                let answer = ui.msgbox(&msg, 0x2024, &title);
                if answer == 6 {
                    result = 1;
                } else {
                    g.asset_warn_declined_ind = industry_id;
                }
                break;
            }
        }
    } else if offer_type == 1 && g.cur_quarter != g.stock_ad_quarter {
        let corp_id = stock_corp_id;
        let group_pct = calc_group_stake_pct(g, g.cur_player, corp_id);
        if group_pct == 0 {
            return result;
        }
        let mut offer_text = PbStr::from(__content.string(10625));
        if crate::logic::holdings::find_stock_holding(g, g.cur_player, corp_id) > 0 {
            offer_text = __content.string(10626).into();
        }
        if g.cur_player == g.corp_controller[corp_id] {
            offer_text = __content.string(10627).into();
        }
        let offeror_id = seller_id;
        if g.cur_player == g.corp_controller[offeror_id] {
            return result;
        }
        let mut msg = PbStr::from(__content.string(10628))
            + "\r\n"
            + "\r\n"
            + __content.string(10629)
            + &g.ent_name[corp_id]
            + " ("
            + &g.ent_symbol[corp_id]
            + __content.string(10630)
            + &offer_text
            + __content.string(10631)
            + &g.ent_name[offeror_id]
            + "...."
            + "\r\n"
            + "\r\n"
            + __content.string(10632)
            + &g.ent_symbol[corp_id]
            + __content.string(10633)
            + __content.string(10634)
            + __content.string(10635);
        if g.suppress_popups == 0 {
            // (UI) DIALOG SHOW STATE hDlgMain, 0
            show_quick_message(g, ui, &mut msg);
            // (UI) DIALOG SHOW STATE hDlgMain, 5
            g.stock_ad_quarter = g.cur_quarter;
        }
        g.news_msg = g.ent_symbol[offeror_id].clone()
            + __content.string(5247)
            + &g.ent_symbol[corp_id]
            + __content.string(10636)
            + &g.cur_player_name
            + __content.string(10637);
        post_news(g, ui, 0, 0);
    }
    result
}

/// Port of AiOfferAssetsForSale (0x66b218) with its GOSUB AiOfferAssets_CheckExisting (0x66bc27).
/// The corp `SimCoCount` (not managed by a player; at most 50 offers listed) may list part of a stock
/// holding for sale (mostly when its credit rating is weak, or when the stock is rated a sell — fewer
/// holdings and a player's autopilot corps are restricted; autopilot corps ask permission), with an
/// advertisement to the current player ([`notify_discount_offer`]), or — for operating corps with low
/// margins — 10..30% [runtime resource wording] `"corp|CA|amount"`). The offer expires one or two quarters
/// later (`"…|S|quarter|year"`, [`add_remove_asset_sale_offer`]). Existing offers of the same stock /
/// assets block a new one. `time_secs`: seconds of `TIME$` (for [`ask_autopilot_permission`]).
pub fn ai_offer_assets_for_sale(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    if val(g.asset_sale_offers[0].as_bytes()) > Ext::from_f64(50.0) {
        return;
    }
    let corp_id = g.sim_co_count;
    let mut industry_id = g.industry_num[corp_id];
    let mut offer_stock = 0;
    let mut existing_flags = 0;
    let mut stock_corp_id = 0;
    let mut offer_text = PbStr::new();
    if g.hold_stock_id[(0, corp_id)] > 0 && rng.rnd() * Ext::from_f64(5.0) > Ext::from_f64(1.0) {
        offer_stock = 1;
        if corp_id > 85 && g.hold_stock_id[(0, corp_id)] < 3 && rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(2.0) {
            offer_stock = 0;
        }
    }
    if crate::logic::finance::get_managing_player(g, corp_id) > 0 {
        return;
    }
    if crate::logic::finance::get_controller_type(g, corp_id) == 1
        && g.cur_player != g.corp_controller[corp_id]
    {
        offer_stock = 0;
    }
    // GOSUB AiOfferAssets_CheckExisting
    let check_existing = |g: &Globals, stock_corp_id: i32, existing_flags: &mut i32| {
        *existing_flags = 0;
        let offer_count = to_long(val(g.asset_sale_offers[0].as_bytes()));
        for offer_idx in 1..=offer_count {
            if g.asset_sale_offers[offer_idx].is_empty() {
                continue;
            }
            let rec = g.asset_sale_offers[offer_idx].as_bytes();
            if val(parse_any(rec, b"|", 1).as_bytes()) == cast::<Ext, _>(corp_id) {
                if val(parse_any(rec, b"|", 2).as_bytes()) == cast::<Ext, _>(stock_corp_id)
                    && *existing_flags < 2
                {
                    *existing_flags += 2;
                }
                if parse_any(rec, b"|", 2) == "CA" {
                    *existing_flags += 1;
                }
                if *existing_flags == 3 {
                    return;
                }
            }
        }
    };
    if offer_stock > 0 {
        let hold_slot = cast::<i32, _>(cast::<i16, _>(rng.rnd_range(1, cast::<i32, _>(g.hold_stock_id[(0, corp_id)]))));
        let pct_base = cast::<i32, _>(g.hold_stock_pct[(hold_slot, corp_id)]);
        let held_pct = pct_base;
        stock_corp_id = cast::<i32, _>(g.hold_stock_id[(hold_slot, corp_id)]);
        if stock_corp_id == 0 || pct_base == 0 {
            return;
        }
        if g.industry_num[stock_corp_id] == 71 {
            return;
        }
        if g.hold_stock_pct[(hold_slot, corp_id)] >= 80
            && g.earnings_ytd[stock_corp_id] >= Ext::from_f64(0.0)
            && g.tax_loss_carry[corp_id] < -Ext::from_f64(200.0)
        {
            return;
        }
        existing_flags = 0;
        check_existing(g, stock_corp_id, &mut existing_flags);
        if existing_flags > 1 {
            return;
        }
        let rating = crate::logic::valuation::calc_analyst_rating(g, ui, rng, stock_corp_id);
        let mut pct_to_offer = 0;
        if industry_id < 3 || g.credit_rating_num[corp_id] < 5 {
            if g.credit_rating_num[corp_id] < 5 {
                if rating == 1 && rng.rnd() * Ext::from_f64(10.0) > Ext::from_f64(1.0) {
                    return;
                }
                let p = cast::<Ext, _>(pct_base);
                pct_to_offer = to_long(p * (rng.rnd() * sf(0.3f32)) + p * sf(0.2f32));
                if pct_base < 40 {
                    pct_to_offer = pct_base;
                }
                if pct_base >= 40 && pct_to_offer < 20 {
                    pct_to_offer = 20;
                }
            } else if g.credit_rating_num[corp_id] < 7 {
                let rand_pct = rng.rnd_range(1, 40) + 10;
                if rating == 1 && rng.rnd() * Ext::from_f64(5.0) > Ext::from_f64(1.0) {
                    return;
                }
                if rating == 2 && rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0) {
                    return;
                }
                pct_to_offer = to_long(cast::<Ext, _>(pct_base) * (sf(0.01f32) * cast::<Ext, _>(rand_pct)));
                if pct_base < 40 && rating > 1 {
                    pct_to_offer = to_long(cast::<Ext, _>(pct_base) * sf(0.6f32));
                }
                if pct_base < 30 {
                    pct_to_offer = pct_base;
                }
                if pct_base >= 30 && pct_to_offer < 20 {
                    pct_to_offer = 20;
                }
            }
        } else if industry_id != 70 && rating > 3 {
            if pct_base > 80 {
                pct_to_offer = pct_base - 80;
            } else if pct_base > 20 && pct_base < 75 {
                pct_to_offer = to_long((cast::<Ext, _>(pct_base) - Ext::from_f64(20.0)) * sf(0.2f32) + Ext::from_f64(20.0));
            } else if pct_base < 21 {
                pct_to_offer = pct_base;
            }
        }
        let pct_short = cast::<i16, _>(pct_to_offer);
        if pct_to_offer > 0 {
            if crate::logic::finance::get_controller_type(g, corp_id) == 1
                && ask_autopilot_permission(
                    g,
                    ui,
                    rng,
                    corp_id,
                    stock_corp_id,
                    pct_short,
                    held_pct,
                    2,
                    time_secs,
                ) == 0
            {
                return;
            }
            offer_text = str15(cast::<Ext, _>(corp_id))
                + "|"
                + &str15(cast::<Ext, _>(stock_corp_id))
                + "|"
                + &str15(cast::<Ext, _>(pct_to_offer));
            notify_discount_offer(g, ui, corp_id, 1, stock_corp_id, 0);
        }
    } else if industry_id > 2 && industry_id < 70 && existing_flags != 1 {
        if rng.rnd() * Ext::from_f64(5.0) > cast::<Ext, _>(g.profit_margin[corp_id]) {
            if val(g.asset_sale_offers[0].as_bytes()) > rng.rnd() * Ext::from_f64(30.0) {
                return;
            }
            check_existing(g, stock_corp_id, &mut existing_flags);
            if existing_flags == 1 || existing_flags == 3 {
                return;
            }
            industry_id = g.industry_num[corp_id];
            if g.ind_demand_gap[cast::<i32, _>(industry_id)] < -Ext::from_f64(2.0)
                && val(g.asset_sale_offers[0].as_bytes()) > Ext::from_f64(10.0)
            {
                return;
            }
            let mut pct_base = 0;
            if g.credit_rating_num[corp_id] > 4 && rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                pct_base = to_long(rng.rnd() * Ext::from_f64(10.0) + Ext::from_f64(10.0));
            } else if g.credit_rating_num[corp_id] < 5 {
                pct_base = to_long(rng.rnd() * Ext::from_f64(10.0) + Ext::from_f64(20.0));
            }
            if pct_base > 0 {
                let asset_amt = to_quad(cast::<Ext, _>(pct_base) * sf(0.01f32) * g.business_assets[corp_id]);
                if (cast::<Ext, _>(asset_amt)) < Ext::from_f64(10.0) {
                    return;
                }
                offer_text = str15(cast::<Ext, _>(corp_id)) + "|CA|" + &str15(cast::<Ext, _>(asset_amt));
            }
        }
    }
    if !offer_text.is_empty() {
        let mut exp_year = g.cur_year;
        let mut exp_qtr = 1 + g.cur_quarter;
        if g.sim_co_count > 500 || g.rand_seed % 2 == 1 {
            exp_qtr += 1;
        }
        if exp_qtr > 4 {
            exp_qtr -= 4;
            exp_year += 1;
        }
        offer_text = offer_text + "|S|" + &str15(cast::<Ext, _>(exp_qtr)) + "|" + &str15(cast::<Ext, _>(exp_year));
        add_remove_asset_sale_offer(g, &offer_text, 0);
    }
}

/// Port of AiBuyOfferedStock (0x66d13a).
/// With probability 1/3 the corp `SimCoCount` (active; not a player's corp unless the current player's;
/// funds only from 1586) buys stock offered for sale with up to 75% of `LiquidFunds` (from the caller's
/// CalcEntityFinancials): first an offer of a stock it already holds (market cap below the budget),
/// else another stock (cap below 1/20 of the budget; operating corps prefer their industry, funds stocks
/// rated as buys that fit [`pick_etf_holding_stock`](crate::logic::newgame_helpers::pick_etf_holding_stock);
/// players' offers are capped by a random budget). It buys up to the offered percentage at the market cap
/// (5% discount unless the seller is the current player's), borrowing a cash shortfall, books the
/// seller's gain/loss, posts news, reduces the offer ([`reduce_asset_sale_offer`]) and notifies the
/// seller's player (the current player's autopilot companies ask first, [`ask_autopilot_permission`]).
/// `time_secs`: seconds of `TIME$` (for CalcStockPrice / SelectTargetCorp).
pub fn ai_buy_offered_stock(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    let __content = crate::content::current();
    use crate::logic::finance::get_controller_type;
    use crate::logic::holdings::{find_holding_slot, find_stock_holding};
    use crate::logic::valuation::{calc_analyst_rating, calc_stock_price};
    let buyer_id = g.sim_co_count;
    if get_controller_type(g, buyer_id) == 1 && g.cur_player != g.corp_controller[buyer_id] {
        return;
    }
    if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0) {
        return;
    }
    if g.entity_status[buyer_id] != 1 || val(g.asset_sale_offers[0].as_bytes()) == Ext::from_f64(0.0) {
        return;
    }
    let buyer_industry = g.industry_num[buyer_id];
    if buyer_industry == 71 && buyer_id < 1586 {
        return;
    }
    let mut overweight_ind = 0;
    if buyer_id > 130 && buyer_industry < 70 {
        if g.business_assets[buyer_id] > cast::<Ext, _>(g.ind_demand[cast::<i32, _>(buyer_industry)]) * sf(0.4f32) {
            overweight_ind = 1;
        }
    }
    let mut budget = g.liquid_funds * sf(0.75f32);
    let mut seller_budget_cap: i64 = 0;
    let mut target_corp_id = 0;
    let mut chosen_offer = 0;
    let mut seller_id;
    let mut market_cap: Ext;
    let mut etf_room_pct = 0;
    let mut hold_idx = 1;
    // 1) offers of stocks the buyer already holds (slot by slot)
    'slots: while g.hold_stock_id[(0, buyer_id)] != 0 {
        let offer_count = to_long(val(g.asset_sale_offers[0].as_bytes()));
        for offer_idx in 1..=offer_count {
            let offer_text = g.asset_sale_offers[offer_idx].clone();
            let ot = offer_text.as_bytes();
            if offer_text.is_empty() || parse_any(ot, b"|", 2) == "CA" {
                continue;
            }
            if val(parse_any(ot, b"|", 1).as_bytes()) == cast::<Ext, _>(buyer_id) {
                continue;
            }
            if val(parse_any(ot, b"|", 2).as_bytes()) == cast::<Ext, _>(buyer_id) {
                continue;
            }
            if val(parse_any(ot, b"|", 2).as_bytes())
                != cast::<Ext, _>(g.hold_stock_id[(hold_idx, buyer_id)])
            {
                continue;
            }
            let offer_corp_id = to_long(val(parse_any(ot, b"|", 2).as_bytes()));
            if g.auto_pilot_rejected_target == offer_corp_id {
                continue;
            }
            if overweight_ind == 1
                && buyer_industry == g.industry_num[offer_corp_id]
                && g.corp_controller[buyer_id] != g.corp_controller[offer_corp_id]
            {
                continue;
            }
            market_cap = g.market_cap[offer_corp_id];
            if !(budget > market_cap) {
                continue;
            }
            seller_id = to_long(val(parse_any(ot, b"|", 1).as_bytes()));
            if get_controller_type(g, seller_id) == 1 {
                seller_budget_cap = to_quad(budget);
            }
            if budget >= market_cap {
                target_corp_id = offer_corp_id;
                chosen_offer = offer_idx;
                break;
            }
        }
        if target_corp_id > 0 {
            break 'slots;
        }
        hold_idx += 1;
        if hold_idx > 15 {
            break 'slots;
        }
    }
    if g.hold_stock_id[(0, buyer_id)] == 15 && target_corp_id == 0 {
        return;
    }
    if buyer_industry == 71 && target_corp_id > 10 {
        if calc_analyst_rating(g, ui, rng, target_corp_id) > 2 {
            return;
        }
        let buy_slot = cast::<i16, _>(find_holding_slot(g, buyer_id, target_corp_id));
        if buy_slot != 0 {
            etf_room_pct = 10 - cast::<i32, _>(g.hold_stock_pct[(cast::<i32, _>(buy_slot), buyer_id)]);
            if etf_room_pct < 1 {
                return;
            }
        }
    }
    let is_operating = if buyer_industry > 2 && buyer_industry < 70 {
        1
    } else {
        0
    };
    // 2) offers of other stocks
    if target_corp_id == 0 {
        let offer_count = to_long(val(g.asset_sale_offers[0].as_bytes()));
        for offer_idx in 1..=offer_count {
            let offer_text = g.asset_sale_offers[offer_idx].clone();
            let ot = offer_text.as_bytes();
            if offer_text.is_empty() || parse_any(ot, b"|", 2) == "CA" {
                continue;
            }
            if val(parse_any(ot, b"|", 1).as_bytes()) == cast::<Ext, _>(buyer_id) {
                continue;
            }
            let offer_corp_id = to_long(val(parse_any(ot, b"|", 2).as_bytes()));
            if buyer_id == offer_corp_id || g.auto_pilot_rejected_target == offer_corp_id {
                continue;
            }
            if is_operating == 1
                && buyer_industry != g.industry_num[offer_corp_id]
                && overweight_ind == 0
                && rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0)
            {
                continue;
            }
            if overweight_ind == 1
                && buyer_industry == g.industry_num[offer_corp_id]
                && g.corp_controller[buyer_id] != g.corp_controller[offer_corp_id]
            {
                continue;
            }
            if buyer_industry == 71 {
                if crate::logic::newgame_helpers::pick_etf_holding_stock(
                    g,
                    ui,
                    rng,
                    buyer_id,
                    offer_corp_id,
                ) == 0
                {
                    continue;
                }
                if g.industry_num[offer_corp_id] == 71 {
                    continue;
                }
                if calc_analyst_rating(g, ui, rng, offer_corp_id) > 2 {
                    continue;
                }
            } else if val(parse_any(ot, b"|", 3).as_bytes()) < Ext::from_f64(20.0) && rng.rnd() * Ext::from_f64(5.0) > Ext::from_f64(1.0) {
                continue;
            }
            market_cap = g.market_cap[offer_corp_id];
            if !(budget > market_cap * Ext::from_f64(20.0)) {
                continue;
            }
            seller_id = to_long(val(parse_any(ot, b"|", 1).as_bytes()));
            if get_controller_type(g, seller_id) == 1 {
                seller_budget_cap =
                    to_quad((rng.rnd() * Ext::from_f64(4000.0) + Ext::from_f64(1000.0)) * Ext::from_f64(100.0) * g.exchange_rate);
                let pct_offered = to_long(val(parse_any(ot, b"|", 3).as_bytes()));
                let mut pct_arg = 20;
                let fifth = cast::<Ext, _>(pct_offered) / Ext::from_f64(5.0);
                if fifth >= Ext::from_f64(1.0) && fifth < Ext::from_f64(20.0) {
                    pct_arg = to_long(fifth);
                }
                if !(cast::<Ext, _>(seller_budget_cap) >= market_cap * cast::<Ext, _>(pct_arg)) {
                    continue;
                }
            }
            target_corp_id = offer_corp_id;
            chosen_offer = offer_idx;
            break;
        }
    }
    if target_corp_id == 0 || g.auto_pilot_rejected_target == target_corp_id {
        return;
    }
    // 3) the purchase
    let offer_text = g.asset_sale_offers[chosen_offer].clone();
    let ot = offer_text.as_bytes();
    let mut pct_offered = to_long(val(parse_any(ot, b"|", 3).as_bytes()));
    seller_id = to_long(val(parse_any(ot, b"|", 1).as_bytes()));
    if get_controller_type(g, seller_id) == 1
        && g.corp_controller[buyer_id] == g.corp_controller[seller_id]
    {
        return;
    }
    let seller_slot_idx = find_stock_holding(g, seller_id, target_corp_id);
    if seller_slot_idx <= 0 {
        add_remove_asset_sale_offer(g, &PbStr::new(), chosen_offer);
        return;
    }
    let seller_slot = cast::<i32, _>(cast::<i16, _>(seller_slot_idx));
    if (cast::<i32, _>(g.hold_stock_pct[(seller_slot, seller_id)])) < pct_offered {
        pct_offered = cast::<i32, _>(g.hold_stock_pct[(seller_slot, seller_id)]);
    }
    if pct_offered < 1 {
        return;
    }
    market_cap = g.market_cap[target_corp_id];
    if market_cap < sf(0.5f32) {
        market_cap = Ext::from_f64(1.0);
    }
    if get_controller_type(g, seller_id) == 1 && budget > cast::<Ext, _>(seller_budget_cap) {
        budget = cast::<Ext, _>(seller_budget_cap);
    }
    let mut pct_to_buy = to_quad(budget / market_cap);
    if !(cast::<Ext, _>(pct_to_buy) > Ext::from_f64(0.0)) {
        return;
    }
    if cast::<Ext, _>(pct_to_buy) > cast::<Ext, _>(pct_offered) {
        pct_to_buy = cast::<i64, _>(pct_offered);
    }
    if get_controller_type(g, seller_id) == 1
        && (cast::<Ext, _>(pct_to_buy)) < cast::<Ext, _>(pct_offered) * sf(0.2f32)
        && pct_offered > 5
    {
        return;
    }
    if (cast::<Ext, _>(pct_to_buy)) < Ext::from_f64(15.0)
        && find_stock_holding(g, buyer_id, target_corp_id) == 0
        && g.credit_rating_num[target_corp_id] < 5
    {
        return;
    }
    if buyer_industry == 71 {
        if cast::<Ext, _>(pct_to_buy) > cast::<Ext, _>(etf_room_pct) {
            pct_to_buy = cast::<i64, _>(etf_room_pct);
        }
        if (cast::<Ext, _>(pct_to_buy)) < Ext::from_f64(1.0) {
            return;
        }
    }
    if g.credit_rating_num[target_corp_id] < 2 {
        return;
    }
    let buy_slot = cast::<i32, _>(cast::<i16, _>(find_holding_slot(g, buyer_id, target_corp_id)));
    let held_pct = cast::<i32, _>(g.hold_stock_pct[(buy_slot, buyer_id)]);
    let pct_short = to_integer(cast::<Ext, _>(pct_to_buy));
    if get_controller_type(g, buyer_id) == 1
        && ask_autopilot_permission(
            g,
            ui,
            rng,
            buyer_id,
            target_corp_id,
            pct_short,
            held_pct,
            1,
            time_secs,
        ) == 0
    {
        return;
    }
    let mut price = cast::<Ext, _>(pct_to_buy) * market_cap;
    if g.cur_player != g.corp_controller[seller_id] {
        price = price * sf(0.95f32);
    }
    g.cash[buyer_id] = g.cash[buyer_id] - price;
    if g.cash[buyer_id] + g.t_bills[buyer_id] < Ext::from_f64(0.0) {
        g.bank_debt[buyer_id] = g.bank_debt[buyer_id] - g.cash[buyer_id];
        g.cash[g.lender_id] = g.cash[buyer_id] + g.cash[g.lender_id];
        g.cash[buyer_id] = Ext::from_f64(0.0);
    }
    g.hold_stock_id[(buy_slot, buyer_id)] = cast::<i16, _>(target_corp_id);
    g.hold_stock_pct[(buy_slot, buyer_id)] =
        cast::<i16, _>(to_quad(cast::<Ext, _>(pct_to_buy) + cast::<Ext, _>(g.hold_stock_pct[(buy_slot, buyer_id)])));
    g.hold_stock_basis[(buy_slot, buyer_id)] = price + g.hold_stock_basis[(buy_slot, buyer_id)];
    g.cash[seller_id] = price + g.cash[seller_id];
    let seller_pct = g.hold_stock_pct[(seller_slot, seller_id)];
    let seller_basis = g.hold_stock_basis[(seller_slot, seller_id)];
    let gain;
    if cast::<Ext, _>(pct_to_buy) == cast::<Ext, _>(seller_pct) {
        gain = price - seller_basis;
        g.hold_stock_id[(seller_slot, seller_id)] = 0;
        g.hold_stock_pct[(seller_slot, seller_id)] = 0;
        g.hold_stock_basis[(seller_slot, seller_id)] = Ext::from_f64(0.0);
    } else {
        let basis_sold = seller_basis * (cast::<Ext, _>(pct_to_buy) / cast::<Ext, _>(seller_pct));
        gain = price - basis_sold;
        g.hold_stock_pct[(seller_slot, seller_id)] =
            cast::<i16, _>(to_quad(cast::<Ext, _>(g.hold_stock_pct[(seller_slot, seller_id)]) - cast::<Ext, _>(pct_to_buy)));
        g.hold_stock_basis[(seller_slot, seller_id)] = seller_basis - basis_sold;
    }
    if seller_id > 10 {
        g.extraordinary_pl[seller_id] = to_quad(gain + cast::<Ext, _>(g.extraordinary_pl[seller_id]));
    } else {
        g.player_cap_gains[seller_id] = gain + g.player_cap_gains[seller_id];
    }
    g.news_msg = g.ent_symbol[seller_id].clone()
        + " sells"
        + str_num(cast::<Ext, _>(pct_to_buy), 15).as_str()
        + __content.string(1909)
        + &g.ent_symbol[target_corp_id]
        + " to "
        + &g.ent_symbol[buyer_id];
    if g.news_msg.len() < 60 {
        let mut news_suffix = PbStr::from(__content.string(10638));
        if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
            news_suffix = replace(news_suffix.as_bytes(), b"deal", b"sale");
        }
        if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0) {
            g.valuation_mode = 0;
            calc_stock_price(g, ui, rng, target_corp_id, time_secs);
            news_suffix = PbStr::from(" at ") + &g.stock_price_str + __content.string(10639);
        }
        g.news_msg = g.news_msg.clone() + &news_suffix;
    }
    let mut news_a = seller_id;
    if g.cur_player == news_a {
        news_a = 0;
    }
    post_news(g, ui, news_a, target_corp_id);
    let pct_arg = to_long(cast::<Ext, _>(pct_to_buy));
    reduce_asset_sale_offer(g, ui, chosen_offer, pct_arg, 0);
    if g.cur_player == g.corp_controller[seller_id] {
        g.turn_action_count += 1;
        let mut news_suffix = PbStr::new();
        if seller_id > 10 {
            news_suffix = PbStr::from(__content.string(10640)) + &g.ent_name[seller_id] + ")";
        }
        let mut accept_word = PbStr::from(__content.string(10641));
        if (cast::<Ext, _>(pct_to_buy)) < cast::<Ext, _>(pct_offered) {
            accept_word = PbStr::from(__content.string(10642)) + &accept_word;
        }
        // (UI) DIALOG SHOW STATE hDlgMain, 0
        let mut msg = PbStr::from(__content.string(10609))
            + "\r\n"
            + "\r\n"
            + __content.string(10643)
            + &news_suffix
            + __content.string(10644)
            + &g.ent_name[target_corp_id]
            + __content.string(6422)
            + &accept_word
            + __content.string(10645)
            + &g.ent_name[buyer_id]
            + " ("
            + &g.ent_symbol[buyer_id]
            + ").";
        crate::logic::report::show_quick_message(g, ui, &mut msg);
        // (UI) DIALOG SHOW STATE hDlgMain, 5
    }
}

/// Port of CalcIndustryUnmetDemand (0x5db9a8).
/// Sums [runtime resource wording] the active corps 131..1600 of `ind_num` into `IndTotalAssets` and
/// returns the unmet demand `ROUND(IndDemand * 0.8) - IndTotalAssets` (QUAD, at least 0).
pub fn calc_industry_unmet_demand(g: &mut Globals, ind_num: i16) -> i64 {
    g.ind_total_assets = Ext::from_f64(0.0);
    for corp_id in 131..=1600 {
        if g.entity_status[corp_id] == 1 && g.industry_num[corp_id] == ind_num {
            g.ind_total_assets = g.business_assets[corp_id] + g.ind_total_assets;
        }
    }
    let demand = to_quad(cast::<Ext, _>(g.ind_demand[cast::<i32, _>(ind_num)]) * sf(0.8f32));
    let mut unmet_demand = to_quad(cast::<Ext, _>(demand) - g.ind_total_assets);
    if (cast::<Ext, _>(unmet_demand)) < Ext::from_f64(0.0) {
        unmet_demand = 0;
    }
    unmet_demand
}

/// Port of StartNewCorpOrExpand (0x4f21e7) with its GOSUB StartNewCorp_SetUpSubsidiary (0x4f3d35).
/// Starts a company in `industry` in a free entity slot 131..1580 (status 0 slot of that industry, a
/// public shell (industry 70) of its default industry that re-enters, a 1471..1580 shell, a fallback
/// shell, or a random unused slot). `mode = 0`: a new AI start-up (not when 1300+ corps exist or a
/// recent headline already announced an expansion/entry/set-up, nor in an industry with 25 active
/// corps): assets from the industry's demand, cash and T-bills, and possibly a parent shell that owns it
/// 100% (news). `mode = 1`: a company founded with capital `TmpExt1` (start-up cost 10%, at most 100,
/// or 25 for shells), its assets limited by the industry's unmet demand. The slot is cleared
/// (ClearEntityRecord) unless active, activated with a lending bank, valued (ValuationMode 3), may swap
/// names with a free slot (FindFreeEntitySlot), and announced (news posted only by chance in mode 0).
/// Clears `ShortageIndustry` if it was this industry and `TmpExt1`. Returns the new company's id
/// (0 if none). `time_secs`: seconds of `TIME$` (for CalcStockPrice).
pub fn start_new_corp_or_expand(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    industry: i16,
    mode: i32,
    time_secs: i32,
) -> i32 {
    let __content = crate::content::current();
    use crate::logic::control::update_corp_control;
    use crate::logic::finance::get_controller_type;
    use crate::logic::holdings::{clear_all_holdings, find_holding_slot};
    use crate::logic::newgame_helpers::{
        clear_entity_record, find_free_entity_slot, get_default_industry_for_slot,
    };
    use crate::logic::news::is_in_recent_headlines;
    use crate::logic::valuation::{
        adjust_index_divisor, calc_stock_price, check_stock_split, count_active_corps,
    };
    use crate::pb::string::{lcase, ucase};
    // ON ERROR GOTO -> ShowRuntimeError("CreateNewCo", …) (error path not ported)
    let mut corp_count = 0;
    let mut fallback_slot = 0;
    let mut new_corp_id = 0;
    let mut prev_status = 0;
    let mut news_variant = 0;
    let mut excess_assets: i64 = 0;
    let mut startup_cost = Ext::from_f64(0.0);
    let headlines_block = |g: &Globals| {
        for t in [
            &__content.bytes(7792)[..],
            b"EXPANDING",
            b"enters",
            b"ENTERS",
            b"sets up",
            b"SETS UP",
        ] {
            if is_in_recent_headlines(g, t) == 1 {
                return true;
            }
        }
        false
    };
    if mode == 0 {
        if rng.rnd() * Ext::from_f64(30.0) > Ext::from_f64(1.0) && rng.rnd() * Ext::from_f64(50.0) + Ext::from_f64(1300.0) < cast::<Ext, _>(g.active_corp_count) {
            return 0;
        }
        if headlines_block(g) {
            return 0;
        }
        for slot in 131..=1580 {
            if g.entity_status[slot] == 1 && g.industry_num[slot] == industry {
                corp_count += 1;
                if corp_count > 24 {
                    return 0;
                }
            }
        }
        corp_count = 0;
    } else if g.cur_player > 0
        && g.cur_player < 6
        && g.is_computer_player[g.cur_player] == 1
        && headlines_block(g)
    {
        return 0;
    }
    let mut found = false;
    for slot in 131..=1580 {
        if g.entity_status[slot] == 1 && slot > 1470 && slot < 1501 {
            continue;
        }
        prev_status = cast::<i32, _>(g.entity_status[slot]);
        let def_ind = get_default_industry_for_slot(slot);
        if g.industry_num[slot] == 71 {
            continue;
        }
        news_variant = 0;
        if g.industry_num[slot] == 70 && slot < 1501 {
            if slot > 1470 && g.entity_status[slot] == 1 {
                continue;
            }
            new_corp_id = slot;
            if def_ind == industry {
                news_variant = 2;
            } else {
                new_corp_id = 0;
            }
            if new_corp_id == 0 && fallback_slot == 0 {
                fallback_slot = slot;
                g.public_pct = 100;
                if g.entity_status[fallback_slot] == 1 {
                    update_corp_control(g, ui, rng, fallback_slot);
                    if mode == 1 {
                        fallback_slot = 0;
                    }
                }
                if g.public_pct < 100 {
                    fallback_slot = 0;
                }
            }
            if new_corp_id > 0 && g.entity_status[slot] == 1 {
                news_variant = 1;
            }
        }
        if mode == 1 {
            new_corp_id = 0;
            news_variant = 0;
        }
        if new_corp_id == 0 && g.industry_num[slot] == industry && g.entity_status[slot] == 0 {
            new_corp_id = slot;
        }
        if new_corp_id == 0
            && slot > 1470
            && g.industry_num[slot] == 70
            && get_controller_type(g, slot) != 1
        {
            new_corp_id = slot;
            news_variant = 3;
        }
        if new_corp_id == 0 && slot == 1580 {
            if fallback_slot > 130 && mode == 0 && get_controller_type(g, fallback_slot) != 1 {
                new_corp_id = fallback_slot;
                news_variant = 3;
                if g.entity_status[new_corp_id] == 1 {
                    news_variant = 0;
                }
            } else {
                let mut tries = 1;
                while tries <= 50 {
                    let rand_slot = to_long(rng.rnd() * Ext::from_f64(1449.0) + Ext::from_f64(131.0));
                    if g.entity_status[rand_slot] != 1 {
                        new_corp_id = rand_slot;
                        news_variant = 3;
                        tries = 50;
                    }
                    tries += 1;
                }
            }
        }
        if mode == 1 && new_corp_id > 130 && g.entity_status[new_corp_id] == 1 {
            new_corp_id = 0;
            news_variant = 0;
            continue;
        }
        if new_corp_id > 0 {
            g.public_pct = 100;
            if g.entity_status[new_corp_id] == 1 {
                update_corp_control(g, ui, rng, new_corp_id);
            }
            if g.public_pct != 100 {
                new_corp_id = 0;
            }
        }
        if new_corp_id == 0 {
            continue;
        }
        // ---- set up the company in slot new_corp_id
        let n = new_corp_id;
        if g.entity_status[n] != 1 {
            clear_entity_record(g, ui, rng, n);
        }
        if mode != 1 {
            g.cash[n] = (cast::<Ext, _>(rng.rnd_range(1, 100)) + Ext::from_f64(50.0)) * g.exchange_rate + g.cash[n];
            g.t_bills[n] = (cast::<Ext, _>(rng.rnd_range(1, 200)) + Ext::from_f64(100.0)) * g.exchange_rate + g.t_bills[n];
            for t in 131..=1600 {
                if g.industry_num[t] == industry && g.entity_status[t] == 1 {
                    corp_count += 1;
                }
            }
            if corp_count > 12 {
                g.business_assets[n] =
                    cast::<Ext, _>(g.ind_demand[cast::<i32, _>(industry)]) * (rng.rnd() * sf(0.02f32) + sf(0.04f32));
                g.t_bills[n] = g.exchange_rate * Ext::from_f64(200.0) + g.t_bills[n];
            } else {
                g.business_assets[n] = cast::<Ext, _>(g.ind_demand[cast::<i32, _>(industry)]) * sf(0.025f32);
            }
            if g.business_assets[n] > g.exchange_rate * Ext::from_f64(5000.0) {
                g.business_assets[n] = g.exchange_rate * (rng.rnd() * Ext::from_f64(1200.0) + Ext::from_f64(4000.0));
            }
            g.working_capital[n] = g.business_assets[n] * sf(0.12f32);
            g.growth_rate[n] = cast::<i16, _>(to_quad(rng.rnd() * Ext::from_f64(20.0) + Ext::from_f64(20.0)));
            g.profit_margin[n] = cast::<i16, _>(to_quad(rng.rnd() * Ext::from_f64(10.0) - Ext::from_f64(3.0)));
            g.disc_spend_pct[n] = cast::<i16, _>(to_quad((cast::<Ext, _>(g.profit_margin[n])).abs() + Ext::from_f64(15.0)));
            // GOSUB StartNewCorp_SetUpSubsidiary
            if g.entity_status[n] != 1 && rng.rnd() * Ext::from_f64(2.0) <= Ext::from_f64(1.0) {
                let capital = to_quad(
                    ((g.t_bills[n] + g.cash[n]) + g.business_assets[n]) + g.working_capital[n],
                );
                let mut parent_id;
                let mut parent_tries = 100;
                loop {
                    parent_id = rng.rnd_range(131, 1500);
                    if g.entity_status[parent_id] == 1
                        && g.industry_num[parent_id] == 70
                        && get_controller_type(g, parent_id) != 1
                    {
                        if g.hold_stock_id[(0, parent_id)] <= 12
                            && cast::<Ext, _>(capital) * Ext::from_f64(2.0) <= g.t_bills[parent_id] + g.cash[parent_id]
                        {
                            break;
                        }
                        parent_id = 0;
                    } else {
                        parent_id = 0;
                    }
                    parent_tries -= 1;
                    if parent_tries == 0 {
                        break;
                    }
                }
                if parent_id != 0 {
                    let parent_slot = cast::<i32, _>(cast::<i16, _>(find_holding_slot(g, parent_id, n)));
                    if parent_slot > 0 {
                        g.hold_stock_id[(parent_slot, parent_id)] = cast::<i16, _>(n);
                        g.hold_stock_pct[(parent_slot, parent_id)] = 100;
                        g.hold_stock_basis[(parent_slot, parent_id)] = cast::<Ext, _>(capital);
                        g.cash[parent_id] = g.cash[parent_id] - cast::<Ext, _>(capital);
                        if is_in_recent_headlines(g, __content.bytes(10646)) == 0 {
                            g.news_msg = g.ent_symbol[parent_id].clone()
                                + __content.string(10647)
                                + &lcase(g.ind_name[cast::<i32, _>(industry)].as_bytes(), CaseMode::Ansi)
                                + __content.string(4980)
                                + &g.ent_symbol[n];
                            post_news(g, ui, parent_id, n);
                        }
                    }
                }
            }
        } else {
            startup_cost = g.tmp_ext1 * sf(0.1f32);
            if startup_cost > g.exchange_rate * Ext::from_f64(100.0) {
                startup_cost = g.exchange_rate * Ext::from_f64(100.0);
            }
            if industry == 70 && startup_cost > g.exchange_rate * Ext::from_f64(25.0) {
                startup_cost = g.exchange_rate * Ext::from_f64(25.0);
            }
            let capital = to_quad(g.tmp_ext1 - startup_cost);
            let mut unmet_demand = cast::<Ext, _>(calc_industry_unmet_demand(g, industry));
            g.t_bills[n] = cast::<Ext, _>(capital) * sf(0.28f32);
            g.working_capital[n] = cast::<Ext, _>(capital) * sf(0.08f32);
            g.business_assets[n] = cast::<Ext, _>(capital) * sf(0.64f32);
            if unmet_demand < g.exchange_rate * Ext::from_f64(100.0) {
                unmet_demand = g.exchange_rate * Ext::from_f64(100.0);
            }
            if unmet_demand < g.business_assets[n] {
                excess_assets = to_quad(g.business_assets[n] - unmet_demand);
            }
            if cast::<Ext, _>(excess_assets) > Ext::from_f64(0.0) {
                g.business_assets[n] = g.business_assets[n] - cast::<Ext, _>(excess_assets);
                g.working_capital[n] = g.working_capital[n] - cast::<Ext, _>(excess_assets) * sf(0.12f32);
                g.t_bills[n] = cast::<Ext, _>(excess_assets) * sf(1.12f32) + g.t_bills[n];
            }
            if industry != 70 {
                g.profit_margin[n] = cast::<i16, _>(to_quad(-Ext::from_f64(5.0) + rng.rnd() * Ext::from_f64(12.0)));
                g.disc_spend_pct[n] =
                    cast::<i16, _>(to_quad(cast::<Ext, _>(rng.rnd_range(1, 3)) + (cast::<Ext, _>(g.profit_margin[n])).abs()));
            }
        }
        g.tax_credits[n] = g.business_assets[n] * sf(0.07f32) + g.tax_credits[n];
        g.corp_controller[n] = n;
        if g.ent_misc_data[n].len() < 21 {
            g.ent_misc_data[n] = PbStr::new();
        }
        g.industry_num[n] = industry;
        if g.entity_status[n] == 1 {
            news_variant = 0;
            if n < 1471 {
                news_variant = 1;
                if industry != def_ind {
                    news_variant = 3;
                }
            }
        } else {
            g.entity_status[n] = 1;
            g.lending_bank[n] = 0;
            while g.lending_bank[n] < 11 {
                excess_assets = to_quad(cast::<Ext, _>(rng.rnd_range(50, 85)));
                if g.entity_status[cast::<i32, _>(to_quad(cast::<Ext, _>(excess_assets)))] != 1 {
                    excess_assets = to_quad(cast::<Ext, _>(rng.rnd_range(11, 85)));
                }
                let b = cast::<i32, _>(to_quad(cast::<Ext, _>(excess_assets)));
                if g.industry_num[b] == 1 && g.entity_status[b] == 1 {
                    g.lending_bank[n] = cast::<i16, _>(to_quad(cast::<Ext, _>(excess_assets)));
                }
            }
            g.shares_outstanding[n] = Ext::from_f64(1.0);
            if news_variant == 0 {
                news_variant = 2;
            }
            if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(2.0) {
                news_variant = 3;
            }
        }
        g.valuation_mode = 3;
        if mode == 1 {
            g.market_cap[n] = g.tmp_ext1 * sf(0.01f32);
            news_variant = 3;
            g.accrued_pl[n] = g.accrued_pl[n] - startup_cost;
            g.tmp_ext3 = g.tmp_ext1;
        } else {
            g.tmp_ext3 = ((g.t_bills[n] + g.cash[n]) + g.business_assets[n]) + g.working_capital[n];
            g.accrued_pl[n] = g.accrued_pl[n] - rng.rnd() * Ext::from_f64(200.0) * g.exchange_rate;
        }
        calc_stock_price(g, ui, rng, n, time_secs);
        adjust_index_divisor(g, n, 0);
        check_stock_split(g, n);
        if prev_status == 0 && rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0) {
            find_free_entity_slot(g, rng, n, industry);
        }
        let last = g.last_ind_entry_news_variant;
        if last == 1 && news_variant == 1 {
            news_variant = 3;
        } else if last == 2 && news_variant == 2 {
            news_variant = 3;
            if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                news_variant = 0;
            }
        } else if last == 3 && news_variant == 3 {
            news_variant = 2;
        } else if last == 0 && news_variant == 0 {
            news_variant = 3;
        }
        let verb_text = match news_variant {
            1 => {
                g.last_ind_entry_news_variant = 1;
                __content.string(10486)
            }
            2 => {
                g.last_ind_entry_news_variant = 2;
                __content.string(10648)
            }
            3 => {
                g.last_ind_entry_news_variant = 3;
                __content.string(10649)
            }
            _ => {
                g.last_ind_entry_news_variant = 0;
                __content.string(10650)
            }
        };
        g.news_msg = g.ent_symbol[n].clone()
            + verb_text
            + &mcase(g.ind_name[cast::<i32, _>(industry)].as_bytes(), CaseMode::Ansi)
            + __content.string(1547);
        if is_in_recent_headlines(g, __content.bytes(10651)) == 1 {
            g.news_msg = PbStr::new();
        } else {
            if g.news_msg.len() < 48 {
                g.news_msg = ucase(g.news_msg.as_bytes(), CaseMode::Ansi);
            }
            if mode == 0 && g.rand_seed % 3 == 2 {
                post_news(g, ui, 0, 0);
            }
        }
        if industry == g.shortage_industry {
            g.shortage_industry = 0;
        }
        found = true;
        break;
    }
    if !found {
        new_corp_id = 0;
    }
    g.tmp_ext1 = Ext::from_f64(0.0);
    let result_id = new_corp_id;
    if new_corp_id > 10 && prev_status != 1 {
        g.entity_status[new_corp_id] = cast::<i16, _>(prev_status);
        clear_all_holdings(g, 2);
        g.entity_status[new_corp_id] = 1;
        count_active_corps(g);
        if industry < 70 {
            g.ctrl_since_year[new_corp_id] = g.cur_year;
        }
    }
    if news_variant != 1 && new_corp_id > 10 {
        g.tax_loss_carry[new_corp_id] = Ext::from_f64(0.0);
    }
    result_id
}

/// Port of AskAutopilotPermission (0x4be9a8).
/// For a company of the current (human) player on autopilot (`AutoPilot(corp_id) <> 0`, controller type
/// 1, not managed by the player), asks — at most once per quarter, and not again this year for the same
/// company unless selling — permission for a stock transaction: `action_type` 0/1 = buy `pct_change`%
/// of `target_id` (1: from a discounted offer), 2 = list `pct_change`% of `target_id` for sale, 3 =
/// issue `pct_change`% new stock. `owned_pct` is the current stake. A first message box announces the
/// request, a YES/NO/CANCEL box asks; returns 1 for YES. NO or CANCEL remember the denied target/issuer
/// (`PermDenied*`), and NO selects the target for the player (SelectTargetCorp; a buy from an offer
/// also sets `AutoPilotRejectedTarget`). Returns 0 without asking for denied targets, funds as buy
/// targets, while earnings display is suppressed for the selected entity, or in the other cases.
/// `time_secs`: seconds of `TIME$` (for SelectTargetCorp).
pub fn ask_autopilot_permission(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    corp_id: i32,
    target_id: i32,
    pct_change: i16,
    owned_pct: i32,
    action_type: i32,
    time_secs: i32,
) -> i32 {
    let __content = crate::content::current();
    use crate::logic::finance::{get_controller_type, get_managing_player};
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut result = 0;
    if corp_id < 11 || get_controller_type(g, corp_id) != 1 || g.auto_pilot[corp_id] == 0 {
        return result;
    }
    if g.cur_quarter == g.perm_ask_quarter {
        return result;
    }
    if corp_id == g.perm_ask_corp && g.cur_year == g.perm_ask_year && action_type != 2 {
        return result;
    }
    if g.cur_year != g.perm_ask_year {
        g.perm_ask_corp = 0;
    }
    if action_type < 2 && g.perm_denied_buy_target == target_id {
        return result;
    }
    if action_type == 2 && g.perm_denied_sell_target == target_id {
        return result;
    }
    if action_type == 3 && g.perm_denied_issue_corp == corp_id {
        return result;
    }
    let new_pct;
    if action_type < 2 {
        new_pct = cast::<i32, _>(pct_change) + owned_pct;
        if target_id > 10 && g.industry_num[target_id] == 71 {
            return result;
        }
    } else {
        new_pct = owned_pct - cast::<i32, _>(pct_change);
    }
    if g.cur_suppress_earnings == 1 && g.cur_player == g.sel_entity {
        return result;
    }
    if g.cur_player != g.corp_controller[corp_id] {
        return result;
    }
    if get_managing_player(g, corp_id) > 0 {
        return result;
    }
    g.perm_ask_quarter = g.cur_quarter;
    g.perm_ask_year = g.cur_year;
    g.perm_ask_corp = corp_id;
    let mut msg_text =
        PbStr::from(__content.string(10652))
            + &g.ent_name[corp_id];
    if right(msg_text.as_bytes(), 1) != "." {
        msg_text = msg_text + ".";
    }
    msg_text = msg_text
        + "\r"
        + "\r"
        + __content.string(10653)
        + "\r"
        + "\r"
        + __content.string(10654);
    ui.msgbox(
        &msg_text,
        0x2040,
        &PbStr::from(__content.string(10655)),
    );
    msg_text = PbStr::from(__content.string(10656))
        + &g.ent_name[corp_id]
        + " ("
        + &g.ent_symbol[corp_id]
        + __content.string(10657);
    if action_type < 3 {
        msg_text = msg_text
            + "to buy"
            + str_num(cast::<Ext, _>(pct_change), 7).as_str()
            + __content.string(2928)
            + &g.ent_name[target_id]
            + " ("
            + &g.ent_symbol[target_id]
            + "). ";
        if action_type < 2 && target_id > 10 {
            let trend_months = crate::logic::valuation::price_trend_streak(g, target_id);
            if (cast::<Ext, _>(trend_months)).abs() > Ext::from_f64(0.0) {
                let trend_verb = if trend_months < 0 { "sink" } else { "ris" };
                let words = crate::logic::report::number_to_words(trend_months, 0, 0);
                msg_text = msg_text
                    + __content.string(10658)
                    + &g.ent_symbol[target_id]
                    + __content.string(6422)
                    + trend_verb
                    + __content.string(10659)
                    + &words
                    + __content.string(10660);
            } else if g.proj_qtr_earnings[target_id] < Ext::from_f64(0.0) {
                msg_text = msg_text
                    + __content.string(10661)
                    + &g.ent_symbol[target_id]
                    + __content.string(10662);
            } else if g.credit_rating_num[target_id] < 4 {
                msg_text = msg_text
                    + __content.string(10661)
                    + &g.ent_symbol[target_id]
                    + __content.string(10663);
            }
        }
        msg_text = msg_text + "\r" + "\r";
    } else if action_type == 3 {
        msg_text = msg_text
            + __content.string(10664)
            + str_num(cast::<Ext, _>(pct_change), 7).as_str()
            + "%."
            + "\r"
            + "\r";
    }
    if action_type == 2 {
        msg_text = replace(
            msg_text.as_bytes(),
            b"to buy",
            __content.bytes(10665),
        );
        msg_text = replace(msg_text.as_bytes(), __content.bytes(10666), b"offer");
    } else if action_type == 1 {
        msg_text = msg_text
            + __content.string(1093)
            + &g.ent_name[target_id]
            + __content.string(10667)
            + __content.string(10668)
            + "\r"
            + "\r";
    }
    if owned_pct > 0 {
        msg_text = msg_text
            + "("
            + &g.ent_symbol[corp_id]
            + __content.string(10669)
            + str_num(cast::<Ext, _>(owned_pct), 15).as_str()
            + __content.string(2163)
            + &g.ent_name[target_id]
            + __content.string(10670)
            + str_num(cast::<Ext, _>(new_pct), 15).as_str()
            + __content.string(10671);
        if action_type < 2 {
            msg_text = msg_text + __content.string(10672) + "\r" + "\r";
        } else if action_type == 2 {
            msg_text = msg_text + __content.string(10673) + "\r" + "\r";
            msg_text = replace(msg_text.as_bytes(), b"already", __content.bytes(10674));
        }
    } else if action_type < 2 {
        msg_text = msg_text
            + "("
            + &g.ent_symbol[corp_id]
            + __content.string(10675)
            + &g.ent_name[target_id]
            + ".)"
            + "\r"
            + "\r";
    }
    msg_text = msg_text + __content.string(10676) + &g.ent_name[corp_id] + __content.string(10677);
    if action_type < 2 {
        msg_text = msg_text + __content.string(10678);
    } else if action_type == 2 {
        msg_text =
            msg_text + __content.string(10679) + &g.ent_symbol[target_id] + "?";
    } else if action_type == 3 {
        msg_text = msg_text + __content.string(10680);
    }
    msg_text = msg_text
        + "\r"
        + "\r"
        + &g.ent_name[corp_id]
        + __content.string(10681)
        + __content.string(10682)
        + __content.string(10683)
        + "\r"
        + "\r"
        + __content.string(10684)
        + "\r"
        + "\r"
        + __content.string(10685)
        + "\r"
        + __content.string(10686)
        + __content.string(10687)
        + "\r"
        + __content.string(10688);
    let mut title_text =
        PbStr::from(__content.string(10689)) + &g.ent_symbol[corp_id];
    if action_type == 2 {
        title_text = replace(
            title_text.as_bytes(),
            __content.bytes(10690),
            __content.bytes(10691),
        );
    }
    if action_type == 3 {
        title_text = replace(title_text.as_bytes(), __content.bytes(10692), __content.bytes(10693));
    }
    let answer = ui.msgbox(&msg_text, 0x2123, &title_text);
    if answer != 6 {
        if action_type < 2 {
            g.perm_denied_buy_target = target_id;
        }
        if action_type == 2 {
            g.perm_denied_sell_target = target_id;
        }
        if action_type == 3 {
            g.perm_denied_issue_corp = corp_id;
        }
    }
    if answer == 6 {
        result = 1;
    }
    if answer == 7 {
        g.target_corp = target_id;
        if action_type == 3 {
            g.target_corp = corp_id;
        }
        if action_type == 1 {
            g.auto_pilot_rejected_target = target_id;
        }
        crate::logic::market::select_target_corp(g, ui, rng, time_secs);
        // (UI) CONTROL SET TEXT GetDlgItem(hDlgMain, 202), EntSymbol(SelEntity)
    }
    result
}
