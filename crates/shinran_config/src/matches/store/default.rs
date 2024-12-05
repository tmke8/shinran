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

use crate::{
    error::NonFatalErrorSet,
    matches::group::{loader, MatchFile, MatchFileRef, MatchFileStore},
};
use anyhow::Context;
use rkyv::{with::AsString, Archive, Deserialize, Serialize};
use shinran_types::{MatchesAndGlobalVars, RegexMatch, TriggerMatch, Variable};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

/// Struct representing a match file, where all imports have been resolved.
///
/// In contrast, a [`LoadedMatchFile`] contains unresolved imports.
#[derive(Debug, Clone, PartialEq, Default, Archive, Serialize, Deserialize)]
#[archive(check_bytes)]
pub struct ResolvedMatchFile {
    imports: Vec<MatchFileRef>,
    content: MatchFile,
    #[with(AsString)]
    source_path: PathBuf,
}

impl ArchivedResolvedMatchFile {
    pub fn get_source_path(&self) -> &Path {
        Path::new(self.source_path.as_str())
    }
}

/// The MatchStore contains all matches that we have loaded.
///
/// We have a hash map of all match files, indexed by their file system path.
#[derive(Archive, Serialize, Deserialize)]
#[archive(check_bytes)]
pub struct MatchStore {
    // TODO: This HashMap should be a Vec, with the index being the MatchFileRef.
    indexed_files: HashMap<MatchFileRef, ResolvedMatchFile>,
}

impl MatchStore {
    pub fn load(
        paths: &[PathBuf],
    ) -> (Self, HashMap<PathBuf, MatchFileRef>, Vec<NonFatalErrorSet>) {
        let mut non_fatal_error_sets = Vec::new();
        let mut match_file_map = HashMap::new();
        let mut loaded_files = MatchFileStore::new();

        // Because match files can import other match files,
        // we have to load them recursively starting from the top-level ones.
        load_match_files_recursively(
            &mut loaded_files,
            &mut match_file_map,
            paths,
            &mut non_fatal_error_sets,
        );

        let mut indexed_files = HashMap::new();

        for (path, match_file) in loaded_files.into_enumerate() {
            let imports = match_file
                .import_paths
                .iter()
                .filter_map(|path| match_file_map.get(path).copied())
                .collect::<_>();

            let indexed_file = ResolvedMatchFile {
                imports,
                content: match_file.content,
                source_path: match_file.source_path,
            };
            indexed_files.insert(path, indexed_file);
        }

        (Self { indexed_files }, match_file_map, non_fatal_error_sets)
    }

    /// Returns all matches and global vars that were defined in the given paths.
    ///
    /// This function recursively loads all the matches in the given paths and their imports.
    pub fn collect_matches_and_global_vars<'store>(
        &'store self,
        paths: &[MatchFileRef],
    ) -> MatchesAndGlobalVars<'store> {
        let mut visited_paths = HashSet::new();
        let mut visited_trigger_matches = Vec::new();
        let mut visited_regex_matches = Vec::new();
        let mut visited_global_vars = Vec::new();

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

    pub fn loaded_paths(&self) -> Vec<MatchFileRef> {
        self.indexed_files.keys().copied().collect()
    }
}

impl ArchivedMatchStore {
    pub fn get_source_paths(&self) -> impl Iterator<Item = &Path> {
        self.indexed_files
            .iter()
            .map(|(_, file)| file.get_source_path())
    }
}

fn query_matches_for_paths<'store>(
    indexed_files: &'store HashMap<MatchFileRef, ResolvedMatchFile>,
    visited_paths: &mut HashSet<MatchFileRef>,
    visited_trigger_matches: &mut Vec<&'store TriggerMatch>,
    visited_regex_matches: &mut Vec<&'store RegexMatch>,
    visited_global_vars: &mut Vec<&'store Variable>,
    paths: &[MatchFileRef],
) {
    for path in paths {
        if visited_paths.contains(path) {
            continue; // Already visited
        }

        visited_paths.insert(*path);

        let file = indexed_files.get(path).unwrap();
        visited_trigger_matches.extend(file.content.trigger_matches.iter());
        visited_regex_matches.extend(file.content.regex_matches.iter());
        visited_global_vars.extend(file.content.global_vars.iter());

        query_matches_for_paths(
            indexed_files,
            visited_paths,
            visited_trigger_matches,
            visited_regex_matches,
            visited_global_vars,
            &file.imports,
        );
    }
}

