use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Sender},
    },
    thread::{self, JoinHandle},
};

use anyhow::{Context, Result};

use super::{OpenResponse, backend::PdfSession};

enum Command {
    Open { request: u64, path: PathBuf },
    Close,
    Shutdown,
}

// Event queues may outlive a suspended Runtime and its worker.
static NEXT_REQUEST: AtomicU64 = AtomicU64::new(0);

pub struct PdfWorker {
    sender: Sender<Command>,
    latest: Arc<AtomicU64>,
    thread: Option<JoinHandle<()>>,
}

impl PdfWorker {
    pub fn new(deliver: impl Fn(OpenResponse) + Send + 'static) -> Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let latest = Arc::new(AtomicU64::new(0));
        let worker_latest = Arc::clone(&latest);
        let thread = thread::Builder::new()
            .name("mupdf".to_owned())
            .spawn(move || {
                let mut session = None;
                while let Ok(mut command) = receiver.recv() {
                    // Opening another document supersedes queued work. Pixel buffers
                    // are produced only for the newest request, not every UI action.
                    while let Ok(newer) = receiver.try_recv() {
                        if matches!(command, Command::Close) {
                            session.take();
                        }
                        command = newer;
                    }
                    match command {
                        Command::Open { request, path } => {
                            if request != worker_latest.load(Ordering::Acquire) {
                                continue;
                            }
                            let result = PdfSession::open(&path).and_then(|opened| {
                                if request != worker_latest.load(Ordering::Acquire) {
                                    return Ok(None);
                                }
                                let preview = opened.preview(&path)?;
                                Ok(Some((opened, preview)))
                            });
                            if request != worker_latest.load(Ordering::Acquire) {
                                continue;
                            }
                            let result = match result {
                                Ok(Some((opened, preview))) => {
                                    session.replace(opened);
                                    Ok(preview)
                                }
                                Ok(None) => continue,
                                Err(error) => {
                                    log::warn!("MuPDF open failed ({}): {error:#}", path.display());
                                    Err(format!("{error:#}"))
                                }
                            };
                            deliver(OpenResponse { request, result });
                        }
                        Command::Close => {
                            session.take();
                        }
                        Command::Shutdown => break,
                    }
                }
                // MuPDF objects are destroyed before this thread's local context.
                drop(session);
            })
            .context("Cannot start MuPDF worker")?;
        Ok(Self {
            sender,
            latest,
            thread: Some(thread),
        })
    }

    pub fn open(&self, path: PathBuf) -> Result<u64> {
        let request = self.invalidate();
        self.sender
            .send(Command::Open { request, path })
            .context("MuPDF worker stopped")?;
        Ok(request)
    }

    pub fn close(&self) {
        self.invalidate();
        let _ = self.sender.send(Command::Close);
    }

    fn invalidate(&self) -> u64 {
        let request = NEXT_REQUEST.fetch_add(1, Ordering::Relaxed) + 1;
        self.latest.store(request, Ordering::Release);
        request
    }
}

impl Drop for PdfWorker {
    fn drop(&mut self) {
        self.invalidate();
        let _ = self.sender.send(Command::Shutdown);
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            log::error!("MuPDF worker panicked");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn queued_opens_are_superseded_and_close_invalidates_pending_work() {
        let (responses, received) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let worker = PdfWorker::new(move |response| {
            let _ = responses.send(response);
            // Hold the worker at a known boundary while the test queues actions.
            let _ = wait.recv_timeout(Duration::from_secs(5));
        })
        .unwrap();
        let missing = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".scratch/no-such-pdf-for-worker-test.pdf");
        let first = worker.open(missing.clone()).unwrap();
        let response = received.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(response.request, first);
        assert!(response.result.is_err());
        let superseded = worker.open(missing.clone()).unwrap();
        let newest = worker.open(missing.clone()).unwrap();
        assert_ne!(superseded, newest);
        release.send(()).unwrap();
        let response = received.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(response.request, newest);

        // Both commands are queued while the callback above still owns the worker.
        worker.open(missing.clone()).unwrap();
        worker.close();
        release.send(()).unwrap();
        assert!(matches!(
            received.recv_timeout(Duration::from_millis(100)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        let reopened = worker.open(missing).unwrap();
        let response = received.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(response.request, reopened);
        release.send(()).unwrap();
        drop(worker);
    }
}
