pub mod buf;
pub mod functional;
pub mod macros;
pub mod notify;
pub mod num;

pub use functional::*;

use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::{self, ThreadId};

use tokio::task::JoinHandle;

pub use macros::*;
pub use notify::*;

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

#[inline]
pub fn thread_id() -> ThreadId {
    thread_local! {
        static THREAD_ID: ThreadId = thread::current().id();
    }
    THREAD_ID.with(|t| *t)
}
