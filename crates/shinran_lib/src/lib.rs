use time::{
    format_description::well_known::Rfc3339,
    OffsetDateTime
};
use std::time::SystemTime;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_command() {
        assert!(check_command("hello").is_none());
    }
}
