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

use super::{
    ExportResponse, OpenResponse, PdfResponse, TileKey, TileResponse, backend::PdfSession,
};
use crate::document::StrokeRecord;

enum Command {
    Open {
        request: u64,
        path: PathBuf,
    },
    Render {
        document: u64,
        request: u64,
        pages: Vec<u32>,
        keys: Vec<TileKey>,
    },
    Close,
    ExportProbe {
        document: u64,
        request: u64,
        lifecycle: u64,
        stroke: Arc<StrokeRecord>,
    },
    Shutdown,
}

// Event queues may outlive a suspended Runtime and its worker.
static NEXT_REQUEST: AtomicU64 = AtomicU64::new(0);

pub struct PdfWorker {
    sender: Sender<Command>,
    latest: Arc<AtomicU64>,
    latest_render: Arc<AtomicU64>,
    thread: Option<JoinHandle<()>>,
}

impl PdfWorker {
    pub fn new(deliver: impl Fn(PdfResponse) + Send + 'static) -> Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let latest = Arc::new(AtomicU64::new(0));
        let worker_latest = Arc::clone(&latest);
        let latest_render = Arc::new(AtomicU64::new(0));
        let worker_render = Arc::clone(&latest_render);
        let thread = thread::Builder::new()
            .name("mupdf".to_owned())
            .spawn(move || {
                let mut session = None;
                while let Ok(command) = receiver.recv() {
                    match command {
                        Command::Open { request, path } => {
                            if request != worker_latest.load(Ordering::Acquire) {
                                continue;
                            }
                            let result = PdfSession::open(&path).and_then(|mut opened| {
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
                                    session.replace((request, opened));
                                    Ok(preview)
                                }
                                Ok(None) => continue,
                                Err(error) => {
                                    log::warn!("MuPDF open failed ({}): {error:#}", path.display());
                                    Err(format!("{error:#}"))
                                }
                            };
                            deliver(PdfResponse::Opened(OpenResponse { request, result }));
                        }
                        Command::Render {
                            document,
                            request,
                            pages,
                            keys,
                        } => {
                            if request != worker_render.load(Ordering::Acquire) {
                                continue;
                            }
                            let Some((id, opened)) = session.as_mut() else {
                                continue;
                            };
                            if *id != document {
                                continue;
                            }
                            // Deliver each completed item so a complex later page
                            // cannot delay already available visible content.
                            let count = pages.len() + keys.len();
                            let items = pages.into_iter().map(Ok).chain(keys.into_iter().map(Err));
                            for (position, item) in items.enumerate() {
                                if request != worker_render.load(Ordering::Acquire) {
                                    break;
                                }
                                let mut previews = Vec::new();
                                let page = match item {
                                    Ok(page) => page,
                                    Err(key) => key.page,
                                };
                                let result = match item {
                                    Ok(page) => opened.render_preview(page).map(|bitmap| {
                                        previews.push((page, Arc::new(bitmap)));
                                        Vec::new()
                                    }),
                                    Err(key) => {
                                        opened.render_tile(key).map(|tile| vec![Arc::new(tile)])
                                    }
                                }
                                .map_err(|error| format!("{error:#}"));
                                let failed = result.is_err();
                                if request != worker_render.load(Ordering::Acquire) {
                                    break;
                                }
                                deliver(PdfResponse::Tiles(TileResponse {
                                    document,
                                    request,
                                    page,
                                    previews,
                                    complete: failed || position + 1 == count,
                                    result,
                                }));
                                if failed {
                                    break;
                                }
                            }
                        }
                        Command::ExportProbe {
                            document,
                            request,
                            lifecycle,
                            stroke,
                        } => {
                            if lifecycle != worker_latest.load(Ordering::Acquire) {
                                continue;
                            }
                            let Some((id, opened)) = session.as_ref() else {
                                continue;
                            };
                            if *id != document {
                                continue;
                            }
                            let result = super::probe::output_directory()
                                .and_then(|directory| {
                                    let path = directory.join("pressure.pdf");
                                    opened.export_probe(&stroke, &path)?;
                                    Ok(path)
                                })
                                .map_err(|error| {
                                    log::warn!("PDF save probe failed: {error:#}");
                                    format!("{error:#}")
                                });
                            deliver(PdfResponse::Exported(ExportResponse {
                                document,
                                request,
                                result,
                            }));
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
            latest_render,
            thread: Some(thread),
        })
    }

    pub fn open(&self, path: PathBuf) -> Result<u64> {
        self.cancel_render();
        let request = self.invalidate();
        self.sender
            .send(Command::Open { request, path })
            .context("MuPDF worker stopped")?;
        Ok(request)
    }

    pub fn close(&self) {
        self.cancel_render();
        self.invalidate();
        let _ = self.sender.send(Command::Close);
    }

    #[cfg(test)]
    pub fn render(&self, document: u64, keys: Vec<TileKey>) -> Result<u64> {
        self.render_pages(document, Vec::new(), keys)
    }

    pub fn render_pages(&self, document: u64, pages: Vec<u32>, keys: Vec<TileKey>) -> Result<u64> {
        let request = self.cancel_render();
        self.sender
            .send(Command::Render {
                document,
                request,
                pages,
                keys,
            })
            .context("MuPDF worker stopped")?;
        Ok(request)
    }

    pub fn cancel_render(&self) -> u64 {
        let request = NEXT_REQUEST.fetch_add(1, Ordering::Relaxed) + 1;
        self.latest_render.store(request, Ordering::Release);
        request
    }

    pub fn export_probe(&self, document: u64, stroke: Arc<StrokeRecord>) -> Result<u64> {
        let request = NEXT_REQUEST.fetch_add(1, Ordering::Relaxed) + 1;
        self.sender
            .send(Command::ExportProbe {
                document,
                request,
                lifecycle: self.latest.load(Ordering::Acquire),
                stroke,
            })
            .context("MuPDF worker stopped")?;
        Ok(request)
    }

    fn invalidate(&self) -> u64 {
        let request = NEXT_REQUEST.fetch_add(1, Ordering::Relaxed) + 1;
        self.latest.store(request, Ordering::Release);
        request
    }
}

impl Drop for PdfWorker {
    fn drop(&mut self) {
        self.cancel_render();
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
            if let PdfResponse::Opened(response) = response {
                let _ = responses.send(response);
            }
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
