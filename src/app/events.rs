use crate::models::{ThreadDetail, ThreadSummary};

#[derive(Debug, Clone)]
pub enum Request {
    Inbox,
    Thread,
    Send,
}

#[derive(Debug, Clone)]
pub enum PostSendAction {
    RefreshInbox,
    OpenThread(String),
}

#[derive(Debug)]
pub enum AppEvent {
    InboxLoaded(Result<Vec<ThreadSummary>, String>),
    ThreadLoaded(Result<ThreadDetail, String>),
    MessageSent(Result<(), String>, PostSendAction),
}
