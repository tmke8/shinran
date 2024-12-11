use std::collections::HashMap;
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
use std::path::{Path, PathBuf};

use rkyv::with::AsString;
use rkyv::{Archive, Deserialize, Serialize};
use shinran_types::{RegexMatch, TriggerMatch, Variable};

pub(crate) mod loader;
mod path;

/// Content of a match file.
///
/// This struct owns the variables and matches, and is used to store the content of a match file.
#[derive(Debug, Clone, PartialEq, Default, Archive, Serialize, Deserialize)]
#[archive(check_bytes)]
pub struct MatchFile {
    pub global_vars: Vec<Variable>,
    pub trigger_matches: Vec<TriggerMatch>,
    pub regex_matches: Vec<RegexMatch>,
}

/// A `LoadedMatchFile` describes one file in the `match` directory.
///
/// Such a file has a list of imports, and the content, which is the matches and variables.
/// The imports have been converted to paths, but they haven't been loaded yet.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LoadedMatchFile {
    pub import_paths: Vec<PathBuf>,
    pub content: MatchFile,
    pub source_path: PathBuf,
}

/// A wrapper around `Vec` which only allows appending, and which returns a reference to the
/// appended element.
#[derive(Debug, Clone, PartialEq, Default, Archive, Serialize, Deserialize)]
#[archive(check_bytes)]
#[repr(transparent)]
pub struct FileStore<T> {
    files: Vec<T>,
}

/// A reference to a file in a `FileStore`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash, Archive, Serialize, Deserialize)]
#[archive(check_bytes)]
#[archive_attr(derive(Hash, PartialEq, Eq))]
#[repr(transparent)]
pub struct MatchFileRef {
    idx: usize,
}

impl PartialEq<usize> for MatchFileRef {
    fn eq(&self, other: &usize) -> bool {
        self.idx == *other
    }
}

impl<T> FileStore<T> {
    #[inline]
    pub fn len(&self) -> usize {
        self.files.len()
    }
}

impl FileStore<LoadedMatchFile> {
    #[inline]
    pub(crate) fn new() -> Self {
        Self { files: Vec::new() }
    }

    #[inline]
    pub(crate) fn add(&mut self, file: LoadedMatchFile) -> MatchFileRef {
        let idx = self.len();
        self.files.push(file);
        MatchFileRef { idx }
    }

    #[inline]
    pub fn into_enumerate(self) -> impl Iterator<Item = (MatchFileRef, LoadedMatchFile)> {
        self.files
            .into_iter()
            .enumerate()
            .map(|(idx, elem)| (MatchFileRef { idx }, elem))
    }

    /// Resolve all imports with the given map.
    ///
    /// This function consumes the `FileStore` and returns a new one with resolved imports.
    /// Any [`MatchFileRef`] should remain valid for the new `FileStore`.
    pub(crate) fn resolve(
        self,
        match_file_map: &HashMap<PathBuf, MatchFileRef>,
    ) -> FileStore<ResolvedMatchFile> {
        let indexed_files = self
            .files
            .into_iter()
            .map(|match_file| {
                let resolved_imports = match_file
                    .import_paths
                    .into_iter()
                    .filter_map(|path| match_file_map.get(&path).copied())
                    .collect::<_>();
                ResolvedMatchFile {
                    imports: resolved_imports,
                    content: match_file.content,
                    source_path: match_file.source_path,
                }
            })
            .collect();
        FileStore {
            files: indexed_files,
        }
    }
}

/// Struct representing a match file, where all imports have been resolved.
///
/// In contrast, a [`LoadedMatchFile`] contains unresolved imports.
#[derive(Debug, Clone, PartialEq, Default, Archive, Serialize, Deserialize)]
#[archive(check_bytes)]
pub struct ResolvedMatchFile {
    pub(crate) imports: Vec<MatchFileRef>,
    pub(crate) content: MatchFile,
    #[with(AsString)]
    pub(crate) source_path: PathBuf,
}

impl ArchivedResolvedMatchFile {
    pub fn get_source_path(&self) -> &Path {
        Path::new(self.source_path.as_str())
    }
}

impl FileStore<ResolvedMatchFile> {
    #[inline]
    pub fn get(&self, idx: MatchFileRef) -> &ResolvedMatchFile {
        &self.files[idx.idx]
    }
}

impl ArchivedFileStore<ResolvedMatchFile> {
    pub fn get_source_paths(&self) -> impl Iterator<Item = &Path> {
        self.files.iter().map(|file| file.get_source_path())
    }
}
