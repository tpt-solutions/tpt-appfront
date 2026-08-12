//! Backend-agnostic keyed list reconciliation (Phase 3).
//!
//! `tpt-appfront-dom` already performs keyed reconciliation against the live DOM
//! in `tpt_appfront_dom::update_list`; this module factors the *pure* part of
//! that algorithm out so it can be unit-tested on any target (the DOM backend
//! is `wasm32`-only and can't run native tests) and reused by other backends
//! that render keyed collections (`tpt-appfront-canvas`, `tpt-appfront-tui`).
//!
//! The input is two ordered sequences of keys (the previous render's keys and
//! the next render's keys). The output is a [`KeyedDiff`] describing, per new
//! item, whether the existing node can be kept in place, must be moved, or is
//! new — plus the list of keys that disappeared and should be removed. A
//! backend walks [`KeyedDiff::edits`] in order, reusing/creating/moving DOM
//! (or canvas/`ratatui`) nodes accordingly, which adds/removes/reorders
//! without rebuilding the whole list.
//!
//! Keys should be unique. Duplicate keys are not meaningful for reconciliation
//! and will produce undefined moves. In debug builds a warning is emitted when
//! duplicate keys are detected (see [`reconcile_keys`]).

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

/// What to do with one item in the new sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListEdit<K> {
    /// Key already rendered and already sitting at this position — no-op.
    Keep { key: K },
    /// Key already rendered but at a different position — move it here.
    Move { key: K },
    /// Key is new — create a node here.
    Insert { key: K },
}

/// The result of diffing two keyed sequences.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyedDiff<K> {
    /// One entry per item in the *new* sequence, in order.
    pub edits: Vec<ListEdit<K>>,
    /// Keys present in `old` but absent from `new` — remove their nodes.
    pub removed: Vec<K>,
}

/// Diffs `old` against `new`, producing the edits a backend needs to reconcile
/// a rendered keyed collection (add/remove/reorder) without a full rebuild.
pub fn reconcile_keys<K>(old: &[K], new: &[K]) -> KeyedDiff<K>
where
    K: Clone + Eq + Hash,
{
    // Debug-only guard for the documented-as-undefined duplicate-key case:
    // reconciliation is meaningless when the same key appears more than once
    // in `old` or `new` (a node can't have two stable identities). In debug
    // builds we surface it instead of silently emitting undefined moves.
    #[cfg(debug_assertions)]
    warn_on_duplicate_keys("old", old);
    #[cfg(debug_assertions)]
    warn_on_duplicate_keys("new", new);

    // O(n + m) set membership instead of the previous `Vec::contains` scans.
    let new_set: HashSet<&K> = new.iter().collect();
    let old_in_new: HashSet<&K> = old.iter().filter(|k| new_set.contains(k)).collect();
    let removed: Vec<K> = old
        .iter()
        .filter(|k| !new_set.contains(k))
        .cloned()
        .collect();

    // Survivors (old keys that appear in `new`) in old order, plus an index map
    // so a "move" can mark a survivor consumed in O(1) without shifting the
    // backing `Vec`. The map entry is set to `usize::MAX` when a survivor has
    // been consumed, so the front-advance loop skips over it.
    let survivors: Vec<&K> = old
        .iter()
        .filter(|k| old_in_new.contains(k))
        .collect();
    let mut survivor_idx: HashMap<&K, usize> = survivors
        .iter()
        .enumerate()
        .map(|(i, &k)| (k, i))
        .collect();

    let mut edits = Vec::with_capacity(new.len());
    let mut front_idx = 0usize;
    for k in new {
        if !old_in_new.contains(k) {
            edits.push(ListEdit::Insert { key: k.clone() });
            continue;
        }
        // Skip consumed survivors at the front of the old-order queue.
        while front_idx < survivors.len()
            && survivor_idx.get(survivors[front_idx]) != Some(&front_idx)
        {
            front_idx += 1;
        }
        if front_idx < survivors.len() && survivors[front_idx] == k {
            // Next survivor in old order — keep in place, no DOM move needed.
            survivor_idx.remove(k);
            front_idx += 1;
            edits.push(ListEdit::Keep { key: k.clone() });
        } else {
            // Already rendered but not in its old-order position — move it here.
            // Mark consumed (O(1)) rather than removing from the `Vec`.
            survivor_idx.insert(k, usize::MAX);
            edits.push(ListEdit::Move { key: k.clone() });
        }
    }

    KeyedDiff { edits, removed }
}

