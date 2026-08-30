# SubAgent

在紫幻里，可以定义sub agent，sub agent是一个独立的agent系统，拥有自己的上下文和工具列表，并且自身也可以作为一套工具供别的agent使用。

## 配置文件

每个定义保存为程序目录下 `sub_agents` 文件夹中的独立 YAML 文件：

```text
<程序目录>/sub_agents/<id>.yaml
```

配置定义如下：

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
