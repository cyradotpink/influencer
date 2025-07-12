use std::collections::{HashMap, VecDeque};

#[derive(Debug)]
pub struct SubscriberQueue<T> {
    held: VecDeque<T>,
    cursors: HashMap<usize, usize>,
    next_cursor_id: usize,
}
impl<T> SubscriberQueue<T> {
    pub fn new() -> Self {
        Self {
            held: VecDeque::new(),
            cursors: HashMap::new(),
            next_cursor_id: 0,
        }
    }
    pub fn write(&mut self, value: T) {
        self.held.push_back(value);
    }
    pub fn subscribe(&mut self) -> usize {
        let cursor_id = self.next_cursor_id;
        self.cursors.insert(cursor_id, self.held.len());
        self.next_cursor_id += 1;
        cursor_id
    }
    pub fn unsubscribe(&mut self, cursor_id: usize) {
        // Likely not maximally efficient if this cursor is significantly behind all others
        while self.ack(cursor_id) {}
        self.cursors.remove(&cursor_id);
    }
    pub fn peek(&self, cursor_id: usize) -> Option<&T> {
        let cursor_pos = *self.cursors.get(&cursor_id).expect("Invalid cursor ID");
        self.held.get(cursor_pos)
    }
    pub fn ack(&mut self, cursor_id: usize) -> bool {
        let cursor_pos = self.cursors.get_mut(&cursor_id).expect("Invalid cursor ID");
        if *cursor_pos >= self.held.len() {
            // No items to acknowledge
            return false;
        }
        *cursor_pos += 1;
        if *cursor_pos > 1 {
            // Shortcut: This cursor was not the last cursor at the oldest item, because it was not at the oldest item at all.
            return true;
        }
        if self.cursors.iter().all(|(_, pos)| *pos > 0) {
            // This cursor was the last cursor at the oldest item
            for (_, pos) in self.cursors.iter_mut() {
                *pos -= 1
            }
            self.held.pop_front();
        } // Else, this cursor just left the oldest item, but other cursors are still there
        true
    }
}
