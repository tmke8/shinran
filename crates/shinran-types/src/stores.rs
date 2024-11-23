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
