pub fn check_command(command: &str) -> Option<String> {
    match command {
        "times" => Some("×".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_command() {
        assert!(check_command("hello").is_none());
    }
}
