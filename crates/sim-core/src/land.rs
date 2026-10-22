//! Land and property market: assessment, voluntary and forced sales, and
//! Harberger (COST) buy-outs. Prices are formed by bids — each bidder values
//! the parcel with its own discount rate and tax position — never by a
//! formula that bakes in the expected effect of a tax.

use crate::econ::{self, Investor};
use crate::types::*;
use crate::world::{Event, World};

/// Quarterly mass appraisal. Land is valued "as if vacant": what the site
/// would fetch from a typical (absentee) investor for its best permitted
/// project, given local rents, occupancy and the tax schedule. Nothing the
/// owner has built enters, so a land tax stays a fixed charge. Recent sale
/// prices of unbuilt land are blended in, values are floored at
/// agricultural use, and assessors smooth over several rounds (≈ 1 year).
/// Land without street access can only be farmed until a road arrives, so it
/// is assessed as farmland.
pub fn assess(w: &mut World, force: bool) {
    if !force && w.tick % 3 != 0 {
        return;
    }
    let inv = w.absentee_investor();
    let noise = w.econ.assessment_noise as f64;
    let n = w.p.len();
    for i in 0..n {
        let u = w.p.land_use[i];
        if matches!(u, LandUse::Water | LandUse::Road) {
            w.p.assessed_land[i] = 0.0;
            w.p.assessed_bldg[i] = 0.0;
            continue;
        }
        let farm = farm_value(w, i, &inv);
        let bldg = econ::building_value(w, i);
        let site =
            if w.p.in_city[i] || w.p.road_access[i] { econ::self_consistent_site_value(w, i, &inv) } else { 0.0 };
        let mut land = site.max(farm);
        // Only sales of unbuilt land are land comparables. A built parcel
        // sells with its building and its leases (a sitting tenant may pay
        // far above market rent), so price minus replacement cost is not
        // land value.
        if w.p.months_since_sale[i] < 24
            && w.p.price[i] > 0.0
            && matches!(u, LandUse::Wild | LandUse::Farm | LandUse::Vacant)
        {
            land = (2.0 * land + w.p.price[i] as f64) / 3.0;
        }
        let eps = 1.0 + noise * w.rng.normal().clamp(-2.5, 2.5);
        let fresh = (land.max(farm) * eps).max(0.0) as f32;
        let old = w.p.assessed_land[i];
        // Symmetric smoothing: the best-project residual is a thin margin of
        // value over construction cost, which a nearby project start or a
        // quarter's dip in expected occupancy can wipe out for a quarter.
        w.p.assessed_land[i] = if force || old <= 0.0 { fresh } else { 0.75 * old + 0.25 * fresh };
        w.p.assessed_bldg[i] = bldg as f32;
        if w.p.months_since_sale[i] >= 24 || w.p.price[i] <= 0.0 {
            w.p.price[i] = w.p.assessed_land[i] + w.p.assessed_bldg[i];
        }
    }
}

/// Capitalised agricultural rent: the floor under all land values.
pub fn farm_value(w: &World, i: usize, inv: &Investor) -> f64 {
    let farm_rent = w.econ.farm_rent as f64 * (0.4 + w.terrain.fertility[i] as f64);
    let denom = (inv.r - inv.g + w.taxes.holding_rate(1.0)).max(0.01);
    12.0 * farm_rent / denom
}

/// Owners under a Harberger tax declare a value: their own valuation times a
/// personal markup that adapts (lower when never challenged, higher after
/// being bought out).
pub fn declare(w: &mut World) {
    if w.taxes.harberger_rate() <= 0.0 {
        return;
    }
    for i in 0..w.p.len() {
        if !crate::property::taxable(w, i) || (i as u64 + w.tick) % 6 != 0 {
            continue;
        }
        let owner = w.p.owner[i];
        let inv = w.investor_for(owner);
        let v = econ::current_use_value(w, i, &inv).max(econ::valuation(w, i, &inv) * 0.9);
        let markup = match owner {
            Owner::Hh(id) => {
                let h = &mut w.hh[id as usize];
                h.declare_markup = (h.declare_markup * 0.995).clamp(0.8, 1.6);
                h.declare_markup
            }
            _ => 1.0,
        } as f64;
        w.p.declared[i] = (v * markup) as f32;
    }
}

struct Bid {
    buyer: Owner,
    value: f64,
}

