//! 16 kHz mono windows. `upstream/Sources/DigUpKit/MediaExtractors.swift` `AudioExtractor`.

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use super::error::ExtractError;
use super::run::{abs_path, ffmpeg_timeout};
use super::video::MediaPiece;

pub struct AudioExtractor;

impl AudioExtractor {
    pub const SAMPLE_RATE: f64 = 16_000.0;

    pub fn windows(
        url: &Path,
        window: f64,
        hop: f64,
        minimum: f64,
        folder: &Path,
    ) -> Result<(Vec<MediaPiece>, f64), ExtractError> {
        let src = abs_path(url);
        let duration = super::video::MediaInfo::duration(&src).unwrap_or(0.0);
        std::fs::create_dir_all(folder).map_err(|err| ExtractError::Unreadable(err.to_string()))?;
        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-nostdin", "-loglevel", "error", "-i"])
            .arg(&src)
            .args(["-f", "s16le", "-ac", "1", "-ar", "16000", "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        super::run::limit_child_memory(&mut cmd);
        let samples = decode_pcm(cmd, ffmpeg_timeout(duration))?;
        let window_samples = (window * Self::SAMPLE_RATE) as usize;
        let hop_samples = (hop * Self::SAMPLE_RATE) as usize;
        let min_samples = (minimum * Self::SAMPLE_RATE) as usize;
        let mut pieces = Vec::new();
        let mut offset = 0usize;
        if window_samples == 0 || hop_samples == 0 {
            return Ok((pieces, duration));
        }
        while offset + window_samples <= samples.len() {
            emit(
                &samples[offset..offset + window_samples],
                offset,
                folder,
                &mut pieces,
            )?;
            offset += hop_samples;
        }
        let already_covered = if offset == 0 {
            0
        } else {
            window_samples - hop_samples
        };
        let tail = samples.get(offset..).unwrap_or(&[]);
        if (tail.len().saturating_sub(already_covered.min(tail.len())) >= min_samples
            || (offset == 0 && samples.len() >= min_samples))
            && offset < samples.len()
        {
            emit(&samples[offset..], offset, folder, &mut pieces)?;
        }
        let dur = if duration > 0.0 {
            duration
        } else {
            samples.len() as f64 / Self::SAMPLE_RATE
        };
        Ok((pieces, dur))
    }
}

/// Runs the ffmpeg `cmd` (stdout = s16le PCM) and collects its samples within `timeout`.
/// stderr is drained on its own thread: a damaged AAC stream makes ffmpeg log megabytes of
/// decode errors, and an unread pipe fills at 64 KiB and blocks ffmpeg until the timeout.
fn decode_pcm(mut cmd: Command, timeout: Duration) -> Result<Vec<i16>, ExtractError> {
    let mut child = cmd
        .spawn()
        .map_err(|err| ExtractError::Unreadable(format!("ffmpeg: {err}")))?;
    let mut stdout = child.stdout.take().expect("stdout");
    if let Some(mut stderr) = child.stderr.take() {
        thread::spawn(move || {
            let _ = std::io::copy(&mut stderr, &mut std::io::sink());
        });
    }
    let (tx, rx) = std::sync::mpsc::channel::<Result<Vec<i16>, ExtractError>>();
    thread::spawn(move || {
        let mut raw = Vec::new();
        if let Err(err) = stdout.read_to_end(&mut raw) {
            let _ = tx.send(Err(ExtractError::Unreadable(err.to_string())));
            return;
        }
        let mut samples = Vec::with_capacity(raw.len() / 2);
        for chunk in raw.as_chunks::<2>().0 {
            samples.push(i16::from_le_bytes(*chunk));
        }
        let _ = tx.send(Ok(samples));
    });
    match rx.recv_timeout(timeout) {
        Ok(Ok(samples)) => {
            let _ = child.wait();
            Ok(samples)
        }
        Ok(Err(err)) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(err)
        }
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(ExtractError::Unreadable("ffmpeg timed out".into()))
        }
    }
}

fn emit(
    slice: &[i16],
    offset: usize,
    folder: &Path,
    pieces: &mut Vec<MediaPiece>,
) -> Result<(), ExtractError> {
    if rms(slice) <= 0.003 {
        return Ok(());
    }
    let out = folder.join(format!("win-{offset}.wav"));
    write_wav(slice, &out)?;
    pieces.push(MediaPiece {
        path: out,
        start: offset as f64 / AudioExtractor::SAMPLE_RATE,
        end: (offset + slice.len()) as f64 / AudioExtractor::SAMPLE_RATE,
    });
    Ok(())
}

fn rms(samples: &[i16]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut sum = 0.0;
    for sample in samples {
        let v = f64::from(*sample) / 32768.0;
        sum += v * v;
    }
    (sum / samples.len() as f64).sqrt()
}

fn write_wav(samples: &[i16], path: &Path) -> Result<(), ExtractError> {
    let mut data = Vec::with_capacity(44 + samples.len() * 2);
    fn append32(data: &mut Vec<u8>, v: u32) {
        data.extend_from_slice(&v.to_le_bytes());
    }
    fn append16(data: &mut Vec<u8>, v: u16) {
        data.extend_from_slice(&v.to_le_bytes());
    }
    let rate = 16_000u32;
    data.extend_from_slice(b"RIFF");
    append32(&mut data, 36 + samples.len() as u32 * 2);
    data.extend_from_slice(b"WAVE");
    data.extend_from_slice(b"fmt ");
    append32(&mut data, 16);
    append16(&mut data, 1);
    append16(&mut data, 1);
    append32(&mut data, rate);
    append32(&mut data, rate * 2);
    append16(&mut data, 2);
    append16(&mut data, 16);
    data.extend_from_slice(b"data");
    append32(&mut data, samples.len() as u32 * 2);
    for s in samples {
        data.extend_from_slice(&s.to_le_bytes());
    }
    File::create(path)
        .and_then(|mut f| f.write_all(&data))
        .map_err(|err| ExtractError::Unreadable(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silent_wav_yields_no_windows() {
        let dir = std::env::temp_dir().join(format!("trowel-aud-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let wav = dir.join("silence.wav");
        write_wav(&vec![0; 16_000 * 4], &wav).unwrap();
        let (pieces, dur) = AudioExtractor::windows(&wav, 30.0, 25.0, 2.0, &dir).unwrap();
        assert!(pieces.is_empty());
        assert!((dur - 4.0).abs() < 0.2);
    }

    /// 2 MiB of stderr (a damaged AAC stream's decode errors) must not stall the decode.
    #[test]
    fn noisy_stderr_does_not_block_decode() {
        let mut cmd = Command::new("sh");
        cmd.args([
            "-c",
            "head -c 2097152 /dev/zero | tr '\\0' x >&2; head -c 64000 /dev/zero",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        let start = std::time::Instant::now();
        let samples = decode_pcm(cmd, Duration::from_secs(20)).unwrap();
        assert_eq!(samples.len(), 32_000);
        assert!(start.elapsed() < Duration::from_secs(10));
    }
}
