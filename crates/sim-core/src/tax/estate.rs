//! Estate (inheritance) tax, levied when a household's head dies and the
//! estate passes to an heir.

use super::{TaxModule, TaxSpec};

pub struct EstateTax {
    pub threshold: f32,
    pub rate: f32,
    /// Price level the money amounts are indexed to.
    pub index: f64,
}

impl TaxModule for EstateTax {
    fn set_index(&mut self, index: f64) {
        self.index = index;
    }
    fn spec(&self) -> TaxSpec {
        TaxSpec::Estate { threshold: self.threshold, rate: self.rate }
    }
    fn estate(&self, estate: f64) -> f64 {
        (estate - self.threshold as f64 * self.index).max(0.0) * self.rate as f64
    }
}
