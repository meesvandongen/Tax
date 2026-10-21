//! Pluggable tax system.
//!
//! A tax is a [`TaxModule`]: a set of pure functions from *tax bases* to
//! amounts. The simulation computes the bases (income, wealth, land value,
//! consumption, …) for each agent and asks the [`TaxSystem`] what is owed.
//!
//! Agents use exactly the same functions when they *anticipate* taxes in
//! their decisions (work or not, where to live, how tall to build, whether to
//! hold land vacant, whether to leave the city). That is the only channel by
//! which taxes influence behaviour — no outcome is scripted.
//!
//! Adding a tax: create a file implementing `TaxModule`, add a variant to
//! [`TaxSpec`] and a line in [`TaxSpec::build`].

mod corporate;
mod estate;
mod harberger;
mod head;
mod income;
mod land_value;
mod payroll;
mod pollution;
mod property;
mod sales;
mod transfer;
mod vacancy;
mod wealth;

use serde::{Deserialize, Serialize};

pub use income::Bracket;

/// Serializable description of one tax instrument.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaxSpec {
    /// Annual rate on assessed *land* value only (Georgist single tax).
    LandValue { rate: f32 },
    /// Annual rates on assessed land and building value (split-rate allowed).
    Property { land_rate: f32, building_rate: f32 },
    /// Progressive schedule on annual income. Thresholds are annual.
    Income { brackets: Vec<Bracket>, include_capital: bool },
    /// Annual rate on net worth above an exemption threshold.
    Wealth { threshold: f32, rate: f32 },
    /// Ad-valorem tax on consumer purchases.
    Sales { rate: f32 },
    /// Tax on firm profits.
    Corporate { rate: f32 },
    /// Employer-side payroll tax on wages.
    Payroll { rate: f32 },
    /// Stamp duty paid by the buyer on property transactions.
    Transfer { rate: f32 },
    /// Self-assessed (COST / Harberger) tax: annual rate on the declared
    /// value; anyone may buy the parcel at the declared price.
    Harberger { rate: f32 },
    /// Pigouvian tax per unit of emissions.
    Pollution { per_unit: f32 },
    /// Annual rate on the value of empty dwellings / idle lots.
    Vacancy { rate: f32 },
    /// Tax on estates above a threshold at death.
    Estate { threshold: f32, rate: f32 },
    /// Lump-sum tax per household per month.
    Head { amount: f32 },
}

impl TaxSpec {
    pub fn build(&self) -> Box<dyn TaxModule> {
        match self.clone() {
            TaxSpec::LandValue { rate } => Box::new(land_value::LandValueTax { rate }),
            TaxSpec::Property { land_rate, building_rate } => {
                Box::new(property::PropertyTax { land_rate, building_rate })
            }
            TaxSpec::Income { brackets, include_capital } => {
                Box::new(income::IncomeTax::new(brackets, include_capital))
            }
            TaxSpec::Wealth { threshold, rate } => Box::new(wealth::WealthTax { threshold, rate }),
            TaxSpec::Sales { rate } => Box::new(sales::SalesTax { rate }),
            TaxSpec::Corporate { rate } => Box::new(corporate::CorporateTax { rate }),
            TaxSpec::Payroll { rate } => Box::new(payroll::PayrollTax { rate }),
            TaxSpec::Transfer { rate } => Box::new(transfer::TransferTax { rate }),
            TaxSpec::Harberger { rate } => Box::new(harberger::HarbergerTax { rate }),
            TaxSpec::Pollution { per_unit } => Box::new(pollution::PollutionTax { per_unit }),
            TaxSpec::Vacancy { rate } => Box::new(vacancy::VacancyTax { rate }),
            TaxSpec::Estate { threshold, rate } => Box::new(estate::EstateTax { threshold, rate }),
            TaxSpec::Head { amount } => Box::new(head::HeadTax { amount }),
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            TaxSpec::LandValue { .. } => "land_value",
            TaxSpec::Property { .. } => "property",
            TaxSpec::Income { .. } => "income",
            TaxSpec::Wealth { .. } => "wealth",
            TaxSpec::Sales { .. } => "sales",
            TaxSpec::Corporate { .. } => "corporate",
            TaxSpec::Payroll { .. } => "payroll",
            TaxSpec::Transfer { .. } => "transfer",
            TaxSpec::Harberger { .. } => "harberger",
            TaxSpec::Pollution { .. } => "pollution",
            TaxSpec::Vacancy { .. } => "vacancy",
            TaxSpec::Estate { .. } => "estate",
            TaxSpec::Head { .. } => "head",
        }
    }
}

