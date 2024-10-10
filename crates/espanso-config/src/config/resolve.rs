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

use super::{
    default::{
        DEFAULT_CLIPBOARD_THRESHOLD, DEFAULT_POST_FORM_DELAY, DEFAULT_POST_SEARCH_DELAY,
        DEFAULT_PRE_PASTE_DELAY, DEFAULT_RESTORE_CLIPBOARD_DELAY, DEFAULT_SHORTCUT_EVENT_DELAY,
    },
    parse::ParsedConfig,
    path::calculate_paths,
    util::os_matches,
    AppProperties, Backend, RMLVOConfig, ToggleKey,
};
use crate::{counter::next_id, merge};
use anyhow::Result;
use indoc::formatdoc;
use log::error;
use regex::Regex;
use std::path::PathBuf;
use std::{collections::HashSet, path::Path};
use thiserror::Error;

const STANDARD_INCLUDES: &[&str] = &["../match/**/[!_]*.yml"];

#[derive(Debug, Clone, Default)]
pub struct ResolvedConfig {
    pub(crate) parsed: ParsedConfig,

    pub(crate) source_path: Option<PathBuf>,

    // Generated properties
    pub(crate) id: i32,
    pub(crate) match_paths: Vec<String>,

    pub(crate) filter_title: Option<Regex>,
    pub(crate) filter_class: Option<Regex>,
    pub(crate) filter_exec: Option<Regex>,
}

impl ResolvedConfig {
    pub fn id(&self) -> i32 {
        self.id
    }

    pub fn label(&self) -> &str {
        if let Some(label) = self.parsed.label.as_deref() {
            return label;
        }

        if let Some(source_path) = self.source_path.as_ref() {
            if let Some(source_path) = source_path.to_str() {
                return source_path;
            }
        }

        "none"
    }

    pub fn match_paths(&self) -> &[String] {
        &self.match_paths
    }

    pub fn is_match(&self, app: &AppProperties) -> bool {
        if self.parsed.filter_os.is_none()
            && self.parsed.filter_title.is_none()
            && self.parsed.filter_exec.is_none()
            && self.parsed.filter_class.is_none()
        {
            return false;
        }

        let is_os_match = if let Some(filter_os) = self.parsed.filter_os.as_deref() {
            os_matches(filter_os)
        } else {
            true
        };

        let is_title_match = if let Some(title_regex) = self.filter_title.as_ref() {
            if let Some(title) = app.title {
                title_regex.is_match(title)
            } else {
                false
            }
        } else {
            true
        };

        let is_exec_match = if let Some(exec_regex) = self.filter_exec.as_ref() {
            if let Some(exec) = app.exec {
                exec_regex.is_match(exec)
            } else {
                false
            }
        } else {
            true
        };

        let is_class_match = if let Some(class_regex) = self.filter_class.as_ref() {
            if let Some(class) = app.class {
                class_regex.is_match(class)
            } else {
                false
            }
        } else {
            true
        };

        // All the filters that have been specified must be true to define a match
        is_os_match && is_exec_match && is_title_match && is_class_match
    }

    // The mechanism used to perform the injection. Espanso can either
    // inject text by simulating keypresses (Inject backend) or
    // by using the clipboard (Clipboard backend). Both of them have pros
    // and cons, so the "Auto" backend is used by default to automatically
    // choose the most appropriate one based on the situation.
    // If for whatever reason the Auto backend is not appropriate, you
    // can change this option to override it.
    pub fn backend(&self) -> Backend {
        // TODO: test
        match self
            .parsed
            .backend
            .as_deref()
            .map(str::to_lowercase)
            .as_deref()
        {
            Some("clipboard") => Backend::Clipboard,
            Some("inject") => Backend::Inject,
            Some("auto") => Backend::Auto,
            None => Backend::Auto,
            err => {
                error!("invalid backend specified {:?}, falling back to Auto", err);
                Backend::Auto
            }
        }
    }

    // If false, espanso will be disabled for the current configuration.
    // This option can be used to selectively disable espanso when
    // using a specific application (by creating an app-specific config).
    pub fn enable(&self) -> bool {
        self.parsed.enable.unwrap_or(true)
    }

