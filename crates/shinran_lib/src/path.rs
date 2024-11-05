/*
 * This file is part of espanso.
 *
 * Copyright (C) 2019-2021 Federico Terzi
 *
 * espanso is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * espanso is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with espanso.  If not, see <https://www.gnu.org/licenses/>.
 */

use log::{debug, info};
use std::{
    fs::create_dir_all,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct Paths {
    pub config: PathBuf,
    pub runtime: PathBuf,
    pub packages: PathBuf,
}

pub fn resolve_paths(
    force_config_dir: Option<&Path>,
    force_package_dir: Option<&Path>,
    force_runtime_dir: Option<&Path>,
) -> Paths {
    let config_dir = if let Some(config_dir) = force_config_dir {
        config_dir.to_path_buf()
    } else if let Some(config_dir) = get_config_dir() {
        config_dir
    } else {
        // Create the config directory if not already present
        let config_dir = get_default_config_path();
        info!("creating config directory in {:?}", config_dir);
        create_dir_all(&config_dir).expect("unable to create config directory");
        config_dir
    };

    let runtime_dir = if let Some(runtime_dir) = force_runtime_dir {
        runtime_dir.to_path_buf()
    } else if let Some(runtime_dir) = get_runtime_dir() {
        runtime_dir
    } else {
        // Create the runtime directory if not already present
        let runtime_dir = if is_portable_mode() {
            get_portable_runtime_path().expect("unable to obtain runtime directory path")
        } else {
            get_default_runtime_path()
        };
        info!("creating runtime directory in {:?}", runtime_dir);
        create_dir_all(&runtime_dir).expect("unable to create runtime directory");
        runtime_dir
    };

    let packages_dir = if let Some(package_dir) = force_package_dir {
        package_dir.to_path_buf()
    } else if let Some(package_dir) = get_packages_dir(&config_dir) {
        package_dir
    } else {
        // Create the packages directory if not already present
        let packages_dir = get_default_packages_path(&config_dir);
        info!("creating packages directory in {:?}", packages_dir);
        create_dir_all(&packages_dir).expect("unable to create packages directory");
        packages_dir
    };

    Paths {
        config: config_dir,
        runtime: runtime_dir,
        packages: packages_dir,
    }
}

fn get_config_dir() -> Option<PathBuf> {
    if let Some(portable_dir) = get_portable_config_dir() {
        // Portable mode
        debug!("detected portable config directory in {:?}", portable_dir);
        Some(portable_dir)
    } else if let Some(config_dir) = get_home_espanso_dir() {
        // $HOME/.espanso
        debug!("detected config directory in $HOME/.espanso");
        Some(config_dir)
    } else if let Some(config_dir) = get_home_config_espanso_dir() {
        // $HOME/.config/espanso
        debug!("detected config directory in $HOME/.config/espanso");
        Some(config_dir)
    } else if let Some(config_dir) = get_default_config_dir() {
        debug!("detected default config directory at {:?}", config_dir);
        Some(config_dir)
    } else {
        None
    }
}

fn get_portable_config_dir() -> Option<PathBuf> {
    let espanso_exe_path = std::env::current_exe().expect("unable to obtain executable path");
    let exe_dir = espanso_exe_path.parent();
    if let Some(parent) = exe_dir {
        let config_dir = parent.join(".espanso");
        if config_dir.is_dir() {
            return Some(config_dir);
        }
    }
    None
}

fn get_home_espanso_dir() -> Option<PathBuf> {
    if let Some(home_dir) = dirs::home_dir() {
        let config_espanso_dir = home_dir.join(".espanso");
        if config_espanso_dir.is_dir() {
            return Some(config_espanso_dir);
        }
    }
    None
}

fn get_home_config_espanso_dir() -> Option<PathBuf> {
    if let Some(home_dir) = dirs::home_dir() {
        let home_espanso_dir = home_dir.join(".config").join("espanso");
        if home_espanso_dir.is_dir() {
            return Some(home_espanso_dir);
        }
    }
    None
}

fn get_default_config_dir() -> Option<PathBuf> {
    let config_path = get_default_config_path();
    if config_path.is_dir() {
        return Some(config_path);
    }
    None
}

fn get_default_config_path() -> PathBuf {
    let config_dir = dirs::config_dir().expect("unable to obtain dirs::config_dir()");
    config_dir.join("espanso")
}

fn get_runtime_dir() -> Option<PathBuf> {
    if let Some(runtime_dir) = get_portable_runtime_dir() {
        debug!("detected portable runtime dir: {:?}", runtime_dir);
        Some(runtime_dir)
    } else if let Some(default_dir) = get_default_runtime_dir() {
        debug!("detected default runtime dir: {:?}", default_dir);
        Some(default_dir)
    } else {
        None
    }
}

fn get_portable_runtime_dir() -> Option<PathBuf> {
    if let Some(runtime_dir) = get_portable_runtime_path() {
        if runtime_dir.is_dir() {
            return Some(runtime_dir);
        }
    }
    None
}

fn get_portable_runtime_path() -> Option<PathBuf> {
    let espanso_exe_path = std::env::current_exe().expect("unable to obtain executable path");
    let exe_dir = espanso_exe_path.parent();
    if let Some(parent) = exe_dir {
        let config_dir = parent.join(".espanso-runtime");
        return Some(config_dir);
    }
    None
}

fn get_default_runtime_dir() -> Option<PathBuf> {
    let default_dir = get_default_runtime_path();
    if default_dir.is_dir() {
        Some(default_dir)
    } else {
        None
    }
}

fn get_default_runtime_path() -> PathBuf {
    let runtime_dir = dirs::cache_dir().expect("unable to obtain dirs::cache_dir()");
    runtime_dir.join("espanso")
}

fn get_packages_dir(config_dir: &Path) -> Option<PathBuf> {
    if let Some(packages_dir) = get_default_packages_dir(config_dir) {
        debug!("detected default packages dir: {:?}", packages_dir);
        Some(packages_dir)
    } else {
        None
    }
}

fn get_default_packages_dir(config_dir: &Path) -> Option<PathBuf> {
    let packages_dir = get_default_packages_path(config_dir);
    if packages_dir.is_dir() {
        Some(packages_dir)
    } else {
        None
    }
}

fn get_default_packages_path(config_dir: &Path) -> PathBuf {
    config_dir.join("match").join("packages")
}

fn is_portable_mode() -> bool {
    let espanso_exe_path = std::env::current_exe().expect("unable to obtain executable path");
    let exe_dir = espanso_exe_path.parent();
    if let Some(parent) = exe_dir {
        let config_dir = parent.join(".espanso");
        if config_dir.is_dir() {
            return true;
        }
    }
    false
}
