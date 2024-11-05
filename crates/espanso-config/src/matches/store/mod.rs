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

use crate::error::NonFatalErrorSet;

use super::{Match, Variable};

mod default;
pub use default::MatchStore;

/// The set of matches returned by a query to the `MatchStore`.
///
/// This struct contains a list of references to the matches that matched the query
/// and a list of references to the global variables that were defined in the matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchSet<'store> {
    pub matches: Vec<&'store Match>,
    pub global_vars: Vec<&'store Variable>,
}

pub fn load(paths: &[PathBuf]) -> (MatchStore, Vec<NonFatalErrorSet>) {
    // TODO: here we can replace the MatchStore with a caching wrapper
    // that returns the same response for the given "paths" query
    default::MatchStore::load(paths)
}
