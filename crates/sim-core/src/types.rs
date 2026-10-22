//! Core entity types: parcels (structure-of-arrays), households and firms.

use serde::{Deserialize, Serialize};

pub const NONE: u32 = u32::MAX;

/// What occupies a grid cell. `repr(u8)` so the renderer can read it raw.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LandUse {
    Water = 0,
    Wild = 1,
    Farm = 2,
    /// Cleared land without a building: speculative holdings, demolished lots.
    Vacant = 3,
    Road = 4,
    Residential = 5,
    Commercial = 6,
    Industrial = 7,
    Park = 8,
    Civic = 9,
    Construction = 10,
}

impl LandUse {
    pub fn is_building(self) -> bool {
        matches!(self, LandUse::Residential | LandUse::Commercial | LandUse::Industrial | LandUse::Civic)
    }
    pub fn is_developable(self) -> bool {
        matches!(
            self,
            LandUse::Wild
                | LandUse::Farm
                | LandUse::Vacant
                | LandUse::Residential
                | LandUse::Commercial
                | LandUse::Industrial
        )
    }
    pub fn is_private_use(self) -> bool {
        matches!(self, LandUse::Residential | LandUse::Commercial | LandUse::Industrial)
    }
    pub fn is_urban(self) -> bool {
        !matches!(self, LandUse::Water | LandUse::Wild | LandUse::Farm)
    }
}

/// Who holds title to a parcel or a firm.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Owner {
    /// Outside capital: rural landholders and absentee investors. Has deep
    /// pockets, requires a market return, and its income leaves the city.
    Absentee,
    Gov,
    Hh(u32),
}

impl Owner {
    pub fn code(self) -> u32 {
        match self {
            Owner::Absentee => NONE,
            Owner::Gov => NONE - 1,
            Owner::Hh(id) => id,
        }
    }
}

/// Structure-of-arrays parcel store; index = grid cell.
#[derive(Clone, Debug, Default)]
pub struct Parcels {
    pub land_use: Vec<LandUse>,
    /// For `Construction`: what is being built.
    pub target_use: Vec<LandUse>,
    pub build_left: Vec<u16>,
    pub floors: Vec<u8>,
    /// Residential units or job slots.
    pub units: Vec<u16>,
    pub occupied: Vec<u16>,
    /// Dwellings owned by the households living in them (flats held on a
    /// long lease or as a condominium unit). The parcel owner keeps the
    /// freehold of the land, the common parts and the other dwellings.
    pub owned_flats: Vec<u16>,
    /// Asking rent per unit / job-slot per month for new leases.
    pub asking_rent: Vec<f32>,
    /// 1 = new, 0 = derelict.
    pub condition: Vec<f32>,
    pub age_months: Vec<u32>,
    pub owner: Vec<Owner>,
    pub firm: Vec<u32>,
    /// Natural vegetation density 0..1 (forest / meadow).
    pub nature: Vec<f32>,
    /// Last observed market price (land + building).
    pub price: Vec<f32>,
    /// Government assessment, used by ad-valorem taxes.
    pub assessed_land: Vec<f32>,
    pub assessed_bldg: Vec<f32>,
    /// Self-assessed value under a Harberger (COST) tax.
    pub declared: Vec<f32>,
    pub months_vacant: Vec<u16>,
    pub months_held_empty: Vec<u16>,
    /// Property-type taxes levied on this parcel last month.
    pub tax_last: Vec<f32>,
    pub road_access: Vec<bool>,
    /// Unhoused households camping here.
    pub tents: Vec<u16>,
    /// Ticks since last sale (for "lock-in" visuals / turnover stats).
    pub months_since_sale: Vec<u32>,
    /// Debt secured against this parcel by its owner.
    pub mortgage: Vec<f32>,
    /// Inside the municipal boundary (within a few cells of urban land);
    /// only these parcels are in the city's tax base.
    pub in_city: Vec<bool>,
    /// Months of unpaid property tax (an outside owner who finds the parcel
    /// worth less than its taxes stops paying; the city then forecloses).
    pub arrears: Vec<u16>,
}

