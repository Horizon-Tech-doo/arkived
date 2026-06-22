//! Azure Queue Storage backend, hand-rolled on `reqwest`.
//!
//! Reuses the same [`HttpPipeline`](super::http::HttpPipeline) (auth, signing,
//! retry) as the blob backend — only the endpoint and REST shapes differ.

use super::http::{Body, HttpPipeline, RequestTemplate};
use super::xml::parse_xml;
use crate::auth::ResolvedCredential;
use crate::policy::{Action, ActionContext, PolicyDecision};
use crate::types::AzureEnvironment;
use crate::{Ctx, Error};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// A queue in the list-queues response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Queue {
    /// Queue name.
    pub name: String,
}

/// A queue message (from peek or dequeue).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueMessage {
    /// Server-assigned message id.
    pub message_id: String,
    /// Message text (decoded as the queue stored it).
    pub message_text: String,
    /// Number of times this message has been dequeued.
    pub dequeue_count: Option<u64>,
    /// Pop receipt — required (with id) to delete a dequeued message.
    pub pop_receipt: Option<String>,
    /// Insertion time (raw RFC-1123 string as returned).
    pub insertion_time: Option<String>,
    /// Expiration time (raw RFC-1123 string as returned).
    pub expiration_time: Option<String>,
}

/// Azure Queue backend.
#[derive(Clone)]
pub struct AzureQueueBackend {
    endpoint: url::Url,
    credential: Arc<ResolvedCredential>,
    http: reqwest::Client,
}

impl std::fmt::Debug for AzureQueueBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AzureQueueBackend")
            .field("endpoint", &self.endpoint.as_str())
            .field("credential", &self.credential.kind())
            .finish()
    }
}

impl AzureQueueBackend {
    /// Construct from a queue endpoint URL and resolved credential.
    pub fn new(endpoint: url::Url, credential: ResolvedCredential) -> crate::Result<Self> {
        Ok(Self {
            endpoint,
            credential: Arc::new(credential),
            http: reqwest::Client::new(),
        })
    }

    /// The configured queue endpoint URL.
    pub fn endpoint(&self) -> &url::Url {
        &self.endpoint
    }

    /// Build from a storage account name + Azure environment:
    /// `https://<account>.queue.<suffix>`.
    pub fn for_account(
        account_name: &str,
        environment: &AzureEnvironment,
        credential: ResolvedCredential,
    ) -> crate::Result<Self> {
        let endpoint = url::Url::parse(&format!(
            "https://{}.queue.{}",
            account_name,
            environment.storage_suffix()
        ))
        .map_err(|e| Error::Backend(format!("build queue endpoint: {e}")))?;
        Self::new(endpoint, credential)
    }

    /// Derive a queue backend from a blob endpoint URL (swaps `.blob.` for
    /// `.queue.` in the host), reusing the same credential. Lets a surface that
    /// already resolved a blob backend reach the queue service for the account.
    pub fn from_blob_endpoint(
        blob_endpoint: &url::Url,
        credential: ResolvedCredential,
    ) -> crate::Result<Self> {
        let host = blob_endpoint
            .host_str()
            .ok_or_else(|| Error::Backend("blob endpoint has no host".into()))?;
        let queue_host = host.replacen(".blob.", ".queue.", 1);
        let mut endpoint = blob_endpoint.clone();
        endpoint
            .set_host(Some(&queue_host))
            .map_err(|e| Error::Backend(format!("derive queue host: {e}")))?;
        endpoint.set_path("/");
        endpoint.set_query(None);
        Self::new(endpoint, credential)
    }

