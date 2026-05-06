use crate::data_value::ImageData;
use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use zihuan_core::ims_bot_adapter::models::message::Message;
use zihuan_core::error::{Error, Result};

/// Converts a QQMessage input into ImageData when the message variant is Image.
pub struct QQMessageToImageNode {
    id: String,
    name: String,
}

impl QQMessageToImageNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for QQMessageToImageNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("将 QQMessage(Image) 转换为 Image 数据类型，附带对象存储路径")
    }

    node_input![port! { name = "qq_message", ty = QQMessage, desc = "输入 QQ 消息段，必须是 image 类型" },];

    node_output![
        port! { name = "image", ty = Image, desc = "输出 Image 数据（metadata + object_storage_path）" },
        port! { name = "object_storage_path", ty = String, desc = "解析出的对象存储路径（优先 object_key）" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let qq_message = inputs
            .get("qq_message")
            .and_then(|value| match value {
                DataValue::QQMessage(message) => Some(message.clone()),
                _ => None,
            })
            .ok_or_else(|| Error::InvalidNodeInput("qq_message is required".to_string()))?;

        let image = match qq_message {
            Message::Image(image) => image,
            _ => {
                return Err(Error::ValidationError(
                    "qq_message must be image variant".to_string(),
                ))
            }
        };

        let object_storage_path = image
            .object_key
            .clone()
            .or_else(|| image.object_url.clone())
            .or_else(|| image.path.clone())
            .or_else(|| image.url.clone())
            .or_else(|| image.file.clone())
            .ok_or_else(|| {
                Error::ValidationError("image has no resolvable object_storage_path".to_string())
            })?;

        let image_value = ImageData {
            metadata: image,
            binary: None,
            object_storage_path: Some(object_storage_path.clone()),
        };

        let mut outputs = HashMap::new();
        outputs.insert("image".to_string(), DataValue::Image(image_value));
        outputs.insert(
            "object_storage_path".to_string(),
            DataValue::String(object_storage_path),
        );

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