fn candidate_buyers(w: &mut World, rich: &[u32], i: usize) -> Vec<Bid> {
    let mut bids = Vec::with_capacity(5);
    let inv = w.absentee_investor();
    bids.push(Bid { buyer: Owner::Absentee, value: econ::valuation(w, i, &inv) });
    for _ in 0..4 {
        if let Some(&id) = w.rng.pick(rich) {
            if w.p.owner[i] == Owner::Hh(id) {
                continue;
            }
            let inv = w.household_investor(id);
            bids.push(Bid { buyer: Owner::Hh(id), value: econ::valuation(w, i, &inv) });
        }
    }
    bids
}

fn can_afford(w: &World, buyer: Owner, price: f64, transfer: f64) -> bool {
    match buyer {
        Owner::Hh(id) => {
            let ltv = w.econ.loan_to_value as f64;
            w.hh[id as usize].cash >= price * (1.0 - ltv) + transfer
        }
        _ => true,
    }
}

/// Transfer title. Buyer pays price + transfer tax; seller is paid out net
/// of the mortgage. Household buyers finance with a mortgage.
fn execute(w: &mut World, i: usize, buyer: Owner, price: f64) {
    let seller = w.p.owner[i];
    let transfer = if buyer == Owner::Gov { 0.0 } else { w.taxes.levy(|m| m.transfer(price)) };
    w.gov.cash += transfer;
    // buyer side
    let mut new_mortgage = 0.0;
    match buyer {
        Owner::Hh(id) => {
            let ltv = w.econ.loan_to_value as f64;
            let h = &mut w.hh[id as usize];
            let down = (price * (1.0 - ltv)).max(price - (h.cash - transfer).max(0.0));
            let down = down.min(price);
            new_mortgage = price - down;
            h.cash -= down + transfer;
            h.taxes_paid += transfer as f32;
        }
        Owner::Absentee => w.flows.absentee_income -= price + transfer,
        Owner::Gov => w.gov.cash -= price,
    }
    // seller side
    let old_mortgage = w.p.mortgage[i] as f64;
    let net = price - old_mortgage;
    match seller {
        Owner::Hh(id) => {
            w.hh[id as usize].cash += net;
        }
        Owner::Absentee => w.flows.absentee_income += net,
        Owner::Gov => w.gov.cash += net,
    }
    w.p.owner[i] = buyer;
    w.p.mortgage[i] = new_mortgage as f32;
    w.p.price[i] = price as f32;
    w.p.months_since_sale[i] = 0;
    w.flows.sales += 1;
}

