#![feature(catch_expr)]
#![feature(generators)]
#![feature(generator_trait)]
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
use std::ops::{
    Generator,
    GeneratorState,
};
use std::path::PathBuf;
use std::rc::Rc;

use crypto::digest::Digest;
use crypto::sha2::Sha256;
pub use futures::Async;
use futures::{
    stream,
    Future,
    IntoFuture,
    Poll,
    Sink,
    Stream,
};
use futures::sync::mpsc;
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
    num_threads: usize,
    pool: CpuPool
}

impl HashPool {
    fn new(num_threads: usize) -> Self {
        let pool = CpuPool::new(num_threads);
        Self { num_threads, pool }
    }

    fn hash(&self, block: Vec<u8>) -> impl Future<Item=[u8; 32], Error=io::Error> {
        self.pool.spawn_fn(move || {
            let mut hasher = Sha256::new();
            hasher.input(&block[..]);
            Ok(finish_sha256(hasher))
        })
    }
}

#[derive(Clone)]
struct IOPool {
    pool: CpuPool,
    block_size: usize,
    outstanding_buffers: usize,
}

impl IOPool {
    fn new(num_threads: usize, block_size: usize, outstanding_buffers: usize) -> Self {
        let pool = CpuPool::new(num_threads);
        Self { pool, block_size, outstanding_buffers }
    }

    fn read_block(f: &mut File, block_size: usize) -> io::Result<Vec<u8>> {
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
        Ok(buf)
    }

