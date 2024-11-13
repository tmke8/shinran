/*
 * This file is part of espanso.
 *
 * Copyright (C) 2019-202 case_insensitive: (), preserve_case_markers: (), left_word: (), right_word: ()1 Federico Terzi
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

use log::error;
use regex::{Regex, RegexSet};

use crate::engine::DetectedMatch;

#[derive(Debug)]
pub struct RegexMatch<Id> {
    pub id: Id,
    pub regex: String,
}

impl<Id> RegexMatch<Id> {
    pub fn new(id: Id, regex: &str) -> Self {
        Self {
            id,
            regex: regex.to_string(),
        }
    }
}

pub struct RegexMatcher {
    ids: Vec<i32>,
    // The RegexSet is used to efficiently determine which regexes match
    regex_set: RegexSet,

    // The single regexes are then used to find the captures
    regexes: Vec<Regex>,
}

impl RegexMatcher {
    // fn process(
    //     &'a self,
    //     prev_state: Option<&RegexMatcherState>,
    //     event: Event,
    // ) -> (RegexMatcherState, Vec<MatchResult<Id>>) {
    //     let mut buffer = if let Some(prev_state) = prev_state {
    //         prev_state.buffer.clone()
    //     } else {
    //         String::new()
    //     };

    //     if let Event::Key {
    //         key: _,
    //         chars: Some(chars),
    //     } = event
    //     {
    //         buffer.push_str(&chars);
    //     }

    //     // Keep the buffer length in check
    //     if buffer.len() > self.max_buffer_size {
    //         buffer.remove(0);
    //     }

    //     // Find matches
    //     let matches = if self.regex_set.is_match(&buffer) {
    //         self.fun_name(&buffer)
    //     };
    //     if !matches.is_empty() {
    //         return (RegexMatcherState::default(), matches);
    //     }

    //     let current_state = RegexMatcherState { buffer };
    //     (current_state, Vec::new())
    // }

    pub fn find_matches(&self, trigger: &str) -> Vec<DetectedMatch> {
        let mut matches = Vec::new();

        for index in self.regex_set.matches(trigger) {
            let (Some(id), Some(regex)) = (self.ids.get(index), self.regexes.get(index)) else {
                error!(
                    "received inconsistent index from regex set with index: {}",
                    index
                );
                continue;
            };

            let Some(captures) = regex.captures(trigger) else {
                continue;
            };

            let full_match = captures.get(0).map_or("", |m| m.as_str());
            if !full_match.is_empty() {
                // Now extract the captured names as variables
                let variables: HashMap<String, String> = regex
                    .capture_names()
                    .flatten()
                    .filter_map(|n| Some((n.to_string(), captures.name(n)?.as_str().to_string())))
                    .collect();

                let result = DetectedMatch {
                    id: *id,
                    trigger: full_match.to_string(),
                    left_separator: None,
                    right_separator: None,
                    args: variables,
                };

                matches.push(result);
            }
        }
        matches
    }
}

impl RegexMatcher {
    pub fn new(matches: Vec<RegexMatch<i32>>) -> Self {
        let mut ids = Vec::new();
        let mut regexes = Vec::new();
        let mut good_regexes = Vec::new();

        for m in matches {
            match Regex::new(&m.regex) {
                Ok(regex) => {
                    ids.push(m.id);
                    good_regexes.push(m.regex);
                    regexes.push(regex);
                }
                Err(err) => {
                    error!("unable to compile regex: '{}', error: {:?}", m.regex, err);
                }
            }
        }

        let regex_set = RegexSet::new(&good_regexes).expect("unable to build regex set");

        Self {
            ids,
            regex_set,
            regexes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn get_matches_after_str(
        string: &str,
        matcher: &RegexMatcher,
    ) -> Vec<DetectedMatch> {
        matcher.find_matches(&string)

        // for c in string.chars() {
        //     let (state, vec_matches) = matcher.process(
        //         prev_state.as_ref(),
        //         Event::Key {
        //             key: Key::Other,
        //             chars: Some(c.to_string()),
        //         },
        //     );

        //     prev_state = Some(state);
        //     matches = vec_matches;
        // }

        // matches
    }

    fn match_result(id: i32, trigger: &str, vars: &[(&str, &str)]) -> DetectedMatch {
        let args: HashMap<String, String> = vars
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect();

        DetectedMatch {
            id,
            trigger: trigger.to_string(),
            left_separator: None,
            right_separator: None,
            args,
        }
    }

    #[test]
    fn matcher_simple_matches() {
        let matcher = RegexMatcher::new(vec![
            RegexMatch::new(1, "hello"),
            RegexMatch::new(2, "num\\d{1,3}s"),
        ]);
        assert_eq!(get_matches_after_str("hi", &matcher), vec![]);
        assert_eq!(
            get_matches_after_str("hello", &matcher),
            vec![match_result(1, "hello", &[])]
        );
        assert_eq!(
            get_matches_after_str("say hello", &matcher),
            vec![match_result(1, "hello", &[])]
        );
        assert_eq!(
            get_matches_after_str("num1s", &matcher),
            vec![match_result(2, "num1s", &[])]
        );
        assert_eq!(
            get_matches_after_str("num134s", &matcher),
            vec![match_result(2, "num134s", &[])]
        );
        assert_eq!(get_matches_after_str("nums", &matcher), vec![]);
    }

    #[test]
    fn matcher_with_variables() {
        let matcher = RegexMatcher::new(vec![
            RegexMatch::new(1, "hello\\((?P<name>.*?)\\)"),
            RegexMatch::new(2, "multi\\((?P<name1>.*?),(?P<name2>.*?)\\)"),
        ]);
        assert_eq!(get_matches_after_str("hi", &matcher), vec![]);
        assert_eq!(
            get_matches_after_str("say hello(mary)", &matcher),
            vec![match_result(1, "hello(mary)", &[("name", "mary")])]
        );
        assert_eq!(get_matches_after_str("hello(mary", &matcher), vec![]);
        assert_eq!(
            get_matches_after_str("multi(mary,jane)", &matcher),
            vec![match_result(
                2,
                "multi(mary,jane)",
                &[("name1", "mary"), ("name2", "jane")]
            )]
        );
    }

    #[test]
    fn matcher_max_buffer_size() {
        let matcher = RegexMatcher::new(vec![
            RegexMatch::new(1, "hello\\((?P<name>.*?)\\)"),
            RegexMatch::new(2, "multi\\((?P<name1>.*?),(?P<name2>.*?)\\)"),
        ]);
        assert_eq!(
            get_matches_after_str("say hello(mary)", &matcher),
            vec![match_result(1, "hello(mary)", &[("name", "mary")])]
        );
        assert_eq!(
            get_matches_after_str("hello(very long name over buffer)", &matcher),
            vec![match_result(
                1,
                "hello(very long name over buffer)",
                &[("name", "very long name over buffer")]
            )]
        );
    }
}
