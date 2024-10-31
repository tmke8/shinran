use std::{collections::HashMap, path::PathBuf, time::SystemTime};

use espanso_config::{config::ConfigStore, matches::store::MatchStore};
use log::info;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

mod builtin;
mod config;
mod engine;
mod event;
mod load;
mod match_cache;
mod match_select;
mod multiplex;
mod path;
mod render;

pub fn check_command(command: &str) -> Option<String> {
    match command {
        "times" => Some("×".to_string()),
        "time" => Some(time_now()),
        _ => None,
    }
}

fn time_now() -> String {
    let now: OffsetDateTime = SystemTime::now().into();
    now.format(&Rfc3339).expect("valid date time")
}

fn get_extensions(paths: path::Paths) -> Vec<Box<dyn espanso_render::Extension>> {
    let date_extension = espanso_render::extension::date::DateExtension::new();
    let echo_extension = espanso_render::extension::echo::EchoExtension::new();
    // For backwards compatiblity purposes, the echo extension can also be called with "dummy" type
    let dummy_extension = espanso_render::extension::echo::EchoExtension::new_with_alias("dummy");
    let random_extension = espanso_render::extension::random::RandomExtension::new();
    let home_path = dirs::home_dir().expect("unable to obtain home dir path");
    let script_extension = espanso_render::extension::script::ScriptExtension::new(
        &paths.config,
        &home_path,
        &paths.packages,
    );
    let shell_extension = espanso_render::extension::shell::ShellExtension::new(&paths.config);
    vec![
        Box::new(date_extension),
        Box::new(echo_extension),
        Box::new(dummy_extension),
        Box::new(random_extension),
        Box::new(script_extension),
        Box::new(shell_extension),
    ]
}

pub fn load_config_and_renderer() -> (espanso_render::Renderer, ConfigStore, MatchStore) {
    // See also
    // `initialize_and_spawn()`
    // in `espanso/src/cli/worker/engine/mod.rs`.
    let cli_overrides = HashMap::new();
    let force_config_path = get_path_override(&cli_overrides, "config_dir", "ESPANSO_CONFIG_DIR");
    let force_package_path =
        get_path_override(&cli_overrides, "package_dir", "ESPANSO_PACKAGE_DIR");
    let force_runtime_path =
        get_path_override(&cli_overrides, "runtime_dir", "ESPANSO_RUNTIME_DIR");

    let paths = path::resolve_paths(
        force_config_path.as_deref(),
        force_package_path.as_deref(),
        force_runtime_path.as_deref(),
    );
    info!("reading configs from: {:?}", paths.config);
    info!("reading packages from: {:?}", paths.packages);
    info!("using runtime dir: {:?}", paths.runtime);

    let config_result =
        load::load_config(&paths.config, &paths.packages).expect("unable to load config");
    let config_store = config_result.config_store;
    let match_store = config_result.match_store;

    let extensions = get_extensions(paths);
    let renderer = espanso_render::Renderer::new(extensions);

    (renderer, config_store, match_store)
}

pub fn make_cache<'store>(
    config_store: ConfigStore,
    match_store: &'store MatchStore,
) -> (
    match_cache::MatchCache<'store>,
    config::ConfigManager<'store>,
    Vec<builtin::BuiltInMatch>,
) {
    let match_cache = match_cache::MatchCache::load(&config_store, match_store);

    // `config_manager` could own `match_store`
    let config_manager = config::ConfigManager::new(config_store, match_store);

    let config = &*config_manager.default();
    let builtin_matches = builtin::get_builtin_matches(config);
    (match_cache, config_manager, builtin_matches)
}

pub struct Backend<'a> {
    adapter: render::RendererAdapter<'a>,
    cache: match_cache::CombinedMatchCache<'a>,
}

impl<'a> Backend<'a> {
    pub fn new(
        renderer: espanso_render::Renderer,
        match_cache: &'a match_cache::MatchCache,
        config_manager: config::ConfigManager<'a>,
        builtin_matches: &'a Vec<builtin::BuiltInMatch>,
    ) -> anyhow::Result<Backend<'a>> {
        // `combined_cache` stores references to `cache` and `builtin_matches`
        let combined_cache = match_cache::CombinedMatchCache::load(match_cache, builtin_matches);
        // `adapter` could own `cache`
        let adapter = render::RendererAdapter::new(&match_cache, config_manager, renderer);
        Ok(Backend {
            adapter,
            cache: combined_cache,
        })
    }

    pub fn check_trigger(&'a self, trigger: &str) -> anyhow::Result<String> {
        let matches = self.cache.find_matches_from_trigger(trigger);
        let match_ = matches.into_iter().next().unwrap();
        self.adapter.render(match_.id, Some(trigger), match_.args)
    }
}

macro_rules! error_eprintln {
  ($($tts:tt)*) => {
    eprintln!($($tts)*);
    log::error!($($tts)*);
  }
}

fn get_path_override(
    cli_overrides: &HashMap<String, String>,
    argument: &str,
    env_var: &str,
) -> Option<PathBuf> {
    if let Some(path) = cli_overrides.get(argument) {
        let path = PathBuf::from(path.trim());
        if path.is_dir() {
            Some(path)
        } else {
            error_eprintln!("{} argument was specified, but it doesn't point to a valid directory. Make sure to create it first.", argument);
            std::process::exit(1);
        }
    } else if let Ok(path) = std::env::var(env_var) {
        let path = PathBuf::from(path.trim());
        if path.is_dir() {
            Some(path)
        } else {
            error_eprintln!("{} env variable was specified, but it doesn't point to a valid directory. Make sure to create it first.", env_var);
            std::process::exit(1);
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_command() {
        assert!(check_command("hello").is_none());
    }

    #[test]
    fn all() {
        let (renderer, config_store, match_store) = load_config_and_renderer();
        let (match_cache, config_manager, builtin_matches) = make_cache(config_store, &match_store);
        let backend =
            Backend::new(renderer, &match_cache, config_manager, &builtin_matches).unwrap();
        let trigger = ":date";
        let result = backend.check_trigger(trigger).unwrap();
        println!("{result}");
    }
}
