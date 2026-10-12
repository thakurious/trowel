//! `vips thumbnail` shrink-on-load. `upstream/Sources/DigUpKit/Extractors.swift` `ImageExtractor`.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use exif::{In, Reader, Tag};

use super::error::ExtractError;
use super::run::{abs_path, run_argv, stdout_text, TIMEOUT_IMAGE};

#[derive(Debug)]
pub struct PreparedImage {
    pub path: PathBuf,
    pub width: i32,
    pub height: i32,
    /// EXIF DateTimeOriginal, as written by the camera.
    pub taken: Option<String>,
}

pub struct ImageExtractor;

impl ImageExtractor {
    pub fn prepare(
        url: &Path,
        max_pixel: i32,
        min_side: i32,
        folder: &Path,
        png: bool,
    ) -> Result<PreparedImage, ExtractError> {
        let src = abs_path(url);
        let (width, height) = dimensions(&src)?;
        if width.max(height) < min_side {
            return Err(ExtractError::TooSmall { width, height });
        }
        std::fs::create_dir_all(folder).map_err(|err| ExtractError::Unreadable(err.to_string()))?;
        let ext = if png { "png" } else { "jpg" };
        let out = folder.join(format!("{}.{ext}", uuid_like()));
        let size = max_pixel.to_string();
        let mut args: Vec<&std::ffi::OsStr> = vec!["thumbnail".as_ref(), src.as_os_str()];
        let out_arg = if png {
            out.as_os_str().to_os_string()
        } else {
            // JPEG quality 0.9 matches ImageIO's lossy compression.
            let s = format!("{}[Q=90]", out.display());
            std::ffi::OsString::from(s)
        };
        args.push(out_arg.as_os_str());
        args.push(size.as_ref());
        let output = run_argv("vips", &args, TIMEOUT_IMAGE)?;
        stdout_text(&output, "vips")?;
        Ok(PreparedImage {
            path: out,
            width,
            height,
            taken: datetime_original(&src),
        })
    }
}

fn dimensions(path: &Path) -> Result<(i32, i32), ExtractError> {
    let w = header_field(path, "width")?;
    let h = header_field(path, "height")?;
    Ok((w, h))
}

fn header_field(path: &Path, field: &str) -> Result<i32, ExtractError> {
    let args = ["-f".as_ref(), field.as_ref(), path.as_os_str()];
    let output = run_argv("vipsheader", &args, TIMEOUT_IMAGE)?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.trim()
        .parse()
        .map_err(|_| ExtractError::Unreadable(format!("bad {field}: {text}")))
}

fn datetime_original(path: &Path) -> Option<String> {
    let file = File::open(path).ok()?;
    let mut buf = BufReader::new(file);
    let exif = Reader::new().read_from_container(&mut buf).ok()?;
    let field = exif.get_field(Tag::DateTimeOriginal, In::PRIMARY)?;
    Some(field.display_value().to_string())
}

fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{t:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiny_png_is_too_small() {
        let dir = std::env::temp_dir().join(format!("trowel-img-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let src = dir.join("tiny.png");
        // 1×1 PNG
        let png = [
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1,
            8, 2, 0, 0, 0, 144, 119, 83, 222, 0, 0, 0, 12, 73, 68, 65, 84, 8, 215, 99, 248, 207,
            192, 0, 0, 0, 2, 0, 1, 226, 33, 188, 51, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
        ];
        std::fs::write(&src, png).unwrap();
        let err = ImageExtractor::prepare(&src, 1024, 256, &dir, true).unwrap_err();
        match err {
            ExtractError::TooSmall {
                width: 1,
                height: 1,
            } => {}
            other => panic!("{other}"),
        }
    }
}
