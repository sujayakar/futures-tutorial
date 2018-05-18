//! The first part of the system we will parallelize is computing SHA256 for a
//! single file's blocks.  Doing so will require expressing the computation
//! of asynchronously computing a buffer's SHA256 as a `Future`.  Take a look
//! at `src/hash_pool.rs` and come back here.
//!
//! Okay, so we have the ability to schedule hashing on a thread pool and then
//! get the result back asynchronously.  Then, when we're hashing a file, we
//! want to read the `block_size`d chunks in from the file, kick off all their
//! hashes, and then wait on the results in order to accumulate them into the
//! final blocklist hash.
//!
//! Fill out `hash_tree`, using the `HashPool` to schedule hashing all of a
//! file's blocks and then using `.wait()` to wait on the results in order.  Try
//! it in `--release` mode on a large file and see if you get a speedup!
//!
extern crate crypto;
extern crate futures;
extern crate hex;
extern crate futures_tutorial;

use std::io::{
    self,
    Read,
};
use std::fs::File;
use std::path::PathBuf;
use futures::{Future};
use crypto::digest::Digest;
use crypto::sha2::Sha256;
use hex::ToHex;
use futures_tutorial::{path_filename, finish_sha256};
use futures_tutorial::hash_pool::HashPool;

fn hash_tree(path: PathBuf, hash_pool: HashPool, block_size: usize) -> io::Result<[u8; 32]> {
    unimplemented!()
}

fn main() -> Result<(), io::Error> {
    let hash_pool = HashPool::new(4);
    let path_str = std::env::args().nth(1).unwrap_or_else(|| ".".to_owned());
    let path = PathBuf::from(path_str);
    let result = hash_tree(path.clone(), hash_pool, 4096)?;
    println!("Final result: {:?} -> {}", path, result.to_hex());
    Ok(())
}
