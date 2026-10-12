//! Which folders Trowel searches. `upstream/Sources/DigUpKit/FolderSelection.swift`

/// `upstream/Sources/DigUpKit/FolderSelection.swift:10`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderSelection {
    pub chosen: Vec<String>,
    pub skipped: Vec<String>,
    pub kept: Vec<String>,
}

/// A folder's checkbox: all of it, some of its subfolders, or none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    On,
    Mixed,
    Off,
}

impl FolderSelection {
    pub fn new(chosen: Vec<String>, skipped: Vec<String>, kept: Vec<String>) -> Self {
        Self {
            chosen: unique(chosen),
            skipped: unique(skipped),
            kept: unique(kept),
        }
    }

    pub fn is_searched(&self, path: &str) -> bool {
        Self::is_searched_in(
            path,
            &[&self.kept[..], &self.chosen[..]].concat(),
            &self.skipped,
        )
    }

    pub fn is_searched_in(path: &str, folders: &[String], skipped: &[String]) -> bool {
        folders.iter().any(|folder| {
            (path == folder || is_inside(path, folder))
                && !skipped
                    .iter()
                    .any(|skip| is_inside(skip, folder) && (path == skip || is_inside(path, skip)))
        })
    }

    pub fn searched_as(&self, path: &str) -> Option<&str> {
        let folders: Vec<&String> = self.kept.iter().chain(self.chosen.iter()).collect();
        folders
            .into_iter()
            .filter(|folder| {
                (path == folder.as_str() || is_inside(path, folder))
                    && Self::is_searched_in(path, std::slice::from_ref(*folder), &self.skipped)
            })
            .max_by_key(|f| f.len())
            .map(|s| s.as_str())
    }

    pub fn folders(&self) -> Vec<String> {
        let all = unique(
            self.kept
                .iter()
                .chain(self.chosen.iter())
                .cloned()
                .collect(),
        );
        all.iter()
            .filter(|folder| {
                !all.iter().any(|other| {
                    other != *folder
                        && is_inside(folder, other)
                        && Self::is_searched_in(folder, std::slice::from_ref(other), &self.skipped)
                })
            })
            .cloned()
            .collect()
    }

    pub fn effective_skips(&self) -> Vec<String> {
        let folders = self.folders();
        self.skipped
            .iter()
            .filter(|skip| folders.iter().any(|f| is_inside(skip, f)))
            .cloned()
            .collect()
    }

    pub fn mark(&self, folder: &str) -> Mark {
        if self.is_searched(folder) {
            if self
                .skipped
                .iter()
                .any(|s| parent(s) == folder && !self.is_searched(s))
            {
                Mark::Mixed
            } else {
                Mark::On
            }
        } else if self
            .kept
            .iter()
            .chain(self.chosen.iter())
            .any(|p| parent(p) == folder)
        {
            Mark::Mixed
        } else {
            Mark::Off
        }
    }

    pub fn set_folder(&mut self, folder: &str, on: bool) {
        self.skipped.retain(|s| parent(s) != folder);
        self.chosen.retain(|s| parent(s) != folder);
        self.set_subfolder(folder, on);
    }

    pub fn set_subfolder(&mut self, folder: &str, on: bool) {
        if on {
            self.skipped.retain(|s| s != folder);
            if !self.is_searched(folder) {
                self.chosen.push(folder.to_string());
            }
        } else {
            self.chosen.retain(|s| s != folder);
            if self.is_searched(folder) {
                self.skipped.push(folder.to_string());
            }
        }
    }

    pub fn unskip(&mut self, folder: &str) {
        self.skipped.retain(|s| s != folder);
    }
}

/// `upstream/Sources/DigUpKit/FolderSelection.swift:95`
pub fn is_inside(path: &str, folder: &str) -> bool {
    let prefix = if folder.ends_with('/') {
        folder.to_string()
    } else {
        format!("{folder}/")
    };
    path.starts_with(&prefix)
}

/// `upstream/Sources/DigUpKit/FolderSelection.swift:99`
pub fn parent(path: &str) -> String {
    match path.rsplit_once('/') {
        Some(("", _)) => "/".to_string(),
        Some((head, _)) => head.to_string(),
        None => path.to_string(),
    }
}

/// `upstream/Sources/DigUpKit/FolderSelection.swift:104`
pub fn real_path(path: &str) -> String {
    std::fs::canonicalize(path)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_string())
}

