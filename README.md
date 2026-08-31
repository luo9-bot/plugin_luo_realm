# Luo Realm

Luo Realm（LR）是面向 `luo9_bot` 的 Rust 修行世界插件。运行时完全使用 Rust，SQLite 是动态状态的唯一可信来源；Python 仅用于一次性迁移旧插件数据。

## 功能

- 11 个独立修行体系：修真、剑修、体修、法修、灵修、气修、血魔邪修、阵修、丹器修、召唤流、音修。
- 境界、机缘、战力计算和确定性战斗。
- 显式角色注册、体系锁定、签到、钱包流水、玩家资料、排行和逐回合决斗记录。
- SQLite 外键、事务、WAL 恢复、幂等奖励和在线备份。
- PNG 角色卡片与 GIF 战斗过程。
- 群白名单、群级功能开关和仅管理员可用的私聊入口。
- 内嵌 Web 管理后台，可管理玩家、钱包、修行、物品、统计、备份与审计。

## 构建

项目使用 Rust 2024 edition，并依赖 `luo9_bot` Rust SDK：

```toml
luo9_sdk = "0.7.1"
```

在其他机器构建时需修改为实际 SDK 路径。

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

DLL 产物：

```text
target/release/luo_realm.dll
```

插件不会自动复制 DLL 到 bot 目录。

## 访问控制

群聊默认全部禁用。只有在管理后台登记并启用的群才能触发命令；每个群还可独立关闭
通用命令、每日事件或战斗。群门禁在命令解析和玩家创建之前执行，未知群不会产生玩家数据。

私聊默认关闭，仅 `config/config.toml` 中 `admin.admin_ids` 列出的 QQ 可以执行命令。
私聊管理员权限不能绕过后台认证，两者使用不同的身份边界。

```toml
[admin]
enabled = true
bind = "0.0.0.0"
port = 18765
admin_ids = [123456789]
```

## 管理后台

后台默认监听 `0.0.0.0:18765`。如果端口占用，会依次尝试
`18765` 到 `18774`，共十个端口。实际端口写入：

```text
data/luo_realm/admin.runtime.json
```

首次启动自动生成至少 256 bit 随机 Token：

```text
data/luo_realm/admin.token
```

浏览器打开实际监听地址后输入该 Token。Token 仅保存在当前标签页的 `sessionStorage`，
可在运维页面轮换。除健康检查和登录外，所有 API 都强制 Bearer Token；写操作要求原因，
停用群、扣款、大额钱包调整、修行修改、素材覆盖、数据恢复、永久删除玩家和 Token 轮换
还要求服务端目标确认。

后台包含群聊、玩家、素材、数据、设置和审计工作区。素材库可分页预览、上传、删除并以
ZIP 整体导入导出。素材路径严格限制在 `assets/realm` 和 `assets/fonts/font.ttf`，单文件
最大 16 MiB，压缩包解压后最大 128 MiB；PNG 在写入前会校验格式、尺寸和解码内存上限。
整包导入先复制到 staging 目录，校验完成后再交换素材目录；中断或审计失败会恢复旧目录。

数据工作区可下载一致性 SQLite 快照，也可恢复由 Luo Realm 导出的同版本快照。恢复前会
检查 SQLite 文件头、数据库完整性、外键和 schema 版本，并自动将当前数据库在线备份到：

```text
data/luo_realm/backups/
```

恢复通过 SQLite Backup API 写入正在运行的 WAL 数据库，不直接替换主数据库文件。失败时
会立即使用导入前备份回滚；导入期间还会持久化 pending 标记，进程突然中断后会在主库
重新打开前自动判定回滚或清理已提交状态。永久删除玩家会在一个事务内清理钱包、突破、
机缘和相关战斗记录，审计日志保留删除摘要。

后台直接绑定 `0.0.0.0` 且只提供 HTTP，仅应在受信局域网使用。不要将端口直接暴露到
互联网；远程访问必须配置 HTTPS 反向代理、防火墙和来源限制。Windows 部署时还应限制
插件数据目录的账户访问权限，避免 `admin.token` 被其他本机用户读取。

## 数据库

数据库位于插件目录下：

```text
data/luo_realm/luo_realm.sqlite3
```

插件依次使用以下目录来源：

1. `LUO_REALM_PLUGIN_DIR`
2. `LUO9_PLUGIN_DIR`
3. `LUO9_PLUGIN_PATH`
4. `PLUGIN_DIR`
5. 宿主当前工作目录

SQLite 启用：

```sql
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;
PRAGMA synchronous = FULL;
PRAGMA busy_timeout = 5000;
```

所有写命令使用 `BEGIN IMMEDIATE`。只有提交成功后才回复操作成功。插件异常退出后，SQLite 会恢复已提交事务并丢弃未提交事务。

管理后台每个请求使用短生命周期连接。钱包、玩家、修行、物品、统计和群配置修改与成功
审计记录在同一事务中提交。即使进程突然终止，WAL 也不会留下半次提交；下次打开数据库时
SQLite 会自动恢复。配置和 Token 使用 `.new/.bak` 可恢复替换，启动时会处理上次中断残留。

