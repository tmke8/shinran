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
use std::path::PathBuf;

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

#[repr(transparent)]
pub struct MatchFileStore {
    files: Vec<LoadedMatchFile>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash, Archive, Serialize, Deserialize)]
#[archive(check_bytes)]
#[archive_attr(derive(Hash, PartialEq, Eq))]
#[repr(transparent)]
pub struct MatchFileRef {
    pub idx: usize,
}

impl PartialEq<usize> for MatchFileRef {
    fn eq(&self, other: &usize) -> bool {
        self.idx == *other
    }
}

impl MatchFileStore {
    #[inline]
    pub fn new() -> Self {
        Self { files: Vec::new() }
    }

    #[inline]
    pub fn add(&mut self, file: LoadedMatchFile) -> MatchFileRef {
        let idx = self.files.len();
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

    #[inline]
    pub fn len(&self) -> usize {
        self.files.len()
    }
}