    fn pipeline(&self) -> HttpPipeline<'_> {
        HttpPipeline {
            http: &self.http,
            credential: &self.credential,
        }
    }

    /// List all queues in the account.
    pub async fn list_queues(&self) -> crate::Result<Vec<Queue>> {
        let mut out = Vec::new();
        let mut marker: Option<String> = None;
        loop {
            let mut url = self.endpoint.clone();
            url.set_path("/");
            let mut query = String::from("comp=list");
            if let Some(m) = &marker {
                query.push_str(&format!("&marker={}", urlencoding::encode(m)));
            }
            url.set_query(Some(&query));

            let resp = self
                .pipeline()
                .send(RequestTemplate {
                    method: Method::GET,
                    url,
                    headers: vec![],
                    body: Body::Empty,
                })
                .await?;
            let body = resp
                .text()
                .await
                .map_err(|e| Error::Backend(format!("read list-queues body: {e}")))?;
            let parsed: ListQueuesResult = parse_xml(&body)?;
            out.extend(
                parsed
                    .queues
                    .queues
                    .into_iter()
                    .map(|q| Queue { name: q.name }),
            );
            match parsed.next_marker.filter(|m| !m.is_empty()) {
                Some(m) => marker = Some(m),
                None => break,
            }
        }
        Ok(out)
    }

    /// Create a queue (idempotent server-side). Not policy-gated.
    pub async fn create_queue(&self, name: &str) -> crate::Result<()> {
        let mut url = self.endpoint.clone();
        url.set_path(&format!("/{name}"));
        url.set_query(None);
        self.pipeline()
            .send(RequestTemplate {
                method: Method::PUT,
                url,
                headers: vec![],
                body: Body::Empty,
            })
            .await
            .map(|_| ())
    }

    /// Delete a queue and all its messages. Policy-gated (destructive).
    pub async fn delete_queue(&self, ctx: &Ctx, name: &str) -> crate::Result<()> {
        self.gate(
            ctx,
            "delete_queue",
            name,
            format!("delete queue '{name}' and all its messages"),
            false,
        )
        .await?;
        let mut url = self.endpoint.clone();
        url.set_path(&format!("/{name}"));
        url.set_query(None);
        self.pipeline()
            .send(RequestTemplate {
                method: Method::DELETE,
                url,
                headers: vec![],
                body: Body::Empty,
            })
            .await
            .map(|_| ())
    }

    /// Enqueue a message. `ttl_secs` of `None` uses the service default; a
    /// negative value means "never expire". Not policy-gated (additive).
    pub async fn put_message(&self, queue: &str, text: &str) -> crate::Result<()> {
        let body = format!(
            "<QueueMessage><MessageText>{}</MessageText></QueueMessage>",
            xml_escape(text)
        );
        let bytes = bytes::Bytes::from(body.into_bytes());
        let mut url = self.endpoint.clone();
        url.set_path(&format!("/{queue}/messages"));
        url.set_query(None);
        let headers = vec![
            ("content-length".to_string(), bytes.len().to_string()),
            ("content-type".to_string(), "application/xml".to_string()),
        ];
        self.pipeline()
            .send(RequestTemplate {
                method: Method::POST,
                url,
                headers,
                body: Body::Bytes(bytes),
            })
            .await
            .map(|_| ())
    }

    /// Peek up to `count` messages without dequeuing them.
    pub async fn peek_messages(&self, queue: &str, count: u32) -> crate::Result<Vec<QueueMessage>> {
        let mut url = self.endpoint.clone();
        url.set_path(&format!("/{queue}/messages"));
        url.set_query(Some(&format!("peekonly=true&numofmessages={count}")));
        self.fetch_messages(url).await
    }

    /// Dequeue up to `count` messages, hiding them for `visibility_secs`. The
    /// returned messages carry a `pop_receipt` needed to delete them.
    pub async fn get_messages(
        &self,
        queue: &str,
        count: u32,
        visibility_secs: u32,
    ) -> crate::Result<Vec<QueueMessage>> {
        let mut url = self.endpoint.clone();
        url.set_path(&format!("/{queue}/messages"));
        url.set_query(Some(&format!(
            "numofmessages={count}&visibilitytimeout={visibility_secs}"
        )));
        self.fetch_messages(url).await
    }

    /// Delete a dequeued message by id + pop receipt. Policy-gated.
    pub async fn delete_message(
        &self,
        ctx: &Ctx,
        queue: &str,
        message_id: &str,
        pop_receipt: &str,
    ) -> crate::Result<()> {
        self.gate(
            ctx,
            "delete_message",
            queue,
            format!("delete message {message_id} from queue '{queue}'"),
            false,
        )
        .await?;
        let mut url = self.endpoint.clone();
        url.set_path(&format!("/{queue}/messages/{message_id}"));
        url.set_query(Some(&format!(
            "popreceipt={}",
            urlencoding::encode(pop_receipt)
        )));
        self.pipeline()
            .send(RequestTemplate {
                method: Method::DELETE,
                url,
                headers: vec![],
                body: Body::Empty,
            })
            .await
            .map(|_| ())
    }

    /// Clear all messages from a queue. Policy-gated (destructive).
    pub async fn clear_messages(&self, ctx: &Ctx, queue: &str) -> crate::Result<()> {
        self.gate(
            ctx,
            "clear_messages",
            queue,
            format!("clear all messages from queue '{queue}'"),
            false,
        )
        .await?;
        let mut url = self.endpoint.clone();
        url.set_path(&format!("/{queue}/messages"));
        url.set_query(None);
        self.pipeline()
            .send(RequestTemplate {
                method: Method::DELETE,
                url,
                headers: vec![],
                body: Body::Empty,
            })
            .await
            .map(|_| ())
    }

    async fn fetch_messages(&self, url: url::Url) -> crate::Result<Vec<QueueMessage>> {
        let resp = self
            .pipeline()
            .send(RequestTemplate {
                method: Method::GET,
                url,
                headers: vec![],
                body: Body::Empty,
            })
            .await?;
        let body = resp
            .text()
            .await
            .map_err(|e| Error::Backend(format!("read messages body: {e}")))?;
        let parsed: QueueMessagesList = parse_xml(&body)?;
        Ok(parsed
            .messages
            .into_iter()
            .map(|m| QueueMessage {
                message_id: m.message_id,
                message_text: m.message_text.unwrap_or_default(),
                dequeue_count: m.dequeue_count,
                pop_receipt: m.pop_receipt,
                insertion_time: m.insertion_time,
                expiration_time: m.expiration_time,
            })
            .collect())
    }

    async fn gate(
        &self,
        ctx: &Ctx,
        verb: &str,
        target: &str,
        summary: String,
        reversible: bool,
    ) -> crate::Result<()> {
        let decision = ctx
            .policy
            .confirm(
                &Action {
                    verb: verb.to_string(),
                    target: target.to_string(),
                    summary,
                    reversible,
                },
                &ActionContext::default(),
            )
            .await;
        match decision {
            PolicyDecision::Allow | PolicyDecision::AllowAlways => Ok(()),
            PolicyDecision::Deny(reason) => Err(Error::PolicyDenied(reason)),
        }
    }
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

