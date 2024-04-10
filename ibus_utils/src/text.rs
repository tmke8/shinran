use zbus::zvariant::{Dict, Signature, StructureBuilder, Value};

#[derive(Debug, Copy, Clone)]
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

pub struct IbusAttribute {
    pub attr: Attribute,
    pub start_index: u32,
    pub end_index: u32,
}

impl IbusAttribute {
    pub fn to_value(&self) -> Value<'static> {
        Value::Structure(
            StructureBuilder::new()
                .add_field("IBusAttribute") // Name
                .add_field(empty_dict()) // Attachments
                .add_field(self.attr.type_()) // Type
                .add_field(self.attr.value()) // Value
                .add_field(self.start_index) // StartIndex
                .add_field(self.end_index) // EndIndex
                .build(),
        )
    }
}

#[repr(transparent)]
pub struct IbusAttrList<'a>(&'a [IbusAttribute]);

impl IbusAttrList<'_> {
    pub fn to_value(&self) -> Value<'static> {
        Value::Structure(
            StructureBuilder::new()
                .add_field("IBusAttrList") // Name
                .append_field(empty_dict()) // Attachments
                .add_field(
                    // Attributes
                    self.0
                        .iter()
                        .map(IbusAttribute::to_value)
                        .collect::<Vec<_>>(),
                )
                .build(),
        )
    }
}

pub struct IbusText<'a> {
    pub text: &'a str,
    pub attributes: &'a [IbusAttribute],
}

impl IbusText<'_> {
    pub fn to_value(&self) -> Value<'_> {
        Value::Structure(
            StructureBuilder::new()
                .add_field("IBusText") // Name
                .append_field(empty_dict()) // Attachments
                .add_field(self.text) // Text
                .append_field(IbusAttrList(self.attributes).to_value()) // AttrList
                .build(),
        )
    }
}

fn empty_dict() -> Value<'static> {
    let key_signature = Signature::try_from("s").unwrap();
    let value_signature = Signature::try_from("v").unwrap();
    Value::Dict(Dict::new(key_signature, value_signature))
}

pub const fn rgb_to_u32(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) << 16 | (g as u32) << 8 | b as u32
}

#[cfg(test)]
mod tests {
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
}
