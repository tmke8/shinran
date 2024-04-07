use std::collections::HashMap;

use zbus::zvariant::{StructureBuilder, Value};

pub const IBUS_ATTR_TYPE_NONE: u32 = 0;
pub const IBUS_ATTR_TYPE_UNDERLINE: u32 = 1;
pub const IBUS_ATTR_TYPE_FOREGROUND: u32 = 2;
pub const IBUS_ATTR_TYPE_BACKGROUND: u32 = 3;

pub const IBUS_ATTR_UNDERLINE_NONE: u32 = 0;
pub const IBUS_ATTR_UNDERLINE_SINGLE: u32 = 1;
pub const IBUS_ATTR_UNDERLINE_DOUBLE: u32 = 2;
pub const IBUS_ATTR_UNDERLINE_LOW: u32 = 3;
pub const IBUS_ATTR_UNDERLINE_ERROR: u32 = 4;

pub struct Attribute {
    pub type_: u32,
    pub value: u32,
    pub start_index: u32,
    pub end_index: u32,
}

fn attr_to_value(attr: &Attribute) -> Value<'static> {
    let attachments: HashMap<String, Value<'static>> = HashMap::new();
    Value::Structure(
        StructureBuilder::new()
            .add_field("IBusAttribute") // name
            .add_field(attachments)
            .add_field(attr.type_)
            .add_field(attr.value)
            .add_field(attr.start_index)
            .add_field(attr.end_index)
            .build(),
    )
}

fn attr_list(attributes: &[Attribute]) -> Value<'static> {
    let attachments: HashMap<String, Value<'static>> = HashMap::new();
    Value::Structure(
        StructureBuilder::new()
            .add_field("IBusAttrList") // name
            .add_field(attachments)
            .add_field(attributes.iter().map(attr_to_value).collect::<Vec<_>>())
            .build(),
    )
}

pub fn ibus_text<'a>(text: &'a str, attributes: &[Attribute]) -> Value<'a> {
    let attachments: HashMap<String, Value<'static>> = HashMap::new();
    Value::Structure(
        StructureBuilder::new()
            .add_field("IBusText") // name
            .add_field(attachments)
            .add_field(text)
            .add_field(attr_list(attributes))
            .build(),
    )
}