    // Number of chars after which a match is injected with the clipboard
    // backend instead of the default one. This is done for efficiency
    // reasons, as injecting a long match through separate events becomes
    // slow for long strings.
    pub fn clipboard_threshold(&self) -> usize {
        self.parsed
            .clipboard_threshold
            .unwrap_or(DEFAULT_CLIPBOARD_THRESHOLD)
    }

    // If true, instructs the daemon process to restart the worker (and refresh
    // the configuration) after a configuration file change is detected on disk.
    pub fn auto_restart(&self) -> bool {
        self.parsed.auto_restart.unwrap_or(true)
    }

    // Delay (in ms) that espanso should wait to trigger the paste shortcut
    // after copying the content in the clipboard. This is needed because
    // if we trigger a "paste" shortcut before the content is actually
    // copied in the clipboard, the operation will fail.
    pub fn pre_paste_delay(&self) -> usize {
        self.parsed
            .pre_paste_delay
            .unwrap_or(DEFAULT_PRE_PASTE_DELAY)
    }

    // Defines the key that disables/enables espanso when double pressed
    pub fn toggle_key(&self) -> Option<ToggleKey> {
        // TODO: test
        match self
            .parsed
            .toggle_key
            .as_deref()
            .map(str::to_lowercase)
            .as_deref()
        {
            Some("ctrl") => Some(ToggleKey::Ctrl),
            Some("alt") => Some(ToggleKey::Alt),
            Some("shift") => Some(ToggleKey::Shift),
            Some("meta" | "cmd") => Some(ToggleKey::Meta),
            Some("right_ctrl") => Some(ToggleKey::RightCtrl),
            Some("right_alt") => Some(ToggleKey::RightAlt),
            Some("right_shift") => Some(ToggleKey::RightShift),
            Some("right_meta" | "right_cmd") => Some(ToggleKey::RightMeta),
            Some("left_ctrl") => Some(ToggleKey::LeftCtrl),
            Some("left_alt") => Some(ToggleKey::LeftAlt),
            Some("left_shift") => Some(ToggleKey::LeftShift),
            Some("left_meta" | "left_cmd") => Some(ToggleKey::LeftMeta),
            Some("off") => None,
            None => None,
            err => {
                error!("invalid toggle_key specified {:?}", err);
                None
            }
        }
    }

    // If true, espanso will attempt to preserve the previous clipboard content
    // after an expansion has taken place (when using the Clipboard backend).
    pub fn preserve_clipboard(&self) -> bool {
        self.parsed.preserve_clipboard.unwrap_or(true)
    }

    // The number of milliseconds to wait before restoring the previous clipboard
    // content after an expansion. This is needed as without this delay, sometimes
    // the target application detects the previous clipboard content instead of
    // the expansion content.
    pub fn restore_clipboard_delay(&self) -> usize {
        self.parsed
            .restore_clipboard_delay
            .unwrap_or(DEFAULT_RESTORE_CLIPBOARD_DELAY)
    }

    // Number of milliseconds between keystrokes when simulating the Paste shortcut
    // For example: CTRL + (wait 5ms) + V + (wait 5ms) + release V + (wait 5ms) + release CTRL
    // This is needed as sometimes (for example on macOS), without a delay some keystrokes
    // were not registered correctly
    pub fn paste_shortcut_event_delay(&self) -> usize {
        self.parsed
            .paste_shortcut_event_delay
            .unwrap_or(DEFAULT_SHORTCUT_EVENT_DELAY)
    }

    // Customize the keyboard shortcut used to paste an expansion.
    // This should follow this format: CTRL+SHIFT+V
    pub fn paste_shortcut(&self) -> Option<String> {
        self.parsed.paste_shortcut.clone()
    }

    // NOTE: This is only relevant on Linux under X11 environments
    // Switch to a slower (but sometimes more supported) way of injecting
    // key events based on XTestFakeKeyEvent instead of XSendEvent.
    // From my experiements, disabling fast inject becomes particularly slow when
    // using the Gnome desktop environment.
    pub fn disable_x11_fast_inject(&self) -> bool {
        self.parsed.disable_x11_fast_inject.unwrap_or(false)
    }

    // Number of milliseconds between text injection events. Increase if the target
    // application is missing some characters.
    pub fn inject_delay(&self) -> Option<usize> {
        self.parsed.inject_delay
    }

    // Number of milliseconds between key injection events. Increase if the target
    // application is missing some key events.
    pub fn key_delay(&self) -> Option<usize> {
        self.parsed.key_delay
    }

