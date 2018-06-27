//! In exercise 2, we parallelized hashing a single file, but what if we have
//! many small files?  Ideally we'd like to pipeline hashing multiple files in
//! parallel to ensure we're fully utilizing our system's compute and IO
//! resources.
//!
//! If we want to be able to process multiple files at once, we can't perform
//! blocking IO on the "main" thread orchestrating the work, since blocking on
//! IO may prevent the scheduling thread from queueing work to the hash pool,
//! causing our pipeline to stall.  Check out `src/io_pool.rs` and come back
//! here.
//!
//! Now that we have asynchronous hashing and file streaming components, how do
//! we connect them to the directory traversal?  The first step will be to make
//! `hash_tree` asynchronous so it can call itself recursively.  Note the
//! updated signature below.
//!
//! The easiest way to create a `Box<Future<...>>` is to use the `async_boxed!`
//! macro.  This macro declares an asynchronous block: If its inner expression
//! has type `Result<T, E>`, the async block has type
//! `Box<Future<Item=T, Error=E>>`.  Within an async block, you can use
//! the `await!` macro to "block" on the result of a future.  For example,
//! ```no_run
//! async_boxed!({
//!     let a = await!(something_expensive())?;
//!     let b = await!(something_else(a))?;
//!     Ok(a + b)
//! })
//! ```
//! is a `Box<Future>` that first does `something_expensive`, failing the future
//! if that fails, performs `something_else`, propagating errors again, and then
//! returns the sum of their results.
//!
//! Just like `await!` helps sequence futures (like `;` in synchronous code),
//! the `for_stream!` macro helps process a stream (like a `for` loop).
//! ```no_run
//! async_boxed!({
//!     let mut total = 0;
//!     for_stream!((value in stream) {
//!         total += value;
//!     });
//!     Ok(total)
//! })
//! ```
//! With these primitives, implement a parallel directory traversal.  For a
//! directory, process all of its children recursively in parallel:  The
//! `futures::stream::futures_ordered` combinator is helpful for turning a list
//! of futures into a stream of results.
//!
//! For a file we can first stream in its blocks and then hash them in parallel
//! with our two pools.  However, simply using `for_stream` to consume the
//! stream and then `await!`ing the hash results isn't good enough, since we'd
//! want to hash multiple blocks in parallel if our IO throughput is greater
//! than our throughput hashing a single block at a time.
//!
//! To solve this, create a *stream of futures* from the stream of blocks using
//! the `map` combinator, and then use the `buffered` combinator to turn the
//! stream of futures into a stream of results.  Internally, the `buffered`
//! combinator kicks off a bounded number of futures, buffers their results, and
//! returns them in order.  Then, we can use `for_stream!` to consume the
//! buffered stream in order.
//!
#![feature(generators)]
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

fn hash_tree(path: PathBuf, hash_pool: HashPool, block_size: usize) -> Box<Future<Item=[u8; 32], Error=io::Error>> {
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
