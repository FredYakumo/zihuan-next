import { port } from "#zihuan-sdk";

const toMessage = (content, role) => ({
  role: ["system", "user", "assistant", "tool"].includes(String(role).toLowerCase()) ? String(role).toLowerCase() : "system",
  parts: [{ type: "text", text: content }],
  tool_calls: [],
});

/** @type {import("#zihuan-sdk").NodeDefinition[]} */
export const nodes = [
  {
    type_id: "string_to_llm_message", display_name: "字符串转 LLMMessage", category: "消息", description: "将字符串封装为可选 role 的 LLMMessage",
    input_ports: [port("content", "String"), port("role", "String")], output_ports: [port("message", "LLMMessage")],
    execute: ({ inputs }) => ({ message: toMessage(inputs.content, inputs.role) }),
  },
  {
    type_id: "as_system_llm_message", display_name: "字符串转 LLMMessage", category: "消息", description: "将字符串封装为默认 system 的 LLMMessage",
    input_ports: [port("content", "String"), port("role", "String")], output_ports: [port("message", "LLMMessage")],
    execute: ({ inputs }) => ({ message: toMessage(inputs.content, inputs.role) }),
  },
  {
    type_id: "message_content", display_name: "提取 LLMMessage 内容", category: "消息", description: "从 LLMMessage 中提取 content 字段",
    input_ports: [port("message", "LLMMessage")], output_ports: [port("content", "String")],
    execute: ({ inputs }) => {
      const parts = inputs.message?.parts ?? [];
      if (parts.length !== 1 || parts[0]?.type !== "text") throw new Error("LLMMessage content is None");
      return { content: parts[0].text };
    },
  },
  {
    type_id: "llm_message_to_string", display_name: "LLMMessage转字符串", category: "消息", description: "将 reasoning_content 与 content 拼接为字符串",
    input_ports: [port("message", "LLMMessage")], output_ports: [port("content", "String")],
    execute: ({ inputs }) => {
      const message = inputs.message;
      const text = message?.parts?.length === 1 && message.parts[0]?.type === "text" ? message.parts[0].text : "";
      const content = [message?.reasoning_content, text].filter((value) => value).join("\n\n");
      if (!content) throw new Error("LLMMessage reasoning_content and content are both empty");
      return { content };
    },
  },
  {
    type_id: "llm_message_content_as_json", display_name: "LLMMessage内容转JSON", category: "消息", description: "将 LLMMessage 的 content 字符串解析为 JSON",
    input_ports: [port("message", "LLMMessage")], output_ports: [port("json", "Json"), port("failed", "String")],
    execute: ({ inputs }) => {
      const parts = inputs.message?.parts ?? [];
      if (parts.length !== 1 || parts[0]?.type !== "text") throw new Error("LLMMessage content is None");
      const content = parts[0].text;
      try { return { json: JSON.parse(content) }; } catch { /* continue with streaming recovery */ }
      const values = [];
      let offset = 0;
      const decoder = new TextDecoder();
      while (offset < content.length) {
        const fragment = content.slice(offset).trimStart();
        offset += content.slice(offset).length - fragment.length;
        if (!fragment) break;
        try {
          const parsed = JSON.parse(fragment);
          values.push(parsed);
          break;
        } catch {
          const closing = fragment.search(/[}\]](?=\s|$)/);
          if (closing < 0) break;
          try {
            values.push(JSON.parse(fragment.slice(0, closing + 1)));
            offset += closing + 1;
          } catch { break; }
        }
      }
      if (values.length > 0 && values.every(Array.isArray)) return { json: values.flat() };
      for (const suffix of ["]", "]]", "]]]"]) {
        try {
          const value = JSON.parse(`${content}${suffix}`);
          if (Array.isArray(value)) return { json: value };
        } catch { /* try the next suffix */ }
      }
      return { failed: content };
    },
  },
  {
    type_id: "string_to_plain_text", display_name: "字符串转QQ纯文本", category: "消息", description: "将字符串转换为 QQ 消息中的纯文本（PlainText）消息段",
    input_ports: [port("text", "String")], output_ports: [port("result", "QQMessage")],
    execute: ({ inputs }) => ({ result: { type: "text", data: { text: inputs.text } } }),
  },
  {
    type_id: "at_qq_target_message", display_name: "构造QQAt消息", category: "消息", description: "输入 QQ 目标 id 字符串，输出 @ 目标的 QQ 消息段",
    input_ports: [port("id", "String")], output_ports: [port("result", "QQMessage")],
    execute: ({ inputs }) => ({ result: { type: "at", data: { qq: inputs.id } } }),
  },
  {
    type_id: "string_to_image_content_part", display_name: "字符串转图片/视频 MessagePart", category: "消息", description: "将字符串 URL（或 data: URL）封装为 LLM 多模态 MessagePart",
    input_ports: [port("url", "String"), port("media_type", "String", { required: false })], output_ports: [port("content_part", "MessagePart")],
    execute: ({ inputs }) => {
      const mediaType = (inputs.media_type ?? "image").trim().toLowerCase();
      if (mediaType && mediaType !== "image" && mediaType !== "video") throw new Error(`media_type must be 'image' or 'video', got '${mediaType}'`);
      return { content_part: { type: mediaType || "image", media: { source: "upload", original_source: inputs.url, rustfs_path: "", name: null, description: null, mime_type: null } } };
    },
  },
  {
    type_id: "binary_to_image_content_part", display_name: "二进制转图片/视频 MessagePart", category: "消息", description: "将二进制字节 + MIME 编码为 base64 data URL，并封装为多模态 MessagePart",
    input_ports: [port("bytes", "Binary"), port("mime", "String", { required: false }), port("media_type", "String", { required: false })], output_ports: [port("content_part", "MessagePart")],
    execute: ({ inputs }) => {
      const mediaType = (inputs.media_type ?? "image").trim().toLowerCase();
      if (mediaType && mediaType !== "image" && mediaType !== "video") throw new Error(`media_type must be 'image' or 'video', got '${mediaType}'`);
      if (!Array.isArray(inputs.bytes)) throw new Error("bytes is required");
      const mime = inputs.mime?.trim() || "image/png";
      return { content_part: { type: mediaType || "image", media: { source: "upload", original_source: `data:${mime};base64,${Buffer.from(inputs.bytes).toString("base64")}`, rustfs_path: "", name: null, description: null, mime_type: mime } } };
    },
  },
  {
    type_id: "build_multimodal_user_message", display_name: "构建多模态 LLMMessage", category: "消息", description: "将可选文本和若干 MessagePart 拼接为多模态 LLMMessage",
    input_ports: [port("text", "String", { required: false }), port("parts", { Vec: "MessagePart" }, { required: false }), port("role", "String", { required: false })], output_ports: [port("message", "LLMMessage")],
    execute: ({ inputs }) => {
      const role = ["system", "user", "assistant", "tool"].includes(String(inputs.role ?? "user").toLowerCase()) ? String(inputs.role ?? "user").toLowerCase() : "user";
      const parts = [...(inputs.text ? [{ type: "text", text: inputs.text }] : []), ...(inputs.parts ?? [])];
      return { message: { role, parts, reasoning_content: null, tool_calls: [], tool_call_id: null, usage: null } };
    },
  },
  {
    type_id: "tool_result", display_name: "Tool 结果消息", category: "AI", description: "将工具执行结果封装为 role=tool 的 LLMMessage，供 agentic loop 回写对话列表",
    input_ports: [port("tool_call", "Json"), port("content", "String")], output_ports: [port("message", "LLMMessage")],
    execute: ({ inputs }) => {
      const toolCallId = inputs.tool_call?.tool_call_id;
      if (typeof toolCallId !== "string") throw new Error("tool_call missing 'tool_call_id' field");
      return { message: { role: "tool", parts: [{ type: "text", text: inputs.content }], reasoning_content: null, tool_calls: [], tool_call_id: toolCallId, usage: null } };
    },
  },
  {
    type_id: "json_to_qq_message_vec", display_name: "JSON转QQMessage列表", category: "消息", description: "将 LLM 输出的 QQ 消息 JSON 二维数组转换为 Vec<Vec<QQMessage>>",
    input_ports: [port("json", "Json")], output_ports: [port("messages", { Vec: { Vec: "QQMessage" } }), port("failed", "Json")],
    execute: ({ inputs }) => {
      try {
        if (!Array.isArray(inputs.json) || inputs.json.length === 0) throw new Error("QQ message JSON array must not be empty");
        const messages = inputs.json.map((batch, batchIndex) => {
          if (!Array.isArray(batch) || batch.length === 0) throw new Error(`QQ message batch ${batchIndex + 1} must not be empty`);
          const result = [];
          for (const item of batch) {
            if (item?.message_type === "plain_text") {
              if (typeof item.content !== "string" || !item.content.trim()) throw new Error("plain_text.content must not be blank");
              result.push({ type: "text", data: { text: item.content } });
            } else if (item?.message_type === "at") {
              const target = String(item.target ?? "").trim();
              if (!target) throw new Error("at.target must not be empty");
              result.push({ type: "at", data: { qq: target } });
            } else if (item?.message_type === "combine_text") {
              if (!Array.isArray(item.content_list) || item.content_list.length === 0) throw new Error("combine_text.content_list must not be empty");
              let substantive = false;
              for (const segment of item.content_list) {
                if (segment?.message_type === "plain_text") {
                  if (typeof segment.content !== "string" || segment.content === "") throw new Error("combine_text plain_text.content must not be empty");
                  substantive ||= Boolean(segment.content.trim());
                  result.push({ type: "text", data: { text: segment.content } });
                } else if (segment?.message_type === "at") {
                  const target = String(segment.target ?? "").trim();
                  if (!target) throw new Error("combine_text at.target must not be empty");
                  result.push({ type: "at", data: { qq: target } });
                } else {
                  throw new Error("unsupported combine_text content item");
                }
              }
              if (!substantive) throw new Error("combine_text must contain at least one substantive plain_text item");
            } else {
              throw new Error("unsupported QQ message item");
            }
          }
          return result;
        });
        return { messages };
      } catch {
        return { failed: inputs.json };
      }
    },
  },
  {
    type_id: "qq_message_to_image", display_name: "QQ消息转图片数据", category: "消息", description: "将 QQMessage(Image) 转为 Image 数据，并输出对象存储路径",
    input_ports: [port("qq_message", "QQMessage")], output_ports: [port("image", "Image"), port("object_storage_path", "String")],
    execute: ({ inputs }) => {
      const message = inputs.qq_message;
      if (message?.type !== "image") throw new Error("qq_message must be image variant");
      const metadata = message.data;
      const path = metadata?.media?.rustfs_path || metadata?.media?.original_source;
      if (!path) throw new Error("image has no resolvable object_storage_path");
      return { image: { metadata, object_storage_path: path }, object_storage_path: path };
    },
  },
];
