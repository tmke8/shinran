/// Reference to a string in the arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StrRef {
    start: usize,
    end: usize,
}

impl StrRef {
    /// Create a new `StrRef` from a start and end index.
    ///
    /// # Safety
    /// The caller must ensure that `end` is greater than or equal to `start`,
    /// and that the range `[start, end)` is a valid range in the arena.
    pub unsafe fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// Reference to a vector of strings in the arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StrVecRef {
    start: usize,
    end: usize,
}

#[derive(Debug)]
#[repr(transparent)]
pub struct StrArena {
    buf: String,
}

impl StrArena {
    pub fn new() -> Self {
        Self { buf: String::new() }
    }

    /// Allocate a string in the arena.
    pub fn alloc(&mut self, s: &str) -> StrRef {
        let start = self.buf.len();
        self.buf.push_str(s);
        let end = self.buf.len();
        StrRef { start, end }
    }

    /// Allocate a vector of strings in the arena.
    ///
    /// Returns `None` if any of the strings contain a newline character.
    pub fn alloc_all(&mut self, strings: &[&str]) -> Option<StrVecRef> {
        for s in strings {
            if s.contains('\n') {
                return None;
            }
        }
        let start = self.buf.len();
        for s in strings {
            self.buf.push_str(s);
            self.buf.push('\n');
        }
        // Remove the last newline character.
        self.buf.pop();
        let end = self.buf.len();
        Some(StrVecRef { start, end })
    }

    pub fn get(&self, r: StrRef) -> &str {
        &self.buf[r.start..r.end]
    }

    pub fn get_all(&self, r: StrVecRef) -> std::str::Split<'_, char> {
        self.buf[r.start..r.end].split('\n')
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_alloc() {
        let mut arena = StrArena::new();
        let r1 = arena.alloc("hello");
        let r2 = arena.alloc("world");
        assert_eq!(arena.get(r1), "hello");
        assert_eq!(arena.get(r2), "world");
    }

    #[test]
    fn test_alloc_all() {
        let mut arena = StrArena::new();
        let r = arena.alloc_all(&["hello", "world"]).unwrap();
        let mut iter = arena.get_all(r);
        assert_eq!(iter.next(), Some("hello"));
        assert_eq!(iter.next(), Some("world"));
        assert_eq!(iter.next(), None);
    }
}
