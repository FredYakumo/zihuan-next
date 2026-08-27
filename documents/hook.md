# Hook

Hook 是 RoleService 内部固定生命周期点的处理单元，用于在主 Brain 执行前后完成必须的系统处理。

RoleService 的概念性顺序固定为：

```text
Transport -> BeforeBrainAgent -> BrainAgent -> AfterBrainAgent -> Transport
```

## BeforeBrainAgent

`BeforeBrainAgent` 在主 Brain 执行前准备上下文。当前 QQ 实现的职责包括记忆召回、情绪状态更新、Dream 记忆候选读取和最近消息查询；Workspace 在适用时沿用相同的前置位置，但不暴露工作流编辑。

## AfterBrainAgent

`AfterBrainAgent` 在主 Brain 产生候选回复后执行验证和必要的改写。QQ 实现额外保护 QQ 媒体占位符与发送协议；其他 RoleService 可在相同的固定后置位置执行自己的渠道约束。

分为 `BeforeBrainAgent` 和 `AfterBrainAgent`
