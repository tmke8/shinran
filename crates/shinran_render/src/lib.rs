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

use enum_as_inner::EnumAsInner;
use shinran_types::{Params, TriggerMatch, Variable};
use std::collections::HashMap;

pub mod extension;
mod renderer;

pub use renderer::Renderer;

// pub trait Renderer {
//     fn render(
//         &self,
//         template: &Template,
//         context: &Context,
//         options: &RenderOptions,
//     ) -> RenderResult;
// }

#[derive(Debug)]
pub enum RenderResult {
    Success(String),
    Aborted,
    Error(anyhow::Error),
}

#[derive(Debug, Clone, Copy)]
pub struct Context<'a, 'store> {
    pub matches_map: &'a HashMap<&'store str, &'store TriggerMatch>,
    pub global_vars_map: &'a HashMap<&'store str, &'store Variable>,
}

static DEFAULT_CONTEXT: std::sync::LazyLock<(
    HashMap<&'static str, &'static TriggerMatch>,
    HashMap<&'static str, &'static Variable>,
)> = std::sync::LazyLock::new(|| (HashMap::new(), HashMap::new()));

impl Default for Context<'static, 'static> {
    fn default() -> Self {
        let values = &*DEFAULT_CONTEXT;
        Self {
            matches_map: &values.0,
            global_vars_map: &values.1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderOptions {
    pub casing_style: CasingStyle,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            casing_style: CasingStyle::None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CasingStyle {
    None,
    Capitalize,
    CapitalizeWords,
    Uppercase,
}

pub trait Extension {
    fn name(&self) -> &str;
    fn calculate(&self, scope: &Scope, params: &Params) -> ExtensionResult;
}

pub type Scope<'a> = HashMap<&'a str, ExtensionOutput>;

#[derive(Debug, PartialEq, Eq)]
pub enum ExtensionOutput {
    Single(String),
    Multiple(HashMap<String, String>),
}

#[derive(Debug, EnumAsInner)]
pub enum ExtensionResult {
    Success(ExtensionOutput),
    Aborted,
    Error(anyhow::Error),
}
