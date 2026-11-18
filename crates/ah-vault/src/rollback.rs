//! Best-effort cleanup for a command that failed after writing into a shared-home checkout.
//!
//! `git checkout -f` restores tracked files but leaves untracked ones, so anything a command
//! staged as a new file must be removed explicitly when the operation is abandoned.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

thread_local! {
    static CREATED: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
}

/// Record a file this operation created, so it can be removed on rollback.
pub fn note_created(path: &Path) {
    CREATED.with(|created| created.borrow_mut().push(path.to_path_buf()));
}

/// Remove every recorded file, plus directories it left empty, and forget the record.
pub fn remove_created(root: &Path, last: &Path) {
    let mut recorded = CREATED.with(|created| created.borrow_mut().drain(..).collect::<Vec<_>>());
    if !recorded.iter().any(|path| path == last) {
        recorded.push(last.to_path_buf());
    }
    for path in &recorded {
        let _ = std::fs::remove_file(path);
        // Remove now-empty parents up to (but never including) the checkout root.
        let mut parent = path.parent();
        while let Some(dir) = parent {
            if dir == root || !dir.starts_with(root) {
                break;
            }
            if std::fs::remove_dir(dir).is_err() {
                break;
            }
            parent = dir.parent();
        }
    }
}
