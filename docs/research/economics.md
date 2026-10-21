# Economics research foundation for the tax-regime city simulator

Status: research and design foundation, v1 (October 2026)
Scope: the economic mechanisms, prior agent-based models, a recommended agent model, KPIs, visual mapping and feature cut-off for a browser-based, emergent agent-based city simulator that compares tax regimes.

> **Reading guide.** Section 0 sets the rules that keep outcomes emergent. Section 1 covers each tax: its mechanism, what the literature predicts, and the *minimal agent rule* that lets the effect emerge. Section 2 reviews prior ABMs. Section 3 is the recommended model, with equations and pseudocode. Section 4 lists KPIs, Section 5 the 3D visual mapping, and Section 6 the MVP and phase cut-off. Section 7 covers validation and pitfalls. Section 8 lists the references.
>
> **On the sources.** Empirical magnitudes are given as *calibration and validation targets*, never as rules to hard-code. Where only secondary summaries could be checked, the text says so. Some search hits came from `progress.org/wiki`, whose pages are AI-generated and advocacy-leaning. They were used only to find primary papers and are not relied on for numbers.

---

## 0. Design principles: how to keep outcomes emergent

1. **Taxes are only cash flows and information.** A tax must enter the simulation in only two ways: (a) as a transfer in the ledger, from agent to government, on a defined base at a defined time; (b) as a term that agents include when they compute net present values (NPVs), utilities or profits. No rule may say "if LVT then build higher" or "if wealth tax then x% leave". Higher buildings must come from a developer maximizing NPV with the tax bill in the cash flow. Departures must come from a household comparing its after-tax utility with an outside option.
2. **Prices must be formed by bids, not by formulas.** This is the most important lesson from the one published LVT ABM (Walker et al. 2024, §2.6). In its code, land value is computed by a closed-form function of preferences and density (`world.py: LV = avg_pref * (nbr_occ_housing+1)^1.4 * total_occ_housing^0.9 + LV_MIN`). Taxes therefore *cannot* capitalize into land prices, and capitalization is the central predicted effect of LVT. In our simulator, land and property prices must be the clearing prices of auctions in which buyers bid the PV of *after-tax* cash flows. Capitalization then appears without being scripted.
3. **Every distortion needs a margin.** A tax can only distort a decision that exists in the model. The table in §1.11 lists, for each tax, the decision margin the agents must have. If the margin is missing, the tax looks artificially non-distortionary. For example, income tax with inelastic labour supply has no deadweight loss.
4. **Keep an open city with an outside option.** Taxes and public goods capitalize into land rents mainly because people and firms can *leave* (Tiebout/Oates). Closed-city and open-city models give very different incidence. Migration against an exogenous outside utility is therefore core to the MVP, not a luxury.
5. **Use stock-flow consistency.** Money is conserved inside the city. It changes only through explicit flows with the "Outside World": exports and imports, migrant wealth, and government borrowing. A ledger invariant test runs every tick. Without it, taxes "leak" or create money silently and the comparisons are worthless.
6. **Compute shadow counterfactuals, not scripted outcomes.** To measure deadweight loss honestly, agents can compute cheaply *what they would have done without the tax* (e.g. hours worked, building height, whether to trade). They do not act on it; it is only logged. This gives a measured, emergent Harberger-triangle proxy (§4).
7. **Use theory-derived invariance tests as unit tests (not as outcome scripts).** Examples: a lump-sum tax must not change any marginal decision. A land tax on *unimproved* value with perfect assessment must not change the developer's optimal height. A building tax must weakly lower it. These tests check that the *mechanics* are correct and leave the macro outcomes free (§7).

---

## 1. Tax-by-tax analysis

Notation used throughout: `ρ` = owner's discount rate (cost of capital), `g` = expected growth of rent, `τ_X` = rate of tax X, `A_L, A_B` = assessed land and building value, `R` = gross rent, `NOI` = net operating income.

### 1.1 Land value tax (LVT, Georgism, including split-rate)

**Mechanism and incidence.** Land is in fixed supply, so a tax on land value (more precisely, on the land rent or the unimproved value) cannot reduce the quantity of land. The textbook result is no deadweight loss. The burden falls on landowners and does not shift to tenants, because the tax does not change the supply of land and therefore not the rent. The burden is **capitalized**: the land price falls by the PV of future tax payments. For a perpetuity with assessment at market value:

```
P_land = (R_land − τ_L·P_land) / (ρ − g)   ⇒   P_land = R_land / (ρ − g + τ_L)
```

The owner *at the time the tax is announced* bears the burden. Later buyers pay a lower price and are compensated (Danish Economic Councils 2017, which report full capitalization of the PV of future land taxes after the 2007 Danish municipal reform). This result is contested. Nielsson, Wroblewski & Yding (2024), using a similar Danish quasi-experiment, report about zero effect on home prices, with the burden shared with tenants and future buyers. The simulator should therefore *not* assume either answer. Capitalization should emerge from bids, and the degree of capitalization should be a KPI.