impl Parcels {
    pub fn new(n: usize) -> Self {
        Parcels {
            land_use: vec![LandUse::Wild; n],
            target_use: vec![LandUse::Wild; n],
            build_left: vec![0; n],
            floors: vec![0; n],
            units: vec![0; n],
            occupied: vec![0; n],
            owned_flats: vec![0; n],
            asking_rent: vec![0.0; n],
            condition: vec![1.0; n],
            age_months: vec![0; n],
            owner: vec![Owner::Absentee; n],
            firm: vec![NONE; n],
            nature: vec![0.0; n],
            price: vec![0.0; n],
            assessed_land: vec![0.0; n],
            assessed_bldg: vec![0.0; n],
            declared: vec![0.0; n],
            months_vacant: vec![0; n],
            months_held_empty: vec![0; n],
            tax_last: vec![0.0; n],
            road_access: vec![false; n],
            tents: vec![0; n],
            months_since_sale: vec![0; n],
            mortgage: vec![0.0; n],
            in_city: vec![false; n],
            arrears: vec![0; n],
        }
    }
    pub fn len(&self) -> usize {
        self.land_use.len()
    }
    pub fn is_empty(&self) -> bool {
        self.land_use.is_empty()
    }
    pub fn vacant_units(&self, i: usize) -> u16 {
        self.units[i].saturating_sub(self.occupied[i])
    }
    /// Share of the building the parcel owner holds (the rest are flats
    /// owned by their occupants).
    pub fn freehold_share(&self, i: usize) -> f64 {
        if self.units[i] == 0 {
            1.0
        } else {
            1.0 - self.owned_flats[i] as f64 / self.units[i] as f64
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Household {
    pub alive: bool,
    /// Labour productivity multiplier (log-normal across population).
    pub skill: f32,
    /// Monthly money-equivalent disutility of working.
    pub effort_cost: f32,
    pub cash: f64,
    pub debt: f64,
    pub home: u32,
    /// Owns the flat it lives in (see `Parcels::owned_flats`).
    pub owns_home: bool,
    /// Mortgage on that flat.
    pub flat_mortgage: f64,
    /// Monthly rent on the current lease. For a flat owner, the rent the
    /// flat would let for (what owning it saves).
    pub rent: f32,
    pub job: u32,
    pub wage: f32,
    pub age_months: u32,
    pub months_unhoused: u16,
    pub months_unhappy: u16,
    pub months_unemployed: u16,
    // --- last month's accounts (for decisions, metrics and inspection)
    pub income_labor: f32,
    pub income_capital: f32,
    pub income_transfer: f32,
    pub taxes_paid: f32,
    pub consumption: f32,
    /// Net worth estimate (cash + property + firm equity − debt).
    pub wealth: f64,
    /// The firm-equity part of `wealth`.
    pub business: f64,
    pub utility: f32,
    /// Harberger: multiplier applied to own valuation when self-assessing.
    pub declare_markup: f32,
    /// Generation counter, increments on inheritance.
    pub generation: u16,
}

impl Household {
    pub fn housed(&self) -> bool {
        self.home != NONE
    }
    pub fn employed(&self) -> bool {
        self.job != NONE
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sector {
    /// Local shops and services; revenue from local consumer spending.
    Retail = 0,
    /// Tradable goods sold to the outside world; pollutes.
    Industry = 1,
    /// Government services (schools, clinics); paid from the budget.
    Public = 2,
    /// Tradable services (offices): sold to the outside world, in
    /// commercial buildings, gaining most from clustering.
    Office = 3,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Firm {
    pub alive: bool,
    pub sector: Sector,
    pub parcel: u32,
    pub owner: Owner,
    pub cash: f64,
    /// Total factor productivity (heterogeneous, drifts).
    pub productivity: f32,
    /// Wage per unit of skill per month.
    pub wage_rate: f32,
    pub workers: u16,
    pub skill_sum: f32,
    pub desired_workers: u16,
    pub months_unfilled: u16,
    pub months_loss: u16,
    /// Emission intensity 0..1 (industry chooses abatement under pollution tax).
    pub emission_intensity: f32,
    pub revenue: f32,
    pub profit: f32,
    pub age_months: u32,
}
