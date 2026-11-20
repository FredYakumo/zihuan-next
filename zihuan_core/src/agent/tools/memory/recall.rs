//! The pure-Rust recall phase and the typed shape of one turn's memory recall.
//!
//! [`recall_memory`] runs without any LLM: the turn input (plus clause segments of it) is
//! searched against the behavioural and concept stores with the backend's hybrid retrieval,
//! and a separate key scan matches record keys against the input directly so a hit on the key
//! alone is still recalled. Each recalled memory stays in the class its category implies, so
//! downstream code renders a hint from structure rather than re-parsing JSON: a concept is
//! recalled by key only (the agent fetches details through `search_memory`), and a behavior is
//! recalled with both its key and its full rule text.

use crate::agent::tools::memory::resources::MemoryBackend;
use crate::error::Result;
use crate::model_inference::llm::embedding_base::EmbeddingBase;
use crate::storage::memory::{fuse_memory_hits, sort_memory_search_hits};
use crate::storage::{AgentMemoryAccessContext, MemoryCategory, MemoryRecord, MemorySearchHit};
use crate::utils::memory_fingerprint::memory_fingerprint;

use super::resources::MemoryAgentResources;

/// Most memories one turn's hint lists, across all categories.
pub const MAX_RECALLED_MEMORIES: usize = 25;

/// Behavioural rules one recall phase returns at most.
const BEHAVIORAL_RECALL_LIMIT: usize = 5;
/// Concept keys one recall phase returns at most.
const CONCEPT_RECALL_LIMIT: usize = 20;
/// Clause segments searched on top of the full input, to lift lexical and vector
/// hit rate on long messages.
const MAX_SEGMENT_QUERIES: usize = 4;
/// Candidate window the key scan lists before matching keys against the input.
const KEY_SCAN_WINDOW: usize = 100;

/// A fact/concept memory recalled for this turn: its key only, with the stored content left
/// for the agent to fetch through `search_memory` when it decides it needs the details.
#[derive(Debug, Clone)]
pub struct ConceptMemory {
    key: String,
}

impl ConceptMemory {
    pub fn key(&self) -> &str {
        &self.key
    }
}

/// A behavioral memory recalled for this turn: the key names the rule and the content is the
/// short rule itself, both shown so the hint reads as a complete rule.
#[derive(Debug, Clone)]
pub struct BehavioralMemory {
    key: String,
    content: String,
}

impl BehavioralMemory {
    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn content(&self) -> &str {
        &self.content
    }
}

/// One memory recalled for the turn, in the class its category implies.
#[derive(Debug, Clone)]
pub enum RecalledMemory {
    Concept(ConceptMemory),
    Behavioral(BehavioralMemory),
}

/// The memories one recall phase found for a turn, each in its category's class.
///
/// Built once by the recall phase and then carried read-only through the turn: the before-brain
/// assembly renders it into the user message's hint, and mid-turn steer messages render the same
/// recall so they stay consistent with the turn they interrupt. Entries beyond
/// [`MAX_RECALLED_MEMORIES`] are dropped so the hint stays a fixed-size signal.
///
/// Pushing the same memory twice is a no-op: a repeated query pass re-emits records an earlier
/// pass already accepted, and the hint lists each memory once.
#[derive(Debug, Clone, Default)]
pub struct MemoryRecall {
    /// The authenticated source the recall ran for (sender QQ ID and access level), shown with
    /// the keys so the model knows whose memories these are.
    source: Option<String>,
    concepts: Vec<ConceptMemory>,
    behaviors: Vec<BehavioralMemory>,
    /// The lines the hint would render, used to reject duplicates without keeping the records.
    seen: std::collections::HashSet<String>,
}

impl MemoryRecall {
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn len(&self) -> usize {
        self.concepts.len() + self.behaviors.len()
    }

    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    pub fn concepts(&self) -> &[ConceptMemory] {
        &self.concepts
    }

    pub fn behaviors(&self) -> &[BehavioralMemory] {
        &self.behaviors
    }

    /// Notes the authenticated source this recall ran for.
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// Adds one recalled memory while the hint still has room.
    ///
    /// Duplicates are rejected by the entry the hint would render, not by record identity: two
    /// records that render the same line are the same signal to the model.
    pub(crate) fn push(&mut self, memory: RecalledMemory) {
        let entry = match &memory {
            RecalledMemory::Concept(memory) => format!("[concept] {}", memory.key()),
            RecalledMemory::Behavioral(memory) => {
                format!("[behavioral] {}\n{}", memory.key(), memory.content())
            }
        };
        if !self.seen.insert(entry) || self.len() >= MAX_RECALLED_MEMORIES {
            return;
        }
        match memory {
            RecalledMemory::Concept(memory) => self.concepts.push(memory),
            RecalledMemory::Behavioral(memory) => self.behaviors.push(memory),
        }
    }
}

