# Model Configuration JSON

## Top-level Fields

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | string | Yes | Display name of the configuration. Cannot be empty. |
| `enabled` | boolean | Yes | Whether this model configuration is enabled. |
| `model` | object | Yes | Model definition. Its structure depends on `model.type`. |

## Chat Models

Chat models come in two variants: `chat_llm_local` (executed locally) and
`chat_llm_api` (remote OpenAI-compatible API). Both carry the same `model.llm`
payload; the variant selects how outputs are produced.

> **Backward compatibility:** the pre-split `"type": "chat_llm"` tag (and the
> even older bare-`llm` encoding without a `type`) still load. They are
> dispatched by the recorded `api_style`: `candle_gguf`/`candle` →
> `chat_llm_local` with `weight_style: "gguf"`, `candle_hf` →
> `chat_llm_local` with `weight_style: "hf"`, `mock` → `chat_llm_local` with
> `weight_style: "mock"`, anything else → `chat_llm_api`.

### `model.llm` Fields

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `model_name` | string | Yes | Provider model name. For local models, this is the local model directory name. |
| `api_endpoint` | string | API mode only | API base URL. Empty for local models, except `weight_style: "mock"` where it holds the recording directory. |
| `api_key` | string \| null | No | API key. `null` for local models. |
| `api_style` | string | Yes | Request protocol. See the table below. Ignored for local models (stored as `open_ai_chat_completions`). |
| `weight_style` | `"mock"` \| `"gguf"` \| `"hf"` \| null | `chat_llm_local` only | How a local model produces outputs. `null` for API models. |
| `stream` | boolean | Yes | Whether streaming output is requested by default. |
| `supports_multimodal_input` | boolean | Yes | Whether image input is allowed. For local models, this is determined by the local model's capability. |
| `include_reasoning_content` | boolean | Yes | Whether to feed `reasoning_content` back into reasoning requests. |
| `thinking_type` | `"enabled"` \| `"disabled"` \| null | No | Thinking mode. `null` does not set an explicit value. |
| `reasoning_effort` | `"low"` \| `"medium"` \| `"high"` \| `"max"` \| null | No | Thinking effort. `null` does not set an explicit value. |
| `timeout_secs` | number | Yes | Timeout per request in seconds. Must be greater than 0. |
| `retry_count` | number | Yes | Number of retries after a failed request. Must be greater than or equal to 0. |

Supported `api_style` values (API models only):

| Value | Description |
| --- | --- |
| `open_ai_chat_completions` | OpenAI Chat Completions API. |
| `open_ai_chat_completions_tencent_multimodal_compat` | Tencent multimodal-compatible Chat Completions API. |
| `open_ai_responses` | OpenAI Responses API. |
| `open_ai_responses_message_compat` | `message`-compatible Responses API. |
| `open_ai_responses_image_url_object_compat` | `image_url` object-compatible Responses API. |

Supported `weight_style` values (local models only):

| Value | Description |
| --- | --- |
| `mock` | 确定性模拟模型；`api_endpoint` 为调用记录目录，适用于 IMS Role 行为模拟。 |
| `gguf` | Candle GGUF local inference. |
| `hf` | Candle HF local inference (safetensors layout). |

```json
{
  "name": "紫幻模型",
  "enabled": true,
  "model": {
    "type": "chat_llm_api",
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

```json
{
  "name": "本地 GGUF 模型",
  "enabled": true,
  "model": {
    "type": "chat_llm_local",
    "llm": {
      "model_name": "Qwen3-0.6B",
      "api_endpoint": "",
      "api_style": "open_ai_chat_completions",
      "weight_style": "gguf",
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

### Local (`text_embedding_local`)

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

### API (`text_embedding_api`)

API text embedding models call a remote OpenAI-compatible embeddings endpoint
(`POST {api_endpoint}` with `{ "model", "input" }`, `Authorization: Bearer <api_key>`).

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `model.type` | Fixed as `"text_embedding_api"` | Yes | Identifies an API text embedding model. |
| `model.model_name` | string | Yes | Provider embedding model name. Cannot be empty. |
| `model.api_endpoint` | string | Yes | Full URL of the embeddings endpoint. Cannot be empty. |
| `model.api_key` | string \| null | No | API key. |
| `model.timeout_secs` | number | No | Timeout per request in seconds. Defaults to `30`. |
| `model.retry_count` | number | No | Number of retries after a failed request. Defaults to `2`. |

```json
{
  "name": "OpenAI Embedding",
  "enabled": true,
  "model": {
    "type": "text_embedding_api",
    "model_name": "text-embedding-3-small",
    "api_endpoint": "https://api.openai.com/v1/embeddings",
    "api_key": "sk-...",
    "timeout_secs": 30,
    "retry_count": 2
  }
}
```