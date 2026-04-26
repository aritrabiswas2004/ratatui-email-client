/**************************************************************
SPDX License Identifier: GPL-3

Authors: Arnav Waghdhare <arnavwaghdhare@gmail.com>
***************************************************************/


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadSummary {
    pub id: String,
    pub subject: String,
    pub from: String,
    pub date: String,
    pub snippet: String,
    pub message_count: usize,
    pub unread: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MessageDetail {
    pub id: String,
    pub from: String,
    pub to: String,
    pub subject: String,
    pub date: String,
    pub body: String,
    pub message_id: Option<String>,
    pub references: Option<String>,
    pub snippet: String,
    pub internal_date: Option<u64>,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadDetail {
    pub id: String,
    pub subject: String,
    pub snippet: String,
    pub messages: Vec<MessageDetail>,
}

impl ThreadDetail {
    pub fn latest_message(&self) -> Option<&MessageDetail> {
        self.messages.last()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplyContext {
    pub thread_id: String,
    pub in_reply_to: Option<String>,
    pub references: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposeDraft {
    pub to: String,
    pub subject: String,
    pub body: String,
    pub reply_context: Option<ReplyContext>,
}

impl ComposeDraft {
    pub fn new() -> Self {
        Self {
            to: String::new(),
            subject: String::new(),
            body: String::new(),
            reply_context: None,
        }
    }

    pub fn for_reply(thread: &ThreadDetail) -> Self {
        let latest = thread.latest_message();
        let subject = thread
            .subject
            .trim()
            .strip_prefix("Re: ")
            .map(|s| format!("Re: {s}"))
            .unwrap_or_else(|| format!("Re: {}", thread.subject.trim()))
            .trim()
            .to_string();

        let (to, in_reply_to, references) = if let Some(message) = latest {
            (
                message.from.clone(),
                message.message_id.clone(),
                merge_references(message.references.clone(), message.message_id.clone()),
            )
        } else {
            (String::new(), None, None)
        };

        Self {
            to,
            subject,
            body: String::new(),
            reply_context: Some(ReplyContext {
                thread_id: thread.id.clone(),
                in_reply_to,
                references,
            }),
        }
    }

    pub fn is_reply(&self) -> bool {
        self.reply_context.is_some()
    }
}

fn merge_references(references: Option<String>, message_id: Option<String>) -> Option<String> {
    let mut refs = references.unwrap_or_default();
    if let Some(message_id) = message_id {
        if refs.is_empty() {
            refs = message_id;
        } else if !refs.contains(&message_id) {
            refs.push(' ');
            refs.push_str(&message_id);
        }
    }

    if refs.is_empty() { None } else { Some(refs) }
}
