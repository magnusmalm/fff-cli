use crate::error::Result;
use crate::index;
use fff::file_picker::{FilePicker, FilePickerOptions};
use fff::frecency::FrecencyTracker;
use fff::{SharedFrecency, SharedPicker};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub fn run(project_root: &Path) -> Result<()> {
    eprintln!("Watching {} for changes...", project_root.display());
    eprintln!("Press Ctrl-C to stop.");

    let shared_picker = SharedPicker::default();
    let shared_frecency = SharedFrecency::default();

    // Try to init frecency DB in .fff/
    let fff_dir = index::fff_dir(project_root);
    std::fs::create_dir_all(&fff_dir)?;
    let frecency_path = fff_dir.join("frecency");
    if let Ok(tracker) = FrecencyTracker::new(&frecency_path, false) {
        let _ = shared_frecency.init(tracker);
    }

    FilePicker::new_with_shared_state(
        shared_picker.clone(),
        shared_frecency.clone(),
        FilePickerOptions {
            base_path: project_root.to_string_lossy().into_owned(),
            warmup_mmap_cache: false,
            watch: true,
            ..Default::default()
        },
    )?;

    // Wait for initial scan.
    eprintln!("Initial scan...");
    shared_picker.wait_for_scan(Duration::from_secs(120));

    // Write the initial index to disk.
    {
        let guard = shared_picker.read()?;
        if let Some(ref picker) = *guard {
            let files = picker.get_files();
            eprintln!("Scanned {} files, writing index...", files.len());
            index::build_and_write(project_root)?;
        }
    }

    eprintln!("Watching for changes. Index at {}", fff_dir.display());

    // Block until Ctrl-C.
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .expect("failed to set Ctrl-C handler");

    while running.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_secs(1));
    }

    // Cleanup: write final index state.
    eprintln!("\nShutting down, writing final index...");
    index::build_and_write(project_root)?;

    if let Ok(mut guard) = shared_picker.write() {
        if let Some(ref mut picker) = *guard {
            picker.stop_background_monitor();
        }
    }

    Ok(())
}
