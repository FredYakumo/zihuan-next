# Connection

Connection 是一个具体资源的生命周期接口。连接以 struct 承载配置、客户端和运行任务，类型专属的操作由 struct 的固有方法或业务能力接口提供。

`connect` 初始化资源，`health_check` 检查已有资源，`close` 关闭资源，`reconnect` 先关闭再重新初始化。关闭操作可以重复调用。重新连接需要可变访问，不能在关闭之后由旧使用者隐式恢复。

`is_closed` 和 `status` 表达本地生命周期状态。`keep_alive` 和 `heartbeat_interval` 描述连接的持续运行需求。读取状态不会建立连接，也不会发起健康检查。

具体连接类型通过 `from_config` 构造自身资源，持有类型专属的初始化规则。ConnectionHandle 的创建入口按配置类型分发，句柄通过 TryFrom 转换为具体资源引用。

ConnectionManager 提供通用的获取入口；需要区分同一配置的不同资源时，使用变体键。检索 schema 到变体键的映射由调用方负责，管理器不依赖具体业务类型。

ConnectionManager 负责配置查找、实例复用、使用时间和回收。管理器移除实例时调用 Connection 的关闭方法；禁用、删除或修改配置后，旧实例不再复用。仍有使用者的实例不会仅因闲置时间而回收，显式关闭和配置变更仍会关闭这些实例。

| 配置类型 | 运行资源 |
| --- | --- |
| MySQL | MySqlConfig |
| SQLite、MockRdb | SqliteConfig |
| Redis | RedisConfig |
| Weaviate | WeaviateRef |
| Elasticsearch | ElasticsearchRef |
| RustFS | S3Ref |
| Bot Adapter | BotConnection |
| Web Search Engine | PluginWebSearchEngine |
| Tokenizer | BpeSegmenter |

数据库连接关闭底层连接池，Redis 关闭共享连接，HTTP 客户端释放管理中的客户端引用。S3 和搜索插件按请求工作，关闭后拒绝新操作；已经开始的请求允许完成。Tokenizer 关闭后释放已加载资源，文本切分退回标点规则。

健康检查遵循资源本身的能力。网络存储检查服务响应，Bot 查询登录信息，搜索插件检查本地入口是否可用，Tokenizer 检查模型能否加载。搜索插件的本地检查不代表远端搜索服务可用。

BotConnection 持有重连和心跳任务，后端持有该次会话的发送和中继任务。关闭连接同时取消这些任务并清除未完成的动作响应。重连针对持续运行的 Bot 会话；HTTP 请求和数据库命令的重试仍遵循各自协议，不能统一重放可能已经生效的写操作。

Role Service 按角色登记 Bot 连接的使用关系。停止角色时释放它的事件订阅，最后一个角色释放后关闭连接；其他角色共用的连接继续运行。工作流显式获取的 Bot 连接由运行时实例管理。角色资料查询只复用已有连接，不创建后台任务。

Weaviate 的集合、对象和查询操作直接定义在 WeaviateRef 上。WebSearchEngine 是搜索能力接口并继承 Connection，PluginWebSearchEngine 是当前的插件实现。调用方持有 Arc<dyn WebSearchEngine>，可以替换搜索实现，同时统一管理生命周期。