/// One tax instrument. All methods default to "not taxed".
/// Amounts are *annual* unless stated otherwise.
pub trait TaxModule: Send + Sync {
    fn spec(&self) -> TaxSpec;
    /// Annual tax on annual labour and capital income.
    fn income(&self, _labor: f64, _capital: f64) -> f64 {
        0.0
    }
    /// Annual tax on net worth.
    fn wealth(&self, _net_worth: f64) -> f64 {
        0.0
    }
    /// Annual tax on an (assessed) parcel.
    fn property(&self, _land: f64, _building: f64) -> f64 {
        0.0
    }
    /// Annual rate on a self-assessed value.
    fn harberger_rate(&self) -> f64 {
        0.0
    }
    /// Annual tax on idle property: `idle_share` of the value stands empty.
    fn vacancy(&self, _value: f64, _idle_share: f64) -> f64 {
        0.0
    }
    /// Ad-valorem rate added to consumer prices.
    fn sales_rate(&self) -> f64 {
        0.0
    }
    /// Employer payroll rate on gross wages.
    fn payroll_rate(&self) -> f64 {
        0.0
    }
    /// Annual tax on annual profit.
    fn profit(&self, _profit: f64) -> f64 {
        0.0
    }
    /// One-off tax on a sale at `price`, paid by the buyer.
    fn transfer(&self, _price: f64) -> f64 {
        0.0
    }
    /// Monthly tax on monthly emission units.
    fn emission(&self, _units: f64) -> f64 {
        0.0
    }
    /// One-off tax on an estate at death.
    fn estate(&self, _estate: f64) -> f64 {
        0.0
    }
    /// Monthly lump-sum per household.
    fn head(&self) -> f64 {
        0.0
    }
}

/// The active set of taxes plus a per-instrument revenue ledger.
pub struct TaxSystem {
    modules: Vec<Box<dyn TaxModule>>,
    specs: Vec<TaxSpec>,
    /// Revenue collected this month, by module index.
    pub month: Vec<f64>,
    harberger_rate: f64,
    sales_rate: f64,
    payroll_rate: f64,
}

impl TaxSystem {
    pub fn new(specs: &[TaxSpec]) -> Self {
        let modules: Vec<Box<dyn TaxModule>> = specs.iter().map(|s| s.build()).collect();
        let harberger_rate = modules.iter().map(|m| m.harberger_rate()).sum();
        let sales_rate = modules.iter().map(|m| m.sales_rate()).sum();
        let payroll_rate = modules.iter().map(|m| m.payroll_rate()).sum();
        TaxSystem {
            month: vec![0.0; modules.len()],
            modules,
            specs: specs.to_vec(),
            harberger_rate,
            sales_rate,
            payroll_rate,
        }
    }

    pub fn specs(&self) -> &[TaxSpec] {
        &self.specs
    }

    pub fn reset_month(&mut self) {
        self.month.iter_mut().for_each(|v| *v = 0.0);
    }

    /// Total of `f` over modules, without recording (for anticipation).
    #[inline]
    pub fn estimate(&self, f: impl Fn(&dyn TaxModule) -> f64) -> f64 {
        self.modules.iter().map(|m| f(m.as_ref())).sum()
    }

    /// Total of `f` over modules, recorded in the ledger (actual levy).
    #[inline]
    pub fn levy(&mut self, f: impl Fn(&dyn TaxModule) -> f64) -> f64 {
        let mut total = 0.0;
        for (k, m) in self.modules.iter().enumerate() {
            let v = f(m.as_ref());
            if v != 0.0 {
                self.month[k] += v;
                total += v;
            }
        }
        total
    }

    /// Record an amount against every module of a given kind, split by
    /// `share`, used where the levy is computed in aggregate.
    pub fn record(&mut self, k: usize, amount: f64) {
        self.month[k] += amount;
    }

