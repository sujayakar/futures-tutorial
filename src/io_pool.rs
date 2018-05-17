use std::io::{self, Read};
use std::fs::File;
use std::thread;
use std::path::PathBuf;

use futures::{Stream, Sink};
use futures::sync::mpsc;
use crossbeam_channel;

struct StreamFileRequest(PathBuf, mpsc::Sender<Result<Vec<u8>, io::Error>>);

#[derive(Clone)]
pub struct IOPool {
    num_threads: usize,
    window_size: usize,

    sender: crossbeam_channel::Sender<StreamFileRequest>,
}

impl IOPool {
    pub fn new(num_threads: usize, block_size: usize, window_size: usize) -> Self {
        let (sender, receiver) = crossbeam_channel::unbounded();
        for _ in 0..num_threads {
            let thread_receiver = receiver.clone();
            thread::spawn(move || {
                while let Ok(StreamFileRequest(path, tx)) = thread_receiver.recv() {
                    Self::perform_stream(path, tx, block_size);
                }
            });
        }
        Self { num_threads, window_size, sender }
    }

    pub fn stream_file(&self, path: PathBuf) -> impl Stream<Item=Vec<u8>, Error=io::Error> {
        let (tx, rx) = mpsc::channel(self.window_size);
        self.sender.send(StreamFileRequest(path, tx))
            .expect("Unbounded channel can't overflow");
        rx.then(|r| match r {
            Ok(r) => r,
            Err(_) => Err(io::ErrorKind::Interrupted.into()),
        })
    }

    fn perform_stream(path: PathBuf, tx: mpsc::Sender<Result<Vec<u8>, io::Error>>, block_size: usize) {
        let mut tx = tx.wait();

        let result: io::Result<()> = do catch {
            let mut file = File::open(path)?;
            while let Some(block) = Self::read_next_block(&mut file, block_size)? {
                // Abort if the receiver went away
                if tx.send(Ok(block)).is_err() {
                    break;
                }
            }
        };
        // If we hit an error, send it to the stream if it's still there.
        if let Err(e) = result {
            let _ = tx.send(Err(e));
        }
        let _ = tx.flush();
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
}
