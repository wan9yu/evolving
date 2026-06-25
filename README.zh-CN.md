# evolving

[English](README.md) | **中文**

[![CI](https://github.com/wan9yu/evolving/actions/workflows/ci.yml/badge.svg)](https://github.com/wan9yu/evolving/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/wan9yu/evolving/branch/main/graph/badge.svg)](https://codecov.io/gh/wan9yu/evolving)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)

> **决策不会一直正确。** `ev` 盯着一个决策所依据的理由，当那个理由破裂时让决策重新浮现——在你下一次 `ev check`，无需常驻守护进程。

## 问题所在

你的 agents 做决策的速度快到没人追得上——*我们自己来做检索；把 schema 冻住；不要 Redis*——每一个都依据一个当时真实的理由。然后它滚出视野：线程被归档，这一轮结束，做出那个决定的 agent 到下一轮就没了。

几个月后，那个理由悄然移动：一个依赖改变了行为，一个曾让这个决定正确的约束被解除，那个证明声明的测试停止运行了。决策仍在生效，仍在塑造代码库，但它脚下的地基已经不在了。于是一个全新的 agent 重新推导一个你早已敲定的决定，或者在一个悄然失效的前提上继续搭建——而你，那个在承担责任的人，很晚才发现，甚至根本不知道。

`ev` 就是弥合这道缝隙的那一层。它把人类亲自做出的决策*以及它所依据的根据*记录为一条不可变、内容寻址的链，为每条根据绑定一个可证伪的检查，并在**那个检查变红时让决策重新浮现——在你下一次 `ev check`，无需常驻守护进程。** 它处理的是事实，而非裁决：没有评分，没有排名，没有自动判决——只有一份诚实的记录，写明决定了什么、为什么、谁在承担责任，以及守护每条理由的检查是否仍存活。底层是*决策的 git*；agents 提议（propose），由人类批准（ratify）并承担责任。

一个自包含的单一 Rust 二进制文件。无网络，无守护进程。存储是一个本地的 `.evolving/` 目录——内容寻址，仅追加。

## `ev` 是什么——又不是什么

`ev` 最常被问到的问题是*这不就是……吗？* 它不是：

- **一份 ADR。** 一份架构决策记录（Architecture Decision Record）捕获一个决策*为什么*被做出——以散文形式，写一次，然后任其腐烂。没人会重读它，也没有任何东西在它所依据的前提不再成立时告诉你。`ev` 同样记录*为什么*，但把它绑定到一个可证伪的检查上，并在**那个检查变红时把决策带回来**。一份 ADR 是一块墓碑；一个 `ev` 决策是活着的。
- **测试上的一句注释。** 一句像*「这个测试守护着 no-Redis 决策」*的批注，是没有人在决策时会读的非结构化散文。当测试失败时你得到的是一个红色的测试——而不是*no-Redis 决策破裂了；这是被拒绝的那个备选方案，以及谁在承担责任*——而且一句注释无法告诉你那个守卫本身已经悄然停止运行。`ev` 把这条联系做成结构化、内容寻址的，让整个决策重新浮现，并跟踪那个检查是否甚至还活着。
- **git。** git 给*代码*——即*是什么*——做版本管理。它没有决策、决策所依据的根据、或某个过往决断的假设是否仍然成立这些概念；`git log` 是可查找的，但它从不主动*找上*你。`ev` 借用了 git 的脊梁——不可变、内容寻址、仅追加——并补上了 git 缺少的那一个动词：**在某个决策脚下的地面移动时，让它重新浮现。**
- **一个 agent 记忆系统。** Mem0、Zep、Letta 这类工具做的是*记住*——在你询问时召回过往会话里的事实与上下文，且只在被动地被问到时才浮现矛盾。`ev` 不是记忆；它是**主动且狭窄的。** 它持有的是人类亲自做出的*决策、以及绑定在其上的检查*——而非任意召回——并且它不等你来查：当一个决策被绑定的检查变红时，那个决策会自己回到你面前。记忆告诉你*你决定了什么*；`ev` 告诉你*你所决定的何时不再为真*。

它也不是一个任务追踪器、一个 CI 系统、或一个环境监视器：它不管理任何工作项，它不拥有你的测试套件（它只读取一个被绑定的检查是否通过），并且它只对记录进 git 的变化触发——绝不对一次 UI 点击、或一次不留下提交的配置漂移触发。`ev` 侦测并让决策重新浮现；它不做预防。

关于完整的版图——ADR 工具、决策账本、agent 记忆、签名出处协议、架构适应度函数、治理框架——以及 `ev` 在其中的位置，见 [`docs/neighbors.md`](docs/neighbors.md)。

## Install

```sh
cargo install evolving
```

这会在你的 `PATH` 上安装一个 `ev` 二进制文件。包名为 `evolving`；命令是 `ev`。

没有 Rust 工具链？每个 release 都附带**预构建的静态二进制文件**——从 [latest release](https://github.com/wan9yu/evolving/releases/latest) 下载对应平台的文件，放到 `PATH` 上即可。Linux 构建是静态的（`musl`），所以 `aarch64-unknown-linux-musl` 二进制可在任何 ARM-Linux 主机上运行，不受其 glibc 版本影响：

```sh
curl -L <asset-url> | tar xz
install ev ~/.local/bin/ev
ev --version
```

从源码构建：

```sh
git clone https://github.com/wan9yu/evolving
cd evolving
cargo build --release
# binary at target/release/ev
```

## Quickstart

核心循环：**decide → 绑定一个检查 → 继续工作 → 在变红时重新浮现。**

创建存储：

```sh
ev init
```

记录一个决策，附带一条被选中的根据（由某个人在指定时间复检）以及一条未走的路：

```sh
ev decide "build our own retrieval; reject pgvector" \
  --observe "evaluating retrieval backend for v2" \
  --assume "team has bandwidth to maintain it long-term" \
  --revisit "Q3" \
  --reject "pgvector: would lock our schema" \
  --blame "You"
```

记录一个决策，其被选中的根据由一个**测试**而非人工来守护。一个测试绑定必须携带一个对照测试（即当声明被破坏时应当翻红的那个测试）、至少一个用于存活判定的平台 / 触发器 / 表面，以及它上次被验证时所在的提交：

```sh
ev decide "restore-safety counter DB-backed; reject Redis" \
  --observe "multi-pod restore-safety counter" \
  --assume "no Redis; multi-pod coordination via the existing DB" \
  --assume-test "pytest tests/test_redis_absent.py" \
  --counter-test "pytest tests/test_redis_absent.py::test_redis_injection_flips_red" \
  --on-platform linux-ci \
  --triggered-by pyproject.toml \
  --surface pyproject-deps \
  --verified-at-sha d308afac1b2c3d4e5f60718293a4b5c6d7e8f901 \
  --reject "Redis: a new infra dependency" \
  --blame "You"
```

用 `ev guard` 在*事后*为当前 HEAD 决策的某条尚未绑定的根据绑定一个测试。由于该检查是被哈希的负载的一部分，这会写入一个**新的子节点**，而不是改动已有的 tick：

```sh
ev guard "pytest tests/test_schema_frozen.py" <HEAD-id> "schema stays frozen" \
  --counter-test "pytest tests/test_schema_frozen.py::test_schema_change_flips_red" \
  --on-platform linux-ci \
  --triggered-by schema.sql \
  --surface schema-ddl
```

`<HEAD-id>` 是最近一次 `ev decide` / `ev guard` 打印出的 id。第三个位置参数指明要绑定哪条根据（通过声明文本或通过索引）；仅当还有不止一条根据尚未绑定时才需要它。

审计该链及其拒绝项，然后完整地读取一个决策：

```sh
ev verify
ev show <id>
```

`ev verify` 确认每个 id 都等于其负载的哈希、谱系是仅向前的，并且每个 tick 都能针对那个封闭的 schema 与检查形态通过校验。

评估那些被绑定的检查，并让任何检查已变红的决策重新浮现。当一条测试检查绑定到一个可运行的命令时，`ev check --run` 会运行它、记录一条 receipt，并运行其对照测试以证明该绑定确实能翻转；`--exit-on-red` 据此决定退出码。`ev why` 把一个检查映射回它所守护的决策，`ev reopen` 则展示完整的决策对象（冻结态对比当前态，连同那条未走的路）：

```sh
ev check --run --platform linux-ci --exit-on-red
ev why "pytest tests/test_schema_frozen.py"
ev reopen <id>
```

## The honesty boundary

`ev` 完成的是一幅特定的图——*一个被人类审定的决策是否仍然存活，以及守护它的检查本身是否还活着？*——并且对这幅图的边缘保持诚实：

- **它不声称对离线测试结果具有防篡改性。** `ev` 记录的是某个测试曾被绑定、以及它被验证时所在的提交，但它无法证明某个离线测试结果是诚实的。这是一个有记录的边界，而非一项保证。
- **它对记录进 git 的变化触发**——某个被绑定的检查变红，或某次提交改动了一个被声明的触发器。它**不**侦测外部状态漂移：一次 UI 点击、一处 org/配置改动、或一次不留下 git 提交的上游 API 行为变化，都不会触发 `ev`。一个只会因外部状态而失败的检查，应当放在一个定时器上，而非绑定到一个触发器。
- **它侦测；它不预防。** `ev` 是让一个破裂假设重新浮现的决策记忆，而不是一个阻止它发生的环境哨兵。

## Documentation

使用文档位于 [`docs/`](docs/)：

- [`docs/concepts.md`](docs/concepts.md)——深入的模型说明：Tick schema、Ground、Check、内容寻址的身份、仅追加的不可变性、jurisdiction、provenance、向前兼容的 schema，以及 `ev verify` 所强制执行的那些拒绝项。
- [`docs/neighbors.md`](docs/neighbors.md)——`ev` 在版图中的位置：它真正的邻居（ADR 工具、Lore、决策账本、agent 记忆、签名出处、适应度函数、治理框架），每一个的思路与 `ev` 的不同走法——以及共享的地基和 `ev` 自己的缺口。
- [`docs/commands.md`](docs/commands.md)——权威的命令参考：每一个标志、退出码、每个命令打印出的确切字符串，以及每个命令的一个可运行示例。
- [`docs/migrating.md`](docs/migrating.md)——把一份既有的决策历史带入 `ev`：canonical 决策摄取格式、编写一个发出该格式的小适配器，以及那些内置的便捷提取器。
- [`docs/philosophy.md`](docs/philosophy.md)——设计哲学：`ev` 背后的那些信条，以及它为什么做出这些选择。
- [`docs/measuring-drift-defense.md`](docs/measuring-drift-defense.md)——如何诚实地衡量 `ev` 是否真的接住了重复推导：永远不在没有分母的情况下报 catch-rate、那个盲的外部分母、从不出错的对照组、不可捕获的同群体，以及以一个 MISS 开头。

**让 AI agent 使用 `ev`？** [`skills/ev/SKILL.md`](skills/ev/SKILL.md) 是一个与具体工具无关的 agent skill——把它放进你 agent 的 skills 目录，agent 就能正确使用 `ev`，无需翻说明书。

## License

Apache-2.0.
