use std::{path::Path, time::SystemTime};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

mod builtin;
mod config;
mod engine;
mod match_cache;
mod match_select;
mod multiplex;
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

fn get_extensions() -> Vec<Box<dyn espanso_render::Extension>> {
    let date_extension = espanso_render::extension::date::DateExtension::new(&locale_provider);
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

pub fn setup(trigger: &str) -> (render::RendererAdapter, match_cache::CombinedMatchCache) {
    // See also
    // `initialize_and_spawn()`
    // in `espanso/src/cli/worker/engine/mod.rs`.
    let current_directory = Path::new(".");
    let (config, matches, errors) = espanso_config::load(current_directory).unwrap();
    let cache = match_cache::MatchCache::load(&config, &matches);
    let manager = config::ConfigManager::new(&config, &matches);
    let extensions = get_extensions();
    let renderer = espanso_render::DefaultRenderer::new(extensions);
    let adapter = render::RendererAdapter::new(&cache, &manager, &renderer);
    let builtin_matches = builtin::get_builtin_matches(&*manager.default());
    let combined_cache = match_cache::CombinedMatchCache::load(&cache, &builtin_matches);
    (adapter, combined_cache)
}

pub fn check_trigger(
    adapter: render::RendererAdapter,
    cache: match_cache::CombinedMatchCache,
    trigger: &str,
) -> anyhow::Result<String> {
    let matches = cache.find_matches_from_trigger(trigger);
    let match_ = matches[0];
    adapter.render(match_.id, Some(trigger), match_.args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_command() {
        assert!(check_command("hello").is_none());
    }
}
