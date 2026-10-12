//! tesseract `--oem 1`. `upstream/Sources/DigUpKit/Extractors.swift` `OCR`.

use std::path::Path;

use super::error::ExtractError;
use super::run::{abs_path, run_argv, stdout_text, TIMEOUT_OCR};

pub struct Ocr;

impl Ocr {
    /// Languages from installed packs, `eng` always first, skip `osd`.
    pub fn langs() -> String {
        let args = ["--list-langs".as_ref()];
        let Ok(out) = run_argv("tesseract", &args, TIMEOUT_OCR) else {
            return "eng".into();
        };
        let text = String::from_utf8_lossy(&out.stdout);
        let mut langs: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && *l != "osd" && !l.contains("available languages"))
            .collect();
        langs.retain(|l| *l != "List" && !l.starts_with("List "));
        if langs.is_empty() {
            return "eng".into();
        }
        if let Some(i) = langs.iter().position(|l| *l == "eng") {
            langs.swap(0, i);
        }
        langs.join("+")
    }

    pub fn text(path: &Path) -> Result<String, ExtractError> {
        let src = abs_path(path);
        let langs = Self::langs();
        let args = [
            src.as_os_str(),
            "stdout".as_ref(),
            "--oem".as_ref(),
            "1".as_ref(),
            "-l".as_ref(),
            langs.as_ref(),
        ];
        let output = run_argv("tesseract", &args, TIMEOUT_OCR)?;
        Ok(stdout_text(&output, "tesseract")?.trim().to_string())
    }

    pub fn reads_arabic() -> bool {
        Self::langs()
            .split('+')
            .any(|l| l.starts_with("ara") || l == "ar")
    }
}
