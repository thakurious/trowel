//! Query encoder: text-only `trowel-worker`. `upstream/Sources/DigUpApp/Engine.swift`.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};

use base64::Engine as _;
use serde_json::{json, Value};
use trowel_core::{Embedder, Embedding, ModelFiles, WorkerInfo, DIMENSION};

pub struct QueryEncoder {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    info: WorkerInfo,
    next_id: i64,
    closed: bool,
}

impl QueryEncoder {
    pub fn spawn() -> Result<Self, String> {
        let backend = default_backend();
        let bin = locate_worker(backend)?;
        let mut cmd = Command::new(&bin);
        cmd.args(["--backend", backend, "--text-only"]);
        if let Ok(dir) = std::env::var("TROWEL_MODELS").or_else(|_| std::env::var("DIGUP_MODELS")) {
            cmd.env("TROWEL_MODELS", dir);
        }
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
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
                runtime: backend.into(),
                dtype: "q8_0".into(),
                dim: DIMENSION,
                text_only: true,
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
            worker.info.model = info
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or("embeddinggemma-2")
                .into();
            worker.info.revision = info
                .get("revision")
                .and_then(Value::as_str)
                .unwrap_or(ModelFiles::REVISION)
                .into();
            worker.info.runtime = info
                .get("runtime")
                .and_then(Value::as_str)
                .unwrap_or(backend)
                .into();
            worker.info.dtype = info
                .get("dtype")
                .and_then(Value::as_str)
                .unwrap_or("q8_0")
                .into();
            worker.info.load_seconds = info.get("load_seconds").and_then(Value::as_f64);
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
}

impl Embedder for QueryEncoder {
    fn info(&self) -> WorkerInfo {
        self.info.clone()
    }

    fn embed_texts(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let reply = self.request(json!({"op": "text", "inputs": texts}))?;
        let dim = self.info.dim;
        let b64 = reply
            .get("vectors")
            .and_then(Value::as_str)
            .ok_or("missing vectors")?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| format!("base64: {e}"))?;
        let count = texts.len();
        if bytes.len() != count * dim * 4 {
            return Err(format!(
                "vector bytes {} want {}",
                bytes.len(),
                count * dim * 4
            ));
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

    fn embed_images(&mut self, _paths: &[PathBuf], _budget: i32) -> Result<Vec<Embedding>, String> {
        Err("query encoder is text-only".into())
    }

    fn embed_audio(&mut self, _paths: &[PathBuf]) -> Result<Vec<Embedding>, String> {
        Err("query encoder is text-only".into())
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

impl Drop for QueryEncoder {
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
    if let Ok(exe) = std::env::current_exe() {
        let sibling = exe.with_file_name("trowel-worker");
        if sibling.is_file() {
            return Ok(sibling);
        }
        let mut dir = exe.parent();
        for _ in 0..8 {
            let Some(d) = dir else { break };
            let p = d.join(format!("cpp/build-{backend}/trowel-worker"));
            if p.is_file() {
                return Ok(p);
            }
            dir = d.parent();
        }
    }
    let named = PathBuf::from(format!("trowel-worker-{backend}"));
    if named.is_file() {
        return Ok(named);
    }
    let path = PathBuf::from("trowel-worker");
    if path.is_file() {
        return Ok(path);
    }
    Err(format!(
        "no trowel-worker for {backend}; build with scripts/build-llama.sh {backend}"
    ))
}

pub fn default_backend() -> &'static str {
    if locate_worker("vulkan").is_ok() {
        "vulkan"
    } else {
        "cpu"
    }
}

pub fn worker_present() -> bool {
    locate_worker(default_backend()).is_ok()
}
