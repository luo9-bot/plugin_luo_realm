# Luo Realm

Luo Realm（LR）是面向 `luo9_bot` 的 Rust 修行世界插件。运行时完全使用 Rust，SQLite 是动态状态的唯一可信来源；Python 仅用于一次性迁移旧插件数据。

## 功能

- 11 个独立修行体系：修真、剑修、体修、法修、灵修、气修、血魔邪修、阵修、丹器修、召唤流、音修。
- 战斗 2.0：基于 `bevy_ecs` 的确定性 ECS 战斗运行时，包含行动条、吟唱蓄力、位移距离、控制韧性递减、护盾、格挡、闪避、治疗、召唤、领域、装备触发和超时裁决。
- 技能、战术与装备：11 体系差异化技能、5 种战术预设、技能熟练度、八个装备槽位与词条触发，以及每日一次的主要修行动作。
- 显式角色注册、体系锁定、签到、钱包流水、玩家资料、排行和结构化战斗事件记录。
- SQLite 外键、事务、WAL 恢复、幂等奖励和在线备份。
- PNG 角色卡片与事件驱动的动态分屏战斗 GIF。
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

消息处理按玩家身份固定分配到四个有界工作队列；同一玩家的命令保持顺序，御空试炼的
云端激活超时不会阻塞其他分片的群聊消息。队列满时会拒绝该分片的新消息并记录错误，
同时明确回复“命令队列繁忙”，避免请求洪峰耗尽线程或内存。同一玩家每 15 秒只能签发
一个御空试炼链接，云端请求最多等待 2 秒。

## 访问控制

群聊默认全部禁用。只有在管理后台登记并启用的群才能触发命令；每个群还可独立关闭
通用命令、每日事件或战斗。群门禁在命令解析和玩家创建之前执行，未知群不会产生玩家数据。

私聊默认关闭，仅 `data/luo_realm/config/config.toml` 中 `admin.admin_ids` 列出的 QQ 可以执行命令。
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
ZIP 整体导入导出。素材路径严格限制在 `data/luo_realm/assets/realm` 和
`data/luo_realm/assets/fonts/font.ttf`，单文件
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

命令默认无前缀，也接受 `/lr`。将 `data/luo_realm/config/config.toml` 的
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
技能 [技能ID 研习]
战术 [战术代码]
装备 [穿戴 <物品编号> <槽位> | 卸下 <槽位>]
修行行动 [行动]
主页
御空试炼
兑换 <兑换码>
排行
改名 <名称>
```

角色创建分为两个阶段：先发送 `注册 <名称>` 登记名称，再发送 `选择体系 <体系名称>`
完成注册。体系名称直接使用中文（如 `选择体系 剑修`），英文标识仍然可用。体系首次确定后
锁定，不能通过重复命令无代价更换。待选体系角色可以查看菜单、体系并改名，但签到、状态、
战力、事件、排行和决斗均要求完整注册。决斗双方都必须是已启用、已完成体系选择的角色。

## 群内图片界面

`菜单`、`体系`、`状态`、`技能`、`装备`、`每日事件` 和 `世界事件` 默认返回图片卡片，
不附带文字：菜单卡按场景分组列出全部命令，体系卡展示 11 个体系的定位，技能卡带熟练度
刻度，装备卡以素材库图标呈现八槽位与背包网格，物品详情卡展示稀有度星级与词条，机缘卡与
世界事件卡展示当日事件与目标进度。卡片渲染失败时自动回退为文字回复，不影响权威结果。
样例可用 `cargo run --example render_samples` 输出到 `tests/img` 人工查看。

战斗与成长命令说明：

- `技能` 列出已掌握技能和熟练度；`技能 <技能ID> 研习` 提升指定技能熟练度（0 至 3），
  例如 `技能 sword.basic 研习`。
- `战术` 列出可选战术；`战术 <代码>` 设置决斗自动战术，可选
  `balanced / aggressive / defensive / sustain / control`。
- `装备` 查看背包和穿戴状态；`装备 查看 <物品编号>`（或直接 `装备 <编号>`）查看单件物品
  详情卡（稀有度、星级、强化等级与词条）；`装备 穿戴 <物品编号> <槽位>` 穿戴，
  `装备 卸下 <槽位>` 卸下。槽位代码为 `main_hand / off_hand / head / body / hands / feet /
  accessory_1 / accessory_2`。
- `修行行动 [行动]` 是每天一次的主要修行动作，可选 `吐纳 / 闭关 / 研习 / 历练 / 淬炼 / 休养`
  （默认 `吐纳`），影响修为进度、熟练度、疲劳、伤势和心境。
- `主页` 签发一次性网页票据并返回专属档案页链接（10 分钟内有效、仅可打开一次）。

## 玩家网页（Vue 3 · 只读档案页）

`主页` 命令返回形如 `{base_url}?ticket=...` 的专属链接。页面使用 Vue 3 + TypeScript +
Vite 构建（源码位于 `player_page/`），以游戏化深色界面展示角色卡、装备栏与背包（带素材
图标）、技能熟练度、资产流水和最近战斗。

构建与部署：

```powershell
cd player_page
npm install
npm run build            # 产物在 player_page/dist
Copy-Item -Recurse -Force player_page\dist\* data\luo_realm\player_page\
```

插件优先伺服 `data/luo_realm/player_page/` 下的构建产物；目录不存在时回退为内嵌的精简
页面。部署到 Cloudflare Pages 时直接发布 `dist`，并以 `VITE_API_BASE` 指定插件 API 地址
（跨域来源需加入 `[player_web].allowed_origins`）。

安全模型（设计方案书 20.3、27.2）：

- 票据绑定玩家与 `profile:read` 范围，一次性使用，nonce 持久化于 `player_web_tickets`。
- 会话为无状态 HMAC 签名值，默认 2 小时有效；轮换管理 Token 会立即失效全部票据与会话。
- 数据接口全部只读；唯一的网页写入入口是「更换形象」（外观类，写审计），不触及数值资产。
- `/api/player/asset/` 提供装备图标与角色立绘（与群内卡片同一挑选规则），仅暴露游戏素材。
- 玩家可在网页「角色卡」页直接从素材库选择形象并保存，群内卡片即时同步。

启用方式：把 `data/luo_realm/config/config.toml` 中 `[player_web].enabled` 改为 `true`，
并将 `base_url` 指向可访问的页面地址（本地默认 `http://127.0.0.1:18765/player`）。本地联调可运行
`cargo run --example player_web_dev`，它会启动一个种子角色齐全的演示服务器并打印测试票据链接。

