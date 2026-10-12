//! JSON-lines client for `trowel-worker`. `upstream/Sources/DigUpKit/Worker.swift` `EmbeddingWorker`.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};

use base64::Engine as _;
use serde_json::{json, Value};
use trowel_core::{Embedder, Embedding, ModelFiles, WorkerInfo, DIMENSION};

pub struct WorkerEmbedder {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    info: WorkerInfo,
    next_id: i64,
    closed: bool,
}

impl WorkerEmbedder {
    pub fn spawn(backend: &str, text_only: bool) -> Result<Self, String> {
        let bin = locate_worker(backend)?;
        let mut cmd = Command::new(&bin);
        cmd.args(["--backend", backend]);
        if text_only {
            cmd.arg("--text-only");
        }
        if let Ok(dir) = std::env::var("TROWEL_MODELS").or_else(|_| std::env::var("DIGUP_MODELS")) {
            cmd.env("TROWEL_MODELS", dir);
        }
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("spawn {}: {e}", bin.display()))?;
        let stdin = child.stdin.take().ok_or("worker stdin")?;
        let stdout = BufReader::new(child.stdout.take().ok_or("worker stdout")?);
        let mut worker = Self {
            child,
            stdin,
            stdout,
            info: WorkerInfo {
                model: String::new(),
                revision: String::new(),
                runtime: String::new(),
                dtype: String::new(),
                dim: DIMENSION,
                text_only,
                load_seconds: None,
            },
            next_id: 1,
            closed: false,
        };
        let ready = worker.read_line()?;
        if ready.get("ready") != Some(&Value::Bool(true)) {
            return Err(format!("worker not ready: {ready}"));
        }
        if let Some(info) = ready.get("info") {
            worker.info = WorkerInfo {
                model: info
                    .get("model")
                    .and_then(Value::as_str)
                    .unwrap_or("embeddinggemma-2")
                    .into(),
                revision: info
                    .get("revision")
                    .and_then(Value::as_str)
                    .unwrap_or(ModelFiles::REVISION)
                    .into(),
                runtime: info
                    .get("runtime")
                    .and_then(Value::as_str)
                    .unwrap_or(backend)
                    .into(),
                dtype: info
                    .get("dtype")
                    .and_then(Value::as_str)
                    .unwrap_or("q8_0")
                    .into(),
                dim: info
                    .get("dim")
                    .and_then(Value::as_u64)
                    .unwrap_or(DIMENSION as u64) as usize,
                text_only: info
                    .get("text_only")
                    .and_then(Value::as_bool)
                    .unwrap_or(text_only),
                load_seconds: info.get("load_seconds").and_then(Value::as_f64),
            };
        }
        Ok(worker)
    }

    fn read_line(&mut self) -> Result<Value, String> {
        let mut line = String::new();
        let n = self
            .stdout
            .read_line(&mut line)
            .map_err(|e| format!("worker read: {e}"))?;
        if n == 0 {
            return Err("worker closed stdout".into());
        }
        serde_json::from_str(line.trim_end()).map_err(|e| format!("worker json: {e}"))
    }

    fn request(&mut self, mut body: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        body["id"] = json!(id);
        writeln!(self.stdin, "{body}").map_err(|e| format!("worker write: {e}"))?;
        self.stdin
            .flush()
            .map_err(|e| format!("worker flush: {e}"))?;
        let reply = self.read_line()?;
        if reply.get("ok") != Some(&Value::Bool(true)) {
            return Err(reply
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("worker error")
                .to_string());
        }
        Ok(reply)
    }

    fn decode(&self, reply: &Value, count: usize) -> Result<Vec<Embedding>, String> {
        let dim = self.info.dim;
        let errors = reply
            .get("errors")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let b64 = reply
            .get("vectors")
            .and_then(Value::as_str)
            .ok_or("missing vectors")?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| format!("base64: {e}"))?;
        if bytes.len() != count * dim * 4 {
            return Err(format!(
                "vector bytes {} want {}",
                bytes.len(),
                count * dim * 4
            ));
        }
        let mut out = Vec::with_capacity(count);
        for row in 0..count {
            let err = errors.get(row).and_then(|v| {
                if v.is_null() {
                    None
                } else {
                    v.as_str().map(str::to_string)
                }
            });
            if err.is_some() {
                out.push(Embedding {
                    vector: None,
                    error: err,
                });
                continue;
            }
            let mut v = Vec::with_capacity(dim);
            for i in 0..dim {
                let off = (row * dim + i) * 4;
                v.push(f32::from_le_bytes(bytes[off..off + 4].try_into().unwrap()));
            }
            out.push(Embedding {
                vector: Some(v),
                error: None,
            });
        }
        Ok(out)
    }
}

impl Embedder for WorkerEmbedder {
    fn info(&self) -> WorkerInfo {
        self.info.clone()
    }

    fn embed_texts(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let reply = self.request(json!({"op": "text", "inputs": texts}))?;
        Ok(self
            .decode(&reply, texts.len())?
            .into_iter()
            .map(|e| e.vector.unwrap_or_else(|| vec![0.0; self.info.dim]))
            .collect())
    }

    fn embed_images(&mut self, paths: &[PathBuf], budget: i32) -> Result<Vec<Embedding>, String> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }
        let inputs: Vec<String> = paths
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        let reply = self.request(json!({"op": "image", "inputs": inputs, "budget": budget}))?;
        self.decode(&reply, paths.len())
    }

    fn embed_audio(&mut self, paths: &[PathBuf]) -> Result<Vec<Embedding>, String> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }
        let inputs: Vec<String> = paths
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        let reply = self.request(json!({"op": "audio", "inputs": inputs}))?;
        self.decode(&reply, paths.len())
    }

    fn close(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;
        let _ = writeln!(self.stdin, r#"{{"id":0,"op":"quit"}}"#);
        let _ = self.stdin.flush();
        let _ = self.child.wait();
    }
}

impl Drop for WorkerEmbedder {
    fn drop(&mut self) {
        self.close();
    }
}

pub fn locate_worker(backend: &str) -> Result<PathBuf, String> {
    if let Ok(p) = std::env::var("TROWEL_WORKER") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Ok(p);
        }
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let candidates = [
        root.join(format!("cpp/build-{backend}/trowel-worker")),
        root.join(format!("cpp/build-{backend}/bin/trowel-worker")),
        PathBuf::from(format!("trowel-worker-{backend}")),
        PathBuf::from("trowel-worker"),
    ];
    candidates.into_iter().find(|p| p.is_file()).ok_or_else(|| {
        format!("no trowel-worker for {backend}; build with scripts/build-llama.sh {backend}")
    })
}

pub fn worker_source(backend: &str) -> Option<String> {
    locate_worker(backend).ok().map(|_| {
        format!(
            "embeddinggemma-2@{} llama.cpp-{backend} q8_0",
            &ModelFiles::REVISION[..7]
        )
    })
}

pub fn default_backend() -> &'static str {
    if Path::new("cpp/build-vulkan/trowel-worker").is_file()
        || PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../cpp/build-vulkan/trowel-worker")
            .is_file()
    {
        "vulkan"
    } else {
        "cpu"
    }
}
