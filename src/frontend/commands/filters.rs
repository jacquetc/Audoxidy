//! The library filter index: the tree behind the Folder, Artist, Album, Genre
//! and Year filters.

#![warn(missing_docs)]

use crate::frontend::app_context::AppContext;
use crate::frontend::error::FrontendError;
use crate::frontend::events::{LongOpKind, OpId};
use crate::frontend::reply::Reply;
use crate::logic::filter_index::{FilterIndex, FilterType, HierarchyRow};

/// The hierarchy rows of one filter type, straight from the database.
///
/// This is the single-type query behind
/// [`FilterIndex::insert`](crate::logic::filter_index::FilterIndex::insert).
pub fn load_hierarchy(ctx: &AppContext, filter_type: FilterType) -> Reply<Vec<HierarchyRow>> {
    ctx.db
        .read(move |db| Ok(db.get_filter_hierarchy(filter_type.name())?))
}

/// Builds the filter index of all five filter types as a long operation
/// ([`LongOpKind::FilterIndexBuild`]).
///
/// The result is a [`FilterIndex`], taken with
/// `ctx.ops.take_result::<FilterIndex>(op_id)`. Like the Iced UI, a filter
/// type whose query fails is left out of the index rather than failing the
/// whole build; the build fails only when the database worker is gone.
pub fn build_filter_index(ctx: &AppContext) -> OpId {
    let db = ctx.db.clone();
    ctx.ops.start(LongOpKind::FilterIndexBuild, move |op| {
        let mut index = FilterIndex::new();
        let total = FilterType::ALL.len() as f32;
        for (done, filter_type) in FilterType::ALL.into_iter().enumerate() {
            op.check_cancelled()?;
            let rows = db
                .read(move |db| Ok(db.get_filter_hierarchy(filter_type.name())?))
                .recv_blocking();
            match rows {
                Ok(rows) => index.insert(filter_type, rows),
                Err(e @ FrontendError::WorkerStopped(_)) => return Err(e),
                Err(e) => tracing::warn!(
                    filter = filter_type.name(),
                    error = %e,
                    "the filter hierarchy could not be loaded; the type is left out"
                ),
            }
            op.report_progress((done + 1) as f32 / total);
        }
        Ok(index)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::events::{Event, LongOpEvent, Origin};
    use crate::frontend::test_fixtures::LibraryFixture;
    use crate::frontend::threading::test_support::{TEST_TIMEOUT, with_timeout};
    use std::sync::{Arc, Mutex, mpsc};

    fn seeded() -> AppContext {
        let ctx = AppContext::in_memory().unwrap();
        LibraryFixture::demo()
            .seed_on(&ctx.db)
            .recv_blocking()
            .unwrap();
        ctx
    }

    #[test]
    fn the_single_type_query_returns_the_database_rows() {
        with_timeout(TEST_TIMEOUT, || {
            let ctx = seeded();
            let rows = load_hierarchy(&ctx, FilterType::Genre)
                .recv_blocking()
                .unwrap();
            let genres: std::collections::BTreeSet<String> =
                rows.iter().map(|(l1, _, _)| l1.to_string()).collect();
            assert_eq!(
                genres.into_iter().collect::<Vec<_>>(),
                vec!["Pop", "Rock", "Synthpop"]
            );
        });
    }

    #[test]
    fn the_filter_index_operation_equals_the_index_built_from_the_same_rows() {
        with_timeout(TEST_TIMEOUT, || {
            let ctx = seeded();
            let (tx, rx) = mpsc::channel();
            let tx = Mutex::new(tx);
            let _token = ctx.hub.subscribe(
                Origin::LongOperation,
                Arc::new(move |event| {
                    if let Event::LongOperation(LongOpEvent::Completed(info)) = event {
                        let _ = tx.lock().unwrap().send(info);
                    }
                }),
            );
            let op = build_filter_index(&ctx);
            let info = rx.recv_timeout(TEST_TIMEOUT).unwrap();
            assert_eq!(info.op_id, op);
            assert_eq!(info.kind, LongOpKind::FilterIndexBuild);

            let built = ctx.ops.take_result::<FilterIndex>(op).unwrap().unwrap();
            let mut expected = FilterIndex::new();
            for filter_type in FilterType::ALL {
                let rows = load_hierarchy(&ctx, filter_type).recv_blocking().unwrap();
                expected.insert(filter_type, rows);
            }
            for filter_type in FilterType::ALL {
                assert!(built.tree(filter_type).is_some_and(|t| !t.is_empty()));
                assert_eq!(built.tree(filter_type), expected.tree(filter_type));
                assert_eq!(
                    built.subfilters(filter_type),
                    expected.subfilters(filter_type)
                );
            }
        });
    }

    #[test]
    fn building_the_index_after_shutdown_fails_the_operation() {
        with_timeout(TEST_TIMEOUT, || {
            let ctx = seeded();
            assert!(ctx.shutdown());
            let op = build_filter_index(&ctx);
            let result = ctx.ops.take_result::<FilterIndex>(op).unwrap();
            assert!(matches!(result, Err(FrontendError::WorkerStopped(_))));
        });
    }
}
