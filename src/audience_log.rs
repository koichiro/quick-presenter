//! Bounded, off-thread JSON Lines logging, separate from diagnostic messages.
use crate::{audience_events::AudienceEvent, diagnostics::RotatingLogWriter};
use std::{
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex, OnceLock,
    },
    thread::JoinHandle,
};

pub(crate) struct CommentLogger {
    sender: Mutex<Option<mpsc::SyncSender<String>>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    failed: Arc<AtomicBool>,
}
static LOGGER: OnceLock<Arc<CommentLogger>> = OnceLock::new();
#[cfg(not(test))]
pub(crate) fn logger() -> Arc<CommentLogger> {
    LOGGER
        .get_or_init(|| {
            Arc::new(CommentLogger::start(
                crate::diagnostics::default_log_file_path()
                    .map(|path| path.with_file_name("audience-comments.jsonl")),
                10 * 1024 * 1024,
            ))
        })
        .clone()
}
pub(crate) fn shutdown() {
    if let Some(logger) = LOGGER.get() {
        logger.shutdown();
    }
}
impl CommentLogger {
    fn start(path: Option<PathBuf>, limit: u64) -> Self {
        let (sender, receiver) = mpsc::sync_channel::<String>(128);
        let failed = Arc::new(AtomicBool::new(false));
        let worker_failed = failed.clone();
        let spawn = std::thread::Builder::new()
            .name("audience-comment-log".into())
            .spawn(move || {
                let result = (|| -> anyhow::Result<()> {
                    let path =
                        path.ok_or_else(|| anyhow::anyhow!("Comment log directory unavailable"))?;
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    let mut writer = RotatingLogWriter::new(&path, limit)?;
                    for line in receiver {
                        writer.write_all(line.as_bytes())?;
                        writer.flush()?;
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    worker_failed.store(true, Ordering::Release);
                    tracing::warn!(%error, "Audience comment logging unavailable");
                }
            });
        if spawn.is_err() {
            failed.store(true, Ordering::Release);
        }
        Self {
            sender: Mutex::new(Some(sender)),
            worker: Mutex::new(spawn.ok()),
            failed,
        }
    }
    /// Called after the UI event loop and audience network worker have stopped.
    fn shutdown(&self) {
        self.sender.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(worker) = self.worker.lock().unwrap_or_else(|e| e.into_inner()).take() {
            if worker.join().is_err() {
                self.failed.store(true, Ordering::Release);
            }
        }
    }
    pub fn failed(&self) -> bool {
        self.failed.load(Ordering::Acquire)
    }
    pub fn submit(
        &self,
        session_id: &str,
        sequence: u64,
        participant: u64,
        event: &AudienceEvent,
    ) -> bool {
        let AudienceEvent::Comment { body, handle } = event else {
            return true;
        };
        let line = serde_json::json!({"received_at": chrono::Utc::now().to_rfc3339(),
            "session_id": session_id, "sequence": sequence, "participant_id": participant,
            "body": body, "handle": handle})
        .to_string()
            + "\n";
        // Disk failure does not stop reception; the presenter sees a warning.
        if self.failed() {
            return true;
        }
        let sender = self.sender.lock().unwrap_or_else(|e| e.into_inner());
        let Some(sender) = sender.as_ref() else {
            return true;
        };
        match sender.try_send(line) {
            Ok(()) => true,
            Err(mpsc::TrySendError::Full(_)) => false,
            Err(mpsc::TrySendError::Disconnected(_)) => {
                self.failed.store(true, Ordering::Release);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn json_lines_rotate_preserve_handles_and_flush_on_shutdown() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("comments.jsonl");
        let logger = CommentLogger::start(Some(path.clone()), 500);
        let event = AudienceEvent::Comment {
            body: "<script>\\n世界".into(),
            handle: " Anonymous ".into(),
        };
        for n in 0..10 {
            assert!(logger.submit("session", n, 1, &event));
        }
        logger.shutdown();
        assert!(!logger.failed());
        let previous = path.with_file_name("comments.jsonl.1");
        for file in [&path, &previous] {
            assert!(file.metadata().unwrap().len() <= 500);
            let text = std::fs::read_to_string(file).unwrap();
            for line in text.lines() {
                let record: serde_json::Value = serde_json::from_str(line).unwrap();
                assert_eq!(record["handle"], " Anonymous ");
                assert_eq!(record["session_id"], "session");
                assert_eq!(record["body"], "<script>\\n世界");
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
            }
        }
        assert!(std::fs::read_to_string(path)
            .unwrap()
            .contains("\"sequence\":9"));
    }
    #[test]
    fn logging_failure_does_not_stop_comment_reception() {
        let logger = CommentLogger::start(None, 500);
        logger.shutdown();
        assert!(logger.failed());
        assert!(logger.submit(
            "s",
            1,
            1,
            &AudienceEvent::Comment {
                body: "hi".into(),
                handle: "".into()
            }
        ));
    }
}
