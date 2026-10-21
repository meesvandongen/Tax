//! Vacancy tax on the idle part of a property's value (empty dwellings,
//! unused lots inside the urban area).

use super::{TaxModule, TaxSpec};

pub struct VacancyTax {
    pub rate: f32,
}

impl TaxModule for VacancyTax {
    fn spec(&self) -> TaxSpec {
        TaxSpec::Vacancy { rate: self.rate }
    }
    fn vacancy(&self, value: f64, idle_share: f64) -> f64 {
        value * idle_share.clamp(0.0, 1.0) * self.rate as f64
    }
}
