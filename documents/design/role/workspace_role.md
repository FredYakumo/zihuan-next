# Workspace Role 流程

- Workspace 请求进入 Workspace Transport。
- Transport 识别会话、用户输入、工作区路径和输出方式。
- Transport 将请求交给 Workspace Role。
- Role 建立本轮处理上下文。

---

if 请求缺少有效会话或输入
    then 返回请求状态
- Role 解析或创建会话历史。

---

- Role 确定本轮使用的工作区路径。
- Role 读取角色配置和本轮模型配置。
- Role 读取工作区规则文件和会话相关状态。
- Role 准备工作区工具、记忆能力、网页访问能力、图像理解能力和Sub Agent 能力。
- Role 准备本轮可调用的 Tool 集合。
- Role 准备本轮可调用的 sub agent 集合。
- Role 准备记忆读取与记忆写入能力。
- Role 从长期记忆中读取与当前任务相关的信息。
- Role 生成包含角色身份、工作区路径、可用能力和任务要求的系统上下文。
- Role 将当前输入和会话历史交给 BeforeBrain Procedure。
- BeforeBrain Procedure 准备本轮推理上下文。
- Role 将准备后的上下文交给 Brain Agent。

---

- Brain Agent 读取上下文并生成回复或工具调用。
  for 每一次推理循环
- Brain Agent 根据当前任务选择并调用 Tool。
- Tool 执行工作区操作或外部能力并返回工具结果。
- Brain Agent 将 Tool 结果加入当前上下文。
  continue 进入下一次推理
- Brain Agent 根据当前任务选择并调用 sub agent。
- sub agent 接收任务上下文并独立完成子任务。
- sub agent 将子任务结果返回给 Brain Agent。
- Brain Agent 将 sub agent 结果加入当前上下文。
  continue 进入下一次推理
if Brain Agent 需要用户补充信息
    then 生成用户输入请求
    break 结束推理循环
if Brain Agent 形成最终回复或达到处理终点
    then break 结束推理循环

---

- Role 将 Brain Agent 的结果交给 AfterBrain Procedure。
- AfterBrain Procedure 整理最终回复、工具结果、任务状态和等待用户输入状态。
- AfterBrain Procedure 根据本轮结果更新会话状态。
- AfterBrain Procedure 将需要保留的信息写入长期记忆。
- Role 检查本轮是否产生需要整合的内容。
if 需要整合
    then Role 将其交给 Dream 流程
- Dream 流程整理会话内容并生成记忆变更。
- Dream 流程将确认后的记忆变更写入长期记忆。

---

- Role 将本轮用户输入、推理消息、工具消息和最终回复写入会话历史。
- Role 将处理结果交给 Workspace Transport。
- Workspace Transport 将结果转换为对话消息、工具进度、任务状态或用户输入请求。
- Workspace Transport 向工作区界面输出结果。
- Role 完成本轮处理并保留会话状态。

