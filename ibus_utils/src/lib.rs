mod address;
pub mod ibus_constants;
mod text;

pub use address::get_ibus_address;
pub use text::{rgb_to_u32, Attribute, IBusAttrList, IBusAttribute, IBusText, Underline};
