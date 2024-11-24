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

use super::MatchesAndGlobalVars;
use crate::{error::NonFatalErrorSet, matches::group::LoadedMatchFile};
use anyhow::Context;
use shinran_types::{
    BaseMatch, MatchCause, TrigMatchRef, TrigMatchStore, TriggerMatch, VarRef, VarStore,
};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct IndexedMatchFile {
    pub imports: Vec<PathBuf>,
    pub global_vars: Vec<VarRef>,
    pub trigger_matches: Vec<TrigMatchRef>,
    pub regex_matches: Vec<usize>,
}

/// The MatchStore contains all matches that we have loaded.
///
/// We first have hash map of all match files, indexed by their file system path.
/// Then inside the match files, we have a vector of matches and a vector of global variables.
pub struct MatchStore {
    pub indexed_files: HashMap<PathBuf, IndexedMatchFile>,
    pub trigger_matches: TrigMatchStore,
    pub regex_matches: Vec<(String, BaseMatch)>,
    pub global_vars: VarStore,
}

impl MatchStore {
    pub fn load(paths: &[PathBuf]) -> (Self, Vec<NonFatalErrorSet>) {
        let mut loaded_files = HashMap::new();
        let mut non_fatal_error_sets = Vec::new();

        // Because match files can import other match files,
        // we have to load them recursively starting from the
        // top-level ones.
        load_match_files_recursively(&mut loaded_files, paths, &mut non_fatal_error_sets);

        let mut indexed_files = HashMap::new();
        let mut trigger_matches = TrigMatchStore::new();
        let mut regex_matches = Vec::new();
        let mut global_vars = VarStore::new();

        for (path, match_file) in loaded_files.into_iter() {
            let mut trigger_ids = Vec::new();
            let mut regex_ids = Vec::new();
            let mut global_vars_ids = Vec::new();
            for m in match_file.matches {
                let base_match = BaseMatch {
                    // id: m.id,
                    effect: m.effect,
                    label: m.label,
                    search_terms: m.search_terms,
                };
                match m.cause {
                    MatchCause::Trigger(trigger) => {
                        let match_ = TriggerMatch {
                            base_match,
                            propagate_case: trigger.propagate_case,
                            uppercase_style: trigger.uppercase_style,
                        };
                        let idx = trigger_matches.add(trigger.triggers, match_);
                        trigger_ids.push(idx);
                    }
                    MatchCause::Regex(regex) => {
                        let idx = regex_matches.len();
                        regex_matches.push((regex.regex, base_match));
                        regex_ids.push(idx);
                    }
                }
            }

            for v in match_file.global_vars {
                let idx = global_vars.add(v);
                global_vars_ids.push(idx);
            }

            let indexed_file = IndexedMatchFile {
                imports: match_file.imports,
                global_vars: global_vars_ids,
                trigger_matches: trigger_ids,
                regex_matches: regex_ids,
            };
            indexed_files.insert(path, indexed_file);
        }

        (
            Self {
                indexed_files,
                trigger_matches,
                regex_matches,
                global_vars,
            },
            non_fatal_error_sets,
        )
    }

    // pub fn get(&self, index: MatchIdx) -> &BaseMatch {
    //     match index {
    //         MatchIdx::Trigger(idx) => &self.trigger_matches[idx].1.base_match,
    //         MatchIdx::Regex(idx) => &self.regex_matches[idx].1,
    //     }
    // }

    /// Returns all matches and global vars that were defined in the given paths.
    ///
    /// This function recursively loads all the matches in the given paths and their imports.
    pub fn collect_matches_and_global_vars(&self, paths: &[PathBuf]) -> MatchesAndGlobalVars {
        let mut visited_paths = HashSet::new();
        let mut visited_trigger_matches = HashSet::new();
        let mut visited_regex_matches = HashSet::new();
        let mut visited_global_vars = HashSet::new();

        query_matches_for_paths(
            &self.indexed_files,
            &mut visited_paths,
            &mut visited_trigger_matches,
            &mut visited_regex_matches,
            &mut visited_global_vars,
            paths,
        );

        MatchesAndGlobalVars {
            trigger_matches: visited_trigger_matches.into_iter().collect(),
            regex_matches: visited_regex_matches.into_iter().collect(),
            global_vars: visited_global_vars.into_iter().collect(),
        }
    }

