use std::{
    cell::RefCell,
    future::Future,
    marker::PhantomData,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
    time::Duration,
};

use tokio::time;

/// Notify single task.
/// Simple notify mechanism for async current thread. `Notify` is `!Send` and `!Sync`.
/// Notify will call `notify.notified().await` to  listen for notifications.<br />
/// This implementation is very cheap to create, clone and await.<br />
/// Behavior:
///  1. If `Notifier::notify()` is calling before `notified().await`, the next call `notified().await` will complete immediately.
///  2. If `Notifier::notify()` is calling multiple times after `notified().await`,<br />
/// the next call `notified().await` will complete immediately and reset the state to waiting mode,
/// so the one after will wait for a new notify.
pub struct Notify {
    inner: Rc<RefCell<NotifyInner>>,
}

impl Notify {
    #[inline]
    pub fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(NotifyInner::new())),
        }
    }

    /// Create a notifier. Notifier must be used or it will cause the main `Notify` pending forever.
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    #[inline]
    pub fn notifier(&self) -> Notifier {
        self.inner.borrow_mut().create_notifier();
        Notifier::from_inner(self.inner.clone())
    }

    /// Equivalent to:
    /// ```ignore, rust
    /// pub async fn notified(&self);
    /// ```
    #[inline]
    pub fn notified(&self) -> Notified<'_> {
        Notified::from_inner_ref(&self.inner)
    }

    /// Wait for notify with timeout.<br />
    /// Return `true` if it got notified, `false` if timed out.
    #[inline]
    pub async fn notified_timeout(&self, duration: Duration) -> bool {
        time::timeout(duration, self.notified()).await.is_ok()
    }

    /// Count the number of current active notifiers.
    #[inline]
    pub fn notifiers(&self) -> u32 {
        self.inner.borrow().count_notifiers()
    }
}

pub struct Notifier {
    inner: Rc<RefCell<NotifyInner>>,
}

impl Notifier {
    fn from_inner(inner: Rc<RefCell<NotifyInner>>) -> Self {
        Self { inner }
    }

    #[inline]
    pub fn notify(&self) -> bool {
        self.inner.borrow_mut().wake()
    }
}

impl Clone for Notifier {
    #[inline]
    fn clone(&self) -> Notifier {
        self.inner.borrow_mut().create_notifier();

        Self {
            inner: self.inner.clone(),
        }
    }
}

impl Drop for Notifier {
    #[inline]
    fn drop(&mut self) {
        self.inner.borrow_mut().drop_notifier();
    }
}

pub struct Notified<'n> {
    inner: Rc<RefCell<NotifyInner>>,
    _m: PhantomData<&'n Rc<RefCell<NotifyInner>>>,
}

impl<'n> Notified<'n> {
    #[inline]
    fn from_inner_ref(inner: &'n Rc<RefCell<NotifyInner>>) -> Self {
        Self {
            inner: inner.clone(),
            _m: PhantomData,
        }
    }

    #[inline]
    fn set_state(&self, state: NotifyState) {
        self.inner.borrow_mut().set_state(state);
    }

    #[inline]
    fn set_waker(&self, waker: &Waker) {
        self.inner.borrow_mut().set_waker(waker);
    }

    #[inline]
    fn set_waker_if_empty(&self, waker: &Waker) {
        if self.inner.borrow().is_empty_waker() {
            self.inner.borrow_mut().set_waker(waker);
        }
    }

    #[inline]
    fn wake(&self) {
        self.inner.borrow_mut().wake();
    }

    #[inline]
    fn state(&self) -> NotifyState {
        self.inner.borrow().state()
    }

    #[inline]
    fn count_notifiers(&self) -> u32 {
        self.inner.borrow().count_notifiers()
    }
}

impl<'n> Future for Notified<'n> {
    type Output = ();

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.state() {
            NotifyState::Init => {
                self.set_state(NotifyState::Waiting);
                self.set_waker(cx.waker());
                Poll::Pending
            }
            NotifyState::Waiting => Poll::Pending,
            NotifyState::Notified => {
                // Turn back into `Waiting` state regardless of there is no notifier left.
                self.set_state(NotifyState::Waiting);
                self.set_waker_if_empty(cx.waker());
                Poll::Ready(())
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum NotifyState {
    Init,
    Waiting,
    Notified,
}

pub struct NotifyInner {
    state: NotifyState,
    waker: Option<Waker>,
    notifiers: u32,
}

impl NotifyInner {
    #[inline]
    fn new() -> Self {
        Self {
            state: NotifyState::Init,
            waker: None,
            notifiers: 0,
        }
    }

    /// Track the number of active notifiers.<br />
    /// This must be and only be run when a notifier is created.
    #[inline]
    fn create_notifier(&mut self) {
        self.notifiers += 1;
    }

    /// Track the number of active notifiers.<br />
    /// This must be and only be run when a notifier is droped.
    #[inline]
    fn drop_notifier(&mut self) {
        self.notifiers -= 1;
    }

    #[inline]
    fn set_state(&mut self, state: NotifyState) {
        self.state = state;
    }

    #[inline]
    fn set_waker(&mut self, waker: &Waker) {
        self.waker = Some(waker.clone());
    }

    #[inline]
    fn is_empty_waker(&self) -> bool {
        self.waker.is_none()
    }

    #[inline]
    fn state(&self) -> NotifyState {
        self.state
    }

    /// Wake up notify and consume the waker so it's only wake one and wait for `Notify` to run.<br />
    /// `waker` will be set when `NotifyState == Init | Wake`.
    /// The return value is just for monitoring.
    #[inline]
    fn wake(&mut self) -> bool {
        self.set_state(NotifyState::Notified);

        self.waker.take().map(Waker::wake).is_some()
    }

    #[inline]
    fn count_notifiers(&self) -> u32 {
        self.notifiers
    }
}
