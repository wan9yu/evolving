# evolving

[English](README.md) | **中文**

[![CI](https://github.com/wan9yu/evolving/actions/workflows/ci.yml/badge.svg)](https://github.com/wan9yu/evolving/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/badge/crates.io-v0.0.1-orange)](https://crates.io/crates/evolving)
[![codecov](https://codecov.io/gh/wan9yu/evolving/branch/main/graph/badge.svg)](https://codecov.io/gh/wan9yu/evolving)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)

`ev` 是**决策的 git**。它把人类亲自做出的决策、以及这些决策所依据的根据，记录为一条不可变、内容寻址的 *tick 链*；为每条根据绑定一个测试检查或一次人工复检；并在某个被绑定的检查变红时，让相关决策重新浮现。它处理的是**事实，而非裁决**——没有评分，没有排名，没有自动判决；只有一份诚实的记录，写明决定了什么、为什么这样决定、谁在承担责任，以及守护每条理由的检查是否仍然存活。

## Status

`0.0.1`——通往 **`0.1.0` honest-resurface slice** 路上的一个早期、诚实的切片。一个自包含的单一 Rust 二进制文件，无网络，无守护进程；存储位于本地的 `.evolving/` 目录中。

**今天已交付：** 记录决策及其根据（`ev decide`）、在事后绑定一个测试或人工复检（`ev guard`）、读取一个决策（`ev show`），以及审计该链及其拒绝项（`ev verify`）。**仍在向 `0.1.0` 推进：** 评估某个被绑定检查的存活状态、并在它变红时让决策重新浮现（`ev check`），以及 `reopen` / `list` / `log` 这一组能力。所以今天的 `ev` *冻结了契约*——它记录下什么必须保持为真、以及将如何被检查——但还没有替你运行这些检查。

## Install

```sh
cargo install evolving
```

这会在你的 `PATH` 上安装一个 `ev` 二进制文件（包名为 `evolving`；命令是 `ev`）。

从源码构建：

```sh
git clone https://github.com/wan9yu/evolving
cd evolving
cargo build --release
# binary at target/release/ev
```

## Quickstart

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

用 `ev guard` 在*事后*为当前 HEAD 决策的某条尚未绑定的根据附上一个测试。由于该检查是被哈希的负载的一部分，这会写入一个**新的子节点**，而不是改动已有的 tick：

```sh
ev guard "pytest tests/test_schema_frozen.py" <HEAD-id> "schema stays frozen" \
  --counter-test "pytest tests/test_schema_frozen.py::test_schema_change_flips_red" \
  --on-platform linux-ci \
  --triggered-by schema.sql \
  --surface schema-ddl
```

`<HEAD-id>` 是最近一次 `ev decide`/`ev guard` 打印出的 id。第三个位置参数指明要绑定哪条根据（通过声明文本或通过索引）；仅当还有不止一条根据尚未绑定时才需要它。

审计该链及其拒绝项，然后完整地读取一个决策：

```sh
ev verify
ev show <id>
```

`ev verify` 确认每个 id 都等于其负载的哈希、谱系是仅向前的，并且每个 tick 都能针对那个封闭的 schema 与检查形态通过校验。

## The model

- **Tick**——链中的一个决策。它被哈希的负载是 `{decision, observe, grounds, parent_id}`；`id`、`status`、`held_since` 与 `blame` 是保存在哈希之外的簿记信息。
- **Ground**——一个决策所依据的理由。一条根据要么是**被选中的**（支持所采取决策的理由），要么是一条**未走的路**（`rejected:<option>`，即拒绝某个备选方案的理由）。
- **Check**——随着时间推移、使一条被选中的根据保持诚实的东西。它要么是一个**Test**（一个测试选择器加上它的对照测试、使其保持存活的平台/触发器/表面，以及它上次通过时所在的 `verified_at_sha`），要么是一次人工的 **Person** 复检（对某人在何时/何地重新确认该根据的一个引用）。
- **Identity**——`id = first 12 hex of SHA-256`，对 `{decision, observe, grounds, parent_id}` 的规范化 JSON 计算得出。
- **Append-only**——该链从不被就地编辑。一次变更是一个**新的子节点**，其 `parent_id` 指向它的前驱。

## The refusals it enforces (the red lines)

- **封闭的 schema。** 任何带有固定 schema 之外字段的 tick 都会被拒绝。
- **人工复检始终保持为人工。** 一条由某人复检的根据，永远不能被强行绑定到一个测试上。
- **被拒绝的路不携带检查。** 在 `0.1.0` 中，一条未走的路不能携带检查（保留给未来的拒绝理由存活性特性）。
- **系统永远不是自我演进式语言的主语。** 自我演进 / 自我改进类动词必须以人类为主语，而非系统（尽力而为的词法 lint）。
- **每一个改动性操作都要指名一个人。** 一个决策或一次 guard 必须携带 `--blame`（或一个可解析的 `git config user.name`）。
- **不自动关闭。** 没有任何东西会自行关闭、修剪或停止一个决策；每一次变更都由人类亲自作出。

## Honesty / trust boundary

`ev` 完成的是一幅特定的图：*一个被人类审定的决策是否仍然存活，以及守护它的检查本身是否还活着？* 它通过对决策记录进行内容寻址、并要求每一个测试绑定都指名一个对照测试以及使其保持存活的那些表面来做到这一点，这样一个已经悄然死亡的检查就会变得可见。

它**不**声称对离线测试结果具有防篡改性——`ev` 记录的是某个测试曾被绑定、以及它被验证时所在的提交，但它无法证明某个离线测试结果是诚实的。这是一个有记录的边界，而非一项保证。

## Documentation

使用文档位于 [`docs/`](docs/)：

- [`docs/commands.md`](docs/commands.md)——权威的命令参考：每一个标志、退出码、每个命令打印出的确切字符串，以及每个命令的一个可运行示例。
- [`docs/concepts.md`](docs/concepts.md)——深入的模型说明：Tick schema、Ground、Check、内容寻址的身份、仅追加的不可变性，以及 `ev verify` 所强制执行的那些拒绝项。

**让 AI agent 使用 `ev`？** [`skills/ev/SKILL.md`](skills/ev/SKILL.md) 是一个与具体工具无关的 agent skill——把它放进你 agent 的 skills 目录，agent 就能正确使用 `ev`，无需翻说明书。

## License

Apache-2.0.
