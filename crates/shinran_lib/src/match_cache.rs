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

use shinran_config::{
    config::{ProfileFile, ProfileStore},
    matches::store::MatchStore,
    ProfileRef,
};
use shinran_types::{MatchIdx, RegexMatchRef, TrigMatchRef, VarRef};

use crate::engine::DetectedMatch;
use crate::regex::{RegexMatch, RegexMatcher};

use super::builtin::BuiltInMatch;

pub struct MatchCache<'store> {
    trigger_profiles: HashMap<ProfileRef, HashMap<&'store str, TrigMatchRef>>,
    // TODO: Generate these `RegexMatcher`s.
    regex_profiles: HashMap<ProfileRef, RegexMatcher>,
    global_var_profiles: HashMap<ProfileRef, HashMap<&'store str, VarRef>>,
}

impl<'store> MatchCache<'store> {
    pub fn load(profile_store: &'store ProfileStore, match_store: &'store MatchStore) -> Self {
        let mut trigger_profiles: HashMap<ProfileRef, HashMap<&'store str, TrigMatchRef>> =
            HashMap::new();
        let mut global_var_profiles: HashMap<ProfileRef, HashMap<&'store str, VarRef>> =
            HashMap::new();

        for profile_ref in profile_store.all_configs() {
            let profile = profile_store.get(profile_ref);
            let (trigger_map, global_var_map) = create_profile_cache(profile, match_store);
            trigger_profiles.insert(profile_ref, trigger_map);
            global_var_profiles.insert(profile_ref, global_var_map);
        }

        Self {
            trigger_profiles,
            regex_profiles: HashMap::new(),
            global_var_profiles,
        }
    }

    pub fn matches(&self, profile_ref: ProfileRef) -> &HashMap<&'store str, TrigMatchRef> {
        &self.trigger_profiles[&profile_ref]
    }

    pub fn global_vars(&self, profile_ref: ProfileRef) -> &HashMap<&'store str, VarRef> {
        &self.global_var_profiles[&profile_ref]
    }
}

fn create_profile_cache<'store>(
    profile: &'store ProfileFile,
    match_store: &'store MatchStore,
) -> (
    HashMap<&'store str, TrigMatchRef>,
    HashMap<&'store str, VarRef>,
) {
    let mut trigger_map: HashMap<&'store str, TrigMatchRef> = HashMap::new();
    let mut global_var_map: HashMap<&'store str, VarRef> = HashMap::new();

    let file_paths = profile.match_file_paths();
    let collection = match_store.collect_matches_and_global_vars(file_paths);

    for idx in collection.trigger_matches {
        let (triggers, _) = &match_store.trigger_matches.get(idx);
        for trigger in triggers {
            trigger_map.insert(trigger, idx);
        }
    }

    for idx in collection.global_vars {
        let global_var = &match_store.global_vars.get(idx);
        global_var_map.insert(&global_var.name, idx);
    }

    (trigger_map, global_var_map)
}

pub struct CombinedMatchCache<'store> {
    pub user_match_cache: MatchCache<'store>,
    builtin_match_cache: HashMap<i32, BuiltInMatch>,
    pub regex_matcher: RegexMatcher,
}

// pub enum MatchVariant<'a> {
//     Trigger(&'a TriggerMatch),
//     Regex(&'a BaseMatch),
//     Builtin(&'a BuiltInMatch),
// }

impl<'store> CombinedMatchCache<'store> {
    pub fn load(
        match_cache: MatchCache<'store>,
        builtin_matches: Vec<BuiltInMatch>,
        regex_matches: Vec<RegexMatch<RegexMatchRef>>,
    ) -> Self {
        let mut builtin_match_cache = HashMap::new();

        for m in builtin_matches {
            builtin_match_cache.insert(m.id, m);
        }

        let matcher = RegexMatcher::new(regex_matches);

        Self {
            user_match_cache: match_cache,
            builtin_match_cache,
            regex_matcher: matcher,
        }
    }

    // pub fn get(&self, match_id: usize) -> Option<MatchVariant<'_>> {
    //     if let Some(user_match) = self.user_match_cache.cache.get(&match_id) {
    //         return Some(MatchVariant::User(user_match));
    //     }

    //     if let Some(builtin_match) = self.builtin_match_cache.get(&match_id) {
    //         return Some(MatchVariant::Builtin(builtin_match));
    //     }

    //     None
    // }

    // fn get_matches<'a>(&'a self, ids: &[i32]) -> Vec<MatchSummary<'a>> {
    //     ids.iter()
    //         .filter_map(|id| self.get(*id))
    //         .map(|m| match m {
    //             MatchVariant::User(m) => MatchSummary {
    //                 id: m.id,
    //                 label: m.description(),
    //                 tag: m.cause_description(),
    //                 additional_search_terms: m.search_terms(),
    //                 is_builtin: false,
    //             },
    //             MatchVariant::Builtin(m) => MatchSummary {
    //                 id: m.id,
    //                 label: m.label,
    //                 tag: m.triggers.first().map(String::as_ref),
    //                 additional_search_terms: vec![],
    //                 is_builtin: true,
    //             },
    //         })
    //         .collect()
    // }

    // fn get_all_matches_ids(&self) -> Vec<i32> {
    //     let mut ids: Vec<i32> = self.builtin_match_cache.keys().copied().collect();
    //     ids.extend(self.user_match_cache.ids());
    //     ids
    // }

    pub(crate) fn find_matches_from_trigger(
        &self,
        trigger: &str,
        active_profile: ProfileRef,
    ) -> Vec<DetectedMatch> {
        let mut user_matches: Option<DetectedMatch> = self
            .user_match_cache
            .matches(active_profile)
            .get(trigger)
            .map(|&idx| DetectedMatch {
                id: MatchIdx::Trigger(idx),
                trigger: trigger.to_string(),
                ..Default::default()
            });

        if user_matches.is_none() {
            // Try making the trigger lowercase.
            // However, this is only considered a match if `propagate_case` is set to true.
            // This needs to be checked during the rendering.
            user_matches = self
                .user_match_cache
                .matches(active_profile)
                .get(&trigger.to_ascii_lowercase()[..])
                .map(|&idx| DetectedMatch {
                    id: MatchIdx::Trigger(idx),
                    trigger: trigger.to_string(),
                    ..Default::default()
                });
        }

        let builtin_matches: Vec<DetectedMatch> = self
            .builtin_match_cache
            .values()
            .filter_map(|m| {
                if m.triggers.iter().any(|t| t == trigger) {
                    Some(DetectedMatch {
                        id: MatchIdx::BuiltIn(m.id),
                        trigger: trigger.to_string(),
                        ..Default::default()
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
