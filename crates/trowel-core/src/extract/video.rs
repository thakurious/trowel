//! Frames + ffprobe duration. `upstream/Sources/DigUpKit/MediaExtractors.swift`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::error::ExtractError;
use super::run::{abs_path, ffmpeg_timeout, run_argv, stdout_text};

#[derive(Debug, Clone)]
pub struct MediaPiece {
    pub path: PathBuf,
    pub start: f64,
    pub end: f64,
}

pub struct VideoExtractor;

pub struct MediaInfo;

impl MediaInfo {
    pub fn duration(url: &Path) -> Option<f64> {
        let src = abs_path(url);
        let args = [
            "-v".as_ref(),
            "error".as_ref(),
            "-show_entries".as_ref(),
            "format=duration".as_ref(),
            "-of".as_ref(),
            "default=noprint_wrappers=1:nokey=1".as_ref(),
            src.as_os_str(),
        ];
        let out = run_argv("ffprobe", &args, ffmpeg_timeout(60.0)).ok()?;
        let text = stdout_text(&out, "ffprobe").ok()?;
        let secs: f64 = text.trim().parse().ok()?;
        if secs.is_finite() && secs > 0.0 {
            Some(secs)
        } else {
            None
        }
    }

    pub fn has_audio(url: &Path) -> bool {
        let src = abs_path(url);
        let args = [
            "-v".as_ref(),
            "error".as_ref(),
            "-select_streams".as_ref(),
            "a".as_ref(),
            "-show_entries".as_ref(),
            "stream=index".as_ref(),
            "-of".as_ref(),
            "csv=p=0".as_ref(),
            src.as_os_str(),
        ];
        run_argv("ffprobe", &args, ffmpeg_timeout(60.0))
            .ok()
            .is_some_and(|o| !o.stdout.is_empty())
    }
}

impl VideoExtractor {
    pub fn keyframes(
        url: &Path,
        every: f64,
        max_pixel: i32,
        max_frames: i32,
        folder: &Path,
    ) -> Result<(Vec<MediaPiece>, f64, bool), ExtractError> {
        let src = abs_path(url);
        let duration = MediaInfo::duration(&src).unwrap_or(0.0);
        let has_audio = MediaInfo::has_audio(&src);
        if !(duration.is_finite() && duration > 0.0) {
            return Ok((Vec::new(), 0.0, has_audio));
        }
        std::fs::create_dir_all(folder).map_err(|err| ExtractError::Unreadable(err.to_string()))?;
        let step = every.max(duration / f64::from(max_frames));
        let mut frames = Vec::new();
        let mut last_hash: Option<u64> = None;
        let mut t = (0.5_f64).min(duration / 2.0);
        let mut kept = 0;
        while t < duration && kept < max_frames {
            let out = folder.join(format!("frame-{kept}.jpg"));
            let vf = format!("scale='min({max_pixel},iw)':-2");
            let ts = format!("{t:.3}");
            let args = [
                "-nostdin".as_ref(),
                "-loglevel".as_ref(),
                "error".as_ref(),
                "-ss".as_ref(),
                ts.as_ref(),
                "-i".as_ref(),
                src.as_os_str(),
                "-frames:v".as_ref(),
                "1".as_ref(),
                "-q:v".as_ref(),
                "3".as_ref(),
                "-vf".as_ref(),
                vf.as_ref(),
                "-y".as_ref(),
                out.as_os_str(),
            ];
            if run_argv("ffmpeg", &args, ffmpeg_timeout(duration)).is_ok() && out.exists() {
                let hash = average_hash(&out).unwrap_or(0);
                if let Some(prev) = last_hash {
                    if hamming(hash, prev) <= 4 {
                        t += step;
                        continue;
                    }
                }
                last_hash = Some(hash);
                frames.push(MediaPiece {
                    path: out,
                    start: t.max(0.0),
                    end: duration,
                });
                kept += 1;
            }
            t += step;
        }
        for i in 0..frames.len().saturating_sub(1) {
            frames[i].end = frames[i + 1].start;
        }
        Ok((frames, duration, has_audio))
    }
}

/// 8×8 average hash. Hamming ≤ 4 is a near-duplicate.
/// `upstream/Sources/DigUpKit/MediaExtractors.swift:140`
pub fn average_hash(image: &Path) -> Result<u64, ExtractError> {
    let args = [
        "-loglevel".as_ref(),
        "error".as_ref(),
        "-i".as_ref(),
        image.as_os_str(),
        "-vf".as_ref(),
        "scale=8:8,format=gray".as_ref(),
        "-frames:v".as_ref(),
        "1".as_ref(),
        "-f".as_ref(),
        "rawvideo".as_ref(),
        "-".as_ref(),
    ];
    let out = run_argv("ffmpeg", &args, Duration::from_secs(20))?;
    if out.stdout.len() < 64 {
        return Err(ExtractError::Unreadable("hash frame too small".into()));
    }
    let pixels = &out.stdout[..64];
    let mean = pixels.iter().map(|p| *p as i32).sum::<i32>() / 64;
    let mut hash = 0u64;
    for (i, pixel) in pixels.iter().enumerate() {
        if i32::from(*pixel) > mean {
            hash |= 1 << i;
        }
    }
    Ok(hash)
}

pub fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}
