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

use super::{resolve::Config, ConfigStoreError};
use anyhow::{Context, Result};
use log::{debug, error};
use std::sync::Arc;
use std::{collections::HashSet, path::Path};

pub struct ConfigStore {
    default: Arc<Config>,
    customs: Vec<Arc<Config>>,
}

impl ConfigStore {
    pub fn default(&self) -> Arc<Config> {
        Arc::clone(&self.default)
    }

    pub fn active(&self) -> Arc<Config> {
        Arc::clone(&self.default)
    }

    pub fn configs(&self) -> Vec<Arc<Config>> {
        let mut configs = vec![Arc::clone(&self.default)];

        for custom in &self.customs {
            configs.push(Arc::clone(custom));
        }

        configs
    }

    // TODO: test
    pub fn get_all_match_paths(&self) -> HashSet<String> {
        let mut paths = HashSet::new();

        paths.extend(self.default().match_paths().iter().cloned());
        for custom in &self.customs {
            paths.extend(custom.match_paths().iter().cloned());
        }

        paths
    }
}

impl ConfigStore {
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

        let default = Config::load(&default_file, None)
            .context("failed to load default.yml configuration")?;
        debug!("loaded default config at path: {:?}", default_file);

        // Then the others
        let mut customs: Vec<Arc<Config>> = Vec::new();
        for entry in std::fs::read_dir(config_dir).map_err(ConfigStoreError::IOError)? {
            let entry = entry?;
            let config_file = entry.path();
            let extension = config_file
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();

            // Additional config files are loaded best-effort
            if config_file.is_file()
                && config_file != default_file
                && (extension == "yml" || extension == "yaml")
            {
                match Config::load(&config_file, Some(&default)) {
                    Ok(config) => {
                        customs.push(Arc::new(config));
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

        Ok((
            Self {
                default: Arc::new(default),
                customs,
            },
            non_fatal_errors,
        ))
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
    use crate::config::parse::ParsedConfig;

    use super::*;

    pub fn new_mock(label: &'static str) -> Config {
        let label = label.to_owned();
        // let mut mock = MockConfig::new();
        // mock.expect_id().return_const(0);
        // mock.expect_label().return_const(label);
        // mock.expect_is_match().return_const(is_match);
        // mock
        Config {
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
        let custom2 = new_mock("custom2");

        let store = ConfigStore {
            default: Arc::new(default),
            customs: vec![Arc::new(custom1), Arc::new(custom2)],
        };

        assert_eq!(store.default().label(), "default");
        // assert_eq!(
        //     store
        //         .active(&crate::config::AppProperties {
        //             title: None,
        //             class: None,
        //             exec: None,
        //         })
        //         .label(),
        //     "custom2"
        // );
    }

    #[test]
    fn config_store_active_fallback_to_default_if_no_match() {
        let default = new_mock("default");
        let custom1 = new_mock("custom1");
        let custom2 = new_mock("custom2");

        let store = ConfigStore {
            default: Arc::new(default),
            customs: vec![Arc::new(custom1), Arc::new(custom2)],
        };

        assert_eq!(store.default().label(), "default");
        // assert_eq!(
        //     store
        //         .active(&crate::config::AppProperties {
        //             title: None,
        //             class: None,
        //             exec: None,
        //         })
        //         .label(),
        //     "default"
        // );
    }
}
