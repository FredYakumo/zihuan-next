//! Uniform retrieval-store operations across the Weaviate and Elasticsearch
//! backends.
//!
//! The backends report semantic-search scores with opposite semantics:
//! Elasticsearch scores read "higher is more relevant", while Weaviate reports
//! a cosine distance where lower is closer. [`RetrievalDatabase::normalize_score`]
//! is the single per-backend hook that reconciles this, so hits leaving any
//! implementation carry relevance in the same "higher is better" direction.

use std::sync::Arc;

use serde_json::Value;

use crate::error::Result;
use crate::ims_bot_adapter::models::message::PersistedMedia;
use crate::weaviate::{cosine_distance_relevance, gql_escape, WeaviateRef};

use crate::storage::{
    create_elasticsearch_memory_record, create_memory_record_with_vector,
    list_elasticsearch_memory_keys, list_recent_memory_keys, search_elasticsearch_images,
    search_elasticsearch_memory, search_memory_content_by_vector, upsert_elasticsearch_image,
    upsert_image_record, AgentMemoryAccessContext, AgentMemoryRecord, AgentMemorySearchHit,
    AgentMemoryUpsert, ElasticsearchRef, WeaviateClient,
};

/// One image search hit, independent of the retrieval backend.
pub struct RetrievalImageHit {
    pub object_id: String,
    /// Backend document properties (`_source` / Get-item fields).
    pub properties: Value,
    /// Relevance after [`RetrievalDatabase::normalize_score`]; `None` when the
    /// hit was not ranked by score (recent listings, keyword matches that the
    /// backend did not score).
    pub score: Option<f64>,
    /// Match kinds that produced this hit: `"keyword"` (bm25), `"semantic"`
    /// (vector) or `"recent"` (unranked listing). A hit found by several
    /// searches lists them all.
    pub match_kinds: Vec<&'static str>,
}

/// The operations a retrieval store serves for agent memory and image semantic
/// search, independent of the hosting backend.
pub trait RetrievalDatabase {
    /// Maps a raw vector-search score onto the relevance reported on hits.
    ///
    /// Only semantic (vector) search scores pass through here; keyword (bm25)
    /// scores are reported unchanged. The default returns the raw value, which
    /// suits Elasticsearch whose kNN `_score` already reads "higher is better";
    /// Weaviate overrides it to turn its cosine distance into `1 - distance`.
    fn normalize_score(&self, score: f64) -> f64 {
        score
    }

    /// Searches agent memory. `query` drives the lexical leg on backends that
    /// have one and is ignored by purely vector backends.
    fn search_memory(
        &self,
        access: &AgentMemoryAccessContext,
        query: &str,
        query_vector: &[f32],
        top_n: usize,
    ) -> Result<Vec<AgentMemorySearchHit>>;

    /// Lists memory records, most recently updated first, optionally filtered
    /// by `query` without ranking.
    fn list_memory(
        &self,
        access: &AgentMemoryAccessContext,
        top_n: usize,
        query: Option<&str>,
    ) -> Result<Vec<AgentMemorySearchHit>>;

    /// Creates a memory record, or updates the one carrying the same title.
    fn upsert_memory(
        &self,
        input: &AgentMemoryUpsert,
        vector: Vec<f32>,
    ) -> Result<AgentMemoryRecord>;

    /// Writes one image record with its description (and optional name) vectors.
    fn upsert_image(
        &self,
        media: &PersistedMedia,
        description_vector: &[f32],
        name_vector: Option<&[f32]>,
    ) -> Result<()>;

    /// Searches the image store: keyword matches for the text queries, semantic
    /// matches for the vectors, merged into one keyword-first ranked list.
    /// With no queries and no vectors, returns the most recent records.
    fn search_images(
        &self,
        name_query: Option<&str>,
        description_query: Option<&str>,
        name_vector: Option<&[f32]>,
        description_vector: Option<&[f32]>,
        limit: usize,
    ) -> Result<Vec<RetrievalImageHit>>;
}

impl<T: RetrievalDatabase + ?Sized> RetrievalDatabase for Arc<T> {
    fn normalize_score(&self, score: f64) -> f64 {
        (**self).normalize_score(score)
    }

    fn search_memory(
        &self,
        access: &AgentMemoryAccessContext,
        query: &str,
        query_vector: &[f32],
        top_n: usize,
    ) -> Result<Vec<AgentMemorySearchHit>> {
        (**self).search_memory(access, query, query_vector, top_n)
    }