/// Emits a debug-only warning when `seq` contains duplicate keys (the
/// undefined-duplicate-key case documented on [`reconcile_keys`]).
#[cfg(debug_assertions)]
fn warn_on_duplicate_keys<K: Eq + Hash>(which: &str, seq: &[K]) {
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    for k in seq {
        if !seen.insert(k) {
            eprintln!(
                "tpt-appfront reconcile_keys: duplicate key in `{which}` produces \
                 undefined moves; ensure list keys are unique"
            );
            break;
        }
    }
}

/// Applies a [`KeyedDiff`] to a keyed sequence, for tests and for backends
/// that keep their own `Vec<key>` mirror. Returns the resulting order, which
/// must equal `new`.
pub fn apply_edits<K>(old: &[K], diff: &KeyedDiff<K>) -> Vec<K>
where
    K: Clone + Eq + Hash,
{
    let mut working: Vec<K> = old
        .iter()
        .filter(|&k| !diff.removed.contains(k))
        .cloned()
        .collect();
    let mut out = Vec::with_capacity(diff.edits.len());
    for edit in &diff.edits {
        match edit {
            ListEdit::Keep { key } | ListEdit::Move { key } => {
                if let Some(idx) = working.iter().position(|w| w == key) {
                    working.remove(idx);
                }
                out.push(key.clone());
            }
            ListEdit::Insert { key } => out.push(key.clone()),
        }
    }
    out
}

/// A one-line human-readable description of a single [`ListEdit`], for
/// change-explanation UIs (e.g. "moved a", "inserted c", "kept b"). This is the
/// "what changed" half of the undo/redo + change-explanation utility (#75).
pub fn edit_description<K: std::fmt::Display>(edit: &ListEdit<K>) -> String {
    match edit {
        ListEdit::Keep { key } => format!("kept {key}"),
        ListEdit::Move { key } => format!("moved {key}"),
        ListEdit::Insert { key } => format!("inserted {key}"),
    }
}

/// A human-readable summary of a [`KeyedDiff`]: how many items were kept,
/// moved, inserted, and removed. Suitable for surfacing "what changed between
/// renders" in a devtools/undo panel.
pub fn diff_summary<K: std::fmt::Display>(diff: &KeyedDiff<K>) -> String {
    let mut keeps = 0;
    let mut moves = 0;
    let mut inserts = 0;
    for e in &diff.edits {
        match e {
            ListEdit::Keep { .. } => keeps += 1,
            ListEdit::Move { .. } => moves += 1,
            ListEdit::Insert { .. } => inserts += 1,
        }
    }
    format!(
        "kept {keeps}, moved {moves}, inserted {inserts}, removed {}",
        diff.removed.len()
    )
}

/// A bounded undo/redo stack over snapshot values of type `T` (e.g. a `UITree`,
/// a form struct, or any `Clone` app state). Pushing a *new* value records it
/// as the present; [`History::undo`]/`[`History::redo`] walk the timeline.
///
/// This is intentionally generic and backend-agnostic: it stores plain `T`
/// snapshots (the same `UITree`/`Signal` data the render core already uses),
/// so any app gets "what changed / undo this" almost for free on top of the
/// keyed-diffing in this module. Pair [`diff_summary`] with the snapshot delta
/// to explain each step.
pub struct History<T> {
    past: Vec<T>,
    present: T,
    future: Vec<T>,
    limit: Option<usize>,
}

impl<T: Clone> History<T> {
    /// Creates a history seeded with `initial` as the present. `limit`, if
    /// `Some`, caps how many past snapshots are retained (oldest dropped first).
    pub fn new(initial: T, limit: Option<usize>) -> Self {
        History {
            past: Vec::new(),
            present: initial,
            future: Vec::new(),
            limit,
        }
    }

    /// Returns the current value.
    pub fn present(&self) -> &T {
        &self.present
    }

    /// Commits a new present, pushing the old one onto the undo stack and
    /// clearing the redo stack. No-op (aside from `present` already being the
    /// new value) if `new_value` is equal to the current present.
    pub fn push(&mut self, new_value: T)
    where
        T: PartialEq,
    {
        if new_value == self.present {
            return;
        }
        self.past.push(self.present.clone());
        if let Some(limit) = self.limit {
            while self.past.len() > limit {
                self.past.remove(0);
            }
        }
        self.present = new_value;
        self.future.clear();
    }

