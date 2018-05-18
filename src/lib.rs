// Step 1: Implemement it synchronously.
// Step 2: Implement `HashPool`: teaches simple `CpuPool`, maybe we do an explicit version w/`oneshot`
// Step 3: Implement `IOPool`: teaches `mpsc`, `Stream`s
// Step 4: Implement asynchronous Merkle algorithm that moves file reads off thread and parallelizes them: teaches `async/await`.
// Step 5: Implement `Semaphore`: teaches "shared-memory" style programming with `Rc` and `RefCell`.
// Step 6: Re-implement Step 4 with combinators.
// Step 7: Manually implement `and_then` and use it yourself.
// Step 8: Re-implement `oneshot` with manual task parking.
// Step 9: Performance tuning with Cyclotron.

#![feature(catch_expr)]
#![feature(generators)]
#![feature(generator_trait)]

extern crate crypto;
extern crate crossbeam_channel;
extern crate futures;

pub mod async_await;
pub mod hash_pool;
pub mod io_pool;
pub mod semaphore;

pub use async_await::*;

use std::path::Path;
use crypto::digest::Digest;
use crypto::sha2::Sha256;

pub fn path_filename(p: &Path) -> String {
    p.file_name()
        .expect("Traversed to relative path?")
        .to_str()
        .expect("Invalid Unicode in filesystem")
        .to_owned()
}

pub fn finish_sha256(mut hasher: Sha256) -> [u8; 32] {
    let mut out = [0u8; 32];
    hasher.result(&mut out[..]);
    out
}
