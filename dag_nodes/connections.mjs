import { port } from "#zihuan-sdk";

function connectionField(connection_kind, description) {
  return { key: "config_id", data_type: "String", description, required: true, widget: "connection_select", connection_kind };
}

function configId(inline_values) {
  return inline_values.config_id ?? inline_values.connection_id;
}

/** @type {import("#zihuan-sdk").NodeDefinition[]} */
export const nodes = [
  {
    type_id: "redis", display_name: "Redis连接", category: "数据库", description: "从系统连接配置中选择 Redis 并输出 RedisRef 引用",
    config_fields: [connectionField("redis", "选择系统中的 Redis 连接配置")], input_ports: [], output_ports: [port("redis_ref", "RedisRef")],
    execute: async ({ inline_values, zihuan }) => ({ redis_ref: await zihuan.storage.redis(configId(inline_values)) }),
  },
  {
    type_id: "mysql", display_name: "MySQL连接", category: "数据库", description: "从系统连接配置中选择 MySQL 并输出 MySqlRef 引用",
    config_fields: [connectionField("mysql", "选择系统中的 MySQL 连接配置")], input_ports: [], output_ports: [port("rdb_ref", "RdbRef")],
    execute: async ({ inline_values, zihuan }) => ({ rdb_ref: await zihuan.storage.mysql(configId(inline_values)) }),
  },
  {
    type_id: "sqlite", display_name: "SQLite连接", category: "数据库", description: "从系统连接配置中选择 SQLite 并输出 SqliteRef 引用",
    config_fields: [connectionField("sqlite", "选择系统中的 SQLite 连接配置")], input_ports: [], output_ports: [port("sqlite_ref", "RdbRef")],
    execute: async ({ inline_values, zihuan }) => ({ sqlite_ref: await zihuan.storage.sqlite(configId(inline_values)) }),
  },
  {
    type_id: "rustfs", display_name: "RustFS对象存储", category: "数据库", description: "从系统连接配置中选择 RustFS 并输出 S3Ref 引用",
    config_fields: [connectionField("rustfs", "选择系统中的 RustFS 对象存储连接配置")], input_ports: [], output_ports: [port("s3_ref", "S3Ref")],
    execute: async ({ inline_values, zihuan }) => ({ s3_ref: await zihuan.storage.s3(configId(inline_values)) }),
  },
  {
    type_id: "weaviate", display_name: "Weaviate向量数据库", category: "数据库", description: "从系统连接配置中选择 Weaviate 并输出 WeaviateRef 引用",
    config_fields: [connectionField("weaviate", "选择系统中的 Weaviate 连接配置")], input_ports: [], output_ports: [port("weaviate_ref", "WeaviateRef")],
    execute: async ({ inline_values, zihuan }) => ({ weaviate_ref: await zihuan.storage.weaviate(configId(inline_values)) }),
  },
];
