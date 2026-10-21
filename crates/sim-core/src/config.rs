//! Scenario configuration. Everything here is plain serde data so scenarios
//! can be shared as JSON between the browser UI, the native CLI and tests.

use crate::tax::TaxSpec;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct SimConfig {
    pub seed: u64,
    pub width: usize,
    pub height: usize,
    pub initial_households: u32,
    pub econ: EconParams,
    pub policy: Policy,
}

impl Default for SimConfig {
    fn default() -> Self {
        SimConfig {
            seed: 1,
            width: 128,
            height: 128,
            initial_households: 400,
            econ: EconParams::default(),
            policy: Policy::default(),
        }
    }
}

/// Behavioural and technological parameters. These are *not* policy levers;
/// they describe the world the policies act upon.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct EconParams {
    // --- outside world (open-city closure)
    /// Monthly net income (after rent and taxes) a skill-1 household can get elsewhere.
    pub outside_utility: f32,
    /// Real growth of outside opportunities per year.
    pub outside_growth: f32,
    /// Effective tax rate on income elsewhere (for migration comparisons).
    pub outside_income_tax: f32,
    /// Annual return demanded by outside capital.
    pub outside_return: f32,
    /// Max immigrants per month as a fraction of population (plus a floor).
    pub immigration_rate: f32,
    /// Fixed moving cost, in months of income.
    pub moving_cost_months: f32,

    // --- households
    pub skill_sigma: f32,
    pub base_wage: f32,
    pub marginal_propensity_consume: f32,
    pub wealth_consumption: f32,
    pub subsistence: f32,
    /// Share of consumption spent at local shops (rest imported).
    pub local_spend_share: f32,
    /// Commute cost per cell (~40 m) per month (time + money).
    pub commute_cost_per_cell: f32,
    /// Logit scale for location choice (money units).
    pub location_logit_scale: f32,
    pub search_samples: usize,
    /// Probability per month a housed household reconsiders its home.
    pub relocation_rate: f32,
    pub deposit_return: f32,
    pub mean_lifetime_years: f32,

    // --- firms
    pub industry_price: f32,
    pub industry_output_per_worker: f32,
    pub retail_output_per_worker: f32,
    pub labour_elasticity: f32,
    pub agglomeration: f32,
    pub pollution_per_worker: f32,
    pub abatement_cost: f32,
    pub firm_entry_rate: f32,

    // --- real estate
    pub res_units_per_floor: u16,
    pub com_jobs_per_floor: u16,
    pub ind_jobs_per_floor: u16,
    pub max_floors: u8,
    /// Construction cost per floor (residential), before height premium.
    pub cost_per_floor: f32,
    /// Extra cost per floor for going tall: cost ∝ floors^(1+height_premium).
    pub height_premium: f32,
    pub construction_months: u16,
    /// Monthly maintenance as a fraction of building value.
    pub maintenance_rate: f32,
    /// Monthly depreciation of condition when not maintained.
    pub depreciation: f32,
    pub demolition_cost: f32,
    /// Monthly agricultural rent per farm parcel (the rural land-use floor).
    pub farm_rent: f32,
    pub rent_adjust: f32,
    pub loan_to_value: f32,
    pub mortgage_rate: f32,
    /// Months of rent that an owner extrapolates price trends over.
    pub expectation_memory: f32,
    /// Weight of extrapolated price growth in valuations (speculation).
    pub speculation: f32,
    /// Monthly probability an owner reviews a parcel for development or sale.
    pub review_rate: f32,
    /// Relative noise in government assessments.
    pub assessment_noise: f32,

    // --- environment
    pub nature_regrowth: f32,
    pub pollution_decay_cells: f32,
    pub amenity_decay_cells: f32,
    pub access_decay_cells: f32,
}

impl Default for EconParams {
    fn default() -> Self {
        EconParams {
            outside_utility: 1300.0,
            outside_growth: 0.005,
            outside_income_tax: 0.25,
            outside_return: 0.05,
            immigration_rate: 0.015,
            moving_cost_months: 2.0,

            skill_sigma: 0.45,
            base_wage: 3000.0,
            marginal_propensity_consume: 0.75,
            wealth_consumption: 0.003,
            subsistence: 700.0,
            local_spend_share: 0.55,
            commute_cost_per_cell: 9.0,
            location_logit_scale: 120.0,
            search_samples: 10,
            relocation_rate: 0.03,
            deposit_return: 0.02,
            mean_lifetime_years: 45.0,

            industry_price: 1.0,
            industry_output_per_worker: 6200.0,
            retail_output_per_worker: 5200.0,
            labour_elasticity: 0.85,
            agglomeration: 0.06,
            pollution_per_worker: 1.0,
            abatement_cost: 900.0,
            firm_entry_rate: 0.08,

            res_units_per_floor: 4,
            com_jobs_per_floor: 18,
            ind_jobs_per_floor: 14,
            max_floors: 30,
            cost_per_floor: 520_000.0,
            height_premium: 0.18,
            construction_months: 6,
            maintenance_rate: 0.0012,
            depreciation: 0.002,
            demolition_cost: 60_000.0,
            farm_rent: 15.0,
            rent_adjust: 0.03,
            loan_to_value: 0.7,
            mortgage_rate: 0.05,
            expectation_memory: 24.0,
            speculation: 0.25,
            review_rate: 0.04,
            assessment_noise: 0.08,

            nature_regrowth: 0.004,
            pollution_decay_cells: 3.0,
            amenity_decay_cells: 3.0,
            access_decay_cells: 9.0,
        }
    }
}

/// The policy levers. Can be changed while the simulation runs.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Policy {
    pub taxes: Vec<TaxSpec>,
    pub spending: SpendingPolicy,
    /// Separate land uses (no housing next to industry etc.).
    pub zoning: bool,
    /// Optional cap on building height (floors), 0 = none.
    pub height_limit: u8,
}

impl Default for Policy {
    fn default() -> Self {
        crate::presets::preset("status_quo").unwrap_or(Policy {
            taxes: vec![],
            spending: SpendingPolicy::default(),
            zoning: false,
            height_limit: 0,
        })
    }
}

/// How the government uses revenue. Shares are normalised.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SpendingPolicy {
    /// Public services (schools, clinics): public jobs + amenity.
    pub services: f32,
    /// Roads and their maintenance.
    pub infrastructure: f32,
    /// Buying land for parks.
    pub parks: f32,
    /// Equal per-household dividend (UBI / citizen's dividend).
    pub dividend: f32,
    /// Means-tested support for poor and unhoused households.
    pub welfare: f32,
    /// Fraction of revenue that may be borrowed beyond income (deficit).
    pub max_debt_to_revenue: f32,
}

impl Default for SpendingPolicy {
    fn default() -> Self {
        SpendingPolicy {
            services: 0.4,
            infrastructure: 0.3,
            parks: 0.1,
            dividend: 0.0,
            welfare: 0.2,
            max_debt_to_revenue: 1.0,
        }
    }
}
