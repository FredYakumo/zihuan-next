<img src="public/zihuan.png" alt="ZiHuan" width="200" height="200">

## ZiHuan Next

---
[![Rust](https://img.shields.io/badge/built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-AGPL--3.0-blue.svg)](https://github.com/FredYakumo/zihuan-next/blob/master/LICENSE)
[![Latest release](https://img.shields.io/github/v/release/FredYakumo/zihuan-next?label=release)](https://github.com/FredYakumo/zihuan-next/releases/latest)
[![GitHub stars](https://img.shields.io/github/stars/FredYakumo/zihuan-next)](https://github.com/FredYakumo/zihuan-next/stargazers)
[![Downloads](https://img.shields.io/github/downloads/FredYakumo/zihuan-next/total)](https://github.com/FredYakumo/zihuan-next/releases)

Zihuan is a high-performance, highly customizable Rust-based AI Agent platform. Zihuan Impls a **interesting** **Workspace Agent** (zihuan code for coding) and an **IM Agent** (QQ bots, etc.) ~~*(I think these two cover pretty much all scenarios, but I'll add more if new use cases come up)*~~ as the basic framework.
And Zihuan can easily tweak agent behavior and model inference details—whether using **Candle**, **llama.cpp**, or **online APIs in various formats**.


紫幻是一个高性能和高可定制化的Rust的AI Agent平台，现在紫幻实现了一套我觉得比较有趣的**Workspace Agent(zihuan code，写代码的)**和**即时通讯软件Agent(QQ机器人啥的)** ~~*(我觉得目前这两种Agent场景已经覆盖完全了，当然如果以后还有新的方式，也会支持)*~~作为基础能力框架.
然后，紫幻还可以更容易的定制Agent运行的行为细节，和Agent的大脑-模型 推理运行的细节(可以使用多种多样的模型，无论是使用Candle或者llama.cpp，还是各种格式的在线API)。

你可以参考文档查看Agent细节。


## Quick Start

直接下载符合你操作系统响应的版本 [latest release](https://github.com/FredYakumo/zihuan-next/releases/latest)，然后运行即可

*如果你不需要模型推理加速(你的模型不使用zihuan来跑)，直接下载cpu版本最好

*如果需要模型推理加速，需要下载指定gpu加速版本的，然后需要安装对应版本的gpu运行时依赖，例如cuda12.6。*

紫幻启动的时候会在用户数据目录创建或者读取紫幻运行时的相关配置文件，你至少需要一个基础Agent来让你蹬紫幻，所以紫幻首次运行时还会引导你安装和配置一个Agent。


## Features
### 聊天

I think the most interesting part of designing a bot is giving it long-term memory and personal likes/dislikes.

It can't like Millet today and then like  chrysanthemum tomorrow, nor can it forget the people it has interacted with. Those bots that just call an LLM (plus various tools) to spew out tons of output are obviously boring.

While Zihuan supports all the tools you can think of, or tools you develop for it, it also tries to remember every day.

我觉得设计一个机器人最有趣的事情还是让它有自己的长期记忆和偏好好恶，

它不能今天喜欢~~某谷物公司~~明天又喜欢~~某为~~，也不能记不住跟它产生交集的人，那种只会调用LLM(然后加上各种tool)产生一大堆输出的机器人显然很无聊。

紫幻在支持了各种你能想到的工具，或者你为它开发能力工具的同时，它还会尝试记住每天。

<img width="1080" alt="shot-3" src="https://github.com/user-attachments/assets/137e4808-5ce3-4714-a0e3-6f5ddaf9f9cb" />
<img width="1080" alt="shot-4" src="https://github.com/user-attachments/assets/994472eb-2d37-4160-811d-c5b4856e3239" />



## Contribute

welcome your feedback on usage, bug reports, or suggestions for new features in the [Issues](https://github.com/FredYakumo/zihuan-next/issues).

If you find Zihuan is interesting, fun, you are also welcome to join our development team and contribute code in the [PR](https://github.com/FredYakumo/zihuan-next/pulls).


欢迎在[Issues](https://github.com/FredYakumo/zihuan-next/issues)里提出使用意见，Bug反馈，或者如果你希望加入什么新功能等。

如果你觉得紫幻很有趣很好玩，也欢迎你加入一起开发，在[PR](https://github.com/FredYakumo/zihuan-next/pulls)里贡献代码。

## License

AGPL-3.0. See [LICENSE](LICENSE).