    // Chars that when pressed mark the start and end of a word.
    // Examples of this are . or ,
    pub fn word_separators(&self) -> Vec<String> {
        self.parsed.word_separators.clone().unwrap_or_else(|| {
            vec![
                " ".to_string(),
                ",".to_string(),
                ";".to_string(),
                ":".to_string(),
                ".".to_string(),
                "?".to_string(),
                "!".to_string(),
                "(".to_string(),
                ")".to_string(),
                "{".to_string(),
                "}".to_string(),
                "[".to_string(),
                "]".to_string(),
                "<".to_string(),
                ">".to_string(),
                "\'".to_string(),
                "\"".to_string(),
                "\r".to_string(),
                "\t".to_string(),
                "\n".to_string(),
                "\x0c".to_string(), // Form Feed
            ]
        })
    }

    // Maximum number of backspace presses espanso keeps track of.
    // For example, this is needed to correctly expand even if typos
    // are typed.
    pub fn backspace_limit(&self) -> usize {
        self.parsed.backspace_limit.unwrap_or(5)
    }

    // If false, avoid applying the built-in patches to the current config.
    pub fn apply_patch(&self) -> bool {
        self.parsed.apply_patch.unwrap_or(true)
    }

    // On Wayland, overrides the auto-detected keyboard configuration (RMLVO)
    // which is used both for the detection and injection process.
    pub fn keyboard_layout(&self) -> Option<RMLVOConfig> {
        self.parsed
            .keyboard_layout
            .as_ref()
            .map(|layout| RMLVOConfig {
                rules: layout.get("rules").map(String::from),
                model: layout.get("model").map(String::from),
                layout: layout.get("layout").map(String::from),
                variant: layout.get("variant").map(String::from),
                options: layout.get("options").map(String::from),
            })
    }

    // Trigger used to show the Search UI
    pub fn search_trigger(&self) -> Option<String> {
        match self.parsed.search_trigger.as_deref() {
            Some("OFF" | "off") => None,
            Some(x) => Some(x.to_string()),
            None => None,
        }
    }

    // Hotkey used to trigger the Search UI
    pub fn search_shortcut(&self) -> Option<String> {
        match self.parsed.search_shortcut.as_deref() {
            Some("OFF" | "off") => None,
            Some(x) => Some(x.to_string()),
            None => Some("ALT+SPACE".to_string()),
        }
    }

    // When enabled, espanso automatically "reverts" an expansion if the user
    // presses the Backspace key afterwards.
    pub fn undo_backspace(&self) -> bool {
        self.parsed.undo_backspace.unwrap_or(true)
    }

    // If false, avoid showing the espanso icon on the system's tray bar
    // Note: currently not working on Linux
    pub fn show_icon(&self) -> bool {
        self.parsed.show_icon.unwrap_or(true)
    }

    // If false, disable all notifications
    pub fn show_notifications(&self) -> bool {
        self.parsed.show_notifications.unwrap_or(true)
    }

    // If false, avoid showing the `SecureInput`` notification on macOS
    pub fn secure_input_notification(&self) -> bool {
        self.parsed.secure_input_notification.unwrap_or(true)
    }

    // If enabled, Espanso emulates the Alt Code feature available on Windows
    // (keeping ALT pressed and then typing a char code with the numpad).
    // This feature is necessary on Windows because the mechanism used by Espanso
    // to intercept keystrokes disables the Windows' native Alt code functionality
    // as a side effect.
    // Because many users relied on this feature, we try to bring it back by emulating it.
    pub fn emulate_alt_codes(&self) -> bool {
        self.parsed
            .emulate_alt_codes
            .unwrap_or(cfg!(target_os = "windows"))
    }

    // The number of milliseconds to wait after a form has been closed.
    // This is useful to let the target application regain focus
    // after a form has been closed, otherwise the injection might
    // not be targeted to the right application.
    pub fn post_form_delay(&self) -> usize {
        self.parsed
            .post_form_delay
            .unwrap_or(DEFAULT_POST_FORM_DELAY)
    }

    // The maximum width that a form window can take.
    pub fn max_form_width(&self) -> usize {
        self.parsed.max_form_width.unwrap_or(700)
    }

    // The maximum height that a form window can take.
    fn max_form_height(&self) -> usize {
        self.parsed.max_form_height.unwrap_or(500)
    }

