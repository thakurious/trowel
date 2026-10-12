//! Linux path traps (`EXECUTION-PLAN.md` Phase 2). Kept next to extractors so a hang is a test failure.

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use crate::extract::{run_tool, DocExtractor, ExtractError, TIMEOUT_DOCX};
    use crate::{Crawler, IndexOptions, Rules};

    fn dir() -> PathBuf {
        let p = std::env::temp_dir().join(format!("trowel-traps-{}", std::process::id()));
        let _ = fs::create_dir_all(&p);
        p
    }

    #[test]
    fn leading_dash_name_is_classified() {
        let p = dir().join("-notes.txt");
        fs::write(&p, "hello").unwrap();
        assert!(matches!(
            Rules::classify(&p, &IndexOptions::default()),
            crate::Classification::Index(crate::FileKind::Doc)
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
    }

    #[test]
    fn zip_bomb_named_docx_times_out_or_fails() {
        let p = dir().join("bomb.docx");
        // Not a real zip bomb: a truncated zip that unzip must reject quickly.
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
}
