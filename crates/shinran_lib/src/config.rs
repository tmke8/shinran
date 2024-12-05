use std::{alloc::Layout, collections::HashMap, ptr::NonNull};

use log::info;
use rkyv::{
    ser::{serializers::AllocSerializer, ScratchSpace, Serializer},
    with::AsStringError,
    Archive, Deserialize, Fallible, Serialize,
};
use shinran_config::{config::ProfileStore, matches::store::MatchStore};

use crate::{get_path_override, load, path};

/// A struct containing all the information that was loaded from match files and config files.
#[derive(Archive, Serialize, Deserialize)]
#[archive(check_bytes)]
pub struct Configuration {
    pub profile_store: ProfileStore,
    pub match_store: MatchStore,
    /// Renderer for the variables.
    pub renderer: shinran_render::Renderer,
}

impl Configuration {
    pub fn new(cli_overrides: &HashMap<String, String>) -> Self {
        let force_config_path =
            get_path_override(cli_overrides, "config_dir", "SHINRAN_CONFIG_DIR");
        let force_package_path =
            get_path_override(cli_overrides, "package_dir", "SHINRAN_PACKAGE_DIR");
        let force_runtime_path =
            get_path_override(cli_overrides, "runtime_dir", "SHINRAN_RUNTIME_DIR");

        let paths = path::resolve_paths(
            force_config_path.as_deref(),
            force_package_path.as_deref(),
            force_runtime_path.as_deref(),
        );
        info!("reading configs from: {:?}", paths.config);
        info!("reading packages from: {:?}", paths.packages);
        info!("using runtime dir: {:?}", paths.runtime);

        let config_result = load::load_config(&paths.config).expect("unable to load config");

        let home_path = dirs::home_dir().expect("unable to obtain home dir path");
        let base_path = &paths.config;
        let packages_path = &paths.packages;
        let renderer = shinran_render::Renderer::new(base_path, &home_path, packages_path);

        let cfg = Configuration {
            profile_store: config_result.profile_store,
            match_store: config_result.match_store,
            renderer,
        };
        // We can construct our serializer in much the same way as we always do
        let mut serializer = MySerializer::<AllocSerializer<1024>>::default();
        // then manually serialize our value
        serializer.serialize_value(&cfg).unwrap();
        // and finally, dig all the way down to our byte buffer
        let bytes = serializer.into_inner().into_serializer().into_inner();

        // Retrieve source paths from the archived configuration.
        let archived = rkyv::check_archived_root::<Configuration>(&bytes[..]).unwrap();
        let mut paths = Vec::new();
        paths.extend(archived.profile_store.get_source_paths());
        paths.extend(archived.match_store.get_source_paths());

        cfg
    }
}

// This will be our serializer wrappper, it just contains another serializer inside of it and
// forwards everything down.
struct MySerializer<S> {
    inner: S,
}

impl<S> MySerializer<S> {
    pub fn into_inner(self) -> S {
        self.inner
    }
}

// The Fallible trait defines the error type for our serializer. This is our new error type that
// will implement From<AsStringError>.
impl<S: Fallible> Fallible for MySerializer<S> {
    type Error = MySerializerError<S::Error>;
}

// Our Serializer impl just forwards everything down to the inner serializer.
impl<S: Serializer> Serializer for MySerializer<S> {
    #[inline]
    fn pos(&self) -> usize {
        self.inner.pos()
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        self.inner.write(bytes).map_err(MySerializerError::Inner)
    }
}

// Our ScratchSpace impl just forwards everything down to the inner serializer.
impl<S: ScratchSpace> ScratchSpace for MySerializer<S> {
    unsafe fn push_scratch(&mut self, layout: Layout) -> Result<NonNull<[u8]>, Self::Error> {
        self.inner
            .push_scratch(layout)
            .map_err(MySerializerError::Inner)
    }
    unsafe fn pop_scratch(&mut self, ptr: NonNull<u8>, layout: Layout) -> Result<(), Self::Error> {
        self.inner
            .pop_scratch(ptr, layout)
            .map_err(MySerializerError::Inner)
    }
}

// A Default implementation will make it easier to construct our serializer in some cases.
impl<S: Default> Default for MySerializer<S> {
    fn default() -> Self {
        Self {
            inner: S::default(),
        }
    }
}

// This is our new error type. It has one variant for errors from the inner serializer, and one
// variant for AsStringErrors.
#[derive(Debug)]
enum MySerializerError<E> {
    Inner(E),
    AsStringError,
}

// This is the crux of our new error type. Since it implements From<AsStringError>, we'll be able to
// use our serializer with the AsString wrapper.
impl<E> From<AsStringError> for MySerializerError<E> {
    fn from(_: AsStringError) -> Self {
        Self::AsStringError
    }
}
