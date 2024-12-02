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

use std::{borrow::Cow, path::Path, sync::LazyLock};

use crate::{
    extension::{
        date::DateExtension, echo::EchoExtension, random::RandomExtension, script::ScriptExtension,
        shell::ShellExtension,
    },
    CasingStyle, Context, Extension, ExtensionOutput, ExtensionResult, RenderOptions, RenderResult,
    Scope,
};
use log::{error, warn};
use regex::{Captures, Regex};
use shinran_types::{MatchEffect, Params, TextEffect, Value, VarType, Variable};
use thiserror::Error;

use self::util::{inject_variables_into_params, render_variables};

mod resolve;
mod util;

pub(crate) static VAR_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{\s*((?P<name>\w+)(\.(?P<subname>(\w+)))?)\s*\}\}").unwrap());
static WORD_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\w+)").unwrap());

pub struct Renderer<M: Extension = NoOpExtension> {
    date_extension: DateExtension,
    echo_extension: EchoExtension,
    shell_extension: ShellExtension,
    script_extension: ScriptExtension,
    random_extension: RandomExtension,
    mock_extension: M,
}

pub struct NoOpExtension;

impl Extension for NoOpExtension {
    fn name(&self) -> &'static str {
        "NoOp"
    }

    fn calculate(&self, _scope: &Scope, _params: &Params) -> ExtensionResult {
        ExtensionResult::Aborted
    }
}

impl Renderer<NoOpExtension> {
    pub fn new(base_path: &Path, home_path: &Path, packages_path: &Path) -> Self {
        Self {
            date_extension: DateExtension::new(),
            echo_extension: EchoExtension::new(),
            shell_extension: ShellExtension::new(base_path),
            script_extension: ScriptExtension::new(base_path, home_path, packages_path),
            random_extension: RandomExtension::new(),
            mock_extension: NoOpExtension,
        }
    }
}