    // The number of milliseconds to wait after the search bar has been closed.
    // This is useful to let the target application regain focus
    // after the search bar has been closed, otherwise the injection might
    // not be targeted to the right application.
    pub fn post_search_delay(&self) -> usize {
        self.parsed
            .post_search_delay
            .unwrap_or(DEFAULT_POST_SEARCH_DELAY)
    }

    // If true, filter out keyboard events without an explicit HID device source on Windows.
    // This is needed to filter out the software-generated events, including
    // those from espanso, but might need to be disabled when using some software-level keyboards.
    // Disabling this option might conflict with the undo feature.
    pub fn win32_exclude_orphan_events(&self) -> bool {
        self.parsed.win32_exclude_orphan_events.unwrap_or(true)
    }

    // Extra delay to apply when injecting modifiers under the EVDEV backend.
    // This is useful on Wayland if espanso is injecting seemingly random
    // cased letters, for example "Hi theRE1" instead of "Hi there!".
    // Increase if necessary, decrease to speed up the injection.
    pub fn evdev_modifier_delay(&self) -> Option<usize> {
        self.parsed.evdev_modifier_delay
    }

    // The maximum interval (in milliseconds) for which a keyboard layout
    // can be cached. If switching often between different layouts, you
    // could lower this amount to avoid the "lost detection" effect described
    // in this issue: https://github.com/espanso/espanso/issues/745
    pub fn win32_keyboard_layout_cache_interval(&self) -> i64 {
        self.parsed
            .win32_keyboard_layout_cache_interval
            .unwrap_or(2000)
    }

    // If true, use an alternative injection backend based on the `xdotool` library.
    // This might improve the situation for certain locales/layouts on X11.
    pub fn x11_use_xclip_backend(&self) -> bool {
        self.parsed.x11_use_xclip_backend.unwrap_or(false)
    }

    // If true, use an alternative injection backend based on the `xdotool` library.
    // This might improve the situation for certain locales/layouts on X11.
    pub fn x11_use_xdotool_backend(&self) -> bool {
        self.parsed.x11_use_xdotool_backend.unwrap_or(false)
    }

