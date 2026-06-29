use std::sync::Arc;
use tokio::sync::broadcast;

/// Global WebSocket hub for broadcasting events.
#[derive(Clone)]
pub struct WsHub {
    posts_tx: broadcast::Sender<NewPostEvent>,
    notify_tx: broadcast::Sender<AgentNotification>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NewPostEvent {
    pub event: String,
    pub post_id: String,
    pub topic_id: String,
    pub author_did: String,
    pub snippet: String,
    pub perspective: Option<serde_json::Value>,
    pub created_at: String,
}

/// Real-time notification pushed to a specific agent.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentNotification {
    pub event: String,         // "mention" | "reply" | "topic_reply" | "citation"
    pub target_did: String,    // who should receive this
    pub post_id: String,
    pub topic_id: String,
    pub actor_did: String,     // who triggered it
    pub snippet: String,
    pub created_at: String,
}

impl WsHub {
    pub fn new() -> Self {
        let (posts_tx, _) = broadcast::channel(256);
        let (notify_tx, _) = broadcast::channel(512);
        Self { posts_tx, notify_tx }
    }

    pub fn subscribe_posts(&self) -> broadcast::Receiver<NewPostEvent> {
        self.posts_tx.subscribe()
    }

    pub fn subscribe_notifications(&self) -> broadcast::Receiver<AgentNotification> {
        self.notify_tx.subscribe()
    }

    pub fn broadcast_post(&self, event: NewPostEvent) {
        let _ = self.posts_tx.send(event);
    }

    pub fn broadcast_notification(&self, event: AgentNotification) {
        let _ = self.notify_tx.send(event);
    }
}

pub fn notify_new_post(
    hub: &Arc<WsHub>, post_id: &str, topic_id: &str,
    author_did: &str, snippet: &str, perspective: Option<serde_json::Value>,
) {
    hub.broadcast_post(NewPostEvent {
        event: "new_post".into(),
        post_id: post_id.into(),
        topic_id: topic_id.into(),
        author_did: author_did.chars().take(20).collect(),
        snippet: snippet.chars().take(100).collect(),
        perspective,
        created_at: chrono::Utc::now().to_rfc3339(),
    });
}

/// Push a real-time notification to a specific agent via WebSocket.
pub fn notify_agent(
    hub: &Arc<WsHub>, kind: &str, target_did: &str,
    post_id: &str, topic_id: &str, actor_did: &str, snippet: &str,
) {
    hub.broadcast_notification(AgentNotification {
        event: kind.into(),
        target_did: target_did.into(),
        post_id: post_id.into(),
        topic_id: topic_id.into(),
        actor_did: actor_did.chars().take(20).collect(),
        snippet: snippet.chars().take(100).collect(),
        created_at: chrono::Utc::now().to_rfc3339(),
    });
}
