use qdrant_client::prelude::*;
use qdrant_client::qdrant::{
    CreateCollectionBuilder, Distance, PointStruct, SearchPointsBuilder, VectorParamsBuilder,
};
use std::collections::HashMap;
use uuid::Uuid;

/// Semantic search service using Qdrant vector database.
pub struct SemanticSearch {
    client: QdrantClient,
    collection: String,
}

impl SemanticSearch {
    pub async fn new(url: &str, collection: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let config = QdrantClientConfig::from_url(url);
        let client = QdrantClient::new(Some(config)).await?;

        // Ensure collection exists
        let collections = client.list_collections().await?;
        let exists = collections.collections.iter().any(|c| c.name == collection);

        if !exists {
            client
                .create_collection(
                    CreateCollectionBuilder::new(collection)
                        .vectors_config(VectorParamsBuilder::new(256, Distance::Cosine)),
                )
                .await?;
        }

        Ok(Self {
            client,
            collection: collection.into(),
        })
    }

    /// Generate a simple embedding vector from text using character n-grams.
    /// This is a lightweight approach that doesn't require an external API.
    /// Falls back gracefully when Qdrant is unavailable.
    pub fn embed_text(text: &str) -> Vec<f32> {
        let text = text.to_lowercase();
        let mut vec = vec![0.0f32; 256];

        // Character bigram hashing into 256-dim vector
        let chars: Vec<char> = text.chars().collect();
        for window in chars.windows(2) {
            let hash = (window[0] as u32 * 31 + window[1] as u32) % 256;
            vec[hash as usize] += 1.0;
        }

        // Normalize
        let sum: f32 = vec.iter().sum();
        if sum > 0.0 {
            for v in &mut vec {
                *v /= sum;
            }
        }

        vec
    }

    /// Index a topic for semantic search.
    pub async fn index_topic(&self, topic_id: Uuid, title: &str, tags: &[String]) {
        let combined = format!("{} {}", title, tags.join(" "));
        let vector = Self::embed_text(&combined);

        let point = PointStruct::new(
            topic_id.to_string(),
            vector,
            HashMap::from_iter([
                ("title".into(), serde_json::Value::String(title.into())),
                ("tags".into(), serde_json::Value::String(tags.join(","))),
            ]),
        );

        if let Err(e) = self
            .client
            .upsert_points_blocking(self.collection.clone(), None, vec![point], None)
            .await
        {
            tracing::warn!("Qdrant index failed for topic {}: {:?}", topic_id, e);
        }
    }

    /// Search for semantically similar topics.
    pub async fn search(&self, query: &str, limit: u64) -> Vec<Uuid> {
        let vector = Self::embed_text(query);

        match self
            .client
            .search_points(
                SearchPointsBuilder::new(self.collection.clone(), vector, limit).with_payload(true),
            )
            .await
        {
            Ok(response) => response
                .result
                .into_iter()
                .filter_map(|p| p.id.and_then(|id| id.point_id_options))
                .filter_map(|opt| match opt {
                    qdrant_client::qdrant::point_id::PointIdOptions::Uuid(id) => {
                        Some(Uuid::parse_str(&id).ok()?)
                    }
                    _ => None,
                })
                .collect(),
            Err(e) => {
                tracing::warn!("Qdrant search failed: {:?}", e);
                vec![]
            }
        }
    }
}
