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
- Role 准备记忆访问能力和本轮可用工具。
- Role 准备本轮可调用的 Tool 集合。
- Role 准备本轮可调用的 sub agent 集合。
- Role 准备记忆读取与记忆写入能力。
- Role 从长期记忆中读取与当前事件相关的信息。
- Role 对输入进行意图分类并选择本轮推理模型。
- Role 生成 QQChat 的系统上下文。
- Role 生成包含身份、会话环境、时间、发送者和消息内容的用户输入。
- Role 将当前事件及历史上下文交给 BeforeBrain Procedure。
- BeforeBrain Procedure 更新前置上下文、记忆信息和本轮提示内容。
- Role 将处理后的上下文交给 Brain Agent。

---

- Brain Agent 执行模型推理与工具调用。
  for 每一次推理循环
- Brain Agent 根据当前任务选择并调用 Tool。
- Tool 接收调用参数并返回工具结果。
- Brain Agent 将 Tool 结果加入当前上下文。
  continue 进入下一次推理
- Brain Agent 根据当前任务选择并调用 sub agent。
- sub agent 接收任务上下文并独立完成子任务。
- sub agent 将子任务结果返回给 Brain Agent。
- Brain Agent 将 sub agent 结果加入当前上下文。
  continue 进入下一次推理
- Brain Agent 在下一次模型调用前，将暂存的 steer 输入作为新的输入消息加入当前推理上下文。
- Brain Agent 检查是否产生最终回复文本或不回复结果。
  then break 结束推理循环

---

- Role 将 Brain Agent 的结果交给 AfterBrain Procedure。
- AfterBrain Procedure 处理情绪、记忆、回复指令和回复内容审查。
- AfterBrain Procedure 根据本轮结果更新短期会话状态。
- AfterBrain Procedure 将需要保留的信息写入长期记忆。
- Role 检查本轮是否产生需要整合的内容。
if 需要整合
    then Role 将其交给 Dream 流程
- Dream 流程整理会话内容并生成记忆变更。
- Dream 流程将确认后的记忆变更写入长期记忆。
- Role 检查回复内容是否需要改写。
if 需要改写
    then Role 生成可发送的最终文本

---

- Role 将本轮消息、工具过程和最终结果写入会话历史。
- Role 将最终结果交给 QQ Transport。
- QQ Transport 将文本、提及、媒体和引用关系转换为 QQ 消息。
- QQ Transport 发送消息或确认本轮不回复。
- Role 完成本轮处理并释放会话处理状态。

