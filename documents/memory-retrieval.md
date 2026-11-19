# Algorithm

1. Lexical recall
2. embedding recall
3. Remove low relevance rows
4. RRF
5. Full Name matching first
6. Topk


Memory has 3 category `MemoryCategory`

```rust
/// One memory record in the class its category implies. The category tag doubles as the
/// storage discriminator; each variant serializes only its own fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "category", rename_all = "snake_case")]
pub enum MemoryRecord {
    Concept(ConceptMemoryRecord),
    Person(PersonMemoryRecord),
    Behavioral(BehavioralMemoryRecord),
}
```

Concept：Fact/concept that agent should follow（key + content）
Person：Personal profile, key is a person's user_id（name/preferred_name/aliases/impression）
Behavioral：agent's behaviour (likes skill), behavioural can expires, but if a behavioural has been queried many times, it becomes live long （key 是哈希、content 是规则、带 expires_at）




## Concept

### elasticsearch index

```json
{
  "settings": {
    "analysis": {
      "analyzer": {
        "pinyin_analyzer": { "tokenizer": "standard", "filter": ["lowercase", "pinyin"] }
      }
    }
  },
  "mappings": {
    "properties": {
      "category":       { "type": "keyword" },
      "access_level":   { "type": "integer" },
      "starts_at":      { "type": "date" },
      "key":            { "type": "text", "analyzer": "icu_analyzer",
                          "fields": { "pinyin": { "type": "text", "analyzer": "pinyin_analyzer",
                                                  "search_analyzer": "pinyin_analyzer" } } },
      "value":          { "type": "text", "analyzer": "icu_analyzer",
                          "fields": { "pinyin": { "type": "text", "analyzer": "pinyin_analyzer",
                                                  "search_analyzer": "pinyin_analyzer" } } },
      "embedding":      { "type": "dense_vector", "dims": "<连接配置的向量维度>", "index": true, "similarity": "cosine" },
      "sender_id_list": { "type": "keyword" },
      "group_id_list":  { "type": "keyword" },
      "created_at":     { "type": "date" },
      "updated_at":     { "type": "date" }
    }
  }
}
```

### Data Structure

```rust
#[derive(Debug, Clone)]
pub struct ConceptMemoryWrite {          // 写入参数
    pub key: String,                     // Concept标题（也是检索主词）
    pub content: String,
    pub access_level: u32,
    pub starts_at: Option<String>,       // RFC3339，缺省写入时取 now
    pub sender_id_list: Vec<String>,     // 作用域：允许的发送者
    pub group_id_list: Vec<String>,      // 作用域：允许的群
}

#[derive(Debug, Clone)]
pub struct ConceptMemoryRecord {         // 读出记录
    pub key: String,
    pub content: String,
    pub access_level: u32,
    pub starts_at: Option<String>,
    pub sender_id_list: Vec<String>,
    pub group_id_list: Vec<String>,
    pub object_id: String,
    pub created_at: String,
    pub updated_at: String,
}
```

## Behaviour

### elasticsearch index

```json
{
  "settings": {
    "analysis": { "analyzer": { "pinyin_analyzer": { "tokenizer": "standard", "filter": ["lowercase", "pinyin"] } } }
  },
  "mappings": {
    "properties": {
      "access_level":   { "type": "integer" },
      "key":            { "type": "text", "analyzer": "icu_analyzer", "fields": { "pinyin": { "type": "text", "analyzer": "pinyin_analyzer" } } },
      "content":        { "type": "text", "analyzer": "icu_analyzer", "fields": { "pinyin": { "type": "text", "analyzer": "pinyin_analyzer" } } },
      "embedding":      { "type": "dense_vector", "dims": <配置的向量维度>, "index": true, "similarity": "cosine" },
      "expires_at":     { "type": "date" },
      "sender_id_list": { "type": "keyword" },
      "group_id_list":  { "type": "keyword" },
      "created_at":     { "type": "date" },
      "updated_at":     { "type": "date" }
    }
  }
}
```

### Data Structure

```rust
pub struct BehavioralMemoryRecord {
    pub key: String,
    pub content: String,
    pub access_level: u32,          // 默认 default_access_level
    pub expires_at: String,         // RFC3339
    pub sender_id_list: Vec<String>,
    pub group_id_list: Vec<String>,
    pub object_id: String,
    pub created_at: String,
    pub updated_at: String,
}
```

## Person

### elasticsearch index

```json
{
  "access_level": {"type": "integer"},
  "qq_id":         {"type": "keyword"},
  "name":           text,          // {"type":"text","analyzer":"icu_analyzer","fields":{"pinyin":{...}}}
  "preferred_name": text,
  "aliases":        text,
  "impression":     text,
  "guideline":      text,
  "embedding":      {"type":"dense_vector","dims":配置维度,"index":true,"similarity":"cosine"},
  "created_at":    {"type": "date"},
  "updated_at":    {"type": "date"}
}
```

