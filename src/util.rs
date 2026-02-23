//! Shared utility types.

use std::future::Future;
use std::pin::Pin;

/// A boxed, pinned, Send future — used throughout the crate for async callbacks.
pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;
