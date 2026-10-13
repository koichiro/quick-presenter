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

    fn comment() -> AudienceEvent {
        AudienceEvent::Comment {
            body: "Hello\n世界".into(),
            handle: " Original\tName ".into(),
        }
    }

    #[test]
    fn bounded_writer_backpressure_rejects_comment_without_consuming_sequence() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let logger = Arc::new(CommentLogger {
            sender: Mutex::new(Some(sender)),
            worker: Mutex::new(None),
            failed: Arc::new(AtomicBool::new(false)),
        });
        let now = std::time::Instant::now();
        let mut engine = crate::audience_events::AudienceEngine::new(now);
        engine.logger = Some(logger.clone());
        use crate::audience_events::{Admission, ReactionKind};
        assert_eq!(engine.accept_event(comment(), 1, now), Admission::Accepted);
        assert_eq!(engine.accept_event(comment(), 2, now), Admission::Busy);
        assert_eq!(
            engine.accept(ReactionKind::Heart, 3, now),
            Admission::Accepted
        );
        let first: serde_json::Value = serde_json::from_str(&receiver.try_recv().unwrap()).unwrap();
        assert_eq!(first["sequence"], 1);
        assert_eq!(first["participant_id"], 1);
        assert_eq!(engine.accept_event(comment(), 4, now), Admission::Accepted);
        let events = engine.drain(now);
        assert_eq!(
            events
                .iter()
                .map(|event| event.sequence)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(
            events
                .iter()
                .map(|event| event.participant_id)
                .collect::<Vec<_>>(),
            vec![1, 3, 4]
        );
        let last: serde_json::Value = serde_json::from_str(&receiver.try_recv().unwrap()).unwrap();
        assert_eq!(last["sequence"], 3);
        assert!(!logger.failed());
        logger.shutdown();
    }

    #[test]
    fn disconnected_writer_marks_failure_and_allows_later_comments() {
        let (sender, receiver) = mpsc::sync_channel(1);
        drop(receiver);
        let logger = CommentLogger {
            sender: Mutex::new(Some(sender)),
            worker: Mutex::new(None),
            failed: Arc::new(AtomicBool::new(false)),
        };
        assert!(logger.submit("session", 1, 1, &comment()));
        assert!(logger.failed());
        assert!(logger.submit("session", 2, 1, &comment()));
        logger.shutdown();
    }

    #[test]
    fn shutdown_drains_ordered_records_and_does_not_log_reactions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/comments.jsonl");
        let logger = CommentLogger::start(Some(path.clone()), 100_000);
        let reaction = AudienceEvent::Reaction {
            kind: crate::audience_events::ReactionKind::Heart,
        };
        assert!(logger.submit("session", 0, 99, &reaction));
        for sequence in 1..=3 {
            assert!(logger.submit("session", sequence, 7, &comment()));
        }
        logger.shutdown();
        logger.shutdown();
        assert!(!logger.failed());
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.ends_with('\n'));
        let records: Vec<serde_json::Value> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(records.len(), 3);
        for (index, record) in records.iter().enumerate() {
            assert_eq!(record["sequence"], index as u64 + 1);
            assert_eq!(record["participant_id"], 7);
            assert_eq!(record["session_id"], "session");
            assert_eq!(record["body"], "Hello\n世界");
            assert_eq!(record["handle"], " Original\tName ");
            assert!(
                chrono::DateTime::parse_from_rfc3339(record["received_at"].as_str().unwrap())
                    .is_ok()
            );
        }
        assert!(logger.submit("session", 4, 7, &comment()));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    }

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
