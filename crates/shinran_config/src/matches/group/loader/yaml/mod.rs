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

use std::ffi::OsStr;

use crate::{
    error::{ErrorRecord, NonFatalErrorSet},
    matches::{
        group::{path::resolve_imports, LoadedMatchFile},
        LoadedMatch,
    },
};
use anyhow::{anyhow, bail, Context, Result};
use lazy_static::lazy_static;
use parse::YAMLMatchFile;
use regex::{Captures, Regex};
use shinran_types::{
    ImageEffect, MatchCause, MatchEffect, Params, RegexCause, TextEffect, TextFormat,
    TextInjectMode, TriggerCause, UpperCasingStyle, Value, VarType, Variable, WordBoundary,
};

use self::{
    parse::{YAMLMatch, YAMLVariable},
    util::convert_params,
};

// use super::Importer;

pub(crate) mod parse;
mod util;

lazy_static! {
    static ref VAR_REGEX: Regex = Regex::new("\\{\\{\\s*(\\w+)(\\.\\w+)?\\s*\\}\\}").unwrap();
    static ref FORM_CONTROL_REGEX: Regex =
        Regex::new("\\[\\[\\s*(\\w+)(\\.\\w+)?\\s*\\]\\]").unwrap();
}

// Create an alias to make the meaning more explicit
type Warning = anyhow::Error;

pub(crate) struct YAMLImporter {}

impl YAMLImporter {
    pub fn new() -> Self {
        Self {}
    }
}

impl YAMLImporter {
    pub fn is_supported(extension: &OsStr) -> bool {
        extension.eq_ignore_ascii_case("yaml") || extension.eq_ignore_ascii_case("yml")
    }

    pub fn load_file(
        &self,
        path: &std::path::Path,
    ) -> anyhow::Result<(
        crate::matches::group::LoadedMatchFile,
        Option<NonFatalErrorSet>,
    )> {
        let yaml_loaded =
            YAMLMatchFile::parse_from_file(path).context("failed to parse YAML match group")?;

        let mut non_fatal_errors = Vec::new();

        let mut global_vars = Vec::new();
        for yaml_global_var in yaml_loaded.global_vars.clone().unwrap_or_default() {
            match try_convert_into_variable(yaml_global_var, false) {
                Ok((var, warnings)) => {
                    global_vars.push(var);
                    non_fatal_errors.extend(warnings.into_iter().map(ErrorRecord::warn));
                }
                Err(err) => {
                    non_fatal_errors.push(ErrorRecord::error(err));
                }
            }
        }

        let mut matches = Vec::new();
        for yaml_match in yaml_loaded.matches.clone().unwrap_or_default() {
            match try_convert_into_match(yaml_match, false) {
                Ok((m, warnings)) => {
                    matches.push(m);
                    non_fatal_errors.extend(warnings.into_iter().map(ErrorRecord::warn));
                }
                Err(err) => {
                    non_fatal_errors.push(ErrorRecord::error(err));
                }
            }
        }

        // Resolve imports
        let (resolved_imports, import_errors) =
            resolve_imports(path, &yaml_loaded.imports.unwrap_or_default())
                .context("failed to resolve YAML match file imports")?;
        non_fatal_errors.extend(import_errors);

        let non_fatal_error_set = if non_fatal_errors.is_empty() {
            None
        } else {
            Some(NonFatalErrorSet::new(path, non_fatal_errors))
        };

        Ok((
            LoadedMatchFile {
                imports: resolved_imports,
                global_vars,
                matches,
            },
            non_fatal_error_set,
        ))
    }
}

