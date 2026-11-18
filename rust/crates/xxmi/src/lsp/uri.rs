//! `file:` URIs and paths, the way Node's `fileURLToPath` and `pathToFileURL` convert them.

use std::path::{Path, PathBuf};

use xxmi_core::paths::resolve_path;

pub fn uri_to_path(uri: &str) -> PathBuf {
    let rest = uri.strip_prefix("file://").unwrap_or(uri);
    // `file://host/share` is a UNC path, which only matters on Windows; `localhost` is the same as none.
    let path = rest.find('/').map_or("", |at| &rest[at..]);
    let decoded = percent_decode(path);

    // `/C:/dir` is `C:\dir` on Windows.
    let bytes = decoded.as_bytes();
    let drive =
        bytes.len() >= 3 && bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b':';
    let native = if cfg!(windows) && drive {
        &decoded[1..]
    } else {
        decoded.as_str()
    };
    resolve_path(Path::new(native))
}

pub fn path_to_uri(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let slash = if text.starts_with('/') { "" } else { "/" };

    let mut uri = format!("file://{slash}");
    for byte in text.bytes() {
        if needs_escape(byte) {
            uri.push_str(&format!("%{byte:02X}"));
        } else {
            uri.push(char::from(byte));
        }
    }
    uri
}

/// The URL path percent-encode set, plus `%` which would read as an escape.
fn needs_escape(byte: u8) -> bool {
    byte <= 0x20 || byte >= 0x7f || b"\"#<>?`{}%".contains(&byte)
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let escaped = (bytes[i] == b'%')
            .then(|| bytes.get(i + 1..i + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok());
        match escaped {
            Some(byte) => {
                out.push(byte);
                i += 3;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn round_trips_escaped_paths() {
        let path = Path::new("/tmp/My Mods/a#b/é.ini");
        let uri = path_to_uri(path);

        assert_eq!(uri, "file:///tmp/My%20Mods/a%23b/%C3%A9.ini");
        assert_eq!(uri_to_path(&uri), path);
    }

    #[test]
    fn ignores_a_localhost_authority() {
        assert_eq!(
            uri_to_path("file://localhost/tmp/a.ini"),
            Path::new("/tmp/a.ini")
        );
    }
}
