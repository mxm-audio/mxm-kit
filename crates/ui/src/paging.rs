//! Category-first paging above [`crate::flow`], with no window or parameter dependencies.
//!
//! This is the planner, not an editor renderer. Heights are requested at the actual candidate
//! row widths. Missing measurements are returned explicitly; they never stand for zero. Callers
//! must obtain them without editing parameters or consuming telemetry, including for hidden cards.
//! The algorithm is ordered greedy packing (whole category, kind run, preferred group, card),
//! not optimal bin packing: preserving a learnable sequence matters more than saving one tab.

use crate::flow::{self, Card};

/// Collection ordering. These discriminants are NOT developer CC or controller-page numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    Performance,
    Modulators,
    Sequencers,
    Generators,
    Tone,
    Effects,
}

impl Category {
    /// Full category names, as approved for the initial editor rollout.
    pub const fn title(self) -> &'static str {
        match self {
            Self::Performance => "Performance",
            Self::Modulators => "Modulators",
            Self::Sequencers => "Sequencers",
            Self::Generators => "Generators",
            Self::Tone => "Tone",
            Self::Effects => "Effects",
        }
    }
}

/// Caller-owned stable identity; never a page number, row number or current array index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Key(pub u64);

/// Developer navigation is separate from permanent controller/CLAP parameter pages.
/// Values 0–5 address categories; 127 is the hidden Parameters surface; other values are ignored.
pub const PARAMETERS: usize = 127;
impl Category {
    pub const fn from_request(value: usize) -> Option<Self> {
        match value {
            0 => Some(Self::Performance),
            1 => Some(Self::Modulators),
            2 => Some(Self::Sequencers),
            3 => Some(Self::Generators),
            4 => Some(Self::Tone),
            5 => Some(Self::Effects),
            _ => None,
        }
    }
}

pub mod editor;

#[derive(Clone, Copy, Debug)]
pub struct Item<'a> {
    pub key: Key,
    pub card: Card<'a>,
    pub category: Category,
    /// Name for a contiguous run when its category must split, e.g. "LFO" or "Envelope".
    /// Does not reorder cards with interleaved kinds.
    pub kind: &'a str,
}

