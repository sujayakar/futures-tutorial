use std::cell::RefCell;
use std::collections::VecDeque;
use std::io;
use std::rc::Rc;

use futures::Future;
use futures::unsync::oneshot;

#[derive(Clone)]
pub struct Semaphore {
    inner: Rc<RefCell<SemaphoreInner>>,
}

impl Semaphore {
    pub fn new(max: usize) -> Self {
        let inner = SemaphoreInner { count: 0, max, waiting: VecDeque::new() };
        Self { inner: Rc::new(RefCell::new(inner)) }
    }

    pub fn increment(&self) -> impl Future<Item=SemaphoreGuard, Error=io::Error> {
        let (tx, rx) = oneshot::channel();
        {
            let mut inner = self.inner.borrow_mut();
            if inner.count >= inner.max {
                inner.waiting.push_back(tx);
            } else {
                inner.count += 1;
                drop(inner);
                // We're holding a reference to `rx` above.
                assert!(tx.send(SemaphoreGuard { inner: self.inner.clone() }).is_ok());
            }
        }
        rx.map_err(|_| io::ErrorKind::Interrupted.into())
    }
}

struct SemaphoreInner {
    count: usize,
    max: usize,
    waiting: VecDeque<oneshot::Sender<SemaphoreGuard>>,
}

pub struct SemaphoreGuard {
    inner: Rc<RefCell<SemaphoreInner>>,
}

impl Drop for SemaphoreGuard {
    fn drop(&mut self) {
        let mut inner = self.inner.borrow_mut();
        if let Some(tx) = inner.waiting.pop_front() {
            drop(inner);
            // If the other half is gone, the guard we drop here will *itself*
            // pop the next waiter off the queue.
            let _ = tx.send(SemaphoreGuard { inner: self.inner.clone() });
        } else {
            // Decrement if no one was around to take our slot.
            inner.count -= 1;
        }
    }
}
