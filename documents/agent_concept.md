## 前言

这些概念有一些是众所周知的，再在这里写一遍显得有点啰嗦，~~*或者有点像我教你做事的感觉*~~。
主要是写这个文档的时候，有一些部分概念可能不同的人有不同的解释，它还不是一个公理，因此需要先统一一个定义，这样才好继续介绍紫幻中Agent的设计。

## 从LLM 到 Agent

虽然很多人觉得LLM 约等于 AI，也的确大语言模型是具有相当程度(甚至比人类聪明多了)的智能。

不过LLM就是一个无状态的无情的推理模型，它的工作模式就是续写文本(生成文本)，Transformer最初发明出来就是用来在翻译这种seq2seq的任务上的 。

为了实现我们的AI目标，我们需要设计和开发一种叫`Agent`的程序，它其实就是一套使用了大模型的程序，它的执行流程总体而言是确定的，它会保存运行时产生的数据，

这些数据也可能会影响`Agent`程序的运行过程。

Agent仍然是一套可解释的运行过程，在这里，LLM是Agent用于在`自然语言处理`任务上的一种函数，即按Agent这里设计的信息进行输入，LLM输出Agent程序中在这里期望的输出。

*理论上只要模型能力(按现在的模型)不是太差，Agent的表现下限就不会太低。* 对于紫幻里的几种基础Agent，一般能力的模型应该足以胜任，这里说的不是解决复杂任务(如编程)的Agent，而是紫幻基础能力的Agent。

## Zihuan Agent的基础形式(Brain Agent)

在紫幻里，Agent的基本单位为Brain Agent。

Brain Agent负责把模型推理、对话上下文与工具执行组织成一个可追踪的循环。它先将当前对话和已注册工具的定义交给LLM；当模型返回工具调用时，Brain执行对应工具，将结果追加到对话中，再继续推理。模型不再请求工具时，本轮运行结束并返回期间产生的消息。

Brain本身不负责具体业务能力：搜索、发送消息、运行工作流等能力都由`BrainTool`提供。因此，同一个Brain运行时可以通过组合不同的模型和工具，成为Workspace Agent、IM聊天软件Agent、或子Agent的执行核心。

核心实现位于[`zihuan_core/src/agent/brain.rs`](../zihuan_core/src/agent/brain.rs)，其基本结构如下：

```rust
/// Orchestrates a multi-turn LLM ↔ tool call loop.
///
/// Create a `Brain`, register tools with [`Brain::with_tool`] or [`Brain::add_tool`],
/// then call [`Brain::run`] with the initial conversation messages.
pub struct Brain {
    llm: Arc<dyn LLMBase>,
    tools: Vec<Arc<dyn BrainTool>>,
    observer: Option<Arc<dyn BrainObserver>>,
    iteration_hook: Option<Arc<dyn BrainIterationHook>>,
    long_task_context: Option<LongTaskContext>,
}

impl Brain {
    pub fn new(llm: Arc<dyn LLMBase>) -> Self {
        Self {
            llm,
            tools: Vec::new(),
            observer: None,
            iteration_hook: None,
            long_task_context: None,
        }
    }

    /// Register a tool, consuming and returning `self` for builder-style chaining.
    pub fn with_tool(mut self, tool: impl BrainTool) -> Self {
        self.tools.push(Arc::new(tool));
        self
    }

    /// Register a tool in-place.
    pub fn add_tool(&mut self, tool: impl BrainTool) {
        self.tools.push(Arc::new(tool));
    }
}
```

`run()`和`run_streaming()`实现实际的循环：它们保存完整的消息轨迹，支持按资源冲突情况并行执行工具，并通过观察者、迭代钩子和长任务上下文接入日志、插嘴及任务生命周期。
