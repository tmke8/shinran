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
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use crate::error::NonFatalErrorSet;
use crate::matches::group::loader::yaml::YAMLImporter;
use crate::{config::resolve::LoadedProfileFile, matches::group::MatchFileRef};

use super::{resolve::ArchivedProfileFile, ConfigStoreError, ProfileFile};
use anyhow::{Context, Result};
use log::{debug, error};
use rkyv::{Archive, Deserialize, Serialize};

#[derive(Archive, Serialize, Deserialize)]
#[archive(check_bytes)]
pub struct ProfileStore {
    pub default_profile: ProfileFile,
    custom_profiles: Box<[ProfileFile]>,
}

impl ProfileStore {
    pub(crate) fn resolve_paths(
        loaded: LoadedProfileStore,
        file_map: &HashMap<PathBuf, MatchFileRef>,
    ) -> Self {
        let default_profile = ProfileFile::from_loaded_profile(loaded.default_profile, file_map);

        let custom_profiles = loaded
            .custom_profiles
            .into_iter()
            .map(|loaded| ProfileFile::from_loaded_profile(loaded, file_map))
            .collect::<Vec<_>>();
        ProfileStore {
            default_profile,
            custom_profiles: custom_profiles.into_boxed_slice(),
        }
    }

    /// Get the active configuration for the given app.
    ///
    /// This will return the *first* custom configuration that matches the app properties.
    pub fn active_config(&self, app: &super::AppProperties) -> &ProfileFile {
        // Find a custom config that matches or fallback to the default one
        for custom in self.custom_profiles.iter() {
            if custom.filter.is_match(app) {
                return custom;
            }
        }
        &self.default_profile
    }

    pub fn len(&self) -> usize {
        self.custom_profiles.len() + 1
    }
}

impl ArchivedProfileStore {
    pub fn get_source_paths(&self) -> impl Iterator<Item = &Path> {
        self.get_parsed_configs()
            .map(|p| Path::new(p.source_path.as_str()))
    }

    pub fn get_parsed_configs(&self) -> impl Iterator<Item = &ArchivedProfileFile> {
        std::iter::once(&self.default_profile).chain(self.custom_profiles.iter())
    }
}

pub(crate) struct LoadedProfileStore {
    default_profile: LoadedProfileFile,
    custom_profiles: Vec<LoadedProfileFile>,
}

impl LoadedProfileStore {
    // TODO: test
    pub fn get_all_match_file_paths(&self) -> HashSet<PathBuf> {
        let mut paths = HashSet::new();

        paths.extend(self.default_profile.match_file_paths.iter().cloned());

        for profile in &self.custom_profiles {
            paths.extend(profile.match_file_paths.iter().cloned());
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

        debug!("loading default config at path: {:?}", default_file);
        let default_profile = LoadedProfileFile::load_from_path(&default_file, None)
            .context("failed to load default.yml configuration")?;

        // Then the others
        let mut custom_profiles: Vec<LoadedProfileFile> = vec![];
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
                debug!("loading config at path: {:?}", config_file);
                // TODO: Move `config_file` into `load_from_path` instead of passing it by reference
                match LoadedProfileFile::load_from_path(&config_file, Some(&default_profile)) {
                    Ok(config) => {
                        custom_profiles.push(config);
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
                default_profile,
                custom_profiles,
            },
            non_fatal_errors,
        ))
    }
}

#[cfg(test)]
mod tests {
    use regex::Regex;
    use shinran_types::RegexWrapper;

    use crate::config::parse::ParsedConfig;

    use super::*;

    pub fn new_mock(label: &'static str) -> ProfileFile {
        let label = label.to_owned();
        // let mut mock = MockConfig::new();
        // mock.expect_id().return_const(0);
        // mock.expect_label().return_const(label);
        // mock.expect_is_match().return_const(is_match);
        // mock
        ProfileFile {
            content: ParsedConfig {
                label: Some(label),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn config_store_selects_correctly() {
        let default = new_mock("default");
        let custom1 = new_mock("custom1");
        let mut custom2 = new_mock("custom2");
        custom2.filter.class = Some(RegexWrapper::new(Regex::new("foo").unwrap()));

        let store = ProfileStore {
            default_profile: default,
            custom_profiles: Box::new([custom1, custom2]),
        };

        assert_eq!(store.default_profile.label(), "default");
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

        let store = ProfileStore {
            default_profile: default,
            custom_profiles: Box::new([custom1, custom2]),
        };

        assert_eq!(store.default_profile.label(), "default");
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
