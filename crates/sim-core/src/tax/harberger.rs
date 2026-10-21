//! Harberger tax / common-ownership self-assessed tax (COST). Owners declare
//! a value and pay `rate` on it; anyone may buy at the declared price. The
//! purchase rule lives in the land market; this module only sets the rate.

use super::{TaxModule, TaxSpec};

pub struct HarbergerTax {
    pub rate: f32,
}

impl TaxModule for HarbergerTax {
    fn spec(&self) -> TaxSpec {
        TaxSpec::Harberger { rate: self.rate }
    }
    fn harberger_rate(&self) -> f64 {
        self.rate as f64
    }
}
