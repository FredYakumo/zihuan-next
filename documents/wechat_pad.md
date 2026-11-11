# 个人微信渠道（WeChatPadPro）

把个人微信接进 `ims_bot_adapter`。协议由自建的 [WeChatPadPro](https://github.com/WeChatPadPro/WeChatPadPro)
（iPad 协议）负责，本仓库只做协议翻译。

> 用非官方协议登录个人微信违反微信软件许可及服务协议，有封号风险。建议只用小号，并配合群白名单与限流。

## 链路

```
微信 ←→ WeChatPadPro（自建，HTTP API + webhook）
                │  POST /api/ims/wechat_pad/{webhook_token}
                ▼
        zihuan_service 路由（api/ims.rs，只做转发）
                ▼
        runtime/wechat_pad.rs::WeChatPadBotAdapter
                │  翻译成 OneBot 11 事件
                ▼
   BotAdapter::process_event → 现有解析 / handler / 回复流程
                ▲
                │  OneBot action（send_group_msg 等）
        action_tx → 翻译成 WeChatPadPro REST 调用 → 微信
```

`BotAdapterBackend` 的边界就是 OneBot 11 JSON，所以微信后端不改动主流程：入站把桥的
webhook 载荷转成 `RawMessageEvent`，出站把 OneBot action 转成 `SendText`。

## 配置

连接类型仍是 `bot_adapter`，加一个 `wechat` 段即可切换到微信后端（WebUI 的「连接配置」里有开关）：

```json
{
  "type": "bot_adapter",
  "bot_server_url": "",
  "qq_id": "wxid_bot",
  "wechat": {
    "pad_url": "http://127.0.0.1:8849",
    "pad_token": "<adminKey>",
    "self_wxid": "wxid_bot",
    "webhook_token": "换成随机串",
    "webhook_url": "http://<公网地址>/api/ims/wechat_pad/换成随机串",
    "nickname": "小助手"
  }
}
```

| 字段 | 说明 |
| --- | --- |
| `pad_url` | 协议端地址；`pad_token` 作为 `Authorization: Bearer` 发送 |
| `self_wxid` | 机器人自己的 wxid；未设置 `qq_id` 时它就是 bot id，用于识别 @ 与自身消息 |
| `webhook_token` | 回调路径里的密钥，也是后端实例的注册键；为空则收不到消息 |
| `webhook_url` | 填了就在启动时调用 `/v1/webhook/Config` 自动注册，留空则手动在协议端配置 |
| `bot_names` | 群内文本 @ 的别名兜底 |

`webhook_url` 必须能从协议端访问到 `zihuan_service`；两者不在同一台机器时需要公网地址或隧道。

## 已支持 / 未支持

- 支持：私聊与群聊文本、`<atuserlist>` 里的 @、发送文本、`get_login_info`、`get_stranger_info`。
- 未支持：图片（入站直接跳过，避免 XML 进入 prompt）、引用/合并转发、群成员列表、@ 的真实 mention
  结构（`SendText` 只能退化为文本 `@wxid`）。

## ID 约定

`ims_bot_adapter` 的 id 字段（`message_id`、`sender.user_id`、`group_id`、`ReplyMessage.id`）
统一是 `String`：QQ 侧仍是数字（反序列化时自动转换），微信侧直接使用 `wxid_*` 与 `*@chatroom`。
数据库里 `pending_group_id` 也随之改为文本列，旧库由 MySQL 迁移自动改写。
