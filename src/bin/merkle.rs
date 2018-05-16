#![feature(catch_expr)]
extern crate crypto;
extern crate futures;
extern crate futures_cpupool;
extern crate hex_slice;

use std::collections::VecDeque;
use std::cell::RefCell;
use std::fs::File;
use std::io::{
    self,
    Read,
};
use std::path::PathBuf;
use std::rc::Rc;

use crypto::digest::Digest;
use crypto::sha2::Sha256;
use futures::{
    stream,
    Future,
    IntoFuture,
    Stream,
};
use futures::unsync::oneshot;
use futures_cpupool::CpuPool;
use hex_slice::AsHex;

#[derive(Clone)]
struct Semaphore {
    inner: Rc<RefCell<SemaphoreInner>>,
}

impl Semaphore {
    fn new(max: usize) -> Self {
        let inner = SemaphoreInner { count: 0, max, waiting: VecDeque::new() };
        Self { inner: Rc::new(RefCell::new(inner)) }
    }

    fn increment(&self) -> impl Future<Item=SemaphoreGuard, Error=io::Error> {
        let (tx, rx) = oneshot::channel();
        {
            let mut inner = self.inner.borrow_mut();
            if inner.count >= inner.max {
                inner.waiting.push_back(tx);
            } else {
                inner.count += 1;
                drop(inner);
                // We're holding a reference to `rx` above.
                assert!(tx.send(SemaphoreGuard { inner: self.inner.clone() }).is_ok());
            }
        }
        rx.map_err(|_| io::ErrorKind::Interrupted.into())
    }
}

struct SemaphoreInner {
    count: usize,
    max: usize,
    waiting: VecDeque<oneshot::Sender<SemaphoreGuard>>,
}

struct SemaphoreGuard {
    inner: Rc<RefCell<SemaphoreInner>>,
}

impl Drop for SemaphoreGuard {
    fn drop(&mut self) {
        let mut inner = self.inner.borrow_mut();
        if let Some(tx) = inner.waiting.pop_front() {
            drop(inner);
            // If the other half is gone, the guard we drop here will *itself*
            // pop the next waiter off the queue.
            let _ = tx.send(SemaphoreGuard { inner: self.inner.clone() });
        } else {
            // Decrement if no one was around to take our slot.
            inner.count -= 1;
        }
    }
}

#[derive(Clone)]
struct HashPool {
    pool: CpuPool
}

impl HashPool {
    fn new(num_threads: usize) -> Self {
        let pool = CpuPool::new(num_threads);
        Self { pool }
    }

    fn hash(&self, mut hasher: Sha256, buf: Vec<u8>) -> impl Future<Item=Sha256, Error=io::Error> {
        self.pool.spawn_fn(move || {
            hasher.input(&buf[..]);
            Ok(hasher)
        })
    }
}

#[derive(Clone)]
struct IOPool {
    pool: CpuPool,
    bufsize: usize,
}

impl IOPool {
    fn new(num_threads: usize, bufsize: usize) -> Self {
        let pool = CpuPool::new(num_threads);
        Self { pool, bufsize }
    }

    fn stream_file(&self, f: File) -> impl Stream<Item=Vec<u8>, Error=io::Error> {
        let start_state = (self.clone(), Some(f));
        stream::unfold(start_state, move |(self_, f)| {
            let mut file = match f {
                None => return None,
                Some(f) => f,
            };
            let read_future = self_.pool.clone().spawn_fn(move || {
                let mut buf = vec![0u8; self_.bufsize];
                let num_read = file.read(&mut buf[..])?;
                buf.truncate(num_read);
                let next_state = if num_read == 0 {
                    (self_, None)
                } else {
                    (self_, Some(file))
                };
                Ok((buf, next_state))
            });
            Some(read_future)
        })
    }
}

fn hash_tree(p: PathBuf, hash_pool: HashPool, io_pool: IOPool, semaphore: Semaphore) -> Box<Future<Item=[u8; 32], Error=io::Error>> {
    if p.is_dir() {
        let children: Result<_, io::Error> = do catch {
            let mut paths = vec![];
            for entry in p.read_dir()? {
                paths.push(entry?.path());
            }
            paths
        };
        let result = children.into_future()
            .and_then(move |mut children| {
                children.sort();
                let futures = children.into_iter()
                    .map(|p| hash_tree(p, hash_pool.clone(), io_pool.clone(), semaphore.clone()));
                stream::futures_ordered(futures)
                    .fold(Sha256::new(), |mut hasher, hash| {
                        hasher.input(&hash[..]);
                        Ok::<_, io::Error>(hasher)
                    })
                    .map(|hasher| finish_sha256(hasher))
            });
        Box::new(result)
    } else {
        let result = semaphore.increment()
            .and_then(move |guard| File::open(p).map(move |f| (guard, f)))
            .and_then(move |(guard, file)| {
                io_pool.stream_file(file)
                    .fold(Sha256::new(), move |hasher, buf| hash_pool.hash(hasher, buf))
                    .map(move |hasher| {
                        drop(guard);
                        finish_sha256(hasher)
                    })
            });
        Box::new(result)
    }
}

fn finish_sha256(mut hasher: Sha256) -> [u8; 32] {
    let mut out = [0u8; 32];
    hasher.result(&mut out[..]);
    out
}

fn main() -> Result<(), io::Error> {
    let num_hashers = 4;
    let num_io = 4;
    let bufsize = 4096;
    let max_file_handles = 512;

    let hashers = HashPool::new(num_hashers);
    let readers = IOPool::new(num_io, bufsize);
    let semaphore = Semaphore::new(max_file_handles);

    let path_str = std::env::args().nth(1).unwrap_or_else(|| ".".to_owned());
    let path = PathBuf::from(path_str);
    let result = hash_tree(path.clone(), hashers, readers, semaphore).wait()?;
    println!("Final result: {:?} -> {:x}", path, result.as_hex());
    Ok(())
}
