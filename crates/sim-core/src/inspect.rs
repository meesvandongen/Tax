//! Human-readable details of a single parcel, for the UI inspector.

use crate::econ;
use crate::property::idle_share;
use crate::types::*;
use crate::world::World;
use serde_json::{json, Value};

pub fn parcel(w: &World, i: usize) -> Value {
    if i >= w.p.len() {
        return Value::Null;
    }
    let p = &w.p;
    let [x, y] = w.mesh.centroid[i];
    let owner = match p.owner[i] {
        Owner::Absentee => json!({"kind": "absentee"}),
        Owner::Gov => json!({"kind": "government"}),
        Owner::Hh(id) => {
            let h = &w.hh[id as usize];
            json!({
                "kind": "resident",
                "id": id,
                "wealth": h.wealth,
                "cash": h.cash,
                "income": h.income_labor + h.income_capital + h.income_transfer,
                "lives_here": h.home == i as u32,
                "generation": h.generation,
            })
        }
    };
    let firm = if p.firm[i] != NONE {
        let f = &w.firms[p.firm[i] as usize];
        json!({
            "sector": format!("{:?}", f.sector),
            "workers": f.workers,
            "desired": f.desired_workers,
            "wage_rate": f.wage_rate,
            "revenue": f.revenue,
            "profit": f.profit,
            "emission_intensity": f.emission_intensity,
            "owner": match f.owner { Owner::Hh(_) => "resident", Owner::Gov => "government", Owner::Absentee => "absentee" },
        })
    } else {
        Value::Null
    };
    let residents: Vec<Value> =
        w.hh.iter()
            .filter(|h| h.alive && h.home == i as u32)
            .take(12)
            .map(|h| {
                json!({
                    "skill": h.skill,
                    "employed": h.job != NONE,
                    "rent": h.rent,
                    "owns_flat": h.owns_home,
                    "income": h.income_labor + h.income_capital + h.income_transfer,
                    "taxes": h.taxes_paid,
                    "wealth": h.wealth,
                    "utility": h.utility,
                })
            })
            .collect();
    let inv = w.investor_for(p.owner[i]);
    let best = econ::best_development(w, i, &inv).map(
        |d| json!({"use": format!("{:?}", d.land_use), "floors": d.floors, "residual": d.residual, "cost": d.cost}),
    );
    json!({
        "cell": i, "x": x, "y": y,
        "land_use": format!("{:?}", p.land_use[i]),
        "target_use": format!("{:?}", p.target_use[i]),
        "floors": p.floors[i],
        "units": p.units[i],
        "occupied": p.occupied[i],
        "owned_flats": p.owned_flats[i],
        "asking_rent": p.asking_rent[i],
        "condition": p.condition[i],
        "age_years": p.age_months[i] as f32 / 12.0,
        "nature": p.nature[i],
        "elevation": w.terrain.height[i],
        "in_city": p.in_city[i],
        "road_access": p.road_access[i],
        "assessed_land": p.assessed_land[i],
        "assessed_building": p.assessed_bldg[i],
        "declared": p.declared[i],
        "last_price": p.price[i],
        "months_since_sale": p.months_since_sale[i],
        "months_held_idle": p.months_held_empty[i],
        "mortgage": p.mortgage[i],
        "tax_last_month": p.tax_last[i],
        "idle_share": idle_share(w, i),
        "owner": owner,
        "firm": firm,
        "residents": residents,
        "best_development": best,
        "fields": {
            "pollution": w.f.pollution[i],
            "amenity": w.f.amenity[i],
            "service": w.f.service[i],
            "commute_eff": w.f.commute_eff[i],
            "rent_res": w.f.rent_res[i],
            "rent_com": w.f.rent_com[i],
            "rent_ind": w.f.rent_ind[i],
        },
    })
}
