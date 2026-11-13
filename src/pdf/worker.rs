use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Sender},
    },
    thread::{self, JoinHandle},
    time::Instant,
};

use crate::document::ReferenceLines;
use anyhow::{Context, Result};

use super::{
    OpenResponse, PdfResponse, SaveError, SaveResponse, SaveSnapshot, SavedPdf, TileKey,
    TileResponse, backend::PdfSession,
};

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
        reference_lines: ReferenceLines,
    },
    Close,
    Save {
        request: u64,
        snapshot: SaveSnapshot,
        enqueued: Instant,
    },
    RemoveDraft(PathBuf),
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
                            reference_lines,
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
                            opened.set_reference_lines(reference_lines);
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
                        Command::Close => {
                            session.take();
                        }
                        Command::Save {
                            request,
                            snapshot,
                            enqueued,
                        } => {
                            let started = Instant::now();
                            let result =
                                save_snapshot(&snapshot, session.as_ref()).map_err(|error| {
                                    log::warn!("PDF save failed: {error:#}");
                                    SaveError {
                                        message: format!("{error:#}"),
                                        conflict: error
                                            .downcast_ref::<crate::storage::SaveConflict>()
                                            .is_some(),
                                    }
                                });
                            log::info!(
                                "PDF save r{}: queue {:.1} ms, transaction {:.1} ms, success={}",
                                snapshot.revision,
                                started.duration_since(enqueued).as_secs_f64() * 1000.0,
                                started.elapsed().as_secs_f64() * 1000.0,
                                result.is_ok()
                            );
                            deliver(PdfResponse::Saved(SaveResponse {
                                document: snapshot.document,
                                request,
                                revision: snapshot.revision,
                                result,
                            }));
                        }
                        Command::RemoveDraft(path) => {
                            let result = match std::fs::remove_file(&path) {
                                Ok(()) => Ok(()),
                                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                                    Ok(())
                                }
                                Err(error) => Err(format!("草稿副本清理失败：{error}")),
                            };
                            deliver(PdfResponse::DraftRemoved { path, result });
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
        self.render_pages(document, Vec::new(), keys, ReferenceLines::default())
    }

    pub fn render_pages(
        &self,
        document: u64,
        pages: Vec<u32>,
        keys: Vec<TileKey>,
        reference_lines: ReferenceLines,
    ) -> Result<u64> {
        let request = self.cancel_render();
        self.sender
            .send(Command::Render {
                document,
                request,
                pages,
                keys,
                reference_lines,
            })
            .context("MuPDF worker stopped")?;
        Ok(request)
    }

    pub fn cancel_render(&self) -> u64 {
        let request = NEXT_REQUEST.fetch_add(1, Ordering::Relaxed) + 1;
        self.latest_render.store(request, Ordering::Release);
        request
    }

    pub fn save(&self, snapshot: SaveSnapshot) -> Result<u64> {
        let request = NEXT_REQUEST.fetch_add(1, Ordering::Relaxed) + 1;
        self.sender
            .send(Command::Save {
                request,
                snapshot,
                enqueued: Instant::now(),
            })
            .context("MuPDF worker stopped")?;
        Ok(request)
    }

    /// FIFO with saves: a late write cannot recreate a discarded recovery copy.
    pub fn remove_draft(&self, path: PathBuf) {
        let _ = self.sender.send(Command::RemoveDraft(path));
    }

    fn invalidate(&self) -> u64 {
        let request = NEXT_REQUEST.fetch_add(1, Ordering::Relaxed) + 1;
        self.latest.store(request, Ordering::Release);
        request
    }
}

fn save_snapshot(snapshot: &SaveSnapshot, session: Option<&(u64, PdfSession)>) -> Result<SavedPdf> {
    use crate::storage;
    // Save As observes an existing destination before serialization. A save to
    // the current path must still match the bytes last opened/saved by us.
    let expected = match &snapshot.tracked_target {
        Some((path, fingerprint)) if storage::same_file_path(path, &snapshot.path) => {
            Some(*fingerprint)
        }
        _ if snapshot.require_new_target => None,
        _ => storage::fingerprint(&snapshot.path)?,
    };
    let (bytes, _) = if let Some(document) = snapshot.document {
        let (id, opened) = session.context("PDF session was closed before saving")?;
        anyhow::ensure!(*id == document, "PDF session changed before saving");
        super::save::serialize_session(
            opened,
            &snapshot.strokes,
            snapshot.document_id,
            snapshot.reference_lines,
            &snapshot.appended_pages,
        )?
    } else {
        let draft = super::save::draft_pdf(snapshot.draft_page)?;
        super::save::serialize_with_annotations(
            &draft,
            &snapshot.strokes,
            snapshot.document_id,
            snapshot.reference_lines,
            &snapshot.appended_pages,
            &[],
        )?
    };
    let fingerprint = storage::save_atomic(&snapshot.path, expected, &bytes, |path| {
        let disk = super::save::read_pdf(path)?;
        anyhow::ensure!(
            disk == bytes,
            "On-disk PDF differs from serialized snapshot"
        );
        // The serialized bytes already passed structural/metadata validation.
        // Byte equality proves the synced file is that same validated PDF.
        Ok(())
    })?;
    Ok(SavedPdf {
        path: snapshot.path.clone(),
        fingerprint,
    })
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
mod tests;