    /// Moves the present onto the redo stack and restores the most recent past
    /// snapshot. Returns `true` if an undo happened.
    pub fn undo(&mut self) -> bool {
        if let Some(prev) = self.past.pop() {
            self.future.push(std::mem::replace(&mut self.present, prev));
            true
        } else {
            false
        }
    }

    /// Moves the present onto the undo stack and restores the most recent
    /// future snapshot. Returns `true` if a redo happened.
    pub fn redo(&mut self) -> bool {
        if let Some(next) = self.future.pop() {
            self.past.push(std::mem::replace(&mut self.present, next));
            true
        } else {
            false
        }
    }

    /// Whether an [`History::undo`] would succeed.
    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    /// Whether an [`History::redo`] would succeed.
    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    /// Number of undo-able steps currently retained.
    pub fn depth(&self) -> usize {
        self.past.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diff_and_apply(old: &[&str], new: &[&str]) -> Vec<String> {
        let owned_old: Vec<String> = old.iter().map(|s| s.to_string()).collect();
        let owned_new: Vec<String> = new.iter().map(|s| s.to_string()).collect();
        let diff = reconcile_keys(&owned_old, &owned_new);
        apply_edits(&owned_old, &diff).into_iter().collect()
    }

    #[test]
    fn identical_lists_are_all_keeps() {
        let old = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let diff = reconcile_keys(&old, &old);
        assert!(diff.removed.is_empty());
        assert_eq!(
            diff.edits,
            vec![
                ListEdit::Keep {
                    key: "a".to_string()
                },
                ListEdit::Keep {
                    key: "b".to_string()
                },
                ListEdit::Keep {
                    key: "c".to_string()
                },
            ]
        );
    }

    #[test]
    fn append_produces_insert_and_no_removes() {
        let out = diff_and_apply(&["a", "b"], &["a", "b", "c"]);
        assert_eq!(out, vec!["a", "b", "c"]);
    }

    #[test]
    fn truncate_removes_tail() {
        let old = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let new = vec!["a".to_string()];
        let diff = reconcile_keys(&old, &new);
        assert_eq!(diff.removed, vec!["b".to_string(), "c".to_string()]);
        let out = apply_edits(&old, &diff);
        assert_eq!(out, vec!["a".to_string()]);
    }

    #[test]
    fn reorder_is_moves_not_full_rebuild() {
        let out = diff_and_apply(&["a", "b", "c", "d"], &["d", "c", "b", "a"]);
        assert_eq!(out, vec!["d", "c", "b", "a"]);

        let old = vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ];
        let new = vec![
            "d".to_string(),
            "c".to_string(),
            "b".to_string(),
            "a".to_string(),
        ];
        let diff = reconcile_keys(&old, &new);
        assert!(diff.removed.is_empty());
        // No inserts: every key already existed.
        assert!(diff
            .edits
            .iter()
            .all(|e| !matches!(e, ListEdit::Insert { .. })));
    }

    #[test]
    fn insert_in_middle_moves_following() {
        // Inserting "x" between "a" and "b": a stays, x inserted, b and c are
        // now already in their correct (shifted) positions so they stay Keep.
        let old = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let new = vec![
            "a".to_string(),
            "x".to_string(),
            "b".to_string(),
            "c".to_string(),
        ];
        let diff = reconcile_keys(&old, &new);
        assert_eq!(
            diff.edits[0],
            ListEdit::Keep {
                key: "a".to_string()
            }
        );
        assert_eq!(
            diff.edits[1],
            ListEdit::Insert {
                key: "x".to_string()
            }
        );
        assert_eq!(
            diff.edits[2],
            ListEdit::Keep {
                key: "b".to_string()
            }
        );
        let out = apply_edits(&old, &diff);
        assert_eq!(out, new);
    }

