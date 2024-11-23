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

use espanso_config::matches::store::MatchesAndGlobalVars;
use espanso_config::{config::ProfileId, matches::store::MatchStore};
use espanso_render::{CasingStyle, Context, RenderOptions};
use shinran_types::{
    BaseMatch, MatchEffect, MatchIdx, Params, TextEffect, UpperCasingStyle, Value, VarType,
    Variable,
};

use crate::{
    config::Configuration,
    engine::RendererError,
    match_cache::{self},
};

pub struct RendererAdapter {
    /// Renderer for the variables.
    renderer: espanso_render::Renderer,
    combined_cache: match_cache::CombinedMatchCache,
    /// Configuration of the shinran instance.
    configuration: Configuration,

    /// Map of all templates, indexed by the corresponding match ID.
    template_map: HashMap<usize, (Vec<String>, TextEffect)>,
    /// Map of all global variables, indexed by the corresponding variable ID.
    global_vars_map: HashMap<usize, Variable>,

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
        let template_map = generate_template_map(&configuration.match_store);
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
fn generate_template_map(match_store: &MatchStore) -> HashMap<usize, (Vec<String>, TextEffect)> {
    let mut template_map = HashMap::new();
    for (idx, (trig, m)) in match_store.trigger_matches.iter().enumerate() {
        let entry = convert_to_template(&m.base_match);
        let Some(entry) = entry else { continue };
        // TODO: Why are we inserting entries that are `None`?
        template_map.insert(idx, (trig.clone(), entry));
    }
    template_map
}

// TODO: test
fn generate_global_vars_map(configuration: &Configuration) -> HashMap<usize, Variable> {
    let mut global_vars_map: HashMap<usize, Variable> = HashMap::new();

    // Variables are stored in match files, so we need to iterate over all match files recursively.
    // We're using `collect_matches_and_global_vars` here under the hood to do this, even though
    // that function is overkill (it also collects all matches, for example).
    // But on the other hand, we don't want to reimplement the recursive logic here.
    for (_, match_set) in configuration.collect_matches_and_global_vars_from_all_configs() {
        for &var_index in &match_set.global_vars {
            let var = &configuration.match_store.global_vars[var_index];
            // TODO: Investigate how to avoid this clone.
            global_vars_map
                .entry(var_index)
                .or_insert_with(|| var.clone());
        }
    }

    global_vars_map
}

/// Iterates over the matches in the match set and finds the corresponding templates.
///
/// Analogously, it iterates over the global vars in the match set and finds the corresponding vars.
fn generate_context(
    match_set: MatchesAndGlobalVars,
    template_map: &HashMap<usize, (Vec<String>, TextEffect)>,
    global_vars_map: &HashMap<usize, Variable>,
) -> Context {
    let mut templates = Vec::new();
    let mut global_vars = Vec::new();

    for match_idx in match_set.trigger_matches {
        if let Some(template) = template_map.get(&match_idx) {
            // TODO: Investigate how to avoid this clone.
            templates.push(template.clone());
        }
    }

    for var_id in match_set.global_vars {
        if let Some(var) = global_vars_map.get(&var_id) {
            // TODO: Investigate how to avoid this clone.
            global_vars.push(var.clone());
        }
    }

    Context {
        global_vars,
        templates,
    }
}

// This function does little more than clone some fields of the given match.
// TODO: Remove this function.
fn convert_to_template(m: &BaseMatch) -> Option<TextEffect> {
    if let MatchEffect::Text(text_effect) = &m.effect {
        // TODO: Investigate how to avoid this clone.
        Some(text_effect.clone())
    } else {
        None
    }
}

// fn convert_vars(vars: Vec<espanso_config::matches::Variable>) -> Vec<espanso_render::Variable> {
//     vars.into_iter().map(convert_var).collect()
// }

// fn convert_var(var: espanso_config::matches::Variable) -> espanso_render::Variable {
//     let var_type = match &var.var_type[..] {
//         "echo" => VarType::Echo,
//         "date" => VarType::Date,
//         "shell" => VarType::Shell,
//         "script" => VarType::Script,
//         // "global" => VarType::Global,
//         // "match" => VarType::Match,
//         "dummy" => VarType::Echo,
//         "random" => VarType::Random,
//         // "" => VarType::Match,
//         _ => {
//             unreachable!()
//         }
//     };
//     Variable {
//         name: var.name,
//         var_type,
//         params: convert_params(var.params),
//         inject_vars: var.inject_vars,
//         depends_on: var.depends_on,
//     }
// }

// // The difference between the two `Params` types is that one uses `BTreeMap` and the other uses
// // `HashMap`.
// fn convert_params(params: espanso_config::matches::Params) -> espanso_render::Params {
//     let mut new_params = espanso_render::Params::new();
//     for (key, value) in params {
//         new_params.insert(key, convert_value(value));
//     }
//     new_params
// }

// // TODO: Investigate whether this is necessary.
// //       The only difference between the two `Value` types is the `Object` variant.
// //       In espano_config, `Object` is a `BTreeMap<String, Value>`, while in espanso_render it is
// //       a `HashMap<String, Value>`.
// fn convert_value(value: espanso_config::matches::Value) -> espanso_render::Value {
//     match value {
//         espanso_config::matches::Value::Null => espanso_render::Value::Null,
//         espanso_config::matches::Value::Bool(v) => espanso_render::Value::Bool(v),
//         espanso_config::matches::Value::Number(n) => match n {
//             espanso_config::matches::Number::Integer(i) => {
//                 espanso_render::Value::Number(espanso_render::Number::Integer(i))
//             }
//             espanso_config::matches::Number::Float(f) => {
//                 espanso_render::Value::Number(espanso_render::Number::Float(f))
//             }
//         },
//         espanso_config::matches::Value::String(s) => espanso_render::Value::String(s),
//         espanso_config::matches::Value::Array(v) => {
//             espanso_render::Value::Array(v.into_iter().map(convert_value).collect())
//         }
//         espanso_config::matches::Value::Object(params) => {
//             espanso_render::Value::Object(convert_params(params))
//         }
//     }
// }

impl RendererAdapter {
    pub fn render(
        &self,
        match_id: MatchIdx,
        trigger: Option<&str>,
        trigger_vars: HashMap<String, String>,
    ) -> anyhow::Result<String> {
        // let Some(Some(template)) = self.template_map.get(&match_id) else {
        //     // Found no template for the given match ID.
        //     return Err(RendererError::NotFound.into());
        // };

        let (profile, match_set) = self.configuration.default_profile_and_matches();

        let mut context_cache = self.context_cache.write().unwrap();
        let context = context_cache.entry(profile.id()).or_insert_with(|| {
            generate_context(match_set, &self.template_map, &self.global_vars_map)
        });

        let (effect, propagate_case, preferred_uppercasing_style) = match match_id {
            MatchIdx::Trigger(idx) => {
                let m = &self.configuration.match_store.trigger_matches[idx].1;
                (
                    &m.base_match.effect,
                    m.propagate_case,
                    Some(m.uppercase_style),
                )
            }
            MatchIdx::Regex(idx) => (
                &self.configuration.match_store.regex_matches[idx].1.effect,
                false,
                None,
            ),
            MatchIdx::BuiltIn(_) => {
                unreachable!()
            }
        };

        let MatchEffect::Text(text_effect) = effect else {
            // TODO: This function should maybe directly receive a `TextEffect` object.
            return Err(RendererError::NotFound.into());
        };

        // TODO: We should use `combined_cache.get()` here to also get the built-in matches.
        // let raw_match = self.combined_cache.user_match_cache.get(match_id);
        // let propagate_case = raw_match.is_some_and(is_propagate_case);
        // let preferred_uppercasing_style = raw_match.and_then(extract_uppercasing_style);

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
            let mut augmented = text_effect.clone();
            for (name, value) in trigger_vars {
                let mut params = Params::new();
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
            text_effect
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
