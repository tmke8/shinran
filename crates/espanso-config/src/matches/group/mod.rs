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

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::error::NonFatalErrorSet;

use super::{Match, Variable};

pub(crate) mod loader;
mod path;

/// A `LoadedMatchFile` describes one file in the `match` directory.
///
/// Such a file has a list of imports, a list of global variables and a list of matches.
/// The imports have been resolved to paths, but they haven't been loaded yet.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LoadedMatchFile {
    pub imports: Vec<PathBuf>,
    pub global_vars: Vec<Variable>,
    pub matches: Vec<Match>,
}

impl LoadedMatchFile {
    // TODO: test
    pub fn load(file_path: &Path) -> Result<(Self, Option<NonFatalErrorSet>)> {
        loader::load_match_file(file_path)
    }
}
