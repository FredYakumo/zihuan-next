# Web Search Engine Connection

Web Search Engine 连接封装一个由插件实现的搜索能力。每个操作在插件自己的依赖环境中运行，凭证和超时随配置传入插件。

健康检查确认插件入口在本地可用，不代表搜索服务可用。关闭后拒绝新的搜索操作。
