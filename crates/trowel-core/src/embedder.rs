//! Embedder protocol. `upstream/Sources/DigUpKit/Worker.swift`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::vectors::DIMENSION;

#[derive(Debug, Clone)]
pub struct WorkerInfo {
    pub model: String,
    pub revision: String,
    pub runtime: String,
    pub dtype: String,
    pub dim: usize,
    pub text_only: bool,
    pub load_seconds: Option<f64>,
}

impl WorkerInfo {
    /// `upstream/Sources/DigUpKit/Worker.swift:25`
    pub fn vector_source(&self) -> String {
        let rev: String = self.revision.chars().take(7).collect();
        format!("{}@{rev} {} {}", self.model, self.runtime, self.dtype)
    }
}

#[derive(Debug, Clone)]
pub struct Embedding {
    pub vector: Option<Vec<f32>>,
    pub error: Option<String>,
}

pub trait Embedder: Send {
    fn info(&self) -> WorkerInfo;
    fn embed_texts(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>, String>;
    fn embed_images(&mut self, paths: &[PathBuf], budget: i32) -> Result<Vec<Embedding>, String>;
    fn embed_audio(&mut self, paths: &[PathBuf]) -> Result<Vec<Embedding>, String>;
    fn close(&mut self);
}

/// Deterministic unit vectors for tests. `upstream/Tests/DigUpKitTests/LongFilesTests.swift` `FakeEmbedder`.
#[derive(Clone)]
pub struct FakeEmbedder {
    texts: Arc<AtomicUsize>,
}

impl FakeEmbedder {
    pub fn new() -> Self {
        Self {
            texts: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn texts(&self) -> usize {
        self.texts.load(Ordering::Relaxed)
    }

    pub fn vector_for(text: &str) -> Vec<f32> {
        let mut state: u64 = 5381;
        for byte in text.as_bytes() {
            state = state.wrapping_mul(33).wrapping_add(u64::from(*byte));
        }
        let mut values = Vec::with_capacity(DIMENSION);
        for _ in 0..DIMENSION {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            values.push(((state % 2001) as i64 - 1000) as f32 / 1000.0);
        }
        let norm = values.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut values {
                *v /= norm;
            }
        }
        values
    }
}

impl Default for FakeEmbedder {
    fn default() -> Self {
        Self::new()
    }
}

impl Embedder for FakeEmbedder {
    fn info(&self) -> WorkerInfo {
        WorkerInfo {
            model: "fake".into(),
            revision: "0000000".into(),
            runtime: "test".into(),
            dtype: "f32".into(),
            dim: DIMENSION,
            text_only: false,
            load_seconds: None,
        }
    }

    fn embed_texts(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        self.texts.fetch_add(texts.len(), Ordering::Relaxed);
        Ok(texts.iter().map(|t| Self::vector_for(t)).collect())
    }

    fn embed_images(&mut self, paths: &[PathBuf], _budget: i32) -> Result<Vec<Embedding>, String> {
        Ok(paths
            .iter()
            .map(|p| Embedding {
                vector: Some(Self::vector_for(&p.to_string_lossy())),
                error: None,
            })
            .collect())
    }

    fn embed_audio(&mut self, paths: &[PathBuf]) -> Result<Vec<Embedding>, String> {
        self.embed_images(paths, 0)
    }

    fn close(&mut self) {}
}

/// D28 index speed: an embedder that leaves the shared iGPU idle between requests. Images and
/// sound windows go to the worker [`Paced::CHUNK`] at a time (the worker encodes each on its own,
/// `cpp/embedder.cpp` `embed_media`, so the vectors are identical), and after each request it
/// rests `pace` × the request's time: pace 2 keeps the GPU busy at most ~1/3 of the time, in
/// bursts of a few hundred ms. Texts are not split (they may share one encoder pass); they rest
/// after the call. Search's query encoder is never paced.
pub struct Paced {
    inner: Box<dyn Embedder>,
    pace: f64,
    sleep: Box<dyn FnMut(std::time::Duration) + Send>,
}

impl Paced {
    pub const CHUNK: usize = 2;
    /// One rest never runs longer than this, whatever a request took.
    pub const MAX_REST: std::time::Duration = std::time::Duration::from_secs(4);

