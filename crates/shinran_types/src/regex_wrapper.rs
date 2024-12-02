use regex::Regex;
use rkyv::{
    string::{ArchivedString, StringResolver},
    with::{ArchiveWith, AsString, AsStringError, DeserializeWith, SerializeWith},
    Fallible, SerializeUnsized,
};

#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct RegexWrapper(Regex);

/// A wrapper around a regex that can be serialized and deserialized with rkyv.
impl RegexWrapper {
    pub fn new(regex: Regex) -> Self {
        Self(regex)
    }

    /// Returns the original string of this regex.
    pub fn to_str(&self) -> &str {
        self.0.as_str()
    }

    /// Returns true if and only if there is a match for the regex anywhere in the haystack given.
    pub fn is_match(&self, haystack: &str) -> bool {
        self.0.is_match(haystack)
    }
}

impl ArchiveWith<RegexWrapper> for AsString {
    type Archived = ArchivedString;
    type Resolver = StringResolver;

    #[inline]
    unsafe fn resolve_with(
        field: &RegexWrapper,
        pos: usize,
        resolver: Self::Resolver,
        out: *mut Self::Archived,
    ) {
        ArchivedString::resolve_from_str(field.to_str(), pos, resolver, out);
    }
}

impl<S: Fallible + ?Sized> SerializeWith<RegexWrapper, S> for AsString
where
    S::Error: From<AsStringError>,
    str: SerializeUnsized<S>,
{
    #[inline]
    fn serialize_with(
        field: &RegexWrapper,
        serializer: &mut S,
    ) -> Result<Self::Resolver, S::Error> {
        ArchivedString::serialize_from_str(field.to_str(), serializer)
    }
}

impl<D: Fallible + ?Sized> DeserializeWith<ArchivedString, RegexWrapper, D> for AsString {
    #[inline]
    fn deserialize_with(field: &ArchivedString, _: &mut D) -> Result<RegexWrapper, D::Error> {
        // It's safe to unwrap here because we know it's a valid regex.
        Ok(RegexWrapper(Regex::new(field.as_str()).unwrap()))
    }
}
