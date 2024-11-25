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

use shinran_types::{MatchCause, MatchEffect, TriggerCause, Variable};

pub(crate) mod group;
pub mod store;

#[derive(Debug, Clone, PartialEq)]
pub struct LoadedMatch {
    pub cause: MatchCause,
    pub effect: MatchEffect,

    // Metadata
    pub label: Option<String>,
    pub search_terms: Vec<String>,
}

impl Default for LoadedMatch {
    fn default() -> Self {
        Self {
            cause: MatchCause::Trigger(TriggerCause::default()),
            effect: MatchEffect::None,
            label: None,
            search_terms: vec![],
        }
    }
}

impl LoadedMatch {
    // TODO: test
    pub fn description(&self) -> &str {
        if let Some(label) = &self.label {
            label
        } else if let MatchEffect::Text(text_effect) = &self.effect {
            &text_effect.body
        } else if let MatchEffect::Image(_) = &self.effect {
            "Image content"
        } else {
            "No description available for this match"
        }
    }

    // TODO: test
    pub fn cause_description(&self) -> Option<&str> {
        self.cause.description()
    }

    pub fn search_terms(&self) -> Vec<&str> {
        self.search_terms
            .iter()
            .map(String::as_str)
            .chain(self.cause.search_terms())
            .collect()
    }
}
