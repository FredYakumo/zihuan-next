use std::time::Duration;


#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeaviateCollectionSchema {
    MessageRecordSemantic,
    ImageSemantic,
}

#[derive(Clone)]
pub struct WeaviateRef {
    pub base_url: String,
    pub class_name: String,
    pub api_key: Option<String>,
    pub timeout: Duration,
    client: Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaviateCollectionConfig {
    #[serde(rename = "class")]
    pub class_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub properties: Vec<WeaviatePropertyConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vectorizer: Option<String>,
}

