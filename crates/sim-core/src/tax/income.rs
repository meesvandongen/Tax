//! Personal income tax with a progressive bracket schedule. Labour income is
//! always taxed; capital income (rents, dividends, interest) optionally.

use super::{progressive, TaxModule, TaxSpec};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Bracket {
    /// Annual income above which `rate` applies.
    pub threshold: f32,
    pub rate: f32,
}

pub struct IncomeTax {
    brackets: Vec<Bracket>,
    include_capital: bool,
}

impl IncomeTax {
    pub fn new(mut brackets: Vec<Bracket>, include_capital: bool) -> Self {
        brackets.sort_by(|a, b| a.threshold.total_cmp(&b.threshold));
        IncomeTax { brackets, include_capital }
    }
}

impl TaxModule for IncomeTax {
    fn spec(&self) -> TaxSpec {
        TaxSpec::Income { brackets: self.brackets.clone(), include_capital: self.include_capital }
    }
    fn income(&self, labor: f64, capital: f64) -> f64 {
        let base = labor + if self.include_capital { capital.max(0.0) } else { 0.0 };
        progressive(base, &self.brackets)
    }
}
