use enum_as_inner::EnumAsInner;
use std::collections::HashMap;

mod stores;

pub use stores::{RegexMatchRef, RegexMatchStore, TrigMatchRef, TrigMatchStore, VarRef, VarStore};

pub type StructId = i32;

#[derive(Debug, Clone, PartialEq, Default)]
pub enum VarType {
    Date,
    Mock,
    Shell,
    Script,
    Random,
    Echo,
    Form,
    /// For nested matches: https://espanso.org/docs/matches/basics/#nested-matches
    Match,
    #[default]
    Unresolved,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Variable {
    pub name: String,
    pub var_type: VarType,
    pub params: Params,
    pub inject_vars: bool,
    pub depends_on: Vec<String>,
}

impl Default for Variable {
    fn default() -> Self {
        Self {
            name: String::new(),
            var_type: VarType::Unresolved,
            params: Params::new(),
            inject_vars: true,
            depends_on: Vec::new(),
        }
    }
}

pub type Params = HashMap<String, Value>;

#[derive(Debug, Clone, PartialEq, EnumAsInner)]
pub enum Value {
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<Value>),
    Object(Params),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Number {
    Integer(i64),
    // Float(OrderedFloat<f64>),
    Float(f64),
}

#[derive(Debug, Clone, PartialEq)]
pub enum MatchIdx {
    Trigger(TrigMatchRef),
    Regex(RegexMatchRef),
    BuiltIn(i32),
}

impl Default for MatchIdx {
    fn default() -> Self {
        Self::Trigger(TrigMatchRef::default())
    }
}

// Causes

#[derive(Debug, Clone, Eq, Hash, PartialEq, EnumAsInner)]
pub enum MatchCause {
    Trigger(TriggerCause),
    Regex(RegexCause),
    // TODO: shortcut
}

impl MatchCause {
    pub fn description(&self) -> Option<&str> {
        match &self {
            MatchCause::Trigger(trigger_cause) => {
                trigger_cause.triggers.first().map(String::as_str)
            }
            MatchCause::Regex(trigger_cause) => Some(trigger_cause.regex.as_str()),
        }
        // TODO: insert rendering for hotkey/shortcut
    }

    pub fn long_description(&self) -> String {
        match &self {
            MatchCause::Trigger(trigger_cause) => format!("triggers: {:?}", trigger_cause.triggers),
            MatchCause::Regex(trigger_cause) => format!("regex: {:?}", trigger_cause.regex),
        }
        // TODO: insert rendering for hotkey/shortcut
    }

    pub fn search_terms(&self) -> Vec<&str> {
        if let MatchCause::Trigger(trigger_cause) = &self {
            trigger_cause.triggers.iter().map(String::as_str).collect()
        } else {
            vec![]
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum WordBoundary {
    #[default]
    None,
    Left,
    Right,
    Both,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct TriggerCause {
    pub triggers: Vec<String>,

    pub word_boundary: WordBoundary,

    pub propagate_case: bool,
    pub uppercase_style: UpperCasingStyle,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum UpperCasingStyle {
    #[default]
    Uppercase,
    Capitalize,
    CapitalizeWords,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct RegexCause {
    pub regex: String,
}

// Effects

#[derive(Debug, Clone, PartialEq, EnumAsInner)]
pub enum MatchEffect {
    None,
    Text(TextEffect),
    Image(ImageEffect),
}

impl Default for MatchEffect {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextEffect {
    pub body: String,
    pub vars: Vec<Variable>,
    pub format: TextFormat,
    pub force_mode: Option<TextInjectMode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TextFormat {
    Plain,
    Markdown,
    Html,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TextInjectMode {
    Keys,
    Clipboard,
}

impl Default for TextEffect {
    fn default() -> Self {
        Self {
            body: String::new(),
            vars: Vec::new(),
            format: TextFormat::Plain,
            force_mode: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct ImageEffect {
    pub path: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BaseMatch {
    // pub id: i32,
    pub effect: MatchEffect,

    // Metadata
    pub label: Option<String>,
    pub search_terms: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TriggerMatch {
    pub base_match: BaseMatch,

    pub propagate_case: bool,
    pub uppercase_style: UpperCasingStyle,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trigger_cause() -> TriggerCause {
        TriggerCause {
            triggers: vec![":greet".to_string()],
            ..TriggerCause::default()
        }
    }

    fn regex_cause() -> RegexCause {
        RegexCause {
            regex: ":greet\\d".to_string(),
        }
    }

    #[test]
    fn match_cause_trigger_description() {
        let trigger = trigger_cause();

        assert_eq!(MatchCause::Trigger(trigger).description(), Some(":greet"));
    }

    #[test]
    fn match_cause_regex_description() {
        let regex = regex_cause();
        assert_eq!(MatchCause::Regex(regex).description(), Some(":greet\\d"));
    }

    #[test]
    fn match_cause_trigger_long_description() {
        let trigger = trigger_cause();

        assert_eq!(
            MatchCause::Trigger(trigger).long_description(),
            r#"triggers: [":greet"]"#
        );
    }

    #[test]
    fn match_cause_regex_long_description() {
        let regex = regex_cause();

        assert_eq!(
            MatchCause::Regex(regex).long_description(),
            r#"regex: ":greet\\d""#
        );
    }
}
