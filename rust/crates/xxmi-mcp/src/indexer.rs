//! The DLL-release indexer (git fetch and archive, per-version spec builds) is a later stage:
//! the server runs on the checkouts and specs already on the volume.

use std::future::Future;
use std::pin::Pin;

use serde::Serialize;

/// What one indexing pass did, as `POST /admin/refresh?wait=1` reports it.
#[derive(Debug, Default, Serialize)]
pub struct PassResult {
    pub published: Vec<String>,
    pub errors: Vec<String>,
}

pub trait Indexer: Send + Sync {
    fn refresh(&self) -> Pin<Box<dyn Future<Output = PassResult> + Send + '_>>;
}

/// Stands in until the indexer is ported.
pub struct Unavailable;

impl Indexer for Unavailable {
    fn refresh(&self) -> Pin<Box<dyn Future<Output = PassResult> + Send + '_>> {
        Box::pin(async {
            PassResult {
                published: Vec::new(),
                errors: vec![
                    "indexing unavailable: the indexer is not part of this build".to_string(),
                ],
            }
        })
    }
}
