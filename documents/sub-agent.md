# SubAgent

在紫幻里，可以定义sub agent，sub agent是一个独立的agent系统，拥有自己的上下文和工具列表，并且自身也可以作为一套工具供别的agent使用。

## 配置文件

每个定义保存为应用数据目录下的独立 YAML 文件：

```text
<用户目录>/subagent/<id>.yaml
```

其中 `<用户目录>` 由 `zihuan_core::system_config::application_data_dir()` 决定。文件名由 `id` 决定，例如 Memory 的默认文件为：

```text
<application_data_dir>/subagent/memory.yaml
```

配置定义如下：

字段含义：

- `id`：唯一标识，同时决定 YAML 文件名和 `SubAgentTool` 的工具名。
- `name`：名称
- `inputs`：调用所需的输入端口；每个端口包含 `name`、`data_type`、`description` 与 `required`。
- `outputs`：预期返回的输出端口，字段结构与输入端口相同。
- `system_prompt`：系统提示词
- `tool_ids`：工具名字id

```yaml
id: memory
name: Memory
inputs:
  - name: content
    data_type: String
    description: Memory request or chat context
    required: true
outputs:
  - name: result
    data_type: String
    description: Memory result
    required: true
system_prompt: |
  You manage durable role memory.
  Use the available tools when needed.
  Return JSON with a result field only.
tool_ids:
  - search_memory
  - update_memory
  - list_memory_keys
  - research
```