//! Per-modality extractors. Tools run as subprocesses (`ROBUSTNESS.md` F5).

use std::path::Path;

mod audio;
mod code;
mod doc;
mod error;
mod image;
mod ocr;
mod pdf;
mod run;
mod video;

#[cfg(test)]
mod traps;

pub use audio::AudioExtractor;
pub use code::{CodeExtractor, CodeFile, CodeLookup, CodePiece};
pub use doc::{excerpt, DocExtractor};
pub use error::ExtractError;
pub use image::{ImageExtractor, PreparedImage};
pub use ocr::Ocr;
pub use pdf::{PdfExtractor, PdfPage};
pub use run::{
    abs_path, ffmpeg_timeout, run_tool, TIMEOUT_DOCX, TIMEOUT_FFMPEG, TIMEOUT_IMAGE, TIMEOUT_OCR,
    TIMEOUT_PDF_PAGE,
};
pub use video::{average_hash, hamming, MediaInfo, MediaPiece, VideoExtractor};

/// F5 boundary: each extractor talks to an external tool through [`run_tool`].
pub trait Extractor {
    type Output;
    fn extract(&self, path: &Path) -> Result<Self::Output, ExtractError>;
}

impl Extractor for DocExtractor {
    type Output = String;
    fn extract(&self, path: &Path) -> Result<Self::Output, ExtractError> {
        Self::text_of(path)
    }
}
