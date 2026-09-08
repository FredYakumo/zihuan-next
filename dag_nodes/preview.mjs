import { port } from "#zihuan-sdk";

/** Flatten an LLMMessage content (string or content parts) into display text. */
function llmMessageText(message) {
  const content = message?.content;
  if (typeof content === "string") return content;
  if (Array.isArray(content)) {
    return content
      .filter((part) => part?.type === "text" && part.text)
      .map((part) => part.text)
      .join("\n");
  }
  return "";
}

function qqMessageImageUrl(data) {
  return data.object_url || data.url || data.path || data.file || "";
}

/** Shape a QQMessage into the bubble state consumed by the UI template.
 *  cls carries the full class list because data-bind-attr replaces the attribute. */
function qqBubble(message) {
  const data = message?.data ?? {};
  switch (message?.type) {
    case "text":
      return { cls: "znp-qq-bubble is-text", text: data.text ?? "" };
    case "at":
      return { cls: "znp-qq-bubble is-at", prefix: `@${data.target ?? "?"}` };
    case "reply":
      return { cls: "znp-qq-bubble is-reply", prefix: `[Reply id=${data.id ?? "?"}]` };
    case "forward":
      return {
        cls: "znp-qq-bubble is-forward",
        prefix: `[Forward (${Array.isArray(data.content) ? data.content.length : 0})]`,
      };
    case "image": {
      const image = qqMessageImageUrl(data);
      return image
        ? { cls: "znp-qq-bubble is-image", image }
        : { cls: "znp-qq-bubble is-image is-missing", text: "（无图片源）" };
    }
    default:
      return { cls: "znp-qq-bubble is-error", prefix: `[未知类型: ${message?.type}]` };
  }
}

/** @type {import("#zihuan-sdk").NodeDefinition[]} */
export const nodes = [
  {
    type_id: "preview_string", display_name: "Preview String", category: "工具", description: "在节点卡片内预览输入字符串",
    input_ports: [port("text", "String", { required: false })], output_ports: [],
    ui: { template_path: "preview_string.html", style_path: "preview_string.scss" },
    execute: async ({ inputs, zihuan }) => {
      const text = typeof inputs.text === "string" ? inputs.text : "";
      // Preview is display-only: ignore publish failures in contexts without a UI publisher.
      await zihuan.ui.publish({ text, empty: text === "" }).catch(() => false);
      return {};
    },
  },
  {
    type_id: "preview_message_list", display_name: "Preview LLMMessage List", category: "工具", description: "在节点卡片内预览 LLMMessage 列表",
    input_ports: [port("messages", { Vec: "LLMMessage" }, { required: false })], output_ports: [],
    ui: { template_path: "preview_message_list.html", style_path: "preview_message_list.scss" },
    execute: async ({ inputs, zihuan }) => {
      const messages = Array.isArray(inputs.messages) ? inputs.messages : [];
      const items = messages.map((message) => ({
        cls: `znp-mlist-row is-${message?.role ?? "unknown"}`,
        role: String(message?.role ?? "unknown"),
        text: llmMessageText(message),
      }));
      await zihuan.ui.publish({ messages: items, empty: items.length === 0 }).catch(() => false);
      return {};
    },
  },
  {
    type_id: "qq_message_preview", display_name: "Preview QQ Messages", category: "工具", description: "在节点卡片内实时预览 QQMessage 列表（含图片）",
    input_ports: [port("messages", { Vec: "QQMessage" }, { required: false })], output_ports: [],
    ui: { template_path: "qq_message_preview.html", style_path: "qq_message_preview.scss" },
    execute: async ({ inputs, zihuan }) => {
      const messages = Array.isArray(inputs.messages) ? inputs.messages : [];
      const bubbles = messages.map(qqBubble);
      await zihuan.ui.publish({ bubbles, empty: bubbles.length === 0 }).catch(() => false);
      return {};
    },
  },
];