    pub fn pretty_dump(&self) -> String {
        formatdoc! {"
          [espanso config: {:?}]

          backend: {:?}
          enable: {:?}
          paste_shortcut: {:?}
          inject_delay: {:?}
          key_delay: {:?}
          apply_patch: {:?}
          word_separators: {:?}

          preserve_clipboard: {:?}
          clipboard_threshold: {:?}
          disable_x11_fast_inject: {}
          pre_paste_delay: {}
          paste_shortcut_event_delay: {}
          toggle_key: {:?}
          auto_restart: {:?}
          restore_clipboard_delay: {:?}
          post_form_delay: {:?}
          max_form_width: {:?}
          max_form_height: {:?}
          post_search_delay: {:?}
          backspace_limit: {}
          search_trigger: {:?}
          search_shortcut: {:?}
          keyboard_layout: {:?}

          show_icon: {:?}
          show_notifications: {:?}
          secure_input_notification: {:?}

          x11_use_xclip_backend: {:?}
          x11_use_xdotool_backend: {:?}
          win32_exclude_orphan_events: {:?}
          win32_keyboard_layout_cache_interval: {:?}

          match_paths: {:#?}
        ",
          self.label(),
          self.backend(),
          self.enable(),
          self.paste_shortcut(),
          self.inject_delay(),
          self.key_delay(),
          self.apply_patch(),
          self.word_separators(),

          self.preserve_clipboard(),
          self.clipboard_threshold(),
          self.disable_x11_fast_inject(),
          self.pre_paste_delay(),
          self.paste_shortcut_event_delay(),
          self.toggle_key(),
          self.auto_restart(),
          self.restore_clipboard_delay(),
          self.post_form_delay(),
          self.max_form_width(),
          self.max_form_height(),
          self.post_search_delay(),
          self.backspace_limit(),
          self.search_trigger(),
          self.search_shortcut(),
          self.keyboard_layout(),

          self.show_icon(),
          self.show_notifications(),
          self.secure_input_notification(),

          self.x11_use_xclip_backend(),
          self.x11_use_xdotool_backend(),
          self.win32_exclude_orphan_events(),
          self.win32_keyboard_layout_cache_interval(),

          self.match_paths(),
        }
    }

    pub fn load(path: &Path, parent: Option<&Self>) -> Result<Self> {
        let mut config = ParsedConfig::load(path)?;

        // Merge with parent config if present
        if let Some(parent) = parent {
            Self::merge_parsed(&mut config, &parent.parsed);
        }

        // Extract the base directory
        let base_dir = path
            .parent()
            .ok_or_else(ResolveError::ParentResolveFailed)?;

        let match_paths = Self::generate_match_paths(&config, base_dir)
            .into_iter()
            .collect();

        let filter_title = if let Some(filter_title) = config.filter_title.as_deref() {
            Some(Regex::new(filter_title)?)
        } else {
            None
        };

        let filter_class = if let Some(filter_class) = config.filter_class.as_deref() {
            Some(Regex::new(filter_class)?)
        } else {
            None
        };

        let filter_exec = if let Some(filter_exec) = config.filter_exec.as_deref() {
            Some(Regex::new(filter_exec)?)
        } else {
            None
        };

        Ok(Self {
            parsed: config,
            source_path: Some(path.to_owned()),
            id: next_id(),
            match_paths,
            filter_title,
            filter_class,
            filter_exec,
        })
    }

    fn merge_parsed(child: &mut ParsedConfig, parent: &ParsedConfig) {
        // Override the None fields with the parent's value
        merge!(
            ParsedConfig,
            child,
            parent,
            // Fields
            label,
            backend,
            enable,
            clipboard_threshold,
            auto_restart,
            pre_paste_delay,
            preserve_clipboard,
            restore_clipboard_delay,
            paste_shortcut,
            apply_patch,
            paste_shortcut_event_delay,
            disable_x11_fast_inject,
            toggle_key,
            inject_delay,
            key_delay,
            evdev_modifier_delay,
            word_separators,
            backspace_limit,
            keyboard_layout,
            search_trigger,
            search_shortcut,
            undo_backspace,
            show_icon,
            show_notifications,
            secure_input_notification,
            emulate_alt_codes,
            post_form_delay,
            max_form_width,
            max_form_height,
            post_search_delay,
            win32_exclude_orphan_events,
            win32_keyboard_layout_cache_interval,
            x11_use_xclip_backend,
            x11_use_xdotool_backend,
            includes,
            excludes,
            extra_includes,
            extra_excludes,
            use_standard_includes,
            filter_title,
            filter_class,
            filter_exec,
            filter_os
        );
    }

    fn aggregate_includes(config: &ParsedConfig) -> HashSet<String> {
        let mut includes = HashSet::new();

        if config.use_standard_includes.is_none() || config.use_standard_includes.unwrap() {
            for include in STANDARD_INCLUDES {
                includes.insert((*include).to_string());
            }
        }

        if let Some(yaml_includes) = config.includes.as_ref() {
            for include in yaml_includes {
                includes.insert(include.to_string());
            }
        };

        if let Some(extra_includes) = config.extra_includes.as_ref() {
            for include in extra_includes {
                includes.insert(include.to_string());
            }
        };

        includes
    }

    fn aggregate_excludes(config: &ParsedConfig) -> HashSet<String> {
        let mut excludes = HashSet::new();

        if let Some(yaml_excludes) = config.excludes.as_ref() {
            for exclude in yaml_excludes {
                excludes.insert(exclude.to_string());
            }
        }

        if let Some(extra_excludes) = config.extra_excludes.as_ref() {
            for exclude in extra_excludes {
                excludes.insert(exclude.to_string());
            }
        }

        excludes
    }

    fn generate_match_paths(config: &ParsedConfig, base_dir: &Path) -> HashSet<String> {
        let includes = Self::aggregate_includes(config);
        let excludes = Self::aggregate_excludes(config);

        // Extract the paths
        let exclude_paths = calculate_paths(base_dir, excludes.iter());
        let include_paths = calculate_paths(base_dir, includes.iter());

        include_paths
            .difference(&exclude_paths)
            .cloned()
            .collect::<HashSet<_>>()
    }
}

#[derive(Error, Debug)]
pub enum ResolveError {
    #[error("unable to resolve parent path")]
    ParentResolveFailed(),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::tests::use_test_directory;
    use std::fs::create_dir_all;

    #[test]
    fn aggregate_includes_empty_config() {
        assert_eq!(
            ResolvedConfig::aggregate_includes(&ParsedConfig {
                ..Default::default()
            }),
            ["../match/**/[!_]*.yml".to_string()]
                .iter()
                .cloned()
                .collect::<HashSet<_>>()
        );
    }

