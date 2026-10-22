//! Annual net-worth tax above an exemption. Must be paid in cash, so
//! asset-rich, cash-poor households may be forced to sell.

use super::{TaxModule, TaxSpec};

pub struct WealthTax {
    pub threshold: f32,
    pub rate: f32,
    /// Price level the money amounts are indexed to.
    pub index: f64,
}

impl TaxModule for WealthTax {
    fn set_index(&mut self, index: f64) {
        self.index = index;
    }
    fn spec(&self) -> TaxSpec {
        TaxSpec::Wealth { threshold: self.threshold, rate: self.rate }
    }
    fn wealth(&self, net_worth: f64) -> f64 {
        (net_worth - self.threshold as f64 * self.index).max(0.0) * self.rate as f64
    }
}
