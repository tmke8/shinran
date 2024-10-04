use std::collections::HashMap;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
struct Match {
    trigger: Trigger,
    replace: String,
    #[serde(default)]
    word: bool,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    vars: Vec<Var>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
enum Trigger {
    Single(String),
    Multi(Vec<String>),
}

impl Default for Trigger {
    fn default() -> Self {
        Trigger::Single(String::default())
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
struct Var {
    name: String,
    r#type: String,
    #[serde(default)]
    params: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simpl() {
        let config = r#"
  - trigger: hello
    replace: world"#;
        assert_eq!(
            serde_yaml_ng::from_str::<Vec<Match>>(config).unwrap(),
            vec![Match {
                trigger: Trigger::Single("hello".to_string()),
                replace: "world".to_string(),
                ..Default::default()
            }]
        );
    }

    #[test]
    fn test_multiline() {
        let config = r#"
  - trigger: include newlines
    replace: |
              exactly as you see
              will appear these three
              lines of poetry
  - trigger: fold newlines
    replace: >
              this is really a
              single line of text
              despite appearances"#;
        assert_eq!(
            serde_yaml_ng::from_str::<Vec<Match>>(config).unwrap(),
            vec![
                Match {
                    trigger: Trigger::Single("include newlines".to_string()),
                    replace: "exactly as you see\nwill appear these three\nlines of poetry\n"
                        .to_string(),
                    ..Default::default()
                },
                Match {
                    trigger: Trigger::Single("fold newlines".to_string()),
                    replace: "this is really a single line of text despite appearances".to_string(),
                    ..Default::default()
                }
            ]
        );
    }

    #[test]
    fn test_complex_example() {
        let config = r#"
  - trigger: :tomorrow
    replace: "{{mytime}}"
    label: Insert tomorrow's date, such as 5-Jan-2022
    vars:
      - name: mytime
        type: date
        params:
          format: "%v"
          offset: 86400

  - trigger: :yesterday
    replace: "{{mytime}}"
    label: Insert yesterday's date, such as 5-Jan-2022
    vars:
      - name: mytime
        type: date
        params:
          format: "%v"
          offset: -86400"#;
        assert_eq!(
            serde_yaml_ng::from_str::<Vec<Match>>(config).unwrap(),
            vec![
                Match {
                    trigger: Trigger::Single(":tomorrow".to_string()),
                    replace: "{{mytime}}".to_string(),
                    label: Some("Insert tomorrow's date, such as 5-Jan-2022".to_string()),
                    vars: vec![Var {
                        name: "mytime".to_string(),
                        r#type: "date".to_string(),
                        params: [
                            ("format".to_string(), "%v".to_string()),
                            ("offset".to_string(), "86400".to_string())
                        ]
                        .iter()
                        .cloned()
                        .collect()
                    }],
                    ..Default::default()
                },
                Match {
                    trigger: Trigger::Single(":yesterday".to_string()),
                    replace: "{{mytime}}".to_string(),
                    label: Some("Insert yesterday's date, such as 5-Jan-2022".to_string()),
                    vars: vec![Var {
                        name: "mytime".to_string(),
                        r#type: "date".to_string(),
                        params: [
                            ("format".to_string(), "%v".to_string()),
                            ("offset".to_string(), "-86400".to_string())
                        ]
                        .iter()
                        .cloned()
                        .collect()
                    }],
                    ..Default::default()
                },
            ]
        );
    }
}
