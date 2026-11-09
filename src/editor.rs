use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[derive(Default, Clone)]
pub struct Editor {
    pub text: String,
    pub cursor: usize,
}
impl Editor {
    pub fn new(text: String) -> Self {
        let cursor = text.len();
        Self { text, cursor }
    }
    pub fn insert(&mut self, text: &str) {
        let text: String = text
            .chars()
            .filter(|c| !c.is_control() || *c == '\n')
            .collect();
        self.text.insert_str(self.cursor, &text);
        self.cursor += text.len();
    }
    pub fn left(&mut self) {
        self.cursor = self.text[..self.cursor]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i);
    }
    pub fn right(&mut self) {
        if let Some(c) = self.text[self.cursor..].chars().next() {
            self.cursor += c.len_utf8();
        }
    }
    pub fn backspace(&mut self) {
        let end = self.cursor;
        self.left();
        self.text.drain(self.cursor..end);
    }
    pub fn delete(&mut self) {
        let start = self.cursor;
        self.right();
        self.text.drain(start..self.cursor);
        self.cursor = start;
    }
    pub fn take(&mut self) -> String {
        self.cursor = 0;
        std::mem::take(&mut self.text)
    }
    pub fn visible(&self, width: usize) -> (String, u16) {
        // Keep a single-line viewport around the cursor; newlines are visible separators.
        let flattened = self.text.replace('\n', "↵");
        let before = self.text[..self.cursor].replace('\n', "↵");
        let mut start = 0;
        let mut columns = before.width();
        for (i, c) in before.char_indices() {
            if columns < width.max(1) {
                break;
            }
            columns = columns.saturating_sub(c.width().unwrap_or(0));
            start = i + c.len_utf8();
        }
        let visible = clip(&flattened[start..], width);
        (visible, columns.min(width.saturating_sub(1)) as u16)
    }
}
pub fn clip(text: &str, width: usize) -> String {
    let mut columns = 0;
    text.chars()
        .take_while(|c| {
            columns += c.width().unwrap_or(0);
            columns <= width
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edit_unicode_and_multiline_paste() {
        let mut e = Editor::new("a界b".into());
        e.left();
        e.backspace();
        e.insert("è\nhello\x1b");
        assert_eq!(e.text, "aè\nhellob");
        e.delete();
        assert_eq!(e.text, "aè\nhello");
        e.cursor = 0;
        e.delete();
        assert_eq!(e.text, "è\nhello");
        e.cursor = e.text.len();
        let (v, c) = e.visible(4);
        assert!(v.width() <= 4);
        assert!(c < 4);
        assert_eq!(e.take(), "è\nhello");
        assert_eq!(e.cursor, 0);
    }
}