    fn list_memory(
        &self,
        access: &AgentMemoryAccessContext,
        top_n: usize,
        query: Option<&str>,
    ) -> Result<Vec<AgentMemorySearchHit>> {
        (**self).list_memory(access, top_n, query)
    }

    fn upsert_memory(
        &self,
        input: &AgentMemoryUpsert,
        vector: Vec<f32>,
    ) -> Result<AgentMemoryRecord> {
        (**self).upsert_memory(input, vector)
    }

    fn upsert_image(
        &self,
        media: &PersistedMedia,
        description_vector: &[f32],
        name_vector: Option<&[f32]>,
    ) -> Result<()> {
        (**self).upsert_image(media, description_vector, name_vector)
    }

    fn search_images(
        &self,
        name_query: Option<&str>,
        description_query: Option<&str>,
        name_vector: Option<&[f32]>,
        description_vector: Option<&[f32]>,
        limit: usize,
    ) -> Result<Vec<RetrievalImageHit>> {
        (**self).search_images(
            name_query,
            description_query,
            name_vector,
            description_vector,
            limit,
        )
    }
}

impl RetrievalDatabase for WeaviateRef {
    fn normalize_score(&self, score: f64) -> f64 {
        cosine_distance_relevance(score)
    }

    fn search_memory(
        &self,
        access: &AgentMemoryAccessContext,
        query: &str,
        query_vector: &[f32],
        top_n: usize,
    ) -> Result<Vec<AgentMemorySearchHit>> {
        let _ = query; // Weaviate memory search ranks by vector only.
        let mut hits = search_memory_content_by_vector(self, access, query_vector, top_n)?;
        for hit in &mut hits {
            if let Some(distance) = hit.distance.take() {
                hit.distance = Some(self.normalize_score(distance));
            }
        }
        Ok(hits)
    }

    fn list_memory(
        &self,
        access: &AgentMemoryAccessContext,
        top_n: usize,
        query: Option<&str>,
    ) -> Result<Vec<AgentMemorySearchHit>> {
        list_recent_memory_keys(self, access, top_n, query)
    }

    fn upsert_memory(
        &self,
        input: &AgentMemoryUpsert,
        vector: Vec<f32>,
    ) -> Result<AgentMemoryRecord> {
        create_memory_record_with_vector(self, input, Some(vector))
    }

    fn upsert_image(
        &self,
        media: &PersistedMedia,
        description_vector: &[f32],
        name_vector: Option<&[f32]>,
    ) -> Result<()> {
        upsert_image_record(self, media, description_vector, name_vector).map(|_| ())
    }

    fn search_images(
        &self,
        name_query: Option<&str>,
        description_query: Option<&str>,
        name_vector: Option<&[f32]>,
        description_vector: Option<&[f32]>,
        limit: usize,
    ) -> Result<Vec<RetrievalImageHit>> {
        let properties = image_property_names();
        let mut hits = Vec::new();
        for (query, field) in [(name_query, "name"), (description_query, "description")] {
            if let Some(query) = query {
                let args = format!(
                    "bm25: {{ query: \"{}\", properties: [\"{field}\"] }}, limit: {limit}",
                    gql_escape(query)
                );
                if let Ok(response) =
                    self.query_with_args(&self.class_name, &args, &properties, true)
                {
                    merge_image_hits(
                        &mut hits,
                        weaviate_image_hits(self, response, "keyword", false),
                    );
                }
            }
        }
        for (vector, target) in
            [(name_vector, "name_vector"), (description_vector, "description_vector")]
        {
            if let Some(vector) = vector {
                if let Ok(response) = self.query_near_vector(
                    &self.class_name,
                    vector,
                    Some(target),
                    limit,
                    &properties,
                    true,
                    false,
                ) {
                    merge_image_hits(
                        &mut hits,
                        weaviate_image_hits(self, response, "semantic", true),
                    );
                }
            }
        }
        if name_query.is_none() && description_query.is_none() {
            if let Ok(response) = self.query_all(&self.class_name, limit, &properties) {
                merge_image_hits(&mut hits, weaviate_image_hits(self, response, "recent", false));
            }
        }
        Ok(hits)
    }
}

