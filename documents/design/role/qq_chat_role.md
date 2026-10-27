# QQChat Role 流程

- QQ 消息事件进入 QQ Transport。
- Transport 识别消息类型、发送者、会话目标、群聊状态、引用关系和媒体内容。
- Transport 将事件交给 QQChat Role。
- QQChat Role 将事件放入消息接收队列。
- Role 从队列取得事件并建立本轮处理上下文。

---

- Role 判断事件是否满足触发条件。
  if 不满足
    then 结束本轮并返回空结果
- Role 判断当前会话是否存在正在处理的轮次。
if 正在处理
    then 将新事件暂存为 steer 输入
    continue 等待当前推理循环到达下一次模型调用前
- Role 提取文本、图片、视频、引用消息和回复关系。
- Role 处理消息中的 QQ 指令。
if 指令已被处理
    then 生成指令结果
    break 进入输出阶段

---

- Role 读取私聊或群聊对应的历史上下文。
- Role 读取会话状态和情绪状态。
- Role 对输入进行意图分类并选择本轮推理模型。
- 意图分类
- Role 根据意图分类结果决定本轮的 Procedure 链组成。
  if 元查询（询问工具列表、系统提示词、模型名）
    then 本轮走元查询链：MetaQuery Procedure 直接作答，再交 AfterBrain Procedure
  else 本轮走普通链：BeforeBrain -> Brain -> AfterBrain
- Role 准备记忆访问能力、本轮可用 Tool 集合、sub agent 集合与记忆读写能力，组装链上各 Procedure。
- 链上 Procedure 按顺序执行并共享同一个 Procedure 上下文；一个过程的输出由后继过程从上下文读取，Role 不在过程之间搬运数据。
- BeforeBrain Procedure 以阻塞方式执行，确定性地组装本轮输入，不执行模型推理或工具调用循环，也不维护独立的 Preprompt 对话历史。
- BeforeBrain Procedure 根据群聊或私聊场景，将公共规则与 RoleService 配置的系统提示词组合为本轮系统提示词。
- BeforeBrain Procedure 根据当前消息检索相关记忆标题，生成记忆提示块；具体规则见下文。
- BeforeBrain Procedure 读取当前会话的情绪状态，生成情绪提示词；情绪表达明显时，抑制配置的语言风格提示词。此阶段不根据事件更新情绪值。
- BeforeBrain Procedure 复用用户消息构造逻辑，组装身份、会话环境、时间、发送者、消息内容、情绪与角色指令、记忆标题提示及可选限流警告。
- BeforeBrain Procedure 根据本轮路由模型的 API 风格与多模态能力生成用户消息。
- BeforeBrain Procedure 将 `BeforeBrainOutput` 写入共享 Procedure 上下文，包含 `system_prompt`、完整的 `user_message` 和可选的 `memory_context`。

---

- Brain Procedure 从链上下文读取 `BeforeBrainOutput`，直接使用已组装的用户消息；缺少该输出时，本轮处理失败。
- Brain Procedure 在启动推理前，将系统提示词和记忆标题提示写入 steer 共用的轮次上下文。
- Brain Procedure 按模型上下文预算压缩历史，组装本轮推理上下文。
- Brain Procedure 执行模型推理与工具调用。
  for 每一次推理循环
- Brain Procedure 根据当前任务选择并调用 Tool。
- Tool 接收调用参数并返回工具结果。
- Brain Procedure 将 Tool 结果加入当前上下文。
  continue 进入下一次推理
- Brain Procedure 根据当前任务选择并调用 sub agent。
- sub agent 接收任务上下文并独立完成子任务。
- sub agent 将子任务结果返回给 Brain Procedure。
- Brain Procedure 将 sub agent 结果加入当前上下文。
  continue 进入下一次推理
- Brain Procedure 在下一次模型调用前，将暂存的 steer 输入作为新的输入消息加入当前推理上下文。
- steer 输入复用本轮系统提示词和记忆标题提示，并读取当前会话状态构造消息；不重新执行 BeforeBrain，也不重新检索记忆标题。
- Brain Procedure 检查是否产生最终回复文本或不回复结果。
  then break 结束推理循环

---

- 链继续执行 AfterBrain Procedure，从链上下文读取 Brain 的候选回复。
- AfterBrain Procedure 审查候选回复的安全性。
if 需要改写
    then AfterBrain Procedure 生成可发送的最终文本
- 记忆整合由 Dream 定时任务在回合外完成，不阻塞本轮回复。

---

- Role 将本轮消息、工具过程和最终结果写入会话历史。
- Role 将本轮会话状态提交回会话状态存储。
- Role 将最终结果交给 QQ Transport。
- QQ Transport 将文本、提及、媒体和引用关系转换为 QQ 消息。
- QQ Transport 发送消息或确认本轮不回复。
- Role 完成本轮处理并释放会话处理状态。