    fn stream_file(&self, path: PathBuf) -> impl Stream<Item=Vec<u8>, Error=io::Error> {
        let (tx, rx) = mpsc::channel(self.outstanding_buffers);
        let block_size = self.block_size;
        let stream_interrupted = |_| io::Error::from(io::ErrorKind::Interrupted);
        let on_pool = self.pool.spawn_fn(move || {
            let mut tx = tx.wait();
            let result: io::Result<()> = do catch {
                let mut file = File::open(path)?;
                loop {
                    let block = Self::read_block(&mut file, block_size)?;
                    if block.is_empty() {
                        break;
                    }
                    tx.send(Ok(block)).map_err(stream_interrupted)?;
                }
            };
            // If we encountered an error, send it to the stream if it's still there.
            if let Err(e) = result {
                let _ = tx.send(Err(e));
            }
            tx.flush().map_err(stream_interrupted)
        });
        on_pool.forget();
        rx.then(|r| match r {
            Ok(r) => r,
            Err(_) => Err(io::ErrorKind::Interrupted.into()),
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
        let num_threads = hash_pool.num_threads;
        let result = semaphore.increment()
            .and_then(move |guard| {
                io_pool.stream_file(p)
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

#[doc(hidden)]
pub struct GenFuture<T>(T);

// represents a `NotReady` return of an awaited future
#[doc(hidden)]
pub struct AsyncYield;

impl<'a, U, E, T> GenFuture<T>
where
    T: Generator<Yield = AsyncYield, Return = Result<U, E>> + 'a,
{
    pub fn new(inner: T) -> GenFuture<T> {
        GenFuture(inner)
    }
}

impl<U, E, T> Future for GenFuture<T>
where
    T: Generator<Yield = AsyncYield, Return = Result<U, E>>,
{
    type Item = U;
    type Error = E;
    fn poll(&mut self) -> Poll<U, E> {
        // unsafety: this should only be used with generators created from `async!`, which aren't immovable.
        match unsafe { self.0.resume() } {
            GeneratorState::Yielded(AsyncYield) => Ok(Async::NotReady),
            GeneratorState::Complete(r) => Ok(Async::Ready(r?)),
        }
    }
}

/// Constructs a future that executes the contained block, allowing `await!()` etc. to be used inside.
/// The block must return a `Result`; the created future will evaluate to the returned value or error, as appropriate.
#[macro_export]
macro_rules! async {
    ($e:expr) => {{
        // This is the secret sauce that makes all the other macros work.
        // It loops until the provided expression returns `Async::Ready`, yielding out of the generator on
        // `NotReady`, then evaluates to the `Ready` value.
        #[allow(unused)]
        macro_rules! await_poll {
            ($f: expr) => {{
                loop {
                    let poll = $f;
                    #[allow(unreachable_code)]
                    #[allow(unreachable_patterns)]
                    match poll {
                        $crate::Async::NotReady => yield $crate::AsyncYield,
                        $crate::Async::Ready(r) => break r,
                    }
                }
            }};
        }
        $crate::GenFuture::new(move || {
            if false {
                yield $crate::AsyncYield;
            }
            $e
        })
    }};
}

#[macro_export]
macro_rules! for_stream {
    (($item:pat in $s:expr) $b:expr) => {{
        let mut awaited_stream = $s;
        while let Some(item) = await_poll!(awaited_stream.poll()?) {
            let $item = item;
            $b
        }
    }};
}

#[macro_export]
macro_rules! await {
    ($f:expr) => {{
        let mut awaited_future = $f;
        await_poll!(
            // The return type of `poll()` might be `Poll<_, !>`, making the `Err(e)` case unreachable,
            // so we want to allow(unreachable_patterns). However, applying attributes to expressions is unstable
            // (feature(stmt_expr_attributes)) and _downstream_ crates would need to use it. But nested blocks are ok.
            {
                #[allow(unreachable_code)]
                #[allow(unreachable_patterns)]
                {
                    match awaited_future.poll() {
                        Ok($crate::Async::NotReady) => $crate::Async::NotReady,
                        Ok($crate::Async::Ready(x)) => $crate::Async::Ready(Ok(x)),
                        Err(e) => $crate::Async::Ready(Err(e)),
                    }
                }
            }
        )
    }};
}

/// Convenience macro for `Box::new(async!(...))`.
#[macro_export]
macro_rules! async_boxed {
    ($e:expr) => {
        Box::new(async!($e))
    };
}

fn hash_tree2(p: PathBuf, hash_pool: HashPool, io_pool: IOPool, semaphore: Semaphore) -> Box<Future<Item=[u8; 32], Error=io::Error>> {
    async_boxed!({
        let guard = await!(semaphore.increment())?;
        let result = if p.is_dir() {
            let mut paths = vec![];
            for entry in p.read_dir()? {
                paths.push(entry?.path());
            }
            paths.sort();

            let child_hashes = paths.into_iter()
                .map(move |p| hash_tree2(p, hash_pool.clone(), io_pool.clone(), semaphore.clone()));

            let mut hasher = Sha256::new();
            for_stream!((hash in stream::futures_ordered(child_hashes)) {
                hasher.input(&hash[..]);
            });
            finish_sha256(hasher)
        } else {
            let num_threads = hash_pool.num_threads;
            let stream = io_pool.stream_file(p)
                .map(move |block| hash_pool.hash(block))
                .buffered(num_threads);

            let mut hasher = Sha256::new();
            for_stream!((block_hash in stream) {
                hasher.input(&block_hash[..]);
            });
            finish_sha256(hasher)
        };
        drop(guard);
        Ok(result)
    })
}

fn finish_sha256(mut hasher: Sha256) -> [u8; 32] {
    let mut out = [0u8; 32];
    hasher.result(&mut out[..]);
    out
}

// Step 1: Implement `HashPool`: teaches simple `CpuPool`, maybe we do an explicit version w/`oneshot`
// Step 2: Implement synchronous Merkle algorithm that hashes a file in parallel, moving hashing off-thread.
// Step 3: Implement `IOPool`: teaches `mpsc`, `Stream`s
// Step 4: Implement asynchronous Merkle algorithm that moves file reads off thread and parallelizes them: teaches `async/await`.
// Step 5: Implement `Semaphore`: teaches "shared-memory" style programming with `Rc` and `RefCell`.
// Step 6: Re-implement Step 4 with combinators.
// Step 7: Manually implement `and_then` and use it yourself.
// Step 8: Re-implement `Semaphore` with manual task parking.

fn main() -> Result<(), io::Error> {
    let num_hashers = 4;
    let num_io = 4;
    let block_size = 4096;
    let max_file_handles = 512;

    let hashers = HashPool::new(num_hashers);
    let readers = IOPool::new(num_io, block_size, num_hashers);
    let semaphore = Semaphore::new(max_file_handles);

    let path_str = std::env::args().nth(1).unwrap_or_else(|| ".".to_owned());
    let path = PathBuf::from(path_str);
    let result = hash_tree2(path.clone(), hashers, readers, semaphore).wait()?;
    println!("Final result: {:?} -> {:x}", path, result.as_hex());
    Ok(())
}