## Cloudflare 双层部署指南（源站不暴露）

部署形态与御空试炼同源：**插件只发起出站请求**，把档案快照推送到 Cloudflare Worker
（写入 D1）；玩家页面是纯静态站点（Pages），读取只来自 Cloudflare；写操作由 Worker 用
**环境变量中隐藏的源站地址**转发回插件。插件服务器不需要公网 IP、不开任何入站端口。
不部署此页面时插件完整可用——页面是可选增强。

```text
插件 ──出站推送(快照)──▶ CF Worker ──▶ D1 ◀──页面读取
页面 ──写操作──▶ CF Worker ──(env PLUGIN_URL, 隐藏)──▶ 插件 /api/player/command
```

鉴权链（三层，全部严格校验）：

1. 插件 → Worker：`Authorization: Bearer <SYNC_TOKEN>`（≥32 字符高熵，常量时间比较）。
2. 页面 → Worker：页面令牌（256 位随机、D1 校验有效期）。
3. Worker → 插件：同一 `SYNC_TOKEN` 常量时间校验 + 页面令牌在插件本地会话表复核，
   动作白名单（当前仅 `set_character`），全部写审计；SQL 全部预编译。

部署步骤：

```powershell
# 1. 创建 D1 并建表
npx wrangler d1 create luo-realm-page
npx wrangler d1 execute luo-realm-page --file player_page/worker/schema.sql --remote

# 2. 部署 Worker（复制 wrangler.toml.example 填入 database_id）
Copy-Item player_page\worker\wrangler.toml.example player_page\worker\wrangler.toml
#   编辑 wrangler.toml：database_id、PLUGIN_URL（插件 API 的内网/公网 HTTPS 地址）
npx wrangler deploy
npx wrangler secret put PLUGIN_TOKEN     # 生成 ≥32 字符随机串，与插件保持一致

# 3. 构建并发布静态页面
cd player_page
npm install
$env:VITE_API_BASE = ""                  # 页面与 Worker 同域时可留空
$env:VITE_DATA_MODE = "cf"               # 走快照/回传通道
npm run build
npx wrangler pages deploy dist --project-name luo-realm
```

第四步，插件配置（`data/luo_realm/config/config.toml`）：

```toml
[player_web]
enabled = true
base_url = "https://luo-realm.pages.dev"   # 群内「主页」链接指向 Pages
sync_url = "https://<worker 地址>/api/plugin/sync"
sync_token = "<与 Worker 相同的 ≥32 字符随机串>"
ticket_ttl_minutes = 10
session_ttl_minutes = 120
```

保存配置后在群里发送 `主页` 验证：链接指向 Pages 并携带 `?token=`，页面应显示已推送的
档案快照；「更换形象」应成功并同步回群内卡片。本地直连模式（不配 `sync_url`）保持原有
票据流程不变。

安全清单：

- `PLUGIN_TOKEN` 走 `wrangler secret put`，绝不写进仓库、前端或日志。
- `PLUGIN_URL` 只存在于 Worker 环境变量（对玩家不可见），必须 HTTPS、无路径无查询。
- Worker 转发只允许固定路径；所有 SQL 预编译；所有令牌常量时间比较。
- 含 `token`/`ticket` 的链接等同钥匙，不要转发到群聊之外。
- 管理 Token 与数据库只存在于插件服务器，绝不写入任何前端配置。