### data structure

```rust
/// QQ ID is the identity. Names and aliases are retrieval hints only.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PersonMemory {
    pub qq_id: String,
    pub name: String,
    pub preferred_name: String,
    pub aliases: Vec<String>,
    pub impression: String,
    pub guideline: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonMemoryRecord {
    #[serde(flatten)]
    pub profile: PersonMemory,
    #[serde(default = "default_access_level")]
    pub access_level: u32,
    pub object_id: String,   // 由 memory_object_id(Person, qq_id, …) 哈希生成
    pub created_at: String,
    pub updated_at: String,
}

pub struct PersonMemoryWrite {
    pub profile: PersonMemory,
    pub access_level: u32,
}
```


# search / query memory


# Turn recall

1. **Identity.** `get_person(id)` for the sender and `ROLE_SELF` (the bot) returns their profiles -> `[QQ Identity Memories]` context.
   - Skip empty ids; when sender == bot, read once (dedup by id).
   - Read the profiles from the store, then compare with the `[QQ Identity Memories]` message already in the history (`saved_identity_context`):
     ```
     inject = saved_identity_context != fresh_profiles
     ```
     identical -> do not inject again; different or absent (new session, history compacted) -> inject the fresh profiles.
2. **Recall passes.** The turn input is the user's message only (no scope prefix, no profile JSON). When step 1 found the sender's person memory, its name, preferred name and aliases are the identity terms:
   ```
   identity_terms = [name, preferred_name] + aliases     (empty and duplicate ones dropped)

   pass 0      source = turn_input,  queries = [turn_input] + up to 4 clause segments
   pass i      source = identity_term_i,  queries = [identity_term_i]
   ```
   - Terms are never merged into the message's query: each pass is an independent recall, and all passes run concurrently.
   - Segments are split on `，。！？；、`, newline, `,.!?;`; only 4–64 chars are kept, deduplicated. Short segments match individual keys and rules better than a long message.
   - A rule or concept filed under the sender's nickname is recalled even when the message does not mention it.
   - If the sender has no accessible person memory, there are no identity terms and only pass 0 runs.
   - The bot's own profile is only injected into the context; it is not used for recall.
3. **Per pass, per category** (`Behavioral` limit 5, `Concept` limit 20: concepts are recalled as titles only, so they cost little context), all filtered by the `access` context (access level, `sender_id_list`, `group_id_list`, `expires_at`/`starts_at`):
   - Hybrid search once per query of the pass: lexical (`key`/`content`, icu + pinyin) + embedding recall, low-relevance rows removed (see [Algorithm](#algorithm)).
   - Key scan: list up to `KEY_SCAN_WINDOW` records and match each key against the pass's source text directly, so a key-only hit is recalled even if the joint ranking misses it. The key score is:
     ```
     key_score = 1.0                       if key is contained in source
     key_score = ratio * 0.5               if ratio >= 0.5
     key_score = no match                  otherwise

     ratio = (key terms found in source) / (all key terms)
     ```
     Key terms are latin/digit word tokens and CJK bigrams.
   - The queries' rankings and the key scan fuse (RRF) into one ranking for the pass.
4. **Merge passes.** RRF over the passes' rankings, then sort, then truncate.
   - Each ranking is best-first. A record's fused score is the sum over every ranking that returned it (`rank` starts at 1, a duplicate inside one ranking counts once at its first position):
     ```
     rrf_score(record) = sum over rankings of 1 / (k + rank)

     k = 60    (DEFAULT_RRF_RANK_CONSTANT)
     ```
   - Only the position matters, so lexical, vector and key-scan rankings with incomparable raw scores combine without normalization; a record hit by several passes or queries rises.
   - Sort order, each rule only breaks the ties of the previous one:
     ```
     1. key == turn_input (trimmed, lowercase)    exact match first
     2. rrf_score                                 descending
     3. updated_at                                newest first
     4. object_id                                 stable order
     ```
   - Truncate to the category limit.
5. **De-duplicate against history.** Drop a record when:
   ```
   previous_context.contains(memory_fingerprint(record))
   ```
   so the hint never repeats what the model can still see. A group-window turn replaces the sender history, so it uses an empty previous context and recalls everything again.
6. **Render.** Behaviors first, then concepts, with the total capped:
   ```
   total <= MAX_RECALLED_MEMORIES    (25 = 5 + 20)
   ```
   - `Behavioral`: key + full rule `content`.
   - `Concept`: key only. The agent fetches details with `search_memory` when needed.
   - The hint is tagged with the authenticated source (sender QQ id + access level).

The recall is rerun with the same `resources` and `input` when the brain compacts the turn history, so the hint stays consistent with the turn it interrupts.
