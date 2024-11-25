use std::{collections::HashMap, path::PathBuf};

use shinran_config::{config::ProfileStore, matches::store::MatchStore};
use log::info;
use shinran_types::RegexMatchRef;

mod builtin;
mod config;
mod cursor;
mod engine;
mod event;
mod load;
mod match_cache;
mod path;
mod regex;
mod render;

pub struct Stores {
    pub profiles: ProfileStore,
    pub matches: MatchStore,
    pub renderer: shinran_render::Renderer,
}

pub fn load_config_and_renderer(cli_overrides: &HashMap<String, String>) -> Stores {
    let force_config_path = get_path_override(cli_overrides, "config_dir", "SHINRAN_CONFIG_DIR");
    let force_package_path = get_path_override(cli_overrides, "package_dir", "SHINRAN_PACKAGE_DIR");
    let force_runtime_path = get_path_override(cli_overrides, "runtime_dir", "SHINRAN_RUNTIME_DIR");

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

    Stores {
        profiles: profile_store,
        matches: match_store,
        renderer,
    }
}

fn get_regex_matches(
    _: &ProfileStore,
    match_store: &MatchStore,
) -> Vec<regex::RegexMatch<RegexMatchRef>> {
    let mut regex_matches = Vec::new();

    // TODO: This should take into account the current profile.
    for (match_idx, (regex, _)) in match_store.regex_matches.enumerate() {
        regex_matches.push(regex::RegexMatch::new(match_idx, regex.clone()));
    }
    regex_matches
}

pub struct Backend<'store> {
    adapter: render::RendererAdapter<'store>,
}

impl<'store> Backend<'store> {
    pub fn new(stores: &'store Stores) -> anyhow::Result<Self> {
        let match_cache = match_cache::MatchCache::load(&stores.profiles, &stores.matches);
        let regex_matches = get_regex_matches(&stores.profiles, &stores.matches);

        let configuration = config::Configuration::new(&stores.profiles, &stores.matches);

        let builtin_matches = builtin::get_builtin_matches();
        // `combined_cache` stores references to `cache` and `builtin_matches`
        let combined_cache =
            match_cache::CombinedMatchCache::load(match_cache, builtin_matches, regex_matches);
        // `adapter` could own `cache`
        let adapter = render::RendererAdapter::new(combined_cache, configuration, &stores.renderer);
        Ok(Backend { adapter })
    }

