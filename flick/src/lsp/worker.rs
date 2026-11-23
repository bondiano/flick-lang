//! The analysis worker: document changes are analyzed off the message loop,
//! so a long analysis never holds up a request the editor sends meanwhile.
//!
//! A change is debounced for [`DEBOUNCE`] and only the last text of each
//! document in that window is analyzed. Each job carries a generation; the
//! server publishes a result only when it is still the latest one asked for
//! its document, so a slow analysis of an old revision is cancelled in
//! effect and never overwrites newer diagnostics.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use super::{Caches, Context, Snapshot, analyze};

/// How long changes to the same document are collected before analyzing.
const DEBOUNCE: Duration = Duration::from_millis(15);

/// One revision of one document to analyze, over the open buffers.
pub(super) struct Job {
    pub path: PathBuf,
    pub docs: HashMap<PathBuf, String>,
    pub generation: u64,
}

/// An analysis of one revision.
pub(super) struct Done {
    pub path: PathBuf,
    pub generation: u64,
    pub snapshot: Snapshot,
}

enum Task {
    Analyze(Job),
    /// Forget the cached modules of a context, or of every context.
    Clear(Option<Context>),
    /// Forget the cached modules of every context outside this set.
    Retain(HashSet<Context>),
    /// Report how many contexts are cached, for the tests to observe reuse.
    #[allow(dead_code)]
    Sync(Sender<()>),
    Stop,
}

/// The thread that analyzes changes; drop it to let the thread end.
pub(super) struct Worker {
    tasks: Sender<Task>,
    done: Receiver<Done>,
    #[allow(dead_code)]
    len: Arc<AtomicUsize>,
}

impl Worker {
    pub fn new() -> Self {
        let (tasks, received) = mpsc::channel();
        let (finished, done) = mpsc::channel();
        let len = Arc::new(AtomicUsize::new(0));
        thread::Builder::new()
            .name("flick-lsp-analysis".to_owned())
            .spawn({
                let len = len.clone();
                move || run(received, finished, len)
            })
            .expect("the analysis thread starts");
        Worker { tasks, done, len }
    }

    /// Queues a revision; the worker analyzes the last one queued per path.
    pub fn submit(&self, job: Job) {
        let _ = self.tasks.send(Task::Analyze(job));
    }

    /// Forgets cached modules: one context's, or all of them.
    pub fn clear(&self, context: Option<Context>) {
        let _ = self.tasks.send(Task::Clear(context));
    }

    /// Forgets the cached modules of every context outside `kept`.
    pub fn retain(&self, kept: HashSet<Context>) {
        let _ = self.tasks.send(Task::Retain(kept));
    }

    /// A finished analysis, if one is waiting.
    pub fn try_recv(&self) -> Option<Done> {
        self.done.try_recv().ok()
    }

    /// A finished analysis, waiting at most `timeout` for one.
    pub fn recv_timeout(&self, timeout: Duration) -> Option<Done> {
        self.done.recv_timeout(timeout).ok()
    }

    /// How many contexts the worker still caches.
    #[allow(dead_code)]
    pub fn cache_len(&self) -> usize {
        self.len.load(Ordering::Relaxed)
    }

    /// Waits until the worker has applied every queued clear, for the tests.
    #[cfg(test)]
    pub fn synchronize(&self) {
        let (ack, done) = mpsc::channel();
        let _ = self.tasks.send(Task::Sync(ack));
        let _ = done.recv_timeout(Duration::from_secs(30));
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.tasks.send(Task::Stop);
    }
}

#[allow(clippy::needless_pass_by_value)]
fn run(tasks: Receiver<Task>, done: Sender<Done>, len: Arc<AtomicUsize>) {
    let mut caches = Caches::new();
    // submission order, so the newest buffers are the last published
    let mut queued: Vec<Job> = Vec::new();
    loop {
        match tasks.recv() {
            Ok(Task::Analyze(job)) => queue(&mut queued, job),
            Ok(Task::Clear(context)) => {
                clear(&mut caches, context);
                len.store(caches.len(), Ordering::Relaxed);
                continue;
            }
            Ok(Task::Retain(kept)) => {
                caches.retain(|context, _| kept.contains(context));
                len.store(caches.len(), Ordering::Relaxed);
                continue;
            }
            Ok(Task::Sync(ack)) => {
                len.store(caches.len(), Ordering::Relaxed);
                let _ = ack.send(());
                continue;
            }
            Ok(Task::Stop) | Err(_) => return,
        }
        // collect the changes that follow closely: a burst of keystrokes is
        // one analysis of the text they end at
        let deadline = Instant::now() + DEBOUNCE;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            match tasks.recv_timeout(left) {
                Ok(Task::Analyze(job)) => queue(&mut queued, job),
                Ok(Task::Clear(context)) => {
                    clear(&mut caches, context);
                    len.store(caches.len(), Ordering::Relaxed);
                }
                Ok(Task::Retain(kept)) => {
                    caches.retain(|context, _| kept.contains(context));
                    len.store(caches.len(), Ordering::Relaxed);
                }
                Ok(Task::Sync(ack)) => {
                    len.store(caches.len(), Ordering::Relaxed);
                    let _ = ack.send(());
                }
                Ok(Task::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => return,
                Err(mpsc::RecvTimeoutError::Timeout) => break,
            }
        }
        for job in queued.drain(..) {
            let mut snapshot = analyze(&job.docs, &job.path, &mut caches);
            snapshot.drop_compilation();
            len.store(caches.len(), Ordering::Relaxed);
            let finished = Done {
                path: job.path,
                generation: job.generation,
                snapshot,
            };
            if done.send(finished).is_err() {
                return;
            }
        }
    }
}

/// Queues `job` last: a newer revision of a document supersedes the queued one.
fn queue(queued: &mut Vec<Job>, job: Job) {
    if let Some(index) = queued.iter().position(|queued| queued.path == job.path) {
        queued.remove(index);
    }
    queued.push(job);
}

fn clear(caches: &mut Caches, context: Option<Context>) {
    match context {
        Some(context) => {
            caches.remove(&context);
        }
        None => caches.clear(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(path: &str, generation: u64) -> Job {
        Job {
            path: PathBuf::from(path),
            docs: HashMap::new(),
            generation,
        }
    }

    #[test]
    fn a_newer_revision_of_a_document_supersedes_the_queued_one() {
        let mut queued = Vec::new();
        queue(&mut queued, job("a", 1));
        queue(&mut queued, job("b", 1));
        queue(&mut queued, job("a", 2));

        let order: Vec<_> = queued
            .iter()
            .map(|job| (job.path.to_string_lossy().into_owned(), job.generation))
            .collect();
        assert_eq!(
            order,
            [("b".to_owned(), 1), ("a".to_owned(), 2)],
            "a burst of changes analyzes the last text of each document once"
        );
    }
}
