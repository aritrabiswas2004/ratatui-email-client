use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use color_eyre::eyre::{Context, Result};
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::json;

use crate::models::{ComposeDraft, MessageDetail, ThreadDetail, ThreadSummary};

const GMAIL_API_BASE: &str = "https://gmail.googleapis.com/gmail/v1/users/me";

#[derive(Clone)]
pub struct GmailClient {
    client: Client,
    access_token: String,
}

impl GmailClient {
    pub fn new(access_token: String) -> Self {
        Self {
            client: Client::new(),
            access_token,
        }
    }

    pub fn list_inbox(&self, limit: usize) -> Result<Vec<ThreadSummary>> {
        let mut threads = self.list_thread_refs(limit)?;
        let mut summaries = Vec::with_capacity(threads.len());

        for thread in threads.drain(..) {
            if let Ok(detail) = self.get_thread(&thread.id) {
                summaries.push(detail.into());
            }
        }

        Ok(summaries)
    }

    pub fn get_thread(&self, thread_id: &str) -> Result<ThreadDetail> {
        let response = self
            .client
            .get(format!("{GMAIL_API_BASE}/threads/{thread_id}"))
            .bearer_auth(&self.access_token)
            .query(&[("format", "full")])
            .send()
            .context("failed to fetch Gmail thread")?
            .error_for_status()
            .context("Google rejected the thread request")?
            .json::<GmailThread>()
            .context("failed to parse Gmail thread response")?;

        Ok(ThreadDetail::from(response))
    }

    pub fn send_message(&self, draft: &ComposeDraft) -> Result<()> {
        let raw = build_raw_message(draft);
        let mut payload = json!({
            "raw": raw,
        });

        if let Some(reply) = &draft.reply_context {
            payload["threadId"] = json!(reply.thread_id);
        }

        self.client
            .post(format!("{GMAIL_API_BASE}/messages/send"))
            .bearer_auth(&self.access_token)
            .json(&payload)
            .send()
            .context("failed to send Gmail message")?
            .error_for_status()
            .context("Google rejected the message send request")?;

        Ok(())
    }

    fn list_thread_refs(&self, limit: usize) -> Result<Vec<GmailThreadRef>> {
        let max_results = limit.to_string();
        let response = self
            .client
            .get(format!("{GMAIL_API_BASE}/threads"))
            .bearer_auth(&self.access_token)
            .query(&[
                ("labelIds", "INBOX"),
                ("maxResults", max_results.as_str()),
                ("includeSpamTrash", "false"),
            ])
            .send()
            .context("failed to list Gmail threads")?
            .error_for_status()
            .context("Google rejected the thread list request")?
            .json::<GmailThreadListResponse>()
            .context("failed to parse Gmail thread list response")?;

        Ok(response.threads.unwrap_or_default())
    }
}

impl From<GmailThread> for ThreadDetail {
    fn from(thread: GmailThread) -> Self {
        let mut messages = thread
            .messages
            .into_iter()
            .map(MessageDetail::from)
            .collect::<Vec<_>>();
        messages.sort_by_key(|message| message.internal_date.unwrap_or(0));

        let subject = messages
            .last()
            .map(|message| message.subject.clone())
            .unwrap_or_else(|| "(no subject)".to_string());

        let snippet = thread.snippet.unwrap_or_default();

        Self {
            id: thread.id,
            subject,
            snippet,
            messages,
        }
    }
}

impl From<ThreadDetail> for ThreadSummary {
    fn from(thread: ThreadDetail) -> Self {
        let latest = thread
            .messages
            .last()
            .cloned()
            .unwrap_or_else(MessageDetail::default);

        Self {
            id: thread.id,
            subject: latest.subject,
            from: latest.from,
            date: latest.date,
            snippet: thread.snippet,
            message_count: thread.messages.len(),
            unread: thread
                .messages
                .iter()
                .any(|message| message.labels.iter().any(|label| label == "UNREAD")),
        }
    }
}

#[derive(Debug, Deserialize)]
struct GmailThreadListResponse {
    threads: Option<Vec<GmailThreadRef>>,
}

#[derive(Debug, Deserialize)]
struct GmailThreadRef {
    id: String,
}

#[derive(Debug, Deserialize)]
struct GmailThread {
    id: String,
    snippet: Option<String>,
    messages: Vec<GmailMessage>,
}

#[derive(Debug, Deserialize)]
struct GmailMessage {
    id: String,
    #[serde(default, rename = "internalDate")]
    internal_date: Option<String>,
    #[serde(default)]
    label_ids: Vec<String>,
    snippet: Option<String>,
    payload: Option<MessagePayload>,
}

#[derive(Debug, Deserialize)]
struct MessagePayload {
    #[serde(rename = "mimeType")]
    mime_type: String,
    #[serde(default)]
    headers: Vec<MessageHeader>,
    #[serde(default)]
    body: MessageBody,
    #[serde(default)]
    parts: Vec<MessagePayload>,
}

