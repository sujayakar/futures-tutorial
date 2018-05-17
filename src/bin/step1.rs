extern crate crypto;
extern crate hex_slice;
extern crate futures_tutorial;

use std::fs::File;
use std::io::{
    self,
    Read,
};
use std::path::PathBuf;

use crypto::digest::Digest;
use crypto::sha2::Sha256;
use hex_slice::AsHex;
use futures_tutorial::{
    path_filename,
    finish_sha256,
};

fn hash_tree(path: PathBuf, block_size: usize) -> io::Result<[u8; 32]> {
    if path.is_dir() {
        let mut paths = vec![];
        for entry in path.read_dir()? {
            paths.push(entry?.path());
        }
        paths.sort();

        let mut hasher = Sha256::new();
        for path in paths {
            let filename = path_filename(&path);
            let child_hash = hash_tree(path, block_size)?;
            hasher.input(filename.as_bytes());
            hasher.input(&child_hash[..]);
        }
        Ok(finish_sha256(hasher))
    }
    else {
        let mut file = File::open(path)?;
        let mut hasher = Sha256::new();
        while let Some(block_hash) = hash_next_block(&mut file, block_size)? {
            hasher.input(&block_hash[..]);
        }
        Ok(finish_sha256(hasher))
    }
}

fn hash_next_block(f: &mut File, block_size: usize) -> io::Result<Option<[u8; 32]>> {
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; block_size];
    let mut total_read = 0;
    while total_read < block_size {
        let num_read = f.read(&mut buf[..block_size - total_read])?;
        if num_read == 0 {
            break;
        }
        hasher.input(&buf[..num_read]);
        total_read += num_read;
    }
    Ok(if total_read == 0 { None } else { Some(finish_sha256(hasher)) })
}

fn main() -> Result<(), io::Error> {
    let path_str = std::env::args().nth(1).unwrap_or_else(|| ".".to_owned());
    let path = PathBuf::from(path_str);
    let result = hash_tree(path.clone(), 4096)?;
    println!("Final result: {:?} -> {:x}", path, result.as_hex());
    Ok(())
}
