pub mod buf;
pub mod functional;

pub use functional::*;

use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};

use tokio::task::JoinHandle;

#[inline]
pub fn next_id() -> usize {
    static _INTERNAL_ID: AtomicUsize = AtomicUsize::new(0);

    _INTERNAL_ID.fetch_add(1, Ordering::Relaxed)
}

#[inline]
pub fn spawn<F>(f: F) -> JoinHandle<F::Output>
where
    F: Future + 'static,
    F::Output: 'static,
{
    tokio::task::spawn_local(f)
}
