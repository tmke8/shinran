use std::{collections::HashMap, path::PathBuf};

use espanso_config::{config::ConfigStore, matches::store::MatchStore};
use log::info;

mod builtin;
mod config;
mod cursor;
mod engine;
mod event;
mod load;
mod match_cache;
mod match_select;
mod multiplex;
mod path;
mod regex;
mod render;

fn load_config_and_renderer(
    cli_overrides: &HashMap<String, String>,
) -> (espanso_render::Renderer, ConfigStore, MatchStore) {
    // See also
    // `initialize_and_spawn()`
    // in `espanso/src/cli/worker/engine/mod.rs`.
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

    let config_result = load::load_config(&paths.config).expect("unable to load config");
    let config_store = config_result.config_store;
    let match_store = config_result.match_store;

    let home_path = dirs::home_dir().expect("unable to obtain home dir path");
    let config_path = &paths.config;
    let packages_path = &paths.packages;
    let renderer = espanso_render::Renderer::new(config_path, &home_path, packages_path);

    (renderer, config_store, match_store)
}

fn get_regex_matches(
    config_store: &ConfigStore,
    match_store: &MatchStore,
) -> Vec<regex::RegexMatch<i32>> {
    let paths = config_store.get_all_match_paths();
    let global_set = match_store.query(&paths.into_iter().collect::<Vec<_>>());
    let mut regex_matches = Vec::new();

    for m in global_set.matches {
        if let espanso_config::matches::MatchCause::Regex(cause) = &m.cause {
            regex_matches.push(regex::RegexMatch::new(m.id, &cause.regex));
        }
    }
    regex_matches
}

pub struct Backend {
    adapter: render::RendererAdapter,
}

impl Backend {
    pub fn new(cli_overrides: &HashMap<String, String>) -> anyhow::Result<Backend> {
        let (renderer, config_store, match_store) = load_config_and_renderer(cli_overrides);

        let match_cache = match_cache::MatchCache::load(&config_store, &match_store);
        let regex_matches = get_regex_matches(&config_store, &match_store);

        // `config_manager` could own `match_store`
        let config_manager = config::ConfigManager::new(config_store, match_store);

        let builtin_matches = builtin::get_builtin_matches();
        // `combined_cache` stores references to `cache` and `builtin_matches`
        let combined_cache =
            match_cache::CombinedMatchCache::load(match_cache, builtin_matches, regex_matches);
        // `adapter` could own `cache`
        let adapter = render::RendererAdapter::new(combined_cache, config_manager, renderer);
        Ok(Backend { adapter })
    }

    pub fn check_trigger(&self, trigger: &str) -> anyhow::Result<Option<String>> {
        let matches = self
            .adapter
            .combined_cache
            .find_matches_from_trigger(trigger);
        let match_ = if let Some(match_) = matches.into_iter().next() {
            match_
        } else {
            let matches = self
                .adapter
                .combined_cache
                .matcher
                .find_matches(trigger)
                .into_iter()
                .next();
            if let Some(matches) = matches {
                matches
            } else {
                return Ok(None);
            }
        };
        self.adapter
            .render(match_.id, Some(trigger), match_.args)
            .map(|body| Some(cursor::process_cursor_hint(body).0))
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
    fn test_date() {
        let cli_overrides = HashMap::new();
        let backend = Backend::new(&cli_overrides).unwrap();
        // let trigger = "date";
        let trigger = "greet(Bob)";
        let result = backend.check_trigger(trigger).unwrap().unwrap();
        println!("{result}");
    }
}