impl<M: Extension> Renderer<M> {
    pub fn render_template(
        &self,
        template: &TextEffect,
        context: Context,
        options: &RenderOptions,
    ) -> RenderResult {
        let body = if VAR_REGEX.is_match(&template.body) {
            // Resolve unresolved variables with global variables, if necessary.
            // TODO: Find out whether this code can actually ever be triggered.
            let local_variables: Vec<&Variable> = if template
                .vars
                .iter()
                .any(|var| matches!(var.var_type, VarType::Unresolved))
            {
                template
                    .vars
                    .iter()
                    .filter_map(|var| {
                        if matches!(var.var_type, VarType::Unresolved) {
                            // Try to resolve it with a global variable.
                            context.global_vars_map.get(&*var.name).copied()
                        } else {
                            Some(var)
                        }
                    })
                    .collect()
            } else {
                template.vars.iter().collect()
            };

            // Here we execute a graph dependency resolution algorithm to determine a valid
            // evaluation order for variables.
            let global_vars = context
                .global_vars_map
                .values()
                .copied()
                .collect::<Vec<_>>();
            let variables = match resolve::resolve_evaluation_order(
                &template.body,
                &local_variables,
                global_vars.as_slice(),
            ) {
                Ok(variables) => variables,
                Err(err) => return RenderResult::Error(err),
            };

            // Compute the variable outputs
            let mut scope = Scope::new();
            for variable in variables {
                if matches!(variable.var_type, VarType::Match) {
                    // Recursive call
                    // Call render recursively
                    let sub_template = get_trigger_from_var(variable)
                        .and_then(|trigger| context.matches_map.get(trigger).copied())
                        .map(|match_| &match_.base_match.effect);
                    let Some(MatchEffect::Text(sub_template)) = sub_template else {
                        error!("unable to find sub-match: {}", variable.name);
                        return RenderResult::Error(RendererError::MissingSubMatch.into());
                    };
                    match self.render_template(sub_template, context, options) {
                        RenderResult::Success(output) => {
                            scope.insert(&variable.name, ExtensionOutput::Single(output));
                        }
                        result => return result,
                    }
                    continue;
                };

                let variable_params = if variable.inject_vars {
                    match inject_variables_into_params(&variable.params, &scope) {
                        Ok(augmented_params) => Cow::Owned(augmented_params),
                        Err(err) => {
                            error!(
                                "unable to inject variables into params of variable '{}': {}",
                                variable.name, err
                            );

                            // if variable.var_type == "form" {
                            //     if let Some(RendererError::MissingVariable(_)) =
                            //         err.downcast_ref::<RendererError>()
                            //     {
                            //         log_new_form_syntax_tip();
                            //     }
                            // }

                            return RenderResult::Error(err);
                        }
                    }
                } else {
                    Cow::Borrowed(&variable.params)
                };

                let extension_result = match &variable.var_type {
                    VarType::Date => self.date_extension.calculate(&scope, &variable_params),
                    VarType::Echo => self.echo_extension.calculate(&scope, &variable_params),
                    VarType::Shell => self.shell_extension.calculate(&scope, &variable_params),
                    VarType::Script => self.script_extension.calculate(&scope, &variable_params),
                    VarType::Random => self.random_extension.calculate(&scope, &variable_params),
                    VarType::Mock => self.mock_extension.calculate(&scope, &variable_params),
                    VarType::Form => {
                        // Do nothing.
                        return RenderResult::Success("".to_string());
                    }
                    VarType::Unresolved | VarType::Match => {
                        unreachable!()
                    }
                };

                match extension_result {
                    ExtensionResult::Success(output) => {
                        scope.insert(&variable.name, output);
                    }
                    ExtensionResult::Aborted => {
                        warn!(
                            "rendering was aborted by extension: {:?}, on var: {}",
                            variable.var_type, variable.name
                        );
                        return RenderResult::Aborted;
                    }
                    ExtensionResult::Error(err) => {
                        warn!(
                            "extension '{:?}' on var: '{}' reported an error: {}",
                            variable.var_type, variable.name, err
                        );
                        return RenderResult::Error(err);
                    }
                }
            }

            // Replace the variables
            match render_variables(&template.body, &scope) {
                Ok(output) => output,
                Err(error) => {
                    return RenderResult::Error(error);
                }
            }
        } else {
            template.body.clone()
        };

        let body = util::unescape_variable_inections(&body);

        // Process the casing style
        let body_with_casing = match options.casing_style {
            CasingStyle::None => body,
            CasingStyle::Uppercase => body.to_uppercase(),
            CasingStyle::Capitalize => {
                // Capitalize the first letter
                let mut v: Vec<char> = body.chars().collect();
                v[0] = v[0].to_uppercase().next().unwrap();
                v.into_iter().collect()
            }
            CasingStyle::CapitalizeWords => {
                // Capitalize the first letter of each word
                WORD_REGEX
                    .replace_all(&body, |caps: &Captures| {
                        if let Some(word_match) = caps.get(0) {
                            let mut v: Vec<char> = word_match.as_str().chars().collect();
                            v[0] = v[0].to_uppercase().next().unwrap();
                            let capitalized_word: String = v.into_iter().collect();
                            capitalized_word
                        } else {
                            String::new()
                        }
                    })
                    .to_string()
            }
        };

        RenderResult::Success(body_with_casing)
    }
}

fn get_trigger_from_var(variable: &Variable) -> Option<&str> {
    let trigger = variable.params.get("trigger")?;
    if let Value::String(trigger) = trigger {
        Some(trigger)
    } else {
        None
    }
}

#[derive(Error, Debug)]
pub enum RendererError {
    #[error("missing variable: `{0}`")]
    MissingVariable(String),

    #[error("missing sub match")]
    MissingSubMatch,

    #[error("circular dependency: `{0}` -> `{1}`")]
    CircularDependency(String, String),
}

#[cfg(test)]
mod tests {

    use compact_str::CompactString;
    use shinran_types::{BaseMatch, Params, TextFormat, TriggerMatch, Variable};

    use super::*;
    use std::{collections::HashMap, iter::FromIterator};

    struct MockExtension {}

    impl Extension for MockExtension {
        fn name(&self) -> &str {
            "mock"
        }