pub fn market(w: &mut World) {
    let e = w.econ.clone();
    let n = w.p.len();
    // Resident investors: households with savings to deploy.
    let rich: Vec<u32> =
        w.hh.iter()
            .enumerate()
            .filter(|(_, h)| h.alive && h.cash > 60_000.0 * w.index as f64)
            .map(|(id, _)| id as u32)
            .collect();

    // ---- forced sales: owners who cannot meet obligations
    let mut distressed: Vec<u32> =
        w.hh.iter().enumerate().filter(|(_, h)| h.alive && h.cash < 0.0).map(|(id, _)| id as u32).collect();
    w.rng.shuffle(&mut distressed);
    if !distressed.is_empty() {
        let mut owned: Vec<Vec<u32>> = vec![Vec::new(); w.hh.len()];
        for i in 0..n {
            if let Owner::Hh(id) = w.p.owner[i] {
                if w.hh[id as usize].cash < 0.0 {
                    owned[id as usize].push(i as u32);
                }
            }
        }
        for id in distressed {
            let Some(&i) = owned[id as usize]
                .iter()
                .max_by(|a, b| w.p.assessed_land[**a as usize].total_cmp(&w.p.assessed_land[**b as usize]))
            else {
                continue;
            };
            let i = i as usize;
            // Sell to the highest bidder at a fire-sale discount.
            let bids = candidate_buyers(w, &rich, i);
            let best = bids
                .into_iter()
                .filter(|b| can_afford(w, b.buyer, b.value, 0.0))
                .max_by(|a, b| a.value.total_cmp(&b.value));
            if let Some(b) = best {
                let price = (b.value / (1.0 + w.taxes.transfer_cost(1.0))) * 0.9;
                execute(w, i, b.buyer, price.max(1.0));
                w.flows.forced_sales += 1;
                w.push_event(Event::Sale { cell: i as u32, forced: true });
            }
        }
    }

    // ---- owners in tax arrears try to sell before they walk away, and the
    // city auctions foreclosed parcels to the highest bidder
    for i in 0..n {
        let in_arrears = w.p.arrears[i] > 0;
        let foreclosed = w.p.owner[i] == Owner::Gov && w.p.land_use[i].is_developable();
        if !(in_arrears || foreclosed) || (i as u64 + w.tick) % 3 != 0 {
            continue;
        }
        let bids = candidate_buyers(w, &rich, i);
        let t_rate = w.taxes.transfer_cost(1.0);
        let best = bids
            .into_iter()
            .map(|b| (b.buyer, b.value / (1.0 + t_rate)))
            .filter(|&(buyer, price)| price > 0.0 && can_afford(w, buyer, price, price * t_rate))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((buyer, price)) = best {
            if buyer != w.p.owner[i] {
                execute(w, i, buyer, price);
                w.p.arrears[i] = 0;
                w.push_event(Event::Sale { cell: i as u32, forced: true });
            }
        }
    }

    // ---- voluntary sales and Harberger buy-outs on a sample of parcels
    let samples = ((n as f32) * e.review_rate * 0.5) as usize;
    let harberger = w.taxes.harberger_rate() > 0.0;
    for _ in 0..samples {
        let i = w.rng.below(n);
        if !crate::property::taxable(w, i)
            || !w.p.land_use[i].is_developable() && w.p.land_use[i] != LandUse::Construction
        {
            continue;
        }
        if w.p.land_use[i] == LandUse::Construction {
            continue;
        }
        let seller = w.p.owner[i];
        let seller_inv = w.investor_for(seller);
        let reservation = econ::valuation(w, i, &seller_inv) * 1.03;
        let bids = candidate_buyers(w, &rich, i);
        let t_rate = w.taxes.transfer_cost(1.0);
        if harberger {
            // Anyone may buy at the declared price.
            let declared = w.p.declared[i] as f64;
            let taker = bids
                .iter()
                .filter(|b| b.buyer != seller && b.value > declared * (1.0 + t_rate) && declared > 0.0)
                .filter(|b| can_afford(w, b.buyer, declared, declared * t_rate))
                .max_by(|a, b| a.value.total_cmp(&b.value))
                .map(|b| b.buyer);
            if let Some(buyer) = taker {
                if let Owner::Hh(id) = seller {
                    let h = &mut w.hh[id as usize];
                    h.declare_markup = (h.declare_markup * 1.08).min(1.6);
                }
                execute(w, i, buyer, declared);
                w.flows.harberger_buyouts += 1;
                w.push_event(Event::HarbergerBuyout { cell: i as u32 });
                continue;
            }
        }
        let best = bids.into_iter().filter(|b| b.buyer != seller).max_by(|a, b| a.value.total_cmp(&b.value));
        let Some(b) = best else { continue };
        if b.value > reservation * (1.0 + t_rate) {
            let price = 0.5 * (reservation + b.value / (1.0 + t_rate));
            if can_afford(w, b.buyer, price, price * t_rate) {
                execute(w, i, b.buyer, price);
                w.push_event(Event::Sale { cell: i as u32, forced: false });
            }
        } else if b.value > reservation {
            // Would trade in a world without transfer tax.
            w.flows.blocked_sales += 1;
        }
    }

    for v in w.p.months_since_sale.iter_mut() {
        *v = v.saturating_add(1);
    }
}

/// A household owner who would build but cannot finance it sells to outside
/// capital at the developer's valuation, if that beats keeping the parcel.
pub fn sell_to_developer(w: &mut World, i: usize) -> bool {
    let seller = w.p.owner[i];
    let inv = w.absentee_investor();
    let bid = econ::valuation(w, i, &inv);
    let keep = econ::current_use_value(w, i, &w.investor_for(seller));
    let price = bid / (1.0 + w.taxes.transfer_cost(1.0));
    if price <= keep || price <= 0.0 {
        return false;
    }
    execute(w, i, Owner::Absentee, price);
    w.push_event(Event::Sale { cell: i as u32, forced: false });
    true
}

/// A departing household sells a parcel to outside capital at its
/// valuation (at least the assessed value).
pub fn sell_on_departure(w: &mut World, i: usize) {
    let inv = w.absentee_investor();
    let price = (econ::valuation(w, i, &inv) / (1.0 + w.taxes.transfer_cost(1.0))).max(w.parcel_value(i) * 0.9);
    execute(w, i, Owner::Absentee, price.max(1.0));
}

/// Government purchase (parks, roads, civic buildings) at the owner's
/// valuation, or the assessed value if higher, plus the purchase premium.
pub fn compulsory_purchase(w: &mut World, i: usize) -> f64 {
    let owner = w.p.owner[i];
    if owner == Owner::Gov {
        return 0.0;
    }
    let inv = w.investor_for(owner);
    let premium = 1.0 + w.cfg.policy.spending.purchase_premium as f64;
    let price = premium * econ::valuation(w, i, &inv).max(w.parcel_value(i));
    execute(w, i, Owner::Gov, price);
    w.gov.spend_compensation += price;
    price
}
