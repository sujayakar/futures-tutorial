extern crate crypto;
extern crate futures;
extern crate hex_slice;
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
use hex_slice::AsHex;
use futures_tutorial::{path_filename, finish_sha256};
use futures_tutorial::hash_pool::HashPool;

fn hash_tree(path: PathBuf, hash_pool: HashPool, block_size: usize) -> io::Result<[u8; 32]> {
    if path.is_dir() {
        let mut paths = vec![];
        for entry in path.read_dir()? {
            paths.push(entry?.path());
        }
        paths.sort();

        let mut hasher = Sha256::new();
        for path in paths {
            let filename = path_filename(&path);
            let child_hash = hash_tree(path, hash_pool.clone(), block_size)?;
            hasher.input(filename.as_bytes());
            hasher.input(&child_hash[..]);
        }
        Ok(finish_sha256(hasher))
    }
    else {
        let mut file = File::open(path)?;
        let mut futures = vec![];
        while let Some(block) = read_next_block(&mut file, block_size)? {
            futures.push(hash_pool.hash(block));
        }
        let mut hasher = Sha256::new();
        for future in futures {
            let block_hash = future.wait()?;
            hasher.input(&block_hash[..]);
        }
        Ok(finish_sha256(hasher))
    }
}

fn read_next_block(f: &mut File, block_size: usize) -> io::Result<Option<Vec<u8>>> {
    let mut buf = vec![0u8; block_size];
    let mut total_read = 0;
    loop {
        let num_read = f.read(&mut buf[total_read..])?;
        if num_read == 0 {
            break;
        }
        total_read += num_read;
    }
    buf.truncate(total_read);
    Ok(if total_read == 0 { None } else { Some(buf) })
}


fn main() -> Result<(), io::Error> {
    let hash_pool = HashPool::new(4);
    let path_str = std::env::args().nth(1).unwrap_or_else(|| ".".to_owned());
    let path = PathBuf::from(path_str);
    let result = hash_tree(path.clone(), hash_pool, 4096)?;
    println!("Final result: {:?} -> {:x}", path, result.as_hex());
    Ok(())
}
