use crate::{TriggerMatch, Variable};

#[repr(transparent)]
pub struct VarStore {
    vars: Vec<Variable>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
#[repr(transparent)]
pub struct VarRef {
    idx: usize,
}

impl VarStore {
    #[inline]
    pub fn new() -> Self {
        Self { vars: Vec::new() }
    }

    #[inline]
    pub fn add(&mut self, var: Variable) -> VarRef {
        let idx = self.vars.len();
        self.vars.push(var);
        VarRef { idx }
    }

    #[inline]
    pub fn get(&self, ref_: VarRef) -> &Variable {
        &self.vars[ref_.idx]
    }
}

#[repr(transparent)]
pub struct TrigMatchStore {
    matches: Vec<(Vec<String>, TriggerMatch)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
#[repr(transparent)]
pub struct TrigMatchRef {
    idx: usize,
}

impl TrigMatchStore {
    #[inline]
    pub fn new() -> Self {
        Self {
            matches: Vec::new(),
        }
    }

    #[inline]
    pub fn add(&mut self, triggers: Vec<String>, m: TriggerMatch) -> TrigMatchRef {
        let idx = self.matches.len();
        self.matches.push((triggers, m));
        TrigMatchRef { idx }
    }

    #[inline]
    pub fn get(&self, ref_: TrigMatchRef) -> &(Vec<String>, TriggerMatch) {
        &self.matches[ref_.idx]
    }

    #[inline]
    pub fn enumerate(&self) -> impl Iterator<Item = (TrigMatchRef, &(Vec<String>, TriggerMatch))> {
        self.matches
            .iter()
            .enumerate()
            .map(|(idx, elem)| (TrigMatchRef { idx }, elem))
    }
}
