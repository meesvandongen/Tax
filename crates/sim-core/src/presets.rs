//! Named policy presets. They are starting points; every number can be
//! edited from the UI or in scenario JSON.

use crate::config::{Policy, SpendingPolicy};
use crate::tax::{Bracket, TaxSpec};

pub const PRESETS: &[(&str, &str)] = &[
    ("status_quo", "Mixed system: progressive income tax, property tax, sales tax, corporate tax, stamp duty"),
    ("georgist", "Land value tax replaces most other taxes; part of the land rent returned as a citizen's dividend"),
    ("nordic", "High progressive income tax, high VAT, generous services and welfare"),
    ("wealth_tax", "Status quo plus an annual 2% tax on net worth above 1M"),
    ("harberger", "Self-assessed property tax (COST) with forced-sale rule, funding a dividend"),
    ("flat_tax", "Single flat income tax and a low sales tax"),
    ("laissez_faire", "Minimal state: small property tax for roads only"),
    ("poll_tax", "Equal lump-sum tax per household"),
    ("green", "Pollution tax and land value tax, revenue to parks and dividend"),
];

fn brackets(v: &[(f32, f32)]) -> Vec<Bracket> {
    v.iter().map(|&(threshold, rate)| Bracket { threshold, rate }).collect()
}

pub fn preset(name: &str) -> Option<Policy> {
    let spend = |services, infrastructure, parks, dividend, welfare| SpendingPolicy {
        services,
        infrastructure,
        parks,
        dividend,
        welfare,
        max_debt_to_revenue: 1.0,
        purchase_premium: 0.25,
    };
    let p = match name {
        "status_quo" => Policy {
            taxes: vec![
                TaxSpec::Income {
                    brackets: brackets(&[(0.0, 0.0), (15_000.0, 0.2), (50_000.0, 0.32), (120_000.0, 0.45)]),
                    include_capital: true,
                },
                TaxSpec::Property { land_rate: 0.01, building_rate: 0.01 },
                TaxSpec::Sales { rate: 0.08 },
                TaxSpec::Corporate { rate: 0.21 },
                TaxSpec::Transfer { rate: 0.03 },
            ],
            spending: spend(0.4, 0.3, 0.08, 0.0, 0.22),
            zoning: false,
            height_limit: 0,
            collective_sale_threshold: 0.8,
        },
        "georgist" => Policy {
            taxes: vec![TaxSpec::LandValue { rate: 0.06 }],
            spending: spend(0.32, 0.3, 0.08, 0.25, 0.05),
            zoning: false,
            height_limit: 0,
            collective_sale_threshold: 0.8,
        },
        "nordic" => Policy {
            taxes: vec![
                TaxSpec::Income {
                    brackets: brackets(&[(0.0, 0.0), (10_000.0, 0.3), (60_000.0, 0.45), (100_000.0, 0.55)]),
                    include_capital: true,
                },
                TaxSpec::Sales { rate: 0.25 },
                TaxSpec::Property { land_rate: 0.005, building_rate: 0.005 },
                TaxSpec::Corporate { rate: 0.2 },
            ],
            spending: spend(0.5, 0.2, 0.08, 0.0, 0.22),
            zoning: false,
            height_limit: 0,
            collective_sale_threshold: 0.8,
        },
        "wealth_tax" => {
            let mut p = preset("status_quo")?;
            p.taxes.push(TaxSpec::Wealth { threshold: 1_000_000.0, rate: 0.02 });
            p
        }
        "harberger" => Policy {
            taxes: vec![TaxSpec::Harberger { rate: 0.04 }],
            spending: spend(0.32, 0.3, 0.08, 0.25, 0.05),
            zoning: false,
            height_limit: 0,
            collective_sale_threshold: 0.8,
        },
        "flat_tax" => Policy {
            taxes: vec![
                TaxSpec::Income { brackets: brackets(&[(0.0, 0.22)]), include_capital: true },
                TaxSpec::Sales { rate: 0.05 },
            ],
            spending: spend(0.4, 0.35, 0.05, 0.0, 0.2),
            zoning: false,
            height_limit: 0,
            collective_sale_threshold: 0.8,
        },
        "laissez_faire" => Policy {
            taxes: vec![TaxSpec::Property { land_rate: 0.004, building_rate: 0.004 }],
            spending: spend(0.15, 0.8, 0.05, 0.0, 0.0),
            zoning: false,
            height_limit: 0,
            collective_sale_threshold: 0.8,
        },
        "poll_tax" => Policy {
            taxes: vec![TaxSpec::Head { amount: 450.0 }],
            spending: spend(0.4, 0.35, 0.05, 0.0, 0.2),
            zoning: false,
            height_limit: 0,
            collective_sale_threshold: 0.8,
        },
        "green" => Policy {
            taxes: vec![
                TaxSpec::LandValue { rate: 0.04 },
                TaxSpec::Pollution { per_unit: 600.0 },
                TaxSpec::Income { brackets: brackets(&[(0.0, 0.0), (25_000.0, 0.2)]), include_capital: true },
            ],
            spending: spend(0.32, 0.25, 0.2, 0.15, 0.08),
            zoning: false,
            height_limit: 0,
            collective_sale_threshold: 0.8,
        },
        _ => return None,
    };
    Some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_presets_exist() {
        for (name, _) in PRESETS {
            assert!(preset(name).is_some(), "{name}");
        }
    }
}
