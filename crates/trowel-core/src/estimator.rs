//! First-index time from metadata. `upstream/Sources/DigUpKit/Estimator.swift`.
//! Page counts and durations come from poppler/ffprobe, not Spotlight.

use std::collections::HashMap;
use std::path::Path;

use crate::extract::{MediaInfo, PdfExtractor};
use crate::folder_selection;
use crate::model::{Candidate, FileKind, IndexOptions};

/// Seconds of indexing per unit. `upstream/Sources/DigUpKit/Estimator.swift:8`
#[derive(Debug, Clone)]
pub struct Costs {
    pub image: f64,
    pub screenshot: f64,
    pub pdf_page: f64,
    pub doc_chunk: f64,
    pub audio_second: f64,
    pub video_second: f64,
    pub code_stretch: f64,
}

impl Default for Costs {
    fn default() -> Self {
        Self {
            image: 0.12,
            screenshot: 0.32,
            pdf_page: 0.09,
            doc_chunk: 0.095,
            audio_second: 0.0089,
            video_second: 0.019,
            code_stretch: 0.037,
        }
    }
}

/// `upstream/Sources/DigUpKit/Estimator.swift:22`
pub struct CodeIndexSize;

impl CodeIndexSize {
    pub const PER_CHARACTER: f64 = 2.4;
    pub const PER_STRETCH: f64 = 1600.0;
    pub const CHARACTERS_PER_STRETCH: i64 = 1400;
}

#[derive(Debug, Clone, Default)]
pub struct Estimate {
    pub files: HashMap<FileKind, usize>,
    pub pdf_pages: i32,
    pub pdf_pages_total: i32,
    pub audio_seconds: f64,
    pub video_seconds: f64,
    pub doc_chunks: i32,
    pub seconds: f64,
    pub long_files: i32,
    pub later_seconds: f64,
    pub code_stretches: i32,
    pub bytes: f64,
    /// Directories the crawl walked (F3 inotify budget).
    pub directories: usize,
}

pub struct Estimator;

impl Estimator {
    pub fn estimate(
        candidates: &[Candidate],
        options: &IndexOptions,
        costs: &Costs,
        directories: usize,
    ) -> Estimate {
        let mut estimate = Estimate {
            directories,
            ..Estimate::default()
        };
        for file in candidates {
            *estimate.files.entry(file.kind).or_default() += 1;
            match file.kind {
                FileKind::Image => estimate.seconds += costs.image,
                FileKind::Screenshot => estimate.seconds += costs.screenshot,
                FileKind::Pdf => {
                    let total = pages(&file.path);
                    let all = total.min(options.max_pages);
                    let first = if total > options.big_pdf_pages {
                        options.pdf_step.min(all)
                    } else {
                        all
                    };
                    estimate.pdf_pages_total += total;
                    estimate.pdf_pages += first;
                    estimate.seconds += f64::from(first) * costs.pdf_page;
                    if all > first {
                        estimate.long_files += 1;
                        estimate.later_seconds += f64::from(all - first) * costs.pdf_page;
                    }
                }
                FileKind::Doc => {
                    let all = (file.size / 2000).max(1).min(i64::from(options.max_chunks)) as i32;
                    let first = all.min(options.doc_step);
                    estimate.doc_chunks += first;
                    estimate.seconds += f64::from(first) * costs.doc_chunk;
                    if all > first {
                        estimate.long_files += 1;
                        estimate.later_seconds += f64::from(all - first) * costs.doc_chunk;
                    }
                }
                FileKind::Audio => {
                    let seconds = duration(&file.path);
                    estimate.audio_seconds += seconds;
                    estimate.seconds += seconds * costs.audio_second;
                }
                FileKind::Video => {
                    let seconds = duration(&file.path);
                    estimate.video_seconds += seconds;
                    estimate.seconds += seconds * costs.video_second;
                }
                FileKind::Code => {
                    let characters = if file.path.to_lowercase().ends_with(".ipynb") {
                        file.size / 8
                    } else {
                        file.size
                    };
                    let stretches = ((characters + CodeIndexSize::CHARACTERS_PER_STRETCH - 1)
                        / CodeIndexSize::CHARACTERS_PER_STRETCH)
                        .max(1) as i32;
                    estimate.code_stretches += stretches;
                    estimate.seconds += f64::from(stretches) * costs.code_stretch;
                    estimate.bytes += characters as f64 * CodeIndexSize::PER_CHARACTER
                        + f64::from(stretches) * CodeIndexSize::PER_STRETCH;
                }
            }
        }
        estimate
    }