    pub fn new(inner: Box<dyn Embedder>, pace: f64) -> Self {
        Self::with_sleep(inner, pace, Box::new(std::thread::sleep))
    }

    pub fn with_sleep(
        inner: Box<dyn Embedder>,
        pace: f64,
        sleep: Box<dyn FnMut(std::time::Duration) + Send>,
    ) -> Self {
        Self { inner, pace, sleep }
    }

    fn rest(&mut self, busy: std::time::Duration) {
        let rest = busy.mul_f64(self.pace).min(Self::MAX_REST);
        if !rest.is_zero() {
            (self.sleep)(rest);
        }
    }

    fn chunked(
        &mut self,
        paths: &[PathBuf],
        mut call: impl FnMut(&mut dyn Embedder, &[PathBuf]) -> Result<Vec<Embedding>, String>,
    ) -> Result<Vec<Embedding>, String> {
        let mut out = Vec::with_capacity(paths.len());
        for chunk in paths.chunks(Self::CHUNK) {
            let started = std::time::Instant::now();
            out.extend(call(self.inner.as_mut(), chunk)?);
            self.rest(started.elapsed());
        }
        Ok(out)
    }
}

impl Embedder for Paced {
    fn info(&self) -> WorkerInfo {
        self.inner.info()
    }

    fn embed_texts(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        let started = std::time::Instant::now();
        let out = self.inner.embed_texts(texts);
        self.rest(started.elapsed());
        out
    }

    fn embed_images(&mut self, paths: &[PathBuf], budget: i32) -> Result<Vec<Embedding>, String> {
        self.chunked(paths, |e, chunk| e.embed_images(chunk, budget))
    }

    fn embed_audio(&mut self, paths: &[PathBuf]) -> Result<Vec<Embedding>, String> {
        self.chunked(paths, |e, chunk| e.embed_audio(chunk))
    }

    fn close(&mut self) {
        self.inner.close()
    }
}

/// One model for both indexes. A handle's `close` leaves it running.
/// `upstream/Sources/digup/IndexService.swift` `SharedEmbedder`.
#[derive(Clone)]
pub struct SharedEmbedder {
    make: Arc<dyn Fn() -> Result<Box<dyn Embedder>, String> + Send + Sync>,
    inner: Arc<Mutex<Option<Box<dyn Embedder>>>>,
}

impl SharedEmbedder {
    pub fn new(
        make: impl Fn() -> Result<Box<dyn Embedder>, String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            make: Arc::new(make),
            inner: Arc::new(Mutex::new(None)),
        }
    }

    pub fn is_running(&self) -> bool {
        self.inner.lock().map(|g| g.is_some()).unwrap_or(false)
    }

    fn ensure(&self) -> Result<(), String> {
        let mut guard = self.inner.lock().map_err(|e| e.to_string())?;
        if guard.is_none() {
            *guard = Some((self.make)()?);
        }
        Ok(())
    }

    pub fn shutdown(&self) {
        if let Ok(mut g) = self.inner.lock() {
            if let Some(mut m) = g.take() {
                m.close();
            }
        }
    }
}

impl Embedder for SharedEmbedder {
    fn info(&self) -> WorkerInfo {
        self.ensure().ok();
        self.inner
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|m| m.info()))
            .unwrap_or(WorkerInfo {
                model: String::new(),
                revision: String::new(),
                runtime: String::new(),
                dtype: String::new(),
                dim: DIMENSION,
                text_only: false,
                load_seconds: None,
            })
    }

    fn embed_texts(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        self.ensure()?;
        self.inner
            .lock()
            .map_err(|e| e.to_string())?
            .as_mut()
            .ok_or_else(|| "embedder closed".to_string())?
            .embed_texts(texts)
    }

    fn embed_images(&mut self, paths: &[PathBuf], budget: i32) -> Result<Vec<Embedding>, String> {
        self.ensure()?;
        self.inner
            .lock()
            .map_err(|e| e.to_string())?
            .as_mut()
            .ok_or_else(|| "embedder closed".to_string())?
            .embed_images(paths, budget)
    }

    fn embed_audio(&mut self, paths: &[PathBuf]) -> Result<Vec<Embedding>, String> {
        self.ensure()?;
        self.inner
            .lock()
            .map_err(|e| e.to_string())?
            .as_mut()
            .ok_or_else(|| "embedder closed".to_string())?
            .embed_audio(paths)
    }

    fn close(&mut self) {}
}