    #[test]
    fn aggregate_includes_no_standard() {
        assert_eq!(
            ResolvedConfig::aggregate_includes(&ParsedConfig {
                use_standard_includes: Some(false),
                ..Default::default()
            }),
            HashSet::new()
        );
    }

    #[test]
    fn aggregate_includes_custom_includes() {
        assert_eq!(
            ResolvedConfig::aggregate_includes(&ParsedConfig {
                includes: Some(vec!["custom/*.yml".to_string()]),
                ..Default::default()
            }),
            [
                "../match/**/[!_]*.yml".to_string(),
                "custom/*.yml".to_string()
            ]
            .iter()
            .cloned()
            .collect::<HashSet<_>>()
        );
    }

    #[test]
    fn aggregate_includes_extra_includes() {
        assert_eq!(
            ResolvedConfig::aggregate_includes(&ParsedConfig {
                extra_includes: Some(vec!["custom/*.yml".to_string()]),
                ..Default::default()
            }),
            [
                "../match/**/[!_]*.yml".to_string(),
                "custom/*.yml".to_string()
            ]
            .iter()
            .cloned()
            .collect::<HashSet<_>>()
        );
    }

    #[test]
    fn aggregate_includes_includes_and_extra_includes() {
        assert_eq!(
            ResolvedConfig::aggregate_includes(&ParsedConfig {
                includes: Some(vec!["sub/*.yml".to_string()]),
                extra_includes: Some(vec!["custom/*.yml".to_string()]),
                ..Default::default()
            }),
            [
                "../match/**/[!_]*.yml".to_string(),
                "custom/*.yml".to_string(),
                "sub/*.yml".to_string()
            ]
            .iter()
            .cloned()
            .collect::<HashSet<_>>()
        );
    }

    #[test]
    fn aggregate_excludes_empty_config() {
        assert_eq!(
            ResolvedConfig::aggregate_excludes(&ParsedConfig {
                ..Default::default()
            })
            .len(),
            0
        );
    }

    #[test]
    fn aggregate_excludes_no_standard() {
        assert_eq!(
            ResolvedConfig::aggregate_excludes(&ParsedConfig {
                use_standard_includes: Some(false),
                ..Default::default()
            }),
            HashSet::new()
        );
    }

    #[test]
    fn aggregate_excludes_custom_excludes() {
        assert_eq!(
            ResolvedConfig::aggregate_excludes(&ParsedConfig {
                excludes: Some(vec!["custom/*.yml".to_string()]),
                ..Default::default()
            }),
            ["custom/*.yml".to_string()]
                .iter()
                .cloned()
                .collect::<HashSet<_>>()
        );
    }

    #[test]
    fn aggregate_excludes_extra_excludes() {
        assert_eq!(
            ResolvedConfig::aggregate_excludes(&ParsedConfig {
                extra_excludes: Some(vec!["custom/*.yml".to_string()]),
                ..Default::default()
            }),
            ["custom/*.yml".to_string()]
                .iter()
                .cloned()
                .collect::<HashSet<_>>()
        );
    }

    #[test]
    fn aggregate_excludes_excludes_and_extra_excludes() {
        assert_eq!(
            ResolvedConfig::aggregate_excludes(&ParsedConfig {
                excludes: Some(vec!["sub/*.yml".to_string()]),
                extra_excludes: Some(vec!["custom/*.yml".to_string()]),
                ..Default::default()
            }),
            ["custom/*.yml".to_string(), "sub/*.yml".to_string()]
                .iter()
                .cloned()
                .collect::<HashSet<_>>()
        );
    }

    #[test]
    fn merge_parent_field_parent_fallback() {
        let parent = ParsedConfig {
            use_standard_includes: Some(false),
            ..Default::default()
        };
        let mut child = ParsedConfig {
            ..Default::default()
        };
        assert_eq!(child.use_standard_includes, None);

        ResolvedConfig::merge_parsed(&mut child, &parent);
        assert_eq!(child.use_standard_includes, Some(false));
    }

