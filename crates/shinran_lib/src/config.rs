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

use espanso_config::{
    config::{ProfileFile, ProfileStore},
    matches::store::{MatchStore, MatchesAndGlobalVars},
};

use crate::builtin::is_builtin_match;
// use espanso_info::{AppInfo, AppInfoProvider};

// use super::{
//     builtin::is_builtin_match,
//     engine::process::middleware::render::extension::clipboard::ClipboardOperationOptionsProvider,
// };

/// Struct containing all information loaded from the configuration files.
/// This includes the config files in the `config` directory and the match files in the `match` directory.
pub struct Configuration {
    profile_store: ProfileStore,
    pub match_store: MatchStore,
    // app_info_provider: &'a dyn AppInfoProvider,
}

impl Configuration {
    pub fn new(
        profile_store: ProfileStore,
        match_store: MatchStore,
        // app_info_provider: &'a dyn AppInfoProvider,
    ) -> Self {
        Self {
            profile_store,
            match_store,
            // app_info_provider,
        }
    }

    #[inline]
    pub fn default_profile(&self) -> &ProfileFile {
        self.profile_store.default_config()
    }

    pub fn default_profile_and_matches(&self) -> (&ProfileFile, MatchesAndGlobalVars) {
        let config = self.default_profile();
        let match_paths = config.match_file_paths();
        (
            config,
            self.match_store
                .collect_matches_and_global_vars(match_paths),
        )
    }

    /// Get the active configuration file according to the current app.
    ///
    /// This functionality is not implemented yet.
    pub fn active_profile(&self) -> &ProfileFile {
        // let current_app = self.app_info_provider.get_info();
        // let info = to_app_properties(&current_app);
        let info = espanso_config::config::AppProperties {
            title: None,
            class: None,
            exec: None,
        };
        self.profile_store.active_config(&info)
    }

    pub fn active_profile_and_matches(&self) -> (&ProfileFile, MatchesAndGlobalVars) {
        let profile = self.active_profile();
        let match_paths = profile.match_file_paths();
        (
            profile,
            self.match_store
                .collect_matches_and_global_vars(match_paths),
        )
    }

    // pub fn filter_active(&self, matches_ids: &[i32]) -> Vec<i32> {
    //     let ids_set: HashSet<i32> = matches_ids.iter().copied().collect::<HashSet<_>>();
    //     let (_, match_set) = self.active_profile_and_matches();

    //     let active_user_defined_matches: Vec<i32> = match_set
    //         .trigger_matches
    //         .iter()
    //         .filter(|m| ids_set.contains(&m.id))
    //         .map(|m| m.id)
    //         .collect();

    //     let builtin_matches: Vec<i32> = matches_ids
    //         .iter()
    //         .filter(|id| is_builtin_match(**id))
    //         .copied()
    //         .collect();

    //     let mut output = active_user_defined_matches;
    //     output.extend(builtin_matches);
    //     output
    // }

    /// Get all the configs and their match sets.
    pub fn collect_matches_and_global_vars_from_all_configs(
        &self,
    ) -> Vec<(&ProfileFile, MatchesAndGlobalVars)> {
        self.profile_store
            .all_configs()
            .into_iter()
            .map(|config| {
                let match_set = self
                    .match_store
                    .collect_matches_and_global_vars(config.match_file_paths());
                (config, match_set)
            })
            .collect()
    }
}
