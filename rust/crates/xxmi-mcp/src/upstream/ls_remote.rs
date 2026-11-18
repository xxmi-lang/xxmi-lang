//! Parses `git ls-remote --symref <url> HEAD 'refs/heads/*' 'refs/tags/*'`.

use std::collections::HashMap;

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

fn is_sha(text: &str) -> bool {
    text.len() == 40 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

pub fn parse_ls_remote(output: &str) -> Result<RemoteRefs, String> {
    let mut symref: Option<&str> = None;
    let mut head_sha: Option<&str> = None;
    let mut heads: HashMap<&str, &str> = HashMap::new();
    let mut tags: Vec<(&str, &str)> = Vec::new();
    let mut peeled: HashMap<&str, &str> = HashMap::new();

    for line in output.split('\n') {
        if let Some(target) = line
            .strip_prefix("ref: ")
            .and_then(|rest| rest.strip_suffix("\tHEAD"))
            && !target.contains(char::is_whitespace)
        {
            symref = Some(target);
            continue;
        }
        let mut parts = line.split('\t');
        let (sha, name) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        if !is_sha(sha) {
            continue;
        }
        if name == "HEAD" {
            head_sha = Some(sha);
        } else if let Some(branch) = name.strip_prefix("refs/heads/") {
            heads.insert(branch, sha);
        } else if let Some(tag) = name
            .strip_prefix("refs/tags/")
            .and_then(|rest| rest.strip_suffix("^{}"))
        {
            peeled.insert(tag, sha);
        } else if let Some(tag) = name.strip_prefix("refs/tags/") {
            tags.push((tag, sha));
        }
    }

    let default_branch = symref.map(|s| s.strip_prefix("refs/heads/").unwrap_or(s));
    let head = default_branch.and_then(|b| heads.get(b).copied().or(head_sha));
    let (Some(default_branch), Some(head)) = (default_branch, head) else {
        return Err("remote has no default branch (empty repository?)".to_string());
    };
    Ok(RemoteRefs {
        default_branch: default_branch.to_string(),
        head: head.to_string(),
        tags: tags
            .into_iter()
            .map(|(name, sha)| RemoteTag {
                name: name.to_string(),
                sha: peeled.get(name).unwrap_or(&sha).to_string(),
            })
            .collect(),
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