    #[test]
    fn merge_parent_field_child_overwrite_parent() {
        let parent = ParsedConfig {
            use_standard_includes: Some(true),
            ..Default::default()
        };
        let mut child = ParsedConfig {
            use_standard_includes: Some(false),
            ..Default::default()
        };
        assert_eq!(child.use_standard_includes, Some(false));

        ResolvedConfig::merge_parsed(&mut child, &parent);
        assert_eq!(child.use_standard_includes, Some(false));
    }

    #[test]
    fn match_paths_generated_correctly() {
        use_test_directory(|_, match_dir, config_dir| {
            let sub_dir = match_dir.join("sub");
            create_dir_all(&sub_dir).unwrap();

            let base_file = match_dir.join("base.yml");
            std::fs::write(&base_file, "test").unwrap();
            let another_file = match_dir.join("another.yml");
            std::fs::write(&another_file, "test").unwrap();
            let under_file = match_dir.join("_sub.yml");
            std::fs::write(under_file, "test").unwrap();
            let sub_file = sub_dir.join("sub.yml");
            std::fs::write(&sub_file, "test").unwrap();

            let config_file = config_dir.join("default.yml");
            std::fs::write(&config_file, "").unwrap();

            let config = ResolvedConfig::load(&config_file, None).unwrap();

            let mut expected = vec![
                base_file.to_string_lossy().to_string(),
                another_file.to_string_lossy().to_string(),
                sub_file.to_string_lossy().to_string(),
            ];
            expected.sort();

            let mut result = config.match_paths().to_vec();
            result.sort();

            assert_eq!(result, expected.as_slice());
        });
    }

    #[test]
    fn match_paths_generated_correctly_with_child_config() {
        use_test_directory(|_, match_dir, config_dir| {
            let sub_dir = match_dir.join("sub");
            create_dir_all(&sub_dir).unwrap();

            let base_file = match_dir.join("base.yml");
            std::fs::write(&base_file, "test").unwrap();
            let another_file = match_dir.join("another.yml");
            std::fs::write(another_file, "test").unwrap();
            let under_file = match_dir.join("_sub.yml");
            std::fs::write(under_file, "test").unwrap();
            let sub_file = sub_dir.join("another.yml");
            std::fs::write(&sub_file, "test").unwrap();
            let sub_under_file = sub_dir.join("_sub.yml");
            std::fs::write(&sub_under_file, "test").unwrap();

            // Configs

            let parent_file = config_dir.join("parent.yml");
            std::fs::write(
                &parent_file,
                r"
      excludes: ['../**/another.yml']
      ",
            )
            .unwrap();

            let config_file = config_dir.join("default.yml");
            std::fs::write(
                &config_file,
                r#"
      use_standard_includes: false
      excludes: []
      includes: ["../match/sub/*.yml"]
      "#,
            )
            .unwrap();

            let parent = ResolvedConfig::load(&parent_file, None).unwrap();
            let child = ResolvedConfig::load(&config_file, Some(&parent)).unwrap();

            let mut expected = vec![
                sub_file.to_string_lossy().to_string(),
                sub_under_file.to_string_lossy().to_string(),
            ];
            expected.sort();

            let mut result = child.match_paths().to_vec();
            result.sort();
            assert_eq!(result, expected.as_slice());

            let expected = vec![base_file.to_string_lossy().to_string()];

            assert_eq!(parent.match_paths(), expected.as_slice());
        });
    }

    #[test]
    fn match_paths_generated_correctly_with_underscore_files() {
        use_test_directory(|_, match_dir, config_dir| {
            let sub_dir = match_dir.join("sub");
            create_dir_all(&sub_dir).unwrap();

            let base_file = match_dir.join("base.yml");
            std::fs::write(&base_file, "test").unwrap();
            let another_file = match_dir.join("another.yml");
            std::fs::write(&another_file, "test").unwrap();
            let under_file = match_dir.join("_sub.yml");
            std::fs::write(&under_file, "test").unwrap();
            let sub_file = sub_dir.join("sub.yml");
            std::fs::write(&sub_file, "test").unwrap();

            let config_file = config_dir.join("default.yml");
            std::fs::write(&config_file, "extra_includes: ['../match/_sub.yml']").unwrap();

            let config = ResolvedConfig::load(&config_file, None).unwrap();

            let mut expected = vec![
                base_file.to_string_lossy().to_string(),
                another_file.to_string_lossy().to_string(),
                sub_file.to_string_lossy().to_string(),
                under_file.to_string_lossy().to_string(),
            ];
            expected.sort();

            let mut result = config.match_paths().to_vec();
            result.sort();

            assert_eq!(result, expected.as_slice());
        });
    }

