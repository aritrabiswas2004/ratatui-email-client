use crate::models::ComposeDraft;

#[derive(Debug, Clone)]
pub enum View {
    Loading(String),
    Inbox,
    Thread,
    Compose,
}

#[derive(Debug, Clone)]
pub struct ComposeState {
    pub draft: ComposeDraft,
    pub field: ComposeField,
    pub to_cursor: usize,
    pub subject_cursor: usize,
    pub body_cursor: usize,
    pub body_preferred_col: Option<usize>,
    pub origin: ComposeOrigin,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposeField {
    To,
    Subject,
    Body,
}

impl ComposeField {
    pub fn next(self) -> Self {
        match self {
            ComposeField::To => ComposeField::Subject,
            ComposeField::Subject => ComposeField::Body,
            ComposeField::Body => ComposeField::To,
        }
    }

    pub fn previous(self) -> Self {
        match self {
            ComposeField::To => ComposeField::Body,
            ComposeField::Subject => ComposeField::To,
            ComposeField::Body => ComposeField::Subject,
        }
    }
}

impl ComposeState {
    pub fn sync_cursors_to_text(&mut self) {
        self.to_cursor = self.to_cursor.min(char_count(&self.draft.to));
        self.subject_cursor = self.subject_cursor.min(char_count(&self.draft.subject));
        self.body_cursor = self.body_cursor.min(char_count(&self.draft.body));
    }

    pub fn move_to_next_field(&mut self) {
        self.field = self.field.next();
        self.body_preferred_col = None;
    }

    pub fn move_to_previous_field(&mut self) {
        self.field = self.field.previous();
        self.body_preferred_col = None;
    }

    pub fn move_cursor_left(&mut self) {
        match self.field {
            ComposeField::To => {
                self.to_cursor = self.to_cursor.saturating_sub(1);
            }
            ComposeField::Subject => {
                self.subject_cursor = self.subject_cursor.saturating_sub(1);
            }
            ComposeField::Body => {
                self.body_cursor = self.body_cursor.saturating_sub(1);
                self.body_preferred_col = None;
            }
        }
    }

    pub fn move_cursor_right(&mut self) {
        match self.field {
            ComposeField::To => {
                self.to_cursor = (self.to_cursor + 1).min(char_count(&self.draft.to));
            }
            ComposeField::Subject => {
                self.subject_cursor = (self.subject_cursor + 1).min(char_count(&self.draft.subject));
            }
            ComposeField::Body => {
                self.body_cursor = (self.body_cursor + 1).min(char_count(&self.draft.body));
                self.body_preferred_col = None;
            }
        }
    }

    pub fn move_cursor_home(&mut self) {
        match self.field {
            ComposeField::To => self.to_cursor = 0,
            ComposeField::Subject => self.subject_cursor = 0,
            ComposeField::Body => {
                let (line, _) = line_col_for_index(&self.draft.body, self.body_cursor);
                self.body_cursor = index_for_line_col(&self.draft.body, line, 0);
                self.body_preferred_col = Some(0);
            }
        }
    }

    pub fn move_cursor_end(&mut self) {
        match self.field {
            ComposeField::To => self.to_cursor = char_count(&self.draft.to),
            ComposeField::Subject => self.subject_cursor = char_count(&self.draft.subject),
            ComposeField::Body => {
                let (line, _) = line_col_for_index(&self.draft.body, self.body_cursor);
                let col = line_lengths(&self.draft.body)
                    .get(line)
                    .copied()
                    .unwrap_or_default();
                self.body_cursor = index_for_line_col(&self.draft.body, line, col);
                self.body_preferred_col = Some(col);
            }
        }
    }

    pub fn move_cursor_up(&mut self) {
        if self.field != ComposeField::Body {
            return;
        }

        let (line, col) = line_col_for_index(&self.draft.body, self.body_cursor);
        if line == 0 {
            self.body_cursor = 0;
            self.body_preferred_col = Some(col);
            return;
        }

        let preferred_col = self.body_preferred_col.unwrap_or(col);
        self.body_cursor = index_for_line_col(&self.draft.body, line - 1, preferred_col);
        self.body_preferred_col = Some(preferred_col);
    }

    pub fn move_cursor_down(&mut self) {
        if self.field != ComposeField::Body {
            return;
        }

        let lengths = line_lengths(&self.draft.body);
        let (line, col) = line_col_for_index(&self.draft.body, self.body_cursor);
        if line + 1 >= lengths.len() {
            self.body_cursor = char_count(&self.draft.body);
            self.body_preferred_col = Some(col);
            return;
        }

        let preferred_col = self.body_preferred_col.unwrap_or(col);
        self.body_cursor = index_for_line_col(&self.draft.body, line + 1, preferred_col);
        self.body_preferred_col = Some(preferred_col);
    }

    pub fn insert_char(&mut self, ch: char) {
        match self.field {
            ComposeField::To => {
                insert_char_at(&mut self.draft.to, &mut self.to_cursor, ch);
            }
            ComposeField::Subject => {
                insert_char_at(&mut self.draft.subject, &mut self.subject_cursor, ch);
            }
            ComposeField::Body => {
                insert_char_at(&mut self.draft.body, &mut self.body_cursor, ch);
                self.body_preferred_col = None;
            }
        }
    }

    pub fn insert_newline(&mut self) {
        if self.field == ComposeField::Body {
            insert_char_at(&mut self.draft.body, &mut self.body_cursor, '\n');
            self.body_preferred_col = None;
        }
    }

    pub fn backspace(&mut self) {
        match self.field {
            ComposeField::To => {
                remove_char_before(&mut self.draft.to, &mut self.to_cursor);
            }
            ComposeField::Subject => {
                remove_char_before(&mut self.draft.subject, &mut self.subject_cursor);
            }
            ComposeField::Body => {
                remove_char_before(&mut self.draft.body, &mut self.body_cursor);
                self.body_preferred_col = None;
            }
        }
    }
}

fn char_count(value: &str) -> usize {
    value.chars().count()
}

fn char_to_byte_index(value: &str, char_index: usize) -> usize {
    value.char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(value.len())
}

fn insert_char_at(value: &mut String, cursor: &mut usize, ch: char) {
    let byte_index = char_to_byte_index(value, *cursor);
    value.insert(byte_index, ch);
    *cursor += 1;
}

fn remove_char_before(value: &mut String, cursor: &mut usize) {
    if *cursor == 0 {
        return;
    }

    let start = char_to_byte_index(value, *cursor - 1);
    let end = char_to_byte_index(value, *cursor);
    value.replace_range(start..end, "");
    *cursor -= 1;
}

fn line_lengths(value: &str) -> Vec<usize> {
    value.split('\n').map(|line| line.chars().count()).collect()
}

pub fn line_col_for_index(value: &str, cursor: usize) -> (usize, usize) {
    let mut line = 0;
    let mut col = 0;

    for ch in value.chars().take(cursor) {
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }

    (line, col)
}

pub fn index_for_line_col(value: &str, line: usize, col: usize) -> usize {
    let lengths = line_lengths(value);
    if lengths.is_empty() {
        return 0;
    }

    let target_line = line.min(lengths.len().saturating_sub(1));
    let mut index = 0;
    for length in lengths.iter().take(target_line) {
        index += *length + 1;
    }

    index + col.min(lengths[target_line])
}

#[derive(Debug, Clone)]
pub enum ComposeOrigin {
    Inbox,
    Thread(String),
}
