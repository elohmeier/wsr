//! Player-visible observations. This module never receives the live RNG, files or host UI.
use serde_json::{Value, json};
use wsr_core::{
    pb::{pbstr::PbStr, real::Ext, rng::Rng},
    state::Globals,
    ui::Scripted,
};

fn number(x: impl Into<Ext>) -> Value {
    let x = x.into().to_f64();
    if x.is_finite() { json!(x) } else { Value::Null }
}

fn financials(g: &Globals, entity: i32) -> Value {
    let mut view = g.clone();
    wsr_core::logic::finance::calc_entity_financials(
        &mut view,
        &mut Scripted::default(),
        &mut Rng::from_state(1),
        entity,
    );
    json!({
        "id": entity, "name": g.ent_name[entity].to_display(),
        "cash": number(g.cash[entity]), "treasuryBills": number(g.t_bills[entity]),
        "bankDebt": number(g.bank_debt[entity]), "totalAssets": number(view.total_assets),
        "totalDebt": number(view.tmp_total_debt), "netWorth": number(view.net_worth),
        "unusedCreditLine": number(view.unused_credit_line),
        "creditFrozen": g.credit_frozen[entity] != 0,
    })
}

fn fields(rec: &PbStr) -> Vec<String> {
    rec.to_display()
        .split('|')
        .map(|s| s.trim().to_string())
        .collect()
}

fn owned_records(
    records: &wsr_core::pb::array::Arr1<PbStr>,
    owner: i32,
    owner_fields: &[usize],
) -> Vec<Value> {
    (1..=1000)
        .filter_map(|i| {
            let f = fields(&records[i]);
            if owner_fields
                .iter()
                .any(|&index| f.get(index).and_then(|s| s.parse::<i32>().ok()) == Some(owner))
            {
                Some(json!(f))
            } else {
                None
            }
        })
        .collect()
}

fn portfolio(g: &Globals, owner: i32) -> Value {
    let holdings: Vec<Value> = (1..=15).filter_map(|slot| {
        let id = i32::from(g.hold_stock_id[(slot, owner)]);
        let percent = g.hold_stock_pct[(slot, owner)];
        if !(11..=1600).contains(&id) || percent == 0 { return None; }
        Some(json!({
            "id": id, "name": g.ent_name[id].to_display(), "symbol": g.ent_symbol[id].to_display(),
            "percentOfCompany": percent, "side": if percent < 0 { "short" } else { "long" },
            // MarketCap is in hundreds of millions; a holding is a whole percentage.
            "marketValue": number(g.market_cap[id] * Ext::from_f64(f64::from(percent))),
            "costBasis": number(g.hold_stock_basis[(slot, owner)]),
        }))
    }).collect();
    let bonds: Vec<Value> = (1..=15)
        .filter_map(|slot| {
            let id = g.hold_bond_id[(slot, owner)];
            if !(11..=1600).contains(&id) {
                return None;
            }
            Some(json!({
                "issuerId": id, "name": g.ent_name[id].to_display(),
                "faceValue": g.hold_bond_face[(slot, owner)],
                "pricePercentOfPar": number(g.bond_price[id].get()),
                "costBasis": number(g.bond_hold_cost[(slot, owner)]),
            }))
        })
        .collect();
    json!({
        "entityId": owner, "holdings": holdings, "corporateBonds": bonds,
        "longGovernmentBondFace": g.long_gov_bond_face[owner],
        "shortGovernmentBondFace": g.short_gov_bond_face[owner],
        "commodityMarginBalance": number(g.commod_margin_bal[owner]),
        "options": owned_records(&g.option_recs, owner, &[2]),
        "futures": owned_records(&g.futures_recs, owner, &[1]),
        "physicalCommodities": owned_records(&g.phys_commod_rec, owner, &[1]),
        "swaps": owned_records(&g.swap_contracts, owner, &[0, 1]),
        "recordSchemas": {
            "options": ["underlyingId", "type:1=call,2=put", "holderId", "percentOfShares:negative=written", "strikeInMarketCapUnits:divideBySharesForPrice", "expiryMonth", "expiryYear", "premiumMillionsOrGRANTorVESTED"],
            "futures": ["commodity", "ownerId", "quantity", "expiryMonth", "expiryYear", "cost", "price"],
            "physicalCommodities": ["commodity", "ownerId", "units", "cost"],
            "swaps": ["receiveFixedEntity", "payFixedEntity", "fixedRatePercent", "floatingRate:P=prime,L=longBond,S=shortBond", "startYearQuarter", "endYearQuarter", "notionalMillions"]
        }
    })
}

