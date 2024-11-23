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

use espanso_config::{
    config::{ProfileFile, ProfileStore},
    matches::store::MatchStore,
};
use shinran_types::{MatchIdx, TrigMatchRef, VarRef};

use crate::engine::DetectedMatch;
// use crate::match_select::MatchSummary;
use crate::regex::{RegexMatch, RegexMatcher};

use super::builtin::BuiltInMatch;

pub struct MatchCache {
    trigger_default_profile: HashMap<String, TrigMatchRef>,
    trigger_custom_profiles: Vec<HashMap<String, TrigMatchRef>>,
    regex_default_profile: HashMap<String, usize>,
    regex_custom_profiles: Vec<HashMap<String, usize>>,
    global_var_default_profile: HashMap<String, VarRef>,
    global_var_custom_profile: Vec<HashMap<String, VarRef>>,
}

impl MatchCache {
    pub fn load(profile_store: &ProfileStore, match_store: &MatchStore) -> Self {
        let default_config = profile_store.default_config();

        let (trigger_default_profile, regex_default_profile, global_var_default_profile) =
            create_profile_cache(default_config, match_store);

        let mut trigger_custom_profiles: Vec<HashMap<String, TrigMatchRef>> = Vec::new();
        let mut regex_custom_profiles: Vec<HashMap<String, usize>> = Vec::new();
        let mut global_var_custom_profile: Vec<HashMap<String, VarRef>> = Vec::new();

        for profile in profile_store.custom_configs() {
            let (trigger_map, regex_map, global_var_map) =
                create_profile_cache(profile, match_store);
            trigger_custom_profiles.push(trigger_map);
            regex_custom_profiles.push(regex_map);
            global_var_custom_profile.push(global_var_map);
        }

        Self {
            trigger_default_profile,
            trigger_custom_profiles,
            regex_default_profile,
            regex_custom_profiles,
            global_var_default_profile,
            global_var_custom_profile,
        }
    }

    // fn ids(&self) -> Vec<i32> {
    //     self.cache.keys().copied().collect()
    // }

    // pub fn matches(&self) -> Vec<&Match> {
    //     self.cache.values().collect()
    // }

    // pub fn get(&self, id: i32) -> Option<&Match> {
    //     self.cache.get(&id)
    // }

    pub fn default_profile_and_matches(&self) -> &HashMap<String, TrigMatchRef> {
        &self.trigger_default_profile
    }
}

fn create_profile_cache(
    profile: &ProfileFile,
    match_store: &MatchStore,
) -> (
    HashMap<String, TrigMatchRef>,
    HashMap<String, usize>,
    HashMap<String, VarRef>,
) {
    let mut trigger_map: HashMap<String, TrigMatchRef> = HashMap::new();
    let mut regex_map: HashMap<String, usize> = HashMap::new();
    let mut global_var_map: HashMap<String, VarRef> = HashMap::new();

    let file_paths = profile.match_file_paths();
    let collection = match_store.collect_matches_and_global_vars(file_paths);

    for idx in collection.trigger_matches {
        let (triggers, _) = &match_store.trigger_matches.get(idx);
        for trigger in triggers {
            trigger_map.insert(trigger.clone(), idx);
        }
    }

    for idx in collection.regex_matches {
        let (regex, _) = &match_store.regex_matches[idx];
        regex_map.insert(regex.clone(), idx);
    }

    for idx in collection.global_vars {
        let global_var = &match_store.global_vars.get(idx);
        global_var_map.insert(global_var.name.clone(), idx);
    }

    (trigger_map, regex_map, global_var_map)
}

// impl<'a> espanso_engine::process::MatchInfoProvider for MatchCache<'a> {
//     fn get_force_mode(
//         &self,
//         match_id: i32,
//     ) -> Option<espanso_engine::event::effect::TextInjectMode> {
//         let m = self.cache.get(&match_id)?;
//         if let MatchEffect::Text(text_effect) = &m.effect {
//             if let Some(force_mode) = &text_effect.force_mode {
//                 match force_mode {
//                     espanso_config::matches::TextInjectMode::Keys => {
//                         return Some(espanso_engine::event::effect::TextInjectMode::Keys)
//                     }
//                     espanso_config::matches::TextInjectMode::Clipboard => {
//                         return Some(espanso_engine::event::effect::TextInjectMode::Clipboard)
//                     }
//                 }
//             }
//         }

//         None
//     }
// }

pub struct CombinedMatchCache {
    pub user_match_cache: MatchCache,
    builtin_match_cache: HashMap<i32, BuiltInMatch>,
    pub regex_matcher: RegexMatcher,
}

// pub enum MatchVariant<'a> {
//     Trigger(&'a TriggerMatch),
//     Regex(&'a BaseMatch),
//     Builtin(&'a BuiltInMatch),
// }

impl CombinedMatchCache {
    pub fn load(
        match_cache: MatchCache,
        builtin_matches: Vec<BuiltInMatch>,
        regex_matches: Vec<RegexMatch<usize>>,
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

    pub(crate) fn find_matches_from_trigger(&self, trigger: &str) -> Vec<DetectedMatch> {
        let user_matches: Option<DetectedMatch> = self
            .user_match_cache
            .default_profile_and_matches()
            .get(trigger)
            .map(|&idx| DetectedMatch {
                id: MatchIdx::Trigger(idx),
                trigger: trigger.to_string(),
                ..Default::default()
            });
        // .values()
        // .filter_map(|idx| {
        //     let m = self.match_store.trigger_matches[idx];
        //     if trigger_cause.triggers.iter().any(|t| t == trigger) {
        //         Some(DetectedMatch {
        //             id: m.id,
        //             trigger: trigger.to_string(),
        //             ..Default::default()
        //         })
        //     } else {
        //         None
        //     }
        // })
        // .collect();

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
