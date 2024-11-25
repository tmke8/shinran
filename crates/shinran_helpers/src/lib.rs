use std::{fs::create_dir_all, path::Path};
use tempdir::TempDir;

/// Create a temporary directory for testing purposes and call the callback with the path to the
/// temporary directory, the path to the match directory, and the path to the config directory.
pub fn use_test_directory(callback: impl FnOnce(&Path, &Path, &Path)) {
    let temp_dir = TempDir::new("tempconfig").unwrap();
    let temp_dir = temp_dir.path();
    let match_dir = temp_dir.join("match");
    create_dir_all(&match_dir).unwrap();

    let config_dir = temp_dir.join("config");
    create_dir_all(&config_dir).unwrap();

    callback(
        &dunce::canonicalize(temp_dir).unwrap(),
        &dunce::canonicalize(match_dir).unwrap(),
        &dunce::canonicalize(config_dir).unwrap(),
    );
}
