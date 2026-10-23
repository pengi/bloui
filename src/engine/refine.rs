// SPDX-License-Identifier: GPL-2.0-only
//! Cutting fragments at label boundaries.

use super::ResolvedLabels;
use crate::model::{Fragment, Label, Span};

/// Cut `frag` at the boundaries of `labels`, and call `out` with each piece
/// and its label. Bytes without an entry get the fallback label.
pub fn cut(frag: &Fragment, labels: &ResolvedLabels, mut out: impl FnMut(Fragment, &Label)) {
    let span = frag.span;
    let mut cursor = span.start;
    for (s, label) in labels.map.overlapping(span) {
        if s.start > cursor {
            out(frag.piece(Span::new(cursor, s.start)), &labels.fallback);
        }
        let piece = Span::new(cursor.max(s.start), s.end.min(span.end));
        out(frag.piece(piece), label);
        cursor = piece.end;
    }
    if cursor < span.end {
        out(frag.piece(Span::new(cursor, span.end)), &labels.fallback);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BinaryId, Chunk, Interner, Label, RangeMap};

    #[test]
    fn cut_with_gaps() {
        let mut i = Interner::new();
        let a = Label::single(i.intern("a"));
        let fb = Label::single(i.intern("fb"));
        let labels = ResolvedLabels {
            map: RangeMap::from_sorted(vec![
                (Span::new(5, 10), a.clone()),
                (Span::new(12, 30), a.clone()),
            ]),
            fallback: fb.clone(),
        };
        let chunk = Chunk {
            unified: Span::new(0, 20),
            file: Some(0),
            vma: None,
            lma: None,
        };
        let frag = Fragment::new(BinaryId(0), 0, &chunk, Span::new(2, 20));
        let mut got = Vec::new();
        cut(&frag, &labels, |f, l| {
            got.push((f.span, l.clone(), f.sizes.file))
        });
        assert_eq!(
            got,
            vec![
                (Span::new(2, 5), fb.clone(), 3),
                (Span::new(5, 10), a.clone(), 5),
                (Span::new(10, 12), fb, 2),
                (Span::new(12, 20), a, 8),
            ]
        );
    }
}
