# ev — 闭环引擎

[![CI](https://github.com/wan9yu/evolving/actions/workflows/ci.yml/badge.svg)](https://github.com/wan9yu/evolving/actions/workflows/ci.yml)
[![coverage](https://codecov.io/gh/wan9yu/evolving/branch/main/graph/badge.svg)](https://codecov.io/gh/wan9yu/evolving)
[![crates.io](https://img.shields.io/crates/v/evolving.svg)](https://crates.io/crates/evolving)

[English](README.md) | 简体中文

声称是廉价的。agent 报告「做完了」「修好了」「验证过了」；仪表盘报告绿色。
证据并不廉价——而几乎没有任何东西，强制一条声称先穿过证据、再被相信。

`ev` 是一个小小的命令行**闭环引擎**，给一个人和 TA 身边的 agents 用。agent 提出**声称
（claim）**，附上类型化的**证据指针**。引擎只检查**指针能否解析（resolve）**——确定性地，
从不评判工作本身好坏——并数出每条锚下面世界已经移动了多远（**漂移，drift**）。**只有人
能闭合**一条声称：带证据闭合、灰置搁置、或宣告死亡。什么都不拦、什么都不 gate——判断
发生在每天一次的短**暂停（pause）**里，攒下来的是一条「什么带着证据闭合了」的**线**。

```
agent 提出声称 ─▶ 证据指针 ─▶ 锚能解析吗？（存在 · 匹配 —— 是事实，不是裁决）
                                   │
                             漂移：锚定之后，
                             世界移动了多远
                                   │
                             人来闭合 ─▶ 带证据闭合
 （什么都不 gate；没有证据的        │   灰置（grey）
  声称只是在暂停处等着）           └─  宣告死亡
                                   │
                             线：带证据闭合的 vs 放手的
```

## 安装

```sh
cargo install evolving   # 安装 `ev` 二进制
```

每个 GitHub Release 都附带预编译静态二进制（含 aarch64 musl，无工具链的主机可直接用）。

## 循环

```sh
ev init                                   # 把一个仓库接入
ev hook install                           # 接上会话钩子：自动捕获 + 开场简报
ev claim "修好了解析器" \                  # agent 提出带指针的声称
    --by agent --evidence commit:<sha>
ev claim "这个边界是安全的"                # 一条裸声称——还没有指针
ev reading <声称id> --depth plain --lang zh <ref>  # agent 给它指一份非作者也能读的解释
ev verify                                 # 重查锚的解析；报告状态与漂移的联合
ev pause                                  # 人的每日仪式：要证据、附证据、灰置、ack、放手
ev ack <声称id> --i-am-the-human          # 人看过了，声称依然成立
ev line                                   # 工作线：什么带着证据闭合了
ev doctor                                 # 检查账本完整性
```

会话还会留下「尾气」（exhaust）：你的 commits 会被自动捕获为自证声称——所以不必为每个
commit 都提声称；只在想断言 bare commit 说不出的东西时才提一条，并钉上指针。

证据指针类型：`commit:<sha>` · `test:<路径>[::<该行的文本>]` · `file:<路径>[::<该行的文本>]` ·
`artifact:<名字>[::<该行的文本>]` · `metric:<文本>` 和 `url:<文本>`（仅记录，不检查）。

`::` 后面是**要匹配的文本，不是行号**——ev 按内容锚定。`file:src/x.rs:56` 会被拒绝；
`file:src/x.rs::fn parse(` 在被引用的那一行发生变化时转红，而裸的 `file:src/x.rs` 只在文件被删除时转红。

## 工作原理

一切都是追加式账本里的事件（`.evolving/ledger/`，每台机器一个 JSONL 文件，随仓库提交）。
没有数据库、没有守护进程：每次调用重读全部事件，折叠出当前状态——一条声称沿着
`裸（bare）→ 有证据（evidenced）→ 锚已解析（anchored）` 移动，或者灰置着，或者以闭合、
死亡告终。历史永不改写；纠正是旧事件旁边的新事件。

锚只被检查一件事：**指针能否解析**——commit 存在吗、文件里有没有那一行。`resolves` 是
关于指针的事实，永远不是对声称的裁决。从你自己的 commits 自动派生的证据标 **⊙**（自证）；
独立归档的锚标 **✓**——两个标记永不混用，因为**证据不能自我认证**。

归档的锚会记下它锚定时的仓库状态（它的 `base`）。此后 ev 能数出**漂移**：base 之后，
所引路径被多少个 commit 改动过（`test:`、`file:`、`artifact:` 这些带路径的锚；自动捕获的
commit 尾气不带 base——commit 本身就是自己的不动点）。漂移以世界的移动计量，不以时钟——
一条锚可以依然解析，而它支撑的声称已经在底下悄悄过时。引擎负责数数；数字意味着什么，
是人的判断。

`status` 和漂移并排读，就是一个 **cell**：`still`（数过了，且是零）· `neighborhood-moved`
（引用的那一行还在，代码在它旁边动了）· `anchor-changed` · `file-gone` · `legacy`（一个
0.2.3 之前的状态，不重新核验就无法归类）。漂移数不出来时不出 cell——没有 cell 意味着 ev
什么都没断言。`neighborhood-moved` 让锚的盲区现形：大多数对调用方可见的缺陷，是靠在被引
那一行**旁边**加代码修好的，这恰好让内容锚保持绿色。ev 无法告诉任何人一条声称是否被
修好了——只能说锚下面的地面动了，这是提示重读，从来不是裁决。

一条声称还可以带一份 **reading**：agent 在理解深度（`maintainer`——声称本身；`plain`——非
作者的读法；`ground`——假设零背景）和语言（`zh`/`en`）两个维度上填的指针，让写给维护者
的声称不再是唯一的入口。ev 只存指针，从不存解释本身，并如实指出哪些格子还空着——是事
实，不是打分。在暂停里，`>` 往深处钻一层、`~` 切换语言；一条认知负债提示——「上次理
解是 N 个 commit 之前——需要重读」——只在这条声称锚定的代码在人上次查看之后动过时才出现。

判断发生在暂停里：要证据、当场附上、灰置、ack 一句依然成立、或者放手让它死。闭合是一个
独立而刻意的动作——`ev close <id>`，给配得上的声称。攒下来的是线——两个裸计数，永远没有分数。

## 它拒绝做的事

- **事实，不是裁决。** 引擎检查指针能否解析、世界在它下面漂了多远——从不评判背后的工作
  好不好，从不问模型，从不碰网络。证据是否覆盖了承诺，是人在暂停时的判断。
- **什么都不 gate。** 会话钩子永远成功；仅有的拒绝发生在你自己的动词上——没有证据的闭合
  会被拒绝，因为*反正就关了*这种事不该存在。
- **没有理解度打分。** ev 从不生成、补全或评判一份解释。`reading` 只存指针；ev 只报告哪
  些格子是空的，从不评判填了的那格好不好。
- **只有人能闭合。** agent 可以提声称、附证据。关门是你的。
- **没有守护进程。** 状态只在你调用 `ev` 时刷新，从不在后台。

## 给 agents

跑着 `ev` 的仓库带一份 [`AGENTS.md`](AGENTS.md)，告诉任何 coding agent 怎么提出带证据的
声称、怎么回应一次 demand。

## 设计

内部机制——追加式账本、折叠、锚解析与漂移、sweep、暂停——见
[`docs/design.md`](docs/design.md)（英文）。

## 许可证

Apache-2.0。
