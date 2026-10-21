//! Land value tax (Georgism): levied on the assessed value of land alone, so
//! it does not depend on what the owner builds. Holding land idle costs the
//! same as using it well.

use super::{TaxModule, TaxSpec};

pub struct LandValueTax {
    pub rate: f32,
}

impl TaxModule for LandValueTax {
    fn spec(&self) -> TaxSpec {
        TaxSpec::LandValue { rate: self.rate }
    }
    fn property(&self, land: f64, _building: f64) -> f64 {
        land * self.rate as f64
    }
}
