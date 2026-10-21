//! Sales tax / VAT on local consumer purchases.

use super::{TaxModule, TaxSpec};

pub struct SalesTax {
    pub rate: f32,
}

impl TaxModule for SalesTax {
    fn spec(&self) -> TaxSpec {
        TaxSpec::Sales { rate: self.rate }
    }
    fn sales_rate(&self) -> f64 {
        self.rate as f64
    }
}
