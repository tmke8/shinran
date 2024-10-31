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

use std::{cell::RefCell, collections::HashMap};

// use thiserror::Error;

// pub mod extension;

use espanso_config::matches::{store::MatchSet, Match, MatchCause, MatchEffect, UpperCasingStyle};
use espanso_render::{CasingStyle, Context, RenderOptions, Template, Value, Variable};

use crate::{
    config::ConfigManager,
    engine::RendererError,
    match_cache::{self, MatchCache},
};
// use espanso_engine::process::{Renderer, RendererError};

// pub trait MatchProvider<'a> {
//     fn matches(&self) -> Vec<&'a Match>;
//     fn get(&self, id: i32) -> Option<&'a Match>;
// }

// pub trait ConfigProvider<'a> {
//     fn configs(&self) -> Vec<(Arc<ResolvedConfig>, MatchSet)>;
//     fn active(&self) -> (Arc<ResolvedConfig>, MatchSet);
// }

pub struct RendererAdapter {
    renderer: espanso_render::Renderer,
    pub combined_cache: match_cache::CombinedMatchCache,
    config_manager: ConfigManager,

    template_map: HashMap<i32, Option<Template>>,
    global_vars_map: HashMap<i32, Variable>,

    context_cache: RefCell<HashMap<i32, Context>>,
}

impl<'a> RendererAdapter {
    pub fn new(
        combined_cache: crate::match_cache::CombinedMatchCache,
        config_manager: ConfigManager,
        renderer: espanso_render::Renderer,
    ) -> Self {
        let match_cache = &combined_cache.user_match_cache;
        let template_map = generate_template_map(match_cache);
        let global_vars_map = generate_global_vars_map(&config_manager);

        Self {
            renderer,
            config_manager,
            combined_cache,
            template_map,
            global_vars_map,
            context_cache: RefCell::new(HashMap::new()),
        }
    }
}

// TODO: test
fn generate_template_map(match_provider: &MatchCache) -> HashMap<i32, Option<Template>> {
    let mut template_map = HashMap::new();
    for m in match_provider.matches() {
        let entry = convert_to_template(m);
        template_map.insert(m.id, entry);
    }
    template_map
}

// TODO: test
fn generate_global_vars_map(config_provider: &ConfigManager) -> HashMap<i32, Variable> {
    let mut global_vars_map = HashMap::new();

    for (_, match_set) in config_provider.configs() {
        for var in &match_set.global_vars {
            global_vars_map
                .entry(var.id)
                .or_insert_with(|| convert_var((*var).clone()));
        }
    }

    global_vars_map
}

// TODO: test
fn generate_context(
    match_set: &MatchSet,
    template_map: &HashMap<i32, Option<Template>>,
    global_vars_map: &HashMap<i32, Variable>,
) -> Context {
    let mut templates = Vec::new();
    let mut global_vars = Vec::new();

    for m in &match_set.matches {
        if let Some(Some(template)) = template_map.get(&m.id) {
            // TODO: Investigate how to avoid this clone.
            templates.push(template.clone());
        }
    }

    for var in &match_set.global_vars {
        if let Some(var) = global_vars_map.get(&var.id) {
            // TODO: Investigate how to avoid this clone.
            global_vars.push(var.clone());
        }
    }

    Context {
        global_vars,
        templates,
    }
}

// TODO: move conversion methods to new file?

fn convert_to_template(m: &Match) -> Option<Template> {
    if let MatchEffect::Text(text_effect) = &m.effect {
        let ids = if let MatchCause::Trigger(cause) = &m.cause {
            cause.triggers.clone()
        } else {
            Vec::new()
        };

        Some(Template {
            ids,
            body: text_effect.replace.clone(),
            vars: convert_vars(text_effect.vars.clone()),
        })
    } else {
        None
    }
}

fn convert_vars(vars: Vec<espanso_config::matches::Variable>) -> Vec<espanso_render::Variable> {
    vars.into_iter().map(convert_var).collect()
}

fn convert_var(var: espanso_config::matches::Variable) -> espanso_render::Variable {
    Variable {
        name: var.name,
        var_type: var.var_type,
        params: convert_params(var.params),
        inject_vars: var.inject_vars,
        depends_on: var.depends_on,
    }
}

fn convert_params(params: espanso_config::matches::Params) -> espanso_render::Params {
    let mut new_params = espanso_render::Params::new();
    for (key, value) in params {
        new_params.insert(key, convert_value(value));
    }
    new_params
}