不要手动删除 `-wal`、`-shm` 文件，也不要在运行时只复制主数据库。在线备份应调用 `Database::backup_to`，它使用 SQLite Backup API。

## 命令

命令默认无前缀，也接受 `/lr`。将部署目录下 `config/config.toml` 的
`command.prefix_enabled` 改为 `true` 后，只有配置前缀开头的命令会被处理：

```text
注册 <名称>
菜单
体系
选择体系 <体系标识>
战力
状态
今日状态
签到
每日事件
世界事件
决斗 <QQ>
排行
改名 <名称>
```

角色创建分为两个阶段：先发送 `注册 <名称>` 登记名称，再发送 `选择体系 <体系标识>`
完成注册。体系首次确定后锁定，不能通过重复命令无代价更换。待选体系角色可以查看菜单、
体系并改名，但签到、状态、战力、事件、排行和决斗均要求完整注册。决斗双方都必须是已启用、
已完成体系选择的角色。

未注册、待选体系和已停用角色会分别收到明确提示。任何普通玩法命令都不会隐式创建玩家，
未知文本也不会写入数据库。

玩家当天首次执行有效玩法命令时，会根据修行数据、连续签到、最近战斗、个人机缘和往期
状态生成并固定“今日状态”。状态以有限幅度修正生命、攻击、防御、速度、暴击和机缘，
同一天的战力与决斗不会因后续活动重新计算状态。

每个已启用群每天会生成一个世界事件。群员首次签到、首次取得个人机缘和完成决斗会自动
推进对应目标；每名玩家每天的签到与机缘各计一次，决斗最多计三次。事件完成后，金币与
刻印奖励会在同一事务中自动发给当天贡献者，重复请求或插件重启不会重复结算。

示例：

```text
/lr 战力
/lr 注册 洛玖
/lr 选择体系 sword
/lr 决斗 123456789
/lr 今日状态
/lr 世界事件
```

## 角色卡与战斗动画

角色卡和战斗 GIF 使用固定 `960 × 540` 画布，显示 LR 形象、境界徽章、体系、境界、
属性、货币、胜负、真实生命变化、技能和暴击信息。每次出手的完整帧同时写入
`combat_rounds.frame_json`，动画不再根据最终结果反推中间生命值。
默认只发送战斗 GIF，不附加逐回合文字。后台可以设置全局默认值，每个群还可选择跟随
全局、强制开启或强制关闭；GIF 生成失败时返回精简胜负摘要。
每次动作包含蓄势和命中阶段，并表现攻击位移、受击抖动、技能特效、暴击跳字、生命变化
和终局胜负。渲染器会预加载本场人物与技能资源，并使用快速 GIF 量化避免阻塞消息处理。

运行时资源统一位于插件目录：

```text
assets/realm/
├── portraits/
├── realm_badges/
├── skill_icons/
├── skill_effects/
├── equipment/
├── item_rarities/
├── monsters/
├── shop_characters/
├── enhancement_items/
├── true_damage/
└── ui/
```

中文字体可放在
`assets/fonts/font.ttf`；Windows 下也会自动尝试系统中文字体。资源或字体不可用时使用
几何降级界面；渲染失败时保留已提交的业务结果并返回完整文字信息。

旧格式 INI/JSON 仅是离线迁移输入，不属于运行时资源。完成迁移后应归档在插件目录之外，
不得从插件代码读取或继续作为配置源。

## 修行体系目录

每个体系位于 `src/cultivation/<system>`，统一包含：

```text
mod.rs
attributes.rs
realms.rs
skills.rs
mechanics.rs
balance.rs
```

上层通过 `CultivationSystem` 接口调用体系。新增体系时不得在消息入口写体系类型判断。

## 迁移旧数据

迁移工具只读 GB18030 旧文件，将动态数据写入 SQLite，将 Boss、装备、技能和战斗奖励定义转换为 JSON。每个非空旧字段都会进入迁移审计表。

```powershell
python tools/legacy_migration.py inspect ./OldData

python tools/legacy_migration.py migrate `
  ./OldData `
  H:\path\to\luo-realm-plugin

python tools/legacy_migration.py verify `
  ./OldData `
  H:\path\to\luo-realm-plugin\data\luo_realm\luo_realm.sqlite3
```

生成的静态配置位于 `config/legacy`。原始旧文件不会被修改。

## 项目结构

```text
src/database/       SQLite 连接、迁移和领域操作
src/admin/          Token、HTTP API 和内嵌管理页面
src/cultivation/    11 个独立修行体系
src/engine/         机缘、事件和战力计算
src/command/        命令解析、注册门禁和玩法分发
src/render/         旧素材定位、角色卡和战斗动画
migrations/         唯一 SQLite schema 来源
tools/              离线迁移工具
```

宗门、地图、网页大世界和 AI 对话属于后续独立项目。
