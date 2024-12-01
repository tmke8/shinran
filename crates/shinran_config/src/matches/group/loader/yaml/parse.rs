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

use std::borrow::Cow;

use anyhow::Result;
use serde::Deserialize;
use serde_yaml_ng::Mapping;

use crate::util::is_yaml_empty;

#[derive(Debug, Deserialize)]
pub struct YAMLMatchFile<'buffer> {
    #[serde(default)]
    pub imports: Option<Vec<Cow<'buffer, str>>>,

    #[serde(default)]
    pub global_vars: Option<Vec<YAMLVariable<'buffer>>>,

    #[serde(default, borrow)]
    pub matches: Option<Vec<YAMLMatch<'buffer>>>,
}

impl<'buffer> YAMLMatchFile<'buffer> {
    pub fn parse_from_str(yaml: &'buffer str) -> Result<Self> {
        // Because an empty string is not valid YAML but we want to support it anyway
        if is_yaml_empty(yaml) {
            return Ok(serde_yaml_ng::from_str(
                "arbitrary_field_that_will_not_block_the_parser: true",
            )?);
        }

        Ok(serde_yaml_ng::from_str(yaml)?)
    }
}

#[derive(Debug, Deserialize)]
pub struct YAMLMatch<'buffer> {
    #[serde(default)]
    pub label: Option<Cow<'buffer, str>>,

    #[serde(default)]
    pub trigger: Option<Cow<'buffer, str>>,

    #[serde(default)]
    pub triggers: Option<Vec<Cow<'buffer, str>>>,

    #[serde(default)]
    pub regex: Option<Cow<'buffer, str>>,

    #[serde(default)]
    pub replace: Option<Cow<'buffer, str>>,

    #[serde(default)]
    pub image_path: Option<Cow<'buffer, str>>,

    #[serde(default)]
    pub form: Option<Cow<'buffer, str>>,

    #[serde(default)]
    pub form_fields: Option<Mapping>,

    #[serde(default)]
    pub vars: Option<Vec<YAMLVariable<'buffer>>>,

    #[serde(default)]
    pub word: Option<bool>,

    #[serde(default)]
    pub left_word: Option<bool>,

    #[serde(default)]
    pub right_word: Option<bool>,

    #[serde(default)]
    pub propagate_case: Option<bool>,

    #[serde(default)]
    pub uppercase_style: Option<Cow<'buffer, str>>,

    #[serde(default)]
    pub force_clipboard: Option<bool>,

    #[serde(default)]
    pub force_mode: Option<Cow<'buffer, str>>,

    #[serde(default)]
    pub markdown: Option<Cow<'buffer, str>>,

    #[serde(default)]
    pub paragraph: Option<bool>,

    #[serde(default)]
    pub html: Option<Cow<'buffer, str>>,

    #[serde(default)]
    pub search_terms: Option<Vec<Cow<'buffer, str>>>,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct YAMLVariable<'buffer> {
    pub name: Cow<'buffer, str>,

    #[serde(rename = "type")]
    pub var_type: Cow<'buffer, str>,

    #[serde(default = "default_params")]
    pub params: Mapping,

    #[serde(default)]
    pub inject_vars: Option<bool>,

    #[serde(default)]
    pub depends_on: Vec<Cow<'buffer, str>>,
}

fn default_params() -> Mapping {
    Mapping::new()
}