    pub fn loaded_paths(&self) -> Vec<PathBuf> {
        self.indexed_files.keys().cloned().collect()
    }
}

fn query_matches_for_paths(
    indexed_files: &HashMap<PathBuf, IndexedMatchFile>,
    visited_paths: &mut HashSet<PathBuf>,
    visited_trigger_matches: &mut HashSet<TrigMatchRef>,
    visited_regex_matches: &mut HashSet<usize>,
    visited_global_vars: &mut HashSet<VarRef>,
    paths: &[PathBuf],
) {
    for path in paths {
        if visited_paths.contains(path) {
            continue; // Already visited
        }

        visited_paths.insert(path.clone());

        if let Some(file) = indexed_files.get(path) {
            query_matches_for_paths(
                indexed_files,
                visited_paths,
                visited_trigger_matches,
                visited_regex_matches,
                visited_global_vars,
                &file.imports,
            );

            for m in &file.trigger_matches {
                if !visited_trigger_matches.contains(m) {
                    visited_trigger_matches.insert(*m);
                }
            }

            for m in &file.regex_matches {
                if !visited_regex_matches.contains(m) {
                    visited_regex_matches.insert(*m);
                }
            }

            for var in &file.global_vars {
                if !visited_global_vars.contains(var) {
                    visited_global_vars.insert(*var);
                }
            }
        }
    }
}
/// Load the files in the given paths and their imports recursively.
///
/// This function fills up the `groups` HashMap with the loaded match groups.
fn load_match_files_recursively(
    groups: &mut HashMap<PathBuf, LoadedMatchFile>,
    paths: &[PathBuf],
    non_fatal_error_sets: &mut Vec<NonFatalErrorSet>,
) {
    for path in paths {
        if groups.contains_key(path) {
            continue; // Already loaded
        }

        let group_path = PathBuf::from(path);
        match LoadedMatchFile::load(&group_path)
            .with_context(|| format!("unable to load match group {group_path:?}"))
        {
            Ok((group, non_fatal_error_set)) => {
                let imports = group.imports.clone();
                groups.insert(path.clone(), group);

                if let Some(non_fatal_error_set) = non_fatal_error_set {
                    non_fatal_error_sets.push(non_fatal_error_set);
                }

                load_match_files_recursively(groups, &imports, non_fatal_error_sets);
            }
            Err(err) => {
                non_fatal_error_sets.push(NonFatalErrorSet::single_error(&group_path, err));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use shinran_helpers::use_test_directory;
    use shinran_types::{MatchEffect, TextEffect, Variable};

    use super::*;
    use std::fs::create_dir_all;

    fn create_match(trigger: &str, replace: &str) -> (Vec<String>, TriggerMatch) {
        (
            vec![trigger.to_string()],
            TriggerMatch {
                base_match: BaseMatch {
                    effect: MatchEffect::Text(TextEffect {
                        body: replace.to_string(),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
    }

    fn create_matches(matches: &[(&str, &str)]) -> Vec<(Vec<String>, TriggerMatch)> {
        matches
            .iter()
            .map(|(trigger, replace)| create_match(trigger, replace))
            .collect()
    }

    fn sort_matches(matches: &mut Vec<(Vec<String>, TriggerMatch)>) {
        matches.sort_unstable_by(|a, b| {
            (&a.0, &a.1.base_match.effect.as_text().unwrap().body)
                .cmp(&(&b.0, &b.1.base_match.effect.as_text().unwrap().body))
        });
    }

    fn create_test_var(name: &str) -> Variable {
        Variable {
            name: name.to_string(),
            ..Default::default()
        }
    }

    fn create_vars(vars: &[&str]) -> Vec<Variable> {
        vars.iter().map(|var| create_test_var(var)).collect()
    }

    #[test]
    fn match_store_loads_correctly() {
        use_test_directory(|_, match_dir, _| {
            let sub_dir = match_dir.join("sub");
            create_dir_all(&sub_dir).unwrap();

            let base_file = match_dir.join("base.yml");
            std::fs::write(
                &base_file,
                r#"
      imports:
        - "_another.yml"

      matches:
        - trigger: "hello"
          replace: "world"
      "#,
            )
            .unwrap();

            let another_file = match_dir.join("_another.yml");
            std::fs::write(
                &another_file,
                r#"
      imports:
        - "sub/sub.yml"

      matches:
        - trigger: "hello"
          replace: "world2"
        - trigger: "foo"
          replace: "bar"
      "#,
            )
            .unwrap();

            let sub_file = sub_dir.join("sub.yml");
            std::fs::write(
                &sub_file,
                r#"
      matches:
        - trigger: "hello"
          replace: "world3"
      "#,
            )
            .unwrap();

            let (match_store, non_fatal_error_sets) = MatchStore::load(&[base_file.clone()]);
            assert_eq!(non_fatal_error_sets.len(), 0);
            assert_eq!(match_store.indexed_files.len(), 3);

            let base_group = &match_store
                .indexed_files
                .get(&base_file)
                .unwrap()
                .trigger_matches;
            let base_group: Vec<(Vec<String>, TriggerMatch)> = base_group
                .iter()
                .map(|&m| match_store.trigger_matches.get(m).clone())
                .collect();

            assert_eq!(base_group, create_matches(&[("hello", "world")]));

            let another_group = &match_store
                .indexed_files
                .get(&another_file)
                .unwrap()
                .trigger_matches;
            let another_group: Vec<(Vec<String>, TriggerMatch)> = another_group
                .iter()
                .map(|&m| match_store.trigger_matches.get(m).clone())
                .collect();
            assert_eq!(
                another_group,
                create_matches(&[("hello", "world2"), ("foo", "bar")])
            );

            let sub_group = &match_store
                .indexed_files
                .get(&sub_file)
                .unwrap()
                .trigger_matches;
            let sub_group: Vec<(Vec<String>, TriggerMatch)> = sub_group
                .iter()
                .map(|&m| match_store.trigger_matches.get(m).clone())
                .collect();
            assert_eq!(sub_group, create_matches(&[("hello", "world3")]));
        });
    }

    #[test]
    fn match_store_handles_circular_dependency() {
        use_test_directory(|_, match_dir, _| {
            let sub_dir = match_dir.join("sub");
            create_dir_all(&sub_dir).unwrap();

            let base_file = match_dir.join("base.yml");
            std::fs::write(
                &base_file,
                r#"
      imports:
        - "_another.yml"

      matches:
        - trigger: "hello"
          replace: "world"
      "#,
            )
            .unwrap();

            let another_file = match_dir.join("_another.yml");
            std::fs::write(
                another_file,
                r#"
      imports:
        - "sub/sub.yml"

      matches:
        - trigger: "hello"
          replace: "world2"
        - trigger: "foo"
          replace: "bar"
      "#,
            )
            .unwrap();

            let sub_file = sub_dir.join("sub.yml");
            std::fs::write(
                sub_file,
                r#"
      imports:
        - "../_another.yml"

      matches:
        - trigger: "hello"
          replace: "world3"
      "#,
            )
            .unwrap();

            let (match_store, non_fatal_error_sets) = MatchStore::load(&[base_file]);

            assert_eq!(match_store.indexed_files.len(), 3);
            assert_eq!(non_fatal_error_sets.len(), 0);
        });
    }

    #[test]
    fn match_store_query_single_path_with_imports() {
        use_test_directory(|_, match_dir, _| {
            let sub_dir = match_dir.join("sub");
            create_dir_all(&sub_dir).unwrap();

            let base_file = match_dir.join("base.yml");
            std::fs::write(
                &base_file,
                r#"
      imports:
        - "_another.yml"

      global_vars:
        - name: var1
          type: test

      matches:
        - trigger: "hello"
          replace: "world"
      "#,
            )
            .unwrap();

            let another_file = match_dir.join("_another.yml");
            std::fs::write(
                another_file,
                r#"
      imports:
        - "sub/sub.yml"

      matches:
        - trigger: "hello"
          replace: "world2"
        - trigger: "foo"
          replace: "bar"
      "#,
            )
            .unwrap();

            let sub_file = sub_dir.join("sub.yml");
            std::fs::write(
                sub_file,
                r#"
      global_vars:
        - name: var2
          type: test

      matches:
        - trigger: "hello"
          replace: "world3"
      "#,
            )
            .unwrap();

            let (match_store, non_fatal_error_sets) = MatchStore::load(&[base_file.clone()]);
            assert_eq!(non_fatal_error_sets.len(), 0);

            let match_set = match_store.collect_matches_and_global_vars(&[base_file]);

            let mut matches = match_set
                .trigger_matches
                .into_iter()
                .map(|m| match_store.trigger_matches.get(m).clone())
                .collect::<Vec<(Vec<String>, TriggerMatch)>>();

            sort_matches(&mut matches);

            assert_eq!(
                matches,
                create_matches(&[
                    ("foo", "bar"),
                    ("hello", "world"),
                    ("hello", "world2"),
                    ("hello", "world3"),
                ])
            );
            let mut vars = match_set
                .global_vars
                .into_iter()
                .map(|m| match_store.global_vars.get(m).clone())
                .collect::<Vec<Variable>>();
            vars.sort_unstable_by(|a, b| a.name.cmp(&b.name));

            assert_eq!(vars, create_vars(&["var1", "var2"]));
        });
    }

    #[test]
    fn match_store_query_handles_circular_depencencies() {
        use_test_directory(|_, match_dir, _| {
            let sub_dir = match_dir.join("sub");
            create_dir_all(&sub_dir).unwrap();

            let base_file = match_dir.join("base.yml");
            std::fs::write(
                &base_file,
                r#"
      imports:
        - "_another.yml"

      global_vars:
        - name: var1
          type: test

      matches:
        - trigger: "hello"
          replace: "world"
      "#,
            )
            .unwrap();

            let another_file = match_dir.join("_another.yml");
            std::fs::write(
                another_file,
                r#"
      imports:
        - "sub/sub.yml"

      matches:
        - trigger: "hello"
          replace: "world2"
        - trigger: "foo"
          replace: "bar"
      "#,
            )
            .unwrap();

            let sub_file = sub_dir.join("sub.yml");
            std::fs::write(
                sub_file,
                r#"
      imports:
        - "../_another.yml"  # Circular import

      global_vars:
        - name: var2
          type: test

      matches:
        - trigger: "hello"
          replace: "world3"
      "#,
            )
            .unwrap();

            let (match_store, non_fatal_error_sets) = MatchStore::load(&[base_file.clone()]);
            assert_eq!(non_fatal_error_sets.len(), 0);

            let match_set = match_store.collect_matches_and_global_vars(&[base_file]);
            let mut matches = match_set
                .trigger_matches
                .into_iter()
                .map(|m| match_store.trigger_matches.get(m).clone())
                .collect::<Vec<(Vec<String>, TriggerMatch)>>();
            sort_matches(&mut matches);

            assert_eq!(
                matches,
                create_matches(&[
                    ("foo", "bar"),
                    ("hello", "world"),
                    ("hello", "world2"),
                    ("hello", "world3"),
                ])
            );

            let mut vars = match_set
                .global_vars
                .into_iter()
                .map(|m| match_store.global_vars.get(m).clone())
                .collect::<Vec<Variable>>();
            vars.sort_unstable_by(|a, b| a.name.cmp(&b.name));

            assert_eq!(vars, create_vars(&["var1", "var2"]));
        });
    }

    #[test]
    fn match_store_query_multiple_paths() {
        use_test_directory(|_, match_dir, _| {
            let sub_dir = match_dir.join("sub");
            create_dir_all(&sub_dir).unwrap();

            let base_file = match_dir.join("base.yml");
            std::fs::write(
                &base_file,
                r#"
      imports:
        - "_another.yml"

      global_vars:
        - name: var1
          type: test

      matches:
        - trigger: "hello"
          replace: "world"
      "#,
            )
            .unwrap();

            let another_file = match_dir.join("_another.yml");
            std::fs::write(
                another_file,
                r#"
      matches:
        - trigger: "hello"
          replace: "world2"
        - trigger: "foo"
          replace: "bar"
      "#,
            )
            .unwrap();

            let sub_file = sub_dir.join("sub.yml");
            std::fs::write(
                &sub_file,
                r#"
      global_vars:
        - name: var2
          type: test

      matches:
        - trigger: "hello"
          replace: "world3"
      "#,
            )
            .unwrap();

            let (match_store, non_fatal_error_sets) =
                MatchStore::load(&[base_file.clone(), sub_file.clone()]);
            assert_eq!(non_fatal_error_sets.len(), 0);

            let match_set = match_store.collect_matches_and_global_vars(&[base_file, sub_file]);
            let mut matches = match_set
                .trigger_matches
                .into_iter()
                .map(|m| match_store.trigger_matches.get(m).clone())
                .collect::<Vec<(Vec<String>, TriggerMatch)>>();
            sort_matches(&mut matches);

            assert_eq!(
                matches,
                create_matches(&[
                    ("foo", "bar"),
                    ("hello", "world"),
                    ("hello", "world2"),
                    ("hello", "world3"),
                ])
            );

            let mut vars = match_set
                .global_vars
                .into_iter()
                .map(|m| match_store.global_vars.get(m).clone())
                .collect::<Vec<Variable>>();
            vars.sort_unstable_by(|a, b| a.name.cmp(&b.name));

            assert_eq!(vars, create_vars(&["var1", "var2"]));
        });
    }

    #[test]
    fn match_store_query_handle_duplicates_when_imports_and_paths_overlap() {
        use_test_directory(|_, match_dir, _| {
            let sub_dir = match_dir.join("sub");
            create_dir_all(&sub_dir).unwrap();

            let base_file = match_dir.join("base.yml");
            std::fs::write(
                &base_file,
                r#"
      imports:
        - "_another.yml"

      global_vars:
        - name: var1
          type: test

      matches:
        - trigger: "hello"
          replace: "world"
      "#,
            )
            .unwrap();

            let another_file = match_dir.join("_another.yml");
            std::fs::write(
                another_file,
                r#"
      imports:
        - "sub/sub.yml"

      matches:
        - trigger: "hello"
          replace: "world2"
        - trigger: "foo"
          replace: "bar"
      "#,
            )
            .unwrap();

            let sub_file = sub_dir.join("sub.yml");
            std::fs::write(
                &sub_file,
                r#"
      global_vars:
        - name: var2
          type: test

      matches:
        - trigger: "hello"
          replace: "world3"
      "#,
            )
            .unwrap();

            let (match_store, non_fatal_error_sets) = MatchStore::load(&[base_file.clone()]);
            assert_eq!(non_fatal_error_sets.len(), 0);

            let match_set = match_store.collect_matches_and_global_vars(&[base_file, sub_file]);
            let mut matches = match_set
                .trigger_matches
                .into_iter()
                .map(|m| match_store.trigger_matches.get(m).clone())
                .collect::<Vec<(Vec<String>, TriggerMatch)>>();
            sort_matches(&mut matches);

            assert_eq!(
                matches,
                create_matches(&[
                    ("foo", "bar"),
                    ("hello", "world"),
                    ("hello", "world2"),
                    ("hello", "world3"), // This appears only once, though it appears 2 times
                ])
            );

            let mut vars = match_set
                .global_vars
                .into_iter()
                .map(|m| match_store.global_vars.get(m).clone())
                .collect::<Vec<Variable>>();
            vars.sort_unstable_by(|a, b| a.name.cmp(&b.name));

            assert_eq!(vars, create_vars(&["var1", "var2"]));
        });
    }

    // TODO: add fatal and non-fatal error cases
}