// ---- XML response models -------------------------------------------------

#[derive(Deserialize)]
struct ListQueuesResult {
    #[serde(rename = "Queues", default)]
    queues: QueueList,
    #[serde(rename = "NextMarker", default)]
    next_marker: Option<String>,
}

#[derive(Deserialize, Default)]
struct QueueList {
    #[serde(rename = "Queue", default)]
    queues: Vec<XmlQueue>,
}

#[derive(Deserialize)]
struct XmlQueue {
    #[serde(rename = "Name")]
    name: String,
}

#[derive(Deserialize, Default)]
struct QueueMessagesList {
    #[serde(rename = "QueueMessage", default)]
    messages: Vec<XmlMessage>,
}

#[derive(Deserialize)]
struct XmlMessage {
    #[serde(rename = "MessageId")]
    message_id: String,
    #[serde(rename = "MessageText", default)]
    message_text: Option<String>,
    #[serde(rename = "DequeueCount", default)]
    dequeue_count: Option<u64>,
    #[serde(rename = "PopReceipt", default)]
    pop_receipt: Option<String>,
    #[serde(rename = "InsertionTime", default)]
    insertion_time: Option<String>,
    #[serde(rename = "ExpirationTime", default)]
    expiration_time: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::DenyAllPolicy;
    use crate::progress::NoopSink;
    use crate::types::{AuthKind, ResourceKind};
    use async_trait::async_trait;

