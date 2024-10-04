use std::collections::HashMap;

use serde::Deserialize;
use serde_yaml_ng::Value;

#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
struct Match {
    trigger: Value,
    replace: String,
    #[serde(default)]
    word: bool,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    vars: Vec<Var>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
struct Var {
    name: String,
    r#type: String,
    #[serde(default)]
    params: HashMap<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple() {
        let config = r#"
  - trigger: hello
    replace: world"#;
        assert_eq!(
            serde_yaml_ng::from_str::<Vec<Match>>(config).unwrap(),
            vec![Match {
                trigger: "hello".into(),
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
                    trigger: "include newlines".into(),
                    replace: "exactly as you see\nwill appear these three\nlines of poetry\n"
                        .to_string(),
                    ..Default::default()
                },
                Match {
                    trigger: "fold newlines".into(),
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
                    trigger: ":tomorrow".into(),
                    replace: "{{mytime}}".to_string(),
                    label: Some("Insert tomorrow's date, such as 5-Jan-2022".to_string()),
                    vars: vec![Var {
                        name: "mytime".to_string(),
                        r#type: "date".to_string(),
                        params: [
                            ("format".to_string(), "%v".into()),
                            ("offset".to_string(), 86400.into())
                        ]
                        .iter()
                        .cloned()
                        .collect::<HashMap<String, Value>>()
                        .into()
                    }],
                    ..Default::default()
                },
                Match {
                    trigger: ":yesterday".into(),
                    replace: "{{mytime}}".to_string(),
                    label: Some("Insert yesterday's date, such as 5-Jan-2022".to_string()),
                    vars: vec![Var {
                        name: "mytime".to_string(),
                        r#type: "date".to_string(),
                        params: [
                            ("format".to_string(), "%v".into()),
                            ("offset".to_string(), (-86400).into())
                        ]
                        .iter()
                        .cloned()
                        .collect::<HashMap<String, Value>>()
                        .into()
                    }],
                    ..Default::default()
                },
            ]
        );
    }

    #[test]
    fn test_missing_params() {
        let config = r#"
  - trigger: ":a"
    replace: "<a href='{{clipb}}' />$|$</a>"
    vars:
      - name: "clipb"
        type: "clipboard""#;
        assert_eq!(
            serde_yaml_ng::from_str::<Vec<Match>>(config).unwrap(),
            vec![
                Match {
                    trigger: ":a".into(),
                    replace: "<a href='{{clipb}}' />$|$</a>".to_string(),
                    vars: vec![Var {
                        name: "clipb".to_string(),
                        r#type: "clipboard".to_string(),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            ]
        );
    }

    #[test]
    fn test_multi_trigger() {
        let config = r#"
  - trigger: [hello, hi]
    replace: world"#;
        assert_eq!(
            serde_yaml_ng::from_str::<Vec<Match>>(config).unwrap(),
            vec![Match {
                trigger: Value::Sequence(vec!["hello".into(), "hi".into()]),
                replace: "world".to_string(),
                ..Default::default()
            }]
        );
    }
}