    fn test_filter_is_match(config: &str, app: &AppProperties) -> bool {
        let mut result = false;
        let result_ref = &mut result;
        use_test_directory(move |_, _, config_dir| {
            let config_file = config_dir.join("default.yml");
            std::fs::write(&config_file, config).unwrap();

            let config = ResolvedConfig::load(&config_file, None).unwrap();

            *result_ref = config.is_match(app);
        });
        result
    }

    #[test]
    fn is_match_no_filters() {
        assert!(!test_filter_is_match(
            "",
            &AppProperties {
                title: Some("Google"),
                class: Some("Chrome"),
                exec: Some("chrome.exe"),
            },
        ));
    }

    #[test]
    fn is_match_filter_title() {
        assert!(test_filter_is_match(
            "filter_title: Google",
            &AppProperties {
                title: Some("Google Mail"),
                class: Some("Chrome"),
                exec: Some("chrome.exe"),
            },
        ));

        assert!(!test_filter_is_match(
            "filter_title: Google",
            &AppProperties {
                title: Some("Yahoo"),
                class: Some("Chrome"),
                exec: Some("chrome.exe"),
            },
        ));

        assert!(!test_filter_is_match(
            "filter_title: Google",
            &AppProperties {
                title: None,
                class: Some("Chrome"),
                exec: Some("chrome.exe"),
            },
        ));
    }

    #[test]
    fn is_match_filter_class() {
        assert!(test_filter_is_match(
            "filter_class: Chrome",
            &AppProperties {
                title: Some("Google Mail"),
                class: Some("Chrome"),
                exec: Some("chrome.exe"),
            },
        ));

        assert!(!test_filter_is_match(
            "filter_class: Chrome",
            &AppProperties {
                title: Some("Yahoo"),
                class: Some("Another"),
                exec: Some("chrome.exe"),
            },
        ));

        assert!(!test_filter_is_match(
            "filter_class: Chrome",
            &AppProperties {
                title: Some("google"),
                class: None,
                exec: Some("chrome.exe"),
            },
        ));
    }

    #[test]
    fn is_match_filter_exec() {
        assert!(test_filter_is_match(
            "filter_exec: chrome.exe",
            &AppProperties {
                title: Some("Google Mail"),
                class: Some("Chrome"),
                exec: Some("chrome.exe"),
            },
        ));

        assert!(!test_filter_is_match(
            "filter_exec: chrome.exe",
            &AppProperties {
                title: Some("Yahoo"),
                class: Some("Another"),
                exec: Some("zoom.exe"),
            },
        ));

        assert!(!test_filter_is_match(
            "filter_exec: chrome.exe",
            &AppProperties {
                title: Some("google"),
                class: Some("Chrome"),
                exec: None,
            },
        ));
    }

    #[test]
    fn is_match_filter_os() {
        let (current, another) = if cfg!(target_os = "windows") {
            ("windows", "macos")
        } else if cfg!(target_os = "macos") {
            ("macos", "windows")
        } else if cfg!(target_os = "linux") {
            ("linux", "macos")
        } else {
            ("invalid", "invalid")
        };

        assert!(test_filter_is_match(
            &format!("filter_os: {current}"),
            &AppProperties {
                title: Some("Google Mail"),
                class: Some("Chrome"),
                exec: Some("chrome.exe"),
            },
        ));

        assert!(!test_filter_is_match(
            &format!("filter_os: {another}"),
            &AppProperties {
                title: Some("Google Mail"),
                class: Some("Chrome"),
                exec: Some("chrome.exe"),
            },
        ));
    }

    #[test]
    fn is_match_multiple_filters() {
        assert!(test_filter_is_match(
            r#"
      filter_exec: chrome.exe
      filter_title: "Youtube"
      "#,
            &AppProperties {
                title: Some("Youtube - Broadcast Yourself"),
                class: Some("Chrome"),
                exec: Some("chrome.exe"),
            },
        ));

        assert!(!test_filter_is_match(
            r#"
      filter_exec: chrome.exe
      filter_title: "Youtube"
      "#,
            &AppProperties {
                title: Some("Gmail"),
                class: Some("Chrome"),
                exec: Some("chrome.exe"),
            },
        ));
    }
}
