# Glossary

| 英文 | CN | 定义 | 标识 |
|---|---|---|---|
| `session` | 会话 | 对话身份与状态的容器，跨回合存续 | `session_id`、`QqChatSessionState` |
| `turn` | 回合 | 一条用户消息的完整处理，含其工具调用循环 | `turn_id`、`TurnStarted` / `TurnFinished` |
| `turnGroup` | 回合组 | 一个 `turn` 加继续它的 steer 回合，即用户视角的一次提问 | `continuesPrevious` |
| `steer` | 插嘴 | `turn` 进行中注入的用户消息，后端为它新开一个 `turn` | `PendingSteerStore`、`SteerTurnContext` |
| `iteration` | 迭代 | 工具循环内的一次 LLM 调用，不是 `turn` | `iteration_hook`、`MAX_TOOL_ITERATIONS` |
| `stage` | 环节 | 可上报的处理步骤，如 System Prompt 组装、一次 LLM 推理 | `QQ_CONTEXT_STAGE_*`、`record_graph_stage` |
| `step` | 步骤 | 命令引擎与脚本引擎的执行单元，与上述层级无关 | `command/step.rs` |
| `history` | 历史 | 持久化的消息序列，跨 `turn` 累积 | `history_key`、`load_history` |
| `conversation` | 推理上下文 | 本次请求发给模型的消息列表，内存态 | `brain_conversation` |
| `procedure` | 处理单元 | 一个 `turn` 的处理由链上若干 procedure 依次执行，主推理本身也是其一 | `Procedure` |
