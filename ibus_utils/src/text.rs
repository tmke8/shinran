use zbus::zvariant::{Dict, Signature, Str, Type, Value};

#[derive(Debug, Clone)]
#[repr(u32)]
pub enum Attribute {
    Underline(Underline) = 1,
    Foreground(u32) = 2,
    Background(u32) = 3,
}

impl Attribute {
    fn type_(&self) -> u32 {
        // SAFETY: Because `Attribute` is marked `repr(u32)`, its layout is a `repr(C)` `union`
        // between `repr(C)` structs, each of which has the `u32` discriminant as its first
        // field, so we can read the discriminant without offsetting the pointer.
        unsafe { *<*const _>::from(self).cast::<u32>() }
    }

    fn value(&self) -> u32 {
        match self {
            Attribute::Underline(underline) => *underline as u32,
            Attribute::Foreground(color) => *color,
            Attribute::Background(color) => *color,
        }
    }
}

#[derive(Debug, Copy, Clone)]
#[repr(u32)]
pub enum Underline {
    None = 0,
    Single = 1,
    Double = 2,
    Low = 3,
    Error = 4,
}

#[derive(Clone, Value)]
pub struct IBusAttribute {
    name: Str<'static>,
    attachments: EmptyDict,
    type_: u32,
    value: u32,
    start_index: u32,
    end_index: u32,
}

impl IBusAttribute {
    pub fn new(attr: Attribute, start_index: u32, end_index: u32) -> IBusAttribute {
        IBusAttribute {
            name: "IBusAttribute".into(),
            attachments: EmptyDict {},
            type_: attr.type_(),
            value: attr.value(),
            start_index,
            end_index,
        }
    }
}

#[derive(Value)]
pub struct IBusAttrList {
    name: Str<'static>,
    attachments: EmptyDict,
    attributes: Vec<Value<'static>>,
}

impl IBusAttrList {
    pub fn new(attributes: &[IBusAttribute]) -> IBusAttrList {
        IBusAttrList {
            name: "IBusAttrList".into(),
            attachments: EmptyDict {},
            attributes: attributes
                .iter()
                .map(|a| Value::from(a.clone()))
                .collect::<Vec<_>>(),
        }
    }
}

#[derive(Value)]
pub struct IBusText<'a> {
    name: Str<'a>,
    attachments: EmptyDict,
    text: Str<'a>,
    attr_list: Value<'a>,
}

impl IBusText<'_> {
    pub fn new<'a>(text: &'a str, attributes: &[IBusAttribute]) -> IBusText<'a> {
        IBusText {
            name: "IBusText".into(),
            attachments: EmptyDict {},
            text: text.into(),
            attr_list: IBusAttrList::new(attributes).into(),
        }
    }
}

/// Manual implementation of an empty dict.
///
/// It's possible to just use `HashMap<String, Value>`, but it doesn't seem good
/// to construct an empty `HashMap` just to represent an empty dict.
#[derive(Clone, Type)]
#[zvariant(signature = "dict")]
pub struct EmptyDict;

impl From<EmptyDict> for Value<'_> {
    fn from(_: EmptyDict) -> Value<'static> {
        // SAFETY: I'm very sure these are valid signatures.
        let key_signature = Signature::from_static_str_unchecked("s");
        let value_signature = Signature::from_static_str_unchecked("v");
        Value::Dict(Dict::new(key_signature, value_signature))
    }
}

impl TryFrom<Value<'_>> for EmptyDict {
    type Error = zbus::zvariant::Error;
    fn try_from(value: Value<'_>) -> Result<Self, Self::Error> {
        match value {
            Value::Dict(_) => Ok(EmptyDict {}),
            _ => Err(zbus::zvariant::Error::IncorrectType),
        }
    }
}

pub const fn rgb_to_u32(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) << 16 | (g as u32) << 8 | b as u32
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn test_attr_type() {
        assert_eq!(Attribute::Underline(Underline::Single).type_(), 1);
        assert_eq!(Attribute::Foreground(0xFF00FF).type_(), 2);
        assert_eq!(Attribute::Background(0).type_(), 3);
    }

    #[test]
    fn test_rgb_to_num() {
        assert_eq!(rgb_to_u32(0xFF, 0, 0xFF), 0xFF00FF);
    }

    #[test]
    fn test_derive() {
        let s = IBusAttribute::new(Attribute::Underline(Underline::Single), 0, 4);
        let value2: Value = s.into();
        assert_eq!(value2.value_signature(), "(sa{sv}uuuu)");
    }

    #[test]
    fn test_empty_dict() {
        assert_eq!(EmptyDict::signature(), "a{sv}");
        let empty_dict = EmptyDict {};
        let value: Value = empty_dict.into();
        assert_eq!(value.value_signature(), "a{sv}");

        let value2: Value = HashMap::<String, Value>::new().into();
        assert_eq!(value2.value_signature(), "a{sv}");

        assert_eq!(value, value2);
    }
}
