//! Pigouvian tax per unit of emissions. Firms trade off abatement cost
//! against the tax when choosing emission intensity.

use super::{TaxModule, TaxSpec};

pub struct PollutionTax {
    pub per_unit: f32,
    /// Price level the money amounts are indexed to.
    pub index: f64,
}

impl TaxModule for PollutionTax {
    fn set_index(&mut self, index: f64) {
        self.index = index;
    }
    fn spec(&self) -> TaxSpec {
        TaxSpec::Pollution { per_unit: self.per_unit }
    }
    fn emission(&self, units: f64) -> f64 {
        units.max(0.0) * self.per_unit as f64 * self.index
    }
}
