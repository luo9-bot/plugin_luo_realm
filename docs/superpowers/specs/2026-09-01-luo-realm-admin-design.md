# Luo Realm 群访问控制与管理后台设计

## 1. 目标与边界

本阶段为 Luo Realm 增加群级访问控制和内嵌 Web 管理后台。后台绑定
`0.0.0.0`，使用强制 Token 认证；地图、宗门、AI 对话和独立前端服务不在本阶段。

运行时默认拒绝全部群聊和私聊：

- 只有 SQLite `groups.enabled = 1` 的群可以执行游戏命令。
- 未登记或已停用的群直接忽略，不创建玩家、签到、机缘或战斗数据。
- 私聊仅允许配置中的管理员 QQ 使用，普通用户私聊直接忽略。
- 访问检查发生在命令解析和任何数据库写入之前。

## 2. 配置与启动

`config/config.toml` 增加：

```toml
[admin]
enabled = true
bind = "0.0.0.0"
port = 18765
admin_ids = []
```

后台从端口 `18765` 开始绑定，按顺序尝试 `18765..=18774`，总计十次。只有地址占用
错误才尝试下一端口；其他绑定错误立即停止后台线程。后台启动失败不终止机器人消息线程。

绑定成功后，将当前进程、监听地址、实际端口和启动时间原子写入
`data/luo_realm/admin.runtime.json`。该文件仅用于定位当前后台，不作为锁或认证依据。

## 3. Token 与认证

首次启动使用操作系统安全随机源生成 32 字节随机值，以 URL-safe 文本写入
`data/luo_realm/admin.token`。文件采用独占创建；已有文件不会被覆盖。Unix 平台设置
`0600` 权限，Windows 使用当前账户文件权限并在文档中提示限制目录访问。

除 `GET /api/health`、后台静态页面和 `POST /api/login` 外，所有 API 要求
`Authorization: Bearer <token>`。服务端只保存 Token 的 SHA-256 摘要用于比较，比较采用
固定时间逻辑，日志和审计表不得记录 Token 明文。

登录成功后浏览器将 Token 保存在当前标签页的 `sessionStorage`，关闭标签页后失效。
Token 修改接口接收管理员明确输入的新 Token，要求至少 32 个字符。页面可通过 Web Crypto
在浏览器本地生成新 Token，用户确认已保存后提交；修改采用临时文件、刷盘和原子替换，旧
Token 在替换成功后立即失效。

后台使用纯 HTTP，因此只适用于受信局域网。跨公网访问必须由用户在前方配置 HTTPS 反向
代理；README 明确警告不得直接把管理端口暴露到互联网。

## 4. 数据库迁移

新增 `migrations/0002_admin.sql`，保留现有迁移不变。新增表：

- `group_features(group_id, feature_code, enabled, updated_at)`：群级功能开关。
- `runtime_settings(setting_key, setting_value, updated_at)`：可热更新的全局设置。
- `admin_audit_log`：操作者、动作、目标类型、目标 ID、原因、修改前后 JSON、时间和结果。

`groups.enabled` 是群总开关。不存在的群等价于关闭。后台显式新增群时必须指定启用状态，
不得依赖旧 schema 的默认值。

管理员对钱包、玩家、修行、物品和统计的修改都在 `BEGIN IMMEDIATE` 事务中完成，业务
修改与成功审计记录同事务提交。失败操作记录不包含敏感值，并通过独立短事务写入失败原因。
钱包只能调用现有钱包领域接口，使用后台请求生成的唯一幂等键，禁止直接更新余额表。

## 5. Rust 模块

```text
src/admin/
├─ mod.rs          服务启动、端口探测和线程入口
├─ auth.rs         Token 创建、验证和轮换
├─ router.rs       HTTP 路由、统一响应和输入限制
├─ handlers.rs     仪表盘、群、玩家、配置、备份和审计 API
└─ ui.rs           内嵌 HTML/CSS/JavaScript

src/database/
├─ admin.rs        审计、概览和运行时设置
└─ group.rs        群总开关及功能开关
```

后台每个请求打开一个短生命周期 SQLite 连接，并应用与主连接相同的 WAL、外键、FULL
同步和 busy timeout 配置。请求不得跨响应持有事务。机器人消息线程保留现有连接，双方通过
SQLite WAL 协调并发。

## 6. API

