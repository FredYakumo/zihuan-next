# Procedure

Procedure 是 RoleService 内部固定生命周期点的处理单元，用于完成主推理（BrainAgent）以外的额外处理与副作用。

RoleService 的概念性顺序固定为：

```text
Transport -> BeforeBrain Procedures -> BrainAgent -> AfterBrain Procedures -> Transport
```

## BeforeBrain Procedure

BeforeBrain Procedure 在主 Brain 执行前运行。当前实现包括：

- QQ 的 `qq_before_brain`（BeforeBrainAgent）：记忆召回、情绪状态更新、Dream 记忆候选读取和最近消息查询，产出注入主推理 prompt 的上下文块。
- Workspace 的会话命名（`workspace_session_title`）：新会话的首条消息由 Agent编排模型生成会话标题，作为该会话在会话列表中的名字。

## AfterBrain Procedure

AfterBrain Procedure 在主 Brain 产生候选回复后执行验证和必要的改写。QQ 的 `qq_after_brain`（AfterBrainAgent）额外保护 QQ 媒体占位符与发送协议；其他 RoleService 可在相同的固定后置位置执行自己的渠道约束。

## 执行方式

Procedure 按执行方式分为两类：

- `Blocking`：在管道内同步执行，结果返回给调用方参与后续决策（如 QQ 的回复审查）。
- `Background`：后台异步执行，失败仅记录日志，不阻塞主推理（如 Workspace 的会话命名）。

所有 RoleService 中除主推理以外的额外处理与副作用都必须实现为 procedure——即 `zihuan_core::role::procedure` 的 `Procedure` trait。trait 锚定了 procedure 的执行流程：实现方只提供 `run()`，所有 procedure 统一经由 trait 的 `execute()` 锚点执行（统一开始/完成/失败日志），执行器不会绕过该锚点直接调用 `run()`。每个 procedure 的代码放在对应 RoleService crate 的 `procedure` 模块下，以 procedure 的名字命名 `.rs` 文件。
