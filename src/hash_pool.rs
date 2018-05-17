use std::io;
use std::thread;

use futures::Future;
use futures::sync::oneshot;
use crypto::digest::Digest;
use crypto::sha2::Sha256;
use crossbeam_channel;

use finish_sha256;

struct HashRequest(Vec<u8>, oneshot::Sender<[u8; 32]>);

#[derive(Clone)]
pub struct HashPool {
    pub num_threads: usize,
    sender: crossbeam_channel::Sender<HashRequest>,
}

impl HashPool {
    pub fn new(num_threads: usize) -> Self {
        let (sender, receiver) = crossbeam_channel::unbounded();
        for _ in 0..num_threads {
            let thread_receiver = receiver.clone();
            thread::spawn(move || {
                while let Ok(HashRequest(buf, sender)) = thread_receiver.recv() {
                    let mut hasher = Sha256::new();
                    hasher.input(&buf[..]);
                    // We don't care if the receiver has gone away.
                    let _ = sender.send(finish_sha256(hasher));
                }
            });
        }
        Self { num_threads, sender }
    }

    pub fn hash(&self, buf: Vec<u8>) -> impl Future<Item=[u8; 32], Error=io::Error> {
        let (completion, future) = oneshot::channel();
        self.sender.send(HashRequest(buf, completion))
            .expect("Unbounded channel can't overflow");
        future.map_err(|_| io::ErrorKind::Interrupted.into())
    }
}