        fn calculate(&self, scope: &Scope, params: &Params) -> ExtensionResult {
            if let Some(Value::String(string)) = params.get("echo") {
                return ExtensionResult::Success(ExtensionOutput::Single(string.clone()));
            }
            if let (Some(Value::String(name)), Some(Value::String(value))) =
                (params.get("name"), params.get("value"))
            {
                let mut map = HashMap::new();
                map.insert(name.to_string(), value.to_string());
                return ExtensionResult::Success(ExtensionOutput::Multiple(map));
            }
            // If the "read" param is present, echo the value of the corresponding result in the scope
            if let Some(Value::String(string)) = params.get("read") {
                if let Some(ExtensionOutput::Single(value)) = scope.get(string.as_str()) {
                    return ExtensionResult::Success(ExtensionOutput::Single(value.to_string()));
                }
            }
            if params.get("abort").is_some() {
                return ExtensionResult::Aborted;
            }
            if params.get("error").is_some() {
                return ExtensionResult::Error(
                    RendererError::MissingVariable("missing".to_string()).into(),
                );
            }
            ExtensionResult::Aborted
        }
    }

    fn get_renderer() -> Renderer<MockExtension> {
        Renderer::<MockExtension> {
            date_extension: DateExtension::new(),
            echo_extension: EchoExtension::new(),
            shell_extension: ShellExtension::new(Path::new(".")),
            script_extension: ScriptExtension::new(Path::new("."), Path::new("."), Path::new(".")),
            random_extension: RandomExtension::new(),
            mock_extension: MockExtension {},
        }
    }

    pub fn template_for_str(str: &str) -> TextEffect {
        TextEffect {
            body: str.to_string(),
            vars: Vec::new(),
            format: TextFormat::Plain,
            force_mode: None,
        }
    }

    pub fn template(body: &str, vars: &[(&str, &str)]) -> TextEffect {
        let vars = vars
            .iter()
            .map(|(name, value)| Variable {
                name: (*name).to_string(),
                var_type: VarType::Mock,
                params: vec![("echo".to_string(), Value::String((*value).to_string()))]
                    .into_iter()
                    .collect::<Params>(),
                ..Default::default()
            })
            .collect();
        TextEffect {
            body: body.to_string(),
            vars,
            format: TextFormat::Plain,
            force_mode: None,
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct MyVariable {
        pub name: &'static str,
        pub var_type: VarType,
        pub params: Params,
        pub inject_vars: bool,
        pub depends_on: Vec<&'static str>,
    }

    impl Default for MyVariable {
        fn default() -> Self {
            Self {
                name: "",
                var_type: VarType::Mock,
                params: Params::new(),
                inject_vars: true,
                depends_on: Vec::new(),
            }
        }
    }

    impl From<MyVariable> for Variable {
        fn from(v: MyVariable) -> Self {
            Variable {
                name: v.name.to_string(),
                var_type: v.var_type,
                params: v.params,
                inject_vars: v.inject_vars,
                depends_on: v.depends_on.into_iter().map(|s| s.to_string()).collect(),
            }
        }
    }

    struct MyContext<'a> {
        matches_map: HashMap<&'a str, &'a TriggerMatch>,
        global_vars_map: HashMap<&'a str, &'a Variable>,
    }

    impl<'a> MyContext<'a> {
        pub fn new(vars: &[&'a Variable], ms: &[&'a TriggerMatch]) -> MyContext<'a> {
            let mut global_vars_map = HashMap::new();
            for var in vars {
                let var_name = &var.name;
                global_vars_map.insert(var_name.as_str(), *var);
            }
            let mut matches_map = HashMap::new();
            for m in ms {
                let triggers = &m.triggers;
                for trigger in triggers {
                    matches_map.insert(trigger.as_str(), *m);
                }
            }
            MyContext {
                matches_map,
                global_vars_map,
            }
        }

        pub fn as_context(&self) -> Context {
            Context {
                matches_map: &self.matches_map,
                global_vars_map: &self.global_vars_map,
            }
        }
    }

    #[test]
    fn no_variable_no_styling() {
        let renderer = get_renderer();
        let res = renderer.render_template(
            &template_for_str("plain body"),
            Context::default(),
            &RenderOptions::default(),
        );
        assert!(matches!(res, RenderResult::Success(str) if str == "plain body"));
    }

    #[test]
    fn no_variable_capitalize() {
        let renderer = get_renderer();
        let res = renderer.render_template(
            &template_for_str("plain body"),
            Context::default(),
            &RenderOptions {
                casing_style: CasingStyle::Capitalize,
            },
        );
        assert!(matches!(res, RenderResult::Success(str) if str == "Plain body"));
    }