未注册、待选体系和已停用角色会分别收到明确提示。任何普通玩法命令都不会隐式创建玩家，
未知文本也不会写入数据库。

玩家当天首次执行有效玩法命令时，会根据修行数据、连续签到、最近战斗、个人机缘和往期
状态生成并固定“今日状态”。状态以有限幅度修正生命、攻击、防御、速度、暴击和机缘，
同一天的战力与决斗不会因后续活动重新计算状态。

每个已启用群每天会生成一个世界事件。群员首次签到、首次取得个人机缘和完成决斗会自动
推进对应目标；每名玩家每天的签到与机缘各计一次，决斗最多计三次。事件完成后，金币与
刻印奖励会在同一事务中自动发给当天贡献者，重复请求或插件重启不会重复结算。决斗胜者的
金币奖励同样按日封顶：每天最多 3 次入账，之后的决斗仍记录战斗与贡献，但不重复发钱。

示例：

```text
/lr 战力
/lr 注册 洛玖
/lr 选择体系 sword
/lr 修行行动 闭关
/lr 技能 sword.basic 研习
/lr 战术 aggressive
/lr 装备 穿戴 42 main_hand
/lr 决斗 123456789
/lr 今日状态
/lr 世界事件
/lr 御空试炼
/lr 兑换 eyJhbGciOiJFZERTQSIs...
```

御空试炼会生成形如 `随机票据.ascii-fpv.luo-realm.drluo.top` 的两小时专属网址。
插件会先在 Cloudflare D1 激活新会话，成功后才返回链接；同一玩家再次生成链接会立即
使旧会话失去起飞和结算权限。游戏可以无限游玩，每局结束后由云端签发绑定玩家的
Ed25519 兑换码；兑换码 24 小时有效，插件只保存公钥，因此逆向插件无法伪造兑换码。
兑换次数由 SQLite 按玩家和日期限制，默认每天 3 次。
完整部署步骤见 `https://github.com/luo-realm-webgames/ascii-fpv/README.md`。

## 角色卡与战斗动画

角色卡使用固定 `960 × 540` 画布，显示 LR 形象、境界徽章、体系、境界、属性和货币信息。
战斗形象由玩家资料中的 `character_id` 指定；缺少素材时使用明确的降级界面，不以几何人物
替身冒充自定义形象。

决斗开始前，应用层在事务内创建不可变 `CombatSnapshot`（双方属性、技能配置、装备、资源
和战术），交给基于 `bevy_ecs` 的战斗运行时推演。运行时输出带单调序号的结构化
`CombatEvent` 事件流和 `CombatOutcome` 结果，在同一事务中写入 `combat_records`、
`combat_participants` 和 `combat_events`。相同快照、规则版本和随机种子产生完全相同的
事件流和结果；战斗不会无限循环，达到时间片上限时按存活、生命比例和有效伤害裁决。

GIF 渲染由 `CombatOutcome` 驱动节拍，使用全局调色板和差量帧编码，纯 CPU 1 秒内出图：
人物立绘完整显示，暴击触发镜头缩放，技能图标与特效素材按事件叠加，彗星弹道、命中纹章、
伤害跳字和状态面板跟随目标单位，顶部 HUD 显示双方生命与资源。默认只发送战斗 GIF，
不附加逐回合文字；渲染失败或素材缺失时返回精简胜负摘要，不影响已提交的权威结果。
后台可以设置全局战报默认值，每个群还可选择跟随全局、强制开启或强制关闭。

运行时资源统一位于插件数据目录（旧版本根目录的 `assets/`、`config/` 会在启动时自动
迁移到新布局）：

```text
data/luo_realm/assets/realm/
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

中文字体可放在 `data/luo_realm/assets/fonts/font.ttf`；Windows 下也会自动尝试系统中文
字体。资源或字体不可用时使用几何降级界面；渲染失败时保留已提交的业务结果并返回完整
文字信息。

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

上层通过 `CultivationSystem` 接口调用体系。战斗侧的技能、体系资源类型和效果模板由
`src/combat/catalog.rs` 统一定义，11 个体系各自拥有差异化的防御、恢复、控制、机动、
终极和超阶技能组。新增体系时不得在消息入口写体系类型判断。

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
src/combat/         bevy_ecs 战斗运行时、快照模型和 11 体系技能目录
src/database/       SQLite 连接、迁移和领域操作
src/admin/          Token、HTTP API 和内嵌管理页面
src/cultivation/    11 个独立修行体系
src/engine/         机缘、事件和战力计算
src/command/        命令解析、注册门禁和玩法分发
src/render/         素材定位、角色卡和事件驱动战斗 GIF
src/equipment.rs    装备槽位、词条预算和战斗触发
migrations/         唯一 SQLite schema 来源
tools/              离线迁移工具
```

宗门、地图、网页大世界和 AI 对话属于后续独立项目。
