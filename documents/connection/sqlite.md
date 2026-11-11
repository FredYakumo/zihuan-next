# SQLite Connection

SQLite 连接管理一个基于本地文件的关系数据库连接池。MockRdb 是内存态的 SQLite，数据不落盘。

连接时建立连接池、确保表结构存在并确认可用。健康检查执行一次探针查询。关闭时销毁连接池。
