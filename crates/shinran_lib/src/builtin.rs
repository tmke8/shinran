pub struct BuiltInMatch {
    pub id: i32,
    pub label: &'static str,
    pub triggers: Vec<String>,
    pub hotkey: Option<String>,
    // pub action: fn(context: &dyn Context) -> EventType,
}

impl Default for BuiltInMatch {
    fn default() -> Self {
        Self {
            id: 0,
            label: "",
            triggers: Vec::new(),
            hotkey: None,
            // action: |_| EventType::NOOP,
        }
    }
}
