use std::collections::HashMap;

use shinran_types::MatchIdx;
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

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DetectedMatch {
    pub id: MatchIdx,
    pub trigger: String,
    pub left_separator: Option<String>,
    pub right_separator: Option<String>,
    pub args: HashMap<String, String>,
}
