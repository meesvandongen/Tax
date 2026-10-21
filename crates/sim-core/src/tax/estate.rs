//! Estate (inheritance) tax, levied when a household's head dies and the
//! estate passes to an heir.

use super::{TaxModule, TaxSpec};

pub struct EstateTax {
    pub threshold: f32,
    pub rate: f32,
}

impl TaxModule for EstateTax {
    fn spec(&self) -> TaxSpec {
        TaxSpec::Estate { threshold: self.threshold, rate: self.rate }
    }
    fn estate(&self, estate: f64) -> f64 {
        (estate - self.threshold as f64).max(0.0) * self.rate as f64
    }
}