/// Only public corporate facts, the current player's finances, and the active controlled entity.
/// Forecasts, event queues, AI intents, other players' balances and private contracts are omitted.
pub fn observe(g: &Globals) -> Value {
    let player = g.cur_player;
    if !(1..=g.num_players.min(5)).contains(&player) || g.is_computer_player[player] != 0 {
        return json!({"schemaVersion":1, "available":false, "reason":"No human player's turn is available."});
    }
    // The desk's date/economy readouts already calculate on an isolated clone.
    let summary: Value =
        serde_json::from_str(&crate::summary::summary_json(g)).unwrap_or(Value::Null);
    let companies: Vec<Value> = (11..=1600).filter(|&id| g.entity_status[id] == 1).map(|id| {
        let shares = g.shares_outstanding[id];
        json!({
            "id": id, "name": g.ent_name[id].to_display(), "symbol": g.ent_symbol[id].to_display(),
            "industryId": g.industry_num[id], "countryId": g.country_idx[id],
            "marketCapitalization": number(g.market_cap[id] * Ext::from_f64(100.0)),
            "price": if shares > Ext::from_f64(0.0) { number(g.market_cap[id] / shares) } else { Value::Null },
            "annualDividendPerShare": number(g.dividend_per_share[id]),
            "cash": number(g.cash[id]), "treasuryBills": number(g.t_bills[id]),
            "bankDebt": number(g.bank_debt[id]), "bondsIssuedFace": g.bonds_issued[id],
            "earningsLastYear": number(g.earnings_y1[id]),
            "earningsTwoYearsAgo": number(g.earnings_yr2_ago[id]),
            "earningsThreeYearsAgo": number(g.earnings_yr3_ago[id]),
            "sharesOutstandingInHundredMillions": number(shares),
            "creditRatingRank": g.credit_rating_num[id], "controllerId": g.corp_controller[id],
        })
    }).collect();
    let industries: Vec<Value> = (1..=71)
        .map(|id| json!({"id":id,"name":g.ind_name[id].to_display()}))
        .collect();
    let active = g.active_entity;
    let controlled = (11..=1600).contains(&active) && g.corp_controller[active] == player;
    let mut portfolios = vec![portfolio(g, player)];
    let active_finances = if controlled {
        portfolios.push(portfolio(g, active));
        financials(g, active)
    } else {
        Value::Null
    };
    json!({
        "schemaVersion":1, "available":true,
        "context": {
            "playerId":player, "playerName":g.player_name[player].to_display(),
            "activeEntityId":active, "selectedEntityId":g.sel_entity,
            "gameDate":summary["gameDate"], "year":g.cur_year, "quarter":g.cur_quarter,
            "transactionsLeft":summary["transactionsLeft"], "gameOver":summary["gameOver"],
            "currencyCode":g.currency_code.to_display(), "currencySymbol":g.currency_symbol.to_display(),
        },
        "units": {
            "money":format!("{}illions of {}", g.million_prefix_lc.to_display(), g.currency_code.to_display()), "price":"displayed currency per share",
            "rates":"percent", "marketCapitalization":"same unit as money", "holdings":"percent of the company, not percent of the portfolio",
            "creditRatingRank":"10=AAA,9=AA,8=A,7=BBB,6=BB,5=B,4=CCC,3=CC,2=C,0=D",
        },
        "player":financials(g, player), "activeEntity":active_finances, "portfolios":portfolios,
        "companies":companies, "industries":industries,
        "economy": {
            "stockIndex":number(g.stock_index), "primeRate":number(g.prime_rate),
            "gdpGrowth":number(g.gdp_growth), "commoditiesAndYields":summary["economy"],
        },
        "news":summary["news"],
        "limitations":["Detailed company research and price charts are available to the coach after the player opens them. No forecasts or hidden game state are exported."]
    })
}