    #[test]
    fn remove_from_middle_shifts_others_to_keep() {
        let old = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let new = vec!["a".to_string(), "c".to_string()];
        let diff = reconcile_keys(&old, &new);
        assert_eq!(diff.removed, vec!["b".to_string()]);
        // a kept, c kept (still in order, just shifted left).
        assert_eq!(
            diff.edits[0],
            ListEdit::Keep {
                key: "a".to_string()
            }
        );
        assert_eq!(
            diff.edits[1],
            ListEdit::Keep {
                key: "c".to_string()
            }
        );
    }

    #[test]
    fn mixed_add_remove_reorder_reproduces_new() {
        let out = diff_and_apply(&["a", "b", "c", "d", "e"], &["e", "b", "f", "d"]);
        assert_eq!(out, vec!["e", "b", "f", "d"]);
    }

    #[test]
    fn empty_old_is_all_inserts() {
        let old: Vec<String> = vec![];
        let new = vec!["a".to_string(), "b".to_string()];
        let diff = reconcile_keys(&old, &new);
        assert!(diff.removed.is_empty());
        assert_eq!(
            diff.edits,
            vec![
                ListEdit::Insert {
                    key: "a".to_string()
                },
                ListEdit::Insert {
                    key: "b".to_string()
                },
            ]
        );
    }

    #[test]
    fn edit_description_is_readable() {
        assert_eq!(edit_description(&ListEdit::Keep { key: "a" }), "kept a");
        assert_eq!(edit_description(&ListEdit::Move { key: "b" }), "moved b");
        assert_eq!(
            edit_description(&ListEdit::Insert { key: "c" }),
            "inserted c"
        );
    }

    #[test]
    fn diff_summary_counts_ops() {
        let old = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let new = vec!["a".to_string(), "x".to_string(), "c".to_string()];
        let diff = reconcile_keys(&old, &new);
        assert_eq!(
            diff_summary(&diff),
            "kept 2, moved 0, inserted 1, removed 1"
        );
    }

    #[test]
    fn large_full_reorder_stays_correct_and_fast() {
        // Worst case for the old O(n·m) scan (a full reversal): must still produce
        // the correct reorder and must not be quadratic. 20k items reversed.
        let n = 20_000usize;
        let old: Vec<String> = (0..n).map(|i| format!("k{i}")).collect();
        let mut new = old.clone();
        new.reverse();
        let diff = reconcile_keys(&old, &new);
        assert!(diff.removed.is_empty());
        assert!(diff
            .edits
            .iter()
            .all(|e| !matches!(e, ListEdit::Insert { .. })));
        assert_eq!(apply_edits(&old, &diff), new);
    }

    #[test]
    fn duplicate_keys_do_not_panic() {
        // Documented as undefined behavior; the algorithm must at least not panic
        // and must still emit one edit per new item.
        let old = vec!["a".to_string(), "a".to_string(), "b".to_string()];
        let new = vec!["b".to_string(), "a".to_string()];
        let diff = reconcile_keys(&old, &new);
        assert_eq!(apply_edits(&old, &diff).len(), new.len());
    }
}

#[cfg(test)]
mod history_tests {
    use super::*;

    #[test]
    fn push_then_undo_redo_round_trips() {
        let mut h = History::new(0u32, None);
        h.push(1);
        h.push(2);
        assert_eq!(*h.present(), 2);
        assert!(h.can_undo());
        assert!(h.undo());
        assert_eq!(*h.present(), 1);
        assert!(h.undo());
        assert_eq!(*h.present(), 0);
        assert!(!h.can_undo());

        assert!(h.redo());
        assert_eq!(*h.present(), 1);
        assert!(h.redo());
        assert_eq!(*h.present(), 2);
        assert!(!h.can_redo());
    }

    #[test]
    fn new_push_clears_redo() {
        let mut h = History::new(0u32, None);
        h.push(1);
        h.undo();
        assert!(h.can_redo());
        h.push(5);
        assert!(!h.can_redo());
        assert_eq!(*h.present(), 5);
    }

    #[test]
    fn equal_push_is_noop() {
        let mut h = History::new(1u32, None);
        let depth_before = h.depth();
        h.push(1);
        assert_eq!(h.depth(), depth_before);
    }

    #[test]
    fn limit_drops_oldest() {
        let mut h = History::new(0u32, Some(2));
        h.push(1);
        h.push(2);
        h.push(3);
        // past holds at most 2 snapshots.
        assert_eq!(h.depth(), 2);
        assert!(h.undo());
        assert_eq!(*h.present(), 2);
    }
}
