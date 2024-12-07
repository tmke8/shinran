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

use std::collections::HashMap;

use shinran_config::{config::ProfileFile, matches::store::MatchStore};
use shinran_types::{MatchRef, RegexMatch, TriggerMatch, Variable};

use crate::engine::DetectedMatch;
use crate::regex::RegexMatcher;
use crate::Configuration;

use super::builtin::BuiltInMatch;

/// A cache for the active profile.
///
/// - For the trigger-based matches, we have a hash map from the trigger to the match.
/// - For the regex-based matches, we have a regex set.
/// - For the global variables, we have a hash map from the variable name to the variable.
pub struct ProfileCache<'store> {
    trigger_map: HashMap<&'store str, &'store TriggerMatch>,
    regex_matcher: RegexMatcher<'store>,
    global_var_map: HashMap<&'store str, &'store Variable>,
}

impl<'store> ProfileCache<'store> {
    pub fn new(configuration: &'store Configuration) -> Self {
        let active_profile = configuration.active_profile();
        let (trigger_map, global_var_map, regex_matches) =
            create_profile_cache(active_profile, &configuration.match_store);
        let regex_matcher = RegexMatcher::new(regex_matches);

        Self {
            trigger_map,
            regex_matcher,
            global_var_map,
        }
    }

    #[inline]
    pub fn trigger_matches(&self) -> &HashMap<&'store str, &'store TriggerMatch> {
        &self.trigger_map
    }

    #[inline]
    pub fn regex_matches(&self) -> &RegexMatcher<'store> {
        &self.regex_matcher
    }

    #[inline]
    pub fn global_vars(&self) -> &HashMap<&'store str, &'store Variable> {
        &self.global_var_map
    }
}

fn create_profile_cache<'store>(
    profile: &'store ProfileFile,
    match_store: &'store MatchStore,
) -> (
    HashMap<&'store str, &'store TriggerMatch>,
    HashMap<&'store str, &'store Variable>,
    Vec<&'store RegexMatch>,
) {
    let mut trigger_map = HashMap::new();
    let mut global_var_map = HashMap::new();

    let file_paths = profile.match_file_paths();
    let collection = match_store.collect_matches_and_global_vars(file_paths);

    for m in collection.trigger_matches {
        let triggers = &m.triggers;
        for trigger in triggers {
            trigger_map.insert(trigger.as_str(), m);
        }
    }

    for var in collection.global_vars {
        let var_name = var.name.as_str();
        global_var_map.insert(var_name, var);
    }

    (trigger_map, global_var_map, collection.regex_matches)
}

pub struct CombinedMatchCache<'store> {
    pub user_match_cache: ProfileCache<'store>,
    builtin_match_cache: HashMap<i32, BuiltInMatch>,
}

impl<'store> CombinedMatchCache<'store> {
    pub fn load(match_cache: ProfileCache<'store>, builtin_matches: Vec<BuiltInMatch>) -> Self {
        let mut builtin_match_cache = HashMap::new();

        for m in builtin_matches {
            builtin_match_cache.insert(m.id, m);
        }

        Self {
            user_match_cache: match_cache,
            builtin_match_cache,
        }
    }

    pub fn regex_matcher(&self) -> &RegexMatcher<'store> {
        self.user_match_cache.regex_matches()
    }

    pub(crate) fn find_matches_from_trigger(&self, trigger: &str) -> Vec<DetectedMatch> {
        let mut user_matches: Option<DetectedMatch> = self
            .user_match_cache
            .trigger_matches()
            .get(trigger)
            .map(|&m| DetectedMatch {
                id: MatchRef::Trigger(m),
                trigger: trigger.to_string(),
                left_separator: None,
                right_separator: None,
                args: HashMap::new(),
            });

        if user_matches.is_none() {
            // Try making the trigger lowercase.
            // However, this is only considered a match if `propagate_case` is set to true.
            // This needs to be checked during the rendering.
            user_matches = self
                .user_match_cache
                .trigger_matches()
                .get(&trigger.to_ascii_lowercase()[..])
                .map(|&m| DetectedMatch {
                    id: MatchRef::Trigger(m),
                    trigger: trigger.to_string(),
                    left_separator: None,
                    right_separator: None,
                    args: HashMap::new(),
                });
        }

        let builtin_matches: Vec<DetectedMatch> = self
            .builtin_match_cache
            .values()
            .filter_map(|m| {
                if m.triggers.iter().any(|t| t == trigger) {
                    Some(DetectedMatch {
                        id: MatchRef::BuiltIn(m.id),
                        trigger: trigger.to_string(),
                        left_separator: None,
                        right_separator: None,
                        args: HashMap::new(),
                    })
                } else {
                    None
                }
            })
            .collect();

        let mut matches =
            Vec::with_capacity(user_matches.as_ref().map_or(0, |_| 1) + builtin_matches.len());
        matches.extend(user_matches);
        matches.extend(builtin_matches);

        matches
    }
}