所有 JSON 响应使用 `{ "ok": true, "data": ... }` 或
`{ "ok": false, "error": { "code": "...", "message": "..." } }`。
请求体限制为 1 MiB，文本字段和分页参数有明确上限。

- `GET /api/health`：只返回版本、服务状态和数据库是否可用。
- `POST /api/login`：验证 Token，不返回 Token。
- `GET /api/overview`：玩家、启用群、战斗、钱包和数据库健康概览。
- `GET/POST /api/groups`：分页查询或新增群。
- `PUT /api/groups/{id}`：启用或停用群。
- `GET/PUT /api/groups/{id}/features`：群级命令、事件和战斗开关。
- `GET /api/players`、`GET /api/players/{id}`：搜索和查看玩家。
- `PUT /api/players/{id}/profile`：名称和资料。
- `POST /api/players/{id}/wallet`：钱包调整。
- `PUT /api/players/{id}/cultivation`：体系、境界和进度。
- `POST/DELETE /api/players/{id}/items`：物品增删。
- `PUT /api/players/{id}/statistics`：统计修正。
- `GET/PUT /api/config`：允许热更新的全局配置。
- `GET/PUT /api/definitions/{kind}`：事件、奖励、Boss、技能和物品静态定义。
- `GET /api/audit`：分页筛选审计日志。
- `POST /api/backup`：调用 SQLite Backup API 创建在线备份。
- `POST /api/token/rotate`：提交并启用新 Token。

写接口要求 `reason`，高风险请求还要求 `confirm` 与服务端目标摘要一致。仅依赖页面弹窗不算
二次确认，服务端必须验证确认字段。

## 7. 管理页面

页面采用内嵌单文件 HTML/CSS/JavaScript，不引入 Node、前端框架或 CDN。界面为安静、紧凑
的运维后台：固定侧栏、顶部连接状态和内容工作区，不使用营销式 Hero 或装饰性大卡片。

页面包括：

- 登录页：Token 输入和服务地址。
- 仪表盘：运行状态、实际端口、数据库健康、玩家与群统计、最近错误。
- 群管理：搜索、批量录入、总开关和功能开关。
- 玩家管理：资料、钱包、修行、物品、机缘与统计标签页。
- 世界配置：体系、事件、奖励、Boss、技能、装备及命令配置编辑器。
- 运维：在线备份、审计日志、Token 轮换和运行信息。

删除物品、降低境界、大额钱包调整、停用群和轮换 Token 使用确认对话框，显示目标、当前值、
新值和原因。页面在请求进行中禁用重复提交，并展示可恢复的错误状态。

## 8. 群消息数据流

```text
收到消息
  -> 判断消息类型
  -> 群聊：查询 groups.enabled 和 group_features
  -> 私聊：检查 admin_ids
  -> 拒绝则静默返回
  -> 解析前缀与命令
  -> 执行业务事务
  -> COMMIT
  -> 发送成功回复
```

群总开关关闭时，不允许任何游戏命令。后台不通过机器人消息入口，因此不受群总开关影响。

## 9. 错误与恢复

- Token 文件创建、读取或校验失败时，后台拒绝启动，游戏消息线程继续运行。
- 数据库完整性失败时，后台只允许健康检查、只读概览和 Backup API，所有写接口返回只读故障。
- 端口十次均失败时删除本次进程生成的 runtime 文件并记录尝试范围。
- API 不返回 SQL、绝对路径、Token、私钥或 Rust 调用栈。
- 在线备份只使用 SQLite Backup API，成功后执行完整性检查并记录审计。

## 10. 测试与验收

Rust 测试覆盖：

- 未登记群和停用群不产生任何玩家或领域数据。
- 启用群可以执行命令，单项功能关闭时对应命令被拒绝。
- 普通用户私聊被忽略，管理员私聊可执行。
- 首次 Token 创建、重复启动不覆盖、错误 Token 拒绝和轮换后旧 Token 失效。
- 基础端口占用后选择下一端口，以及连续十个端口占用后停止后台。
- API 未认证、非法输入、请求体过大和高风险确认缺失。
- 钱包、修行、物品及群修改与审计记录原子提交。
- 并发后台请求与机器人命令不破坏 WAL 数据一致性。
- Backup API 产物可独立打开并通过完整性检查。

最终执行 `cargo fmt --check`、Clippy 零警告、完整测试和 release 构建。使用本地 HTTP 请求
验证页面、登录、群开关和玩家修改流程；不向 bot 安装目录部署产物。
