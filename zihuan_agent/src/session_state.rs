use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use zihuan_core::agent_config::QqChatEmotionDimensionConfig;
use zihuan_core::error::{Error, Result};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EmotionAdjustmentDirection {
    Increase,
    Decrease,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QqChatAgentSessionState {
    #[serde(default)]
    pub emotion_dimensions: HashMap<String, f64>,
    #[serde(default)]
    pub extra_state: HashMap<String, Value>,
}

impl QqChatAgentSessionState {
    pub fn sync_emotion_dimensions(&mut self, dimensions: &[QqChatEmotionDimensionConfig]) {
        let mut allowed_names = Vec::with_capacity(dimensions.len());
        for dimension in dimensions {
            let name = dimension.name.trim();
            if name.is_empty() || allowed_names.iter().any(|existing| existing == &name) {
                continue;
            }
            allowed_names.push(name.to_string());
            self.emotion_dimensions
                .entry(name.to_string())
                .or_insert(0.0);
        }
        self.emotion_dimensions
            .retain(|name, _| allowed_names.iter().any(|allowed| allowed == name));
    }

    pub fn apply_emotion_adjustment(
        &mut self,
        dimensions: &[QqChatEmotionDimensionConfig],
        dimension_name: &str,
        direction: EmotionAdjustmentDirection,
    ) -> Result<f64> {
        self.sync_emotion_dimensions(dimensions);

        let normalized_name = dimension_name.trim();
        let Some(dimension) = dimensions
            .iter()
            .find(|item| item.name.trim() == normalized_name)
        else {
            return Err(Error::ValidationError(format!(
                "unsupported emotion dimension '{}'",
                dimension_name
            )));
        };

        let weight = match direction {
            EmotionAdjustmentDirection::Increase => dimension.increase_weight,
            EmotionAdjustmentDirection::Decrease => dimension.decrease_weight,
        };
        let delta = if matches!(direction, EmotionAdjustmentDirection::Increase) {
            weight
        } else {
            -weight
        };
        let entry = self
            .emotion_dimensions
            .entry(dimension.name.trim().to_string())
            .or_insert(0.0);
        *entry += delta;
        Ok(*entry)
    }

    pub fn ordered_emotion_dimensions(
        &self,
        dimensions: &[QqChatEmotionDimensionConfig],
    ) -> Vec<(String, f64)> {
        dimensions
            .iter()
            .filter_map(|dimension| {
                let name = dimension.name.trim();
                if name.is_empty() {
                    return None;
                }
                Some((
                    name.to_string(),
                    *self.emotion_dimensions.get(name).unwrap_or(&0.0),
                ))
            })
            .collect()
    }
}