#[cfg(test)]
mod paced_tests {
    use super::*;
    use std::time::Duration;

    /// FakeEmbedder that takes 50 ms per image and records each call's size.
    struct Slow {
        fake: FakeEmbedder,
        calls: Arc<Mutex<Vec<usize>>>,
    }

    impl Embedder for Slow {
        fn info(&self) -> WorkerInfo {
            self.fake.info()
        }
        fn embed_texts(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
            self.calls.lock().unwrap().push(texts.len());
            self.fake.embed_texts(texts)
        }
        fn embed_images(
            &mut self,
            paths: &[PathBuf],
            budget: i32,
        ) -> Result<Vec<Embedding>, String> {
            self.calls.lock().unwrap().push(paths.len());
            std::thread::sleep(Duration::from_millis(50 * paths.len() as u64));
            self.fake.embed_images(paths, budget)
        }
        fn embed_audio(&mut self, paths: &[PathBuf]) -> Result<Vec<Embedding>, String> {
            self.embed_images(paths, 0)
        }
        fn close(&mut self) {}
    }

    #[test]
    fn splits_media_rests_by_pace_and_keeps_vectors() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let rests = Arc::new(Mutex::new(Vec::new()));
        let r = Arc::clone(&rests);
        let mut paced = Paced::with_sleep(
            Box::new(Slow {
                fake: FakeEmbedder::new(),
                calls: Arc::clone(&calls),
            }),
            2.0,
            Box::new(move |d| r.lock().unwrap().push(d)),
        );
        let frames: Vec<PathBuf> = (0..5)
            .map(|i| PathBuf::from(format!("/f/{i}.jpg")))
            .collect();
        let got = paced.embed_images(&frames, 70).unwrap();
        // Same vectors, same order, as one unpaced call.
        let want = FakeEmbedder::new().embed_images(&frames, 70).unwrap();
        assert_eq!(
            got.iter().map(|e| e.vector.clone()).collect::<Vec<_>>(),
            want.iter().map(|e| e.vector.clone()).collect::<Vec<_>>()
        );
        assert_eq!(*calls.lock().unwrap(), vec![2, 2, 1]);
        // Each rest is ~2× its chunk: 100 ms of work → ~200 ms of rest.
        let rests = rests.lock().unwrap().clone();
        assert_eq!(rests.len(), 3);
        assert!(
            rests[0] >= Duration::from_millis(190) && rests[0] < Duration::from_millis(400),
            "{rests:?}"
        );
        assert!(
            rests[2] >= Duration::from_millis(95) && rests[2] < Duration::from_millis(300),
            "{rests:?}"
        );
        // Texts go through whole.
        calls.lock().unwrap().clear();
        let texts: Vec<String> = (0..7).map(|i| format!("t{i}")).collect();
        paced.embed_texts(&texts).unwrap();
        assert_eq!(*calls.lock().unwrap(), vec![7]);
    }

    #[test]
    fn a_rest_is_capped() {
        let rests = Arc::new(Mutex::new(Vec::new()));
        let r = Arc::clone(&rests);
        let mut paced = Paced::with_sleep(
            Box::new(FakeEmbedder::new()),
            4.0,
            Box::new(move |d| r.lock().unwrap().push(d)),
        );
        paced.rest(Duration::from_secs(30));
        assert_eq!(*rests.lock().unwrap(), vec![Paced::MAX_REST]);
    }
}
