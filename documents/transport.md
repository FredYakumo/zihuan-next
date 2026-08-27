# Transport

Transport 是外部渠道与 RoleService 之间的私有协议边界。它把外部事件转换为 RoleService 可处理的请求，并把 RoleService 的动作转换为渠道实际可发送的输出。

QQ / Workspace event -> Transport -> RoleService
RoleService action -> Transport -> QQ / Workspace output

- 接收 QQ 消息事件或 Workspace 请求。
- 处理渠道专属协议、媒体格式、身份信息和消息发送。
- 将渠道输入整理为 RoleService 请求，并将回复渲染为渠道输出。