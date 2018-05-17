#[doc(hidden)]
pub use futures::Async;
use futures::{
    Future,
    Poll,
};
use std::ops::{
    Generator,
    GeneratorState,
};

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
