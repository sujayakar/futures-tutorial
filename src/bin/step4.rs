#![feature(generators)]

extern crate crypto;
extern crate futures;
extern crate hex_slice;
#[macro_use]
extern crate futures_tutorial;

use std::io::{
    self,
};
use std::path::PathBuf;

use futures::{stream, Future, Stream};
use crypto::digest::Digest;
use crypto::sha2::Sha256;
use hex_slice::AsHex;
use futures_tutorial::{path_filename, finish_sha256};
use futures_tutorial::hash_pool::HashPool;
use futures_tutorial::io_pool::IOPool;
use futures_tutorial::semaphore::Semaphore;

fn hash_tree(path: PathBuf, hash_pool: HashPool, io_pool: IOPool, semaphore: Semaphore) -> Box<Future<Item=[u8; 32], Error=io::Error>> {
    async_boxed!({
        if path.is_dir() {
            let guard = await!(semaphore.increment())?;
            let mut paths = vec![];
            for entry in path.read_dir()? {
                paths.push(entry?.path());
            }
            paths.sort();
            drop(guard);

            let child_hashes = paths.into_iter()
                .map(move |path| hash_with_filename(path, &hash_pool, &io_pool, &semaphore));

            let mut hasher = Sha256::new();
            for_stream!(((filename, hash) in stream::futures_ordered(child_hashes)) {
                hasher.input(filename.as_bytes());
                hasher.input(&hash[..]);
            });
            Ok(finish_sha256(hasher))
        }
        else {
            let num_threads = hash_pool.num_threads;
            let guard = await!(semaphore.increment())?;
            let stream = io_pool.stream_file(path)
                .map(move |block| hash_pool.hash(block))
                .buffered(num_threads);

            let mut hasher = Sha256::new();
            for_stream!((block_hash in stream) {
                hasher.input(&block_hash[..]);
            });
            drop(guard);
            Ok(finish_sha256(hasher))
        }
    })
}

fn hash_with_filename(path: PathBuf, hash_pool: &HashPool, io_pool: &IOPool, semaphore: &Semaphore) -> impl Future<Item=(String, [u8; 32]), Error=io::Error> {
    let filename = path_filename(&path);
    hash_tree(path, hash_pool.clone(), io_pool.clone(), semaphore.clone())
        .map(move |hash| (filename, hash))
}

fn main() -> Result<(), io::Error> {
    let block_size = 4096;
    let hash_pool = HashPool::new(4);
    let io_pool = IOPool::new(2, block_size, 1);
    let semaphore = Semaphore::new(512);

    let path_str = std::env::args().nth(1).unwrap_or_else(|| ".".to_owned());
    let path = PathBuf::from(path_str);
    let result = hash_tree(path.clone(), hash_pool, io_pool, semaphore).wait()?;
    println!("Final result: {:?} -> {:x}", path, result.as_hex());
    Ok(())
}