**Predicted benefits (literature).**
- **No (or minimal) deadweight loss.** The Mirrlees Review (*Tax by Design*, ch. 16) recommends land value taxation for business and agricultural land "if practicable", and the abolition of transaction taxes ([IFS Tax by Design](https://ifs.org.uk/books/tax-design)).
- **Discourages speculative holding of vacant or under-used land.** The holding cost becomes independent of use, so idle land pays the same as a developed parcel. Developers in the Walker et al. (2024) ABM respond to this. Detroit split-rate analyses argue the same ([Lincoln Institute, Split-Rate Property Taxation in Detroit](https://www.lincolninst.edu/publications/other/split-rate-property-taxation-in-detroit/)).
- **Encourages density and improvement**, but only when it *replaces* the tax on buildings. Pittsburgh's 1979–80 shift to a heavier land tax was followed by a large increase in building activity compared with 14 other Rust Belt cities (Oates & Schwab 1997, *National Tax Journal*). Plassmann & Tideman (2000) find a higher land-to-building rate ratio is associated with more and higher-value building permits in Pennsylvania. Banzhaf & Lavery (2010, *JUE* 67(2)) find split-rate adoption increased housing units per area, mostly through more units (multi-unit structures) rather than larger dwellings ([Lincoln WP version](https://www.lincolninst.edu/app/uploads/2024/04/banzhaf-wp08sb1.pdf)). Oates & Schwab themselves stress that the effect comes mostly from *not taxing buildings*, not from taxing land.
- **Recaptures publicly created value (Henry George theorem).** In an optimally sized city with pure local public goods, aggregate differential land rent equals public-good spending (Arnott & Stiglitz 1979, *QJE* 93(4); [scan](https://academiccommons.columbia.edu/doi/10.7916/D8086FW3/download)). A land tax can therefore fund infrastructure that raises the same land values.
- **Progressive in wealth terms**, because land ownership is concentrated.

**Predicted drawbacks.**
- **Assessment difficulty.** Land value must be separated from building value, which is hard where vacant-land sales are rare ([Bowman, Lincoln Institute, *Methods of Valuing Land*](https://www.lincolninst.edu/app/uploads/2024/04/1120_Bowman_complete_web.pdf); Alvayay & Skidmore's [option-value approach for Detroit](https://www.lincolninst.edu/app/uploads/2024/04/Alvayay-Torrejon_Skidmore-Valuing-Land-in-Detroit-Using-the-Option-Value-Approach.pdf)).
- **Liquidity and "land-rich, cash-poor" owners** (for example retirees in now-valuable areas).
- **Timing non-neutrality when the base is market value.** Market value includes the option to develop, so the tax is paid ahead of the income. This favours early-payoff uses (Bentick 1979, *JPE* 87(4); Mills 1981). Neutrality is restored if post-development site value is defined as "the value if there were no building" (Tideman 1982). See also [Arnott 2005, "Neutral property taxation"](https://ideas.repec.org/a/bla/jpbect/v7y2005i1p27-50.html) and the [Lincoln WP on development timing](https://www.lincolninst.edu/app/uploads/2024/04/grote-wp08kg1.pdf).
- **Abandonment at very high rates.** If the tax exceeds the land rent, the land price goes to zero or below and owners walk away. The Citizens Research Council of Michigan warns of this for Detroit ([CRC](https://crcmich.org/?p=13583)).
- **Transition shock and windfall loss** for current owners, which is a political economy issue.
- **"Value-monistic"**: LVT is neutral about *what* is built, not about nature. Hence the ecological variant ELVT (Walker et al. 2024).

**Minimal agent rules that let the effects emerge.**

| Effect | Rule that must exist |
|---|---|
| Capitalization | Buyers bid `PV(after-tax net cash flows)`, and the land tax bill is a cash-flow term. Assessment tracks market value with a lag. |
| Anti-speculation | Owners of vacant land decide every period between `hold`, `develop` and `sell`, comparing expected appreciation with carrying cost (tax + opportunity cost) (§3.6.5). |
| Density | Developer chooses building height `h` to maximize NPV. The land tax term `τ_L·A_L` does not depend on `h`, but the building tax `τ_B·C(h)` does (§3.6.4). |
| Henry George | Public spending changes accessibility and amenities, which enter households' WTP. Land rents then respond, and the KPI compares aggregate differential rent with public spending. |
| Assessment error | An assessor agent estimates land value from noisy, lagged comparables (vacant sales plus the residual method). Error is a parameter and is visible. |
| Abandonment | If `cash < tax due` and `market value < 0` (or no bidder appears), the owner defaults and the parcel goes to a government auction. |

### 1.2 Property tax on land + buildings (conventional)

**Mechanism.** A tax on total assessed value `τ_P·(A_L + A_B)`. The land part behaves like an LVT. The building part is a tax on capital that is reproducible and mobile. It lowers the after-tax return to building and maintaining structures. In an open economy the building part is shifted forward to occupants as higher rents and lower housing consumption, or backward to land. Oates (1969, *JPE* 77(6); [pdf](https://www.Gwern.net/doc/economics/georgism/1969-oates.pdf)) found property taxes capitalize negatively and school spending positively into house values across New Jersey municipalities. This is the canonical Tiebout capitalization result.

**Predicted benefits.** A broad, visible, stable local base that is hard to evade (immobile). Funding local public goods creates a benefit-tax link (Tiebout).

**Predicted drawbacks.**
- **Discourages improvement, maintenance and density.** The "improvement effect" lowers the capital/land ratio.
- **Ambiguous effect on sprawl.** Brueckner & Kim (2003, *ITPF* 10(1); [RePEc](https://ideas.repec.org/a/kap/itaxpf/v10y2003i1p5-23.html)) show two opposing forces. Lower structural density pushes the city outward, while smaller dwellings pull it inward. The net effect depends on the elasticity of substitution between housing and other goods. Song & Zenou (2006) find the dwelling-size effect dominates under log-linear utility ([Lincoln WP](https://www.lincolninst.edu/app/uploads/2024/04/1095_zenou_final.pdf)). **This is a good test of whether the simulator is truly emergent: it should be possible to get either sign by changing preference parameters.**
- **Assessment regressivity and lags.**

**Minimal agent rules.** Developer NPV includes `τ_B·A_B(h)` (§3.6.4). Owners choose maintenance spending, and the assessed building value rises with maintenance, so the tax lowers the return to maintenance (§3.6.6). Households choose dwelling size (floor area) in the rental bid, which provides the dwelling-size channel.

### 1.3 Income tax (flat and progressive)

**Mechanism.** A tax on labour (and optionally capital) income. It drives a wedge between the gross wage the employer pays and the net wage the worker receives. It distorts:
- **Intensive margin** (hours and effort). The compensated elasticity for prime-age men is close to zero according to the review in Saez, Slemrod & Giertz (2012, *JEL* 50(1); [pdf](https://eml.berkeley.edu/~saez/saez-slemrod-giertzJEL09elasticity.pdf)).
- **Elasticity of taxable income (ETI)**, which includes avoidance. "The best available estimates range from 0.12 to 0.40" (Saez, Slemrod & Giertz 2012, as quoted in the [Cato review](https://www.cato.org/cato-journal/fall-2019/optimal-top-tax-rates-review-critique)).
- **Extensive margin** (participation). This matters most at the bottom, through interaction with benefit withdrawal.
- **Location**: migration of high earners. Kleven, Landais & Saez (2013, *AER*; [pdf](https://eml.berkeley.edu/~saez/kleven-landais-saezAER13football.pdf)) estimate that the elasticity of the number of foreign football players with respect to the net-of-tax rate is about 1, and about 0.15 for domestic players. Moretti & Wilson (2017, *AER* 107(7); [pdf](https://eml.berkeley.edu/~moretti/taxes.pdf)) estimate a long-run elasticity of star-scientist mobility of 1.8 with respect to personal income taxes.

**Incidence** depends on relative elasticities. If workers can leave the city (an elastic outside option), part of a local income tax is shifted to employers (higher gross wages) and then to land (lower rents).

**Benefits.** Can be progressive (redistribution), a large base, and an automatic stabilizer. Optimal top-rate theory: `τ* = (1 − g)/(1 − g + a·e)`. With Pareto parameter a = 1.5, e = 0.25 and g = 0 this gives about 73% combined (Diamond & Saez 2011). This depends on a broad base without avoidance ([EPI](https://www.epi.org/blog/high-top-income-tax-rates-fight/); [Cato critique](https://www.cato.org/cato-journal/fall-2019/optimal-top-tax-rates-review-critique)).

**Drawbacks.** Lower hours and participation, avoidance, emigration of the mobile and high-earning, and a deadweight loss that rises with the square of the rate: `DWL ≈ ½·ε·τ²·(wL)` ([Econlib](https://www.econlib.org/deadweight-loss-from-taxes-is-proportional-to-the-square-of-the-tax-rate/); [Auerbach lecture notes](https://eml.berkeley.edu/~webfac/auerbach/e230a_f19/Lecture%20Note%204.19.pdf)). Progressive schedules have higher marginal rates than average rates, so their DWL per unit of revenue is higher.

**Minimal agent rules.**
- *Hours*: isoelastic quasi-linear utility `u = c − ψ_i·h^{1+1/e}/(1+1/e)` ⇒ `h* = (w·(1−τ_m)/ψ_i)^e`, where `τ_m` is the marginal rate at the household's bracket. Bunching at kinks emerges naturally if `h` is continuous.
- *Participation*: work if `w·h*·(1−τ_avg) − commute_cost − F_i > benefit_if_not_working` (UBI does not count here because it is unconditional; means-tested benefits do). `F_i` is a heterogeneous fixed cost of working.
- *Reservation wage in net terms*: households compare **after-tax** wages across jobs and with the outside world. Firms that cannot fill vacancies raise gross wages (Lengnick rule, §2.11), so tax shifting emerges.
- *Migration*: the location utility uses after-tax income (§3.6.8).
- *Avoidance (phase 2)*: self-employed agents underreport a share `u` chosen to maximize `(1−p_audit)·τ·u·Y − p_audit·penalty·u·Y`.

### 1.4 Wealth tax

**Mechanism.** An annual tax on net wealth above a threshold. On an asset with return `r`, a wealth tax `τ_W` is equivalent to a capital-income tax of `τ_W/r`. At r = 4%, a 1% wealth tax is a 25% tax on the return. It hits low-return and illiquid assets hardest.

**Evidence.**
- *Strong reported-wealth responses*: in Switzerland, a 1 pp cut in the wealth tax rate raises reported taxable wealth by at least 43% after six years. About 24% of this is mobility, 21% housing prices, and most of the rest likely evasion (Brülhart, Gruber, Krapf & Schmidheiny 2022, *AEJ:Policy*; [pdf](https://edoc.unibas.ch/90165/1/pol.20200258.pdf); [VoxEU summary](https://cepr.org/voxeu/columns/wealth-taxation-swiss-experience)).
- Denmark: Jakobsen et al. find sizable long-run responses at the top ([NBER-related WP](https://www.cesifo.org/DocDL/cesifo1_wp7908.pdf)).
- *Decline*: 12 OECD countries had net wealth taxes in 1990, 4 in 2017. Repeals cited efficiency and administrative concerns and low revenue ([OECD 2018](https://www.oecd.org/en/publications/the-role-and-design-of-net-wealth-taxes-in-the-oecd_9789264290303-en.html)).
- *Capital flight / emigration*: Norway's 2022 increases coincided with a spike in departures to Switzerland (82 high-wealth individuals in 2022–23 per the Ministry of Finance, cited in [arXiv 2603.05277](https://arxiv.org/pdf/2603.05277)). Causality is confounded by concurrent changes (dividend tax, exit-tax rules), and revenue nevertheless rose ([The Local](https://www.thelocal.no/20251023/how-norways-wealth-tax-drove-out-the-rich-without-breaking-the-bank)).
- *Liquidity and valuation*: farmers and business owners are over-represented among low-liquidity households. The remedies are payment from income, asset sales, borrowing, deferral and payment in kind (Advani & Tarrant, *Fiscal Studies*; [LSE](https://researchonline.lse.ac.uk/id/eprint/113308); [valuation paper](https://dx.doi.org/10.47445/125)). Norwegian owners pull dividends out of their firms to pay the tax, which reduces firm investment ([BI](https://www.bi.no/en/research/research-centres/centre-for-corporate-governance-research/information-hub/family-firms/test-page-2/)).

**Predicted benefits.** Directly limits wealth concentration. Kinetic exchange models show that even small, frequent redistribution prevents "wealth condensation" or oligarchy (Bouchaud & Mézard 2000, [arXiv](https://arxiv.org/abs/cond-mat/0002374); Boghosian yard-sale variants, [arXiv 2310.16098](https://arxiv.org/pdf/2310.16098), [arXiv 2307.15513](https://arxiv.org/html/2307.15513v2)).

**Drawbacks.** Capital flight and emigration, valuation disputes, avoidance through hard-to-value assets, liquidity-forced fire sales, reduced investment by owner-operated firms, and low net revenue.

**Minimal agent rules.**
```
on annual wealth-tax date, for each household h:
  W_reported = deposits + equity_mkt + Σ property_assessed + (1−u_h)·private_equity_selfvalued − debt
  tax = τ_W · max(0, W_reported − threshold)
  pay from deposits down to a liquidity buffer B_h (e.g. 2 months of consumption)
  shortfall = tax − (deposits − B_h)
  while shortfall > 0:                           # liquidation ladder (most liquid first)
     if equity_mkt > 0: sell min(equity_mkt, shortfall/(1−haircut_eq))
     elif can_borrow_against_property: borrow up to LTV_max
     elif owns_firm: extract dividend from firm cash  (→ firm invests less)
     elif owns_property: list property at discount d (fire sale; may take months → arrears)
     else: arrears (interest + eventual enforced sale)
  # avoidance: u_h chosen to maximize (1−p_detect)·τ_W·u·PE − p_detect·fine·u·PE   (phase 2)
```
- *Emigration*: wealth tax enters expected lifetime after-tax consumption in the migration utility (§3.6.8), weighted by `τ_W·W/(ρ)`, so the wealthy face a larger exit incentive. Rich agents have lower relative moving costs, so higher mobility emerges.
- *Capital flight without emigration*: if the jurisdiction taxes only *local* assets (realistic for a city-level tax), agents reallocate portfolios to "outside assets" with return `r_out`, which removes capital from local firms and property.

### 1.5 Sales tax / VAT

**Mechanism.** A tax on consumption, either at retail (sales tax) or through the production chain with input credits (VAT). It is economically similar to a flat tax on labour income plus existing wealth (consumption = income − saving). It does not distort the saving decision, but it does distort the labour/leisure choice like a flat income tax.

**Incidence.** Statutory incidence on the consumer. Economic incidence depends on pass-through, which in turn depends on demand and supply elasticities and market power. Measured against *annual* income it is regressive, because the poor consume a larger share. Measured against *lifetime* income it is roughly proportional (Poterba, [NBER w4387](https://nber.org/papers/w4387); [Caspersen & Metcalf w4619](https://www.nber.org/papers/w4619.pdf)). Pass-through can be asymmetric: prices rise more after VAT increases than they fall after cuts (Benzarti, Carloni, Harju & Kosonen 2020, *JPE*; not verified in this session). Exemptions for "necessities" are poorly targeted (Benzarti & Carloni 2019).

**Benefits.** Hard to avoid at the border of a VAT chain, a broad base, no saving distortion, and it captures the consumption of tourists and migrants.

**Drawbacks.** Regressive at annual frequency. Cross-border shopping leakage for a city with a sales tax. Labour-supply distortion like a flat income tax.

**Minimal agent rules.**
- Firms post **net** prices `p` and households see `p·(1+τ_s)`. Firms adjust `p` by the inventory/markup rule (Lengnick, §2.11). Pass-through then emerges from how demand responds, not from a fixed parameter.
- Household consumption budget in **real** terms: `c_real = min((m/P_gross)^α, m/P_gross)` (Lengnick).
- *Cross-border shopping*: each household can buy from an "outside vendor" at `p_out + travel_cost(distance_to_city_edge)`. Leakage then emerges near the edge.
- Saving is untaxed, so the saving rate need not respond. Wealthy households with low consumption shares pay less relative to income, and annual-income regressivity emerges.

### 1.6 Corporate / profit tax

**Mechanism.** A tax on accounting profit `π = revenue − wages − rents − interest − depreciation`. If all costs are deductible (a pure economic-rent tax with expensing), it is neutral for investment. With partial depreciation allowances and non-deductible equity costs, it raises the cost of capital: `user_cost = (ρ + δ)·(1 − τ_c·z)/(1 − τ_c)`, where `z` is the PV of depreciation allowances. It also drives relocation and profit shifting.

**Incidence.** Harberger (1962): in a closed economy capital bears the burden. In an open economy with mobile capital, burden shifts to immobile factors (labour, land). Fuest, Peichl & Siegloch (2018, *AER*; [NBER conf. pdf](https://conference.nber.org/conf_papers/f91099.pdf)), using 6,800 German municipal business-tax changes, find workers bear about half, more so low-skilled, young and female workers. Moretti & Wilson (2017) find a state corporate-tax elasticity of star-scientist location of 1.9.

**Benefits.** Taxes economic rents, including those of the immobile local monopolies. Administrative concentration (few taxpayers).

**Drawbacks.** Reduces investment and capital deepening. Relocation of mobile, tradable firms. Partial pass-through to wages and prices. Double taxation with dividend/income tax.

**Minimal agent rules.**
- Firms choose capital `K` by comparing marginal revenue product with user cost (above). The tax enters through `z < 1`.
- *Location*: each tradable firm periodically compares expected after-tax profit in the city (rent, wages, agglomeration productivity, taxes) with an outside option `π_out` net of relocation cost. Logit exit. Non-tradable firms (serving local households) cannot relocate their market, so they shift via prices. This is the difference between "boarded-up export factory" and "pricier corner shop".
- Wage incidence emerges through Lengnick-style wage setting: lower after-tax profits lead to fewer vacancies, which leads to slower wage growth.
- Profits are distributed to owner households (equity), so the income/dividend tax interaction is automatic.

### 1.7 Harberger tax / COST (common ownership self-assessed tax)

**Mechanism.** Each owner self-declares a price `S`, pays `τ_H·S` per year, and must sell to anyone who offers `S`. A high declaration means high tax. A low declaration risks being bought out cheaply. The rate trades off **allocative efficiency** (assets move to the highest-value user, which breaks the monopoly hold-out) against **investment efficiency** (the owner captures only `(1 − τ_H/turnover)` of improvement gains). Weyl & Zhang show that setting `τ` equal to the probability of sale (turnover rate) makes declared prices approximately allocatively efficient, while positive `τ` reduces investment incentives. Posner & Weyl suggest 5–10% per year for most assets ([Wikipedia: Harberger tax](https://en.wikipedia.org/wiki/Harberger_Tax); Posner & Weyl, *Radical Markets*, 2018; [Posner & Weyl, "Property is only another name for monopoly"](https://www.law.nyu.edu/sites/default/files/upload_documents/Property%20Monopoly.pdf)). The critique: it penalizes the information investment needed to value assets (Rallo 2019, [RePEc](https://ideas.repec.org/a/kap/ejlwec/v47y2019i1d10.1007_s10657-018-9607-6.html)). Lab evidence on related self-assessment mechanisms for land assembly shows gains that are "statistically insignificant or transitory" (DeSantis, McCarter & Winn 2022, [Emerald](https://emerald.com/insight/content/doi/10.1108/S0193-230620220000021004)). **No agent-based model of COST was found, so this simulator would be novel here.**

**Benefits.** Fast reallocation to high-value users, no assessor needed (self-assessment), a revenue base close to the true use value, and it limits land hoarding.

**Drawbacks.** Lower investment in improvements, insecurity for residents (forced sale of the home), subjective-value loss (owner attachment, moving costs), strategic and predatory buying, and liquidity needs.

**Minimal agent rules.**
```
each year, owner o of parcel j chooses S_j:
  v_o   = owner's private use value (PV of own rent-equivalent + attachment premium a_o)
  Pr(S) = estimated prob. that some agent buys at S within the year
        = share of recent bids for comparable parcels ≥ S   (empirical CDF, from market log)
  choose S on a grid to maximize  E[payoff] = −τ_H·S + Pr(S)·(S − v_o)
any agent b with WTP_b(j) > S_j·(1 + transaction cost) may execute a forced purchase
  (optional notice period; displaced owner/tenants re-enter the housing market)
improvement decision: invest I if  ΔV_o·(1 − expected forced-sale loss) − τ_H·ΔS/ρ ≥ I
```
The investment-efficiency loss emerges because a declared-value increase raises the tax bill and the forced-sale probability together. Turnover and the allocative gain emerge from the forced-sale rule. Attachment `a_o` (e.g. tenure years × constant) makes the displacement harm visible.

### 1.8 UBI / citizen dividend (revenue recycling)

**Mechanism.** An unconditional, equal per-capita transfer funded by one of the taxes. As a lump-sum transfer it has an *income effect* (less labour) but no *substitution effect* at the margin. Means-tested benefits do have one, through implicit withdrawal rates. The combination "LVT + citizen dividend" is the classic Georgist proposal.

**Evidence.** Alaska Permanent Fund Dividend: no effect on overall employment and +1.8 pp part-time work. General-equilibrium demand effects in non-tradables offset individual labour-supply reductions (Jones & Marinescu 2022, *AEJ:Policy*; [pdf](https://home.uchicago.edu/j1s/Jones_Alaska_2022.pdf)). Finland 2017–18 experiment: small employment effects, better perceived economic security and mental wellbeing ([STM release](https://stm.fi/en/-/perustulokokeilun-tulokset-tyollisyysvaikutukset-vahaisia-toimeentulo-ja-psyykkinen-terveys-koettiin-paremmaksi)). Carbon-tax recycling: British Columbia's tax became progressive *because of* recycling, and aggregate employment was not significantly affected (Azevedo, Wolff & Yamazaki; [LSE](https://researchonline.lse.ac.uk/id/eprint/117346); [World Bank](https://transparency-partnership.net/system/files/migrated_document_files/britishcolumbiataxingcarbonfordevelopmentworldbank2023.pdf)).

**Risks in an open city.** A *city-level* UBI attracts in-migration of low-income households and raises rents. The dividend capitalizes into land, which is the Henry George mechanism in reverse. A residency requirement (e.g. N years) is a policy lever.

**Minimal agent rules.** UBI enters the household budget as unconditional income. The hours rule (§1.3) then produces the income effect, *provided* utility is not quasi-linear for this purpose. Use `u = ln(c) − ψ·h^{1+1/e}/(1+1/e)` so income effects exist. Migration utility includes the UBI, so in-migration and capitalization emerge. Local demand from UBI flows to non-tradable firms through the goods market, so the GE demand effect emerges.

### 1.9 Pigouvian taxes: carbon / pollution / congestion

**Mechanism.** A tax equal to marginal external damage internalizes the externality. It *corrects* a distortion instead of creating one, and may yield a "double dividend" if revenue replaces distortionary taxes. Incidence: energy-intensive consumption is a larger budget share for poorer households, so the tax is regressive before recycling. With per-capita recycling it becomes progressive (§1.8 BC evidence; [Tiemroth/Tufts](https://dl.tufts.edu/concern/pdfs/70795p10f)).

**Drawbacks.** Leakage (emitters relocate outside the city), competitiveness of tradable firms, regressivity without recycling, and uncertainty about damage values.

**Minimal agent rules.**
- Firms emit `e = φ_s·Y·(1 − a)`, where `a` is the abatement share with cost `κ·a²·Y`. Optimal `a* = min(1, τ_e·φ_s/(2κ))`. Firms pay `τ_e·e`.
- Households choose commute mode by logit over generalized cost: `time·VOT + money + τ_e·fuel_emissions`.
- Pollution diffuses on the grid (e.g. one Gaussian blur pass per tick plus decay) and reduces amenity and health in household utility. Nobody is told to dislike polluting firms. Rents near polluters fall through bids.
- Leakage emerges through the corporate location rule (§1.6).

### 1.10 Transfer tax / stamp duty

**Mechanism.** A tax on the transaction value, paid at sale. It creates a wedge between buyer's and seller's prices and reduces the number of trades. It distorts **who lives where**: owners stay in mismatched homes (lock-in), with labour-market and housing-size mismatch.

**Evidence.** UK stamp duty notches: prices bunch below the thresholds, the price effect is 200–500% of the tax at the notch, and a temporary removal of a 1% tax raised transactions by 20% (Best & Kleven 2018, *REStud* 85(1); [IDEAS](https://ideas.repec.org/a/oup/restud/v85y2018i1p157-193..html)). A higher SDLT strongly reduces housing-related and short-distance moves but not job-induced moves (Hilber & Lyytikäinen 2017, *JUE*; [LSE](https://researchonline.lse.ac.uk/id/eprint/65022)). The Finnish reform reduced mobility within and across labour markets (Eerola et al.; [VATT](https://vatt.fi/-/effects-of-real-estate-transfer-taxes-evidence-from-a-natural-experiment)). A proposed Massachusetts transfer tax would *lower* net revenue through lock-in (Duranton, Han & Turner; [Wharton](https://realestate.wharton.upenn.edu/working-papers/the-negative-impact-of-a-massachusetts-realestate-transfer-tax-on-fiscal-revenue-andhousehold-mobility/)). The Mirrlees Review recommends abolishing transfer taxes.

**Minimal agent rules.**
- Owner households re-evaluate their location periodically. Move if `Δutility × horizon − moving_cost − τ_T·price > 0`. The tax enters the threshold, so lock-in emerges.
- Buyers bid with the tax in their budget. Sellers list with it in their reservation price. The wedge then appears in the double auction, and bunching emerges automatically if the tax is a notched schedule.
- Mismatch KPIs (§4) make the cost visible.

### 1.11 Summary: tax → base → distorted margin → required agent rule

| Tax | Base | Main margin distorted | Must-have agent rule | Main visible effect |
|---|---|---|---|---|
| LVT | assessed land value | none in theory (timing if base = market value) | hold/develop/sell choice; NPV-max height; bid-based land prices | vacant lots fill in, taller buildings, lower land prices |
| Property tax | land + buildings | building intensity, maintenance | NPV-max height incl. `τ_B`; maintenance choice | lower buildings, more decay, sprawl ambiguity |
| Income tax | wages (+capital income) | hours, participation, location | isoelastic hours; participation threshold; net-wage reservation; migration | fewer commuters, emigration of high earners |
| Wealth tax | net wealth | saving, portfolio, location, reporting | liquidation ladder; portfolio choice local vs outside; migration | fire-sale "For Sale" signs, moving vans of the rich |
| VAT / sales | consumption | labour/leisure, cross-border shopping | gross-price consumption; outside vendor at edge | shops near border close, malls outside the city |
| Corporate | profits | investment, firm location | user-cost investment; tradable-firm relocation logit | boarded-up factories, fewer office towers |
| Harberger / COST | self-declared value | investment in improvements | declared-price optimization; forced-sale execution | frequent turnover, plainer buildings, displaced residents |
| UBI (recycling) | (transfer) | hours (income effect), migration | non-quasi-linear utility; migration incl. UBI | more part-time, in-migration, rent rises |
| Carbon / pollution | emissions | output, abatement, mode choice | abatement choice; mode logit; pollution diffusion | clearer skies, more transit and bikes, factory leakage |
| Transfer tax | sale price | turnover, mobility | owner moving threshold incl. tax; auction wedge | fewer "Sold" signs, mismatched households |

---

## 2. Agent-based urban and economic models in the literature

### 2.1 Alonso–Muth–Mills (AMM) monocentric model (analytical benchmark, not an ABM)

Identical households commute to a central business district (CBD). Utility `U(z, q)` over a numeraire `z` and floor space `q`; budget `z + R(x)·q = y − t·x`. Spatial equilibrium means equal utility everywhere, so the **bid-rent** `R(x)` is the maximum rent that delivers the equilibrium utility `ū` at distance `x`. With Cobb-Douglas `U = z^α q^β` (α+β=1): `R(x) = [(y − t·x)/(K·ū)]^{1/β}`, K = α^α β^β. The city edge `b` satisfies `R(b) = R_agri`. In a closed city, population fills the space between 0 and `b`. In an open city, `ū` is fixed from outside and population adjusts. ([Aalto lecture](https://mycourses.aalto.fi/pluginfile.php/1427767/mod_resource/content/2/Urban_Econ_Lec2.pdf); [empirical test, arXiv 2111.02112](https://arxiv.org/pdf/2111.02112).)
**Use for us:** a *validation target*. A homogeneous-agent run with one CBD should reproduce a downward-sloping rent gradient and an edge where urban bids equal agricultural rent. Filatova et al. (2009) use exactly this as their replication test.

### 2.2 ALMA: Agent-based Land MArket (Filatova, Parker & van der Veen 2009, *JASSS* 12(1)3)

[Paper](https://www.jasss.org/12/1/3.html) · [notebook implementation](https://nbviewer.cgwebhost.cigi.illinois.edu/github/cybergis/ALMANotebook/blob/master/ALMA.ipynb)
- **Agents:** buyers (households) and sellers (agricultural landowners) on a grid with a CBD.
- **Utility:** Cobb-Douglas in amenity `A` and proximity `P` (β = 0.85 or 0.7). Budget net of linear transport cost `Y − t·D` (Y = 800).
- **WTP:** Michaelis–Menten form `WTP = Y·U/(U + b)` (b = 70). It approaches the budget as utility rises.
- **Bids/asks adjust to market pressure:** `P_bid = WTP·(1 + ε)`, `ε = (N_B − N_S)/(N_B + N_S)`. Sellers' asks move with the same ε, but never below the agricultural reservation price (P_ag = 200).
- **Protocol:** each buyer samples N affordable listings and bids on the highest-utility one. The seller takes the highest bid and trades if bid ≥ ask. **Price = arithmetic mean of ask and best bid.**
- **Results:** reproduces the AMM gradient (linear fit R² ≈ 0.99). Seller's markets raise prices and expand the city. Buyer's markets lower prices but also expand the city. The *distinction between WTP and bid*, driven by market conditions, matters for land prices.
- **Use for us:** the bilateral bid/ask with excess-demand markup is cheap and gives capitalization for free. Adopt it for vacant land.

### 2.3 Parker & Filatova (2008) and successors

Conceptual design of a bilateral ABM land market with households, developers and rural landowners. It adds two local feedbacks: **price expectations formed from neighbourhood transactions** and **spatial externalities** ([MSU summary](https://www.canr.msu.edu/resources/conceptual-design-bilateral-agent-based-land-market-heterogeneous-economic-agents)). Later Filatova work (coastal RHEA, [land-market mechanisms for coastal ecosystems](https://catalog.comses.net/publications/459)) shows how hazard and amenity capitalize through bids. **Use for us:** neighbourhood-based expectations, i.e. each agent estimates `g` from recent local sales.

### 2.4 CHALMS: Coupled Housing and Land Markets (Magliocca, McConnell, Walls & Safirova 2011, *CEUS*)

[CoMSES](https://catalog.comses.net/publications/2419) · [code](https://miracle.comses.net/codebases/3368/)
- **Agents:** consumers, farmers and developers in a growing exurban area.
- Developers form **adaptive price expectations** and decide what lot size and house type to build. Farmers decide when to sell based on expected returns versus farming. Consumers bid on houses.
- **Results:** emergent density gradient and **leapfrog development** (sprawl pattern).
- **Use for us:** the developer and farmer conversion logic for urban growth into nature and farmland. Sprawl emerges from developers' expectations.

### 2.5 UrbanSim (Waddell 2002, *JAPA* 68(3))

A microsimulation of households, jobs and real-estate development used in operational planning. Location choice is a **multinomial logit with sampled alternatives** ([Lyon/STRC](https://www.strc.ch/2006/Buergle_STRC_2006.pdf); [Waddell slides](https://strc.ch/2005/Waddell.pdf)). A developer model builds where it is profitable within zoning capacity. Prices come from hedonic regressions, not auctions (a weakness for our purpose). Lee & Waddell (2010) use nested logit for mobility plus location. **Use for us:** the *sampled-alternatives logit* (choose among K≈10–30 random vacancies instead of all), which makes location choice O(K) per agent per tick.

### 2.6 Walker, Mertan, Farley, Rizzo & Reynolds (2024): LVT / ELVT ABM

[*Land* 13(11):1795](https://doi.org/10.3390/land13111795) · [code](https://github.com/DakotaBWalker/Speculation-and-Land-Tax-ABM) (Python, ~2.6k LOC; read for this report)
- **Agents** (shares 15/80/5): **developers** (wealth ~ lognormal around $5M), **homeowners** (~$345k) and **speculators** (~$3.3M). 100×100 grid. Runs are short (MAX_TIMESTEP = 4–20).
- **Tax schemes:** status quo (land and improvement taxes), LVT (`LVT_RATE = 0.1`, `IVT_RATE = 0.035`), and ELVT (land rate adjusted by ecological impact).
- **Speculator buy rule** (benefit/cost ratio): `BC = (dLV·LV·lot)/(LV·lot·τ_L)`. Buy from the top 10% of options if `max BC > 0.2`. Sell with 10% probability per step. Restore ecology if the ELVT tax saving exceeds the restoration cost.
- **Developer rule:** land cost including the PV of the tax, `LC = LV·lot·(1 + τ_L/ρ_dev)`. Number of units `n ≈ LC/unit_hard_cost × IV/LV_ratio`. Hard cost includes `(1 + τ_B/ρ_dev)`. Build if `1.15·cost_per_unit ≤ WTP_per_unit`. Price set halfway between cost and WTP, adjusted by local vacancy.
- **Homeowner WTP** subtracts the PV of tax bills (`τ·value/ρ_home`).
- **Results (paper):** LVT and ELVT raise capital intensity, lower land intensity, cause losses for speculators and lower average unit prices. ELVT increases urban nature.
- **Limitations (for us):** land value is an *exogenous formula* of preferences and density, not market-formed, so there is no capitalization feedback. The time horizon is very short. There is no migration, labour market or firms. Several "magic numbers" (thresholds 0.2, 0.15, 1.15).
- **Use for us:** the speculator benefit/cost rule and developer feasibility structure, but with market-formed prices.

### 2.7 Schelling segregation (1971, *J. Math. Sociology* 1)

Agents of two types move if the share of unlike neighbours exceeds a tolerance. Strong segregation emerges from mild preferences ([pdf](https://uzh.ch/cmsssl/suz/dam/jcr:00000000-68cb-72db-ffff-ffffff8071db/04.02%7B_%7Dschelling%7B_%7D71.pdf)). Logit (noisy) best-response variants behave better than strict best response (Grauwin, Goffette-Nagot & Jensen; [HAL](https://halshs.archives-ouvertes.fr/halshs-00404400)). **Use for us:** an *income-sorting* externality. Households value the neighbourhood's mean income or services, and income segregation emerges and interacts with taxes (e.g. LVT funding uniform services versus property tax funding local services). Cheap: just a neighbourhood term in utility.

### 2.8 Axtell: endogenous firms (1999 Brookings WP; Handbook of Computational Economics 2018)

[Brookings pdf](https://www.brookings.edu/wp-content/uploads/2016/06/firms.pdf) · [code](https://www.comses.net/codebases/4948/releases/1.2.0/)
Agents with heterogeneous preferences for income versus leisure join teams with increasing returns to effort. Output is shared equally, which creates free-riding in large teams. Agents periodically re-optimize effort and switch firms or start new ones. **Firms are born, grow and die. Heavy-tailed firm sizes and stationary aggregate distributions emerge** even at full US scale (120 million agents). **Use for us (phase 2+):** a cheap way to make firm entry and exit emergent. Too heavy for MVP. For MVP, use a fixed number of firm "slots" with Lengnick-style adjustment and simple entry and exit.

### 2.9 Sugarscape (Epstein & Axtell 1996, *Growing Artificial Societies*)

A grid with a renewable resource. Agents have vision and metabolism, move to the best visible cell, harvest and die if starving. A highly skewed (Pareto-like) wealth distribution emerges from heterogeneity plus landscape, and inheritance amplifies it ([Brookings](https://www.brookings.edu/books/growing-artificial-societies)). **Use for us:** the conceptual proof that inequality emerges from location and endowment. In our city, the "sugar" is land rent at good locations, which ties directly to the Georgist argument.

### 2.10 Kinetic wealth-exchange / yard-sale models

- **Yard-sale model** (Chakraborti; Hayes 2002): random pairs stake a fraction of the *poorer* agent's wealth in a fair coin flip. Wealth condenses into an oligarchy despite fair trades ([Wikipedia](https://en.wikipedia.org/wiki/Yard-sale_model)). With redistribution (a tax toward the mean, χ), a stable distribution with a finite Gini appears. Boghosian derived the Boltzmann/Fokker–Planck description and the Gini under taxation ([arXiv 2307.15513](https://arxiv.org/html/2307.15513v2); [arXiv 2310.16098](https://arxiv.org/pdf/2310.16098)). Extreme inequality is suppressed if the redistribution frequency exceeds 1/N of the exchange frequency.
- **Bouchaud & Mézard (2000)**: exchange plus multiplicative random returns gives a Pareto tail and a "wealth condensation" phase transition. More exchange and higher taxes reduce inequality ([arXiv](https://arxiv.org/abs/cond-mat/0002374)).
- **Drăgulescu & Yakovenko (2000)**: random money exchange gives a Boltzmann–Gibbs (exponential) distribution.
- **Use for us:** a *diagnostic*, not a mechanism. In our model, inequality should emerge from (i) multiplicative, heterogeneous returns on property and equity (Bouchaud–Mézard-like) and (ii) inheritance. The kinetic results predict that wealth taxes and capital-return taxes compress the tail most, which our sim should reproduce *without being told*. If it does not, investigate.

### 2.11 Lengnick (2013) baseline macro ABM (*JEBO* 86:102–120)

[pdf](https://legacy.econ.tuwien.ac.at/lva/compeco.se/artikel/jebo_2013_agent_based_macroeconomics_a_baseline_model.pdf) · 1,000 households, 100 firms. Days (21 per month) and months.
- **Households:** reservation wage `ω_h` and liquidity `m_h`; inelastic labour (1 unit). Trade links to 7 firms ("type A"). Each month, with p = 0.25, compare a random linked firm with a random new firm (chosen proportional to size) and switch if the new one is ≥1% cheaper. With p = 0.25, replace a firm that rationed you. Unemployed visit up to β = 5 firms for a vacancy. Employed search with p = 0.1 if satisfied, p = 1 if paid below `ω_h`.
- **Consumption:** `c_real = min((m_h/P_h)^α, m_h/P_h)`, α = 0.9. Spent evenly over 21 days. Visit up to n = 7 firms until 95% satisfied.
- **Firms:** linear production `f(l) = λ·l` (λ = 3). Inventory band `[φ_low·d, φ_up·d]` = [0.25, 1]×demand: hire one if below, fire one if above (lag 1 month). Price changes only if inventory is out of band, within markup bounds `[1.025, 1.15]×marginal cost`. Step `p·(1 ± U(0, 0.02))`, applied with probability 0.75 (Calvo-like). Wage: raise if a vacancy went unfilled, cut if all positions were filled for γ = 24 months, step `U(0, 0.019)`.
- **End of month:** pay wages, keep a buffer `χ = 0.1 × wage bill`, distribute remaining profits to households **proportional to their liquidity** (a proxy for equity ownership). Reservation wage: raise to the current wage if exceeded; unemployed cut it by 10%.
- **Emergent:** a near-equilibrium with business-cycle stylized facts, endogenous price-change frequency (median 9–12% per month), and robustness unless price or wage adjustment is turned off.
- **Use for us:** **adopt almost verbatim** for the labour and goods markets. It is cheap, needs no auctioneer, and produces tax pass-through endogenously.

### 2.12 Mark-0 (Gualdi, Tarzia, Zamponi & Bouchaud 2015, *JEDC*; [arXiv 1307.5319](https://arxiv.org/abs/1307.5319))

Firms adjust output to excess demand with asymmetric speeds:
```
if Y_i < D_i:  Y_i ← Y_i + min(η⁺·(D_i − Y_i), μ·ũ_i);   if p_i < p̄: p_i ← p_i·(1 + γ_p·ξ)
if Y_i > D_i:  Y_i ← max(Y_i − η⁻·(Y_i − D_i), 0);        if p_i > p̄: p_i ← p_i·(1 − γ_p·ξ)
```
Households hold aggregate savings and spend a fraction. Firms go bankrupt when the debt/payroll ratio exceeds Θ. **Key emergent result:** a phase transition between a "good" low-unemployment economy and a "bad" high-unemployment one, governed by the hiring/firing asymmetry R = η⁺/η⁻ and by the bankruptcy threshold Θ. **Lesson for us:** macro outcomes can be *knife-edge* in such parameters. Tax comparisons must be run in a region of parameter space that is far from such transitions, or the tax effect will be confounded with a regime switch. Do a parameter sweep before trusting comparisons.

### 2.13 Housing ABMs: Geanakoplos/Axtell et al. (Washington DC) and the Bank of England model

- **Axtell, Farmer, Geanakoplos, Howitt et al.** ([Cowles DP 1852, 2012](https://cowles.yale.edu/sites/default/files/2022-08/d1852.pdf); [Bundesbank 2014 slides](https://www.bundesbank.de/resource/blob/636008/c54ccc1e138b5af05f970c50a97b22a3/mL/2014-06-05-eltville-10-axtell-data.pdf)): heterogeneous households make rent or buy decisions, are matched to homes and take mortgages. Calibrated to all DC transactions 1997–2009. They find that the 1997–2007 boom and bust was driven mainly by **leverage** (LTV), more than by interest rates.
- **Baptista, Farmer, Hinterschweiger, Low, Tang & Uluc (2016), Bank of England SWP 619** ([pdf](https://www.bankofengland.co.uk/-/media/boe/files/working-paper/2016/macroprudential-policy-in-an-agent-based-model-of-the-uk-housing-market.pdf); [code](https://github.com/INET-Complexity/housing-model)). The rules are directly reusable:
  - Desired house expenditure: `p_desired = α·y·e^ε / (1 − β·g)`, with `g` the expected house-price growth.
  - Expectations: `g_t = α·[(h_{t−1}+h_{t−2}+h_{t−3})/(h_{t−13}+h_{t−14}+h_{t−15}) − 1]`, i.e. extrapolation from last year's HPI change.
  - Rent vs buy: `P(buy) = σ(β·[r_Q·(1+τ) − 12·(m − p·g)])`, a logistic function of the rent cost (with psychological premium τ) minus the ownership cost (mortgage minus expected appreciation).
  - Renters bid **33% of income** on the rental market.
  - Owners sell on average every 11 years (hazard adjusted for market stock).
  - List price: `ln p_s = α + ln p̄_Q − β·ln(ζ·(1 + f̄)) + ε`, where `p̄_Q` is the average sale price of similar quality and `f̄` is days on market. Unsold listings are cut with probability 0.06 per month by a lognormal fraction. Unlet rentals are cut by 5% per month.
  - Buy-to-let investors: expected yield `Ω = (p/d)·(δ·(g + κ) + (1−δ)·r̄) − m/d`. `P(buy) = σ(βΩ)^{1/12}`. "Fundamentalists" use δ = 0.5 and "trend-followers" δ = 0.9. A larger trend-follower share amplifies cycles.
  - Market clearing: **double auction in rounds**. Each bid matches the highest-quality offer it can afford. If several bidders match one offer, the seller "bids up" the price by a small step and picks randomly among bidders who can still afford it. Unmatched bids are dropped and re-placed next month.
  - **Use for us:** this is the most implementation-ready housing market in the literature. Adopt the double auction, list-price decay and the fundamentalist/trend-follower investor split (the source of *speculation*).

### 2.14 EURACE (Deissenberg, van der Hoog & Dawid 2008; Eurace@Unibi; Cincotti, Raberto, Teglio)

A massively parallel macro ABM (FLAME framework) with regions mirroring Eurostat units, local labour and consumption-goods markets, investment goods, credit and finance ([HAL](https://halshs.archives-ouvertes.fr/halshs-00339756v1/file/DT2008-39.pdf)). Housing and mortgage extensions show that looser mortgage requirements give short booms and more fragile economies (Erlingsson et al. 2014, [IDEAS](https://ideas.repec.org/p/zbw/ifwedp/201332.html)). **Lesson for us:** a full credit-money and banking system is the main driver of EURACE's complexity and cost. We exclude it (§6) and use a fixed-interest "outside bank".

### 2.15 Cross-cutting lessons: how these models stay tractable *and* emergent

1. **Local, sampled search instead of global optimization.** Lengnick: 7 trade links and a few job visits. UrbanSim: sampled logit alternatives. ALMA: N sampled listings. Cost per agent is O(K), not O(N).
2. **Adaptive heuristics with bounded steps** for prices and wages (±U(0, 2%)), which gives a slow, stable, tâtonnement-like convergence without an auctioneer.
3. **Two time scales**: fast (daily/monthly trading) and slow (annual development, assessment, demographics).
4. **Simple expectation rules** (extrapolation of the last year's local price change, fundamentalist/chartist mix) generate booms, busts and speculation endogenously.
5. **Heterogeneity in only a few parameters** (income, preference weights, discount rates, mobility costs) is enough for rich distributions.
6. **Stylized facts as validation, not inputs** (AMM gradient, firm-size tails, wealth Pareto tail, price-change frequency).

---

## 3. Recommended model for the simulator

### 3.1 World and time

- **Grid of parcels**, e.g. 96×96 cells (≈9k parcels) for MVP and 128×128 later. Parcel attributes:
  - Static: terrain/buildability, base ecological value `eco0` (forest, wetland, farmland), water/view amenity.
  - Dynamic: `use ∈ {nature, farm, vacant, residential, commercial, industrial, mixed, park, road, civic}`, `floors h`, `floor area`, `quality q` (0–1, decays), `building age`, `owner`, `tenants`, `assessed land` `A_L`, `assessed building` `A_B`, `declared value S` (COST), `pollution`, `last sale price/time`, `for sale / for rent flags`.
- **Transport network**: a road graph on the grid. Optional transit lines in phase 2. Precompute travel time from each parcel to each job cluster with Dijkstra or BFS. Recompute only when the network changes.
- **Time**: 1 tick = 1 month. Labour and goods markets clear monthly (no daily loop; aggregate Lengnick's 21 days into a few vendor visits). Housing and rent markets clear monthly. Development decisions and assessment run quarterly or yearly. Demographics (aging, births, deaths, inheritance) run yearly. Typical run: 30–60 simulated years = 360–720 ticks.
- **Outside World**: an exogenous region with a fixed utility level `Ū_out` (could drift), wage level `w_out`, return on capital `r_out`, goods price `p_out` and agricultural land rent `R_agri`. It is the source and sink for migrants, exports, imports and capital flight. This is what makes the city "open".

### 3.2 Agent types

| Agent | Count (MVP) | State | Decides |
|---|---|---|---|
| **Household** (1–4 persons; the unit for housing and budget) | 3k–15k | age of head, skill `s`, size, deposits `m`, equity holdings, owned parcels, mortgage, home parcel, job, preference weights (α_c, β_h, γ_amen, δ_commute, θ_neigh), discount rate, mobility cost, ψ (leisure preference), expectation type (fundamentalist/trend), attachment | hours and participation, job search, consumption and saving, where to live (rent/buy/move), migration in or out, portfolio (deposits, local equity, property, outside assets), tax payment and liquidation, (COST) declared values |
| **Firm** | 100–600 | sector (tradable export / local non-tradable / construction), location (parcel), workers, wage, price, inventory, capital K, cash, owners (household shares), emission intensity, abatement | hiring and firing, wage, price, investment, location and relocation, exit and entry, abatement |
| **Developer** (a firm in the construction sector, or a role any wealthy household can take) | 5–30 | cash, land bank, projects in progress, expectation type | buy land, choose building type and height, build, sell or hold, demolish and redevelop |
| **Landlord / investor** | a role of households (BoE BTL style) plus optional institutional investor | portfolio | buy, hold, sell rental property; set rents; maintenance |
| **Government** (city) | 1 | tax schedule, budget, debt, spending shares, assessor | collect taxes, assess values, spend on public goods, pay UBI, auction defaulted parcels, zoning (optional) |
| **Outside World** | 1 | `Ū_out`, `w_out`, `r_out`, `p_out`, `R_agri` | absorbs and supplies migrants, goods, capital |
| **Bank** (passive) | 1 | interest rate `r` (fixed or slowly drifting), LTV and DTI caps | grants mortgages and deposit interest (no money creation beyond loans backed by the Outside World; §6) |

### 3.3 Markets

1. **Rental housing market** (monthly, double auction BoE-style). Vacant units are listed with an asking rent. Households in search compute a bid for each of K sampled vacancies (§3.6.1) and pick by logit. Matching: highest-quality affordable unit, with "bid-up" when contested. Unlet units' asks decay 5% per month (or ε-adjusted per ALMA).
2. **Property (asset) market**: dwellings, commercial buildings and vacant land, all sold on the same auction. Buyers are owner-occupiers, investors and developers. Bids are PVs of after-tax cash flows (§3.6.2). Sellers list per the BoE list-price rule, with decay. Transfer tax is applied at execution.
3. **Labour market** (Lengnick): firms post vacancies at a wage. Households search a few firms per month, ranking by **net wage minus commute cost**. The reservation wage is in after-tax terms.
4. **Goods market** (Lengnick): non-tradable firms sell to linked households at `p·(1+τ_s)`. Households rewire links to cheaper or closer vendors. The outside vendor is always available at `p_out + travel cost`. Tradable firms sell to the Outside World at an exogenous price `p_export` with demand elasticity, which gives the "economic base" that drives city size.
5. **Commercial floor space**: firms rent floor space like households do. Their bid is based on productivity at that location (access to workers and customers, agglomeration).
6. **Equity**: firm ownership shares held by households. MVP: shares fixed at founding plus inheritance. Later: simple trading at book value × multiple.

### 3.4 Money and accounting

- Every transfer goes through `ledger.transfer(from, to, amount, kind)`. Invariant each tick: `Σ cash(agents) + Σ cash(gov) − gov_debt_outside − outside_world_net = constant + net_external_flows`. A test fails the run on violation.
- Mortgages are loans from the passive bank, funded by the Outside World at rate `r`. This keeps credit finite and avoids endogenous money (§6). Interest flows out of the city to the Outside World unless households hold deposits.

### 3.5 Utility and household budget

Per month, for household i at location j with job k:
```
y_net   = w_k·h·(1 − τ_inc(·)) + dividends·(1 − τ_div) + interest + UBI + benefits − tax_wealth/12
budget  = y_net − rent_j (or user cost of owning) − commute_cost(j,k) − property/land tax (if owner)/12
c       = consumption spending (real units, priced at p·(1+τ_s))
U_ij    = α_c·ln(c) + β_h·ln(floor_area_j) + γ·Amen_j − δ·T_commute(j,k) − π·Pollution_j
          + θ·NeighIncome_j + σ·Services_j − ψ·h^{1+1/e}/(1+1/e)
Amen_j  = w_park·ParkAccess_j + w_nature·NatureNearby_j + w_water·Water_j − w_blight·VacantDerelictShare_j
```
Logs give diminishing returns and non-zero income effects, which UBI needs (§1.8). All amenity terms are physical quantities produced by the simulation (park cells within 400 m, mean pollution, share of derelict neighbours), so their capitalization is emergent.

### 3.6 Decision rules (pseudocode)

#### 3.6.1 Household location choice and rental bid (bid-rent plus logit)
```
when: just arrived, lease ended (12–24 mo), utility gap high (prob. per month), job changed, evicted
sample K = 20 vacancies (half random, half near job / previous home)
for each vacancy j:
   # maximum rent keeping utility ≥ current (or outside) utility U_ref, given everything else at j
   WTP_j = solve rent: U_ij(rent) = U_ref + moving_cost_utility       # closed form for log utility:
         = budget_j − exp((U_ref − rest_j)/α_c)·P_gross  (clamped to [0, share_max·y_net])
   bid_j = min(WTP_j, ask_j·(1 + ε_market))      # ALMA-style: don't overbid when buyer's market
   V_j   = U_ij(bid_j)
choose j with prob ∝ exp(μ·V_j), including the "stay" option and the "leave city" option (U_out − migration cost)
if no affordable unit and cannot stay: → shelter (if capacity) / homeless state / leave city
```
- `U_ref` for in-migrants is `Ū_out`. That is the open-city mechanism. When a tax raises city costs, fewer people find `U ≥ Ū_out`, so population falls and bids fall. The tax then capitalizes into rents and land.
- The logit scale `μ` gives realistic noise. It is a parameter, not an outcome.

#### 3.6.2 Asset valuation (buyer's bid for a property or parcel)
```
NOI_t = expected rent·(1 − vacancy_exp) − opex − maintenance
        − τ_L·A_L − τ_B·A_B − τ_P·(A_L+A_B) − τ_H·S (if COST) − (other owner-paid taxes)
g_e   = expectation: fundamentalist → long-run local rent growth; trend-follower → last-year local price change (BoE eq.)
V     = NOI_1 / (ρ_i − g_e)            # perpetuity; cap g_e < ρ_i − 0.01
      or Σ_{t≤H} NOI_t/(1+ρ_i)^t + V_H/(1+ρ_i)^H   (for horizon H if you want a finite-horizon investor)
bid   = min(V, liquidity + mortgage_capacity) − τ_T·price_share_paid_by_buyer
```
Because `A_L` and `A_B` follow market values with a lag, the steady state is `V ≈ NOI_pre-tax/(ρ − g + τ)`. **Capitalization emerges**, and so does its *imperfection* (assessment lag, heterogeneous ρ, expectation errors). For owner-occupiers, replace rent with the dwelling's imputed rent from their WTP (§3.6.1) and use the BoE rent-vs-buy logit.

#### 3.6.3 Assessor (government)
```
yearly:
  for each parcel: market estimate = hedonic/nearest-neighbour average of recent sales (last 24–36 mo) of similar type within radius, × (1 + noise N(0, σ_assess))
  land value:  A_L = estimate from vacant-land sales nearby, else residual: max_type(V_new_building − C_build) (developer's residual)
  building:    A_B = max(0, A_total − A_L)  (property tax) | depreciated replacement cost (alt.)
  assessment ratio drift: if few sales, keep last value (lag) ⇒ regressivity can emerge
```
`σ_assess` and the lag are policy-visible parameters. With the LVT "neutral" definition (Tideman), the land value is assessed **as if vacant**. That removes the development-timing distortion. The market-value definition can be offered as an alternative to show Bentick's timing effect.

#### 3.6.4 Developer feasibility and building height (the key LVT vs property-tax mechanism)
```
for candidate parcel j (owned, or for sale at price P_j) and building type b ∈ {house, row, apt, office, mixed}:
  for floors h in 1..h_max(zoning, terrain):
     FA     = h · footprint_b
     C(h)   = c_b · FA · (1 + κ·(h−1))        # cost per m² rises with height (structure, elevators)
     R(h)   = expected rent per m² at j for type b (from recent leases nearby) · FA · (1 − vac_exp)
              · (1 − λ·max(0, h − h_market_absorption))  # optional saturation
     NOI(h) = R(h) − opex·FA − τ_B·C(h) − τ_P·(A_L + C(h)) − τ_L·A_L
     NPV(h) = NOI(h)/(ρ_dev − g_e) − C(h) − demolition(j) − P_j(if buying) − τ_T·P_j
  h*, b* = argmax NPV
  # real-option rule (Capozza–Helsley/Titman): build now only if NPV(h*) ≥ value of waiting
  wait_value = [E NPV next year] /(1+ρ_dev) − holding_cost_1yr
     where holding_cost_1yr = τ_L·A_L + τ_P·A_L + interest on land + upkeep − interim income (parking/farm)
  build if NPV(h*) > 0 and NPV(h*) ≥ wait_value
```
**Why this lets taxes emerge rather than be scripted:**
- `τ_B·C(h)` increases with h, so a building tax lowers `h*` and can push NPV below 0 (no building).
- `τ_L·A_L` does *not* depend on h, so it does not change `h*`. But it raises `holding_cost`, which lowers `wait_value` and makes "build now" more likely. This is the anti-speculation channel.
- A property tax adds both.
- `E NPV next year` uses the developer's expected growth `g_e`. Trend-followers in a boom hold land longer, so speculation emerges.
- Under COST, NOI includes `−τ_H·S` and S rises with building value. That acts like a building tax plus forced-sale risk.

#### 3.6.5 Land speculator / vacant-land holder (hold–sell–develop)
Based on Walker et al.'s benefit/cost rule, but with market prices:
```
each quarter, for owned vacant parcel j:
  appreciation_exp = g_e · MarketValue_j
  carry            = τ_L·A_L + τ_P·A_L + ρ_i·MarketValue_j − interim_income_j
  if owner is developer-capable and build condition (3.6.4) holds: develop
  elif appreciation_exp − carry > 0 and liquidity ok: hold          (→ vacant lot persists)
  else: list for sale (BoE list-price rule)
buy decision (investor with spare wealth):  buy parcel if (appreciation_exp − carry)/price > hurdle_i
```
The share of trend-followers and the hurdle rates are heterogeneous. Vacancy rates, holding durations and speculative profits are emergent outputs.

#### 3.6.6 Maintenance, decay and redevelopment
```
quality q decays δ_q per year; rent ∝ q^η
owner spends M to raise q by Δq if:  ΔRent/(ρ−g) − τ_B·ΔA_B/(ρ−g) − τ_P·ΔA_B/(ρ−g) ≥ M
teardown/redevelop when: max_h NPV_new(h) − demolition > V_existing   (Dye & McMillen teardown logic)
if q < q_derelict: unit unrentable → "boarded up" (visible), amenity penalty for neighbours (blight externality)
```
A building tax lowers the return to maintenance, so more decay emerges. LVT does not. Blight then feeds back into neighbours' amenity, so spatial clusters of decay can emerge.

#### 3.6.7 Labour supply and job search
- Hours: `h* = clamp((w·(1−τ_m)·MU_c/ψ_i)^e, 0, h_max)`. With log consumption, `MU_c = α_c/c`, so UBI raises c, lowers MU_c and lowers h (income effect). Solve by 2–3 fixed-point iterations per month.
- Participation: work if the utility with `h*` ≥ utility at h = 0 (with UBI and benefits) + fixed cost `F_i`.
- Job search (Lengnick): unemployed visit β = 5 firms. Employed visit 1 with p = 0.1 (or p = 1 if net wage is below the reservation wage). Accept if `net_wage − commute_cost` is higher.
- Reservation wage: raised to the current net wage, cut 10% per month unemployed, floored at the "leave city" alternative.

#### 3.6.8 Migration (in and out)
```
yearly (or monthly with small prob.):
  U_city_i = best achievable U_ij among K sampled (job, home) pairs, using after-tax income incl. UBI,
             minus annualized wealth-tax & expected taxes
  U_out_i  = Ū_out(skill_i) + idiosyncratic ε
  P(leave) = σ(μ_m·(U_out_i − U_city_i − movecost_i))       movecost_i ∝ attachment, kids, owner-occupier, (τ_T if selling)
inflow: candidate migrants drawn from the outside skill/wealth distribution; each evaluates U_city vs U_out the same way
```
Taxes capitalize *because* of this rule. Different groups have different `movecost` and `Ū_out(skill)`: high-skill workers have better outside options, and capital owners can leave with portable wealth. Differential flight therefore emerges. Calibration targets (not rules): long-run elasticities around 1–2 for top earners (Kleven et al. 2013; Moretti & Wilson 2017) and much lower for the general population.

#### 3.6.9 Consumption, saving, portfolio, inheritance
- Consumption: Lengnick `c = min((m/P)^α, m/P)` or a buffer-stock rule `c = y_perm + κ·(m − m_target)`. Either way, saving emerges as a function of liquidity.
- Portfolio (yearly): excess wealth above the buffer goes to the asset with the highest expected after-tax return net of risk: deposits (`r`), local property (§3.6.2), local firm equity (dividend yield + growth), or outside assets (`r_out`, untaxed by a city wealth tax if residence-based taxation does not apply). **Capital flight** is the flow into outside assets.
- Demographics: age +1 per year. Death probability rises with age. On death, the estate (net of optional inheritance tax) goes to a child household (spawned if necessary). This creates dynastic wealth persistence, the Sugarscape inheritance effect. Births and household formation: young adults split off with a draw from the parent's skill distribution plus noise (intergenerational mobility is a parameter).

#### 3.6.10 Firms
- Lengnick rules for price, wage and hiring (§2.11), plus:
- Production: `Y = A_j·L^{a}·K^{1−a}`, with `A_j = A0_s·(1 + local_employment_density_j)^{σ_agg}`. **Agglomeration**: σ_agg ≈ 0.02–0.05 (doubling city size raises productivity by about 2–7%; Combes, Duranton, Gobillon, Puga & Roux 2012, *Econometrica*; [IZA](https://docs.iza.org/dp6502.pdf)). Agglomeration makes density productive, so the density effects of LVT feed back into wages.
- Investment: add capital if `MRPK ≥ user_cost` with the corporate tax as in §1.6.
- Location: when founding or relocating, choose a commercial parcel by logit over expected after-tax profit (rent for floor space, accessibility to workers, `A_j`). Tradable firms also compare with the outside option `π_out`.
- Entry and exit: exit if cash < 0 for 3 months (bankruptcy: assets auctioned, workers laid off). Entry: when sector-wide profit rates exceed `r` + premium, a wealthy household founds a firm (equity from its wealth). This gives a cheap form of Axtell-style firm turnover.
- Emissions and abatement (§1.9).

#### 3.6.11 Government
```
revenue_t = Σ taxes by source (ledger kinds)
spending  = user-set shares of (revenue + allowed deficit):
   roads/transit  → reduce travel times on chosen links (capex + maintenance; decays without maintenance)
   parks          → convert purchased/owned parcels to park (amenity), or protect nature
   services       → service quality index per district (schools/health) in utility; cost ∝ population served
   shelters       → capacity for homeless households
   UBI/dividend   → residual = (revenue − committed spending)/eligible residents  (residency rule option)
budget rule: deficit ≤ d_max·revenue; debt at interest r from Outside World
land acquisition for public uses at market price (so a high-value city pays more for parks: emergent trade-off)
```
**Henry George test (KPI):** compare `Σ_j (R_j − R_agri)·area_j` (aggregate differential land rent) with spending on local public goods. The theorem says they are equal at optimal city size. The sim can show whether a given regime and spending mix gets close.

#### 3.6.12 Environment and nature
- Each parcel has an `eco` value (0–1). Development reduces it to near 0. Parks give a medium value. Restoration (ELVT option) raises it over years.
- Pollution field `P(x)`: sources are firms (`e`) and traffic (commute flows × car share × emission factor). Each tick: `P ← decay·blur(P) + sources`. Pollution enters utility (−π·P), health (optional death risk) and amenity.
- Green cover, nature fragmentation (number of nature patches and the largest patch size) and the ecological footprint (Σ consumption × footprint intensity + land converted) come straight from these fields.
- Sprawl: farm and nature parcels convert when developers' NPV exceeds `R_agri/(ρ−g)` plus the option value (CHALMS/Capozza–Helsley logic). Sprawl into nature is therefore an output.

### 3.7 Order of operations per monthly tick
```
1. Exogenous updates (outside utility drift, export price shocks)
2. Firms: production, inventory, Lengnick price/wage/hiring decisions; abatement
3. Labour market search & matching
4. Goods market purchases (households → vendors; outside vendor leakage)
5. Wages, profits, dividends paid (ledger); income tax withheld; VAT remitted; corporate tax accrued
6. Housing: lease expiries, location decisions (3.6.1), rental auction; sale listings, property auction (incl. land)
7. Owners: rent collection, maintenance, property/land/COST taxes (monthly installments)
8. Developers/speculators (quarterly): hold/build/sell; construction progress
9. Migration decisions (sampled subset each month)
10. Government: collect, spend, UBI payout, public works progress; assessor (yearly); wealth tax (yearly)
11. Environment: pollution diffusion, eco recovery/decay
12. Demographics (yearly): aging, deaths, inheritance, household formation
13. KPIs & shadow counterfactuals logged; ledger invariant check
```

### 3.8 Validation targets (stylized facts; for checking, never for scripting)

| Fact | Target band (approximate) | Source/notes |
|---|---|---|
| Rent/price gradient falls with distance to job centres | monotone in the homogeneous AMM test | AMM; Filatova 2009 replication |
| Rental vacancy rate | ~3–10% | typical urban ranges |
| Median rent burden (rent/income) | ~25–35% | renters bid ~33% of income (BoE) |
| Income Gini (pre-tax) | ~0.35–0.50 | OECD cities |
| Wealth Gini | ~0.6–0.85, Pareto upper tail | Bouchaud–Mézard; empirical |
| Owner sale hazard | ~1/11 per year | BoE (UK data) |
| Firm-size distribution | right-skewed | Axtell; Lengnick |
| Price-change frequency | ~10% of firms per month | Lengnick |
| Park proximity premium | single to low double-digit % | Crompton & Nicholls meta-analysis ([JLR](https://college.agrilife.org/rptsweb/wp-content/uploads/sites/21/2020/08/jlr-volume-37-number-3-pp-321-341.pdf); [ESRI Dublin](https://www.esri.ie/pubs/WP331.pdf)) |
| Tax capitalization into prices | partial to full | Oates 1969; DORS 2017; Nielsson et al. 2024 |

### 3.9 Performance budget for the browser

- 10k households × K = 20 sampled vacancies × (only ~5–10% searching per month) ≈ 20k utility evaluations per tick. Trivial.
- Developer feasibility: only for parcels flagged as candidates (vacant, for sale, or quality below threshold), ≈ a few hundred per quarter × 10 heights × 5 types.
- Pollution diffusion: one 3×3 blur on a 128² grid per tick ≈ 150k ops.
- Use a **Web Worker** for the sim, **struct-of-arrays in TypedArrays** for agents and parcels, a seeded PRNG (e.g. `xoshiro128**`) for reproducibility, and fixed-step ticks decoupled from rendering. Target ≥ 10 sim-months per second at 10k households.
- Determinism: same seed + same policy gives an identical run. That makes **paired comparisons** possible: run the same seed under two regimes and difference the KPIs, which removes most noise.

---

## 4. KPIs and how to compute them

All computed from agent and parcel state. "HH" = household. Weight by persons where noted.

| KPI | Computation | Notes / display |
|---|---|---|
| **Population** | Σ persons in resident HH | Time series; in/out-migration flows separately |
| **Net migration by decile** | arrivals − departures, by income and wealth decile | shows *who* leaves under each tax |
| **Income Gini (pre- and post-tax-and-transfer)** | Gini over HH equivalized income (`income/√size`): sort ascending, `G = (2Σ i·x_i)/(nΣx_i) − (n+1)/n` | two lines: market vs disposable; the gap = redistribution |
| **Wealth Gini; top 1% / 10% share** | same over net wealth (allow negatives: use the standard formula on shifted values or report the share metrics) | plus Pareto tail exponent via Hill estimator on top 5% |
| **Land Gini / ownership concentration** | Gini over land value owned per HH (non-owners = 0); share of land value owned by top 1% | Georgist headline |
| **Homeownership rate** | owner-occupier HH / HH | |
| **Vacancy rate** (housing; commercial) | vacant units / total units; also *vacant land* share of urban parcels and *derelict* units | split "frictional" (vacant < 3 months) vs "long-term" |
| **Speculative vacancy** | vacant parcels whose owner's latest decision was "hold for appreciation" | directly logged from §3.6.5 |
| **Rent burden** | median and share > 30% / > 50% of (rent / disposable income) for renters | |
| **Housing affordability ratio** | median dwelling price / median HH disposable income | |
| **Homelessness** | HH in "homeless" state; shelter occupancy | |
| **Overcrowding / mismatch** | persons per room > threshold; owners in mismatched size; long commutes caused by lock-in (owners whose best alternative beats current by > tax) | lock-in from transfer tax |
| **Unemployment rate** | unemployed / labour force (participating) | plus participation rate and average hours |
| **GDP proxy (city product)** | Σ firm value added = Σ (revenue − purchased inputs) + imputed rent of owner-occupied housing + government services at cost | also per capita |
| **Mean real wage, real disposable income** | deflate by consumption price index (Σ basket × gross prices) | |
| **Tax revenue by source** | sum of ledger flows by kind per year | stacked area; plus revenue/GDP |
| **Effective tax rates by decile** | taxes paid (incl. VAT and property tax incidence on owner; rent burden for renters) / income | progressivity curve |
| **Deadweight-loss proxies (shadow counterfactuals)** | (a) *Labour:* Σ_i ½·(h_i^{no tax} − h_i)·w_i·τ_m,i. (b) *Blocked trades:* in each auction, count pairs where `bid ≥ ask` before tax but not after; lost surplus = Σ(bid − ask). (c) *Unbuilt floor area:* Σ_j (FA*(no τ_B) − FA*(actual)) over feasibility evaluations. (d) *Lock-in moves:* moves not taken where `ΔU·horizon > movecost` but `< movecost + τ_T·P`. | Each agent computes its no-tax decision alongside its real one (cheap). Report the sum in money and as % of revenue: *excess burden per unit of revenue* |
| **Capital flight** | yearly Σ wealth taken by emigrants + net flow into outside assets | split by decile |
| **Firm count, entries, exits, relocations** | by sector | relocations out = corporate-tax flight |
| **Construction** | floor area completed per year; mean building height; floor-area ratio by ring | |
| **Urban footprint / sprawl** | urban parcels count; mean distance of developed parcels from centre; leapfrog index (share of new development with no developed neighbour) | |
| **Density** | persons per urban hectare; jobs per urban hectare | |
| **Green cover & nature** | share of parcels with eco > 0.5; largest nature patch; # patches (fragmentation) | |
| **Ecological footprint** | Σ (consumption × intensity) + converted nature area + emissions × factor | per capita |
| **Pollution exposure** | population-weighted mean P; share of HH above threshold; exposure by income decile (environmental justice) | |
| **Commute** | mean commute time and distance; mode share | |
| **Land value total and land share** | Σ market land values; land value / total property value | |
| **Capitalization index** | after a tax change: Δ(land price) / PV(Δ tax bill) per parcel, averaged | 1 = full capitalization |
| **Henry George gap** | (aggregate differential land rent − local public-good spending) / spending | 0 at HG optimum |
| **Government balance, debt, UBI per person** | ledger | |
| **Assessment quality** | coefficient of dispersion (COD) of assessed / sale price; price-related differential (PRD) for regressivity | standard assessor metrics |
| **Wellbeing** | mean and distribution of HH utility in money-metric (equivalent variation versus baseline run) | welfare comparison across regimes |
| **Economic mobility** | rank correlation of parent/child wealth deciles | inheritance effects |

Dashboard tip: show every KPI as **regime A minus regime B under the same seed**, with a band from several seeds.

---

## 5. Making effects visible in the 3D world

General rule: every visual is a **direct rendering of state** (parcel use, quality, vacancy flag, agent state, flows), never a separate "effect" layer driven by the tax setting.

| Phenomenon (from state) | Visual idea | Expected under (if it emerges) |
|---|---|---|
| Vacant parcel held for appreciation | Weedy lot, chain-link fence, "Land for sale – investors" billboard, pile of rubble | Low LVT / no LVT, booms with trend-followers |
| Vacant lots getting developed | Cranes, scaffolding, construction fences, dust | LVT introduced or raised; building tax cut |
| Building height / FAR | Real extruded heights; skyline ring profile | Higher with LVT/split-rate; lower with building tax/COST |
| Poor maintenance (low q) | Peeling façades, broken windows, darker textures, graffiti; boarded windows when derelict | Building tax, COST (weaker investment incentive), tenant-poor areas |
| Teardown and redevelopment | Demolition animation, then new taller structure | High land values + low building tax |
| Sprawl into nature/farmland | Forest tiles replaced by cul-de-sacs; tree count falls; farm tiles go | Low density regimes, cheap outskirts land |
| Parks and green | Trees and parks with people walking; birds; greener tint near parks | Government spending (HG recycling), ELVT |
| Pollution | Smog haze volume over the grid (density = P field); factory smokestacks; grey tint; people with masks at high P | Absent Pigouvian tax; reduced with carbon tax (abatement filters on stacks) |
| Commute mode | Cars vs buses vs bikes on roads proportional to mode flows; traffic jams | Carbon/fuel tax, transit spending |
| Firm closures | "Closed", "For lease" signs, boarded-up shopfronts, empty parking lots | High corporate tax (tradables leave), VAT near city edge (shoppers go outside) |
| Firms relocating out | Trucks loading at factory → driving off map edge | Corporate tax, carbon tax leakage |
| Out-migration | Moving vans heading to the city edge; occupancy lights turning off in windows | Any tax raising costs above outside utility; luxury vans for wealth-tax flight |
| In-migration | Cars with luggage arriving; lights turning on | UBI, good public services, low rents |
| Homelessness | Tents in parks / under bridges; shelter queues | High rent burden, low shelter funding |
| Overcrowding | More silhouettes per window; laundry on balconies | High rent burden |
| Transaction activity | "SOLD" signs pop up; flicker of "For sale" boards | Low with high transfer tax (lock-in); very high with COST (forced sales) |
| COST forced sale | Brief "Bought at declared price" notice; family carrying boxes out | COST |
| Wealth-tax fire sales | Clusters of "Urgent sale" signs on large villas; yacht/sports-car count dropping | Wealth tax with illiquid owners |
| Prosperity | Café tables, street life density ∝ local spending; shop window lights | High disposable income, agglomeration |
| Inequality | Toggle overlay: colour buildings by occupants' income decile; gated villas vs dense blocks | Schelling-type sorting |
| Land value heatmap | Toggle overlay: height-field or colour of land value per m² | Capitalization of transit and parks visible as "hills" |
| Tax flows | Animated coins from parcels/people to city hall, then to parks/roads/UBI ("dividend rain" to homes) | All regimes; makes recycling visible |

---

## 6. Feature cut-off: MVP vs later phases vs excluded

### 6.1 MVP (phase 1): enough to compare LVT, property tax, income tax, VAT and UBI honestly
- Grid world (≈96×96), one CBD plus a few job clusters, a static road network, nature and farm tiles.
- Households: rent vs own (BoE logit), location choice (sampled logit, bid-rent), labour supply (hours + participation), Lengnick consumption, migration in and out against `Ū_out`.
- Firms: Lengnick goods and labour markets. Two sectors (tradable export, local non-tradable). Fixed construction capacity.
- Developers and speculators: hold/develop/sell, NPV-max height, real-option rule; fundamentalist and trend-follower expectations.
- Housing, rental and land double auctions with list-price decay.
- Assessor with lag and noise; split land/building assessment.
- Taxes: LVT, property tax, split-rate, flat and progressive income tax, VAT/sales, transfer tax, UBI recycling.
- Government: roads maintenance, parks, generic services index, shelters, UBI.
- Environment: eco value, sprawl conversion, green cover; simple pollution from firms.
- Ledger invariant; deterministic seeds; paired-run comparison dashboard; core KPIs (population, Ginis, vacancy, rent burden, homelessness, revenue by source, DWL proxies, footprint, land value).
- 3D: building heights, vacant/derelict states, construction, tents, smog, moving vans.

### 6.2 Phase 2: wealth, firms, COST, Pigou
- Wealth tax with liquidation ladder, portfolio choice including outside assets, capital flight KPI.
- Corporate tax with investment (user cost), firm location and relocation, firm entry and exit.
- Harberger/COST: declared values, forced sales, investment decisions.
- Carbon/pollution tax with abatement, commute mode choice, pollution diffusion and health.
- Demographics: aging, deaths, inheritance, household formation, intergenerational mobility.
- Mortgages with LTV/DTI caps (still fixed rate). Leverage-driven cycles (Geanakoplos/Axtell).
- Income avoidance and evasion (self-employed underreporting, audits).
- Transit investment (new lines changing travel times), zoning tool.

### 6.3 Phase 3: depth and sandbox
- Axtell-style endogenous teams/firms; more sectors with input-output links.
- Multiple jurisdictions (city + suburbs) with **tax competition** (Tiebout sorting between municipalities): the cleanest way to show capital and people moving between regimes.
- ELVT (ecology-weighted LVT), congestion pricing, betterment levies.
- Elections or political feedback (voters' preferences over regimes), optional.
- Scenario editor and calibration to a real city's distributions.

### 6.4 Explicitly excluded (with reasons)
| Excluded | Reason |
|---|---|
| **Endogenous money, commercial banking, central bank, monetary policy** | Biggest source of complexity and fragility in macro ABMs (EURACE, Mark-0 phase transitions). Not needed to compare *tax* regimes at city scale. A fixed outside interest rate keeps the asset-pricing channel without the instability. |
| **International trade / exchange rates** | A city is a small open economy. The Outside World with exogenous prices, wages and returns captures everything that matters (export demand, leakage, flight). |
| **National macro feedbacks (federal taxes, national business cycle)** | Can be an exogenous drift on `Ū_out`, `w_out`, `p_export`. Endogenizing it requires modelling the whole country. |
| **Full financial markets (stock trading, derivatives, credit spreads)** | Equity at book value is enough for dividend and wealth-tax channels. Trading markets add noise without improving tax comparisons. |
| **Detailed household micro-behaviour (education choices, marriage markets, health care)** | Each would need its own literature-calibrated model. They are second-order for tax incidence. Use services index and fixed demographic rates. |
| **Detailed traffic microsimulation** | Use a static assignment (shortest path with congestion BPR factor recomputed yearly). Car-level traffic simulation is costly and not needed for land values. |
| **Political-economy realism (lobbying, corruption, tax evasion networks)** | Value-laden and hard to validate. At most a parameterized evasion rate. |
| **Optimizing (rational-expectations) agents** | Computationally infeasible and against the emergence goal. Bounded rationality with sampled choices and adaptive expectations is the ABM standard. |
| **Hard-coded empirical elasticities as behaviour** | Violates the core principle. Use them only as calibration and validation targets. |

---

## 7. Validation, pitfalls and invariance tests

**Mechanics tests (should pass by construction; automated):**
1. *Lump-sum neutrality*: replacing any tax with an equal-revenue lump-sum tax per household must change no hours, heights or location decisions in the first tick (shadow comparison).
2. *Pure LVT height neutrality*: with `τ_B = 0`, perfect assessment of land "as if vacant", and a fixed land price, `argmax_h NPV(h)` is independent of `τ_L`.
3. *Building tax monotonicity*: `h*` is non-increasing in `τ_B` in the developer function.
4. *Capitalization check*: in a static test world, after announcing a permanent `Δτ_L`, the mean land bid falls by ≈ PV(Δtax) for the representative ρ (within noise).
5. *Ledger conservation*: money invariant holds every tick.
6. *AMM replication*: homogeneous households, single CBD, no taxes: rent decreases monotonically with distance; edge where urban bids equal `R_agri`.
7. *Transfer-tax wedge*: with `τ_T > 0`, the number of executed trades in a fixed-bid test market is weakly lower.

**Pitfalls to avoid:**
- **Formula-based land prices** (Walker et al. limitation): they kill capitalization.
- **Circular assessment** (tax based on value, value based on after-tax bids): stable only with a lag and partial adjustment. Assess on trailing 2–3 year sales.
- **Knife-edge regimes** (Mark-0): sweep key adjustment parameters, and report a comparison only if its sign is robust across the sweep and across seeds.
- **Missing margins**: e.g. inelastic labour makes income tax look free, and no outside option makes all taxes look free to landowners. Check the table in §1.11.
- **Revenue-neutral comparisons**: compare regimes at **equal revenue** (or equal public-good provision), otherwise you compare spending levels, not taxes. Provide a "revenue-matching" mode that solves for the rate, by bisection over short runs, that yields the target revenue.
- **Transition vs steady state**: capitalization makes the *announcement* winners and losers differ from steady-state ones. Show both: "first 5 years" and "after 30 years".
- **Over-interpreting magnitudes**: the simulator shows *mechanisms and their interactions*. Label quantitative results as model outputs, not forecasts.

---

## 8. References (URLs checked during this research)

Land and property taxation
- Walker, Mertan, Farley, Rizzo & Reynolds (2024). Land Stewardship and Development Behaviors Under an Ecological-Impact-Weighted LVT: A Proof-of-Concept ABM. *Land* 13(11):1795. https://doi.org/10.3390/land13111795 · code https://github.com/DakotaBWalker/Speculation-and-Land-Tax-ABM
- Oates & Schwab (1997). The impact of urban land taxation: the Pittsburgh experience. *National Tax Journal* 50(1). (Summaries: https://schalkenbach.org/wp-content/uploads/What-happened-when-PA-taxed-land.pdf)
- Banzhaf & Lavery (2010). Can the land tax help curb urban sprawl? *JUE* 67(2). WP: https://www.lincolninst.edu/app/uploads/2024/04/banzhaf-wp08sb1.pdf
- Brueckner & Kim (2003). Urban sprawl and the property tax. *ITPF* 10(1). https://ideas.repec.org/a/kap/itaxpf/v10y2003i1p5-23.html
- Song & Zenou (2006). Property tax and urban sprawl. https://www.lincolninst.edu/app/uploads/2024/04/1095_zenou_final.pdf
- Oates (1969). Effects of property taxes and local public spending on property values. *JPE* 77(6). https://www.Gwern.net/doc/economics/georgism/1969-oates.pdf
- Arnott & Stiglitz (1979). Aggregate land rents, expenditure on public goods, and optimal city size. *QJE* 93(4). https://academiccommons.columbia.edu/doi/10.7916/D8086FW3/download ; overview https://en.wikipedia.org/wiki/Henry_George_theorem
- Bentick (1979) *JPE* 87(4); Mills (1981) https://www.Gwern.net/doc/economics/georgism/1981-mills.pdf ; Arnott (2005) Neutral property taxation https://ideas.repec.org/a/bla/jpbect/v7y2005i1p27-50.html ; Lincoln WP on development timing https://www.lincolninst.edu/app/uploads/2024/04/grote-wp08kg1.pdf
- Capozza & Helsley (1990). The stochastic city. *JUE* 28(2). https://deepblue.lib.umich.edu/items/c9ab7d24-f603-4959-95c3-6101cb8cfff6
- Bowman et al. Methods of valuing land for real property taxation (Lincoln Institute). https://www.lincolninst.edu/app/uploads/2024/04/1120_Bowman_complete_web.pdf
- Alvayay & Skidmore. Valuing land in Detroit using the option value approach. https://www.lincolninst.edu/app/uploads/2024/04/Alvayay-Torrejon_Skidmore-Valuing-Land-in-Detroit-Using-the-Option-Value-Approach.pdf
- Lincoln Institute (2022). Split-rate property taxation in Detroit. https://www.lincolninst.edu/publications/other/split-rate-property-taxation-in-detroit/ ; CRC Michigan critique https://crcmich.org/?p=13583
- Danish Economic Councils (2017), land tax capitalization working paper: https://savearchive.zbw.eu/bitstream/11159/1082/1/arbejdspapir_land_tax.pdf
- Mirrlees et al. (2011). *Tax by Design*, ch. 16. https://ifs.org.uk/books/tax-design

Transfer taxes
- Best & Kleven (2018). Housing market responses to transaction taxes. *REStud* 85(1). https://ideas.repec.org/a/oup/restud/v85y2018i1p157-193..html
- Hilber & Lyytikäinen (2017). Transfer taxes and household mobility. *JUE*. https://researchonline.lse.ac.uk/id/eprint/65022
- Eerola, Harjunen, Lyytikäinen & Saarimaa. https://vatt.fi/-/effects-of-real-estate-transfer-taxes-evidence-from-a-natural-experiment
- Duranton, Han & Turner. Massachusetts transfer tax. https://realestate.wharton.upenn.edu/working-papers/the-negative-impact-of-a-massachusetts-realestate-transfer-tax-on-fiscal-revenue-andhousehold-mobility/

Income, corporate, consumption taxes
- Saez, Slemrod & Giertz (2012). ETI review. *JEL* 50(1). https://eml.berkeley.edu/~saez/saez-slemrod-giertzJEL09elasticity.pdf
- Diamond & Saez (2011) optimal top rate; discussion https://www.cato.org/cato-journal/fall-2019/optimal-top-tax-rates-review-critique , https://www.epi.org/blog/high-top-income-tax-rates-fight/
- Kleven, Landais & Saez (2013). Taxation and international migration of superstars. *AER*. https://eml.berkeley.edu/~saez/kleven-landais-saezAER13football.pdf
- Moretti & Wilson (2017). State taxes and star scientists. *AER* 107(7). https://eml.berkeley.edu/~moretti/taxes.pdf
- Fuest, Peichl & Siegloch (2018). Do higher corporate taxes reduce wages? *AER*. https://conference.nber.org/conf_papers/f91099.pdf
- Poterba (1989/1993). Is a VAT progressive? annual vs lifetime. https://nber.org/papers/w4387 ; Caspersen & Metcalf https://www.nber.org/papers/w4619.pdf
- Deadweight loss ∝ t²: https://www.econlib.org/deadweight-loss-from-taxes-is-proportional-to-the-square-of-the-tax-rate/ ; Auerbach notes https://eml.berkeley.edu/~webfac/auerbach/e230a_f19/Lecture%20Note%204.19.pdf

Wealth taxes
- Brülhart, Gruber, Krapf & Schmidheiny (2022). Behavioral responses to wealth taxes: Switzerland. *AEJ:Policy*. https://edoc.unibas.ch/90165/1/pol.20200258.pdf ; https://cepr.org/voxeu/columns/wealth-taxation-swiss-experience
- OECD (2018). The role and design of net wealth taxes in the OECD. https://www.oecd.org/en/publications/the-role-and-design-of-net-wealth-taxes-in-the-oecd_9789264290303-en.html
- Advani & Tarrant. Liquidity issues: solutions for the asset rich, cash poor. https://researchonline.lse.ac.uk/id/eprint/113308 ; valuation https://dx.doi.org/10.47445/125
- Norway 2022–23 departures and confounders: https://arxiv.org/pdf/2603.05277 ; https://www.thelocal.no/20251023/how-norways-wealth-tax-drove-out-the-rich-without-breaking-the-bank

Harberger / COST
- Posner & Weyl, Property is only another name for monopoly. https://www.law.nyu.edu/sites/default/files/upload_documents/Property%20Monopoly.pdf ; https://en.wikipedia.org/wiki/Harberger_Tax
- Rallo (2019) critique. https://ideas.repec.org/a/kap/ejlwec/v47y2019i1d10.1007_s10657-018-9607-6.html
- DeSantis, McCarter & Winn (2022) lab experiments. https://emerald.com/insight/content/doi/10.1108/S0193-230620220000021004

UBI and Pigouvian recycling
- Jones & Marinescu (2022). Alaska Permanent Fund. *AEJ:Policy*. https://home.uchicago.edu/j1s/Jones_Alaska_2022.pdf
- Finland basic income experiment final results (2020). https://stm.fi/en/-/perustulokokeilun-tulokset-tyollisyysvaikutukset-vahaisia-toimeentulo-ja-psyykkinen-terveys-koettiin-paremmaksi
- Azevedo, Wolff & Yamazaki. BC carbon tax and jobs. https://researchonline.lse.ac.uk/id/eprint/117346 ; World Bank BC case https://transparency-partnership.net/system/files/migrated_document_files/britishcolumbiataxingcarbonfordevelopmentworldbank2023.pdf ; distribution https://dl.tufts.edu/concern/pdfs/70795p10f

Agent-based and urban models
- Filatova, Parker & van der Veen (2009). ALMA. *JASSS* 12(1)3. https://www.jasss.org/12/1/3.html ; notebook https://nbviewer.cgwebhost.cigi.illinois.edu/github/cybergis/ALMANotebook/blob/master/ALMA.ipynb
- Parker & Filatova (2008) conceptual bilateral land market. https://www.canr.msu.edu/resources/conceptual-design-bilateral-agent-based-land-market-heterogeneous-economic-agents
- Magliocca et al. (2011) CHALMS. https://catalog.comses.net/publications/2419 ; https://miracle.comses.net/codebases/3368/
- Waddell (2002) UrbanSim; location choice logit https://www.strc.ch/2006/Buergle_STRC_2006.pdf , https://strc.ch/2005/Waddell.pdf
- Schelling (1971). https://uzh.ch/cmsssl/suz/dam/jcr:00000000-68cb-72db-ffff-ffffff8071db/04.02%7B_%7Dschelling%7B_%7D71.pdf
- Axtell, Emergence of firms. https://www.brookings.edu/wp-content/uploads/2016/06/firms.pdf ; https://www.comses.net/codebases/4948/releases/1.2.0/
- Epstein & Axtell (1996). *Growing Artificial Societies*. https://www.brookings.edu/books/growing-artificial-societies
- Yard-sale model https://en.wikipedia.org/wiki/Yard-sale_model ; Boghosian et al. https://arxiv.org/pdf/2310.16098 , https://arxiv.org/html/2307.15513v2 ; Bouchaud & Mézard (2000) https://arxiv.org/abs/cond-mat/0002374
- Lengnick (2013). Agent-based macroeconomics: a baseline model. *JEBO* 86. https://legacy.econ.tuwien.ac.at/lva/compeco.se/artikel/jebo_2013_agent_based_macroeconomics_a_baseline_model.pdf
- Gualdi, Tarzia, Zamponi & Bouchaud (2015). Tipping points in macroeconomic ABMs. *JEDC*. https://arxiv.org/abs/1307.5319
- Axtell, Farmer, Geanakoplos, Howitt et al. Housing ABM Washington DC. https://cowles.yale.edu/sites/default/files/2022-08/d1852.pdf ; https://www.bundesbank.de/resource/blob/636008/c54ccc1e138b5af05f970c50a97b22a3/mL/2014-06-05-eltville-10-axtell-data.pdf
- Baptista et al. (2016). Macroprudential policy in an ABM of the UK housing market. BoE SWP 619. https://www.bankofengland.co.uk/-/media/boe/files/working-paper/2016/macroprudential-policy-in-an-agent-based-model-of-the-uk-housing-market.pdf
- Deissenberg, van der Hoog & Dawid (2008). EURACE. https://halshs.archives-ouvertes.fr/halshs-00339756v1/file/DT2008-39.pdf ; Erlingsson et al. housing in Eurace https://ideas.repec.org/p/zbw/ifwedp/201332.html
- AMM model lecture notes https://mycourses.aalto.fi/pluginfile.php/1427767/mod_resource/content/2/Urban_Econ_Lec2.pdf ; test https://arxiv.org/pdf/2111.02112
- Combes, Duranton, Gobillon, Puga & Roux (2012). Productivity advantages of large cities. *Econometrica* 80(6). https://docs.iza.org/dp6502.pdf
- Park proximity premiums: Crompton & Nicholls https://college.agrilife.org/rptsweb/wp-content/uploads/sites/21/2020/08/jlr-volume-37-number-3-pp-321-341.pdf ; Dublin https://www.esri.ie/pubs/WP331.pdf

*Not verified in this session (cited from background knowledge; check before quoting numbers):* Benzarti, Carloni, Harju & Kosonen (2020, *JPE*) on asymmetric VAT pass-through. Benzarti & Carloni (2019) on consumption tax exemptions. Titman (1985, *AER*) on urban land prices under uncertainty. Dye & McMillen (2007, *JUE*) on teardowns. Plassmann & Tideman (2000). Harberger (1962) on corporate tax incidence. Weyl & Zhang on depreciating licenses. Chetty et al. on extensive/intensive labour supply elasticities.