/// Convert a YAMLMatch into a Match.
pub fn try_convert_into_match(
    yaml_match: YAMLMatch,
    use_compatibility_mode: bool,
) -> Result<(LoadedMatch, Vec<Warning>)> {
    let mut warnings = Vec::new();

    if yaml_match.uppercase_style.is_some() && yaml_match.propagate_case.is_none() {
        warnings.push(anyhow!(
            "specifying the 'uppercase_style' option without 'propagate_case' has no effect"
        ));
    }

    let triggers = if let Some(trigger) = yaml_match.trigger {
        Some(vec![trigger])
    } else {
        yaml_match.triggers
    };

    let uppercase_style = match yaml_match
        .uppercase_style
        .map(|s| s.to_lowercase())
        .as_deref()
    {
        Some("uppercase") => UpperCasingStyle::Uppercase,
        Some("capitalize") => UpperCasingStyle::Capitalize,
        Some("capitalize_words") => UpperCasingStyle::CapitalizeWords,
        Some(style) => {
            warnings.push(anyhow!(
                "unrecognized uppercase_style: {:?}, falling back to the default",
                style
            ));
            TriggerCause::default().uppercase_style
        }
        _ => TriggerCause::default().uppercase_style,
    };

    let cause = if let Some(triggers) = triggers {
        let left_word = yaml_match.left_word.or(yaml_match.word).unwrap_or(false);
        let right_word = yaml_match.right_word.or(yaml_match.word).unwrap_or(false);
        let word_boundary = match (left_word, right_word) {
            (true, true) => WordBoundary::Both,
            (true, false) => WordBoundary::Left,
            (false, true) => WordBoundary::Right,
            (false, false) => WordBoundary::None,
        };
        MatchCause::Trigger(TriggerCause {
            triggers,
            word_boundary,
            propagate_case: yaml_match
                .propagate_case
                .unwrap_or(TriggerCause::default().propagate_case),
            uppercase_style,
        })
    } else if let Some(regex) = yaml_match.regex {
        // TODO: add test case
        MatchCause::Regex(RegexCause { regex })
    } else {
        bail!("match must have either 'trigger' or 'regex' field; both are missing");
    };

    // TODO: test force_mode/force_clipboard
    let force_mode = if let Some(true) = yaml_match.force_clipboard {
        Some(TextInjectMode::Clipboard)
    } else if let Some(mode) = yaml_match.force_mode {
        match mode.to_lowercase().as_str() {
            "clipboard" => Some(TextInjectMode::Clipboard),
            "keys" => Some(TextInjectMode::Keys),
            _ => None,
        }
    } else {
        None
    };

    let effect = if yaml_match.replace.is_some()
        || yaml_match.markdown.is_some()
        || yaml_match.html.is_some()
    {
        // TODO: test markdown and html cases
        let (replace, format) = if let Some(plain) = yaml_match.replace {
            (plain, TextFormat::Plain)
        } else if let Some(markdown) = yaml_match.markdown {
            (markdown, TextFormat::Markdown)
        } else if let Some(html) = yaml_match.html {
            (html, TextFormat::Html)
        } else {
            unreachable!();
        };

        let mut vars: Vec<Variable> = Vec::new();
        for yaml_var in yaml_match.vars.unwrap_or_default() {
            let (var, var_warnings) =
                try_convert_into_variable(yaml_var.clone(), use_compatibility_mode)
                    .with_context(|| format!("failed to load variable: {yaml_var:?}"))?;
            warnings.extend(var_warnings);
            vars.push(var);
        }

        MatchEffect::Text(TextEffect {
            body: replace,
            vars,
            format,
            force_mode,
        })
    } else if let Some(form_layout) = yaml_match.form {
        // Replace all the form fields with actual variables

        // In v2.1.0-alpha the form control syntax was replaced with [[control]]
        // instead of {{control}}, so we check if compatibility mode is being used.
        // TODO: remove once compatibility mode is removed

        let (resolved_replace, resolved_layout) = if use_compatibility_mode {
            (
                VAR_REGEX
                    .replace_all(&form_layout, |caps: &Captures| {
                        let var_name = caps.get(1).unwrap().as_str();
                        format!("{{{{form1.{var_name}}}}}")
                    })
                    .to_string(),
                VAR_REGEX
                    .replace_all(&form_layout, |caps: &Captures| {
                        let var_name = caps.get(1).unwrap().as_str();
                        format!("[[{var_name}]]")
                    })
                    .to_string(),
            )
        } else {
            (
                FORM_CONTROL_REGEX
                    .replace_all(&form_layout, |caps: &Captures| {
                        let var_name = caps.get(1).unwrap().as_str();
                        format!("{{{{form1.{var_name}}}}}")
                    })
                    .to_string(),
                form_layout,
            )
        };

        // Convert escaped brakets in forms
        let resolved_replace = resolved_replace.replace("\\{", "{ ").replace("\\}", " }");

        // Convert the form data to valid variables
        let mut params = Params::new();
        params.insert("layout".to_string(), Value::String(resolved_layout));

        if let Some(fields) = yaml_match.form_fields {
            params.insert("fields".to_string(), Value::Object(convert_params(fields)?));
        }

        let vars = vec![Variable {
            // id: next_id(),
            name: "form1".to_owned(),
            var_type: VarType::Form,
            params,
            ..Default::default()
        }];

        MatchEffect::Text(TextEffect {
            body: resolved_replace,
            vars,
            format: TextFormat::Plain,
            force_mode,
        })
    } else if let Some(image_path) = yaml_match.image_path {
        // TODO: test image case
        MatchEffect::Image(ImageEffect { path: image_path })
    } else {
        MatchEffect::None
    };

    if let MatchEffect::None = effect {
        bail!(
      "match triggered by {:?} does not produce any effect. Did you forget the 'replace' field?",
      cause.long_description()
    );
    }

    Ok((
        LoadedMatch {
            cause,
            effect,
            label: yaml_match.label,
            search_terms: yaml_match.search_terms.unwrap_or_default(),
        },
        warnings,
    ))
}

