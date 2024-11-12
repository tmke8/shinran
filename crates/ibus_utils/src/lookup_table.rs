use zbus::zvariant::{Str, Value};

use crate::text::EmptyDict;
use crate::IBusText;

#[repr(i32)]
pub enum TableOrientation {
    Horizontal = 0,
    Vertical = 1,
    System = 2,
}

#[derive(Value)]
pub struct IBusLookupTable<'a> {
    name: Str<'a>,
    attachments: EmptyDict,
    /// Number of candidates shown per page.
    page_size: u32,
    /// Position index of cursor.
    cursor_pos: u32,
    /// Whether the cursor is visible.
    cursor_visible: bool,
    /// `true` for lookup table wrap around.
    round: bool,
    /// Orientation of the table.
    orientation: i32,
    /// Candidate words/phrases.
    candidates: Vec<Value<'a>>,
    /// Candidate labels which identify individual candidates in the same page.
    /// Default is 1, 2, 3, 4 ...
    labels: Vec<Value<'a>>,
}

impl<'a> Default for IBusLookupTable<'a> {
    fn default() -> IBusLookupTable<'a> {
        IBusLookupTable::new(5, 0, true, false, TableOrientation::System, &[], &[])
    }
}

impl<'a> IBusLookupTable<'a> {
    pub fn new(
        page_size: u32,
        cursor_pos: u32,
        cursor_visible: bool,
        round: bool,
        orientation: TableOrientation,
        candidates: &[&'a str],
        labels: &[&'a str],
    ) -> IBusLookupTable<'a> {
        IBusLookupTable {
            name: "IBusLookupTable".into(),
            attachments: EmptyDict {},
            page_size,
            cursor_pos,
            cursor_visible,
            round,
            orientation: orientation as i32,
            candidates: candidates
                .iter()
                .map(|&c| IBusText::new(c, &[]).into())
                .collect(),
            labels: labels
                .iter()
                .map(|&l| IBusText::new(l, &[]).into())
                .collect(),
        }
    }

    pub fn append_candidate(&mut self, text: &'a str) {
        self.candidates.push(IBusText::new(text, &[]).into());
    }

    pub fn append_label(&mut self, text: &'a str) {
        self.labels.push(IBusText::new(text, &[]).into());
    }

    pub fn get_cursor_pos(&self) -> u32 {
        self.cursor_pos
    }

    pub fn get_cursor_pos_in_current_page(&self) -> u32 {
        self.cursor_pos % self.page_size
    }

    pub fn set_cursor_pos_in_current_page(&mut self, pos: u32) -> bool {
        let mut pos = pos;
        if pos >= self.page_size {
            return false;
        }
        pos += self.get_cursor_pos_in_current_page();
        if pos >= self.candidates.len() as u32 {
            return false;
        }
        self.cursor_pos = pos;
        return true;
    }

    pub fn cursor_up(&mut self) -> bool {
        if self.cursor_pos == 0 {
            if self.round {
                self.cursor_pos = self.candidates.len() as u32 - 1;
                return true;
            } else {
                return false;
            }
        }
        self.cursor_pos -= 1;
        return true;
    }

    pub fn cursor_down(&mut self) -> bool {
        if self.cursor_pos == self.candidates.len() as u32 {
            if self.round {
                self.cursor_pos = 0;
                return true;
            } else {
                return false;
            }
        }
        self.cursor_pos += 1;
        return true;
    }

    pub fn clear(&mut self) {
        self.candidates.clear();
        self.labels.clear();
        self.cursor_pos = 0;
    }
}
