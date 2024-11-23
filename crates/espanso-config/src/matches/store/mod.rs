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

mod default;
pub use default::MatchStore;
use shinran_types::{TrigMatchRef, VarRef};

/// The set of matches and global vars associated with one config file.
///
/// This struct contains a list of references to the matches that matched the query
/// and a list of references to the global variables that were defined in the matches.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchesAndGlobalVars {
    pub trigger_matches: Vec<TrigMatchRef>,
    pub regex_matches: Vec<usize>,
    pub global_vars: Vec<VarRef>,
}

pub fn load(paths: &[PathBuf]) -> (MatchStore, Vec<NonFatalErrorSet>) {
    // TODO: here we can replace the MatchStore with a caching wrapper
    // that returns the same response for the given "paths" query
    default::MatchStore::load(paths)
}
