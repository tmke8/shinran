use std::{
    alloc::Layout,
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    ptr::NonNull,
};

use anyhow::{Context, Result};
use log::info;
use rkyv::{
    ser::{serializers::AllocSerializer, ScratchSpace, Serializer},
    with::AsStringError,
    AlignedVec, Archive, Deserialize, Fallible, Serialize,
};
use shinran_config::{
    all_config_files,
    config::{generate_match_paths, ParsedConfig, ProfileRef, ProfileStore},
    matches::store::MatchStore,
};

use crate::{
    get_path_override, load,
    path::{self, load_and_mod_time, most_recent_modification},
};

/// A struct containing all the information that was loaded from match files and config files.
#[derive(Archive, Serialize, Deserialize)]
#[archive(check_bytes)]
pub struct Configuration {
    pub profile_store: ProfileStore,
    pub match_store: MatchStore,
    /// Renderer for the variables.
    pub renderer: shinran_render::Renderer,
    pub loaded_from_cache: bool,
}

impl Configuration {
    pub fn new(cli_overrides: &HashMap<String, String>) -> (Self, PathBuf) {
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

        let cache_path = paths.runtime.join("cache.bin");
        let config_path = paths.config.join("config");

        match load_cache(&cache_path, &config_path) {
            Ok(mut config) => {
                config.loaded_from_cache = true;
                return (config, cache_path);
            }
            Err(e) => {
                // Log the error chain.
                log::warn!("Failed to load configuration cache: {:#}", e);

                // Inspect specific error causes:
                for cause in e.chain() {
                    log::debug!("Caused by: {}", cause);
                }
            }
        }

        let config_result = load::load_config(&paths.config).expect("unable to load config");

        let home_path = dirs::home_dir().expect("unable to obtain home dir path");
        let base_path = &paths.config;
        let packages_path = &paths.packages;
        let renderer = shinran_render::Renderer::new(base_path, &home_path, packages_path);

        let cfg = Configuration {
            profile_store: config_result.profile_store,
            match_store: config_result.match_store,
            renderer,
            loaded_from_cache: false,
        };

        (cfg, cache_path)
    }

    pub fn serialize(&self) -> AlignedVec {
        // We can construct our serializer in much the same way as we always do
        let mut serializer = MySerializer::<AllocSerializer<1024>>::default();
        // then manually serialize our value
        serializer.serialize_value(self).unwrap();
        // and finally, dig all the way down to our byte buffer
        serializer.into_inner().into_serializer().into_inner()
    }

    /// Get the active configuration file according to the current app.
    ///
    /// This functionality is not implemented yet.
    pub fn active_profile(&self) -> ProfileRef {
        // let current_app = self.app_info_provider.get_info();
        // let info = to_app_properties(&current_app);
        let info = shinran_config::config::AppProperties {
            title: None,
            class: None,
            exec: None,
        };
        self.profile_store.active_config(&info)
    }
}

fn load_cache(cache_path: &Path, config_dir: &Path) -> Result<Configuration> {
    // Load file and get modification time.
    let (bytes, cache_mod_time) = load_and_mod_time(cache_path)
        .with_context(|| format!("Failed to read cache file at {}", cache_path.display()))?;

    // Parse the archived configuration.
    // This does not deserialize the configuration yet and so does not allocate any memory.
    let Ok(archived_config) = rkyv::check_archived_root::<Configuration>(&bytes) else {
        anyhow::bail!("Cache file byte check failed.");
    };

    // Collect source paths without deserializing, directly from the archived configuration.
    let mut config_paths = Vec::new();
    config_paths.extend(archived_config.profile_store.get_source_paths());
    config_paths.extend(archived_config.match_store.get_source_paths());

    let config_paths_set: HashSet<&Path> = config_paths.iter().copied().collect();

    // Check modification times and also check whether the files are still present.
    let config_mod_time = most_recent_modification(&config_paths)
        .with_context(|| "Failed to check modification times of configuration files")?;

    // Check if cache is stale.
    if config_mod_time > cache_mod_time {
        anyhow::bail!(
            "Cache is outdated - configuration files have been modified since cache was created"
        );
    }

    // Check whether there are any new files that were not present when the cache was created.
    if all_config_files(config_dir)
        .with_context(|| "Failed to list all configuration files".to_string())?
        .any(|found_path| !config_paths_set.contains(&found_path.as_path()))
    {
        anyhow::bail!("New configuration files have been added since cache was created");
    }

    // Check whether there are any new match files.
    // Each profile file has a regex for match files, which is evaluated by `generate_match_paths`.
    // So, we iterate over all profile files and collect all match paths.
    for profile_file in archived_config.profile_store.get_parsed_configs() {
        // Deserialize just the parsed config.
        let parsed_config: ParsedConfig =
            profile_file
                .content
                .deserialize(&mut rkyv::Infallible)
                .with_context(|| "Failed to deserialize parsed configuration from cache")?;
        let source_path = Path::new(profile_file.source_path.as_str());
        let Some(match_paths) = generate_match_paths(&parsed_config, source_path) else {
            anyhow::bail!("Failed to generate match paths from parsed configuration");
        };
        let match_paths: HashSet<&Path> = match_paths.iter().map(|p| p.as_path()).collect();
        // Check whether the hash set `match_paths` is contained in the hash set `config_paths_set`.
        if !match_paths.is_subset(&config_paths_set) {
            anyhow::bail!("New match files have been added since cache was created");
        }
    }

    // Everything is fine -> deserialize the configuration
    let deserialized_config = archived_config
        .deserialize(&mut rkyv::Infallible)
        .with_context(|| "Failed to deserialize configuration from cache")?;

    Ok(deserialized_config)
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
