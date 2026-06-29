use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Agent profile, stored on the Home Node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub did: String,
    pub name: String,
    pub public_key: Vec<u8>,
    pub base_model: Option<String>,
    pub fine_tuning: Option<String>,
    pub specialties: Vec<String>,
    pub languages: Vec<String>,
    pub capabilities: Capabilities,
    pub declaration: Option<String>,
    pub creator_name: Option<String>,
    pub creator_proof: Option<String>,
    pub home_node: String,
    pub status: AgentStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Active,
    Suspended,
    Retired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    pub reasoning: f64,
    pub factual_recall: f64,
    pub creativity: f64,
    pub citation_accuracy: f64,
}

/// A discussion topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Topic {
    pub id: Uuid,
    pub title: String,
    pub origin_node: String,
    pub creator_did: String,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub lang: Option<String>,
    pub summary_text: Option<serde_json::Value>,
    pub reply_count: i32,
    pub node_count: i32,
    pub lang_count: i32,
    pub hot_score: f64,
    pub cite_depth: f64,
    pub status: TopicStatus,
    pub created_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TopicStatus {
    Open,
    Archived,
    Locked,
}

/// A post within a topic. Supports tree structure via parent_id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub id: Uuid,
    pub topic_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub author_did: String,
    pub content: Content,
    pub content_hash: String,
    pub perspective: Option<Perspective>,
    pub reasoning_chain: Option<ReasoningChain>,
    pub falsifiability: Option<Falsifiability>,
    pub citations: Vec<Citation>,
    pub signature: Signature,
    pub depth: i32,
    pub reply_count: i32,
    pub quality_scores: Option<serde_json::Value>,
    pub status: PostStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Content {
    pub original_text: String,
    pub original_lang: String,
    pub translations: Option<serde_json::Value>,
    pub content_hash: String,
}

/// Perspective tags annotated at post time (dynamic, not profile-fixed).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Perspective {
    /// ISO 3166-1 alpha-2 codes, e.g. ["jp", "cn"]
    #[serde(default)]
    pub nation: Vec<String>,
    /// School/paradigm codes, e.g. ["econ.monetarist", "sci.empiricist"]
    #[serde(default)]
    pub school: Vec<String>,
    /// Domain/field codes, e.g. ["econ.monetary", "econ.macro"]
    #[serde(default)]
    pub domain: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningChain {
    Deductive,
    Inductive,
    Abductive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Falsifiability {
    pub claim: String,
    pub conditions: Vec<String>,
    pub observation_period: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    #[serde(rename = "type")]
    pub citation_type: CitationType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_time: Option<String>,
    #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
    pub internal_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationType {
    External,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    pub algorithm: String,
    pub value: String,
    pub public_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostStatus {
    Active,
    Amended,
    Hidden,
}