/// No live UI or RNG capability is passed to the observer.
#[unsafe(no_mangle)]
pub extern "C" fn wsr_observe() -> *mut u8 {
    let value = crate::GAME.with(|cell| {
        cell.borrow().as_ref().map(observe).unwrap_or_else(
            || json!({"schemaVersion":1,"available":false,"reason":"No game is loaded."}),
        )
    });
    crate::result(value.to_string().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Globals {
        crate::install_test_content();
        let mut g = Globals::default();
        g.num_players = 2;
        g.cur_player = 1;
        g.active_entity = 131;
        g.sel_entity = 131;
        g.entity_status[1] = 1;
        g.entity_status[131] = 1;
        g.corp_controller[131] = 1;
        g.cash[1] = Ext::from_f64(20.0);
        g.cash[2] = Ext::from_f64(987654.0);
        g.market_cap[131] = Ext::from_f64(12.0);
        g.shares_outstanding[131] = Ext::from_f64(2.0);
        g.hold_stock_id[(1, 1)] = 131;
        g.hold_stock_pct[(1, 1)] = 10;
        g.option_recs[1] = PbStr::from("131|1|1|10|20|3|2027|1");
        g.option_recs[2] = PbStr::from("131|1|2|10|20|3|2027|SECRET");
        g
    }

    #[test]
    fn observations_are_repeatable_and_preserve_all_live_state() {
        let g = fixture();
        crate::GAME.with(|cell| *cell.borrow_mut() = Some(g));
        crate::RNG.with(|cell| *cell.borrow_mut() = Rng::from_state(123456));
        crate::FILES.with(|cell| {
            cell.borrow_mut()
                .files
                .insert("GAME01.DAT".into(), b"original".to_vec())
        });
        let before = crate::GAME.with(|cell| cell.borrow().as_ref().unwrap().to_json());
        let files_before = crate::FILES.with(|cell| cell.borrow().clone());
        let a = crate::GAME.with(|cell| observe(cell.borrow().as_ref().unwrap()));
        let b = crate::GAME.with(|cell| observe(cell.borrow().as_ref().unwrap()));
        assert_eq!(a, b);
        assert_eq!(
            crate::GAME.with(|cell| cell.borrow().as_ref().unwrap().to_json()),
            before
        );
        assert_eq!(crate::RNG.with(|cell| cell.borrow().state()), 123456);
        crate::FILES.with(|cell| {
            assert_eq!(cell.borrow().files, files_before.files);
            assert_eq!(cell.borrow().written, files_before.written);
        });
        assert_eq!(a["companies"][0]["price"], 6.0);
        assert_eq!(a["companies"][0]["marketCapitalization"], 1200.0);
        assert_eq!(a["portfolios"][0]["holdings"][0]["marketValue"], 120.0);
        assert!(!a.to_string().contains("987654"));
        assert!(!a.to_string().contains("SECRET"));
        assert_eq!(a["portfolios"][0]["options"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn other_players_and_uncontrolled_active_entities_are_not_exported() {
        let mut g = fixture();
        g.corp_controller[131] = 2;
        let value = observe(&g);
        assert_eq!(value["activeEntity"], Value::Null);
        assert_eq!(value["portfolios"].as_array().unwrap().len(), 1);
        g.cur_player = 0;
        assert_eq!(observe(&g)["available"], false);
        g.cur_player = 1;
        g.is_computer_player[1] = 1;
        assert_eq!(observe(&g)["available"], false);
    }
}