#[derive(Debug, Deserialize, Default)]
struct MessageBody {
    #[serde(default)]
    data: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MessageHeader {
    name: String,
    value: String,
}

impl From<GmailMessage> for MessageDetail {
    fn from(message: GmailMessage) -> Self {
        let payload = message.payload.as_ref();
        let headers = payload
            .map(|payload| payload.headers.as_slice())
            .unwrap_or(&[]);
        let subject =
            header_value(headers, "Subject").unwrap_or_else(|| "(no subject)".to_string());
        let from = header_value(headers, "From").unwrap_or_default();
        let to = header_value(headers, "To").unwrap_or_default();
        let date = header_value(headers, "Date")
            .unwrap_or_else(|| message.internal_date.clone().unwrap_or_default());
        let message_id = header_value(headers, "Message-ID");
        let references = header_value(headers, "References");
        let snippet = message.snippet.clone().unwrap_or_default();
        let body = payload
            .and_then(extract_body)
            .or_else(|| {
                if snippet.is_empty() {
                    None
                } else {
                    Some(snippet.clone())
                }
            })
            .unwrap_or_default();
        let internal_date = message
            .internal_date
            .as_deref()
            .and_then(|value| value.parse::<u64>().ok());

        Self {
            id: message.id,
            from,
            to,
            subject,
            date,
            body,
            message_id,
            references,
            snippet,
            internal_date,
            labels: message.label_ids,
        }
    }
}

fn header_value(headers: &[MessageHeader], wanted: &str) -> Option<String> {
    headers
        .iter()
        .rev()
        .find(|header| header.name.eq_ignore_ascii_case(wanted))
        .map(|header| header.value.clone())
}

fn extract_body(payload: &MessagePayload) -> Option<String> {
    if let Some(data) = &payload.body.data {
        if let Some(decoded) = decode_base64url(data) {
            return Some(normalize_text(&decoded));
        }
    }

    for part in &payload.parts {
        if part.mime_type.starts_with("text/plain") {
            if let Some(body) = extract_body(part) {
                return Some(body);
            }
        }
    }

    for part in &payload.parts {
        if let Some(body) = extract_body(part) {
            return Some(body);
        }
    }

    None
}

fn decode_base64url(value: &str) -> Option<String> {
    URL_SAFE_NO_PAD
        .decode(value.as_bytes())
        .ok()
        .or_else(|| {
            base64::engine::general_purpose::URL_SAFE
                .decode(value.as_bytes())
                .ok()
        })
        .and_then(|bytes| String::from_utf8(bytes).ok())
}

fn normalize_text(input: &str) -> String {
    input.replace("\r\n", "\n").replace('\r', "\n")
}

fn build_raw_message(draft: &ComposeDraft) -> String {
    let to = sanitize_header(&draft.to);
    let subject = sanitize_header(&draft.subject);
    let body = draft.body.replace("\r\n", "\n").replace('\r', "\n");
    let body = body.replace('\n', "\r\n");

    let mut message = String::new();
    message.push_str(&format!("To: {to}\r\n"));
    message.push_str(&format!("Subject: {subject}\r\n"));
    message.push_str("MIME-Version: 1.0\r\n");
    message.push_str("Content-Type: text/plain; charset=\"UTF-8\"\r\n");
    message.push_str("Content-Transfer-Encoding: 8bit\r\n");

    if let Some(reply) = &draft.reply_context {
        if let Some(in_reply_to) = &reply.in_reply_to {
            message.push_str(&format!(
                "In-Reply-To: {}\r\n",
                sanitize_header(in_reply_to)
            ));
        }
        if let Some(references) = &reply.references {
            message.push_str(&format!("References: {}\r\n", sanitize_header(references)));
        }
    }

    message.push_str("\r\n");
    message.push_str(&body);
    URL_SAFE_NO_PAD.encode(message.as_bytes())
}

fn sanitize_header(value: &str) -> String {
    value.replace(['\r', '\n'], " ").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_message_contains_reply_headers() {
        let draft = ComposeDraft {
            to: "person@example.com".into(),
            subject: "Re: Hello".into(),
            body: "Thanks".into(),
            reply_context: Some(crate::models::ReplyContext {
                thread_id: "thread".into(),
                in_reply_to: Some("<id@example.com>".into()),
                references: Some("<old@example.com> <id@example.com>".into()),
            }),
        };

        let raw = build_raw_message(&draft);
        let decoded = URL_SAFE_NO_PAD
            .decode(raw.as_bytes())
            .expect("message should decode");
        let decoded = String::from_utf8(decoded).expect("message should be utf8");

        assert!(decoded.contains("In-Reply-To: <id@example.com>"));
        assert!(decoded.contains("References: <old@example.com> <id@example.com>"));
    }

    #[test]
    fn body_extraction_prefers_plain_text() {
        let payload = MessagePayload {
            mime_type: "multipart/alternative".into(),
            headers: vec![],
            body: MessageBody::default(),
            parts: vec![
                MessagePayload {
                    mime_type: "text/html".into(),
                    headers: vec![],
                    body: MessageBody {
                        data: Some(URL_SAFE_NO_PAD.encode("<p>ignored</p>".as_bytes())),
                    },
                    parts: vec![],
                },
                MessagePayload {
                    mime_type: "text/plain".into(),
                    headers: vec![],
                    body: MessageBody {
                        data: Some(URL_SAFE_NO_PAD.encode("preferred".as_bytes())),
                    },
                    parts: vec![],
                },
            ],
        };

        assert_eq!(extract_body(&payload).as_deref(), Some("preferred"));
    }
}
