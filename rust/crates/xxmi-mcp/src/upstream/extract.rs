//! Unpacks a tar stream into a directory, keeping only the text files worth browsing. The
//! filter is the safety net for a repository that suddenly ships gigabytes of textures.

use std::fs;
use std::io::{self, Read};
use std::path::Path;

use tar::{Archive, EntryType};

use crate::repos::RepoRole;

const TEXT_EXTENSIONS: [&str; 19] = [
    ".ini", ".md", ".txt", ".cpp", ".h", ".hpp", ".c", ".hlsl", ".hlsli", ".py", ".json", ".toml",
    ".j2", ".yml", ".yaml", ".sln", ".vcxproj", ".cs", ".bat",
];

/// What the spec extractor reads, next to root-level files.
const SPEC_DIRS: [&str; 3] = ["directx11", "d3dcompiler", "injector"];

pub const MAX_FILE_BYTES: u64 = 2_000_000;
pub const MAX_TOTAL_BYTES: u64 = 120_000_000;

/// Top-level folders worth keeping per role; third-party code in the others is skipped.
fn role_dirs(role: RepoRole) -> Option<Vec<&'static str>> {
    match role {
        RepoRole::Dll => Some([&SPEC_DIRS[..], &["injectorlib", "docs"]].concat()),
        RepoRole::Baseline => Some(SPEC_DIRS.to_vec()),
        RepoRole::Package | RepoRole::Tools => None,
    }
}

fn extension_of(path: &str) -> String {
    path.rfind('.')
        .map_or_else(String::new, |dot| path[dot..].to_lowercase())
}

/// Whether `rel_path` (inside the repository, `/`-separated) should be kept.
pub fn keep_path(role: RepoRole, rel_path: &str) -> bool {
    if !TEXT_EXTENSIONS.contains(&extension_of(rel_path).as_str()) {
        return false;
    }
    let segments: Vec<&str> = rel_path.split('/').collect();
    if segments.iter().any(|s| *s == ".." || *s == ".git") {
        return false;
    }
    match role_dirs(role) {
        None => true,
        Some(dirs) => segments.len() == 1 || dirs.contains(&segments[0].to_lowercase().as_str()),
    }
}

/// Extracts the tar stream into `dest` (created if missing); returns the bytes written.
pub fn extract_tar(reader: impl Read, dest: &Path, role: RepoRole) -> io::Result<u64> {
    fs::create_dir_all(dest)?;
    let mut total = 0;
    for entry in Archive::new(reader).entries()? {
        let mut entry = entry?;
        if entry.header().entry_type() != EntryType::Regular {
            continue;
        }
        let path = entry.path()?.to_string_lossy().replace('\\', "/");
        let size = entry.header().size()?;
        if path.is_empty()
            || path.starts_with('/')
            || !keep_path(role, &path)
            || size > MAX_FILE_BYTES
        {
            continue;
        }

        total += size;
        if total > MAX_TOTAL_BYTES {
            return Err(io::Error::other("archive exceeds the size cap"));
        }
        let target = dest.join(&path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = fs::File::create(&target)?;
        io::copy(&mut entry, &mut file)?;
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_text_files_only() {
        assert!(keep_path(RepoRole::Package, "ZZMI/d3dx.ini"));
        assert!(!keep_path(RepoRole::Package, "ZZMI/tex.dds"));
        assert!(!keep_path(RepoRole::Package, "a/../b.ini"));
        assert!(!keep_path(RepoRole::Package, ".git/config.ini"));
    }

    #[test]
    fn dll_roles_keep_the_extractor_inputs() {
        assert!(keep_path(RepoRole::Dll, "DirectX11/CommandList.cpp"));
        assert!(keep_path(RepoRole::Dll, "README.md"));
        assert!(!keep_path(RepoRole::Dll, "Libs/zlib/zlib.h"));
        assert!(!keep_path(RepoRole::Baseline, "docs/guide.md"));
    }
}