    pub fn check_trigger(&self, trigger: &str) -> anyhow::Result<Option<String>> {
        let active_profile = self.adapter.configuration.active_profile();
        let matches = self
            .adapter
            .find_matches_from_trigger(trigger, active_profile);
        let match_ = if let Some(match_) = matches.into_iter().next() {
            match_
        } else {
            let matches = self.adapter.find_regex_matches(trigger).into_iter().next();
            if let Some(matches) = matches {
                matches
            } else {
                return Ok(None);
            }
        };
        self.adapter
            .render(match_.id, Some(trigger), match_.args, active_profile)
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
    use std::path::Path;

    use shinran_helpers::use_test_directory;

    use super::*;

    fn make_stores(
        match_definition: &str,
        base_path: &Path,
        match_dir: &Path,
        config_dir: &Path,
    ) -> Stores {
        let base_file = match_dir.join("base.yml");
        std::fs::write(&base_file, match_definition).unwrap();

        let default_file = config_dir.join("default.yml");
        std::fs::write(&default_file, "").unwrap();

        let mut cli_overrides = HashMap::new();
        cli_overrides.insert(
            "config_dir".to_string(),
            base_path.to_str().unwrap().to_string(),
        );
        load_config_and_renderer(&cli_overrides)
    }

    #[test]
    fn test_hello_world() {
        use_test_directory(|base_path, match_dir, config_dir| {
            let match_definition = r#"
                    matches:
                      - trigger: "hello"
                        replace: "world"
            "#;
            let stores = make_stores(match_definition, base_path, match_dir, config_dir);
            let backend = Backend::new(&stores).unwrap();
            let result = backend.check_trigger("hello").unwrap().unwrap();
            assert_eq!(result, "world");
        });
    }

    #[test]
    fn test_regex() {
        use_test_directory(|base_path, match_dir, config_dir| {
            let match_definition = r#"
                matches:
                - regex: "greet\\((?P<person>.*)\\)"
                  replace: "Hi {{person}}!"
            "#;
            let stores = make_stores(match_definition, base_path, match_dir, config_dir);
            let backend = Backend::new(&stores).unwrap();
            let result = backend.check_trigger("greet(Bob)").unwrap().unwrap();
            assert_eq!(result, "Hi Bob!");
        });
    }

    #[test]
    fn test_date() {
        use_test_directory(|base_path, match_dir, config_dir| {
            let match_definition = r#"
                matches:
                - trigger: "now"
                  replace: "It's {{mytime}}"
                  vars:
                    - name: mytime
                      type: date
                      params:
                        format: "%H:%M"
            "#;
            let stores = make_stores(match_definition, base_path, match_dir, config_dir);
            let backend = Backend::new(&stores).unwrap();
            backend.check_trigger("now").unwrap().unwrap();
            // assert_eq!(result, "It's 14:45");
        });
    }

    #[test]
    fn test_global_vars() {
        use_test_directory(|base_path, match_dir, config_dir| {
            let match_definition = r#"
                global_vars:
                  - name: myname
                    type: echo
                    params:
                      echo: Jon

                matches:
                  - trigger: ":hello"
                    replace: "hello {{myname}}"
            "#;
            let stores = make_stores(match_definition, base_path, match_dir, config_dir);
            let backend = Backend::new(&stores).unwrap();
            let result = backend.check_trigger(":hello").unwrap().unwrap();
            assert_eq!(result, "hello Jon");
        });
    }

    #[test]
    fn test_global_inside_local_vars() {
        use_test_directory(|base_path, match_dir, config_dir| {
            let match_definition = r#"
                global_vars:
                  - name: firstname
                    type: echo
                    params:
                      echo: Jon
                  - name: lastname
                    type: echo
                    params:
                      echo: Snow

                matches:
                  - trigger: ":hello"
                    replace: "hello {{fullname}}"
                    vars:
                      - name: fullname
                        type: echo
                        params:
                          echo: "{{firstname}} {{lastname}}"
            "#;
            let stores = make_stores(match_definition, base_path, match_dir, config_dir);
            let backend = Backend::new(&stores).unwrap();
            let result = backend.check_trigger(":hello").unwrap().unwrap();
            assert_eq!(result, "hello Jon Snow");
        });
    }

    #[test]
    fn test_nested_matches() {
        use_test_directory(|base_path, match_dir, config_dir| {
            let match_definition = r#"
                matches:
                - trigger: :one
                  replace: nested

                - trigger: :nested
                  replace: This is a {{output}} match
                  vars:
                    - name: output
                      type: match
                      params:
                        trigger: :one
            "#;
            let stores = make_stores(match_definition, base_path, match_dir, config_dir);
            let backend = Backend::new(&stores).unwrap();
            let result = backend.check_trigger(":nested").unwrap().unwrap();
            assert_eq!(result, "This is a nested match");
        });
    }

    #[test]
    fn test_nested_regex_matches() {
        use_test_directory(|base_path, match_dir, config_dir| {
            let match_definition = r#"
                matches:
                - trigger: :one
                  replace: nested

                - regex: ":greet\\d"
                  replace: This is a {{output}} match
                  vars:
                    - name: output
                      type: match
                      params:
                        trigger: :one
            "#;
            let stores = make_stores(match_definition, base_path, match_dir, config_dir);
            let backend = Backend::new(&stores).unwrap();
            let result = backend.check_trigger(":greet2").unwrap().unwrap();
            assert_eq!(result, "This is a nested match");
        });
    }

    #[test]
    fn test_nested_regex_matches2() {
        use_test_directory(|base_path, match_dir, config_dir| {
            let match_definition = r#"
                matches:
                - regex: :one
                  replace: nested

                - trigger: ":nested"
                  replace: This is a {{output}} match
                  vars:
                  - name: output
                    type: match
                    params:
                      trigger: :one
            "#;
            let stores = make_stores(match_definition, base_path, match_dir, config_dir);
            let backend = Backend::new(&stores).unwrap();
            // TODO: Figure out whether this should be an error or not.
            backend.check_trigger(":nested").unwrap_err();
        });
    }

    #[test]
    fn test_unicode() {
        use_test_directory(|base_path, match_dir, config_dir| {
            let match_definition = r#"
                matches:
                - trigger: :euro
                  replace: "\u20ac"
            "#;
            let stores = make_stores(match_definition, base_path, match_dir, config_dir);
            let backend = Backend::new(&stores).unwrap();
            let result = backend.check_trigger(":euro").unwrap().unwrap();
            assert_eq!(result, "€");
            let result = backend.check_trigger(":Euro").unwrap_err();
            assert_eq!(result.to_string(), "match not found");
        });
    }

    #[test]
    fn test_case_propagation() {
        use_test_directory(|base_path, match_dir, config_dir| {
            let match_definition = r#"
                matches:
                - trigger: alh
                  replace: although
                  propagate_case: true
            "#;
            let stores = make_stores(match_definition, base_path, match_dir, config_dir);
            let backend = Backend::new(&stores).unwrap();
            let result = backend.check_trigger("alh").unwrap().unwrap();
            assert_eq!(result, "although");
            let result = backend.check_trigger("Alh").unwrap().unwrap();
            assert_eq!(result, "Although");
            let result = backend.check_trigger("ALH").unwrap().unwrap();
            assert_eq!(result, "ALTHOUGH");
        });
    }

    #[test]
    fn test_case_propagation_advanced() {
        use_test_directory(|base_path, match_dir, config_dir| {
            let match_definition = r#"
                matches:
                - trigger: ;ols
                  replace: ordinary least squares
                  uppercase_style: capitalize_words
                  propagate_case: true
            "#;
            let stores = make_stores(match_definition, base_path, match_dir, config_dir);
            let backend = Backend::new(&stores).unwrap();
            let result = backend.check_trigger(";ols").unwrap().unwrap();
            assert_eq!(result, "ordinary least squares");
            let result = backend.check_trigger(";Ols").unwrap().unwrap();
            assert_eq!(result, "Ordinary Least Squares");
        });
    }

    #[test]
    fn test_case_multiple_triggers() {
        use_test_directory(|base_path, match_dir, config_dir| {
            let match_definition = r#"
            matches:
            - triggers: [hello, hi]
              replace: world
        "#;
            let stores = make_stores(match_definition, base_path, match_dir, config_dir);
            let backend = Backend::new(&stores).unwrap();
            let result = backend.check_trigger("hello").unwrap().unwrap();
            assert_eq!(result, "world");
            let result = backend.check_trigger("hi").unwrap().unwrap();
            assert_eq!(result, "world");
        });
    }
}
