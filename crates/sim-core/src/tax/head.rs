//! Lump-sum (poll) tax: identical for every household. Non-distorting at
//! the margin but highly regressive.

use super::{TaxModule, TaxSpec};

pub struct HeadTax {
    pub amount: f32,
}

impl TaxModule for HeadTax {
    fn spec(&self) -> TaxSpec {
        TaxSpec::Head { amount: self.amount }
    }
    fn head(&self) -> f64 {
        self.amount as f64
    }
}
