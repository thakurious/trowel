//! poppler. `upstream/Sources/DigUpKit/Extractors.swift` `PDFExtractor`.

use std::path::{Path, PathBuf};

use crate::search_text::PageTextProblem;

use super::error::ExtractError;
use super::ocr::Ocr;
use super::run::{abs_path, run_argv, run_tool, stdout_text, TIMEOUT_PDF_PAGE};

pub struct PdfPage {
    pub number: i32,
    pub text: String,
    pub image_path: Option<PathBuf>,
    pub ocr: bool,
    pub garbled: Option<String>,
}

pub struct PdfExtractor;

impl PdfExtractor {
    pub fn pages(
        url: &Path,
        mut range: impl FnMut(i32) -> std::ops::Range<i32>,
        min_text: i32,
        render_pixels: i32,
        folder: &Path,
    ) -> Result<(Vec<PdfPage>, i32, Option<String>), ExtractError> {
        let src = abs_path(url);
        let info = pdfinfo(&src)?;
        if info.encrypted {
            return Err(ExtractError::Locked);
        }
        let total = info.pages;
        let title = info.title;
        std::fs::create_dir_all(folder).map_err(|err| ExtractError::Unreadable(err.to_string()))?;
        let mut pages = Vec::new();
        let span = range(total);
        for index in span.start.max(0)..span.end.min(total) {
            let number = index + 1;
            let mut text = pdftotext_page(&src, number)?.trim().to_string();
            let mut garbled = None;
            match PageTextProblem::check(&text) {
                Some(PageTextProblem::LostLetters) => {
                    if Ocr::reads_arabic() {
                        if let Ok(image) = render_page(&src, number, 2000, folder) {
                            if let Ok(read) = Ocr::text(&image) {
                                let read = read.trim().to_string();
                                if read.chars().count() as i32 >= min_text
                                    && PageTextProblem::check(&read).is_none()
                                    && read.chars().any(is_arabic)
                                {
                                    pages.push(PdfPage {
                                        number,
                                        text: read,
                                        image_path: None,
                                        ocr: true,
                                        garbled: None,
                                    });
                                    continue;
                                }
                            }
                        }
                    }
                    if text.chars().count() as i32 >= min_text {
                        pages.push(PdfPage {
                            number,
                            text,
                            image_path: None,
                            ocr: false,
                            garbled: None,
                        });
                        continue;
                    }
                }
                Some(PageTextProblem::GlyphOrder) => {
                    garbled = Some(text);
                    text = String::new();
                }
                None => {
                    if text.chars().count() as i32 >= min_text {
                        pages.push(PdfPage {
                            number,
                            text,
                            image_path: None,
                            ocr: false,
                            garbled: None,
                        });
                        continue;
                    }
                }
            }
            let image = render_page(&src, number, render_pixels, folder)?;
            pages.push(PdfPage {
                number,
                text,
                image_path: Some(image),
                ocr: false,
                garbled,
            });
        }
        Ok((pages, total, title))
    }

    pub fn page_count(url: &Path) -> Result<i32, ExtractError> {
        Ok(pdfinfo(&abs_path(url))?.pages)
    }
}

struct Info {
    pages: i32,
    title: Option<String>,
    encrypted: bool,
}

fn pdfinfo(path: &Path) -> Result<Info, ExtractError> {
    let output = run_tool("pdfinfo", &[], &[path.as_os_str()], TIMEOUT_PDF_PAGE)?;
    let text = stdout_text(&output, "pdfinfo")?;
    let mut pages = 0;
    let mut title = None;
    let mut encrypted = false;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("Pages:") {
            pages = v.trim().parse().unwrap_or(0);
        } else if let Some(v) = line.strip_prefix("Title:") {
            let t = v.trim();
            if !t.is_empty() {
                title = Some(t.to_string());
            }
        } else if let Some(v) = line.strip_prefix("Encrypted:") {
            encrypted = !v.trim().eq_ignore_ascii_case("no");
        }
    }
    Ok(Info {
        pages,
        title,
        encrypted,
    })
}

fn pdftotext_page(path: &Path, number: i32) -> Result<String, ExtractError> {
    let n = number.to_string();
    let src = abs_path(path);
    // poppler: `pdftotext [opts] PDF-file [text-file]`. `-` as the PDF path
    // (before `--`) is an empty stdin document. File then stdout `-`, after `--`
    // so a leading dash in the path cannot look like a flag.
    let output = run_argv(
        "pdftotext",
        &[
            "-layout".as_ref(),
            "-f".as_ref(),
            n.as_ref(),
            "-l".as_ref(),
            n.as_ref(),
            "--".as_ref(),
            src.as_os_str(),
            "-".as_ref(),
        ],
        TIMEOUT_PDF_PAGE,
    )?;
    stdout_text(&output, "pdftotext")
}

fn render_page(
    path: &Path,
    number: i32,
    pixels: i32,
    folder: &Path,
) -> Result<PathBuf, ExtractError> {
    let prefix = folder.join(format!("page-{number}-{}", std::process::id()));
    let n = number.to_string();
    let scale = pixels.to_string();
    let output = run_argv(
        "pdftoppm",
        &[
            "-png".as_ref(),
            "-f".as_ref(),
            n.as_ref(),
            "-l".as_ref(),
            n.as_ref(),
            "-scale-to".as_ref(),
            scale.as_ref(),
            path.as_os_str(),
            prefix.as_os_str(),
        ],
        TIMEOUT_PDF_PAGE,
    )?;
    if !output.status.success() {
        return Err(ExtractError::Unreadable("pdftoppm failed".into()));
    }
    let out = prefix.with_extension("png");
    let numbered = folder.join(format!(
        "page-{number}-{}-{number:06}.png",
        std::process::id()
    ));
    // pdftoppm writes prefix-1.png, prefix-01.png, or prefix-001.png
    let mut candidates = vec![
        PathBuf::from(format!("{}-{number}.png", prefix.display())),
        PathBuf::from(format!("{}-{:02}.png", prefix.display(), number)),
        PathBuf::from(format!("{}-{:03}.png", prefix.display(), number)),
        PathBuf::from(format!("{}-{:06}.png", prefix.display(), number)),
        PathBuf::from(format!("{}-1.png", prefix.display())),
        PathBuf::from(format!("{}-001.png", prefix.display())),
        out,
        numbered,
    ];
    if let Some(stem) = prefix.file_name() {
        let stem = stem.to_string_lossy();
        if let Ok(entries) = std::fs::read_dir(folder) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with(&*stem) && name.ends_with(".png") {
                    candidates.push(entry.path());
                }
            }
        }
    }
    for candidate in candidates {
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(ExtractError::Unreadable("pdftoppm wrote no page".into()))
}

fn is_arabic(ch: char) -> bool {
    matches!(ch as u32, 0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF)
}
