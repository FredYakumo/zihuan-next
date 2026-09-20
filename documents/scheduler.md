# Scheduler

紫幻提供了使用脚本语言编排一些周期任务的能力，实现脚本放在 `scheduled_jobs/`，文件里需要声明任务名和入口。脚本可以使用紫幻SDK与紫幻Rust本体进行交互。

### Scheduler脚本配置

| 字段 | 要求 | 默认值 |
| --- | --- | --- |
| `task_name` | 必填，非空 | 无 |
| `entry` | 入口函数名 | `run_job` |
| `description` | 可省 | 无 |


Python 声明模块级Dict：

```python
JOB_MANIFEST = {
    "task_name": "启动hook",
    "entry": "run_job",
    "description": "技术宅挽救世界",
}
```

JavaScript 导出对象：

```javascript
export const job_manifest = {
  task_name: "启动hook",
  entry: "run_job",
  description: "技术宅挽救世界",
};
```