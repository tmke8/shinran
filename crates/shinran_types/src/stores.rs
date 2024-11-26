use crate::{BaseMatch, RegexMatch, TriggerMatch, Variable};

#[derive(Debug)]
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

    #[inline]
    pub fn as_slice(&self) -> &[Variable] {
        &self.vars
    }
}

#[derive(Debug)]
#[repr(transparent)]
pub struct TrigMatchStore {
    matches: Vec<TriggerMatch>,
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
    pub fn add(&mut self, m: TriggerMatch) -> TrigMatchRef {
        let idx = self.matches.len();
        self.matches.push(m);
        TrigMatchRef { idx }
    }

    #[inline]
    pub fn get(&self, ref_: TrigMatchRef) -> &TriggerMatch {
        &self.matches[ref_.idx]
    }

    #[inline]
    pub fn enumerate(&self) -> impl Iterator<Item = (TrigMatchRef, &TriggerMatch)> {
        self.matches
            .iter()
            .enumerate()
            .map(|(idx, elem)| (TrigMatchRef { idx }, elem))
    }
}

#[repr(transparent)]
pub struct RegexMatchStore {
    matches: Vec<RegexMatch>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
#[repr(transparent)]
pub struct RegexMatchRef {
    idx: usize,
}

impl RegexMatchRef {
    /// Construct a new regex match reference.
    ///
    /// This function is unsafe, because there is no guarantee that the index actually
    /// points to an existing regex match.
    pub unsafe fn new(idx: usize) -> Self {
        RegexMatchRef { idx }
    }
}

impl RegexMatchStore {
    #[inline]
    pub fn new() -> Self {
        Self {
            matches: Vec::new(),
        }
    }

    #[inline]
    pub fn add(&mut self, m: RegexMatch) -> RegexMatchRef {
        let idx = self.matches.len();
        self.matches.push(m);
        RegexMatchRef { idx }
    }

    #[inline]
    pub fn get(&self, ref_: RegexMatchRef) -> &RegexMatch {
        &self.matches[ref_.idx]
    }

    #[inline]
    pub fn enumerate(&self) -> impl Iterator<Item = (RegexMatchRef, &RegexMatch)> {
        self.matches
            .iter()
            .enumerate()
            .map(|(idx, elem)| (RegexMatchRef { idx }, elem))
    }
}
