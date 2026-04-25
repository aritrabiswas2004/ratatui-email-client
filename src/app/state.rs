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

#[derive(Debug, Clone)]
pub enum ComposeOrigin {
    Inbox,
    Thread(String),
}
