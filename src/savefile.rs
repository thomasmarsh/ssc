//! Atomic disk saves: bounded autosave history and a separate explicit save.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub const AUTOSAVE_LIMIT: usize = 10;
const MANUAL: &str = "manual.ron";

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

fn is_autosave(name: &str) -> bool {
    name.strip_prefix("auto-")
        .and_then(|rest| rest.strip_suffix(".ron"))
        .is_some_and(|id| !id.is_empty() && id.bytes().all(|c| c.is_ascii_digit()))
}

fn is_save(name: &str) -> bool {
    is_autosave(name) || matches!(name, MANUAL | "manual.bak" | "run.ron" | "run.bak")
}

/// Newest first, including the previous single-slot files. Parsing belongs to the caller:
/// a damaged newest candidate must not hide the valid saves that precede it.
pub fn candidates(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry?;
        if entry.file_name().to_str().is_some_and(is_save) && entry.file_type()?.is_file() {
            paths.push(entry.path());
        }
    }
    paths.sort_by_cached_key(|path| {
        (
            std::cmp::Reverse(
                path.metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(SystemTime::UNIX_EPOCH),
            ),
            std::cmp::Reverse(path.clone()),
        )
    });
    Ok(paths)
}

fn atomic_write(path: &Path, text: &str) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    let mut file = std::fs::File::create(&tmp)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    std::fs::rename(tmp, path)
}

/// Writes a new autosave before pruning the oldest. An interrupted write leaves earlier
/// saves intact; temporary files are never load candidates.
pub fn write(dir: &Path, text: &str) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut autos = candidates(dir)?
        .into_iter()
        .filter(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(is_autosave)
        })
        .collect::<Vec<_>>();
    let max_id = autos
        .iter()
        .filter_map(|path| {
            path.file_stem()?
                .to_str()?
                .strip_prefix("auto-")?
                .parse::<u128>()
                .ok()
        })
        .max()
        .unwrap_or(0);
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_nanos()
        .max(
            max_id
                .checked_add(1)
                .ok_or_else(|| io::Error::other("autosave sequence exhausted"))?,
        );
    let path = dir.join(format!("auto-{stamp:039}.ron"));
    atomic_write(&path, text)?;
    autos.insert(0, path);
    for old in autos.into_iter().skip(AUTOSAVE_LIMIT) {
        std::fs::remove_file(old)?;
    }
    Ok(())
}

/// An explicit save is never replaced or pruned by autosaves. Keep its previous value for
/// recovery as well; copying the backup does not create a gap in the current save.
pub fn write_manual(dir: &Path, text: &str) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(MANUAL);
    if path.exists() {
        let previous = std::fs::read_to_string(&path)?;
        let modified = path.metadata()?.modified()?;
        let backup = dir.join("manual.bak");
        atomic_write(&backup, &previous)?;
        // A recovery copy retains the original save's place in CONTINUE chronology.
        std::fs::File::options()
            .write(true)
            .open(backup)?
            .set_times(std::fs::FileTimes::new().set_modified(modified))?;
    }
    atomic_write(&path, text)
}

/// Clears saves for NEW GAME. Unrelated files in the directory are left alone.
pub fn delete(dir: &Path) -> io::Result<()> {
    for path in candidates(dir)? {
        std::fs::remove_file(path)?;
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
    fn history_is_bounded_and_does_not_overwrite_an_explicit_save() {
        let dir = scratch("history");
        write_manual(&dir, "explicit").unwrap();
        for i in 0..AUTOSAVE_LIMIT + 5 {
            write(&dir, &i.to_string()).unwrap();
        }
        let paths = candidates(&dir).unwrap();
        assert_eq!(paths.len(), AUTOSAVE_LIMIT + 1);
        assert_eq!(std::fs::read_to_string(&paths[0]).unwrap(), "14");
        assert_eq!(
            std::fs::read_to_string(dir.join(MANUAL)).unwrap(),
            "explicit"
        );
        let texts = paths
            .iter()
            .map(|path| std::fs::read_to_string(path).unwrap())
            .collect::<Vec<_>>();
        assert!(!texts.contains(&"4".to_string()));
        assert!(texts.contains(&"5".to_string()));
        std::fs::write(dir.join("unrelated.txt"), "keep").unwrap();
        delete(&dir).unwrap();
        assert!(candidates(&dir).unwrap().is_empty());
        assert!(dir.join("unrelated.txt").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn explicit_save_replaces_atomically_and_keeps_previous_value() {
        let dir = scratch("manual");
        write_manual(&dir, "first").unwrap();
        write_manual(&dir, "second").unwrap();
        assert_eq!(std::fs::read_to_string(dir.join(MANUAL)).unwrap(), "second");
        assert_eq!(
            std::fs::read_to_string(dir.join("manual.bak")).unwrap(),
            "first"
        );
        std::fs::write(dir.join("auto-123.tmp"), "incomplete").unwrap();
        assert_eq!(candidates(&dir).unwrap().len(), 2);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
