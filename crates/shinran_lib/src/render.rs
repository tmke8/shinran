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
use espanso_config::matches::store::MatchesAndGlobalVars;
use espanso_render::{CasingStyle, Context, RenderOptions};
use shinran_types::{
    BaseMatch, MatchEffect, MatchIdx, Params, TextEffect, TrigMatchStore, UpperCasingStyle, Value,
    VarStore, VarType, Variable,
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
        Self {
            renderer,
            configuration,
            combined_cache,
            context_cache: RwLock::new(HashMap::new()),
        }
    }
}

/// Iterates over the matches in the match set and finds the corresponding templates.
///
/// Analogously, it iterates over the global vars in the match set and finds the corresponding vars.
fn generate_context(
    match_set: MatchesAndGlobalVars,
    template_map: &TrigMatchStore,
    global_vars_map: &VarStore,
) -> Context {
    let mut templates = Vec::new();
    let mut global_vars = Vec::new();

    for match_idx in match_set.trigger_matches {
        let (triggers, m) = template_map.get(match_idx);
        if let Some(template) = convert_to_template(&m.base_match) {
            // TODO: Investigate how to avoid this clone.
            templates.push((triggers.clone(), template));
        }
    }

    for var_id in match_set.global_vars {
        let var = global_vars_map.get(var_id);
        // TODO: Investigate how to avoid this clone.
        global_vars.push(var.clone());
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
            generate_context(
                match_set,
                &self.configuration.match_store.trigger_matches,
                &self.configuration.match_store.global_vars,
            )
        });

        let (effect, propagate_case, preferred_uppercasing_style) = match match_id {
            MatchIdx::Trigger(idx) => {
                let (expected_triggers, m) =
                    &self.configuration.match_store.trigger_matches.get(idx);
                if let Some(trigger) = trigger {
                    // If we are not propagating case, we have to make sure that the trigger matches
                    // one of the expected triggers exactly.
                    if !m.propagate_case && !expected_triggers.iter().any(|t| t == trigger) {
                        return Err(RendererError::NotFound.into());
                    }
                }
                (
                    &m.base_match.effect,
                    m.propagate_case,
                    Some(m.uppercase_style),
                )
            }
            MatchIdx::Regex(idx) => (
                &self
                    .configuration
                    .match_store
                    .regex_matches
                    .get(idx)
                    .1
                    .effect,
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

        if !propagate_case {}

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
