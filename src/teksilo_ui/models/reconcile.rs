//! Keyed reconcile of a `ListModel` that cannot panic on duplicate keys.
//!
//! `ListModel::reconcile_by_key` matches old and new rows by key, emits only
//! the granular changes (never a `Reset`, which would drop a selection and the
//! keyboard focus) and stays silent when nothing changed. It panics when a key
//! repeats within the new rows, and the data a model reads (two albums, two
//! devices) is not guaranteed to be unique, so the rows are de-duplicated
//! first, keeping the first of each key.

use std::collections::HashSet;
use std::hash::Hash;

use teksilo::data::ListModel;

/// Reconciles `list` with `rows`, matching by `key`. A later row whose key
/// repeats an earlier one is dropped.
pub fn reconcile_unique<T: PartialEq + 'static, K: Eq + Hash>(
    list: &ListModel<T>,
    rows: Vec<T>,
    key: impl Fn(&T) -> K,
) {
    let mut seen = HashSet::new();
    let unique: Vec<T> = rows
        .into_iter()
        .filter(|row| seen.insert(key(row)))
        .collect();
    list.reconcile_by_key(unique, key);
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use teksilo::data::DataChange;

    use super::*;

    fn changes(list: &ListModel<(u32, &'static str)>) -> (Rc<RefCell<Vec<String>>>, impl Sized) {
        let log = Rc::new(RefCell::new(Vec::new()));
        let sink = log.clone();
        let handle = list.observe_changes(move |change: &DataChange| {
            let name = match change {
                DataChange::ItemsInserted { .. } => "inserted",
                DataChange::ItemsRemoved { .. } => "removed",
                DataChange::ItemsMoved { .. } => "moved",
                DataChange::ItemUpdated { .. } => "updated",
                DataChange::WindowLoaded { .. } => "window",
                DataChange::Reset => "reset",
            };
            sink.borrow_mut().push(name.to_string());
        });
        (log, handle)
    }

    #[test]
    fn duplicate_keys_keep_the_first_row_instead_of_panicking() {
        let list = ListModel::from_vec(vec![(1, "a")]);
        reconcile_unique(&list, vec![(1, "a"), (2, "b"), (2, "dup")], |row| row.0);
        assert_eq!(list.len(), 2);
        assert_eq!(list.with_item(1, |row| row.1), Some("b"));
    }

    #[test]
    fn unchanged_rows_emit_nothing_and_a_change_is_granular() {
        let list = ListModel::from_vec(vec![(1, "a"), (2, "b")]);
        let (log, _handle) = changes(&list);
        reconcile_unique(&list, vec![(1, "a"), (2, "b")], |row| row.0);
        assert!(log.borrow().is_empty(), "silent when unchanged");
        reconcile_unique(&list, vec![(1, "a"), (2, "B"), (3, "c")], |row| row.0);
        let mut seen = log.borrow().clone();
        seen.sort();
        assert_eq!(seen, vec!["inserted", "updated"]);
    }
}
