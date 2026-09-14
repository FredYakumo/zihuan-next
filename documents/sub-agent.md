# SubAgent

在紫幻里，可以定义 sub agent。sub agent 是一个独立的 agent 系统，拥有自己的上下文和工具列表，并且自身也可以作为一套工具供别的 agent 使用。

sub agent 采用声明式定义：一个 YAML 文件描述端口、提示词与工具清单，运行时由 `DeclarativeAgent` 按"系统提示词 + 用户消息 + 工具调用循环"执行一轮任务。核心实现位于 [`zihuan_core/src/agent/declarative_agent.rs`](../zihuan_core/src/agent/declarative_agent.rs)。

## 配置文件

每个定义保存为程序工作目录下 `sub_agents` 文件夹中的独立 YAML 文件：

```text
<程序目录>/sub_agents/<id>.yaml
```

内置定义（`memory_agent`、`run_research_subagent`）随程序嵌入，启动时若目录缺少对应文件会自动补齐，已存在的文件不会被覆盖。WebUI 的 SubAgent 管理页通过 `GET/PUT/DELETE /api/system/subagents/<id>` 读写这些 YAML，保存与读取时都会执行校验。

## 配置定义

- `id`：唯一标识，同时决定 YAML 文件名和 `DeclarativeAgentTool` 的工具名。必须以小写字母开头，只能包含小写字母、数字和下划线。
- `name`：名称；`description` 为空时同时作为工具描述。
- `description`：可选。面向 LLM 的工具描述，为空时回退到 `name`。
- `builtin`：可选，默认 `false`。标记该定义是否为内置定义。
- `inputs`：调用所需的输入端口；每个端口包含 `name`、`data_type`、`description` 与 `required`。调用方看到的工具参数 schema 由输入端口生成，必填端口对应 JSON Schema 的 `required`。
- `outputs`：预期返回的输出端口，字段结构与输入端口相同。
- `system_prompt`：系统提示词。
- `user_prompt`：可选。用户消息模板，`{端口名}` 占位符会被对应输入值替换，无法识别的占位符渲染为空。缺省时使用默认信封：把全部输入渲染成 JSON 的 `Input:` 块，并要求模型只返回包含声明输出的 JSON 对象。
- `prompt_parts`：可选。条件提示词片段，每个片段包含 `port`（引用的输入端口）、`equals`（可选，输入值等于它时才生效）与 `template`（追加的模板文本，支持占位符）。输入存在且非空（或与 `equals` 相等）时，渲染后的片段按声明顺序追加到用户消息末尾。
- `output_mode`：可选，默认 `json_ports`，见下文[输出模式](#输出模式)。
- `llm_kind`：可选，默认 `main`。宿主为本 agent 解析的模型类型，可用值：`main`、`intent_classification`、`math_programming`、`natural_language_reply`；对应类型未注册时回退 `main`。
- `progress_message`：可选。运行前向仪表盘任务输出的进度消息，支持 `{端口名}` 占位符。
- `include_graph_tools`：可选，默认 `false`。把宿主注册的节点图工具并入本 agent 的工具集。
- `run_duration`：可选，默认 `Short`。工具运行时长标记，取值 `Short` 或 `Long`。
- `tool_ids`：本 agent 可调用的工具 id 列表；可以引用其他 sub agent 的 id 实现 agent 嵌套调用，但不能引用自身。

### 输出模式

- `json_ports`（默认）：把最终回复解析为 JSON，并按声明的输出端口映射成 `DataValue`（必填输出缺失时报错）；返回给调用方的是"端口名 → 值"的 JSON 对象字符串。
- `text`：直接返回最终回复文本；输出端口仅作说明，最多声明一个且类型必须为 `String`。

### 校验规则

定义在加载和保存时都会校验，不合法的定义会被拒绝：

- `id` 必须以小写字母开头，只含小写字母、数字或下划线；`name` 不能为空。
- 输入、输出端口的 `name` 必须唯一且非空。
- `tool_ids` 不能为空项或重复项，不能引用自身，且必须存在于允许集合内（宿主注册的工具、其他 agent 的 id）。
- `text` 输出模式下最多声明一个输出端口，且类型必须为 `String`。
- `prompt_parts` 的 `port` 必须指向已声明的输入端口。

## 示例

```yaml
id: translator
name: Translator
description: Translate the given text into the target language.
inputs:
  - name: text
    data_type: String
    description: Text to translate
    required: true
  - name: target_language
    data_type: String
    description: Target language
    required: true
outputs:
  - name: translation
    data_type: String
    description: Translated text
    required: true
system_prompt: |
  You are a translation agent.
  Translate the input text and return only the translation.
user_prompt: '{text}'
output_mode: text
tool_ids: []
```

## 内置 agent 与组合

内置定义位于 [`sub_agents/`](../sub_agents/)，可作为编写自定义 agent 的参考：

- `memory_agent`：记忆管理 agent，工具集为 `list_memory_keys`、`search_memory`、`update_memory`，输出模式 `text`；通过可选的 `operation` 输入配合 `prompt_parts` 强制指定检索或写入操作。
- `run_research_subagent`：研究 agent，使用 `llm_kind: math_programming` 与 `run_duration: Long`；`tool_ids` 引用 `memory_agent`、`web_search` 和 `image_understand`，是 agent 之间组合调用的示例。

## 运行方式

宿主通过 `AgentHost` 注册工具、节点图工具与各类 `llm_kind` 对应的模型，然后把 agent 构建为 `DeclarativeAgent` 并以 `id` 为工具名发布成 `DeclarativeAgentTool`，供其他 agent 的 `tool_ids` 引用。每次调用执行一轮工具调用循环：模型返回工具调用就执行并继续推理，直到模型给出最终回复；异常终止（未正常结束、返回空文本、输出解析失败）会以错误形式返回给调用方。

调度任务脚本还可以通过宿主调用 `subagent.run` 传入内联 YAML 定义与字符串输入，临时运行一个 sub agent 并取回其文本结果。