pub fn try_convert_into_variable(
    yaml_var: YAMLVariable,
    use_compatibility_mode: bool,
) -> Result<(Variable, Vec<Warning>)> {
    let var_type = match &yaml_var.var_type[..] {
        "date" => VarType::Date,
        "dummy" => VarType::Echo,
        "echo" => VarType::Echo,
        "form" => VarType::Form,
        "match" => VarType::Match,
        "random" => VarType::Random,
        "script" => VarType::Script,
        "shell" => VarType::Shell,
        "mock" => VarType::Mock,
        "test" => VarType::Mock,
        _ => return Err(anyhow!("unknown variable type: {:?}", yaml_var.var_type)),
    };
    Ok((
        Variable {
            name: yaml_var.name,
            var_type,
            params: convert_params(yaml_var.params)?,
            inject_vars: !use_compatibility_mode && yaml_var.inject_vars.unwrap_or(true),
            depends_on: yaml_var.depends_on,
        },
        Vec::new(),
    ))
}

#[cfg(test)]
mod tests {
    use shinran_helpers::use_test_directory;
    use shinran_types::{MatchCause, TextEffect, TriggerCause};

    use super::*;
    use crate::matches::LoadedMatch;
    use std::{ffi::OsString, fs::create_dir_all};

    fn create_match_with_warnings(
        yaml: &str,
        use_compatibility_mode: bool,
    ) -> Result<(LoadedMatch, Vec<Warning>)> {
        let yaml_match: YAMLMatch = serde_yaml_ng::from_str(yaml)?;
        let (m, warnings) = try_convert_into_match(yaml_match, use_compatibility_mode)?;

        Ok((m, warnings))
    }

    fn create_match(yaml: &str) -> Result<LoadedMatch> {
        let (m, warnings) = create_match_with_warnings(yaml, false)?;
        assert!(
            warnings.is_empty(),
            "warnings were detected but not handled: {warnings:?}"
        );
        Ok(m)
    }

