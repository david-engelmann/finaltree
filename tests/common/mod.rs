//! The fixture machinery the test files share. A scratch
//! directory carrying the process id in its name and removed
//! when it falls, the pinned git environment every run
//! stands under, and the fixture repo of the Python repo
//! factory, a.txt as alpha and b.txt as beta committed as
//! initial.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A scratch directory that removes itself when it falls.
/// The area word stands in its name beside the process id,
/// one area per test file, so the scratch directories of
/// two test files never collide.
pub struct Scratch(pub PathBuf);

impl Scratch {
    pub fn new(area: &str, tag: &str) -> Scratch {
        let path =
            std::env::temp_dir().join(format!("finaltree-{area}-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("the scratch directory stands");
        Scratch(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The pinned environment every git and binary run stands
/// under, matching the recordings harness of slice 1.
pub fn pinned(command: &mut Command) -> &mut Command {
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Finaltree Tests")
        .env("GIT_AUTHOR_EMAIL", "tests@example.invalid")
        .env("GIT_COMMITTER_NAME", "Finaltree Tests")
        .env("GIT_COMMITTER_EMAIL", "tests@example.invalid")
        .env("GIT_AUTHOR_DATE", "2024-01-01T00:00:00+00:00")
        .env("GIT_COMMITTER_DATE", "2024-01-01T00:00:00+00:00")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_NAMESPACE")
        .env_remove("GIT_PREFIX")
}

pub fn git(root: &Path, args: &[&str]) {
    let output = pinned(&mut Command::new("git"))
        .args(args)
        .current_dir(root)
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?} stands in the fixture"
    );
}

/// Whether a git invocation in the fixture succeeds, for the
/// steps a case stands on a failing invocation. Not every
/// suite that shares this module calls it, so the dead code
/// warning stands allowed here.
#[allow(dead_code)]
pub fn git_succeeds(root: &Path, args: &[&str]) -> bool {
    let output = pinned(&mut Command::new("git"))
        .args(args)
        .current_dir(root)
        .output()
        .expect("git runs");
    output.status.success()
}

pub fn write_file(root: &Path, relative: &str, text: &str) -> PathBuf {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("the fixture folder stands");
    }
    fs::write(&path, text).expect("the fixture file is written");
    path
}

/// The fixture repo of the Python repo factory, a.txt as
/// alpha and b.txt as beta committed as initial.
pub fn make_factory_repo(root: &Path) {
    fs::create_dir_all(root).expect("the fixture root stands");
    git(root, &["init", "-q", "-b", "main", "."]);
    write_file(root, "a.txt", "alpha\n");
    write_file(root, "b.txt", "beta\n");
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "initial"]);
}
