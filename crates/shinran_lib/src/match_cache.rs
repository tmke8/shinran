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
    config::ProfileStore,
    matches::{store::MatchStore, Match, MatchCause},
};

use crate::engine::DetectedMatch;
// use crate::match_select::MatchSummary;
use crate::regex::{RegexMatch, RegexMatcher};

use super::builtin::BuiltInMatch;

pub struct MatchCache {
    cache: HashMap<i32, Match>,
}

impl MatchCache {
    pub fn load(profile_store: &ProfileStore, match_store: &MatchStore) -> Self {
        let mut cache = HashMap::new();

        let all_paths = profile_store
            .get_all_match_file_paths()
            .into_iter()
            .collect::<Vec<_>>();
        let global_set = match_store.collect_matches_and_global_vars(&all_paths);

        for m in global_set.matches {
            // We clone the match because we need to own it.
            // TODO: Investigate if we can avoid cloning the match
            cache.insert(m.id, m.clone());
        }

        Self { cache }
    }

    fn ids(&self) -> Vec<i32> {
        self.cache.keys().copied().collect()
    }

    pub fn matches(&self) -> Vec<&Match> {
        self.cache.values().collect()
    }

    pub fn get(&self, id: i32) -> Option<&Match> {
        self.cache.get(&id)
    }
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

pub enum MatchVariant<'a> {
    User(&'a Match),
    Builtin(&'a BuiltInMatch),
}

impl CombinedMatchCache {
    pub fn load(
        match_cache: MatchCache,
        builtin_matches: Vec<BuiltInMatch>,
        regex_matches: Vec<RegexMatch<i32>>,
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

    pub fn get(&self, match_id: i32) -> Option<MatchVariant<'_>> {
        if let Some(user_match) = self.user_match_cache.cache.get(&match_id) {
            return Some(MatchVariant::User(user_match));
        }

        if let Some(builtin_match) = self.builtin_match_cache.get(&match_id) {
            return Some(MatchVariant::Builtin(builtin_match));
        }

        None
    }

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

    fn get_all_matches_ids(&self) -> Vec<i32> {
        let mut ids: Vec<i32> = self.builtin_match_cache.keys().copied().collect();
        ids.extend(self.user_match_cache.ids());
        ids
    }

    pub(crate) fn find_matches_from_trigger(&self, trigger: &str) -> Vec<DetectedMatch> {
        let user_matches: Vec<DetectedMatch> = self
            .user_match_cache
            .cache
            .values()
            .filter_map(|m| {
                if let MatchCause::Trigger(trigger_cause) = &m.cause {
                    if trigger_cause.triggers.iter().any(|t| t == trigger) {
                        Some(DetectedMatch {
                            id: m.id,
                            trigger: trigger.to_string(),
                            ..Default::default()
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        let builtin_matches: Vec<DetectedMatch> = self
            .builtin_match_cache
            .values()
            .filter_map(|m| {
                if m.triggers.iter().any(|t| t == trigger) {
                    Some(DetectedMatch {
                        id: m.id,
                        trigger: trigger.to_string(),
                        ..Default::default()
                    })
                } else {
                    None
                }
            })
            .collect();

        let mut matches = Vec::with_capacity(user_matches.len() + builtin_matches.len());
        matches.extend(user_matches);
        matches.extend(builtin_matches);

        matches
    }
}
