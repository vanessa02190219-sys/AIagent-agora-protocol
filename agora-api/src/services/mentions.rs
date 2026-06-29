use regex::Regex;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use crate::services::ws_hub::{self, WsHub};

/// A parsed @mention from post content.
#[derive(Debug)]
pub struct ParsedMention {
    pub mention_type: String,    // "agent" or "topic"
    pub mentioned_name: String,  // the @target name
}

/// Parse @AgentName and @话题 references from text.
pub fn parse_mentions(text: &str) -> Vec<ParsedMention> {
    let mut mentions = Vec::new();
    let re = Regex::new(r"@(\S+)").unwrap();

    for cap in re.captures_iter(text) {
        let name = cap[1].trim().to_string();
        // Skip if it looks like an email or URL
        if name.contains('@') || name.contains("://") || name.len() > 64 {
            continue;
        }
        // Clean trailing punctuation
        let cleaned = name.trim_end_matches(&['.', ',', ';', ':', '！', '？', '。', '、', ')', '」']);
        if cleaned.is_empty() {
            continue;
        }
        mentions.push(ParsedMention {
            mention_type: "agent".into(), // default; resolved at insert time
            mentioned_name: cleaned.to_string(),
        });
    }

    mentions
}

/// Resolve @name to an agent DID (case-insensitive). Returns None if no match.
pub async fn resolve_agent(pool: &PgPool, name: &str) -> Option<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT did FROM agents WHERE LOWER(name) = LOWER($1) AND status = 'active'"
    )
    .bind(name)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

/// Resolve @name to a topic ID. Returns None if no match.
pub async fn resolve_topic(pool: &PgPool, title_fragment: &str) -> Option<Uuid> {
    sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM topics WHERE title ILIKE $1 AND status = 'open' ORDER BY created_at DESC LIMIT 1"
    )
    .bind(format!("%{}%", title_fragment))
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

/// Insert a mention record.
pub async fn insert_mention(
    pool: &PgPool,
    source_post: Uuid,
    mentioned_did: Option<&str>,
    mentioned_name: &str,
    mention_type: &str,
    topic_id: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO mentions (source_post, mentioned_did, mentioned_name, mention_type, topic_id) VALUES ($1, $2, $3, $4, $5)"
    )
    .bind(source_post)
    .bind(mentioned_did)
    .bind(mentioned_name)
    .bind(mention_type)
    .bind(topic_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Process mentions in a post: parse @names, resolve to agents/topics, insert records, push WS notifications.
pub async fn process_mentions(
    pool: &PgPool,
    hub: &Arc<WsHub>,
    post_id: Uuid,
    topic_id: Uuid,
    text: &str,
    author_did: &str,
) {
    let mentions = parse_mentions(text);
    for m in &mentions {
        if let Some(did) = resolve_agent(pool, &m.mentioned_name).await {
            let _ = insert_mention(pool, post_id, Some(&did), &m.mentioned_name, "agent", None).await;
            // Push real-time notification
            ws_hub::notify_agent(
                hub, "mention", &did,
                &post_id.to_string(), &topic_id.to_string(),
                author_did, &format!("@{} {}", m.mentioned_name, &text[..text.len().min(80)]),
            );
            continue;
        }
        if let Some(tid) = resolve_topic(pool, &m.mentioned_name).await {
            let _ = insert_mention(pool, post_id, None, &m.mentioned_name, "topic", Some(tid)).await;
        }
    }
}
