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
use std::fs;
use std::fs::File;
use std::path::PathBuf;
use futures::{Future};
use crypto::digest::Digest;
use crypto::sha2::Sha256;
use hex::ToHex;
use futures_tutorial::{path_filename, finish_sha256};
use futures_tutorial::hash_pool::HashPool;

fn hash_tree(path: PathBuf, pool : &HashPool, block_size: usize) -> io::Result<[u8; 32]> {
    let mut hasher = Sha256::new();
    let mut buffer : Vec<u8> = vec![];
    if path.is_dir() {
        let mut paths = vec![];
        for entry in fs::read_dir(path)? {
            paths.push(entry?.path());
        }
        paths.sort();

        // TODO: can we make it parallel and join()?
        for ch_path in paths {
            // TODO: this scope here to make re-borrowing ch_path possible
            // is it the right way?
            {
                let name = ch_path.file_name().unwrap().to_str().unwrap();
                // TODO: replace it by append somehow?
                for byte in name.as_bytes() {
                    buffer.push(*byte);
                }
            }
            
            for byte in hash_tree(ch_path, pool, block_size)?.iter() {
                buffer.push(*byte);
            }
        }
        hasher.input(buffer.as_slice());
    } else {
        // TODO: can we make it parallel and join()?
        for chunk_hash in hash_file(&path, pool, block_size)?.iter() {
            hasher.input(chunk_hash);
        }
    }
    let res = finish_sha256(hasher);
    Ok(res)
}

fn hash_file(path: &PathBuf, pool: &HashPool, block_size: usize) -> io::Result<Vec<[u8; 32]>> {
    let mut f = File::open(path)?;
    let mut buffer = vec![0;block_size];
    let mut result_futures = vec![];
    loop {
        let read_count = f.read(&mut buffer)?;
        if read_count == 0 { break }
        if read_count < block_size {
            buffer = buffer[..read_count].to_vec();
        }
        result_futures.push(pool.hash(buffer.clone()));
    }

    let mut result = vec![];
    for future in result_futures {
        result.push(future.wait()?);
    }

    return Ok(result)
}

fn main() -> Result<(), io::Error> {
    let hash_pool = HashPool::new(4);
    let path_str = std::env::args().nth(1).unwrap_or_else(|| ".".to_owned());
    let path = PathBuf::from(path_str);
    let result = hash_tree(path.clone(), &hash_pool, 4096)?;
    println!("Final result: {:?} -> {}", path, result.to_hex());
    Ok(())
}