    pub fn harberger_rate(&self) -> f64 {
        self.harberger_rate
    }
    pub fn sales_rate(&self) -> f64 {
        self.sales_rate
    }
    pub fn payroll_rate(&self) -> f64 {
        self.payroll_rate
    }

    // ---- convenience wrappers (monthly amounts) ----

    /// Monthly income tax given monthly incomes.
    pub fn income_monthly(&self, labor: f64, capital: f64) -> f64 {
        self.estimate(|m| m.income(labor * 12.0, capital * 12.0)) / 12.0
    }
    /// Monthly property-type taxes for a parcel (ad-valorem + Harberger + vacancy).
    pub fn property_monthly(&self, land: f64, building: f64, declared: f64, idle_share: f64) -> f64 {
        let v = self.estimate(|m| {
            m.property(land, building) + m.vacancy(land + building, idle_share)
        }) + self.harberger_rate * declared;
        v / 12.0
    }
    /// Effective annual rate on the holding value of property (for capitalization).
    pub fn holding_rate(&self, land_share: f64) -> f64 {
        let land = land_share;
        let bldg = 1.0 - land_share;
        self.estimate(|m| m.property(land, bldg)) + self.harberger_rate
    }
    pub fn wealth_monthly(&self, net_worth: f64) -> f64 {
        self.estimate(|m| m.wealth(net_worth)) / 12.0
    }
    /// Marginal annual wealth-tax rate at a given net worth.
    pub fn wealth_marginal(&self, net_worth: f64) -> f64 {
        let d = 10_000.0;
        (self.estimate(|m| m.wealth(net_worth + d)) - self.estimate(|m| m.wealth(net_worth))) / d
    }
    pub fn transfer_cost(&self, price: f64) -> f64 {
        self.estimate(|m| m.transfer(price))
    }
    pub fn profit_monthly(&self, profit: f64) -> f64 {
        self.estimate(|m| m.profit(profit * 12.0)) / 12.0
    }
    pub fn head_monthly(&self) -> f64 {
        self.estimate(|m| m.head())
    }
}

/// Apply a progressive bracket schedule. Brackets are (threshold, rate) with
/// the rate applying to the part of the base above the threshold.
pub fn progressive(base: f64, brackets: &[Bracket]) -> f64 {
    let mut tax = 0.0;
    for (k, b) in brackets.iter().enumerate() {
        let lo = b.threshold as f64;
        if base <= lo {
            break;
        }
        let hi = brackets.get(k + 1).map(|n| n.threshold as f64).unwrap_or(f64::INFINITY);
        tax += (base.min(hi) - lo) * b.rate as f64;
    }
    tax
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progressive_schedule() {
        let b = vec![
            Bracket { threshold: 0.0, rate: 0.0 },
            Bracket { threshold: 10_000.0, rate: 0.2 },
            Bracket { threshold: 50_000.0, rate: 0.5 },
        ];
        assert_eq!(progressive(5_000.0, &b), 0.0);
        assert!((progressive(20_000.0, &b) - 2_000.0).abs() < 1e-9);
        assert!((progressive(60_000.0, &b) - (8_000.0 + 5_000.0)).abs() < 1e-9);
    }

    #[test]
    fn ledger_records_per_module() {
        let mut ts = TaxSystem::new(&[
            TaxSpec::LandValue { rate: 0.05 },
            TaxSpec::Property { land_rate: 0.01, building_rate: 0.01 },
        ]);
        let t = ts.levy(|m| m.property(100.0, 100.0));
        assert!((t - (5.0 + 2.0)).abs() < 1e-9);
        assert!((ts.month[0] - 5.0).abs() < 1e-9);
        assert!((ts.month[1] - 2.0).abs() < 1e-9);
    }

    #[test]
    fn spec_roundtrip_json() {
        let s = TaxSpec::Wealth { threshold: 1e6, rate: 0.02 };
        let j = serde_json::to_string(&s).unwrap();
        assert!(j.contains("\"kind\":\"wealth\""));
        let back: TaxSpec = serde_json::from_str(&j).unwrap();
        assert_eq!(back, s);
    }
}
