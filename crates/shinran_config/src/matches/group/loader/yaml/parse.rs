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
use compact_str::CompactString;
use serde::Deserialize;
use serde_yaml_ng::Mapping;

use crate::util::is_yaml_empty;

#[derive(Debug, Deserialize)]
pub struct YAMLMatchFile {
    #[serde(default)]
    pub imports: Option<Vec<String>>,

    #[serde(default)]
    pub global_vars: Option<Vec<YAMLVariable>>,

    #[serde(default)]
    pub matches: Option<Vec<YAMLMatch>>,
}

impl YAMLMatchFile {
    pub fn parse_from_str(yaml: &str) -> Result<Self> {
        // Because an empty string is not valid YAML but we want to support it anyway
        if is_yaml_empty(yaml) {
            return Ok(serde_yaml_ng::from_str(
                "arbitrary_field_that_will_not_block_the_parser: true",
            )?);
        }

        Ok(serde_yaml_ng::from_str(yaml)?)
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct YAMLMatch {
    #[serde(default)]
    pub label: Option<String>,

    #[serde(default)]
    pub trigger: Option<CompactString>,

    #[serde(default)]
    pub triggers: Option<Vec<CompactString>>,

    #[serde(default)]
    pub regex: Option<String>,

    #[serde(default)]
    pub replace: Option<String>,

    #[serde(default)]
    pub image_path: Option<String>,

    #[serde(default)]
    pub form: Option<String>,

    #[serde(default)]
    pub form_fields: Option<Mapping>,

    #[serde(default)]
    pub vars: Option<Vec<YAMLVariable>>,

    #[serde(default)]
    pub word: Option<bool>,

    #[serde(default)]
    pub left_word: Option<bool>,

    #[serde(default)]
    pub right_word: Option<bool>,

    #[serde(default)]
    pub propagate_case: Option<bool>,

    #[serde(default)]
    pub uppercase_style: Option<String>,

    #[serde(default)]
    pub force_clipboard: Option<bool>,

    #[serde(default)]
    pub force_mode: Option<String>,

    #[serde(default)]
    pub markdown: Option<String>,

    #[serde(default)]
    pub paragraph: Option<bool>,

    #[serde(default)]
    pub html: Option<String>,

    #[serde(default)]
    pub search_terms: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct YAMLVariable {
    pub name: String,

    #[serde(rename = "type")]
    pub var_type: String,

    #[serde(default = "default_params")]
    pub params: Mapping,

    #[serde(default)]
    pub inject_vars: Option<bool>,

    #[serde(default)]
    pub depends_on: Vec<String>,
}

fn default_params() -> Mapping {
    Mapping::new()
}
