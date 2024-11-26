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

use std::collections::HashMap;

// use thiserror::Error;

// pub mod extension;

use shinran_config::ProfileRef;
use shinran_render::{CasingStyle, Context, RenderOptions};
use shinran_types::{MatchEffect, MatchIdx, Params, UpperCasingStyle, Value, VarType, Variable};

use crate::{
    engine::RendererError,
    match_cache::{self},
    Stores,
};

pub struct RendererAdapter<'store> {
    pub combined_cache: match_cache::CombinedMatchCache<'store>,
    pub stores: &'store Stores,
}

impl<'store> RendererAdapter<'store> {
    pub fn new(
        combined_cache: crate::match_cache::CombinedMatchCache<'store>,
        stores: &'store Stores,
    ) -> Self {
        Self {
            stores,
            combined_cache,
        }
    }

    /// Get the active configuration file according to the current app.
    ///
    /// This functionality is not implemented yet.
    pub fn active_profile(&self) -> ProfileRef {
        // let current_app = self.app_info_provider.get_info();
        // let info = to_app_properties(&current_app);
        let info = shinran_config::config::AppProperties {
            title: None,
            class: None,
            exec: None,
        };
        self.stores.profiles.active_config(&info)
    }

    pub fn render(
        &self,
        match_id: MatchIdx,
        trigger: Option<&str>,
        trigger_vars: HashMap<String, String>,
        active_profile: ProfileRef,
    ) -> anyhow::Result<String> {
        // let Some(Some(template)) = self.template_map.get(&match_id) else {
        //     // Found no template for the given match ID.
        //     return Err(RendererError::NotFound.into());
        // };

        let (effect, propagate_case, preferred_uppercasing_style) = match match_id {
            MatchIdx::Trigger(idx) => {
                let (expected_triggers, m) = &self.stores.matches.trigger_matches.get(idx);
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
                &self.stores.matches.regex_matches.get(idx).1.effect,
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

        let context = Context {
            matches: &self.stores.matches.trigger_matches,
            matches_map: self.combined_cache.user_match_cache.matches(active_profile),
            global_vars: &self.stores.matches.global_vars,
            global_vars_map: self
                .combined_cache
                .user_match_cache
                .global_vars(active_profile),
        };

        match self
            .stores
            .renderer
            .render_template(template, context, &options)
        {
            shinran_render::RenderResult::Success(body) => Ok(body),
            shinran_render::RenderResult::Aborted => Err(RendererError::Aborted.into()),
            shinran_render::RenderResult::Error(err) => {
                Err(RendererError::RenderingError(err).into())
            }
        }
    }

    #[inline]
    pub fn find_matches_from_trigger(
        &self,
        trigger: &str,
        active_profile: ProfileRef,
    ) -> Vec<crate::engine::DetectedMatch> {
        self.combined_cache
            .find_matches_from_trigger(trigger, active_profile)
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
