//! Fill out `hash_tree` to write a function that computes a "Merkle hash" of
//! the directory tree specified by `path`.
//!
//! Given `block_size`, define the hash of a regular file by splitting it up
//! into contiguous blocks.  Compute the SHA256 of each block, concatenate
//! the hashes, and then compute the SHA256 of the hashes.
//!
//! For the hash of a directory, recursively compute the hash of its children to
//! get a sequence of pairs `(child_filename, child_hash)`.  Sort this sequence
//! in ascending filename order, and then concatentate it all, encoding the
//! filename as UTF-8.  Then, hash the concatenated byte stream to generate the
//! directory's hash.
//!
//! I've left the imports from my solution (at `solution1.rs`) for convenience.
//! Implement `hash_tree` and then try it on a few inputs!
//!
//! Running `cargo run --bin solution1` with a few `/examples`, I get...
//! - "examples/UCD/StandardizedVariants.txt" -> 52a50421ed7e7818d90f70c1601df02d6e3df6f87b7413c607f70b2cf90703b8
//! - "examples/UCD/NormalizationTest.txt" -> 73963f19ac74888b08db2e09c3660edc5bd750ab473d9e27301666e0537b7f18
//! - "examples/UCD" -> 94ed5176652e905430ba509d89adb0a3863b15c61a672899169558b049e18a28
//! Be sure that your implementation matches, and then you can always go back to
//! this simple sequential version as we start to parallelize it.

extern crate crypto;
extern crate hex;
extern crate futures_tutorial;

use std::fs::File;
use std::io::{
    self,
    Read,
};
use std::path::PathBuf;

use crypto::digest::Digest;
use crypto::sha2::Sha256;
use hex::ToHex;
use futures_tutorial::{
    path_filename,
    finish_sha256,
};

fn hash_tree(path: PathBuf, block_size: usize) -> io::Result<[u8; 32]> {
    unimplemented!();
}

fn main() -> Result<(), io::Error> {
    let path_str = std::env::args().nth(1).unwrap_or_else(|| ".".to_owned());
    let path = PathBuf::from(path_str);
    let result = hash_tree(path.clone(), 4096)?;
    println!("Final result: {:?} -> {}", path, result.to_hex());
    Ok(())
}
