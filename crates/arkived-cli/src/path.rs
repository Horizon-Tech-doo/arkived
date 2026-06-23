//! Parsing and classification of path arguments.
//!
//! Remote paths are `container[/blob...]`. For `cp`, an argument may be either a
//! local filesystem path or a remote path; [`classify_arg`] decides which.

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

/// A parsed remote path: a container and an optional blob name / prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemotePath {
    /// Container name.
    pub container: String,
    /// Blob name or prefix (everything after the first `/`), if any.
    pub blob: Option<String>,
}

/// Parse `container` or `container/blob/path` into a [`RemotePath`].
pub fn parse_remote(s: &str) -> Result<RemotePath> {
    let trimmed = s.trim().trim_start_matches('/');
    if trimmed.is_empty() {
        bail!("expected a remote path like 'container' or 'container/blob'");
    }
    let (container, rest) = match trimmed.split_once('/') {
        Some((c, r)) => (c, Some(r)),
        None => (trimmed, None),
    };
    if container.is_empty() {
        bail!("container name is empty in '{s}'");
    }
    let blob = rest.map(str::to_string).filter(|r| !r.is_empty());
    Ok(RemotePath {
        container: container.to_string(),
        blob,
    })
}

/// A `cp` argument resolved to either a local file/dir or a remote path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CpArg {
    /// A local filesystem path.
    Local(PathBuf),
    /// A remote `container[/blob]` path.
    Remote(RemotePath),
}

/// Classify a `cp` argument as local or remote.
///
/// An argument is local if it carries an explicit filesystem cue (`./`, `../`,
/// a leading slash/backslash, `~`, or a Windows drive prefix) or already exists
/// on disk. Otherwise it is treated as a remote `container/blob` path.
pub fn classify_arg(s: &str) -> Result<CpArg> {
    if looks_local(s) {
        Ok(CpArg::Local(PathBuf::from(s)))
    } else {
        Ok(CpArg::Remote(parse_remote(s)?))
    }
}

fn looks_local(s: &str) -> bool {
    s.starts_with("./")
        || s.starts_with("../")
        || s.starts_with(".\\")
        || s.starts_with("..\\")
        || s.starts_with('/')
        || s.starts_with('\\')
        || s.starts_with('~')
        || has_windows_drive_prefix(s)
        || Path::new(s).exists()
}

/// `C:\` or `C:/` style absolute Windows path prefix.
fn has_windows_drive_prefix(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_remote_container_only() {
        let p = parse_remote("mycontainer").unwrap();
        assert_eq!(p.container, "mycontainer");
        assert_eq!(p.blob, None);
    }

    #[test]
    fn parse_remote_container_and_blob() {
        let p = parse_remote("mycontainer/reports/q1.pdf").unwrap();
        assert_eq!(p.container, "mycontainer");
        assert_eq!(p.blob.as_deref(), Some("reports/q1.pdf"));
    }

    #[test]
    fn parse_remote_strips_leading_slash_and_trailing_marker() {
        let p = parse_remote("/mycontainer/").unwrap();
        assert_eq!(p.container, "mycontainer");
        assert_eq!(p.blob, None);
    }

    #[test]
    fn parse_remote_rejects_empty() {
        assert!(parse_remote("").is_err());
        assert!(parse_remote("/").is_err());
    }

    #[test]
    fn classify_dot_slash_is_local() {
        assert_eq!(
            classify_arg("./report.pdf").unwrap(),
            CpArg::Local(PathBuf::from("./report.pdf"))
        );
    }

    #[test]
    fn classify_windows_drive_is_local() {
        assert_eq!(
            classify_arg("C:\\data\\file.txt").unwrap(),
            CpArg::Local(PathBuf::from("C:\\data\\file.txt"))
        );
    }

    #[test]
    fn classify_bare_container_blob_is_remote() {
        // A name that does not exist on disk and has no local cue is remote.
        assert_eq!(
            classify_arg("mycontainer/reports/q1.pdf").unwrap(),
            CpArg::Remote(RemotePath {
                container: "mycontainer".into(),
                blob: Some("reports/q1.pdf".into()),
            })
        );
    }
}
