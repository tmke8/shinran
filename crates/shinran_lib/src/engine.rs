use std::collections::HashMap;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum RendererError {
    #[error("rendering error")]
    RenderingError(#[from] anyhow::Error),

    #[error("match not found")]
    NotFound,

    #[error("aborted")]
    Aborted,
}

#[derive(Debug, Clone, PartialEq, Default, Eq)]
pub struct DetectedMatch {
    pub id: i32,
    pub trigger: Option<String>,
    pub left_separator: Option<String>,
    pub right_separator: Option<String>,
    pub args: HashMap<String, String>,
}
