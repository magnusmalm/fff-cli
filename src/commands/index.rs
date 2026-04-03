use crate::error::Result;
use crate::index;
use std::path::Path;

pub fn run(project_root: &Path, force: bool) -> Result<()> {
    if !force {
        let dir = index::fff_dir(project_root);
        let head = index::staleness::current_git_head(project_root);
        match index::staleness::check_staleness(&dir, &head) {
            index::staleness::Staleness::Fresh => {
                eprintln!("Index is up to date. Use --force to rebuild.");
                return Ok(());
            }
            _ => {}
        }
    }

    let start = std::time::Instant::now();
    index::build_and_write(project_root)?;
    eprintln!("Completed in {:.2}s", start.elapsed().as_secs_f64());
    Ok(())
}
