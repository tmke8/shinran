#[cfg(test)]
mod tests {
    use fontconfig::Fontconfig;

    #[test]
    fn font_test() {
        let fc = Fontconfig::new().unwrap();
        // `Fontconfig::find()` returns `Option` (will rarely be `None` but still could be)
        let font = fc.find("monospace", None).unwrap();
        // `name` is a `String`, `path` is a `Path`
        println!("Name: {}\nPath: {}", font.name, font.path.display());
    }

    use fontdb::{Database, Family, Query};

    #[test]
    fn fontdb_test() {
        let mut db = Database::new();
        load_system_fonts(&mut db);
        let family = Family::Monospace;
        let name = db.family_name(&family);
        println!("Name: {}", name);
        let font_id = db
            .query(&Query {
                // families: &[Family::Name("DejaVu Sans Mono")],
                // families: &[Family::Name("JetBrains Mono NL")],
                families: &[family],
                ..Default::default()
            })
            .unwrap();
        println!("Source: {:?}", db.face(font_id).unwrap().source);
    }

    pub fn load_system_fonts(db: &mut Database) {
        if !load_fontconfig(db) {
            // log::warn!("Fallback to loading from known font dir paths.");
            load_no_fontconfig(db);
        }
    }

    fn load_no_fontconfig(db: &mut Database) {
        db.load_fonts_dir("/usr/share/fonts/");
        db.load_fonts_dir("/usr/local/share/fonts/");

        if let Ok(ref home) = std::env::var("HOME") {
            let home_path = std::path::Path::new(home);
            db.load_fonts_dir(&home_path.join(".fonts"));
            db.load_fonts_dir(&home_path.join(".local/share/fonts"));
        }
    }

    fn load_fontconfig(db: &mut Database) -> bool {
        use std::path::Path;

        let mut fontconfig = fontconfig_parser::FontConfig::default();
        let home = std::env::var("HOME");

        if let Ok(ref config_file) = std::env::var("FONTCONFIG_FILE") {
            let _ = fontconfig.merge_config(Path::new(config_file));
        } else {
            let xdg_config_home = if let Ok(val) = std::env::var("XDG_CONFIG_HOME") {
                Some(val.into())
            } else if let Ok(ref home) = home {
                // according to https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html
                // $XDG_CONFIG_HOME should default to $HOME/.config if not set
                Some(Path::new(home).join(".config"))
            } else {
                None
            };

            let read_global = match xdg_config_home {
                Some(p) => fontconfig
                    .merge_config(&p.join("fontconfig/fonts.conf"))
                    .is_err(),
                None => true,
            };

            if read_global {
                let _ = fontconfig.merge_config(Path::new("/etc/fonts/local.conf"));
            }
            let _ = fontconfig.merge_config(Path::new("/etc/fonts/fonts.conf"));
        }
        for fontconfig_parser::Alias {
            alias,
            default,
            prefer,
            accept,
        } in fontconfig.aliases
        {
            let name = prefer
                .get(0)
                .or_else(|| accept.get(0))
                .or_else(|| default.get(0));

            if let Some(name) = name {
                match alias.to_lowercase().as_str() {
                    "serif" => db.set_serif_family(name),
                    "sans-serif" => db.set_sans_serif_family(name),
                    "sans serif" => db.set_sans_serif_family(name),
                    "monospace" => {
                        println!("Monospace: {}", name);
                        println!("Prefer: {:?}", prefer);
                        db.set_monospace_family(name)
                    }
                    "cursive" => db.set_cursive_family(name),
                    "fantasy" => db.set_fantasy_family(name),
                    _ => {}
                }
            }
        }

        if fontconfig.dirs.is_empty() {
            return false;
        }

        for dir in fontconfig.dirs {
            let path = if dir.path.starts_with("~") {
                if let Ok(ref home) = home {
                    Path::new(home).join(dir.path.strip_prefix("~").unwrap())
                } else {
                    continue;
                }
            } else {
                dir.path
            };
            db.load_fonts_dir(&path);
        }

        true
    }
}
