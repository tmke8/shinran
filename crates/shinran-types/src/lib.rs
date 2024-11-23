use enum_as_inner::EnumAsInner;
use std::collections::HashMap;

pub type StructId = i32;

#[derive(Debug, Clone, PartialEq)]
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
    /// For global variables: https://espanso.org/docs/matches/basics/#global-variables
    Global,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Variable {
    pub id: StructId,
    pub name: String,
    pub var_type: VarType,
    pub params: Params,
    pub inject_vars: bool,
    pub depends_on: Vec<String>,
}

impl Default for Variable {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            var_type: VarType::Mock,
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
