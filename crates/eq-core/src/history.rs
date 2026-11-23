//! Undo and redo with merging of continuous edits such as drags.

use crate::profile::Profile;

const DEPTH: usize = 200;

#[derive(Debug, Default)]
pub struct History {
    undo: Vec<Profile>,
    redo: Vec<Profile>,
    merge_key: Option<String>,
}

impl History {
    /// Call before changing `current`. Edits sharing a `key` become one step
    /// until [`History::seal`] is called.
    pub fn record(&mut self, current: &Profile, key: Option<&str>) {
        if key.is_some() && self.merge_key.as_deref() == key {
            return;
        }
        self.merge_key = key.map(str::to_owned);
        self.redo.clear();
        self.undo.push(current.clone());
        if self.undo.len() > DEPTH {
            self.undo.remove(0);
        }
    }

    pub fn seal(&mut self) {
        self.merge_key = None;
    }

    pub fn undo(&mut self, current: &mut Profile) -> bool {
        self.seal();
        let Some(prev) = self.undo.pop() else { return false };
        self.redo.push(std::mem::replace(current, prev));
        true
    }

    pub fn redo(&mut self, current: &mut Profile) -> bool {
        self.seal();
        let Some(next) = self.redo.pop() else { return false };
        self.undo.push(std::mem::replace(current, next));
        true
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}
