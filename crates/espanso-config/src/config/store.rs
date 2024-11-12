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

use crate::error::NonFatalErrorSet;
use crate::matches::group::loader::yaml::YAMLImporter;

use super::{resolve::ConfigFile, ConfigStoreError};
use anyhow::{Context, Result};
use log::{debug, error};
use std::path::PathBuf;
use std::{collections::HashSet, path::Path};

pub struct ConfigStore {
    /// The `default.yml` file in the `config` directory.
    default: ConfigFile,
    /// All the other `.yml` files in the `config` directory.
    /// These files may specify one or more of `filter_title`, `filter_class`, `filter_exec`.
    /// We can think of these also as profiles.
    customs: Vec<ConfigFile>,
}

impl ConfigStore {
    pub fn default_config(&self) -> &ConfigFile {
        &self.default
    }

    /// Get the active configuration for the given app.
    ///
    /// This will return the *first* custom configuration that matches the app properties.
    pub fn active_config(&self, app: &super::AppProperties) -> &ConfigFile {
        // Find a custom config that matches or fallback to the default one
        for custom in &self.customs {
            if custom.is_match(app) {
                return &custom;
            }
        }
        &self.default
    }

    pub fn all_configs(&self) -> Vec<&ConfigFile> {
        let mut configs = vec![&self.default];

        for custom in &self.customs {
            configs.push(custom);
        }

        configs
    }

    // TODO: test
    pub fn get_all_match_file_paths(&self) -> HashSet<PathBuf> {
        let mut paths = HashSet::new();

        paths.extend(self.default_config().match_file_paths().iter().cloned());
        for custom in &self.customs {
            paths.extend(custom.match_file_paths().iter().cloned());
        }

        paths
    }

    pub fn load(config_dir: &Path) -> Result<(Self, Vec<NonFatalErrorSet>)> {
        if !config_dir.is_dir() {
            return Err(ConfigStoreError::InvalidConfigDir().into());
        }

        // First get the default.yml file
        let default_file = config_dir.join("default.yml");
        if !default_file.exists() || !default_file.is_file() {
            return Err(ConfigStoreError::MissingDefault().into());
        }

        let mut non_fatal_errors = Vec::new();

        let default = ConfigFile::load_from_path(&default_file, None)
            .context("failed to load default.yml configuration")?;
        debug!("loaded default config at path: {:?}", default_file);

        // Then the others
        let mut customs: Vec<ConfigFile> = Vec::new();
        for entry in std::fs::read_dir(config_dir).map_err(ConfigStoreError::IOError)? {
            let config_file = entry?.path();
            let Some(extension) = config_file.extension() else {
                continue;
            };

            // Additional config files are loaded best-effort
            if config_file.is_file()
                && config_file != default_file
                && YAMLImporter::is_supported(extension)
            {
                match ConfigFile::load_from_path(&config_file, Some(&default)) {
                    Ok(config) => {
                        customs.push(config);
                        debug!("loaded config at path: {:?}", config_file);
                    }
                    Err(err) => {
                        error!(
                            "unable to load config at path: {:?}, with error: {}",
                            config_file, err
                        );
                        non_fatal_errors.push(NonFatalErrorSet::single_error(&config_file, err));
                    }
                }
            }
        }

        Ok((Self { default, customs }, non_fatal_errors))
    }

    // pub fn from_configs(
    //   default: Arc<dyn Config>,
    //   customs: Vec<Arc<dyn Config>>,
    // ) -> DefaultConfigStore {
    //   Self { default, customs }
    // }
}

#[cfg(test)]
mod tests {
    use regex::Regex;

    use crate::config::parse::ParsedConfig;

    use super::*;

    pub fn new_mock(label: &'static str) -> ConfigFile {
        let label = label.to_owned();
        // let mut mock = MockConfig::new();
        // mock.expect_id().return_const(0);
        // mock.expect_label().return_const(label);
        // mock.expect_is_match().return_const(is_match);
        // mock
        ConfigFile {
            parsed: ParsedConfig {
                label: Some(label),
                ..Default::default()
            },
            id: 0,
            ..Default::default()
        }
    }

    #[test]
    fn config_store_selects_correctly() {
        let default = new_mock("default");
        let custom1 = new_mock("custom1");
        let mut custom2 = new_mock("custom2");
        custom2.filter_class = Some(Regex::new("foo").unwrap());

        let store = ConfigStore {
            default,
            customs: vec![custom1, custom2],
        };

        assert_eq!(store.default_config().label(), "default");
        assert_eq!(
            store
                .active_config(&crate::config::AppProperties {
                    title: None,
                    class: Some("foo"),
                    exec: None,
                })
                .label(),
            "custom2"
        );
    }

    #[test]
    fn config_store_active_fallback_to_default_if_no_match() {
        let default = new_mock("default");
        let custom1 = new_mock("custom1");
        let custom2 = new_mock("custom2");

        let store = ConfigStore {
            default,
            customs: vec![custom1, custom2],
        };

        assert_eq!(store.default_config().label(), "default");
        assert_eq!(
            store
                .active_config(&crate::config::AppProperties {
                    title: None,
                    class: None,
                    exec: None,
                })
                .label(),
            "default"
        );
    }
}
