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
    /// Growth per year of every money amount (wages, costs, the outside
    /// option). It scales all of them together, so it changes only the units:
    /// it works like inflation. Zero by default, so money at year 300 means
    /// what it meant at year 0; interest rates are real.
    pub outside_growth: f32,
    /// Effective tax rate on income elsewhere (for migration comparisons).
    pub outside_income_tax: f32,
    /// Annual real return demanded by outside capital.
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
    /// Commute cost per cell (~40 m) per month (time + money), in an
    /// uncongested town.
    pub commute_cost_per_cell: f32,
    /// Congestion: commuting costs scale with `1 + (households / congestion_scale)^congestion_elasticity`.
    pub congestion_scale: f32,
    pub congestion_elasticity: f32,
    /// Logit scale for location choice (money units).
    pub location_logit_scale: f32,
    pub search_samples: usize,
    /// Probability per month a housed household reconsiders its home.
    pub relocation_rate: f32,
    pub deposit_return: f32,
    pub mean_lifetime_years: f32,

    // --- firms
    pub industry_price: f32,
    /// Price elasticity of outside demand for the city's exports. The city
    /// is not a price taker once it is large: the export price falls as
    /// `(exports / export_market_size)^(-1/elasticity)` above that size.
    pub export_demand_elasticity: f32,
    /// Monthly real export volume the outside market absorbs at the base price.
    pub export_market_size: f32,
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
    /// Compensation owed to a sitting household when its building is
    /// cleared, in months of its rent (statutory relocation allowance).
    /// Business tenants get twice this as a lease buy-out.
    pub relocation_months: f32,
    /// Monthly agricultural rent per farm parcel (the rural land-use floor).
    pub farm_rent: f32,
    /// Monthly step by which business landlords move rents.
    pub rent_adjust: f32,
    /// Monthly step by which residential landlords move asking rents at full
    /// market tightness. Tenants react to rents only at lease renewal and
    /// leave after months of discontent, so faster adjustment overshoots
    /// into a boom/bust rent cycle.
    pub housing_rent_adjust: f32,
    /// Monthly rent rise in a full building per unit of housing pressure
    /// (households queueing for a home, as a share of all households).
    pub housing_queue_response: f32,
    pub loan_to_value: f32,
    pub mortgage_rate: f32,
    /// Months of rent that an owner extrapolates price trends over.
    pub expectation_memory: f32,
    /// Weight of extrapolated real rent growth in valuations (speculation).
    /// Land is the residual of property value over building cost, so this
    /// weight sets how strongly land values amplify the rent cycle.
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
            outside_growth: 0.0,
            outside_income_tax: 0.25,
            outside_return: 0.045,
            immigration_rate: 0.006,
            moving_cost_months: 2.0,

            skill_sigma: 0.45,
            base_wage: 3000.0,
            marginal_propensity_consume: 0.75,
            wealth_consumption: 0.003,
            subsistence: 700.0,
            local_spend_share: 0.55,
            commute_cost_per_cell: 9.0,
            congestion_scale: 20_000.0,
            congestion_elasticity: 0.6,
            location_logit_scale: 120.0,
            search_samples: 10,
            relocation_rate: 0.03,
            deposit_return: 0.015,
            mean_lifetime_years: 45.0,

            industry_price: 1.0,
            export_demand_elasticity: 3.0,
            export_market_size: 60_000_000.0,
            industry_output_per_worker: 5600.0,
            retail_output_per_worker: 9000.0,
            labour_elasticity: 0.85,
            agglomeration: 0.06,
            pollution_per_worker: 1.0,
            abatement_cost: 900.0,
            firm_entry_rate: 0.08,

            res_units_per_floor: 4,
            com_jobs_per_floor: 18,
            ind_jobs_per_floor: 14,
            max_floors: 30,
            cost_per_floor: 450_000.0,
            height_premium: 0.18,
            construction_months: 6,
            maintenance_rate: 0.0012,
            depreciation: 0.002,
            demolition_cost: 60_000.0,
            relocation_months: 6.0,
            farm_rent: 15.0,
            rent_adjust: 0.015,
            housing_rent_adjust: 0.005,
            housing_queue_response: 0.3,
            loan_to_value: 0.7,
            mortgage_rate: 0.045,
            expectation_memory: 24.0,
            speculation: 0.1,
            review_rate: 0.04,
            assessment_noise: 0.08,

            nature_regrowth: 0.004,
            pollution_decay_cells: 3.0,
            amenity_decay_cells: 3.0,
            access_decay_cells: 9.0,
        }
    }
}

impl EconParams {
    /// The parameters in money of the day: every money-denominated
    /// quantity scaled by `index`, the level of outside productivity
    /// (`(1 + outside_growth)^year`). Keeps relative prices (wages vs.
    /// construction costs, rents vs. amenities) stable over long runs.
    pub fn indexed(&self, index: f32) -> EconParams {
        let mut e = self.clone();
        e.outside_utility *= index;
        e.base_wage *= index;
        e.subsistence *= index;
        e.commute_cost_per_cell *= index;
        e.location_logit_scale *= index;
        e.industry_output_per_worker *= index;
        e.retail_output_per_worker *= index;
        e.abatement_cost *= index;
        e.cost_per_floor *= index;
        e.demolition_cost *= index;
        e.farm_rent *= index;
        e
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
    /// Premium over market value the government pays when it buys land for
    /// roads, parks and civic buildings (owners are bought out, not merely
    /// compensated).
    pub purchase_premium: f32,
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
            purchase_premium: 0.25,
        }
    }
}