    #[test]
    fn basic_match_maps_correctly() {
        assert_eq!(
            create_match(
                r#"
        trigger: "Hello"
        replace: "world"
        "#
            )
            .unwrap(),
            LoadedMatch {
                cause: MatchCause::Trigger(TriggerCause {
                    triggers: vec!["Hello".to_string()],
                    ..Default::default()
                }),
                effect: MatchEffect::Text(TextEffect {
                    body: "world".to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            }
        );
    }

    #[test]
    fn multiple_triggers_maps_correctly() {
        assert_eq!(
            create_match(
                r#"
        triggers: ["Hello", "john"]
        replace: "world"
        "#
            )
            .unwrap(),
            LoadedMatch {
                cause: MatchCause::Trigger(TriggerCause {
                    triggers: vec!["Hello".to_string(), "john".to_string()],
                    ..Default::default()
                }),
                effect: MatchEffect::Text(TextEffect {
                    body: "world".to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            }
        );
    }

    #[test]
    fn word_maps_correctly() {
        assert_eq!(
            create_match(
                r#"
        trigger: "Hello"
        replace: "world"
        word: true
        "#
            )
            .unwrap(),
            LoadedMatch {
                cause: MatchCause::Trigger(TriggerCause {
                    triggers: vec!["Hello".to_string()],
                    word_boundary: WordBoundary::Both,
                    ..Default::default()
                }),
                effect: MatchEffect::Text(TextEffect {
                    body: "world".to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            }
        );
    }

    #[test]
    fn left_word_maps_correctly() {
        assert_eq!(
            create_match(
                r#"
        trigger: "Hello"
        replace: "world"
        left_word: true
        "#
            )
            .unwrap(),
            LoadedMatch {
                cause: MatchCause::Trigger(TriggerCause {
                    triggers: vec!["Hello".to_string()],
                    word_boundary: WordBoundary::Left,
                    ..Default::default()
                }),
                effect: MatchEffect::Text(TextEffect {
                    body: "world".to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            }
        );
    }

    #[test]
    fn right_word_maps_correctly() {
        assert_eq!(
            create_match(
                r#"
        trigger: "Hello"
        replace: "world"
        right_word: true
        "#
            )
            .unwrap(),
            LoadedMatch {
                cause: MatchCause::Trigger(TriggerCause {
                    triggers: vec!["Hello".to_string()],
                    word_boundary: WordBoundary::Right,
                    ..Default::default()
                }),
                effect: MatchEffect::Text(TextEffect {
                    body: "world".to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            }
        );
    }

    #[test]
    fn propagate_case_maps_correctly() {
        assert_eq!(
            create_match(
                r#"
        trigger: "Hello"
        replace: "world"
        propagate_case: true
        "#
            )
            .unwrap(),
            LoadedMatch {
                cause: MatchCause::Trigger(TriggerCause {
                    triggers: vec!["Hello".to_string()],
                    propagate_case: true,
                    ..Default::default()
                }),
                effect: MatchEffect::Text(TextEffect {
                    body: "world".to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            }
        );
    }

    #[test]
    fn uppercase_style_maps_correctly() {
        assert_eq!(
            create_match(
                r#"
        trigger: "Hello"
        replace: "world"
        uppercase_style: "capitalize"
        propagate_case: true
        "#
            )
            .unwrap()
            .cause
            .into_trigger()
            .unwrap()
            .uppercase_style,
            UpperCasingStyle::Capitalize,
        );

        assert_eq!(
            create_match(
                r#"
        trigger: "Hello"
        replace: "world"
        uppercase_style: "capitalize_words"
        propagate_case: true
        "#
            )
            .unwrap()
            .cause
            .into_trigger()
            .unwrap()
            .uppercase_style,
            UpperCasingStyle::CapitalizeWords,
        );

        assert_eq!(
            create_match(
                r#"
        trigger: "Hello"
        replace: "world"
        uppercase_style: "uppercase"
        propagate_case: true
        "#
            )
            .unwrap()
            .cause
            .into_trigger()
            .unwrap()
            .uppercase_style,
            UpperCasingStyle::Uppercase,
        );

        // Invalid without propagate_case
        let (m, warnings) = create_match_with_warnings(
            r#"
        trigger: "Hello"
        replace: "world"
        uppercase_style: "capitalize"
        "#,
            false,
        )
        .unwrap();
        assert_eq!(
            m.cause.into_trigger().unwrap().uppercase_style,
            UpperCasingStyle::Capitalize,
        );
        assert_eq!(warnings.len(), 1);

        // Invalid style
        let (m, warnings) = create_match_with_warnings(
            r#"
        trigger: "Hello"
        replace: "world"
        uppercase_style: "invalid"
        propagate_case: true
        "#,
            false,
        )
        .unwrap();
        assert_eq!(
            m.cause.into_trigger().unwrap().uppercase_style,
            UpperCasingStyle::Uppercase,
        );
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn form_maps_correctly() {
        let mut params = Params::new();
        params.insert(
            "layout".to_string(),
            Value::String("Hi [[name]]!".to_string()),
        );

        assert_eq!(
            create_match(
                r#"
        trigger: "Hello"
        form: "Hi [[name]]!"
        "#
            )
            .unwrap(),
            LoadedMatch {
                cause: MatchCause::Trigger(TriggerCause {
                    triggers: vec!["Hello".to_string()],
                    ..Default::default()
                }),
                effect: MatchEffect::Text(TextEffect {
                    body: "Hi {{form1.name}}!".to_string(),
                    vars: vec![Variable {
                        // id: 0,
                        name: "form1".to_string(),
                        var_type: VarType::Form,
                        params,
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                ..Default::default()
            }
        );
    }

    #[test]
    fn form_maps_correctly_with_variable_injection() {
        let mut params = Params::new();
        params.insert(
            "layout".to_string(),
            Value::String("Hi [[name]]! {{signature}}".to_string()),
        );

        assert_eq!(
            create_match(
                r#"
        trigger: "Hello"
        form: "Hi [[name]]! {{signature}}"
        "#
            )
            .unwrap(),
            LoadedMatch {
                cause: MatchCause::Trigger(TriggerCause {
                    triggers: vec!["Hello".to_string()],
                    ..Default::default()
                }),
                effect: MatchEffect::Text(TextEffect {
                    body: "Hi {{form1.name}}! {{signature}}".to_string(),
                    vars: vec![Variable {
                        // id: 0,
                        name: "form1".to_string(),
                        var_type: VarType::Form,
                        params,
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                ..Default::default()
            }
        );
    }

    #[test]
    fn form_maps_correctly_legacy_format() {
        let mut params = Params::new();
        params.insert(
            "layout".to_string(),
            Value::String("Hi [[name]]!".to_string()),
        );

        assert_eq!(
            create_match_with_warnings(
                r#"
        trigger: "Hello"
        form: "Hi {{name}}!"
        "#,
                true
            )
            .unwrap()
            .0,
            LoadedMatch {
                cause: MatchCause::Trigger(TriggerCause {
                    triggers: vec!["Hello".to_string()],
                    ..Default::default()
                }),
                effect: MatchEffect::Text(TextEffect {
                    body: "Hi {{form1.name}}!".to_string(),
                    vars: vec![Variable {
                        // id: 0,
                        name: "form1".to_string(),
                        var_type: VarType::Form,
                        params,
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                ..Default::default()
            }
        );
    }

    #[test]
    fn vars_maps_correctly() {
        let mut params = Params::new();
        params.insert("param1".to_string(), Value::Bool(true));
        let vars = vec![Variable {
            name: "var1".to_string(),
            var_type: VarType::Mock,
            params,
            ..Default::default()
        }];
        assert_eq!(
            create_match(
                r#"
        trigger: "Hello"
        replace: "world"
        vars:
          - name: var1
            type: mock
            params:
              param1: true
        "#
            )
            .unwrap(),
            LoadedMatch {
                cause: MatchCause::Trigger(TriggerCause {
                    triggers: vec!["Hello".to_string()],
                    ..Default::default()
                }),
                effect: MatchEffect::Text(TextEffect {
                    body: "world".to_string(),
                    vars,
                    ..Default::default()
                }),
                ..Default::default()
            }
        );
    }

    #[test]
    fn vars_inject_vars_and_depends_on() {
        let vars = vec![
            Variable {
                name: "var1".to_string(),
                var_type: VarType::Mock,
                depends_on: vec!["test".to_owned()],
                ..Default::default()
            },
            Variable {
                name: "var2".to_string(),
                var_type: VarType::Mock,
                inject_vars: false,
                ..Default::default()
            },
        ];
        assert_eq!(
            create_match(
                r#"
        trigger: "Hello"
        replace: "world"
        vars:
          - name: var1
            type: test
            depends_on: ["test"]
          - name: var2
            type: "test"
            inject_vars: false
        "#
            )
            .unwrap(),
            LoadedMatch {
                cause: MatchCause::Trigger(TriggerCause {
                    triggers: vec!["Hello".to_string()],
                    ..Default::default()
                }),
                effect: MatchEffect::Text(TextEffect {
                    body: "world".to_string(),
                    vars,
                    ..Default::default()
                }),
                ..Default::default()
            }
        );
    }

    #[test]
    fn vars_no_params_maps_correctly() {
        let vars = vec![Variable {
            name: "var1".to_string(),
            var_type: VarType::Mock,
            params: Params::new(),
            ..Default::default()
        }];
        assert_eq!(
            create_match(
                r#"
        trigger: "Hello"
        replace: "world"
        vars:
          - name: var1
            type: test
        "#
            )
            .unwrap(),
            LoadedMatch {
                cause: MatchCause::Trigger(TriggerCause {
                    triggers: vec!["Hello".to_string()],
                    ..Default::default()
                }),
                effect: MatchEffect::Text(TextEffect {
                    body: "world".to_string(),
                    vars,
                    ..Default::default()
                }),
                ..Default::default()
            }
        );
    }

    #[test]
    fn importer_is_supported() {
        assert!(YAMLImporter::is_supported(&OsString::from("yaml")));
        assert!(YAMLImporter::is_supported(&OsString::from("YAML")));
        assert!(YAMLImporter::is_supported(&OsString::from("yml")));
        assert!(YAMLImporter::is_supported(&OsString::from("yMl")));
        assert!(!YAMLImporter::is_supported(&OsString::from("invalid")));
    }

    #[test]
    fn importer_works_correctly() {
        use_test_directory(|_, match_dir, _| {
            let sub_dir = match_dir.join("sub");
            create_dir_all(&sub_dir).unwrap();

            let base_file = match_dir.join("base.yml");
            std::fs::write(
                &base_file,
                r#"
      imports:
        - "sub/sub.yml"
        - "invalid/import.yml" # This should be discarded

      global_vars:
        - name: "var1"
          type: "mock"

      matches:
        - trigger: "hello"
          replace: "world"
      "#,
            )
            .unwrap();

            let sub_file = sub_dir.join("sub.yml");
            std::fs::write(&sub_file, "").unwrap();

            let importer = YAMLImporter::new();
            let (file, non_fatal_error_set) = importer.load_file(&base_file).unwrap();
            // The invalid import path should be reported as error
            assert_eq!(non_fatal_error_set.unwrap().errors.len(), 1);

            let vars = vec![Variable {
                name: "var1".to_string(),
                var_type: VarType::Mock,
                params: Params::new(),
                ..Default::default()
            }];

            assert_eq!(
                file,
                LoadedMatchFile {
                    imports: vec![sub_file],
                    global_vars: vars,
                    matches: vec![LoadedMatch {
                        cause: MatchCause::Trigger(TriggerCause {
                            triggers: vec!["hello".to_string()],
                            ..Default::default()
                        }),
                        effect: MatchEffect::Text(TextEffect {
                            body: "world".to_string(),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }],
                }
            );
        });
    }

    #[test]
    fn importer_invalid_syntax() {
        use_test_directory(|_, match_dir, _| {
            let base_file = match_dir.join("base.yml");
            std::fs::write(
                &base_file,
                r"
      imports:
        - invalid
       - indentation
      ",
            )
            .unwrap();

            let importer = YAMLImporter::new();
            assert!(importer.load_file(&base_file).is_err());
        });
    }
}
