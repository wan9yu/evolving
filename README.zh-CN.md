# evolving

[English](README.md) | **中文**

[![CI](https://github.com/wan9yu/evolving/actions/workflows/ci.yml/badge.svg)](https://github.com/wan9yu/evolving/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/wan9yu/evolving/branch/main/graph/badge.svg)](https://codecov.io/gh/wan9yu/evolving)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)

> 决策的 git——一个持久的决策层，在某个被绑定的检查变红的那一刻，让相关决策重新浮现。

## 问题所在

一个决策被做出了——*我们自己来做检索；我们把 schema 冻住；不要 Redis*——当时它背后的推理是真实而经过权衡的。然后它滚出了视野。线程被归档，工单被关闭，人员轮换。

几个月后，这个决策所依据的某个假设悄然破裂：一个依赖改变了行为，一个曾让这个决定正确的约束不再成立，那个证明该声明的测试停止运行了。没有人被可靠地告知。决策仍然在生效，仍然在塑造这份代码库——但它脚下的地面已经移动，而没有人把两者联系起来。

`ev` 就是弥合这道缝隙的那一层。它把人类亲自做出的决策*以及这些决策所依据的根据*记录为一条不可变、内容寻址的链，为每条根据绑定一个可证伪的检查，并在**那个检查变红的那一刻让相关决策重新浮现**。它处理的是事实，而非裁决：没有评分，没有排名，没有自动判决——只有一份诚实的记录，写明决定了什么、为什么这样决定、谁在承担责任，以及守护每条理由的检查是否仍然存活。

一个自包含的单一 Rust 二进制文件。无网络，无守护进程。存储是一个本地的 `.evolving/` 目录——内容寻址，仅追加。

## `ev` 是什么——又不是什么

`ev` 最常被问到的问题是*这不就是……吗？* 它不是：

- **一份 ADR。** 一份架构决策记录（Architecture Decision Record）捕获一个决策*为什么*被做出——以散文形式，写一次，然后任其腐烂。没人会重读它，也没有任何东西在它所依据的前提不再成立时告诉你。`ev` 同样记录*为什么*，但把它绑定到一个可证伪的检查上，并在**那个检查变红的那一刻把决策带回来**。一份 ADR 是一块墓碑；一个 `ev` 决策是活着的。
- **测试上的一句注释。** 一句像*「这个测试守护着 no-Redis 决策」*的批注，是没有人在决策时会读的非结构化散文。当测试失败时你得到的是一个红色的测试——而不是*no-Redis 决策破裂了；这是被拒绝的那个备选方案，以及谁在承担责任*——而且一句注释无法告诉你那个守卫本身已经悄然停止运行。`ev` 把这条联系做成结构化、内容寻址的，让整个决策重新浮现，并跟踪那个检查是否甚至还活着。
- **git。** git 给*代码*——即*是什么*——做版本管理。它没有决策、决策所依据的根据、或某个过往决断的假设是否仍然成立这些概念；`git log` 是可查找的，但它从不主动*找上*你。`ev` 借用了 git 的脊梁——不可变、内容寻址、仅追加——并补上了 git 缺少的那一个动词：**在某个决策脚下的地面移动时，让它重新浮现。**

它也不是一个任务追踪器、一个 CI 系统、或一个环境监视器：它不管理任何工作项，它不拥有你的测试套件（它只读取一个被绑定的检查是否通过），并且它只对记录进 git 的变化触发——绝不对一次 UI 点击、或一次不留下提交的配置漂移触发。`ev` 侦测并让决策重新浮现；它不做预防。

## Install

```sh
cargo install evolving
```

这会在你的 `PATH` 上安装一个 `ev` 二进制文件。包名为 `evolving`；命令是 `ev`。

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

## The model

- **Tick**——链中的一个决策。它**被哈希的负载**是 `{decision, observe, grounds, parent_id}`；`id`、`status`、`held_since`、`blame`、`authority`、`jurisdiction` 与 `round_id` 是保存在哈希*之外*的簿记信息，因此它们可以改变而不必伪造一个新的身份。
- **Ground**——一个决策所依据的理由。一条根据要么是**被选中的**（支持所采取决策的理由），要么是一条**未走的路**（`rejected:<option>`，即拒绝某个备选方案的理由）。
- **Check**——随着世界变化、使一条被选中的根据保持诚实的东西。它要么是一个**Test**（一个测试选择器加上一个对照测试、使其保持存活的平台 / 触发器 / 表面，以及它上次通过时所在的 `verified_at_sha`），要么是一次人工的 **Person** 复检（对某人在何时/何地重新确认该根据的一个引用）。一个 *harvested* 的 Test 绑定可以**不**携带对照测试；它会像任何其他绑定一样被评估，但在补上一个之前读作*falsifiability-not-proven*（可证伪性未被证明）。
- **Identity**——`id = first 12 hex of SHA-256`，对 `{decision, observe, grounds, parent_id}` 的规范化 JSON 计算得出。对任何被哈希字段的改动都会产生一个不同的 `id`；编辑一个簿记字段则不会触动 `id`。
- **Append-only**——该链从不被就地编辑。一次变更是一个**新的子节点**，其 `parent_id` 指向它的前驱。这正是为什么 `ev guard`——它会添加一个被哈希的检查——写入的是一个子节点，而不是改动它的目标。

## The refusals it enforces

`ev` 之被定义，既靠它所做的，也同样靠它所拒绝的。`ev verify` 针对下列各项审计整条链，并报告*所有*违规，而不只是第一个：

- **封闭的 schema。** 任何带有固定身份 schema 之外字段的 tick 都会被拒绝。内容寻址的 id 永远不能携带一个未经校验的字段。
- **人工复检始终保持为人工。** 一条由某人复检的根据，永远不能被强行绑定到一个测试上。
- **被拒绝的路不携带检查。** 一条未走的路不能携带检查。
- **系统永远不是自我演进式语言的主语。** 自我演进 / 自我改进类动词必须以人类为主语，而非系统（尽力而为的词法 lint）。
- **每一个改动性操作都要指名一个人。** 一个决策或一次 guard 必须携带 `--blame`（或一个可解析的 `git config user.name`）。
- **不自动关闭。** 没有任何东西会自行关闭、修剪或停止一个决策；每一次变更都由人类亲自作出。

## Migrating an existing decision history

`ev migrate` 把一份*既有的*决策历史回填进账本——通过**采集（harvesting）**那些记录已经持有的裁定与结构化声明的「未走的路」，而绝不把散文挖掘成一条根据。

- **可插拔的源格式。** 一个源以 `<kind>:<path>` 给出。每种 kind 都是一个纯粹、格式感知的提取器（`gitlog`、`to-human`、`decisions-immutable`、`escalation`）；它们只解析裁定与*结构化*的被拒绝之路。一个不含结构化之路的块会以**零根据**导入——一次诚实的捕获，绝非一个被合成的理由。
- **幂等、保持链。** 一次回填会计算每条记录*将会*取得的内容寻址 id，并跳过任何已在存储中的键，因此运行两次时第二遍什么也不写。该链被保持：一次回溯日期的链中插入会被作为 *re-linked* 报告，绝不被重写。
- **测试采集。** `--bind-check` 把一个既有测试采纳为一个被绑定的检查。一个 harvested 绑定声明完整的存活性（你不能半采集），并在真实变红时 gate——但它不携带对照测试，所以它的可证伪性从未被证明。`ev check` 会像评估任何其他绑定一样评估它、把该行标注为*falsifiability-not-proven*，并计入这笔债；`ev guard` 补上一个对照测试以偿清它。
- **Jurisdiction 标注。** 一个被导入的决策可以携带一个来自 `{A, B, C, D}` 的声明式 `jurisdiction` 标签（用 `ev decide --jurisdiction` 设定，或在整批回填时由 `ev migrate --jurisdiction-map` 应用）。`A` 与 `B` 可以 gate；`C` 与 `D` 在结构上是**只侦测（detect-only）**——对它们的任何非绿裁定都被映射为一个不参与门禁的 `memo` 事实（这样 `--exit-on-red` 永远无法因它们而触发），并且 `ev verify` 禁止一个 `C`/`D` tick 携带任何可运行的测试检查。只侦测是这条记录的一个**结构性**属性，而非某个代码路径可能忘记遵守的约定。
- **对账（Reconciliation）。** `--reconcile --against <kind>:<path>` 把一份源与存储连接起来，并报告**捕获缺口**——一条源持有而账本从未捕获的裁定——连同 in-both、store-only 与 un-keyable 计数。
- **绝不编造作者。** 一条既没有自己的作者、也没有 `--blame` 兜底的源记录*不会*被导入；它会作为一个缺口浮现。作者绝不被捏造。

on-disk schema 在设计上是**向前兼容**的：一个更新版写入者的非哈希簿记字段会被容忍（被解析通过，并作为一条 `verify` warning 浮现），而被哈希的 / 身份负载保持严格封闭。

## The honesty boundary

`ev` 完成的是一幅特定的图——*一个被人类审定的决策是否仍然存活，以及守护它的检查本身是否还活着？*——并且对这幅图的边缘保持诚实：

- **它不声称对离线测试结果具有防篡改性。** `ev` 记录的是某个测试曾被绑定、以及它被验证时所在的提交，但它无法证明某个离线测试结果是诚实的。这是一个有记录的边界，而非一项保证。
- **它对记录进 git 的变化触发**——某个被绑定的检查变红，或某次提交改动了一个被声明的触发器。它**不**侦测外部状态漂移：一次 UI 点击、一处 org/配置改动、或一次不留下 git 提交的上游 API 行为变化，都不会触发 `ev`。一个只会因外部状态而失败的检查，应当放在一个定时器上，而非绑定到一个触发器。
- **它侦测；它不预防。** `ev` 是让一个破裂假设重新浮现的决策记忆，而不是一个阻止它发生的环境哨兵。

## Documentation

使用文档位于 [`docs/`](docs/)：

- [`docs/commands.md`](docs/commands.md)——权威的命令参考：每一个标志、退出码、每个命令打印出的确切字符串，以及每个命令的一个可运行示例。
- [`docs/concepts.md`](docs/concepts.md)——深入的模型说明：Tick schema、Ground、Check、内容寻址的身份、仅追加的不可变性、jurisdiction、向前兼容的 schema，以及 `ev verify` 所强制执行的那些拒绝项。
- [`docs/philosophy.md`](docs/philosophy.md)——设计哲学：`ev` 背后的那些信条，以及它为什么做出这些选择。

**让 AI agent 使用 `ev`？** [`skills/ev/SKILL.md`](skills/ev/SKILL.md) 是一个与具体工具无关的 agent skill——把它放进你 agent 的 skills 目录，agent 就能正确使用 `ev`，无需翻说明书。

## License

Apache-2.0.
