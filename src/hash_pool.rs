//! If you're looking at `Future`s for the first time, just take a look at
//! `hash`'s signature here and move on.  It takes in a buffer and returns
//! something that implements `Future`, returning a `[u8; 32]` on success and an
//! `io:Error` on failure.
//!
//! Otherwise, how are `Future`s built?  Across thread boundaries, they often
//! use `oneshot::channel`s, a channel for communicating just a single value.
//! Within `hash`, we create a channel, send the sending half to the threadpool
//! and return the receiving half as the `Future`.  See inline for details.
//!
//! To send messages to the threadpool, we use `crossbeam_channel`, a crate
//! providing a simple MPMC (multiple producer, multiple consumer) channel.
//! Since it allows multiple producers, the sender half is `Clone`, so our
//! `HashPool` as a whole is made `Clone` as well.
//!
use std::io;
use std::thread;

use futures::Future;
use futures::sync::oneshot;
use crypto::digest::Digest;
use crypto::sha2::Sha256;
use crossbeam_channel;

use finish_sha256;

// We put pairs of the request argument (the buffer) and the response channel
// (the `oneshot`) on the thread pool's work queue.
struct HashRequest(Vec<u8>, oneshot::Sender<[u8; 32]>);

#[derive(Clone)]
pub struct HashPool {
    pub num_threads: usize,
    sender: crossbeam_channel::Sender<HashRequest>,
}

impl HashPool {
    pub fn new(num_threads: usize) -> Self {
        // This sender/receiver pair is for putting work onto our work queue and
        // then dequeuing it.  Both halves are `Clone` and `Send`, so we can
        // create a fresh copy that we `move` onto each thread's closure.
        let (sender, receiver) = crossbeam_channel::unbounded();
        for _ in 0..num_threads {
            // Create a clone of the receiver for our new thread.  In production
            // code we'd keep the return value of `thread::spawn` around to join
            // on the thread when we're `Drop`ping the `HashPool` to ensure
            // we're not leaking threads.
            let thread_receiver = receiver.clone();
            thread::spawn(move || {
                // Crossbeam's channels return `Err` when the other side has
                // gone away, so just loop until the other side hangs up.  Also
                // note that `recv` is a blocking receive, parking our thread
                // until a message shows up.
                while let Ok(HashRequest(buf, sender)) = thread_receiver.recv() {
                    let mut hasher = Sha256::new();
                    hasher.input(&buf[..]);

                    // `oneshot::Sender`s error when the other side has gone
                    // away, but we don't particularly care in this situation.
                    let _ = sender.send(finish_sha256(hasher));
                }
            });
        }
        Self { num_threads, sender }
    }

    pub fn hash(&self, buf: Vec<u8>) -> impl Future<Item=[u8; 32], Error=io::Error> {
        let (completion, future) = oneshot::channel();
        self.sender.send(HashRequest(buf, completion))
            .expect("Unbounded channel can't overflow");

        // `oneshot::Receiver`s error when the other side has gone away.  That
        // can only happen in our case if a work thread panics, so map that
        // error to an `io::Error` to get the types to line up conveniently.
        future.map_err(|_| io::ErrorKind::Interrupted.into())
    }
}
