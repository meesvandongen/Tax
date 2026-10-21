//! Conventional (or split-rate) property tax on land and buildings. With
//! `building_rate > 0` every extra floor raises the bill, which developers
//! anticipate.

use super::{TaxModule, TaxSpec};

pub struct PropertyTax {
    pub land_rate: f32,
    pub building_rate: f32,
}

impl TaxModule for PropertyTax {
    fn spec(&self) -> TaxSpec {
        TaxSpec::Property { land_rate: self.land_rate, building_rate: self.building_rate }
    }
    fn property(&self, land: f64, building: f64) -> f64 {
        land * self.land_rate as f64 + building * self.building_rate as f64
    }
}
