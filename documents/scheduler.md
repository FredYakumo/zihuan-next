# Scheduler

Scheduler 在预定时间执行系统内部的后续工作。调度由内核负责，做什么由脚本声明。

## 一次任务

计划任务是一次性的，没有周期或 cron：服务在需要时写入一条，带任务名和预定时间，要重复触发就再写一条。

内核每隔几秒扫一次到期任务，用一次原子状态翻转（等待中 -> 执行中）把它认领再执行，保证同一条任务只跑一次。

## 任务体是脚本

脚本放在 `scheduled_jobs/`，支持 Python 和 JavaScript，在文件里内联声明任务名和入口。脚本可以使用服务提供的SDK与Rust本体进行交互。

### 清单

清单三项字段：`task_name`（必填，非空）、`entry`（入口函数名，缺省 `run_job`）、`description`（可省）。脚本路径不用写，加载器按文件位置补上。

Python 声明模块级字典：

```python
JOB_MANIFEST = {
    "task_name": "任务名",
    "entry": "run_job",
    "description": "描述",
}
```

JavaScript 导出对象：

```javascript
export const job_manifest = {
  task_name: "任务名",
  entry: "run_job",
  description: "描述",
};
```

文件必须位于 `scheduled_jobs/`，扩展名 `.py` 或 `.mjs`。加载失败的脚本只记一条诊断并跳过，不影响其他脚本；`task_name` 重复时保留先注册的那个，并记录一条错误日志说明冲突的两个脚本。

### 入口与返回

入口收到请求对象，字段为 `task`（含 `id`、`task_name`、`source_service`、`triggered_by`、`start_time`）、`agent_id`、`sender_id`。Python 入口直接接收该对象并从 `zihuan_sdk` 导入 SDK；JavaScript 入口额外接收第二个参数，即 SDK 实例。

返回值必须是对象，含布尔字段 `ok`。`ok` 为真时可选 `result` 字符串，作为任务摘要；为假时用 `error` 说明原因，任务落成失败。

同语言的脚本共用一个常驻进程，所以同语言的多个任务会排队执行。

## 静默触发

每次收到消息，先取消该用户等待中的同类任务，再写一条预定在「现在 + 静默间隔」的新任务。消息不断到来，任务就不断后推，安静满一个间隔才会执行。
