import { port } from "#zihuan-sdk";

/** @type {import("#zihuan-sdk").NodeDefinition[]} */
export const nodes = [
  {
    type_id: "qq_message_list_rdb_persistence", display_name: "QQMessage列表RDB持久化", category: "消息存储", description: "将Vec<QQMessage>及调用方提供的元数据持久化到关系数据库",
    input_ports: [port("qq_message_list", { Vec: "QQMessage" }), port("message_id", "String"), port("sender_id", "String"), port("sender_name", "String"), port("group_id", "String", { required: false }), port("group_name", "String", { required: false }), port("rdb_ref", "RdbRef")],
    output_ports: [port("success", "Boolean"), port("qq_message_list", { Vec: "QQMessage" })],
    execute: async ({ inputs, zihuan }) => ({ success: await zihuan.storage.persistQQMessageRdb(zihuan.resources.rdb(inputs.rdb_ref), inputs.qq_message_list, inputs), qq_message_list: inputs.qq_message_list }),
  },
  {
    type_id: "message_rdb_get_user_history", display_name: "获取QQ号消息历史", category: "消息存储", description: "根据 sender_id 读取最近消息历史，可选限定某个群",
    input_ports: [port("mysql_ref", "RdbRef"), port("sender_id", "String"), port("group_id", "String", { required: false }), port("limit", "Integer")], output_ports: [port("messages", { Vec: "String" })],
    execute: async ({ inputs, zihuan }) => zihuan.storage.userHistory(inputs.mysql_ref, inputs.sender_id, inputs.group_id, inputs.limit),
  },
  {
    type_id: "message_rdb_get_group_history", display_name: "获取QQ群聊消息历史", category: "消息存储", description: "根据 group_id 读取最近消息历史",
    input_ports: [port("mysql_ref", "RdbRef"), port("group_id", "String"), port("limit", "Integer")], output_ports: [port("messages", { Vec: "String" })],
    execute: async ({ inputs, zihuan }) => zihuan.storage.groupHistory(inputs.mysql_ref, inputs.group_id, inputs.limit),
  },
  {
    type_id: "message_rdb_search", display_name: "搜索消息记录", category: "消息存储", description: "在消息记录中搜索，支持发送者、群组、内容关键词、时间范围过滤",
    input_ports: [port("mysql_ref", "RdbRef"), port("sender_id", "String", { required: false }), port("group_id", "String", { required: false }), port("contain", "String", { required: false }), port("start_time", "String", { required: false }), port("end_time", "String", { required: false }), port("limit", "Integer"), port("sort_by_time_desc", "Boolean")], output_ports: [port("messages", { Vec: "String" })],
    execute: async ({ inputs, zihuan }) => zihuan.storage.searchMessages(inputs.mysql_ref, inputs),
  },
];
