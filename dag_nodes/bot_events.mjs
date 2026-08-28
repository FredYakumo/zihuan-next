import { port } from "#zihuan-sdk";

/** @type {import("#zihuan-sdk").NodeDefinition[]} */
export const nodes = [
  {
    type_id: "ims_bot_adapter_provider", display_name: "IMS BotAdapter Provider", category: "Bot适配器", description: "从系统连接配置中选择已启用的 IMS Bot Adapter 并输出 BotAdapterRef 引用",
    config_fields: [{ key: "config_id", data_type: "String", description: "选择系统中的 IMS Bot Adapter 连接配置", required: true, widget: "connection_select", connection_kind: "bot_adapter" }], input_ports: [], output_ports: [port("ims_bot_adapter", "BotAdapterRef")],
    execute: async ({ inline_values, zihuan }) => ({ ims_bot_adapter: await zihuan.bot.adapter(inline_values.config_id ?? inline_values.connection_id) }),
  },
  {
    type_id: "extract_sender_from_event", display_name: "提取发送者", category: "Bot适配器", description: "从消息事件中提取可用于回发的 Sender",
    input_ports: [port("message_event", "MessageEvent")], output_ports: [port("result", "Sender")],
    execute: async ({ inputs, zihuan }) => ({ result: await zihuan.bot.senderFromEvent(inputs.message_event) }),
  },
  {
    type_id: "extract_sender_id_from_event", display_name: "提取发送者ID", category: "Bot适配器", description: "从消息事件中提取发送者的QQ号（字符串）",
    input_ports: [port("message_event", "MessageEvent")], output_ports: [port("result", "String")],
    execute: async ({ inputs, zihuan }) => ({ result: await zihuan.bot.senderIdFromEvent(inputs.message_event) }),
  },
  {
    type_id: "extract_group_id_from_event", display_name: "提取群号", category: "Bot适配器", description: "从群消息事件中提取群号（字符串）",
    input_ports: [port("message_event", "MessageEvent")], output_ports: [port("result", "String")],
    execute: async ({ inputs, zihuan }) => ({ result: await zihuan.bot.groupIdFromEvent(inputs.message_event) }),
  },
  {
    type_id: "extract_optional_group_id_from_event", display_name: "提取可选群号", category: "Bot适配器", description: "从消息事件中提取群号；私聊时返回空字符串",
    input_ports: [port("message_event", "MessageEvent")], output_ports: [port("result", "String")],
    execute: async ({ inputs, zihuan }) => ({ result: await zihuan.bot.optionalGroupIdFromEvent(inputs.message_event) }),
  },
  {
    type_id: "extract_qq_message_list_from_event", display_name: "事件提取 QQMessage 列表", category: "Bot适配器", description: "从消息事件中提取原始 QQ 消息列表 (Vec<QQMessage>)",
    input_ports: [port("message_event", "MessageEvent")], output_ports: [port("message_list", { Vec: "QQMessage" })],
    execute: async ({ inputs, zihuan }) => ({ message_list: await zihuan.bot.messagesFromEvent(inputs.message_event) }),
  },
  {
    type_id: "message_event_type_filter", display_name: "消息类型分支", category: "Bot适配器", description: "根据消息类型（好友/群组）路由消息事件",
    input_ports: [port("message_event", "MessageEvent"), port("filter_type", "String", { required: false })], output_ports: [port("true_event", "MessageEvent"), port("false_event", "MessageEvent")],
    execute: async ({ inputs, zihuan }) => zihuan.bot.filterEventType(inputs.message_event, inputs.filter_type),
  },
  {
    type_id: "send_message", display_name: "发送消息", category: "Bot适配器", description: "根据 Sender 向 QQ 好友或群组发送消息",
    input_ports: [port("ims_bot_adapter", "BotAdapterRef", { description: "Bot适配器引用" }), port("sender", "Sender", { description: "消息目标 Sender" }), port("message", { Vec: "QQMessage" }, { description: "要发送的QQ消息段列表" })],
    output_ports: [port("success", "Boolean", { description: "是否发送成功" }), port("message_id", "Integer", { description: "服务器返回的消息ID" })],
    execute: async ({ inputs, zihuan }) => zihuan.bot.send(zihuan.resources.botAdapter(inputs.ims_bot_adapter), inputs.sender, inputs.message),
  },
  {
    type_id: "send_friend_message_batches", display_name: "批量发送好友消息", category: "Bot适配器", description: "向QQ好友逐批发送 Vec<Vec<QQMessage>>，支持两次发送之间延迟",
    input_ports: [port("ims_bot_adapter", "BotAdapterRef"), port("target_id", "String"), port("message_batches", { Vec: { Vec: "QQMessage" } }), port("delay_millis", "Integer", { required: false })],
    output_ports: [port("success", "Boolean"), port("summary", "String"), port("message_ids", { Vec: "Integer" })],
    execute: async ({ inputs, zihuan }) => zihuan.bot.sendBatches(zihuan.resources.botAdapter(inputs.ims_bot_adapter), inputs.target_id, inputs.message_batches, { target_type: "friend", delay_millis: inputs.delay_millis }),
  },
  {
    type_id: "send_group_message_batches", display_name: "批量发送群组消息", category: "Bot适配器", description: "向QQ群组逐批发送 Vec<Vec<QQMessage>>，支持两次发送之间延迟",
    input_ports: [port("ims_bot_adapter", "BotAdapterRef"), port("target_id", "String"), port("message_batches", { Vec: { Vec: "QQMessage" } }), port("delay_millis", "Integer", { required: false })],
    output_ports: [port("success", "Boolean"), port("summary", "String"), port("message_ids", { Vec: "Integer" })],
    execute: async ({ inputs, zihuan }) => zihuan.bot.sendBatches(zihuan.resources.botAdapter(inputs.ims_bot_adapter), inputs.target_id, inputs.message_batches, { target_type: "group", delay_millis: inputs.delay_millis }),
  },
  {
    type_id: "send_qq_message_batches", display_name: "发送QQ消息批次", category: "Bot适配器", description: "将 QQ 消息批次逐批发送到好友或群组，并输出发送汇总",
    input_ports: [port("ims_bot_adapter_ref", "BotAdapterRef"), port("target_id", "String"), port("target_type", "String", { required: false }), port("message_batches", { Vec: { Vec: "QQMessage" } })],
    output_ports: [port("summary", "String"), port("success", "Boolean")],
    execute: async ({ inputs, zihuan }) => zihuan.bot.sendBatches(zihuan.resources.botAdapter(inputs.ims_bot_adapter_ref), inputs.target_id, inputs.message_batches, { target_type: inputs.target_type }),
  },
  {
    type_id: "extract_message_from_event", display_name: "事件提取 LLMMessage 列表", category: "Bot适配器", description: "从消息事件中提取 LLMMessage 列表",
    input_ports: [port("message_event", "MessageEvent"), port("ims_bot_adapter", "BotAdapterRef"), port("message_id", "Integer", { required: false }), port("rdb_ref", "RdbRef", { required: false }), port("s3_ref", "S3Ref", { required: false })],
    output_ports: [port("messages", { Vec: "LLMMessage" }), port("content", "String"), port("ref_content", "String"), port("is_at_me", "Boolean"), port("at_target_list", { Vec: "String" })],
    execute: async ({ inputs, zihuan }) => zihuan.bot.extractMessages(zihuan.resources.botAdapter(inputs.ims_bot_adapter), inputs.message_event, { message_id: inputs.message_id, s3_ref: inputs.s3_ref }),
  },
];
