//! Parses `git ls-remote --symref <url> HEAD 'refs/heads/*' 'refs/tags/*'`.

use std::collections::HashMap;

use crate::repos::is_sha;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteTag {
    pub name: String,
    /// The commit the tag points at (peeled for annotated tags).
    pub sha: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteRefs {
    pub default_branch: String,
    /// The commit the default branch points at.
    pub head: String,
    pub tags: Vec<RemoteTag>,
}

/// The refs of the output, before they are matched up.
#[derive(Default)]
struct Scan<'a> {
    symref: Option<&'a str>,
    head_sha: Option<&'a str>,
    heads: HashMap<&'a str, &'a str>,
    tags: Vec<(&'a str, &'a str)>,
    peeled: HashMap<&'a str, &'a str>,
}

fn symref_target(line: &str) -> Option<&str> {
    line.strip_prefix("ref: ")
        .and_then(|rest| rest.strip_suffix("\tHEAD"))
        .filter(|target| !target.contains(char::is_whitespace))
}

impl<'a> Scan<'a> {
    fn read(output: &'a str) -> Self {
        let mut scan = Self::default();
        for line in output.split('\n') {
            scan.read_line(line);
        }
        scan
    }

    fn read_line(&mut self, line: &'a str) {
        if let Some(target) = symref_target(line) {
            self.symref = Some(target);
            return;
        }

        let mut parts = line.split('\t');
        let (sha, name) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        if !is_sha(sha) {
            return;
        }
        if name == "HEAD" {
            self.head_sha = Some(sha);
        } else if let Some(branch) = name.strip_prefix("refs/heads/") {
            self.heads.insert(branch, sha);
        } else if let Some(tag) = name
            .strip_prefix("refs/tags/")
            .and_then(|rest| rest.strip_suffix("^{}"))
        {
            self.peeled.insert(tag, sha);
        } else if let Some(tag) = name.strip_prefix("refs/tags/") {
            self.tags.push((tag, sha));
        }
    }

    fn default_branch(&self) -> Option<(&'a str, &'a str)> {
        let branch = self
            .symref
            .map(|s| s.strip_prefix("refs/heads/").unwrap_or(s))?;
        let head = self.heads.get(branch).copied().or(self.head_sha)?;
        Some((branch, head))
    }

    fn remote_tags(&self) -> Vec<RemoteTag> {
        self.tags
            .iter()
            .map(|(name, sha)| RemoteTag {
                name: name.to_string(),
                sha: self.peeled.get(name).unwrap_or(sha).to_string(),
            })
            .collect()
    }
}

pub fn parse_ls_remote(output: &str) -> Result<RemoteRefs, String> {
    let scan = Scan::read(output);
    let Some((default_branch, head)) = scan.default_branch() else {
        return Err("remote has no default branch (empty repository?)".to_string());
    };

    Ok(RemoteRefs {
        default_branch: default_branch.to_string(),
        head: head.to_string(),
        tags: scan.remote_tags(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const C: &str = "cccccccccccccccccccccccccccccccccccccccc";

    #[test]
    fn reads_default_branch_and_peels_annotated_tags() {
        let output = format!(
            "ref: refs/heads/main\tHEAD\n{A}\tHEAD\n{A}\trefs/heads/main\n{B}\trefs/tags/v1\n{C}\trefs/tags/v1^{{}}\n{B}\trefs/tags/v2\n"
        );
        let refs = parse_ls_remote(&output).unwrap();
        assert_eq!(refs.default_branch, "main");
        assert_eq!(refs.head, A);
        assert_eq!(
            refs.tags,
            vec![
                RemoteTag {
                    name: "v1".into(),
                    sha: C.into()
                },
                RemoteTag {
                    name: "v2".into(),
                    sha: B.into()
                },
            ]
        );
    }

    #[test]
    fn an_empty_repository_has_no_default_branch() {
        assert!(parse_ls_remote("").is_err());
    }
}