/// The recall phase: behavioural and concept memories for one turn, found without any LLM.
///
/// Every query pass runs the backend's hybrid retrieval (the behavioural embedding text and the
/// lexical fields cover both the key and the content), and an additional key scan matches record
/// keys against the input directly. The rankings fuse by rank so a record found by several
/// queries rises, and records whose fingerprint already appears in the previous conversation are
/// dropped so the hint never repeats what the model can still see.
///
/// `identity_terms` are the sender's name, preferred name and aliases when the sender's person
/// memory was found. They are never merged into the message's query: the message and each term
/// run as separate recall passes concurrently (each its own hybrid search and key scan), and the
/// passes' rankings are fused afterwards, so a rule filed under a nickname is recalled even when
/// the nickname is not in the message.
pub fn recall_memory(
    resources: MemoryAgentResources,
    input: &str,
    previous_context: String,
    identity_terms: &[String],
) -> Result<MemoryRecall> {
    let mut recall = MemoryRecall::default();
    let input = input.trim();
    if input.is_empty() {
        return Ok(recall);
    }
    let model = resources.embedding_model.as_deref();
    let access = resources.access.clone();
    // One pass per source text: the message (with its clause segments) and each distinct term.
    let mut passes = vec![(input.to_string(), recall_queries(input))];
    for term in identity_terms {
        let term = term.trim();
        if !term.is_empty() && !passes.iter().any(|(text, _)| text == term) {
            passes.push((term.to_string(), vec![term.to_string()]));
        }
    }

    let behaviors = recall_category(
        &resources.memory_backend,
        model,
        &access,
        MemoryCategory::Behavioral,
        &passes,
        input,
        previous_context.as_str(),
        BEHAVIORAL_RECALL_LIMIT,
    )?;
    for hit in behaviors {
        if let MemoryRecord::Behavioral(record) = hit.record {
            recall.push(RecalledMemory::Behavioral(BehavioralMemory {
                key: record.key.trim().to_string(),
                content: record.content.trim().to_string(),
            }));
        }
    }

    let concepts = recall_category(
        &resources.memory_backend,
        model,
        &access,
        MemoryCategory::Concept,
        &passes,
        input,
        previous_context.as_str(),
        CONCEPT_RECALL_LIMIT,
    )?;
    for hit in concepts {
        if let MemoryRecord::Concept(record) = hit.record {
            recall.push(RecalledMemory::Concept(ConceptMemory {
                key: record.key.trim().to_string(),
            }));
        }
    }
    Ok(recall)
}

