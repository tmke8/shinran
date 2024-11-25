use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

use zbus::Address;

pub fn get_ibus_address() -> zbus::Result<Address> {
    if let Ok(address) = env::var("IBUS_ADDRESS") {
        if !address.is_empty() {
            return address.as_str().try_into();
        }
    }

    let path = get_socket_path()?;
    let data = fs::read_to_string(path)?;

    let mut address = "";
    for line in data.lines() {
        if let Some(addr) = line.strip_prefix("IBUS_ADDRESS=") {
            address = addr;
        }
    }

    address.try_into()
}

fn get_socket_path() -> io::Result<PathBuf> {
    if let Ok(path) = env::var("IBUS_ADDRESS_FILE") {
        if !path.is_empty() {
            return Ok(PathBuf::from(path));
        }
    }

    let wayland_display = env::var("WAYLAND_DISPLAY");
    let display: Option<String>;
    let (display_number, hostname) = match &wayland_display {
        Ok(val) if !val.is_empty() => (val.as_str(), None),
        _ => {
            display = env::var("DISPLAY").ok();
            match &display {
                Some(display) if !display.is_empty() => {
                    let (hostname, display_number) =
                        parse_display_string(display).ok_or_else(|| {
                            eprintln!("Failed to parse DISPLAY: {}", display);
                            io::Error::new(io::ErrorKind::InvalidData, "Failed to parse DISPLAY")
                        })?;
                    let hostname = if hostname.is_empty() {
                        None
                    } else {
                        Some(hostname)
                    };
                    (display_number, hostname)
                }
                _ => {
                    eprintln!("DISPLAY is empty! We use default DISPLAY (:0.0)");
                    ("0", None)
                }
            }
        }
    };

    let hostname = hostname.unwrap_or("unix");

    let machine_id = get_machine_id()?;
    let path = get_user_config_dir().join(format!(
        "ibus/bus/{}-{}-{}",
        machine_id, hostname, display_number
    ));

    Ok(path)
}

/// Parse a string of the format "{hostname}:{displaynumber}.{screennumber}".
fn parse_display_string(s: &str) -> Option<(&str, &str)> {
    // Split the string at the first ':'
    let (hostname, display_screen) = s.split_once(':')?;
    // Split the remaining part at the first '.'
    let (display_number, _screen_number) = display_screen.split_once('.')?;
    // If everything went well, return the hostname and display number
    Some((hostname, display_number))
}

fn get_machine_id() -> io::Result<String> {
    let mut id = match fs::read_to_string("/var/lib/dbus/machine-id") {
        Ok(id) => id,
        Err(_) => fs::read_to_string("/etc/machine-id")?,
    };

    // Trim leading and trailing whitespaces in-place.
    let end = id.trim_end().len();
    id.truncate(end);
    let trimmed_len = id.trim_start().len();
    id.replace_range(..(id.len() - trimmed_len), "");
    Ok(id)
}

fn get_user_config_dir() -> PathBuf {
    match env::var("XDG_CONFIG_HOME") {
        Ok(val) => PathBuf::from(val),
        Err(_) => PathBuf::from(env::var("HOME").unwrap()).join(".config"),
    }
}
