//! Where a save lives on disk and how it is written. Std only: no Bevy, no game rules (the
//! state and its format are `simulation::save`). One slot for now; the continue / new / delete
//! menu is workstream 12 slice 4.

use std::io;
use std::path::{Path, PathBuf};

const FILE: &str = "run.ron";
const BACKUP: &str = "run.bak";

/// The save directory: `SSC_SAVE_DIR` when set, else the platform's per-user data folder.
pub fn default_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("SSC_SAVE_DIR") {
        return PathBuf::from(dir);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/ssc")
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or(home)
            .join("ssc")
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share"))
            .join("ssc")
    }
}

/// Writes the save atomically: the text goes to a temporary file that replaces the slot in one
/// rename, and the previous save is kept as the backup, so a crash mid-write loses nothing.
pub fn write(dir: &Path, text: &str) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let slot = dir.join(FILE);
    let tmp = dir.join(format!("{FILE}.tmp"));
    std::fs::write(&tmp, text)?;
    if slot.exists() {
        std::fs::rename(&slot, dir.join(BACKUP))?;
    }
    std::fs::rename(&tmp, &slot)
}

/// The saved text, or None when there is no save. Falls back to the backup when the slot is
/// missing (a crash between the two renames).
pub fn read(dir: &Path) -> io::Result<Option<String>> {
    for name in [FILE, BACKUP] {
        match std::fs::read_to_string(dir.join(name)) {
            Ok(text) => return Ok(Some(text)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(None)
}

/// Removes the save and its backup.
pub fn delete(dir: &Path) -> io::Result<()> {
    for name in [FILE, BACKUP] {
        match std::fs::remove_file(dir.join(name)) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ssc-savefile-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn write_read_replace_and_delete() {
        let dir = scratch("rw");
        assert_eq!(read(&dir).unwrap(), None);
        write(&dir, "one").unwrap();
        assert_eq!(read(&dir).unwrap().as_deref(), Some("one"));
        write(&dir, "two").unwrap();
        assert_eq!(read(&dir).unwrap().as_deref(), Some("two"));
        assert_eq!(std::fs::read_to_string(dir.join(BACKUP)).unwrap(), "one");
        // A crash between the renames leaves only the backup: it is still found.
        std::fs::remove_file(dir.join(FILE)).unwrap();
        assert_eq!(read(&dir).unwrap().as_deref(), Some("one"));
        delete(&dir).unwrap();
        assert_eq!(read(&dir).unwrap(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