/// Load the files in the given paths and their imports recursively.
///
/// This function fills up the `groups` HashMap with the loaded match groups.
fn load_match_files_recursively(
    loaded_files: &mut MatchFileStore,
    match_file_map: &mut HashMap<PathBuf, MatchFileRef>,
    paths: &[PathBuf],
    non_fatal_error_sets: &mut Vec<NonFatalErrorSet>,
) {
    for match_file_path in paths {
        if match_file_map.contains_key(match_file_path) {
            continue; // Already loaded
        }

        let file_path = match_file_path.to_owned();
        match loader::load_match_file(file_path)
            .with_context(|| format!("unable to load match group {match_file_path:?}"))
        {
            Ok((group, non_fatal_error_set)) => {
                // TODO: Restructure code to avoid cloning here.
                let imports = &group.import_paths.clone();
                let file_ref = loaded_files.add(group);
                match_file_map.insert(match_file_path.clone(), file_ref);

                if let Some(non_fatal_error_set) = non_fatal_error_set {
                    non_fatal_error_sets.push(non_fatal_error_set);
                }

                load_match_files_recursively(
                    loaded_files,
                    match_file_map,
                    imports,
                    non_fatal_error_sets,
                );
            }
            Err(err) => {
                non_fatal_error_sets.push(NonFatalErrorSet::single_error(match_file_path, err));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use shinran_helpers::use_test_directory;
    use shinran_types::{BaseMatch, MatchEffect, TextEffect, VarType, Variable};

    use super::*;
    use std::fs::create_dir_all;

    fn create_match(trigger: &str, replace: &str) -> TriggerMatch {
        TriggerMatch {
            triggers: vec![trigger.into()],
            base_match: BaseMatch {
                effect: MatchEffect::Text(TextEffect {
                    body: replace.to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn create_matches(matches: &[(&str, &str)]) -> Vec<TriggerMatch> {
        matches
            .iter()
            .map(|(trigger, replace)| create_match(trigger, replace))
            .collect()
    }

    fn sort_matches(matches: &mut Vec<TriggerMatch>) {
        matches.sort_unstable_by(|a, b| {
            (&a.triggers[0], &a.base_match.effect.as_text().unwrap().body)
                .cmp(&(&b.triggers[0], &b.base_match.effect.as_text().unwrap().body))
        });
    }

    fn create_test_var(name: &str) -> Variable {
        Variable {
            name: name.to_string(),
            var_type: VarType::Mock,
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

            let (match_store, file_map, non_fatal_error_sets) =
                MatchStore::load(&[base_file.clone()]);
            assert_eq!(non_fatal_error_sets.len(), 0);
            assert_eq!(match_store.indexed_files.len(), 3);

            let base_group = &match_store
                .indexed_files
                .get(file_map.get(&base_file).unwrap())
                .unwrap()
                .content
                .trigger_matches;

            assert_eq!(base_group, &create_matches(&[("hello", "world")]));

            let another_group = &match_store
                .indexed_files
                .get(file_map.get(&another_file).unwrap())
                .unwrap()
                .content
                .trigger_matches;
            assert_eq!(
                another_group,
                &create_matches(&[("hello", "world2"), ("foo", "bar")])
            );

            let sub_group = &match_store
                .indexed_files
                .get(file_map.get(&sub_file).unwrap())
                .unwrap()
                .content
                .trigger_matches;
            assert_eq!(sub_group, &create_matches(&[("hello", "world3")]));
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

            let (match_store, _, non_fatal_error_sets) = MatchStore::load(&[base_file]);

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

            let (match_store, file_map, non_fatal_error_sets) =
                MatchStore::load(&[base_file.clone()]);
            assert_eq!(non_fatal_error_sets.len(), 0);

            let match_set =
                match_store.collect_matches_and_global_vars(&[*file_map.get(&base_file).unwrap()]);

            let mut matches = match_set
                .trigger_matches
                .into_iter()
                .map(|m| m.clone())
                .collect::<Vec<TriggerMatch>>();

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
                .map(|m| m.clone())
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

            let (match_store, file_map, non_fatal_error_sets) =
                MatchStore::load(&[base_file.clone()]);
            assert_eq!(non_fatal_error_sets.len(), 0);

            let match_set =
                match_store.collect_matches_and_global_vars(&[*file_map.get(&base_file).unwrap()]);
            let mut matches = match_set
                .trigger_matches
                .into_iter()
                .map(|m| m.clone())
                .collect::<Vec<TriggerMatch>>();
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
                .map(|m| m.clone())
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

            let paths = [base_file, sub_file];
            let (match_store, file_map, non_fatal_error_sets) = MatchStore::load(&paths);
            assert_eq!(non_fatal_error_sets.len(), 0);

            let match_set = match_store.collect_matches_and_global_vars(&[
                *file_map.get(&paths[0]).unwrap(),
                *file_map.get(&paths[1]).unwrap(),
            ]);
            let mut matches = match_set
                .trigger_matches
                .into_iter()
                .map(|m| m.clone())
                .collect::<Vec<TriggerMatch>>();
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
                .map(|m| m.clone())
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

            let (match_store, file_map, non_fatal_error_sets) =
                MatchStore::load(&[base_file.clone()]);
            assert_eq!(non_fatal_error_sets.len(), 0);

            let match_set = match_store.collect_matches_and_global_vars(&[
                *file_map.get(&base_file).unwrap(),
                *file_map.get(&sub_file).unwrap(),
            ]);
            let mut matches = match_set
                .trigger_matches
                .into_iter()
                .map(|m| m.clone())
                .collect::<Vec<TriggerMatch>>();
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
                .map(|m| m.clone())
                .collect::<Vec<Variable>>();
            vars.sort_unstable_by(|a, b| a.name.cmp(&b.name));

            assert_eq!(vars, create_vars(&["var1", "var2"]));
        });
    }

    // TODO: add fatal and non-fatal error cases
}
