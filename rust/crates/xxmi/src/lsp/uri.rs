//! `file:` URIs and paths, the way Node's `fileURLToPath` and `pathToFileURL` convert them.

use std::path::{Path, PathBuf};

use xxmi_core::paths::{resolve_path, strip_verbatim};

pub fn uri_to_path(uri: &str) -> PathBuf {
    resolve_path(Path::new(&uri_to_native(uri, cfg!(windows))))
}

/// The path text a `file:` URI names. `windows` is a parameter so the Windows forms can be tested
/// on any OS.
fn uri_to_native(uri: &str, windows: bool) -> String {
    let rest = uri.strip_prefix("file://").unwrap_or(uri);
    // `file://host/share` is a UNC path, which only matters on Windows; `localhost` is the same as none.
    let (host, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    let decoded = percent_decode(path);
    if !windows {
        return decoded;
    }

    // `/C:/dir` is `C:\dir`; clients may send the colon as `%3A` and the drive letter in either case.
    let bytes = decoded.as_bytes();
    let drive =
        bytes.len() >= 3 && bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b':';
    if drive {
        return decoded[1..].replace('/', "\\");
    }
    if !host.is_empty() && !host.eq_ignore_ascii_case("localhost") {
        return format!("\\\\{host}{}", decoded.replace('/', "\\"));
    }
    decoded
}

pub fn path_to_uri(path: &Path) -> String {
    path_text_to_uri(&path.to_string_lossy())
}

fn path_text_to_uri(path: &str) -> String {
    let text = strip_verbatim(path).replace('\\', "/");
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

#[cfg(test)]
mod windows_tests {
    use super::*;

    #[test]
    fn decodes_an_encoded_drive_colon_in_either_case() {
        let expected = r"c:\mods\Lib\lib.ini";
        assert_eq!(
            uri_to_native("file:///c%3A/mods/Lib/lib.ini", true),
            expected
        );
        assert_eq!(uri_to_native("file:///c:/mods/Lib/lib.ini", true), expected);
        assert_eq!(
            uri_to_native("file:///C%3a/mods/Lib/lib.ini", true),
            r"C:\mods\Lib\lib.ini"
        );
    }

    #[test]
    fn decodes_unc_hosts() {
        assert_eq!(
            uri_to_native("file://host/share/a.ini", true),
            r"\\host\share\a.ini"
        );
        assert_eq!(
            uri_to_native("file://localhost/c%3A/a.ini", true),
            r"c:\a.ini"
        );
    }

    #[test]
    fn encodes_backslash_and_verbatim_paths() {
        let expected = "file:///C:/mods/Lib/lib.ini";
        assert_eq!(path_text_to_uri(r"C:\mods\Lib\lib.ini"), expected);
        assert_eq!(path_text_to_uri(r"C:\mods/Lib\lib.ini"), expected);
        assert_eq!(path_text_to_uri(r"\\?\C:\mods\Lib\lib.ini"), expected);
    }
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
