//! Transfer tax / stamp duty on property sales, paid by the buyer. Drives a
//! wedge between what buyers pay and sellers receive.

use super::{TaxModule, TaxSpec};

pub struct TransferTax {
    pub rate: f32,
}

impl TaxModule for TransferTax {
    fn spec(&self) -> TaxSpec {
        TaxSpec::Transfer { rate: self.rate }
    }
    fn transfer(&self, price: f64) -> f64 {
        price.max(0.0) * self.rate as f64
    }
}
