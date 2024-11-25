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

/// Check if the given string represents an empty YAML.
/// In other words, it checks if the document is only composed
/// of spaces and/or comments
pub fn is_yaml_empty(yaml: &str) -> bool {
    for line in yaml.lines() {
        let trimmed_line = line.trim();
        if !trimmed_line.starts_with('#') && !trimmed_line.is_empty() {
            return false;
        }
    }

    true
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn is_yaml_empty_document_empty() {
        assert!(is_yaml_empty(""));
    }

    #[test]
    fn is_yaml_empty_document_with_comments() {
        assert!(is_yaml_empty("\n#comment \n \n"));
    }

    #[test]
    fn is_yaml_empty_document_with_comments_and_content() {
        assert!(!is_yaml_empty("\n#comment \n field: true\n"));
    }

    #[test]
    fn is_yaml_empty_document_with_content() {
        assert!(!is_yaml_empty("\nfield: true\n"));
    }
}
