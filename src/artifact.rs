//! Atomic, private output artifacts. This module never executes a file.
use crate::{Error, Result};
use std::{fs::File, io::Write, path::Path};

/// Reject input/output aliases, including hard links on Unix.
pub fn reject_alias(input: &Path, output: &Path) -> Result<()> {
    let a = std::fs::canonicalize(input).map_err(|e| Error::io("resolve input", input, e))?;
    if let Ok(b) = std::fs::canonicalize(output) {
        let mut same = a == b;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let left = std::fs::metadata(&a).map_err(|e| Error::io("inspect input", input, e))?;
            let right =
                std::fs::metadata(&b).map_err(|e| Error::io("inspect output", output, e))?;
            same |= left.dev() == right.dev() && left.ino() == right.ino();
        }
        if same {
            return Err(Error::Invalid(
                "input and output refer to the same file; choose a separate output path".into(),
            ));
        }
    }
    Ok(())
}

/// Write an entire artifact in its destination directory, without partial output.
pub fn write(path: &Path, bytes: &[u8], replace: bool, executable: bool) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::Builder::new()
        .prefix(".raskin-")
        .tempfile_in(parent)
        .map_err(|e| Error::io("create temporary output in", parent, e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(if executable {
                0o700
            } else {
                0o600
            }))
            .map_err(|e| Error::io("set output permissions", path, e))?;
    }
    #[cfg(not(unix))]
    let _ = executable;
    temporary
        .write_all(bytes)
        .map_err(|e| Error::io("write output", path, e))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|e| Error::io("flush output", path, e))?;
    let persisted = if replace {
        temporary.persist(path)
    } else {
        temporary.persist_noclobber(path)
    };
    persisted.map_err(|e| {
        Error::io(
            "save output (use --force to replace an existing file)",
            path,
            e.error,
        )
    })?;
    #[cfg(unix)]
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|e| Error::io("sync output directory", parent, e))?;
    Ok(())
}