fn unique(paths: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    paths
        .into_iter()
        .filter(|p| seen.insert(p.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchecking_a_subfolder_skips_it() {
        let mut selection = FolderSelection::new(vec!["/D".into()], vec![], vec![]);
        selection.set_subfolder("/D/Courses", false);
        assert_eq!(selection.skipped, ["/D/Courses"]);
        assert_eq!(selection.mark("/D"), Mark::Mixed);
        assert!(!selection.is_searched("/D/Courses/week1.mp4"));
        assert!(selection.is_searched("/D/Books/a.pdf"));
        selection.set_folder("/D", true);
        assert!(selection.skipped.is_empty());
        assert_eq!(selection.mark("/D"), Mark::On);
    }

    #[test]
    fn checking_a_subfolder_chooses_it_alone() {
        let mut selection = FolderSelection::new(vec![], vec![], vec![]);
        selection.set_subfolder("/D/Receipts", true);
        assert_eq!(selection.chosen, ["/D/Receipts"]);
        assert_eq!(selection.mark("/D"), Mark::Mixed);
        assert!(selection.is_searched("/D/Receipts/may.pdf"));
        assert!(!selection.is_searched("/D/other.pdf"));
        assert_eq!(selection.folders(), vec!["/D/Receipts"]);
        selection.set_folder("/D", true);
        assert_eq!(selection.chosen, ["/D"]);
        selection.set_folder("/D", false);
        assert!(selection.chosen.is_empty());
        assert_eq!(selection.mark("/D"), Mark::Off);
    }

    #[test]
    fn unchecking_a_folder_drops_its_subfolder_choices_but_not_deeper_skips() {
        let mut selection = FolderSelection::new(
            vec!["/D".into()],
            vec!["/D/A".into(), "/D/B/old".into()],
            vec![],
        );
        selection.set_folder("/D", false);
        assert!(selection.chosen.is_empty());
        assert_eq!(selection.skipped, ["/D/B/old"]);
        assert!(selection.effective_skips().is_empty());
    }

    #[test]
    fn a_chosen_folder_inside_a_skipped_one_is_searched_whole() {
        let mut selection =
            FolderSelection::new(vec!["/D".into()], vec!["/D/Media".into()], vec![]);
        selection.set_subfolder("/D/Media/Courses", true);
        assert_eq!(selection.chosen, vec!["/D", "/D/Media/Courses"]);
        assert!(selection.is_searched("/D/Media/Courses/week1/a.mp4"));
        assert!(!selection.is_searched("/D/Media/film.mp4"));
        assert_eq!(
            selection.folders(),
            vec!["/D".to_string(), "/D/Media/Courses".to_string()]
        );
        selection.set_subfolder("/D/Media/Courses/old", false);
        assert!(!selection.is_searched("/D/Media/Courses/old/b.mp4"));
    }

    #[test]
    fn folders_inside_another_chosen_one_are_covered() {
        let selection = FolderSelection::new(
            vec!["/D/A".into(), "/D".into(), "/E".into()],
            vec![],
            vec![],
        );
        assert_eq!(selection.folders(), vec!["/D", "/E"]);
        assert_eq!(selection.searched_as("/D/A/x.png"), Some("/D/A"));
    }

    #[test]
    fn kept_folders_stay_whatever_is_clicked() {
        let mut selection = FolderSelection::new(vec![], vec![], vec!["/D/Receipts".into()]);
        assert_eq!(selection.mark("/D"), Mark::Mixed);
        selection.set_folder("/D", true);
        assert_eq!(selection.folders(), vec!["/D"]);
        selection.set_folder("/D", false);
        assert_eq!(selection.folders(), vec!["/D/Receipts"]);
        assert_eq!(selection.mark("/D"), Mark::Mixed);
    }

    #[test]
    fn checking_a_skipped_folder_takes_the_skip_back() {
        let mut selection =
            FolderSelection::new(vec![], vec!["/H/Movies".into()], vec!["/H".into()]);
        assert!(!selection.is_searched("/H/Movies"));
        selection.set_folder("/H/Movies", true);
        assert!(selection.skipped.is_empty());
        assert!(selection.chosen.is_empty());
        selection.set_folder("/H/Movies", false);
        assert_eq!(selection.skipped, ["/H/Movies"]);
    }

    #[test]
    fn paths_are_compared_by_whole_names() {
        assert!(!is_inside("/a/bc", "/a/b"));
        assert!(is_inside("/a/b/c", "/a/b"));
        assert!(!FolderSelection::new(vec!["/a/b".into()], vec![], vec![]).is_searched("/a/bc/x"));
    }
}