    pub fn estimates_by_subfolder(
        candidates: &[Candidate],
        folder: &str,
        options: &IndexOptions,
        costs: &Costs,
    ) -> HashMap<String, Estimate> {
        let prefix = if folder.ends_with('/') {
            folder.to_string()
        } else {
            format!("{folder}/")
        };
        let real = format!("{}/", folder_selection::real_path(folder));
        let mut groups: HashMap<String, Vec<Candidate>> = HashMap::new();
        for file in candidates {
            let rest = file
                .path
                .strip_prefix(&prefix)
                .or_else(|| file.path.strip_prefix(&real));
            let key = rest
                .and_then(|rest| rest.find('/').map(|i| format!("{}{}", prefix, &rest[..i])))
                .unwrap_or_else(|| folder.to_string());
            groups.entry(key).or_default().push(file.clone());
        }
        groups
            .into_iter()
            .map(|(k, v)| (k, Self::estimate(&v, options, costs, 0)))
            .collect()
    }
}

fn pages(path: &str) -> i32 {
    PdfExtractor::page_count(Path::new(path))
        .unwrap_or(1)
        .max(1)
}

fn duration(path: &str) -> f64 {
    MediaInfo::duration(Path::new(path)).unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Candidate;

    fn file(name: &str, size: i64) -> Candidate {
        Candidate {
            path: format!("/x/{name}"),
            kind: FileKind::Code,
            size,
            mtime: 0.0,
            inode: 0,
            device: 0,
        }
    }

    #[test]
    fn code_estimates_time_and_room() {
        let options = IndexOptions {
            code: true,
            ..IndexOptions::default()
        };
        let estimate = Estimator::estimate(
            &[
                file("a.py", 14_000),
                file("b.swift", 100),
                file("c.ipynb", 112_000),
            ],
            &options,
            &Costs::default(),
            0,
        );
        assert_eq!(estimate.code_stretches, 10 + 1 + 10);
        assert_eq!(estimate.files.get(&FileKind::Code).copied(), Some(3));
        assert!((estimate.seconds - 21.0 * Costs::default().code_stretch).abs() < 1e-9);
        assert!(estimate.bytes > 21.0 * 1600.0 && estimate.bytes < 200_000.0);
    }

    #[test]
    fn first_pass_adds_per_kind_costs() {
        let files = [
            Candidate {
                path: "/x/a.png".into(),
                kind: FileKind::Image,
                size: 10,
                mtime: 0.0,
                inode: 0,
                device: 0,
            },
            Candidate {
                path: "/x/s.png".into(),
                kind: FileKind::Screenshot,
                size: 10,
                mtime: 0.0,
                inode: 0,
                device: 0,
            },
            Candidate {
                path: "/x/n.txt".into(),
                kind: FileKind::Doc,
                size: 2000,
                mtime: 0.0,
                inode: 0,
                device: 0,
            },
        ];
        let estimate = Estimator::estimate(&files, &IndexOptions::default(), &Costs::default(), 0);
        let costs = Costs::default();
        assert!(
            (estimate.seconds - (costs.image + costs.screenshot + costs.doc_chunk)).abs() < 1e-9
        );
        assert_eq!(estimate.doc_chunks, 1);
        assert_eq!(estimate.files.get(&FileKind::Image).copied(), Some(1));
    }
}
