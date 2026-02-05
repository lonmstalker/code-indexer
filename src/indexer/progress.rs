use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Clone)]
pub struct IndexingProgress {
    inner: Arc<Inner>,
}

struct Inner {
    files_total: AtomicUsize,
    files_processed: AtomicUsize,
    symbols_extracted: AtomicUsize,
    errors: AtomicUsize,
    is_active: AtomicBool,
    started_at: Mutex<Option<Instant>>,
}

pub struct ProgressSnapshot {
    pub is_active: bool,
    pub files_total: usize,
    pub files_processed: usize,
    pub symbols_extracted: usize,
    pub errors: usize,
    pub elapsed_ms: u64,
    pub progress_pct: f64,
    pub eta_ms: Option<u64>,
}

impl IndexingProgress {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                files_total: AtomicUsize::new(0),
                files_processed: AtomicUsize::new(0),
                symbols_extracted: AtomicUsize::new(0),
                errors: AtomicUsize::new(0),
                is_active: AtomicBool::new(false),
                started_at: Mutex::new(None),
            }),
        }
    }

    pub fn start(&self, total_files: usize) {
        self.inner.files_total.store(total_files, Ordering::Release);
        self.inner.files_processed.store(0, Ordering::Release);
        self.inner.symbols_extracted.store(0, Ordering::Release);
        self.inner.errors.store(0, Ordering::Release);
        self.inner.is_active.store(true, Ordering::Release);
        *self.inner.started_at.lock().unwrap() = Some(Instant::now());
    }

    pub fn inc(&self, symbols_count: usize) {
        self.inner.files_processed.fetch_add(1, Ordering::Relaxed);
        self.inner
            .symbols_extracted
            .fetch_add(symbols_count, Ordering::Relaxed);
    }

    pub fn inc_error(&self) {
        self.inner.files_processed.fetch_add(1, Ordering::Relaxed);
        self.inner.errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn finish(&self) {
        self.inner.is_active.store(false, Ordering::Release);
    }

    pub fn snapshot(&self) -> ProgressSnapshot {
        let is_active = self.inner.is_active.load(Ordering::Acquire);
        let files_total = self.inner.files_total.load(Ordering::Acquire);
        let files_processed = self.inner.files_processed.load(Ordering::Acquire);
        let symbols_extracted = self.inner.symbols_extracted.load(Ordering::Acquire);
        let errors = self.inner.errors.load(Ordering::Acquire);

        let elapsed_ms = self
            .inner
            .started_at
            .lock()
            .unwrap()
            .map(|t| t.elapsed().as_millis() as u64)
            .unwrap_or(0);

        let progress_pct = if files_total > 0 {
            (files_processed as f64 / files_total as f64) * 100.0
        } else {
            0.0
        };

        let eta_ms = if is_active && files_processed > 0 && files_processed < files_total {
            let remaining = files_total - files_processed;
            let ms_per_file = elapsed_ms as f64 / files_processed as f64;
            Some((remaining as f64 * ms_per_file) as u64)
        } else {
            None
        };

        ProgressSnapshot {
            is_active,
            files_total,
            files_processed,
            symbols_extracted,
            errors,
            elapsed_ms,
            progress_pct,
            eta_ms,
        }
    }
}
