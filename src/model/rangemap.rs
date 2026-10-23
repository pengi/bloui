// SPDX-License-Identifier: GPL-2.0-only
//! Sorted maps of non-overlapping ranges.

use super::Span;
use std::collections::BTreeMap;

/// A sorted list of non-overlapping ranges, each with a value.
#[derive(Clone, Debug)]
pub struct RangeMap<T> {
    entries: Vec<(Span, T)>,
}

impl<T> Default for RangeMap<T> {
    fn default() -> Self {
        RangeMap {
            entries: Vec::new(),
        }
    }
}

impl<T> RangeMap<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// From entries that are already sorted and don't overlap.
    pub fn from_sorted(entries: Vec<(Span, T)>) -> Self {
        debug_assert!(entries.windows(2).all(|w| w[0].0.end <= w[1].0.start));
        RangeMap { entries }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (Span, &T)> {
        self.entries.iter().map(|(s, v)| (*s, v))
    }

    /// The entry containing `addr`.
    pub fn get(&self, addr: u64) -> Option<(Span, &T)> {
        let i = self.entries.partition_point(|(s, _)| s.end <= addr);
        self.entries
            .get(i)
            .filter(|(s, _)| s.contains(addr))
            .map(|(s, v)| (*s, v))
    }

    /// The entries that overlap `span`, unclipped, in order.
    pub fn overlapping(&self, span: Span) -> impl Iterator<Item = (Span, &T)> {
        let first = self.entries.partition_point(|(s, _)| s.end <= span.start);
        self.entries[first..]
            .iter()
            .take_while(move |(s, _)| s.start < span.end)
            .map(|(s, v)| (*s, v))
    }
}

/// What wins when input ranges overlap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overlap {
    /// The range pushed first wins.
    FirstWins,
    /// The smallest range wins (ties: the first pushed).
    SmallestWins,
    /// The largest range wins (ties: the first pushed).
    LargestWins,
}

struct Item<T> {
    tier: u32,
    weight: u64,
    order: usize,
    span: Span,
    value: T,
}

/// Builds a [`RangeMap`] from ranges that may overlap.
///
/// Ranges are sorted by tier first (lower tier wins), then by the overlap
/// policy. Each range then only fills the parts not yet taken.
pub struct RangeMapBuilder<T> {
    items: Vec<Item<T>>,
}

impl<T> Default for RangeMapBuilder<T> {
    fn default() -> Self {
        RangeMapBuilder { items: Vec::new() }
    }
}

impl<T: Clone> RangeMapBuilder<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, span: Span, value: T) {
        self.push_weighted(0, span.len(), span, value);
    }

    /// Push with an explicit tier and weight. The weight is what the
    /// smallest/largest policies compare; it's normally the length of the
    /// original range, even when only a piece of it is pushed.
    pub fn push_weighted(&mut self, tier: u32, weight: u64, span: Span, value: T) {
        if span.is_empty() {
            return;
        }
        let order = self.items.len();
        self.items.push(Item {
            tier,
            weight,
            order,
            span,
            value,
        });
    }

    pub fn build(mut self, policy: Overlap) -> RangeMap<T> {
        match policy {
            Overlap::FirstWins => self.items.sort_by_key(|i| (i.tier, i.order)),
            Overlap::SmallestWins => self.items.sort_by_key(|i| (i.tier, i.weight, i.order)),
            Overlap::LargestWins => {
                self.items
                    .sort_by_key(|i| (i.tier, std::cmp::Reverse(i.weight), i.order));
            }
        }
        // start -> (end, value)
        let mut taken: BTreeMap<u64, (u64, T)> = BTreeMap::new();
        for item in self.items {
            let Span { start, end } = item.span;
            let mut cursor = start;
            // A range starting before `start` may cover the beginning.
            if let Some((_, (e, _))) = taken.range(..start).next_back() {
                cursor = cursor.max(*e);
            }
            let mut gaps = Vec::new();
            for (&s, (e, _)) in taken.range(start..end) {
                if s > cursor {
                    gaps.push(Span::new(cursor, s));
                }
                cursor = cursor.max(*e);
                if cursor >= end {
                    break;
                }
            }
            if cursor < end {
                gaps.push(Span::new(cursor, end));
            }
            for gap in gaps {
                taken.insert(gap.start, (gap.end, item.value.clone()));
            }
        }
        RangeMap::from_sorted(
            taken
                .into_iter()
                .map(|(s, (e, v))| (Span::new(s, e), v))
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(policy: Overlap, input: &[(u64, u64, char)]) -> Vec<(u64, u64, char)> {
        let mut b = RangeMapBuilder::new();
        for &(s, e, v) in input {
            b.push(Span::new(s, e), v);
        }
        b.build(policy)
            .iter()
            .map(|(s, v)| (s.start, s.end, *v))
            .collect()
    }

    #[test]
    fn first_wins() {
        let r = build(
            Overlap::FirstWins,
            &[(0, 10, 'a'), (5, 15, 'b'), (2, 3, 'c')],
        );
        assert_eq!(r, vec![(0, 10, 'a'), (10, 15, 'b')]);
    }

    #[test]
    fn smallest_wins_splits_the_larger() {
        let r = build(Overlap::SmallestWins, &[(0, 10, 'a'), (4, 6, 'b')]);
        assert_eq!(r, vec![(0, 4, 'a'), (4, 6, 'b'), (6, 10, 'a')]);
    }

    #[test]
    fn largest_wins() {
        let r = build(
            Overlap::LargestWins,
            &[(4, 6, 'b'), (0, 10, 'a'), (8, 20, 'c')],
        );
        assert_eq!(r, vec![(0, 8, 'a'), (8, 20, 'c')]);
    }

    #[test]
    fn aliases_tie_on_first() {
        let r = build(Overlap::SmallestWins, &[(0, 4, 'a'), (0, 4, 'b')]);
        assert_eq!(r, vec![(0, 4, 'a')]);
    }

    #[test]
    fn tiers_beat_policy() {
        let mut b = RangeMapBuilder::new();
        b.push_weighted(1, 1, Span::new(0, 10), 'f');
        b.push_weighted(0, 100, Span::new(2, 4), 's');
        let r: Vec<_> = b
            .build(Overlap::SmallestWins)
            .iter()
            .map(|(s, v)| (s.start, s.end, *v))
            .collect();
        assert_eq!(r, vec![(0, 2, 'f'), (2, 4, 's'), (4, 10, 'f')]);
    }

    #[test]
    fn queries() {
        let mut b = RangeMapBuilder::new();
        b.push(Span::new(10, 20), 1);
        b.push(Span::new(30, 40), 2);
        let m = b.build(Overlap::FirstWins);
        assert_eq!(m.get(15).map(|(_, v)| *v), Some(1));
        assert_eq!(m.get(25), None);
        assert_eq!(m.get(20), None);
        let o: Vec<_> = m.overlapping(Span::new(19, 31)).map(|(_, v)| *v).collect();
        assert_eq!(o, vec![1, 2]);
        assert_eq!(m.overlapping(Span::new(20, 30)).count(), 0);
    }
}
