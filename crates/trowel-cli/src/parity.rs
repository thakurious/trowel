use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use base64::Engine as _;
use serde::Deserialize;
use serde_json::{json, Value};
use thiserror::Error;
use trowel_core::{ModelFiles, Prompts};

const AGREEMENT: f64 = 0.999;

#[derive(Debug, Error)]
pub enum ParityError {
    #[error("{0}")]
    Msg(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(Deserialize)]
struct Reference {
    vector_version: i32,
    vectors: std::collections::BTreeMap<String, Vec<f32>>,
}

struct Worker {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    dim: usize,
    next_id: i64,
}

impl Worker {
    fn spawn(bin: &Path, backend: &str) -> Result<Self, ParityError> {
        let mut child = Command::new(bin)
            .args(["--backend", backend])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| ParityError::Msg(format!("spawn {}: {e}", bin.display())))?;
        let stdin = child.stdin.take().expect("stdin");
        let stdout = BufReader::new(child.stdout.take().expect("stdout"));
        let mut worker = Self {
            child,
            stdin,
            stdout,
            dim: 768,
            next_id: 1,
        };
        let ready = worker.read_line()?;
        if ready.get("ready") != Some(&Value::Bool(true)) {
            return Err(ParityError::Msg(format!("worker not ready: {}", ready)));
        }
        if let Some(info) = ready.get("info") {
            if let Some(dim) = info.get("dim").and_then(Value::as_u64) {
                worker.dim = dim as usize;
            }
            eprintln!(
                "worker ready: runtime={} dim={}",
                info.get("runtime").and_then(Value::as_str).unwrap_or("?"),
                worker.dim
            );
        }
        Ok(worker)
    }

    fn read_line(&mut self) -> Result<Value, ParityError> {
        let mut line = String::new();
        let n = self.stdout.read_line(&mut line)?;
        if n == 0 {
            return Err(ParityError::Msg("worker closed stdout".into()));
        }
        Ok(serde_json::from_str(line.trim_end())?)
    }

    fn request(&mut self, mut body: Value) -> Result<Value, ParityError> {
        let id = self.next_id;
        self.next_id += 1;
        body["id"] = json!(id);
        writeln!(self.stdin, "{body}")?;
        self.stdin.flush()?;
        let reply = self.read_line()?;
        if reply.get("ok") != Some(&Value::Bool(true)) {
            return Err(ParityError::Msg(format!("worker error: {reply}")));
        }
        Ok(reply)
    }

    fn embed_texts(&mut self, inputs: &[String]) -> Result<Vec<Vec<f32>>, ParityError> {
        let reply = self.request(json!({"op": "text", "inputs": inputs}))?;
        decode_vectors(&reply, inputs.len(), self.dim)
    }

    fn embed_images(
        &mut self,
        paths: &[String],
        budget: i32,
    ) -> Result<Vec<Vec<f32>>, ParityError> {
        let reply = self.request(json!({"op": "image", "inputs": paths, "budget": budget}))?;
        decode_vectors(&reply, paths.len(), self.dim)
    }

    fn embed_audio(&mut self, paths: &[String]) -> Result<Vec<Vec<f32>>, ParityError> {
        let reply = self.request(json!({"op": "audio", "inputs": paths}))?;
        decode_vectors(&reply, paths.len(), self.dim)
    }

    fn quit(mut self) {
        let _ = writeln!(self.stdin, r#"{{"id":0,"op":"quit"}}"#);
        let _ = self.stdin.flush();
        let _ = self.child.wait();
    }
}

fn decode_vectors(reply: &Value, count: usize, dim: usize) -> Result<Vec<Vec<f32>>, ParityError> {
    if let Some(errors) = reply.get("errors").and_then(Value::as_array) {
        for (i, err) in errors.iter().enumerate() {
            if !err.is_null() {
                return Err(ParityError::Msg(format!("input {i}: {err}")));
            }
        }
    }
    let b64 = reply
        .get("vectors")
        .and_then(Value::as_str)
        .ok_or_else(|| ParityError::Msg("missing vectors".into()))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| ParityError::Msg(format!("base64: {e}")))?;
    if bytes.len() != count * dim * 4 {
        return Err(ParityError::Msg(format!(
            "vector bytes {} want {}",
            bytes.len(),
            count * dim * 4
        )));
    }
    let mut out = Vec::with_capacity(count);
    for row in 0..count {
        let mut v = Vec::with_capacity(dim);
        for i in 0..dim {
            let off = (row * dim + i) * 4;
            v.push(f32::from_le_bytes(bytes[off..off + 4].try_into().unwrap()));
        }
        out.push(v);
    }
    Ok(out)
}

