extern crate crypto;
extern crate futures;
extern crate hex_slice;
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
    if path.is_dir() {
        let listed_dir = semaphore.increment()
            .and_then(move |guard| {
                let mut paths = vec![];
                for entry in path.read_dir()? {
                    paths.push(entry?.path());
                }
                paths.sort();
                drop(guard);
                Ok(paths)
            });

        let result = listed_dir.and_then(move |paths| {
            let child_hashes = paths.into_iter()
                .map(|path| hash_with_filename(path, &hash_pool, &io_pool, &semaphore));
            stream::futures_ordered(child_hashes)
                .fold(Sha256::new(), |mut hasher, (filename, hash)| {
                    hasher.input(filename.as_bytes());
                    hasher.input(&hash[..]);
                    Ok::<_, io::Error>(hasher)
                })
                .map(finish_sha256)
        });
        Box::new(result)
    } else {
        let num_threads = hash_pool.num_threads;
        let result = semaphore.increment()
            .and_then(move |guard| {
                io_pool.stream_file(path)
                    .map(move |block| hash_pool.hash(block))
                    .buffered(num_threads)
                    .fold(Sha256::new(), move |mut hasher, block_hash| {
                        hasher.input(&block_hash[..]);
                        Ok::<_, io::Error>(hasher)
                    })
                    .map(move |hasher| {
                        drop(guard);
                        finish_sha256(hasher)
                    })
            });
        Box::new(result)
    }
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
