# Model Configuration JSON

## Top-level Fields

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | string | Yes | Display name of the configuration. Cannot be empty. |
| `enabled` | boolean | Yes | Whether this model configuration is enabled. |
| `model` | object | Yes | Model definition. Its structure depends on `model.type`. |

## Chat Models

### `model.llm` Fields

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `model_name` | string | Yes | Provider model name. In Candle mode, this is the local model directory name. |
| `api_endpoint` | string | API modes only | API base URL. Use an empty string for Candle local inference. |
| `api_key` | string \| null | No | API key. Use `null` for Candle local inference. |
| `api_style` | string | Yes | Request protocol. See the table below. |
| `stream` | boolean | Yes | Whether streaming output is requested by default. |
| `supports_multimodal_input` | boolean | Yes | Whether image input is allowed. In Candle mode, this is determined by the local model's capability. |
| `include_reasoning_content` | boolean | Yes | Whether to feed `reasoning_content` back into reasoning requests. |
| `thinking_type` | `"enabled"` \| `"disabled"` \| null | No | Thinking mode. `null` does not set an explicit value. |
| `reasoning_effort` | `"low"` \| `"medium"` \| `"high"` \| `"max"` \| null | No | Thinking effort. `null` does not set an explicit value. |
| `timeout_secs` | number | Yes | Timeout per request in seconds. Must be greater than 0. |
| `retry_count` | number | Yes | Number of retries after a failed request. Must be greater than or equal to 0. |

Supported `api_style` values:

| Value | Description |
| --- | --- |
| `candle_gguf` | Candle GGUF local inference. |
| `candle_hf` | Candle HF local inference. |
| `open_ai_chat_completions` | OpenAI Chat Completions API. |
| `open_ai_chat_completions_tencent_multimodal_compat` | Tencent multimodal-compatible Chat Completions API. |
| `open_ai_responses` | OpenAI Responses API. |
| `open_ai_responses_message_compat` | `message`-compatible Responses API. |
| `open_ai_responses_image_url_object_compat` | `image_url` object-compatible Responses API. |

```json
{
  "name": "紫幻模型",
  "enabled": true,
  "model": {
    "type": "chat_llm",
    "llm": {
      "model_name": "zihuan-0.1.5",
      "api_endpoint": "https://api.example.com/v1",
      "api_key": "sk-...",
      "api_style": "open_ai_chat_completions",
      "stream": false,
      "supports_multimodal_input": false,
      "include_reasoning_content": false,
      "thinking_type": null,
      "reasoning_effort": null,
      "timeout_secs": 30,
      "retry_count": 2
    }
  }
}
```

## Text Embedding Models

Text embedding models only need the name of a model placed in the local model directory.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `model.type` | Fixed as `"text_embedding_local"` | Yes | Identifies a local text embedding model. |
| `model.model_name` | string | Yes | Local text embedding model directory name. Cannot be empty. |

```json
{
  "name": "Text Embedding Model",
  "enabled": true,
  "model": {
    "type": "text_embedding_local",
    "model_name": "bge-m3"
  }
}
```