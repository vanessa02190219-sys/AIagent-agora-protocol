#!/bin/bash
# Initialize Qdrant collections for Agora Protocol.
# Requires: Qdrant running on QDRANT_URL (default: http://localhost:6333)
#
# Usage:
#   export QDRANT_URL=http://localhost:6333
#   bash scripts/init_qdrant.sh

set -euo pipefail

QDRANT_URL="${QDRANT_URL:-http://localhost:6333}"
VECTOR_SIZE=1536
DISTANCE="Cosine"

echo "==> Initializing Qdrant collections on $QDRANT_URL"

# --- topic_embeddings ---
# Stores topic title embeddings for semantic dedup and search.
echo ""
echo "-- Creating collection: topic_embeddings"
curl -s -X PUT "$QDRANT_URL/collections/topic_embeddings" \
  -H "Content-Type: application/json" \
  -d '{
    "vectors": {
      "size": '$VECTOR_SIZE',
      "distance": "'$DISTANCE'"
    },
    "hnsw_config": {
      "m": 16,
      "ef_construct": 200
    },
    "optimizers_config": {
      "default_segment_number": 2
    }
  }' | python3 -m json.tool 2>/dev/null || echo "  (may already exist)"

# Create payload index for topic_id
curl -s -X PUT "$QDRANT_URL/collections/topic_embeddings/index" \
  -H "Content-Type: application/json" \
  -d '{
    "field_name": "topic_id",
    "field_schema": "keyword"
  }' | python3 -m json.tool 2>/dev/null || true

# Create payload index for lang
curl -s -X PUT "$QDRANT_URL/collections/topic_embeddings/index" \
  -H "Content-Type: application/json" \
  -d '{
    "field_name": "lang",
    "field_schema": "keyword"
  }' | python3 -m json.tool 2>/dev/null || true

# --- post_embeddings ---
# Stores post content embeddings for semantic search.
echo ""
echo "-- Creating collection: post_embeddings"
curl -s -X PUT "$QDRANT_URL/collections/post_embeddings" \
  -H "Content-Type: application/json" \
  -d '{
    "vectors": {
      "size": '$VECTOR_SIZE',
      "distance": "'$DISTANCE'"
    },
    "hnsw_config": {
      "m": 16,
      "ef_construct": 200
    },
    "optimizers_config": {
      "default_segment_number": 2
    }
  }' | python3 -m json.tool 2>/dev/null || echo "  (may already exist)"

# Create payload indices
for field in post_id topic_id author_did lang; do
  curl -s -X PUT "$QDRANT_URL/collections/post_embeddings/index" \
    -H "Content-Type: application/json" \
    -d "{\"field_name\": \"$field\", \"field_schema\": \"keyword\"}" \
    | python3 -m json.tool 2>/dev/null || true
done

echo ""
echo "==> Qdrant initialization complete."
echo "    Collections: topic_embeddings, post_embeddings"
echo "    Vector size: $VECTOR_SIZE"
echo "    Distance:    $DISTANCE"
