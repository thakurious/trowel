//! fp16 storage and blocked conversion. `upstream/Sources/DigUpKit/Vectors.swift`

use half::f16;
use memmap2::{MmapMut, MmapOptions};

pub const DIMENSION: usize = 768;

/// `upstream/Sources/DigUpKit/Vectors.swift:5`
pub struct Vectors;

impl Vectors {
    pub fn half(vector: &[f32]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(vector.len() * 2);
        for &x in vector {
            bytes.extend_from_slice(&f16::from_f32(x).to_le_bytes());
        }
        bytes
    }

    pub fn floats_from_half(source: &[u8], destination: &mut [f32]) {
        let n = (source.len() / 2).min(destination.len());
        for i in 0..n {
            destination[i] = f16::from_le_bytes([source[i * 2], source[i * 2 + 1]]).to_f32();
        }
    }
}

/// mmap'd `rows×768` fp16 plus one 1024×768 fp32 block.
/// `upstream/Sources/DigUpKit/Vectors.swift:28`
pub struct VectorStorage {
    pub rows: usize,
    map: MmapMut,
    half_bytes: usize,
}

impl VectorStorage {
    pub const BLOCK_ROWS: usize = 1024;

    pub fn new(rows: usize) -> Option<Self> {
        if rows == 0 {
            return None;
        }
        let half_bytes = rows * DIMENSION * 2;
        let block_bytes = Self::BLOCK_ROWS * DIMENSION * 4;
        let bytes = half_bytes + block_bytes;
        let map = MmapOptions::new().len(bytes).map_anon().ok()?;
        Some(Self {
            rows,
            map,
            half_bytes,
        })
    }

    pub fn half_bytes(&self) -> usize {
        self.half_bytes
    }

    pub fn halves(&self) -> &[u8] {
        &self.map[..self.half_bytes]
    }

    pub fn write_row(&mut self, row: usize, bytes: &[u8]) {
        let start = row * DIMENSION * 2;
        let n = bytes.len().min(DIMENSION * 2);
        self.map[start..start + n].copy_from_slice(&bytes[..n]);
    }

    /// Cosine of `query` with every stored row: blocked fp16→fp32, then `(count×768)·(768×1)`.
    /// `upstream/Sources/DigUpKit/Searcher.swift:285`
    pub fn scores(&self, query: &[f32]) -> Vec<f32> {
        let rows = self.rows;
        let mut scores = vec![0f32; rows];
        if rows == 0 || query.len() != DIMENSION {
            return scores;
        }
        let mut block = vec![0f32; Self::BLOCK_ROWS * DIMENSION];
        let mut row = 0;
        while row < rows {
            let count = (rows - row).min(Self::BLOCK_ROWS);
            let src = row * DIMENSION * 2;
            let dst = count * DIMENSION;
            Vectors::floats_from_half(
                &self.map[src..src + count * DIMENSION * 2],
                &mut block[..dst],
            );
            for i in 0..count {
                let off = i * DIMENSION;
                scores[row + i] = block[off..off + DIMENSION]
                    .iter()
                    .zip(query.iter())
                    .map(|(a, b)| a * b)
                    .sum();
            }
            row += count;
        }
        scores
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_roundtrip_stays_close() {
        let v: Vec<f32> = (0..DIMENSION).map(|i| (i as f32) * 0.001 - 0.3).collect();
        let bytes = Vectors::half(&v);
        let mut back = vec![0f32; DIMENSION];
        Vectors::floats_from_half(&bytes, &mut back);
        let dot: f32 = v.iter().zip(&back).map(|(a, b)| a * b).sum();
        let na: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        let nb: f32 = back.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((dot / (na * nb)) > 0.999);
    }

    #[test]
    fn storage_block_rows_is_1024() {
        assert_eq!(VectorStorage::BLOCK_ROWS, 1024);
        let store = VectorStorage::new(3).unwrap();
        assert_eq!(store.rows, 3);
    }
}
