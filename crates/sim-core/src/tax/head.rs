//! Lump-sum (poll) tax: identical for every household. Non-distorting at
//! the margin but highly regressive.

use super::{TaxModule, TaxSpec};

pub struct HeadTax {
    pub amount: f32,
    /// Price level the money amounts are indexed to.
    pub index: f64,
}

impl TaxModule for HeadTax {
    fn set_index(&mut self, index: f64) {
        self.index = index;
    }
    fn spec(&self) -> TaxSpec {
        TaxSpec::Head { amount: self.amount }
    }
    fn head(&self) -> f64 {
        self.amount as f64 * self.index
    }
}
