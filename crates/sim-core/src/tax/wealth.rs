//! Annual net-worth tax above an exemption. Must be paid in cash, so
//! asset-rich, cash-poor households may be forced to sell.

use super::{TaxModule, TaxSpec};

pub struct WealthTax {
    pub threshold: f32,
    pub rate: f32,
}

impl TaxModule for WealthTax {
    fn spec(&self) -> TaxSpec {
        TaxSpec::Wealth { threshold: self.threshold, rate: self.rate }
    }
    fn wealth(&self, net_worth: f64) -> f64 {
        (net_worth - self.threshold as f64).max(0.0) * self.rate as f64
    }
}
