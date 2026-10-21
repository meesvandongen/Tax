//! Employer-side payroll tax: raises the cost of each worker.

use super::{TaxModule, TaxSpec};

pub struct PayrollTax {
    pub rate: f32,
}

impl TaxModule for PayrollTax {
    fn spec(&self) -> TaxSpec {
        TaxSpec::Payroll { rate: self.rate }
    }
    fn payroll_rate(&self) -> f64 {
        self.rate as f64
    }
}
