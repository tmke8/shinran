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

use super::{ConfigStoreError, ProfileFile};
use anyhow::{Context, Result};
use log::{debug, error};
use rkyv::{
    string::ArchivedString,
    with::{AsString, DeserializeWith},
    Archive, Deserialize, Infallible, Serialize,
};

#[derive(Archive, Serialize, Deserialize)]
#[archive(check_bytes)]
#[repr(transparent)]
pub struct ProfileStore {
    profiles: Vec<ProfileFile>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
#[repr(transparent)]
pub struct ProfileRef {
    idx: usize,
}

impl ProfileStore {
    #[inline]
    pub fn default_config(&self) -> ProfileRef {
        ProfileRef { idx: 0 }
    }

    #[inline]
    pub fn get(&self, ref_: ProfileRef) -> &ProfileFile {
        &self.profiles[ref_.idx]
    }

    /// Get the active configuration for the given app.
    ///
    /// This will return the *first* custom configuration that matches the app properties.
    pub fn active_config(&self, app: &super::AppProperties) -> ProfileRef {
        // Find a custom config that matches or fallback to the default one
        for (idx, custom) in self.profiles[1..].iter().enumerate() {
            if custom.filter.is_match(app) {
                return ProfileRef { idx: idx + 1 };
            }
        }
        self.default_config()
    }

    pub fn resolve_paths(
        loaded: LoadedProfileStore,
        file_map: &HashMap<PathBuf, MatchFileRef>,
    ) -> Self {
        let profiles = loaded
            .profiles
            .into_iter()
            .map(|loaded| ProfileFile::from_loaded_profile(loaded, file_map))
            .collect::<_>();
        ProfileStore { profiles }
    }

    pub fn all_configs(&self) -> Vec<ProfileRef> {
        (0..self.profiles.len())
            .map(|idx| ProfileRef { idx })
            .collect()
    }
}

impl ArchivedProfileStore {
    pub fn get_source_paths(&self) -> impl Iterator<Item = &Path> {
        self.profiles
            .iter()
            .map(|p| Path::new(p.source_path.as_str()))
    }
}

#[repr(transparent)]
pub struct LoadedProfileStore {
    profiles: Vec<LoadedProfileFile>,
}

impl LoadedProfileStore {
    #[inline]
    pub fn get(&self, ref_: ProfileRef) -> &LoadedProfileFile {
        &self.profiles[ref_.idx]
    }

    // TODO: test
    pub fn get_all_match_file_paths(&self) -> HashSet<PathBuf> {
        let mut paths = HashSet::new();

        for profile in &self.profiles {
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

        let default = LoadedProfileFile::load_from_path(&default_file, None)
            .context("failed to load default.yml configuration")?;
        debug!("loaded default config at path: {:?}", default_file);

        // Then the others
        let mut profiles: Vec<LoadedProfileFile> = vec![default];
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
                match LoadedProfileFile::load_from_path(&config_file, Some(&profiles[0])) {
                    Ok(config) => {
                        profiles.push(config);
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

        Ok((Self { profiles }, non_fatal_errors))
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
            profiles: vec![default, custom1, custom2],
        };

        assert_eq!(store.get(store.default_config()).label(), "default");
        assert_eq!(
            store
                .get(store.active_config(&crate::config::AppProperties {
                    title: None,
                    class: Some("foo"),
                    exec: None,
                }))
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
            profiles: vec![default, custom1, custom2],
        };

        assert_eq!(store.get(store.default_config()).label(), "default");
        assert_eq!(
            store
                .get(store.active_config(&crate::config::AppProperties {
                    title: None,
                    class: None,
                    exec: None,
                }))
                .label(),
            "default"
        );
    }
}
