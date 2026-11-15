//! Finding FFmpeg. An explicit `--ffmpeg` always wins; otherwise a copy that
//! was installed next to `tvmaster` (the Windows installer puts one there)
//! beats whatever is on `PATH`, because it is the one the installer tested.

use std::path::{Path, PathBuf};

/// `given` is what the user passed (or the default, which is just `name`).
pub fn resolve(given: PathBuf, name: &str) -> PathBuf {
    if given != Path::new(name) {
        return given;
    }
    match std::env::current_exe() {
        Ok(exe) => beside(exe.parent().unwrap_or(Path::new(".")), name).unwrap_or(given),
        Err(_) => given,
    }
}

/// `name` in `dir`, or in `dir/ffmpeg/bin`, if it exists there.
pub fn beside(dir: &Path, name: &str) -> Option<PathBuf> {
    let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    [dir.join(&file), dir.join("ffmpeg").join("bin").join(&file)]
        .into_iter()
        .find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("tvmaster-tools-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn finds_a_copy_beside_the_binary_or_in_its_ffmpeg_folder() {
        let suffix = std::env::consts::EXE_SUFFIX;
        let d = tmp("sub");
        assert_eq!(beside(&d, "ffmpeg"), None);
        let bin = d.join("ffmpeg").join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let in_sub = bin.join(format!("ffmpeg{suffix}"));
        std::fs::write(&in_sub, b"x").unwrap();
        assert_eq!(beside(&d, "ffmpeg"), Some(in_sub));
        let _ = std::fs::remove_dir_all(&d);

        // a copy right next to the binary
        let d = tmp("direct");
        let direct = d.join(format!("ffmpeg{suffix}"));
        std::fs::write(&direct, b"x").unwrap();
        assert_eq!(beside(&d, "ffmpeg"), Some(direct));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn an_explicit_path_is_never_replaced() {
        let given = PathBuf::from("/opt/custom/ffmpeg");
        assert_eq!(resolve(given.clone(), "ffmpeg"), given);
    }

    #[test]
    fn the_default_stays_a_bare_name_when_nothing_is_beside_the_binary() {
        // The test binary's folder has no ffmpeg in it.
        assert_eq!(
            resolve(PathBuf::from("ffmpeg"), "ffmpeg"),
            PathBuf::from("ffmpeg")
        );
    }
}
