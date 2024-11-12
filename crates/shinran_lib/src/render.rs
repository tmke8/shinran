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

use std::{collections::HashMap, sync::RwLock};

// use thiserror::Error;

// pub mod extension;

use espanso_config::config::ProfileId;
use espanso_config::matches::{
    store::MatchesAndGlobalVars, Match, MatchCause, MatchEffect, UpperCasingStyle,
};
use espanso_render::{CasingStyle, Context, RenderOptions, Template, Value, VarType, Variable};

use crate::{
    config::Configuration,
    engine::RendererError,
    match_cache::{self, MatchCache},
};

pub struct RendererAdapter {
    /// Renderer for the variables.
    renderer: espanso_render::Renderer,
    combined_cache: match_cache::CombinedMatchCache,
    /// Configuration of the shinran instance.
    configuration: Configuration,

    /// Map of all templates, indexed by the corresponding match ID.
    template_map: HashMap<i32, Option<Template>>,
    /// Map of all global variables, indexed by the corresponding variable ID.
    global_vars_map: HashMap<i32, Variable>,

    /// Cache for the context objects. We need internal mutability here because we need to
    /// update the cache.
    context_cache: RwLock<HashMap<ProfileId, Context>>,
}

impl RendererAdapter {
    pub fn new(
        combined_cache: crate::match_cache::CombinedMatchCache,
        configuration: Configuration,
        renderer: espanso_render::Renderer,
    ) -> Self {
        let match_cache = &combined_cache.user_match_cache;
        let template_map = generate_template_map(match_cache);
        let global_vars_map = generate_global_vars_map(&configuration);

        Self {
            renderer,
            configuration,
            combined_cache,
            template_map,
            global_vars_map,
            context_cache: RwLock::new(HashMap::new()),
        }
    }
}

// TODO: test
fn generate_template_map(match_cache: &MatchCache) -> HashMap<i32, Option<Template>> {
    let mut template_map = HashMap::new();
    for m in match_cache.matches() {
        let entry = convert_to_template(m);
        // TODO: Why are we inserting entries that are `None`?
        template_map.insert(m.id, entry);
    }
    template_map
}

// TODO: test
fn generate_global_vars_map(configuration: &Configuration) -> HashMap<i32, Variable> {
    let mut global_vars_map = HashMap::new();

    // Variables are stored in match files, so we need to iterate over all match files recursively.
    // We're using `collect_matches_and_global_vars` here under the hood to do this, even though
    // that function is overkill (it also collects all matches, for example).
    // But on the other hand, we don't want to reimplement the recursive logic here.
    for (_, match_set) in configuration.collect_matches_and_global_vars_from_all_configs() {
        for &var in &match_set.global_vars {
            // TODO: Investigate how to avoid this clone.
            global_vars_map
                .entry(var.id)
                .or_insert_with(|| convert_var(var.clone()));
        }
    }

    global_vars_map
}

/// Iterates over the matches in the match set and finds the corresponding templates.
///
/// Analogously, it iterates over the global vars in the match set and finds the corresponding vars.
fn generate_context(
    match_set: MatchesAndGlobalVars,
    template_map: &HashMap<i32, Option<Template>>,
    global_vars_map: &HashMap<i32, Variable>,
) -> Context {
    let mut templates = Vec::new();
    let mut global_vars = Vec::new();

    for m in match_set.matches {
        if let Some(Some(template)) = template_map.get(&m.id) {
            // TODO: Investigate how to avoid this clone.
            templates.push(template.clone());
        }
    }

    for var in match_set.global_vars {
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

fn convert_to_template(m: &Match) -> Option<Template> {
    if let MatchEffect::Text(text_effect) = &m.effect {
        let triggers = if let MatchCause::Trigger(cause) = &m.cause {
            // TODO: Investigate how to avoid this clone.
            cause.triggers.clone()
        } else {
            Vec::new()
        };

        Some(Template {
            triggers,
            // TODO: Investigate how to avoid this clone.
            body: text_effect.replace.clone(),
            // TODO: Investigate how to avoid this clone.
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
    let var_type = match &var.var_type[..] {
        "echo" => VarType::Echo,
        "date" => VarType::Date,
        "shell" => VarType::Shell,
        "script" => VarType::Script,
        // "global" => VarType::Global,
        // "match" => VarType::Match,
        "dummy" => VarType::Echo,
        "random" => VarType::Random,
        // "" => VarType::Match,
        _ => {
            unreachable!()
        }
    };
    Variable {
        name: var.name,
        var_type,
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

// TODO: Investigate whether this is necessary.
//       The only difference between the two `Value` types is the `Object` variant.
//       In espano_config, `Object` is a `BTreeMap<String, Value>`, while in espanso_render it is
//       a `HashMap<String, Value>`.
fn convert_value(value: espanso_config::matches::Value) -> espanso_render::Value {
    match value {
        espanso_config::matches::Value::Null => espanso_render::Value::Null,
        espanso_config::matches::Value::Bool(v) => espanso_render::Value::Bool(v),
        espanso_config::matches::Value::Number(n) => match n {
            espanso_config::matches::Number::Integer(i) => {
                espanso_render::Value::Number(espanso_render::Number::Integer(i))
            }
            espanso_config::matches::Number::Float(f) => {
                espanso_render::Value::Number(espanso_render::Number::Float(f))
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
        let Some(Some(template)) = self.template_map.get(&match_id) else {
            // Found no template for the given match ID.
            return Err(RendererError::NotFound.into());
        };

        let (profile, match_set) = self.configuration.default_profile_and_matches();

        let mut context_cache = self.context_cache.write().unwrap();
        let context = context_cache.entry(profile.id()).or_insert_with(|| {
            generate_context(match_set, &self.template_map, &self.global_vars_map)
        });

        // TODO: We should use `combined_cache.get()` here to also get the built-in matches.
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
                        var_type: VarType::Echo,
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

        match self.renderer.render_template(template, context, &options) {
            espanso_render::RenderResult::Success(body) => Ok(body),
            espanso_render::RenderResult::Aborted => Err(RendererError::Aborted.into()),
            espanso_render::RenderResult::Error(err) => {
                Err(RendererError::RenderingError(err).into())
            }
        }
    }

    #[inline]
    pub fn find_matches_from_trigger(&self, trigger: &str) -> Vec<crate::engine::DetectedMatch> {
        self.combined_cache.find_matches_from_trigger(trigger)
    }

    #[inline]
    pub fn find_regex_matches(&self, trigger: &str) -> Vec<crate::engine::DetectedMatch> {
        self.combined_cache.regex_matcher.find_matches(trigger)
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
