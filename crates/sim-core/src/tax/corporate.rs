//! Corporate profit tax. Losses are not refunded.

use super::{TaxModule, TaxSpec};

pub struct CorporateTax {
    pub rate: f32,
}

impl TaxModule for CorporateTax {
    fn spec(&self) -> TaxSpec {
        TaxSpec::Corporate { rate: self.rate }
    }
    fn profit(&self, profit: f64) -> f64 {
        profit.max(0.0) * self.rate as f64
    }
}
