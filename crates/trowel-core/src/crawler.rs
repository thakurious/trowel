//! Walk chosen folders and apply `Rules`. `upstream/Sources/DigUpKit/Crawler.swift`.

use std::collections::{HashMap, HashSet};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::folder_selection;
use crate::git_files::GitRepos;
use crate::model::{Candidate, FileKind, IndexOptions};
use crate::rules::{Classification, Rules};

#[derive(Debug, Clone, Default)]
pub struct CrawlReport {
    pub candidates: Vec<Candidate>,
    pub skipped: HashMap<String, usize>,
    pub skipped_folders: HashMap<String, usize>,
    pub datasets: HashMap<String, usize>,
    /// Directories walked (F3 inotify budget).
    pub directories: usize,
}

pub struct Crawler;

impl Crawler {
    pub fn crawl(roots: &[PathBuf], options: &IndexOptions) -> CrawlReport {
        let home = PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/".into()));
        let mut report = CrawlReport::default();
        let mut project_roots = HashSet::new();
        let mut repos = GitRepos::default();
        let mut found: Vec<Candidate> = Vec::new();
        let mut found_paths = HashSet::new();
        let root_paths: HashSet<String> = roots
            .iter()
            .map(|r| r.to_string_lossy().into_owned())
            .collect();

        for root in roots {
            if options.code {
                repos.enter_root(root);
            } else if Rules::is_project_root(root) {
                project_roots.insert(root.to_string_lossy().into_owned());
            }
            let real = folder_selection::real_path(&root.to_string_lossy());
            let skips: Vec<String> = options
                .excluded_folders
                .iter()
                .filter(|ex| {
                    folder_selection::is_inside(ex, &root.to_string_lossy())
                        || folder_selection::is_inside(ex, &real)
                })
                .flat_map(|ex| [ex.clone(), folder_selection::real_path(ex)])
                .collect();

            let walker = WalkDir::new(root)
                .follow_links(false)
                .into_iter()
                .filter_entry(|e| {
                    let name = e.file_name().to_string_lossy();
                    !(name.starts_with('.') && name != "." && name != "..")
                });

            let mut skip_prefixes: Vec<PathBuf> = Vec::new();
            for entry in walker.flatten() {
                let path = entry.path();
                if skip_prefixes.iter().any(|p| path.starts_with(p)) {
                    continue;
                }
                let Ok(meta) = entry.metadata() else {
                    continue;
                };
                if meta.is_dir() {
                    if let Some(reason) = Rules::skip_reason_for_folder(path, &home, &skips)
                        .or_else(|| {
                            options
                                .code
                                .then(|| Rules::code_skip_reason(path).map(str::to_string))
                                .flatten()
                        })
                    {
                        skip_prefixes.push(path.to_path_buf());
                        *report.skipped_folders.entry(reason).or_default() += 1;
                    } else if options.code {
                        repos.enter(path);
                    } else if Rules::is_project_root(path) {
                        project_roots.insert(path.to_string_lossy().into_owned());
                    }
                    report.directories += 1;
                    continue;
                }
                if !meta.is_file() {
                    *report
                        .skipped
                        .entry("link or special file".into())
                        .or_default() += 1;
                    continue;
                }
                let classification = if options.code {
                    Rules::classify_code(path, options)
                } else {
                    Rules::classify(path, options)
                };
                match classification {
                    Classification::Ignore => {
                        *report.skipped.entry("other type".into()).or_default() += 1;
                    }
                    Classification::Skip(reason) => {
                        *report.skipped.entry(reason).or_default() += 1;
                    }
                    Classification::Index(kind) => {
                        if options.code {
                            let limit =
                                if path.extension().and_then(|s| s.to_str()) == Some("ipynb") {
                                    options.max_notebook_bytes
                                } else {
                                    options.max_code_bytes
                                };
                            if meta.size() as i64 > limit {
                                *report
                                    .skipped
                                    .entry("too big to be written by hand".into())
                                    .or_default() += 1;
                                continue;
                            }
                            if repos.ignores(path) {
                                *report.skipped.entry("ignored by git".into()).or_default() += 1;
                                continue;
                            }
                        }
                        let path_s = path.to_string_lossy().into_owned();
                        if !found_paths.insert(path_s.clone()) {
                            continue;
                        }
                        let mtime = meta.mtime() as f64 + meta.mtime_nsec() as f64 / 1e9;
                        found.push(Candidate {
                            path: path_s,
                            kind,
                            size: meta.size() as i64,
                            mtime,
                            inode: meta.ino(),
                            device: meta.dev(),
                        });
                    }
                }
            }
        }

        fn is_in_project(
            folder: &str,
            project_roots: &HashSet<String>,
            cache: &mut HashMap<String, bool>,
        ) -> bool {
            if let Some(known) = cache.get(folder) {
                return *known;
            }
            let parent = Path::new(folder)
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            let answer = project_roots.contains(folder)
                || (parent != folder
                    && parent.len() > 1
                    && is_in_project(&parent, project_roots, cache));
            cache.insert(folder.to_string(), answer);
            answer
        }

        let mut in_project = HashMap::new();
        let mut kept = Vec::new();
        for file in found {
            let folder = Path::new(&file.path)
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            if !options.code
                && file.kind != FileKind::Screenshot
                && is_in_project(&folder, &project_roots, &mut in_project)
            {
                *report
                    .skipped
                    .entry("inside a code project".into())
                    .or_default() += 1;
            } else {
                kept.push(file);
            }
        }

        if !options.include_datasets && !options.code {
            let mapped: Vec<_> = kept
                .iter()
                .map(|c| {
                    let folder = Path::new(&c.path)
                        .parent()
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let stem = Path::new(&c.path)
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    (folder, stem, c.kind)
                })
                .collect();
            let datasets = Rules::dataset_folders(&mapped, &root_paths, 300);
            if !datasets.is_empty() {
                report.datasets = datasets.clone();
                kept.retain(|file| {
                    let folder = Path::new(&file.path)
                        .parent()
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let skip = datasets.contains_key(&folder)
                        && matches!(
                            file.kind,
                            FileKind::Image | FileKind::Audio | FileKind::Video
                        );
                    if skip {
                        *report.skipped.entry("dataset".into()).or_default() += 1;
                    }
                    !skip
                });
            }
        }
        report.candidates = kept;
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn nested_roots_are_crawled_once() {
        let dir = std::env::temp_dir().join(format!("trowel-crawl-{}", std::process::id()));
        let inner = dir.join("inner");
        let _ = fs::create_dir_all(&inner);
        fs::write(inner.join("trip.txt"), "notes about the trip").unwrap();
        let report = Crawler::crawl(&[dir.clone(), inner], &IndexOptions::default());
        let names: Vec<_> = report
            .candidates
            .iter()
            .map(|c| {
                Path::new(&c.path)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(names, ["trip.txt"]);
    }

    #[test]
    fn code_crawl_keeps_what_git_keeps() {
        let folder = std::env::temp_dir().join(format!(
            "trowel-gitcrawl-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(folder.join("app/notes")).unwrap();
        fs::create_dir_all(folder.join("app/build")).unwrap();
        fs::create_dir_all(folder.join("app/vendor/lib")).unwrap();
        fs::create_dir_all(folder.join("app/node_modules/pad")).unwrap();
        fs::create_dir_all(folder.join("app/sub")).unwrap();
        fs::create_dir_all(folder.join("loose")).unwrap();
        fs::write(folder.join("app/main.py"), "print(1)\n").unwrap();
        fs::write(folder.join("app/README.md"), "# App\n").unwrap();
        fs::write(folder.join("app/.gitignore"), "build/\n*.log\nscratch.py\n").unwrap();
        fs::write(folder.join("app/scratch.py"), "x = 1\n").unwrap();
        fs::write(folder.join("app/notes/todo.md"), "- ship\n").unwrap();
        fs::write(folder.join("app/build/out.py"), "x = 2\n").unwrap();
        fs::write(folder.join("app/vendor/lib/core.py"), "x = 3\n").unwrap();
        fs::write(folder.join("app/node_modules/pad/index.js"), "x\n").unwrap();
        fs::write(folder.join("app/data.json"), "{}\n").unwrap();
        fs::write(folder.join("app/big.py"), "x = 1\n".repeat(200_000)).unwrap();
        fs::write(folder.join("app/sub/.git"), "gitdir: ../.git/modules/sub\n").unwrap();
        fs::write(folder.join("app/sub/lib.py"), "x = 4\n").unwrap();
        fs::write(folder.join("loose/backup.sh"), "rsync\n").unwrap();
        let status = std::process::Command::new("git")
            .args(["-C", folder.join("app").to_str().unwrap(), "init", "-q"])
            .status()
            .expect("git");
        assert!(status.success());
        let code = IndexOptions {
            code: true,
            ..IndexOptions::default()
        };
        let report = Crawler::crawl(std::slice::from_ref(&folder), &code);
        let mut names: Vec<_> = report
            .candidates
            .iter()
            .map(|c| {
                Path::new(&c.path)
                    .strip_prefix(&folder)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "app/README.md",
                "app/main.py",
                "app/notes/todo.md",
                "loose/backup.sh"
            ]
        );
        assert_eq!(report.skipped.get("ignored by git").copied(), Some(1));
        assert_eq!(
            report
                .skipped_folders
                .get("vendored or built code")
                .copied(),
            Some(2)
        );
        assert_eq!(
            report.skipped_folders.get("submodule or worktree").copied(),
            Some(1)
        );
        let sub = Crawler::crawl(&[folder.join("app")], &code);
        assert!(!sub
            .candidates
            .iter()
            .any(|c| c.path.ends_with("scratch.py")));
        let main = Crawler::crawl(std::slice::from_ref(&folder), &IndexOptions::default());
        assert!(main.candidates.is_empty());
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn crawl_counts_directories_for_inotify_budget() {
        let folder = std::env::temp_dir().join(format!(
            "trowel-dirs-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(folder.join("a/b")).unwrap();
        fs::write(folder.join("a/b/n.txt"), "the harbour").unwrap();
        let report = Crawler::crawl(std::slice::from_ref(&folder), &IndexOptions::default());
        assert!(
            report.directories >= 3,
            "directories={}",
            report.directories
        );
        let estimate = crate::estimator::Estimator::estimate(
            &report.candidates,
            &IndexOptions::default(),
            &crate::estimator::Costs::default(),
            report.directories,
        );
        assert_eq!(estimate.directories, report.directories);
        let _ = fs::remove_dir_all(&folder);
    }
}
