/*
 * This file is part of espanso.
 *
 * Copyright (C) 2019-2021 Federico Terzi
 *
 * espanso is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * espanso is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with espanso.  If not, see <https://www.gnu.org/licenses/>.
 */

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use thiserror::Error;

pub mod config;
pub mod error;
pub mod matches;
mod util;

use config::ProfileStore;
use matches::{group::loader::yaml::YAMLImporter, store::MatchStore};

type LoadableConfig = (ProfileStore, MatchStore, Vec<error::NonFatalErrorSet>);

pub fn load(base_path: &Path) -> Result<LoadableConfig> {
    let config_dir = base_path.join("config");
    if !config_dir.exists() || !config_dir.is_dir() {
        return Err(ConfigError::MissingConfigDir().into());
    }

    let (profile_store, non_fatal_config_errors) = config::load_store(&config_dir)?;
    let root_paths: Vec<_> = profile_store
        .get_all_match_file_paths()
        .into_iter()
        .collect();

    let (match_store, file_map, non_fatal_match_errors) = MatchStore::load(&root_paths);

    let profile_store = ProfileStore::resolve_paths(profile_store, &file_map);

    let mut non_fatal_errors = Vec::new();
    non_fatal_errors.extend(non_fatal_config_errors);
    non_fatal_errors.extend(non_fatal_match_errors);

    Ok((profile_store, match_store, non_fatal_errors))
}

pub fn all_config_files(config_dir: &Path) -> Result<impl Iterator<Item = PathBuf>> {
    let iter = std::fs::read_dir(config_dir)
        .with_context(|| format!("Failed to read directory {:?}", config_dir))?
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let extension = path.extension()?;
            if path.is_file() && YAMLImporter::is_supported(extension) {
                Some(path)
            } else {
                None
            }
        });
    Ok(iter)
}

// pub fn load_legacy(
//     config_dir: &Path,
//     package_dir: &Path,
// ) -> Result<(Box<dyn ConfigStore>, Box<dyn MatchStore>)> {
//     legacy::load(config_dir, package_dir)
// }

// pub fn is_legacy_config(base_dir: &Path) -> bool {
//     base_dir.join("user").is_dir() && base_dir.join("default.yml").is_file()
// }

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("missing config directory")]
    MissingConfigDir(),
}

#[cfg(test)]
mod tests {
    use config::AppProperties;
    use shinran_test_helpers::use_test_directory;

    use super::*;
    // use config::AppProperties;

    #[test]
    fn load_works_correctly() {
        use_test_directory(|base, match_dir, config_dir| {
            let base_file = match_dir.join("base.yml");
            std::fs::write(
                base_file,
                r#"
      matches:
        - trigger: "hello"
          replace: "world"
      "#,
            )
            .unwrap();

            let another_file = match_dir.join("another.yml");
            std::fs::write(
                another_file,
                r#"
      imports:
        - "_sub.yml"

      matches:
        - trigger: "hello2"
          replace: "world2"
      "#,
            )
            .unwrap();

            let under_file = match_dir.join("_sub.yml");
            std::fs::write(
                under_file,
                r#"
      matches:
        - trigger: "hello3"
          replace: "world3"
      "#,
            )
            .unwrap();

            let config_file = config_dir.join("default.yml");
            std::fs::write(config_file, "").unwrap();

            let custom_config_file = config_dir.join("custom.yml");
            std::fs::write(
                custom_config_file,
                r#"
      filter_title: "Chrome"

      use_standard_includes: false
      includes: ["../match/another.yml"]
      "#,
            )
            .unwrap();

            let (config_store, match_store, errors) = load(base).unwrap();

            assert_eq!(errors.len(), 0);
            assert_eq!(config_store.default_profile.match_file_paths().len(), 2);
            assert_eq!(
                config_store
                    .active_config(&AppProperties {
                        title: Some("Google Chrome"),
                        class: None,
                        exec: None,
                    })
                    .match_file_paths()
                    .len(),
                1
            );

            assert_eq!(
                match_store
                    .collect_matches_and_global_vars(
                        config_store.default_profile.match_file_paths()
                    )
                    .trigger_matches
                    .len(),
                3
            );
            assert_eq!(
                match_store
                    .collect_matches_and_global_vars(
                        config_store
                            .active_config(&AppProperties {
                                title: Some("Chrome"),
                                class: None,
                                exec: None,
                            })
                            .match_file_paths()
                    )
                    .trigger_matches
                    .len(),
                2
            );
        });
    }

    #[test]
    fn load_non_fatal_errors() {
        use_test_directory(|base, match_dir, config_dir| {
            let base_file = match_dir.join("base.yml");
            std::fs::write(
                base_file,
                r#"
      matches:
        - "invalid"invalid
      "#,
            )
            .unwrap();

            let another_file = match_dir.join("another.yml");
            std::fs::write(
                another_file,
                r#"
      imports:
        - "_sub.yml"

      matches:
        - trigger: "hello2"
          replace: "world2"
      "#,
            )
            .unwrap();

            let under_file = match_dir.join("_sub.yml");
            std::fs::write(
                under_file,
                r#"
      matches:
        - trigger: "hello3"
          replace: "world3"invalid
      "#,
            )
            .unwrap();

            let config_file = config_dir.join("default.yml");
            std::fs::write(config_file, r"").unwrap();

            let custom_config_file = config_dir.join("custom.yml");
            std::fs::write(
                custom_config_file,
                r#"
      filter_title: "Chrome"
      "

      use_standard_includes: false
      includes: ["../match/another.yml"]
      "#,
            )
            .unwrap();

            let (config_store, match_store, errors) = load(base).unwrap();

            assert_eq!(errors.len(), 3);
            // It shouldn't have loaded the "config.yml" one because of the YAML error
            assert_eq!(config_store.len(), 1);
            // It shouldn't load "base.yml" and "_sub.yml" due to YAML errors
            assert_eq!(match_store.loaded_paths().len(), 1);
        });
    }

    #[test]
    fn load_non_fatal_match_errors() {
        use_test_directory(|base, match_dir, config_dir| {
            let base_file = match_dir.join("base.yml");
            std::fs::write(
                &base_file,
                r#"
      matches:
        - trigger: "hello"
          replace: "world"
        - trigger: "invalid because there is no action field"
      "#,
            )
            .unwrap();

            let config_file = config_dir.join("default.yml");
            std::fs::write(config_file, r"").unwrap();

            let (config_store, match_store, errors) = load(base).unwrap();

            assert_eq!(errors.len(), 1);
            assert_eq!(errors[0].file, base_file);
            assert_eq!(errors[0].errors.len(), 1);

            assert_eq!(
                match_store
                    .collect_matches_and_global_vars(
                        config_store.default_profile.match_file_paths()
                    )
                    .trigger_matches
                    .len(),
                1
            );
        });
    }

    #[test]
    fn load_fatal_errors() {
        use_test_directory(|base, match_dir, config_dir| {
            let base_file = match_dir.join("base.yml");
            std::fs::write(
                base_file,
                r"
      matches:
        - trigger: hello
          replace: world
      ",
            )
            .unwrap();

            let config_file = config_dir.join("default.yml");
            std::fs::write(
                config_file,
                r#"
      invalid

      "
      "#,
            )
            .unwrap();

            // A syntax error in the default.yml file cannot be handled gracefully
            assert!(load(base).is_err());
        });
    }

    #[test]
    fn load_without_valid_config_dir() {
        use_test_directory(|_, match_dir, _| {
            // To correcly load the configs, the "load" method looks for the "config" directory
            assert!(load(match_dir).is_err());
        });
    }
}
