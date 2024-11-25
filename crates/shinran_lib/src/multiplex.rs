use shinran_config::matches::Match;

use crate::builtin::BuiltInMatch;

pub enum MatchResult<'a> {
    User(&'a Match),
    Builtin(&'a BuiltInMatch),
}