fn cosine(a: &[f32], b: &[f32]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| f64::from(*x) * f64::from(*y))
        .sum()
}

fn long_document() -> String {
    (1..=60)
        .map(|n| {
            format!(
                "Note {n}: the review moved to room {} because the projector in the main hall failed again, and the team agreed to send the slides the night before.",
                n % 7 + 2
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn reference_texts() -> Vec<(String, String)> {
    vec![
        (
            "query".into(),
            Prompts::query("payment declined error"),
        ),
        ("query-bn".into(), Prompts::query("ইলিশ মাছ রান্না")),
        ("query-ar".into(), Prompts::query("متى تغلق المكتبة")),
        (
            "document".into(),
            Prompts::document(
                Some("Tokyo itinerary"),
                "Flight to Tokyo Haneda on November 3. Then the train to Kamakura to see the Great Buddha, and back for dinner in Shibuya.",
            ),
        ),
        (
            "document-long".into(),
            Prompts::document(None, &long_document()),
        ),
    ]
}

fn default_worker(backend: &str) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("TROWEL_WORKER") {
        return Some(PathBuf::from(p));
    }
    let root = workspace_root();
    let candidates = [
        root.join(format!("cpp/build-{backend}/trowel-worker")),
        root.join(format!("cpp/build-{backend}/bin/trowel-worker")),
        PathBuf::from(format!("trowel-worker-{backend}")),
        PathBuf::from("trowel-worker"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn default_reference() -> PathBuf {
    workspace_root().join("reference")
}

pub fn run(
    backend: &str,
    worker: Option<PathBuf>,
    reference_dir: Option<PathBuf>,
) -> Result<(), ParityError> {
    if !ModelFiles::complete() {
        return Err(ParityError::Msg(
            "model files missing; run scripts/download-models.sh".into(),
        ));
    }
    let worker_bin = worker.or_else(|| default_worker(backend)).ok_or_else(|| {
        ParityError::Msg(format!(
            "no trowel-worker for {backend}; build with scripts/build-llama.sh {backend}"
        ))
    })?;
    let reference_dir = reference_dir.unwrap_or_else(default_reference);
    let stored: Reference =
        serde_json::from_slice(&std::fs::read(reference_dir.join("vectors.json"))?)?;
    if stored.vector_version != 1 {
        return Err(ParityError::Msg(format!(
            "reference is for vectors {}",
            stored.vector_version
        )));
    }

    let mut worker = Worker::spawn(&worker_bin, backend)?;
    let texts = reference_texts();
    let text_inputs: Vec<String> = texts.iter().map(|(_, t)| t.clone()).collect();
    let text_vecs = worker.embed_texts(&text_inputs)?;
    let mut made = std::collections::BTreeMap::new();
    for ((name, _), vec) in texts.iter().zip(text_vecs) {
        made.insert(name.clone(), vec);
    }

    let images = [
        ("screen-280", "screen.png", 280),
        ("photo-140", "photo.jpg", 140),
        ("frame-70", "photo.jpg", 70),
    ];
    for (name, file, budget) in images {
        let path = reference_dir.join(file);
        let vecs = worker.embed_images(&[path.display().to_string()], budget)?;
        made.insert(name.to_string(), vecs.into_iter().next().unwrap());
    }
    let speech = reference_dir.join("speech.wav");
    let vecs = worker.embed_audio(&[speech.display().to_string()])?;
    made.insert("speech".into(), vecs.into_iter().next().unwrap());
    worker.quit();

    let mut worst_name = String::new();
    let mut worst = 1.0_f64;
    let mut failed = false;
    println!("backend={backend} agreement>={AGREEMENT}");
    for (name, vector) in &made {
        let Some(expected) = stored.vectors.get(name) else {
            println!("{name}: missing from reference");
            failed = true;
            continue;
        };
        if vector.len() != expected.len() {
            println!("{name}: dim {} vs {}", vector.len(), expected.len());
            failed = true;
            continue;
        }
        let c = cosine(vector, expected);
        let ok = if c >= AGREEMENT { "ok" } else { "FAIL" };
        println!("{name}: cosine={c:.6} {ok}");
        if c < worst {
            worst = c;
            worst_name.clone_from(name);
        }
        if c < AGREEMENT {
            failed = true;
        }
    }
    if made.len() != stored.vectors.len() {
        println!(
            "key mismatch: made {} reference {}",
            made.len(),
            stored.vectors.len()
        );
        failed = true;
    }
    println!("worst={worst_name} cosine={worst:.6}");
    if failed {
        return Err(ParityError::Msg(
            "vector parity failed; do not bump vectorVersion silently".into(),
        ));
    }
    Ok(())
}