impl RetrievalDatabase for ElasticsearchRef {
    // `normalize_score`: default. Elasticsearch kNN `_score` already reads
    // "higher is better", so raw scores are reported unchanged.

    fn search_memory(
        &self,
        access: &AgentMemoryAccessContext,
        query: &str,
        query_vector: &[f32],
        top_n: usize,
    ) -> Result<Vec<AgentMemorySearchHit>> {
        search_elasticsearch_memory(self, access, query, query_vector, top_n)
    }

    fn list_memory(
        &self,
        access: &AgentMemoryAccessContext,
        top_n: usize,
        query: Option<&str>,
    ) -> Result<Vec<AgentMemorySearchHit>> {
        list_elasticsearch_memory_keys(self, access, top_n, query)
    }

    fn upsert_memory(
        &self,
        input: &AgentMemoryUpsert,
        vector: Vec<f32>,
    ) -> Result<AgentMemoryRecord> {
        create_elasticsearch_memory_record(self, input, vector)
    }

    fn upsert_image(
        &self,
        media: &PersistedMedia,
        description_vector: &[f32],
        name_vector: Option<&[f32]>,
    ) -> Result<()> {
        upsert_elasticsearch_image(
            self,
            media,
            description_vector.to_vec(),
            name_vector.map(<[f32]>::to_vec),
        )
    }

    fn search_images(
        &self,
        name_query: Option<&str>,
        description_query: Option<&str>,
        name_vector: Option<&[f32]>,
        description_vector: Option<&[f32]>,
        limit: usize,
    ) -> Result<Vec<RetrievalImageHit>> {
        Ok(search_elasticsearch_images(
            self,
            name_query,
            description_query,
            name_vector,
            description_vector,
            limit,
        )?
        .into_iter()
        .map(|hit| RetrievalImageHit {
            object_id: hit.object_id,
            properties: hit.properties,
            score: hit.score.map(|score| self.normalize_score(score)),
            match_kinds: vec![if hit.keyword_match {
                "keyword"
            } else {
                "semantic"
            }],
        })
        .collect())
    }
}

/// The Weaviate image properties every search must request.
fn image_property_names() -> Vec<String> {
    [
        "media_id",
        "original_source",
        "rustfs_path",
        "name",
        "description",
        "mime_type",
        "source",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

/// Parses a Weaviate image Get response into backend-agnostic hits.
///
/// Semantic (nearVector) responses carry a cosine distance, which is turned
/// into a normalized relevance; keyword responses carry a raw bm25 score, and
/// recent responses carry none.
fn weaviate_image_hits(
    weaviate: &WeaviateRef,
    response: Value,
    kind: &'static str,
    semantic: bool,
) -> Vec<RetrievalImageHit> {
    response
        .get("data")
        .and_then(|value| value.get("Get"))
        .and_then(Value::as_object)
        .and_then(|items| items.values().next())
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let extra = item.get("_additional")?;
            let object_id = extra.get("id").and_then(Value::as_str)?.to_string();
            let score = if semantic {
                extra
                    .get("distance")
                    .and_then(Value::as_f64)
                    .map(|distance| weaviate.normalize_score(distance))
            } else {
                extra.get("score").and_then(Value::as_f64)
            };
            let mut properties = item.clone();
            if let Some(object) = properties.as_object_mut() {
                object.remove("_additional");
            }
            Some(RetrievalImageHit {
                object_id,
                properties,
                score,
                match_kinds: vec![kind],
            })
        })
        .collect()
}

/// Merges per-query hit lists: hits found by several queries union their match
/// kinds and keep their best score; keyword matches sort first.
fn merge_image_hits(target: &mut Vec<RetrievalImageHit>, incoming: Vec<RetrievalImageHit>) {
    for item in incoming {
        match target.iter_mut().find(|current| current.object_id == item.object_id) {
            Some(existing) => {
                for kind in item.match_kinds {
                    if !existing.match_kinds.contains(&kind) {
                        existing.match_kinds.push(kind);
                    }
                }
                existing.score = match (existing.score, item.score) {
                    (Some(current), Some(incoming_score)) => Some(current.max(incoming_score)),
                    (Some(current), None) => Some(current),
                    (None, score) => score,
                };
            }
            None => target.push(item),
        }
    }
    target.sort_by_key(|item| !item.match_kinds.contains(&"keyword"));
}