fn convert_value(value: espanso_config::matches::Value) -> espanso_render::Value {
    match value {
        espanso_config::matches::Value::Null => espanso_render::Value::Null,
        espanso_config::matches::Value::Bool(v) => espanso_render::Value::Bool(v),
        espanso_config::matches::Value::Number(n) => match n {
            espanso_config::matches::Number::Integer(i) => {
                espanso_render::Value::Number(espanso_render::Number::Integer(i))
            }
            espanso_config::matches::Number::Float(f) => {
                espanso_render::Value::Number(espanso_render::Number::Float(f.into_inner()))
            }
        },
        espanso_config::matches::Value::String(s) => espanso_render::Value::String(s),
        espanso_config::matches::Value::Array(v) => {
            espanso_render::Value::Array(v.into_iter().map(convert_value).collect())
        }
        espanso_config::matches::Value::Object(params) => {
            espanso_render::Value::Object(convert_params(params))
        }
    }
}

impl RendererAdapter {
    pub fn render(
        &self,
        match_id: i32,
        trigger: Option<&str>,
        trigger_vars: HashMap<String, String>,
    ) -> anyhow::Result<String> {
        if let Some(Some(template)) = self.template_map.get(&match_id) {
            let (config, match_set) = self.config_manager.active_context();

            let mut context_cache = self.context_cache.borrow_mut();
            let context = context_cache.entry(config.id()).or_insert_with(|| {
                generate_context(&match_set, &self.template_map, &self.global_vars_map)
            });

            let raw_match = self.combined_cache.user_match_cache.get(match_id);
            let propagate_case = raw_match.is_some_and(is_propagate_case);
            let preferred_uppercasing_style = raw_match.and_then(extract_uppercasing_style);

            let options = RenderOptions {
                casing_style: if !propagate_case {
                    CasingStyle::None
                } else if let Some(trigger) = trigger {
                    calculate_casing_style(trigger, preferred_uppercasing_style)
                } else {
                    CasingStyle::None
                },
            };

            // If some trigger vars are specified, augment the template with them
            let augmented_template = if trigger_vars.is_empty() {
                None
            } else {
                let mut augmented = template.clone();
                for (name, value) in trigger_vars {
                    let mut params = espanso_render::Params::new();
                    params.insert("echo".to_string(), Value::String(value));
                    augmented.vars.insert(
                        0,
                        Variable {
                            name,
                            var_type: "echo".to_string(),
                            params,
                            inject_vars: false,
                            ..Default::default()
                        },
                    );
                }
                Some(augmented)
            };

            let template = if let Some(augmented) = augmented_template.as_ref() {
                augmented
            } else {
                template
            };

            match self.renderer.render(template, context, &options) {
                espanso_render::RenderResult::Success(body) => Ok(body),
                espanso_render::RenderResult::Aborted => Err(RendererError::Aborted.into()),
                espanso_render::RenderResult::Error(err) => {
                    Err(RendererError::RenderingError(err).into())
                }
            }
        } else {
            Err(RendererError::NotFound.into())
        }
    }
}

fn extract_uppercasing_style(m: &Match) -> Option<UpperCasingStyle> {
    if let MatchCause::Trigger(cause) = &m.cause {
        Some(cause.uppercase_style.clone())
    } else {
        None
    }
}

fn is_propagate_case(m: &Match) -> bool {
    if let MatchCause::Trigger(cause) = &m.cause {
        cause.propagate_case
    } else {
        false
    }
}

// TODO: test
fn calculate_casing_style(
    trigger: &str,
    uppercasing_style: Option<UpperCasingStyle>,
) -> CasingStyle {
    let mut first_alphabetic = None;
    let mut second_alphabetic = None;

    for c in trigger.chars() {
        if c.is_alphabetic() {
            if first_alphabetic.is_none() {
                first_alphabetic = Some(c);
            } else if second_alphabetic.is_none() {
                second_alphabetic = Some(c);
            } else {
                break;
            }
        }
    }

    if let Some(first) = first_alphabetic {
        if let Some(second) = second_alphabetic {
            if first.is_uppercase() {
                if second.is_uppercase() {
                    CasingStyle::Uppercase
                } else {
                    match uppercasing_style {
                        Some(UpperCasingStyle::CapitalizeWords) => CasingStyle::CapitalizeWords,
                        _ => CasingStyle::Capitalize,
                    }
                }
            } else {
                CasingStyle::None
            }
        } else if first.is_uppercase() {
            match uppercasing_style {
                Some(UpperCasingStyle::Capitalize) => CasingStyle::Capitalize,
                Some(UpperCasingStyle::CapitalizeWords) => CasingStyle::CapitalizeWords,
                _ => CasingStyle::Uppercase,
            }
        } else {
            CasingStyle::None
        }
    } else {
        CasingStyle::None
    }
}
