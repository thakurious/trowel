//! Query vector cache (256). `upstream/Sources/DigUpApp/Engine.swift` `QueryVectorCache`.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

pub const LIMIT: usize = 256;

#[derive(Default)]
pub struct QueryCache {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    vectors: HashMap<String, Vec<f32>>,
    order: VecDeque<String>,
}

impl QueryCache {
    pub fn get(&self, key: &str) -> Option<Vec<f32>> {
        self.inner.lock().ok()?.vectors.get(key).cloned()
    }

    pub fn insert(&self, key: String, vector: Vec<f32>) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        if inner.vectors.insert(key.clone(), vector).is_none() {
            inner.order.push_back(key);
        }
        while inner.order.len() > LIMIT {
            if let Some(old) = inner.order.pop_front() {
                inner.vectors.remove(&old);
            }
        }
    }

    pub fn clear(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.vectors.clear();
            inner.order.clear();
        }
    }

    pub fn key(text: &str, code: bool) -> String {
        if code {
            format!("code\u{0}{text}")
        } else {
            text.to_string()
        }
    }
}