    #[test]
    fn no_variable_capitalize_words() {
        let renderer = get_renderer();
        let res = renderer.render_template(
            &template_for_str("ordinary least squares, with other.punctuation !Marks"),
            Context::default(),
            &RenderOptions {
                casing_style: CasingStyle::CapitalizeWords,
            },
        );
        assert!(
            matches!(res, RenderResult::Success(str) if str == "Ordinary Least Squares, With Other.Punctuation !Marks")
        );
    }

    #[test]
    fn no_variable_uppercase() {
        let renderer = get_renderer();
        let res = renderer.render_template(
            &template_for_str("plain body"),
            Context::default(),
            &RenderOptions {
                casing_style: CasingStyle::Uppercase,
            },
        );
        assert!(matches!(res, RenderResult::Success(str) if str == "PLAIN BODY"));
    }

    #[test]
    fn basic_variable() {
        let renderer = get_renderer();
        let template = template("hello {{var}}", &[("var", "world")]);
        let res =
            renderer.render_template(&template, Context::default(), &RenderOptions::default());
        assert!(matches!(res, RenderResult::Success(str) if str == "hello world"));
    }

    #[test]
    fn dict_variable_variable() {
        let renderer = get_renderer();
        let template = TextEffect {
            body: "hello {{var.nested}}".to_string(),
            vars: vec![Variable {
                name: "var".to_string(),
                var_type: VarType::Mock,
                params: vec![
                    ("name".to_string(), Value::String("nested".to_string())),
                    ("value".to_string(), Value::String("dict".to_string())),
                ]
                .into_iter()
                .collect::<Params>(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let res =
            renderer.render_template(&template, Context::default(), &RenderOptions::default());
        assert!(matches!(res, RenderResult::Success(str) if str == "hello dict"));
    }

    #[test]
    fn missing_variable() {
        let renderer = get_renderer();
        let template = template_for_str("hello {{var}}");
        let res =
            renderer.render_template(&template, Context::default(), &RenderOptions::default());
        assert!(matches!(res, RenderResult::Error(_)));
    }

    #[test]
    fn global_variable() {
        let renderer = get_renderer();
        let template = template("hello {{var}}", &[]);
        let var1 = &Variable {
            name: "var".to_string(),
            var_type: VarType::Mock,
            params: Params::from_iter(vec![(
                "echo".to_string(),
                Value::String("world".to_string()),
            )]),
            ..Default::default()
        };
        let global_vars = MyContext::new(&[var1], &[]);
        let res = renderer.render_template(
            &template,
            global_vars.as_context(),
            &RenderOptions::default(),
        );
        assert!(matches!(res, RenderResult::Success(str) if str == "hello world"));
    }

    #[test]
    fn global_dict_variable() {
        let renderer = get_renderer();
        let template = template("hello {{var.nested}}", &[]);
        let var1 = &Variable {
            name: "var".to_string(),
            var_type: VarType::Mock,
            params: vec![
                ("name".to_string(), Value::String("nested".to_string())),
                ("value".to_string(), Value::String("dict".to_string())),
            ]
            .into_iter()
            .collect::<Params>(),
            ..Default::default()
        };
        let global_vars = MyContext::new(&[var1], &[]);
        let res = renderer.render_template(
            &template,
            global_vars.as_context(),
            &RenderOptions::default(),
        );
        assert!(matches!(res, RenderResult::Success(str) if str == "hello dict"));
    }

    #[test]
    fn global_variable_explicit_ordering() {
        let renderer = get_renderer();
        let template = TextEffect {
            body: "hello {{var}} {{local}}".to_string(),
            vars: vec![
                Variable {
                    name: "local".to_string(),
                    var_type: VarType::Mock,
                    params: vec![("echo".to_string(), Value::String("Bob".to_string()))]
                        .into_iter()
                        .collect::<Params>(),
                    ..Default::default()
                },
                Variable {
                    name: "var".to_string(),
                    var_type: VarType::Unresolved,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let var1 = &Variable {
            name: "var".to_string(),
            var_type: VarType::Mock,
            params: Params::from_iter(vec![(
                "read".to_string(),
                Value::String("local".to_string()),
            )]),
            ..Default::default()
        };
        let global_vars = MyContext::new(&[var1], &[]);
        let res = renderer.render_template(
            &template,
            global_vars.as_context(),
            &RenderOptions::default(),
        );
        match res {
            RenderResult::Success(str) => {
                assert_eq!(str, "hello Bob Bob");
            }
            _ => panic!("unexpected result: {res:?}"),
        }
    }

    #[test]
    fn nested_global_variable() {
        let renderer = get_renderer();
        let template = template("hello {{var2}}", &[]);
        let var1 = &Variable {
            name: "var".to_string(),
            var_type: VarType::Mock,
            params: Params::from_iter(vec![(
                "echo".to_string(),
                Value::String("world".to_string()),
            )]),
            ..Default::default()
        };
        let var2 = &Variable {
            name: "var2".to_string(),
            var_type: VarType::Mock,
            params: Params::from_iter(vec![(
                "echo".to_string(),
                Value::String("{{var}}".to_string()),
            )]),
            ..Default::default()
        };
        let global_vars = MyContext::new(&[var1, var2], &[]);
        let res = renderer.render_template(
            &template,
            global_vars.as_context(),
            &RenderOptions::default(),
        );
        assert!(matches!(res, RenderResult::Success(str) if str == "hello world"));
    }

    #[test]
    fn nested_global_variable_circular_dependency_should_fail() {
        let renderer = get_renderer();
        let template = template("hello {{var}}", &[]);
        let var1 = &Variable {
            name: "var".to_string(),
            var_type: VarType::Mock,
            params: Params::from_iter(vec![(
                "echo".to_string(),
                Value::String("{{var2}}".to_string()),
            )]),
            ..Default::default()
        };
        let var2 = &Variable {
            name: "var2".to_string(),
            var_type: VarType::Mock,
            params: Params::from_iter(vec![(
                "echo".to_string(),
                Value::String("{{var3}}".to_string()),
            )]),
            ..Default::default()
        };
        let var3 = &Variable {
            name: "var3".to_string(),
            var_type: VarType::Mock,
            params: Params::from_iter(vec![(
                "echo".to_string(),
                Value::String("{{var}}".to_string()),
            )]),
            ..Default::default()
        };
        let global_vars = MyContext::new(&[var1, var2, var3], &[]);
        let res = renderer.render_template(
            &template,
            global_vars.as_context(),
            &RenderOptions::default(),
        );
        assert!(matches!(res, RenderResult::Error(_)));
    }

    #[test]
    fn global_variable_depends_on() {
        let renderer = get_renderer();
        let template = template("hello {{var}}", &[]);
        let var1 = &Variable {
            name: "var".to_string(),
            var_type: VarType::Mock,
            params: Params::from_iter(vec![(
                "echo".to_string(),
                Value::String("world".to_string()),
            )]),
            depends_on: vec!["var2".to_string()],
            ..Default::default()
        };
        let var2 = &Variable {
            name: "var2".to_string(),
            var_type: VarType::Mock,
            params: Params::from_iter(vec![("abort".to_string(), Value::Null)]),
            ..Default::default()
        };
        let global_vars = MyContext::new(&[var1, var2], &[]);
        let res = renderer.render_template(
            &template,
            global_vars.as_context(),
            &RenderOptions::default(),
        );
        assert!(matches!(res, RenderResult::Aborted));
    }

    #[test]
    fn local_variable_explicit_ordering() {
        let renderer = get_renderer();
        let template = TextEffect {
            body: "hello {{var}}".to_string(),
            vars: vec![Variable {
                name: "var".to_string(),
                var_type: VarType::Mock,
                params: vec![("echo".to_string(), Value::String("something".to_string()))]
                    .into_iter()
                    .collect::<Params>(),
                depends_on: vec!["global".to_string()],
                ..Default::default()
            }],
            ..Default::default()
        };
        let var1 = &Variable {
            name: "global".to_string(),
            var_type: VarType::Mock,
            params: Params::from_iter(vec![("abort".to_string(), Value::Null)]),
            ..Default::default()
        };
        let global_vars = MyContext::new(&[var1], &[]);
        let res = renderer.render_template(
            &template,
            global_vars.as_context(),
            &RenderOptions::default(),
        );
        assert!(matches!(res, RenderResult::Aborted));
    }

    #[test]
    fn nested_match() {
        let renderer = get_renderer();
        let template = TextEffect {
            body: "hello {{var}}".to_string(),
            vars: vec![Variable {
                name: "var".to_string(),
                var_type: VarType::Match,
                params: vec![("trigger".to_string(), Value::String("nested".to_string()))]
                    .into_iter()
                    .collect::<Params>(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let nested_template = TextEffect {
            body: "world".to_string(),
            ..Default::default()
        };
        let match1 = &TriggerMatch {
            triggers: vec![CompactString::const_new("nested")],
            base_match: BaseMatch {
                effect: MatchEffect::Text(nested_template),
                ..Default::default()
            },
            ..Default::default()
        };
        let templates = MyContext::new(&[], &[match1]);
        let res =
            renderer.render_template(&template, templates.as_context(), &RenderOptions::default());
        assert!(matches!(res, RenderResult::Success(str) if str == "hello world"));
    }

    #[test]
    fn missing_nested_match() {
        let renderer = get_renderer();
        let template = TextEffect {
            body: "hello {{var}}".to_string(),
            vars: vec![Variable {
                name: "var".to_string(),
                var_type: VarType::Match,
                params: vec![("trigger".to_string(), Value::String("nested".to_string()))]
                    .into_iter()
                    .collect::<Params>(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let res = renderer.render_template(
            &template,
            Context {
                ..Default::default()
            },
            &RenderOptions::default(),
        );
        assert!(matches!(res, RenderResult::Error(_)));
    }

    #[test]
    fn extension_aborting_propagates() {
        let renderer = get_renderer();
        let template = TextEffect {
            body: "hello {{var}}".to_string(),
            vars: vec![Variable {
                name: "var".to_string(),
                var_type: VarType::Mock,
                params: vec![("abort".to_string(), Value::Null)]
                    .into_iter()
                    .collect::<Params>(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let res =
            renderer.render_template(&template, Context::default(), &RenderOptions::default());
        assert!(matches!(res, RenderResult::Aborted));
    }

    #[test]
    fn extension_error_propagates() {
        let renderer = get_renderer();
        let template = TextEffect {
            body: "hello {{var}}".to_string(),
            vars: vec![Variable {
                name: "var".to_string(),
                var_type: VarType::Mock,
                params: vec![("error".to_string(), Value::Null)]
                    .into_iter()
                    .collect::<Params>(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let res =
            renderer.render_template(&template, Context::default(), &RenderOptions::default());
        assert!(matches!(res, RenderResult::Error(_)));
    }

    #[test]
    fn variable_injection() {
        let renderer = get_renderer();
        let mut template = template_for_str("hello {{fullname}}");
        template.vars = vec![
            Variable {
                name: "firstname".to_string(),
                var_type: VarType::Mock,
                params: Params::from_iter(vec![(
                    "echo".to_string(),
                    Value::String("John".to_string()),
                )]),
                ..Default::default()
            },
            Variable {
                name: "lastname".to_string(),
                var_type: VarType::Mock,
                params: Params::from_iter(vec![(
                    "echo".to_string(),
                    Value::String("Snow".to_string()),
                )]),
                ..Default::default()
            },
            Variable {
                name: "fullname".to_string(),
                var_type: VarType::Mock,
                params: Params::from_iter(vec![(
                    "echo".to_string(),
                    Value::String("{{firstname}} {{lastname}}".to_string()),
                )]),
                inject_vars: true,
                ..Default::default()
            },
        ];

        let res =
            renderer.render_template(&template, Context::default(), &RenderOptions::default());
        assert!(matches!(res, RenderResult::Success(str) if str == "hello John Snow"));
    }

    #[test]
    fn disable_variable_injection() {
        let renderer = get_renderer();
        let mut template = template_for_str("hello {{second}}");
        template.vars = vec![
            Variable {
                name: "first".to_string(),
                var_type: VarType::Mock,
                params: Params::from_iter(vec![(
                    "echo".to_string(),
                    Value::String("one".to_string()),
                )]),
                ..Default::default()
            },
            Variable {
                name: "second".to_string(),
                var_type: VarType::Mock,
                params: Params::from_iter(vec![(
                    "echo".to_string(),
                    Value::String("{{first}} two".to_string()),
                )]),
                inject_vars: false,
                ..Default::default()
            },
        ];

        let res =
            renderer.render_template(&template, Context::default(), &RenderOptions::default());
        assert!(matches!(res, RenderResult::Success(str) if str == "hello {{first}} two"));
    }

    #[test]
    fn escaped_variable_injection() {
        let renderer = get_renderer();
        let mut template = template_for_str("hello {{second}}");
        template.vars = vec![
            Variable {
                name: "first".to_string(),
                var_type: VarType::Mock,
                params: Params::from_iter(vec![(
                    "echo".to_string(),
                    Value::String("one".to_string()),
                )]),
                ..Default::default()
            },
            Variable {
                name: "second".to_string(),
                var_type: VarType::Mock,
                params: Params::from_iter(vec![(
                    "echo".to_string(),
                    Value::String("\\{\\{first\\}\\} two".to_string()),
                )]),
                ..Default::default()
            },
        ];

        let res =
            renderer.render_template(&template, Context::default(), &RenderOptions::default());
        assert!(matches!(res, RenderResult::Success(str) if str == "hello {{first}} two"));
    }

    #[test]
    fn variable_injection_missing_var() {
        let renderer = get_renderer();
        let mut template = template_for_str("hello {{second}}");
        template.vars = vec![Variable {
            name: "second".to_string(),
            var_type: VarType::Mock,
            params: Params::from_iter(vec![(
                "echo".to_string(),
                Value::String("the next is {{missing}}".to_string()),
            )]),
            ..Default::default()
        }];

        let res =
            renderer.render_template(&template, Context::default(), &RenderOptions::default());
        assert!(matches!(res, RenderResult::Error(_)));
    }

    #[test]
    fn variable_injection_with_global_variable() {
        let renderer = get_renderer();
        let mut template = template_for_str("hello {{output}}");
        template.vars = vec![
            Variable {
                name: "var".to_string(),
                var_type: VarType::Unresolved,
                ..Default::default()
            },
            Variable {
                name: "output".to_string(),
                var_type: VarType::Mock,
                params: Params::from_iter(vec![(
                    "echo".to_string(),
                    Value::String("{{var}}".to_string()),
                )]),
                ..Default::default()
            },
        ];

        let var1 = &Variable {
            name: "var".to_string(),
            var_type: VarType::Mock,
            params: Params::from_iter(vec![(
                "echo".to_string(),
                Value::String("global".to_string()),
            )]),
            ..Default::default()
        };
        let global_vars = MyContext::new(&[var1], &[]);
        let res = renderer.render_template(
            &template,
            global_vars.as_context(),
            &RenderOptions::default(),
        );
        assert!(matches!(res, RenderResult::Success(str) if str == "hello global"));
    }

    #[test]
    fn variable_injection_local_var_takes_precedence_over_global() {
        let renderer = get_renderer();
        let mut template = template_for_str("hello {{output}}");
        template.vars = vec![
            Variable {
                name: "var".to_string(),
                var_type: VarType::Mock,
                params: Params::from_iter(vec![(
                    "echo".to_string(),
                    Value::String("local".to_string()),
                )]),
                ..Default::default()
            },
            Variable {
                name: "output".to_string(),
                var_type: VarType::Mock,
                params: Params::from_iter(vec![(
                    "echo".to_string(),
                    Value::String("{{var}}".to_string()),
                )]),
                ..Default::default()
            },
        ];

        let var1 = &Variable {
            name: "var".to_string(),
            var_type: VarType::Mock,
            params: Params::from_iter(vec![(
                "echo".to_string(),
                Value::String("global".to_string()),
            )]),
            ..Default::default()
        };
        let global_vars = MyContext::new(&[var1], &[]);
        let res = renderer.render_template(
            &template,
            global_vars.as_context(),
            &RenderOptions::default(),
        );
        assert!(matches!(res, RenderResult::Success(str) if str == "hello local"));
    }

    #[test]
    fn variable_escape() {
        let renderer = get_renderer();
        let template = template("hello \\{\\{var\\}\\}", &[("var", "world")]);
        let res =
            renderer.render_template(&template, Context::default(), &RenderOptions::default());
        assert!(matches!(res, RenderResult::Success(str) if str == "hello {{var}}"));
    }
}