#[derive(Clone, Copy, Debug)]
pub struct Budget {
    /// Workspace after app bar and outer gutters, but BEFORE the view bar.
    pub width: f32,
    pub height: f32,
    pub ceiling: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Measurement {
    pub key: Key,
    pub width: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub cards: Vec<Key>,
    pub label: String,
    /// Only a page holding an indivisible card may overflow — with whatever fits beside that card
    /// without making the page taller. Render this page with both-axis scrolling.
    pub overflow: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub pages: Vec<Page>,
    /// May conservatively exceed the labels' current required height, to break bar feedback.
    pub bar_height: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Ready(Plan),
    /// Keep the existing plan while fulfilling these; cold start needs an explicit measuring UI.
    Pending(Vec<Measurement>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidBudget,
    InvalidCard,
    DuplicateKey,
    InvalidGroup,
    InvalidMeasurement,
    InvalidBarHeight,
    /// All wrapped rows would leave no workspace. The renderer must offer compact navigation.
    NavigationExhausted,
    NonConvergentBar,
}

/// A pure solve. `groups` contain stable card keys, not positions. Missing cards are singletons;
/// groups must be disjoint, contiguous in category order, and belong to a single category.
///
/// `height` supplies a natural OUTER card height at the requested OUTER width, for the current
/// content/font generation. `bar_height` measures the exact proposed labels including all rows
/// and gaps. It must return zero for zero/one pages. Neither callback may change UI/audio state.
/// The number of reservation increases is bounded by card count plus one; failure is explicit.
pub fn plan(
    items: &[Item<'_>],
    groups: &[&[Key]],
    budget: Budget,
    mut height: impl FnMut(Measurement) -> Option<f32>,
    mut bar_height: impl FnMut(&[String]) -> f32,
) -> Result<Outcome, Error> {
    validate(items, groups, budget)?;
    if items.is_empty() {
        return Ok(Outcome::Ready(Plan::default()));
    }
    let mut order: Vec<_> = (0..items.len()).collect();
    order.sort_by_key(|index| items[*index].category); // stable within each category
    let mut solver = Solver {
        items,
        cards: items.iter().map(|item| item.card).collect(),
        groups,
        budget,
        height: &mut height,
        missing: Vec::new(),
    };
    let whole = solver.fits(&order, budget.height)?;
    if !solver.missing.is_empty() {
        return Ok(Outcome::Pending(solver.missing));
    }
    if whole {
        return Ok(Outcome::Ready(Plan {
            pages: vec![Page {
                cards: order.iter().map(|i| items[*i].key).collect(),
                label: label(items, &order),
                overflow: false,
            }],
            bar_height: 0.0,
        }));
    }

    let mut reserved = 0.0_f32;
    for _ in 0..=items.len() + 1 {
        let indices = solver.partition(&order, budget.height - reserved)?;
        if !solver.missing.is_empty() {
            return Ok(Outcome::Pending(solver.missing));
        }
        let mut labels: Vec<_> = indices.iter().map(|page| label(items, page)).collect();
        // Repeated kinds/categories need distinguishable navigation targets.
        let originals = labels.clone();
        for (i, name) in labels.iter_mut().enumerate() {
            if originals
                .iter()
                .filter(|other| *other == &originals[i])
                .count()
                > 1
            {
                let ordinal = originals[..=i]
                    .iter()
                    .filter(|other| *other == &originals[i])
                    .count();
                *name = format!("{name} {ordinal}");
            }
        }
        let needed = if labels.len() <= 1 {
            0.0
        } else {
            bar_height(&labels)
        };
        if !needed.is_finite() || needed < 0.0 || (labels.len() > 1 && needed == 0.0) {
            return Err(Error::InvalidBarHeight);
        }
        if needed >= budget.height {
            return Err(Error::NavigationExhausted);
        }
        if needed > reserved {
            reserved = needed;
            continue;
        }
        let mut pages = Vec::new();
        for (page, label) in indices.into_iter().zip(labels) {
            let overflow = !solver.fits(&page, budget.height - reserved)?;
            pages.push(Page {
                cards: page.iter().map(|i| items[*i].key).collect(),
                label,
                overflow,
            });
        }
        return if solver.missing.is_empty() {
            Ok(Outcome::Ready(Plan {
                pages,
                bar_height: reserved,
            }))
        } else {
            Ok(Outcome::Pending(solver.missing))
        };
    }
    Err(Error::NonConvergentBar)
}

fn validate(items: &[Item<'_>], groups: &[&[Key]], budget: Budget) -> Result<(), Error> {
    if !budget.width.is_finite()
        || budget.width <= 0.0
        || !budget.height.is_finite()
        || budget.height <= 0.0
        || budget.ceiling.is_some_and(|c| !c.is_finite() || c <= 0.0)
    {
        return Err(Error::InvalidBudget);
    }
    let mut order: Vec<_> = items.iter().collect();
    order.sort_by_key(|item| item.category);
    for (i, item) in items.iter().enumerate() {
        if !item.card.floor.is_finite()
            || item.card.floor <= 0.0
            || item.kind.is_empty()
            || item.card.title.is_empty()
            || item
                .card
                .ceiling
                .or(budget.ceiling)
                .is_some_and(|c| !c.is_finite() || c < item.card.floor)
        {
            return Err(Error::InvalidCard);
        }
        if items[..i].iter().any(|prior| prior.key == item.key) {
            return Err(Error::DuplicateKey);
        }
    }
    let mut used = Vec::new();
    for group in groups {
        let Some(first) = group
            .first()
            .and_then(|key| order.iter().position(|item| item.key == *key))
        else {
            return Err(Error::InvalidGroup);
        };
        for (offset, key) in group.iter().enumerate() {
            if used.contains(key)
                || order
                    .get(first + offset)
                    .is_none_or(|item| item.key != *key || item.category != order[first].category)
            {
                return Err(Error::InvalidGroup);
            }
            used.push(*key);
        }
    }
    Ok(())
}

struct Solver<'a, 'b, F> {
    items: &'a [Item<'a>],
    cards: Vec<Card<'a>>,
    groups: &'a [&'a [Key]],
    budget: Budget,
    height: &'b mut F,
    missing: Vec<Measurement>,
}

impl<F: FnMut(Measurement) -> Option<f32>> Solver<'_, '_, F> {
    fn units(&self, indices: &[usize]) -> Vec<Vec<usize>> {
        let mut units = Vec::new();
        let mut at = 0;
        while at < indices.len() {
            let index = indices[at];
            let len = self
                .groups
                .iter()
                .find(|group| group.first() == Some(&self.items[index].key))
                .filter(|group| {
                    group.len() <= indices.len() - at
                        && group
                            .iter()
                            .zip(&indices[at..])
                            .all(|(key, i)| *key == self.items[*i].key)
                })
                .map_or(1, |group| group.len());
            units.push(indices[at..at + len].to_vec());
            at += len;
        }
        units
    }

    fn fits(&mut self, indices: &[usize], budget: f32) -> Result<bool, Error> {
        Ok(self
            .measure(indices)?
            .is_some_and(|(width_fits, height, _)| width_fits && height <= budget))
    }

    /// How `indices` lays out on a page: whether every row fits the width, the page's height, and
    /// how many rows it takes. `None` while a card's height at its width is still unknown.
    fn measure(&mut self, indices: &[usize]) -> Result<Option<(bool, f32, usize)>, Error> {
        let units = self.units(indices);
        let groups: Vec<_> = units.iter().map(Vec::as_slice).collect();
        let rows = flow::pack(&self.cards, &groups, self.budget.width);
        let row_count = rows.len();
        let mut total = flow::GAP * rows.len().saturating_sub(1) as f32;
        let mut known = true;
        let mut width_fits = true;
        for row in rows {
            let widths =
                flow::row_widths(&self.cards, &row, self.budget.width, self.budget.ceiling);
            // A row that fills its budget sums to it only up to float rounding: flex grow shares
            // the spare width in `f32`, and 1896 plus an ulp is still a row that fits.
            width_fits &= widths.iter().sum::<f32>()
                + flow::GAP * row.len().saturating_sub(1) as f32
                <= self.budget.width + 0.01;
            let mut tallest = 0.0_f32;
            for (index, width) in row.into_iter().zip(widths) {
                let request = Measurement {
                    key: self.items[index].key,
                    width,
                };
                match (self.height)(request) {
                    Some(value) if value.is_finite() && value >= 0.0 => {
                        tallest = tallest.max(value)
                    }
                    Some(_) => return Err(Error::InvalidMeasurement),
                    None => {
                        known = false;
                        if !self.missing.contains(&request) {
                            self.missing.push(request);
                        }
                    }
                }
            }
            total += tallest;
        }
        Ok(known.then_some((width_fits, total, row_count)))
    }

    /// **A page that has to scroll anyway takes what fits beside its tall card** (the owner,
    /// 2026-09-28, on mxm-mono-00 in a short window: *why does it split up like this, with two
    /// tabs each carrying a single oscillator?*). `current` already overflows the height it is
    /// given — a card is taller than any page — so `candidate` joins it when every row still fits
    /// the width and **it adds no row**: the joining card sits beside the tall one, so the page
    /// scrolls no further than its tallest card needs wherever it went. The second oscillator
    /// beside the first costs nothing, where a tab of its own costs a page. A first build asked
    /// that the page grow no taller at all, and mono-00's Oscillator 2, a little taller than
    /// Oscillator 1, still took a tab.
    fn rides_along(
        &mut self,
        current: &[usize],
        candidate: &[usize],
        budget: f32,
    ) -> Result<bool, Error> {
        let Some((_, height, rows)) = self.measure(current)? else {
            return Ok(false);
        };
        if height <= budget {
            return Ok(false);
        }
        Ok(self
            .measure(candidate)?
            .is_some_and(|(width_fits, _, joined)| width_fits && joined == rows))
    }

    /// Whole categories where that costs nothing, and pages filled where it does.
    ///
    /// **Fewer pages first, then whole categories** (the owner, 2026-09-24, ruling mxm-para-07's
    /// opening: *"The small version is not my 1/4 4K hugging rule"*). Category-first alone put
    /// Performance's three cards on a page of their own once the Modulators fitted one page whole,
    /// and a window hugged to that page was a third of the budget with seven tabs. So both
    /// partitions are made — one from whole categories, splitting only an oversized one, and one
    /// filling each page in order from preferred groups and single cards — and the category-first
    /// one is kept unless the filled one saves a page.
    fn partition(&mut self, order: &[usize], budget: f32) -> Result<Vec<Vec<usize>>, Error> {
        let whole = self.partition_by_category(order, budget)?;
        if whole.len() <= 1 {
            return Ok(whole);
        }
        // A preferred group too tall for a page is split into its cards, as the category-first
        // partition splits one.
        let mut units = Vec::new();
        for unit in self.units(order) {
            if unit.len() > 1 && !self.fits(&unit, budget)? {
                units.extend(unit.into_iter().map(|i| vec![i]));
            } else {
                units.push(unit);
            }
        }
        let filled = self.fill(units, budget)?;
        Ok(if filled.len() < whole.len() {
            filled
        } else {
            whole
        })
    }

    fn partition_by_category(
        &mut self,
        order: &[usize],
        budget: f32,
    ) -> Result<Vec<Vec<usize>>, Error> {
        let mut units = Vec::new();
        let mut at = 0;
        while at < order.len() {
            let end = at
                + order[at..]
                    .iter()
                    .take_while(|i| self.items[**i].category == self.items[order[at]].category)
                    .count();
            let category = &order[at..end];
            if self.fits(category, budget)? {
                units.push(category.to_vec());
            } else {
                // Keep a preferred group intact even if it crosses a kind boundary. Category
                // boundaries are not optional, and validation has already rejected those groups.
                let groups = self.units(category);
                let mut kind_runs: Vec<Vec<usize>> = Vec::new();
                for group in groups {
                    if let Some(run) = kind_runs.last_mut().filter(|run| {
                        self.items[*run.last().unwrap()].kind == self.items[group[0]].kind
                    }) {
                        run.extend(group);
                    } else {
                        kind_runs.push(group);
                    }
                }
                for kind in kind_runs {
                    if self.fits(&kind, budget)? {
                        units.push(kind);
                    } else {
                        for group in self.units(&kind) {
                            if self.fits(&group, budget)? {
                                units.push(group);
                            } else {
                                units.extend(group.into_iter().map(|i| vec![i]));
                            }
                        }
                    }
                }
            }
            at = end;
        }
        self.fill(units, budget)
    }

    /// Pages in order, each taking units until the next would not fit.
    fn fill(&mut self, units: Vec<Vec<usize>>, budget: f32) -> Result<Vec<Vec<usize>>, Error> {
        let mut pages = Vec::new();
        let mut current = Vec::new();
        for unit in units {
            let candidate: Vec<_> = current.iter().chain(&unit).copied().collect();
            if !current.is_empty()
                && !self.fits(&candidate, budget)?
                && !self.rides_along(&current, &candidate, budget)?
            {
                pages.push(std::mem::take(&mut current));
            }
            current.extend(unit);
        }
        if !current.is_empty() {
            pages.push(current);
        }
        Ok(pages)
    }
}

fn label(items: &[Item<'_>], page: &[usize]) -> String {
    let mut categories = Vec::new();
    for index in page {
        let category = items[*index].category;
        if categories.last() != Some(&category) {
            categories.push(category);
        }
    }
    if categories.len() == 1 && page.iter().all(|i| items[*i].kind == items[page[0]].kind) {
        let category_count = items
            .iter()
            .filter(|item| item.category == categories[0])
            .count();
        if page.len() < category_count {
            return items[page[0]].kind.to_owned();
        }
    }
    categories
        .iter()
        .map(|c| c.title())
        .collect::<Vec<_>>()
        .join(" + ")
}

/// Selection/gesture state independent of egui and audio. A renderer supplies current-fit evidence
/// and a candidate computed with a smaller merge budget (`height - merge_margin`) while the old
/// pages still fit. Splits use the full budget immediately; merges require that spare height.
/// The pure state owns no margin; editor integration documents its chosen UX margin separately.
#[derive(Clone, Debug, Default)]
pub struct State {
    pub plan: Plan,
    anchor: Option<Key>,
    pending: Option<Plan>,
}

impl State {
    pub fn selected_page(&self) -> usize {
        self.anchor
            .and_then(|key| self.plan.pages.iter().position(|p| p.cards.contains(&key)))
            .unwrap_or(0)
    }

    /// A user-selected page is anchored to its first card, not to the transient page index.
    pub fn select(&mut self, page: usize) {
        if let Some(key) = self
            .plan
            .pages
            .get(page)
            .and_then(|page| page.cards.first())
        {
            self.anchor = Some(*key);
        }
    }

    /// Pending measurement does not erase the last usable layout. While interaction is owned,
    /// even a complete replacement waits; `resume` installs the latest one after ownership ends.
    pub fn update(&mut self, outcome: Outcome, interaction_owned: bool) {
        match outcome {
            Outcome::Ready(plan) if interaction_owned => self.pending = Some(plan),
            Outcome::Ready(plan) => {
                self.pending = None;
                self.install(plan);
            }
            // A newer incomplete solve invalidates any older deferred complete candidate.
            Outcome::Pending(_) => self.pending = None,
        }
    }

    pub fn resume(&mut self) {
        if let Some(plan) = self.pending.take() {
            self.install(plan);
        }
    }

    /// Retain a fitting partition unless a conservatively measured merge saves a page. The caller
    /// recomputes current-fit evidence at the real width, including current bar reservation.
    pub fn update_with_hysteresis(
        &mut self,
        candidate: Outcome,
        current_fits: bool,
        interaction_owned: bool,
    ) {
        if let Outcome::Ready(plan) = &candidate
            && current_fits
            && !self.plan.pages.is_empty()
            && plan.pages.len() >= self.plan.pages.len()
            && plan.pages.iter().flat_map(|p| &p.cards).eq(self
                .plan
                .pages
                .iter()
                .flat_map(|p| &p.cards))
        {
            // A merge-budget candidate may call a fitting card overflow merely because of the
            // dead band. Retain real-budget fit evidence instead. New/removed/reordered keys
            // must never be swallowed by hysteresis.
            let mut retained = self.plan.clone();
            for page in &mut retained.pages {
                page.overflow = false;
            }
            self.update(Outcome::Ready(retained), interaction_owned);
            return;
        }
        self.update(candidate, interaction_owned);
    }

    fn install(&mut self, plan: Plan) {
        let survives = |key: &Key| plan.pages.iter().any(|page| page.cards.contains(key));
        let old: Vec<_> = self
            .plan
            .pages
            .iter()
            .flat_map(|page| page.cards.iter().copied())
            .collect();
        self.anchor = self
            .anchor
            .filter(survives)
            .or_else(|| {
                let at = self
                    .anchor
                    .and_then(|key| old.iter().position(|k| *k == key))
                    .unwrap_or(0);
                old[at..]
                    .iter()
                    .chain(old[..at].iter().rev())
                    .find(|key| survives(key))
                    .copied()
            })
            .or_else(|| {
                plan.pages
                    .first()
                    .and_then(|page| page.cards.first())
                    .copied()
            });
        self.plan = plan;
    }
}