    struct FakeAuth;
    #[async_trait]
    impl crate::auth::AuthProvider for FakeAuth {
        fn kind(&self) -> AuthKind {
            AuthKind::Anonymous
        }
        fn display_name(&self) -> &str {
            "fake"
        }
        async fn resolve(&self) -> crate::Result<ResolvedCredential> {
            Ok(ResolvedCredential::Anonymous)
        }
        fn supports(&self, _: ResourceKind) -> bool {
            true
        }
    }

    #[test]
    fn from_blob_endpoint_swaps_host() {
        let blob = url::Url::parse("https://acme.blob.core.windows.net/c/b?x=1").unwrap();
        let q =
            AzureQueueBackend::from_blob_endpoint(&blob, ResolvedCredential::Anonymous).unwrap();
        assert_eq!(
            q.endpoint().as_str(),
            "https://acme.queue.core.windows.net/"
        );
    }

    #[tokio::test]
    async fn list_queues_parses_names() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/?comp=list")
            .with_status(200)
            .with_body(
                "<?xml version=\"1.0\"?><EnumerationResults><Queues>\
                 <Queue><Name>jobs</Name></Queue>\
                 <Queue><Name>events</Name></Queue>\
                 </Queues><NextMarker/></EnumerationResults>",
            )
            .create_async()
            .await;
        let endpoint = url::Url::parse(&server.url()).unwrap();
        let backend = AzureQueueBackend::new(endpoint, ResolvedCredential::Anonymous).unwrap();
        let queues = backend.list_queues().await.unwrap();
        assert_eq!(queues.len(), 2);
        assert_eq!(queues[0].name, "jobs");
        assert_eq!(queues[1].name, "events");
    }

    #[tokio::test]
    async fn peek_messages_parses() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/q/messages?peekonly=true&numofmessages=1")
            .with_status(200)
            .with_body(
                "<?xml version=\"1.0\"?><QueueMessagesList><QueueMessage>\
                 <MessageId>abc-123</MessageId>\
                 <DequeueCount>2</DequeueCount>\
                 <MessageText>hello</MessageText>\
                 </QueueMessage></QueueMessagesList>",
            )
            .create_async()
            .await;
        let endpoint = url::Url::parse(&server.url()).unwrap();
        let backend = AzureQueueBackend::new(endpoint, ResolvedCredential::Anonymous).unwrap();
        let msgs = backend.peek_messages("q", 1).await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].message_id, "abc-123");
        assert_eq!(msgs[0].message_text, "hello");
        assert_eq!(msgs[0].dequeue_count, Some(2));
    }

    #[tokio::test]
    async fn delete_queue_denied_short_circuits() {
        let endpoint = url::Url::parse("http://127.0.0.1:1/").unwrap();
        let backend = AzureQueueBackend::new(endpoint, ResolvedCredential::Anonymous).unwrap();
        let ctx =
            Ctx::new(Arc::new(FakeAuth), Arc::new(DenyAllPolicy)).with_progress(Arc::new(NoopSink));
        let err = backend.delete_queue(&ctx, "q").await.unwrap_err();
        assert!(matches!(err, Error::PolicyDenied(_)));
    }

    #[test]
    fn message_xml_escapes_text() {
        assert_eq!(xml_escape("a<b>&c"), "a&lt;b&gt;&amp;c");
    }
}
