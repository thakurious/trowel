//! Linux paths that aren't UTF-8. `PROTOCOL.md` §6.

use serde::{Deserialize, Serialize};

/// A filesystem path on the wire: a UTF-8 string, or base64 of the raw bytes plus a lossy display form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PathValue {
    Utf8(String),
    Raw { b64: String, display: String },
}

impl PathValue {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        match std::str::from_utf8(bytes) {
            Ok(s) => Self::Utf8(s.to_string()),
            Err(_) => Self::Raw {
                b64: {
                    use base64::Engine as _;
                    base64::engine::general_purpose::STANDARD.encode(bytes)
                },
                display: String::from_utf8_lossy(bytes).into_owned(),
            },
        }
    }

    pub fn from_path(path: &std::path::Path) -> Self {
        use std::os::unix::ffi::OsStrExt;
        Self::from_bytes(path.as_os_str().as_bytes())
    }

    /// The raw path back (a `b64` that doesn't decode falls back to `display`).
    pub fn to_path_buf(&self) -> std::path::PathBuf {
        use std::os::unix::ffi::OsStringExt;
        match self {
            Self::Utf8(s) => s.into(),
            Self::Raw { b64, display } => {
                use base64::Engine as _;
                match base64::engine::general_purpose::STANDARD.decode(b64) {
                    Ok(bytes) => std::ffi::OsString::from_vec(bytes).into(),
                    Err(_) => display.into(),
                }
            }
        }
    }

    pub fn display(&self) -> &str {
        match self {
            Self::Utf8(s) => s,
            Self::Raw { display, .. } => display,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_is_a_plain_string() {
        let v = PathValue::from_bytes(b"/home/u/tax.pdf");
        let json = serde_json::to_value(&v).unwrap();
        assert_eq!(json, serde_json::json!("/home/u/tax.pdf"));
    }

    #[test]
    fn non_utf8_is_b64_object() {
        let v = PathValue::from_bytes(b"/tmp/notes\xff.txt");
        let json = serde_json::to_value(&v).unwrap();
        assert!(json.get("b64").is_some());
        assert_eq!(json["display"].as_str().unwrap(), "/tmp/notes\u{FFFD}.txt");
        let back: PathValue = serde_json::from_value(json).unwrap();
        assert_eq!(back, v);
        use std::os::unix::ffi::OsStrExt;
        assert_eq!(
            back.to_path_buf().as_os_str().as_bytes(),
            b"/tmp/notes\xff.txt"
        );
    }
}
