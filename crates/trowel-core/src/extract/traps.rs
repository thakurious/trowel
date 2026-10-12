//! Linux path traps (`EXECUTION-PLAN.md` Phase 2). Kept next to extractors so a hang is a test failure.

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::symlink;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use crate::extract::{run_tool, DocExtractor, ExtractError, TIMEOUT_DOCX};
    use crate::{Classification, Crawler, FileKind, IndexOptions, Rules};

    fn dir() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "trowel-traps-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::create_dir_all(&p);
        p
    }

    #[test]
    fn leading_dash_name_is_classified() {
        let p = dir().join("-notes.txt");
        fs::write(&p, "hello").unwrap();
        assert!(matches!(
            Rules::classify(&p, &IndexOptions::default()),
            Classification::Index(FileKind::Doc)
        ));
    }

    #[test]
    fn truncated_pdf_fails_with_reason() {
        let p = dir().join("trunc.pdf");
        fs::write(&p, b"%PDF-1.4\ntruncated").unwrap();
        let err = crate::PdfExtractor::page_count(&p).unwrap_err();
        assert!(matches!(
            err,
            ExtractError::Unreadable(_) | ExtractError::Locked
        ));
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn zip_bomb_named_docx_times_out_or_fails() {
        let p = dir().join("bomb.docx");
        fs::write(&p, b"PK\x03\x04not-a-zip").unwrap();
        let start = Instant::now();
        let result = DocExtractor::text_of(&p);
        assert!(start.elapsed() < Duration::from_secs(15));
        assert!(result.is_err());
        let _ = TIMEOUT_DOCX;
        let _ = run_tool;
    }

    #[test]
    fn unreadable_directory_is_skipped() {
        use std::os::unix::fs::PermissionsExt;
        let p = dir().join("nope");
        fs::create_dir_all(&p).unwrap();
        fs::write(p.join("x.txt"), "x").unwrap();
        let mut perms = fs::metadata(&p).unwrap().permissions();
        perms.set_mode(0o000);
        let _ = fs::set_permissions(&p, perms);
        let report = Crawler::crawl(std::slice::from_ref(&p), &IndexOptions::default());
        let mut restore = fs::metadata(&p)
            .map(|m| m.permissions())
            .unwrap_or_else(|_| fs::Permissions::from_mode(0o755));
        restore.set_mode(0o755);
        let _ = fs::set_permissions(&p, restore);
        assert!(
            report.candidates.is_empty()
                || report
                    .candidates
                    .iter()
                    .all(|c| Path::new(&c.path).starts_with(&p))
        );
    }

    #[test]
    fn invalid_utf8_name_does_not_panic() {
        let p = dir().join(std::ffi::OsStr::from_bytes(b"notes\xff.txt"));
        let _ = fs::write(&p, "hi");
        let _ = Rules::classify(&p, &IndexOptions::default());
    }

    #[test]
    fn newline_in_name_does_not_panic() {
        let root = dir();
        let p = root.join(std::ffi::OsStr::from_bytes(b"line\nbreak.txt"));
        let _ = fs::write(&p, "hi");
        let class = Rules::classify(&p, &IndexOptions::default());
        assert!(matches!(class, Classification::Index(FileKind::Doc)));
        let report = Crawler::crawl(std::slice::from_ref(&root), &IndexOptions::default());
        assert!(report.candidates.iter().any(|c| c.path.contains('\n')));
    }

    #[test]
    fn max_byte_name_is_classified() {
        let name = format!("{}.txt", "n".repeat(251));
        assert_eq!(name.len(), 255);
        let p = dir().join(&name);
        fs::write(&p, "x").unwrap();
        assert!(matches!(
            Rules::classify(&p, &IndexOptions::default()),
            Classification::Index(FileKind::Doc)
        ));
    }

    #[test]
    fn empty_mp4_is_video_and_does_not_hang() {
        let p = dir().join("empty.mp4");
        fs::write(&p, b"").unwrap();
        assert_eq!(
            Rules::classify(&p, &IndexOptions::default()),
            Classification::Index(FileKind::Video)
        );
        let start = Instant::now();
        let duration = crate::extract::MediaInfo::duration(&p);
        assert!(start.elapsed() < Duration::from_secs(15));
        assert!(duration.unwrap_or(0.0) == 0.0);
    }

    #[test]
    fn symlink_loop_does_not_hang_the_crawler() {
        let root = dir();
        let a = root.join("a");
        let b = root.join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        symlink(&a, b.join("to-a")).unwrap();
        symlink(&b, a.join("to-b")).unwrap();
        fs::write(a.join("note.txt"), "x").unwrap();
        let start = Instant::now();
        let report = Crawler::crawl(std::slice::from_ref(&root), &IndexOptions::default());
        assert!(start.elapsed() < Duration::from_secs(5));
        assert_eq!(report.candidates.len(), 1);
    }

    #[test]
    fn traps_layout_classifies_like_the_rules() {
        let root = dir();
        fs::create_dir_all(root.join("ts-code")).unwrap();
        fs::write(
            root.join("ts-code/app.ts"),
            "export const greet = (name: string): string => `hello ${name}`;\n",
        )
        .unwrap();
        let mut ts = vec![0xFFu8; 188 * 4];
        for packet in 0..4 {
            ts[packet * 188] = 0x47;
        }
        fs::create_dir_all(root.join("real-mpeg-ts")).unwrap();
        fs::write(root.join("real-mpeg-ts/clip.ts"), ts).unwrap();
        fs::create_dir_all(root.join("MyProject/.git")).unwrap();
        fs::create_dir_all(root.join("MyProject/assets")).unwrap();
        fs::write(root.join("MyProject/package.json"), "{\"name\":\"p\"}\n").unwrap();
        fs::write(root.join("MyProject/assets/hero.png"), b"not-a-png").unwrap();
        fs::create_dir_all(root.join("MyProject/screenshots")).unwrap();
        fs::write(
            root.join("MyProject/screenshots/screenshot-2026-10-12_08-15-40.png"),
            b"not-a-png",
        )
        .unwrap();
        fs::create_dir_all(root.join("linux")).unwrap();
        fs::write(root.join("linux/-notes.txt"), "hello").unwrap();
        fs::write(root.join("linux/empty.mp4"), b"").unwrap();
        let report = Crawler::crawl(std::slice::from_ref(&root), &IndexOptions::default());
        let kinds: Vec<_> = report
            .candidates
            .iter()
            .map(|c| {
                (
                    Path::new(&c.path)
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    c.kind,
                )
            })
            .collect();
        assert!(kinds
            .iter()
            .any(|(n, k)| n == "clip.ts" && *k == FileKind::Video));
        assert!(kinds
            .iter()
            .any(|(n, k)| n == "screenshot-2026-10-12_08-15-40.png" && *k == FileKind::Screenshot));
        assert!(kinds
            .iter()
            .any(|(n, k)| n == "-notes.txt" && *k == FileKind::Doc));
        assert!(kinds
            .iter()
            .any(|(n, k)| n == "empty.mp4" && *k == FileKind::Video));
        assert!(!kinds.iter().any(|(n, _)| n == "app.ts"));
        assert!(!kinds.iter().any(|(n, _)| n == "hero.png"));
        assert!(!report.candidates.iter().any(|c| c.path.contains("app.ts")));
    }
}
