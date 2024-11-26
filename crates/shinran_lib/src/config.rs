use std::collections::HashMap;

use log::info;
use shinran_config::{config::ProfileStore, matches::store::MatchStore};

use crate::{get_path_override, load, path};

pub struct Configuration {
    pub profiles: ProfileStore,
    pub matches: MatchStore,
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
        let profile_store = config_result.profile_store;
        let match_store = config_result.match_store;

        let home_path = dirs::home_dir().expect("unable to obtain home dir path");
        let base_path = &paths.config;
        let packages_path = &paths.packages;
        let renderer = shinran_render::Renderer::new(base_path, &home_path, packages_path);

        Configuration {
            profiles: profile_store,
            matches: match_store,
            renderer,
        }
    }
}
