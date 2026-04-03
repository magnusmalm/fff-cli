//! Index staleness detection.

use crate::index::format::read_manifest;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Maximum age in seconds before the index is considered stale.
const MAX_AGE_SECS: u64 = 30 * 60; // 30 minutes

pub enum Staleness {
    /// Index is fresh enough to use without warning.
    Fresh,
    /// Index exists but is stale (reason provided).
    Stale(String),
    /// No index found.
    Missing,
}

/// Check staleness of the index at `fff_dir`.
pub fn check_staleness(fff_dir: &Path, current_head: &[u8; 20]) -> Staleness {
    let manifest_path = fff_dir.join("manifest.bin");
    let manifest = match read_manifest(&manifest_path) {
        Ok(m) => m,
        Err(_) => return Staleness::Missing,
    };

    // Check git HEAD
    if manifest.git_head != [0u8; 20] && *current_head != [0u8; 20] {
        if manifest.git_head != *current_head {
            return Staleness::Stale("git HEAD changed since last index".into());
        }
    }

    // Check age
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if now.saturating_sub(manifest.created_at) > MAX_AGE_SECS {
        return Staleness::Stale(format!(
            "index is older than {} minutes",
            MAX_AGE_SECS / 60
        ));
    }

    Staleness::Fresh
}

/// Extract HEAD from an already-discovered repository, or return zeros.
pub fn head_from_repo(repo: Option<&git2::Repository>) -> [u8; 20] {
    let Some(repo) = repo else {
        return [0u8; 20];
    };
    match repo.head() {
        Ok(reference) => {
            let oid = reference.target().unwrap_or_else(|| {
                reference
                    .peel_to_commit()
                    .map(|c| c.id())
                    .unwrap_or_else(|_| git2::Oid::zero())
            });
            let raw = oid.as_bytes();
            let mut out = [0u8; 20];
            out.copy_from_slice(raw);
            out
        }
        Err(_) => [0u8; 20],
    }
}

/// Read the current git HEAD as raw bytes, or all zeros if not in a git repo.
pub fn current_git_head(base_path: &Path) -> [u8; 20] {
    match git2::Repository::discover(base_path) {
        Ok(repo) => head_from_repo(Some(&repo)),
        Err(_) => [0u8; 20],
    }
}
