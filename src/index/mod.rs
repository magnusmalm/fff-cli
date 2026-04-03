pub mod format;
pub mod staleness;

use crate::error::{CliError, Result};
use format::{
    IndexManifest, read_bigram_index, read_file_list, write_bigram_index, write_file_list,
    write_manifest,
};
use fff::file_picker::{FFFMode, FilePicker, FilePickerOptions, build_bigram_index};
use fff::frecency::FrecencyTracker;
use fff::BigramFilter;
use fff::types::{ContentCacheBudget, FileItem};
use staleness::current_git_head;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Everything loaded from the on-disk index.
pub struct LoadedIndex {
    pub files: Vec<FileItem>,
    pub bigram: Option<BigramFilter>,
}

/// Resolve the frecency database path.
///
/// Priority: explicit override > nvim shared cache > `.fff/frecency` in project.
pub fn resolve_frecency_db(
    explicit: Option<&str>,
    project_root: &Path,
) -> Option<String> {
    if let Some(path) = explicit {
        return Some(path.to_string());
    }

    // Share with neovim plugin if its cache directory exists.
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    if !home.is_empty() {
        let nvim_cache = format!("{}/.cache/nvim/fff_nvim", home);
        if Path::new(&nvim_cache).exists() {
            return Some(nvim_cache);
        }
    }

    // Fall back to project-local DB.
    let local = fff_dir(project_root).join("frecency");
    if local.exists() {
        return Some(local.to_string_lossy().into_owned());
    }

    None
}

/// Apply frecency scores to loaded files if a database is available.
fn apply_frecency(files: &mut [FileItem], frecency_db: Option<&str>) {
    let db_path = match frecency_db {
        Some(p) if Path::new(p).exists() => p,
        _ => return,
    };

    let tracker = match FrecencyTracker::new(db_path, true) {
        Ok(t) => t,
        Err(e) => {
            tracing::debug!("frecency db unavailable: {e}");
            return;
        }
    };

    let mut scored = 0usize;
    for file in files.iter_mut() {
        if file.update_frecency_scores(&tracker, FFFMode::default()).is_ok()
            && file.total_frecency_score != 0
        {
            scored += 1;
        }
    }

    if scored > 0 {
        tracing::debug!("{scored} files have frecency scores");
    }
}

/// Discover the git repository and project root in a single call.
/// Returns (project_root, Option<Repository>).
pub fn resolve_project_root(start_dir: &Path) -> (PathBuf, Option<git2::Repository>) {
    match git2::Repository::discover(start_dir) {
        Ok(repo) => {
            let root = repo
                .workdir()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| start_dir.to_path_buf());
            (root, Some(repo))
        }
        Err(_) => (start_dir.to_path_buf(), None),
    }
}

/// Return the `.fff/` directory for a given project root.
pub fn fff_dir(project_root: &Path) -> PathBuf {
    project_root.join(".fff")
}

/// Build the index from scratch and write it to disk.
pub fn build_and_write(project_root: &Path) -> Result<()> {
    let dir = fff_dir(project_root);
    std::fs::create_dir_all(&dir)?;

    eprintln!("Indexing {}...", project_root.display());

    // Scan files synchronously using fff-core.
    let mut picker = FilePicker::new(FilePickerOptions {
        base_path: project_root.to_string_lossy().into_owned(),
        warmup_mmap_cache: false,
        watch: false,
        ..Default::default()
    })?;
    picker.collect_files()?;

    let files = picker.get_files();
    let file_count = files.len();
    eprintln!("Scanned {} files", file_count);

    // Write file list.
    write_file_list(&dir.join("files.bin"), files)?;

    // Build and write bigram index.
    let budget = ContentCacheBudget::new_for_repo(file_count);
    let (bigram_index, _binary_indices) = build_bigram_index(files, &budget);

    write_bigram_index(&dir.join("bigram.bin"), &bigram_index)?;
    if let Some(skip) = bigram_index.skip_index() {
        write_bigram_index(&dir.join("bigram_skip.bin"), skip)?;
    }

    // Write manifest.
    let head = current_git_head(project_root);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let hash = blake3::hash(project_root.to_string_lossy().as_bytes());
    write_manifest(
        &dir.join("manifest.bin"),
        &IndexManifest {
            file_count: file_count as u32,
            base_path_hash: *hash.as_bytes(),
            git_head: head,
            created_at: now,
        },
    )?;

    eprintln!(
        "Index written to {} ({} files, {} bigram columns)",
        dir.display(),
        file_count,
        bigram_index.columns_used(),
    );

    // Hint user to gitignore .fff/
    let gitignore = project_root.join(".gitignore");
    if gitignore.exists() {
        let content = std::fs::read_to_string(&gitignore).unwrap_or_default();
        if !content.lines().any(|l| l.trim() == ".fff/" || l.trim() == ".fff") {
            eprintln!("Hint: add `.fff/` to your .gitignore");
        }
    }

    Ok(())
}

/// Load the index from disk. Returns an error if the index doesn't exist.
pub fn load(project_root: &Path, frecency_db: Option<&str>) -> Result<LoadedIndex> {
    let dir = fff_dir(project_root);
    let files_path = dir.join("files.bin");
    let bigram_path = dir.join("bigram.bin");
    let skip_path = dir.join("bigram_skip.bin");

    if !files_path.exists() {
        return Err(CliError::NoIndex(dir));
    }

    let mut files = read_file_list(&files_path, project_root)?;
    apply_frecency(&mut files, frecency_db);

    let bigram = if bigram_path.exists() {
        let mut idx = read_bigram_index(&bigram_path)?;
        if skip_path.exists() {
            let skip = read_bigram_index(&skip_path)?;
            idx.set_skip_index(skip);
        }
        Some(idx)
    } else {
        None
    };

    Ok(LoadedIndex {
        files,
        bigram,
    })
}

/// Ensure an index exists, building it if necessary. Returns the loaded index.
/// Pass the git repo if already discovered (avoids a second `Repository::discover`).
pub fn ensure_index(
    project_root: &Path,
    repo: Option<&git2::Repository>,
    frecency_db: Option<&str>,
) -> Result<LoadedIndex> {
    let dir = fff_dir(project_root);
    let head = staleness::head_from_repo(repo);

    match staleness::check_staleness(&dir, &head) {
        staleness::Staleness::Fresh => {}
        staleness::Staleness::Stale(reason) => {
            eprintln!("Warning: {reason} — using stale index, run `fff index` to refresh");
        }
        staleness::Staleness::Missing => {
            build_and_write(project_root)?;
        }
    }

    load(project_root, frecency_db)
}
