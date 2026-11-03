//! Typed shape of one turn's memory recall.
//!
//! The recall phase hands its result to the turn assembly as [`MemoryRecall`] instead of a
//! rendered string: each recalled memory stays in the class its category implies, so downstream
//! code renders a hint from structure rather than re-parsing JSON. A concept is a titled fact, a
//! person is identified by its QQ ID while names only hint at it, and a behavior is the short
//! rule itself.

use crate::storage::{AgentMemoryRecord, MemoryCategory};

/// Most memories one turn's hint lists, across all categories. The hint only says which
/// memories exist; their content stays behind `search_memory`.
pub const MAX_RECALLED_MEMORIES: usize = 20;

/// A fact/concept memory recalled for this turn. Its title names the fact.
#[derive(Debug, Clone)]
pub struct ConceptMemory {
    title: String,
}

impl ConceptMemory {
    pub fn title(&self) -> &str {
        &self.title
    }
}

/// A person memory recalled for this turn.
///
/// Distinct from the stored profile (`storage::PersonMemory`): this is the recall-level entry the
/// turn's hint needs. The QQ ID is the identity and the stored key is its `person-<qq_id>` form;
/// `name` and `preferred_name` are retrieval/display hints only and never prove two accounts are
/// the same person.
#[derive(Debug, Clone)]
pub struct PersonMemory {
    key: String,
    qq_id: String,
    name: String,
    preferred_name: String,
}

impl PersonMemory {
    pub fn qq_id(&self) -> &str {
        &self.qq_id
    }

    /// How the hint names this person: the name, with the stored key in parentheses so the
    /// model can tell people with colliding display names apart. Falls back to the key, then to
    /// the preferred name, so a sparse record never renders as an empty entry.
    pub fn display_title(&self) -> String {
        let key = self.key.trim();
        let name = self.name.trim();
        let name = (!name.is_empty() && name != key).then_some(name);
        match (name, key.is_empty()) {
            (Some(name), false) => format!("{name} ({key})"),
            (Some(name), true) => name.to_string(),
            (None, false) => key.to_string(),
            (None, true) => self.preferred_name.trim().to_string(),
        }
    }
}

/// A behavioral memory recalled for this turn. Its content is the short rule; the key is an
/// opaque hash and is never shown.
#[derive(Debug, Clone)]
pub struct BehavioralMemory {
    rule: String,
}

impl BehavioralMemory {
    pub fn rule(&self) -> &str {
        &self.rule
    }
}

/// One memory recalled for the turn, in the class its category implies.
#[derive(Debug, Clone)]
pub enum RecalledMemory {
    Concept(ConceptMemory),
    Person(PersonMemory),
    Behavioral(BehavioralMemory),
}

impl RecalledMemory {
    /// Reads one backend record as its search category's class.
    ///
    /// The category the search ran under is authoritative: older records do not always carry
    /// one, and a legacy record must not silently become a concept because of a missing field.
    pub(crate) fn from_record(category: MemoryCategory, record: &AgentMemoryRecord) -> Self {
        match category {
            MemoryCategory::Concept => {
                Self::Concept(ConceptMemory { title: record.key.trim().to_string() })
            }
            MemoryCategory::Person => Self::Person(PersonMemory {
                key: record.key.clone(),
                qq_id: record.attributes.person.qq_id.clone(),
                name: record.attributes.person.name.clone(),
                preferred_name: record.attributes.person.preferred_name.clone(),
            }),
            MemoryCategory::Behavioral => {
                Self::Behavioral(BehavioralMemory { rule: record.value.trim().to_string() })
            }
        }
    }
}

/// The memories one recall phase found for a turn, each in its category's class.
///
/// Built once by the recall phase and then carried read-only through the turn: the before-brain
/// assembly renders it into the user message's hint, and mid-turn steer messages render the same
/// recall so they stay consistent with the turn they interrupt. Entries beyond
/// [`MAX_RECALLED_MEMORIES`] are dropped so the hint stays a fixed-size signal.
///
/// Pushing the same memory twice is a no-op: a forced re-search re-emits records the recall agent
/// already saw, and the hint lists each memory once.
#[derive(Debug, Clone, Default)]
pub struct MemoryRecall {
    /// The authenticated source the recall ran for (sender QQ ID and access level), shown with
    /// the titles so the model knows whose memories these are.
    source: Option<String>,
    concepts: Vec<ConceptMemory>,
    people: Vec<PersonMemory>,
    behaviors: Vec<BehavioralMemory>,
    /// The lines the hint would render, used to reject duplicates without keeping the records.
    seen: std::collections::HashSet<String>,
}

impl MemoryRecall {
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn len(&self) -> usize {
        self.concepts.len() + self.people.len() + self.behaviors.len()
    }

    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    pub fn concepts(&self) -> &[ConceptMemory] {
        &self.concepts
    }

    pub fn people(&self) -> &[PersonMemory] {
        &self.people
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
            RecalledMemory::Concept(memory) => format!("[concept] {}", memory.title()),
            RecalledMemory::Person(memory) => format!("[person] {}", memory.display_title()),
            RecalledMemory::Behavioral(memory) => format!("[behavioral] {}", memory.rule()),
        };
        if !self.seen.insert(entry) || self.len() >= MAX_RECALLED_MEMORIES {
            return;
        }
        match memory {
            RecalledMemory::Concept(memory) => self.concepts.push(memory),
            RecalledMemory::Person(memory) => self.people.push(memory),
            RecalledMemory::Behavioral(memory) => self.behaviors.push(memory),
        }
    }
}