/// Recalls one category: every pass runs concurrently, then the passes' rankings fuse and the
/// previous-context filter applies.
#[allow(clippy::too_many_arguments)]
fn recall_category(
    backend: &MemoryBackend,
    model: Option<&dyn EmbeddingBase>,
    access: &AgentMemoryAccessContext,
    category: MemoryCategory,
    passes: &[(String, Vec<String>)],
    input: &str,
    previous_context: &str,
    limit: usize,
) -> Result<Vec<MemorySearchHit>> {
    if category == MemoryCategory::Person {
        // Person recall is the exact identity read in the turn's identity context.
        return Ok(Vec::new());
    }
    let pass_results = std::thread::scope(|scope| {
        let handles = passes
            .iter()
            .map(|(text, queries)| {
                scope.spawn(move || {
                    recall_pass(backend, model, access, category, queries, text, limit)
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|_| Err(crate::string_error!("memory recall pass panicked")))
            })
            .collect::<Vec<_>>()
    });
    let mut rankings = Vec::new();
    for result in pass_results {
        let ranking = result?;
        if !ranking.is_empty() {
            rankings.push(ranking);
        }
    }
    if rankings.is_empty() {
        return Ok(Vec::new());
    }
    let mut fused = fuse_memory_hits(&rankings);
    sort_memory_search_hits(input, &mut fused);
    fused.truncate(limit);
    fused.retain(|hit| match memory_fingerprint(&hit.record) {
        Ok(fingerprint) => !previous_context.contains(fingerprint.as_str()),
        // A record that cannot fingerprint is still recalled.
        Err(_) => true,
    });
    Ok(fused)
}

/// One recall pass for one source text: hybrid search per query plus the key scan against the
/// text, fused into a single ranking.
fn recall_pass(
    backend: &MemoryBackend,
    model: Option<&dyn EmbeddingBase>,
    access: &AgentMemoryAccessContext,
    category: MemoryCategory,
    queries: &[String],
    text: &str,
    limit: usize,
) -> Result<Vec<MemorySearchHit>> {
    let mut rankings = Vec::new();
    for query in queries {
        let hits: Vec<MemorySearchHit> = match category {
            MemoryCategory::Behavioral => backend
                .search_behavioral(model, access, Some(query), limit)?
                .into_iter()
                .map(|(record, distance)| MemorySearchHit::new(record, distance))
                .collect(),
            MemoryCategory::Concept => backend
                .search_concept(model, access, Some(query), limit)?
                .into_iter()
                .map(|(record, distance)| MemorySearchHit::new(record, distance))
                .collect(),
            MemoryCategory::Person => continue,
        };
        if !hits.is_empty() {
            rankings.push(hits);
        }
    }
    if let Some(key_hits) = key_scan(backend, access, category, text, limit)? {
        rankings.push(key_hits);
    }
    if rankings.is_empty() {
        return Ok(Vec::new());
    }
    Ok(fuse_memory_hits(&rankings))
}

/// The queries one recall runs: the full input first, then up to
/// [`MAX_SEGMENT_QUERIES`] clause segments. Long inputs dilute lexical and vector
/// matching, while short phrases match individual keys and rules much better.
fn recall_queries(input: &str) -> Vec<String> {
    let mut queries = vec![input.to_string()];
    let mut seen_queries = std::collections::HashSet::from([input.to_string()]);
    let mut segments = Vec::new();
    for segment in input.split(|c: char| {
        matches!(
            c,
            '，' | '。' | '！' | '？' | '；' | '、' | '\n' | '\r' | ',' | '.' | '!' | '?' | ';'
        )
    }) {
        if segments.len() >= MAX_SEGMENT_QUERIES {
            break;
        }
        let segment = segment.trim();
        let characters = segment.chars().count();
        if (4..=64).contains(&characters) && seen_queries.insert(segment.to_string()) {
            segments.push(segment.to_string());
        }
    }
    queries.append(&mut segments);
    queries
}

/// Matches record keys against the input directly, so a hit on the key alone is
/// recalled even when the backend's joint key+content ranking misses it.
///
/// Returns the matched records ranked strongest first, or `None` when the scan found nothing.
fn key_scan(
    backend: &MemoryBackend,
    access: &AgentMemoryAccessContext,
    category: MemoryCategory,
    input: &str,
    limit: usize,
) -> Result<Option<Vec<MemorySearchHit>>> {
    let records = backend.list_records(access, category, KEY_SCAN_WINDOW)?;
    let mut scored = records
        .into_iter()
        .filter_map(|hit| {
            let key = match &hit.record {
                MemoryRecord::Behavioral(record) => record.key.as_str(),
                MemoryRecord::Concept(record) => record.key.as_str(),
                MemoryRecord::Person(_) => return None,
            };
            key_match_score(key, input).map(|score| (score, hit))
        })
        .collect::<Vec<_>>();
    if scored.is_empty() {
        return Ok(None);
    }
    scored.sort_by(|(left, _), (right, _)| right.total_cmp(left));
    Ok(Some(scored.into_iter().take(limit).map(|(_, hit)| hit).collect()))
}

/// Scores how strongly a memory key matches the turn input without any model.
///
/// A key contained in the input matches exactly. Otherwise the key is split into matchable
/// terms — whole word tokens for latin/digit runs, character bigrams for CJK runs — and the
/// share of terms found in the input grades the match. `None` when it does not match.
fn key_match_score(key: &str, input: &str) -> Option<f64> {
    let key = key.trim().to_lowercase();
    let input = input.to_lowercase();
    if key.is_empty() || input.is_empty() {
        return None;
    }
    if input.contains(key.as_str()) {
        return Some(1.0);
    }
    let terms = key_terms(&key);
    if terms.is_empty() {
        return None;
    }
    let hits = terms.iter().filter(|term| input.contains(term.as_str())).count();
    let ratio = hits as f64 / terms.len() as f64;
    (ratio >= 0.5).then_some(ratio * 0.5)
}

/// Splits a key into matchable terms: whole word tokens for latin/digit runs, character
/// bigrams for runs containing CJK characters (a single character stays as-is).
fn key_terms(key: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut run = String::new();
    let flush = |terms: &mut Vec<String>, run: &mut String| {
        let characters: Vec<char> = run.chars().collect();
        let contains_cjk = characters.iter().any(|c| is_cjk(*c));
        if contains_cjk {
            if characters.len() > 1 {
                for pair in characters.windows(2) {
                    terms.push(pair.iter().collect());
                }
            } else if characters.len() == 1 {
                terms.push(run.clone());
            }
        } else if characters.len() >= 2 {
            terms.push(run.clone());
        }
        run.clear();
    };
    for character in key.chars() {
        if character.is_alphanumeric() {
            run.push(character);
        } else {
            flush(&mut terms, &mut run);
        }
    }
    flush(&mut terms, &mut run);
    terms
}

/// Whether the character is a CJK ideograph, the range memory keys are written in.
fn is_cjk(character: char) -> bool {
    matches!(character, '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}')
}
