# Luo Realm Admin Console Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 Luo Realm 增加默认拒绝的群/私聊访问控制，以及具备强认证、端口回退、完整配置和数据管理能力的内嵌 Web 后台。

**Architecture:** 机器人消息入口在解析命令和创建玩家之前查询 SQLite 群开关或管理员 QQ 配置。独立后台线程使用 `tiny_http` 监听 `0.0.0.0`，通过短生命周期 SQLite 连接执行带审计的事务；单文件静态页面通过同源 JSON API 管理群、玩家、规则、备份和 Token。

**Tech Stack:** Rust 2024、rusqlite WAL、tiny_http、serde/serde_json、toml、getrandom、sha2、内嵌 HTML/CSS/JavaScript。

**Design:** `docs/superpowers/specs/2026-09-01-luo-realm-admin-design.md`

---

## File Map

**Modify**

- `Cargo.toml`：增加后台 HTTP 与安全随机依赖。
- `config/config.toml`：增加后台监听和管理员配置。
- `migrations/0001_initial.sql`：不修改。
- `src/config.rs`：解析并校验后台配置。
- `src/database/connection.rs`：提供后台短连接和数据库路径访问。
- `src/database/group.rs`：群总开关与功能开关。
- `src/database/inventory.rs`：后台物品增删所需的所有权操作。
- `src/database/cultivation.rs`：后台修行状态更新。
- `src/database/mod.rs`：导出后台领域模块。
- `src/lib.rs`：消息访问门禁和后台线程启动。
- `README.md`：部署、安全、端口和使用说明。

**Create**

- `migrations/0002_admin.sql`
- `src/admin/mod.rs`
- `src/admin/auth.rs`
- `src/admin/router.rs`
- `src/admin/handlers.rs`
- `src/admin/ui.rs`
- `src/database/admin.rs`
- `tests/access_control.rs`
- `tests/admin_auth.rs`
- `tests/admin_database.rs`
- `tests/admin_http.rs`
- `tests/admin_ports.rs`

---

### Task 1: Add Versioned Admin Schema

**Files:**

- Create: `migrations/0002_admin.sql`
- Modify: `src/database/migrations.rs`
- Test: `tests/admin_database.rs`

- [ ] **Step 1: Write a failing migration test**

```rust
#[test]
fn admin_schema_is_migrated_once() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("lr.sqlite3")).unwrap();

    assert_eq!(database.schema_version().unwrap(), 2);
    for table in ["group_features", "runtime_settings", "admin_audit_log"] {
        assert!(database.table_exists(table).unwrap(), "missing {table}");
    }

    drop(database);
    let reopened = Database::open(directory.path().join("lr.sqlite3")).unwrap();
    assert_eq!(reopened.schema_version().unwrap(), 2);
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `cargo test --test admin_database admin_schema_is_migrated_once`

Expected: FAIL because schema version remains `1`.

- [ ] **Step 3: Add the second migration**

```sql
CREATE TABLE group_features (
    group_id INTEGER NOT NULL REFERENCES groups(group_id) ON DELETE CASCADE,
    feature_code TEXT NOT NULL,
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (group_id, feature_code)
);

CREATE TABLE runtime_settings (
    setting_key TEXT PRIMARY KEY,
    setting_value TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE admin_audit_log (
    audit_id INTEGER PRIMARY KEY,
    operator TEXT NOT NULL,
    action_code TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id TEXT NOT NULL,
    reason TEXT NOT NULL,
    before_json TEXT,
    after_json TEXT,
    result TEXT NOT NULL CHECK (result IN ('success', 'failure')),
    created_at INTEGER NOT NULL
);

CREATE INDEX admin_audit_created_at
ON admin_audit_log(created_at DESC);
```

Add migration metadata in `migrations.rs` and execute each unapplied migration in its own transaction:

```rust
const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("../../migrations/0001_initial.sql")),
    (2, include_str!("../../migrations/0002_admin.sql")),
];
```

- [ ] **Step 4: Verify migration behavior**

Run: `cargo test --test admin_database admin_schema_is_migrated_once`

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add migrations/0002_admin.sql src/database/migrations.rs tests/admin_database.rs
git commit -m "feat: add admin database schema"
```

---

### Task 2: Enforce Group and Private Message Access

**Files:**

- Modify: `src/config.rs`
- Modify: `config/config.toml`
- Modify: `src/database/group.rs`
- Modify: `src/lib.rs`
- Test: `tests/access_control.rs`

- [ ] **Step 1: Write failing access-control tests**

```rust
#[test]
fn unknown_and_disabled_groups_are_rejected_before_player_creation() {
    let fixture = Fixture::new();

    assert_eq!(fixture.route_group(100, 200, "签到").unwrap(), None);
    assert_eq!(fixture.player_count(), 0);

    fixture.set_group(100, false);
    assert_eq!(fixture.route_group(100, 200, "签到").unwrap(), None);
    assert_eq!(fixture.player_count(), 0);
}

#[test]
fn enabled_group_and_admin_private_message_are_allowed() {
    let fixture = Fixture::new_with_admins([90001]);
    fixture.set_group(100, true);

    assert!(fixture.route_group(100, 200, "菜单").unwrap().is_some());
    assert_eq!(fixture.route_private(200, "菜单").unwrap(), None);
    assert!(fixture.route_private(90001, "菜单").unwrap().is_some());
}
```

- [ ] **Step 2: Run the tests and verify they fail**

Run: `cargo test --test access_control`

Expected: FAIL because every matching message currently reaches `handle_message_with_config`.

- [ ] **Step 3: Add validated admin configuration**

```rust
#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct AdminConfig {
    pub enabled: bool,
    pub bind: String,
    pub port: u16,
    pub admin_ids: Vec<u64>,
}

impl Default for AdminConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bind: "0.0.0.0".into(),
            port: 18_765,
            admin_ids: Vec::new(),
        }
    }
}
```

Reject empty bind values and ports above `65526`, because ten consecutive ports must fit in `u16`.

- [ ] **Step 4: Add group access queries**

```rust
pub fn is_enabled(connection: &Connection, group_id: u64) -> DatabaseResult<bool>;
pub fn set_enabled(
    transaction: &Transaction<'_>,
    group_id: u64,
    enabled: bool,
) -> DatabaseResult<()>;
pub fn feature_enabled(
    connection: &Connection,
    group_id: u64,
    feature_code: &str,
) -> DatabaseResult<bool>;
```

`is_enabled` returns `false` for a missing row. `set_enabled` explicitly writes `enabled`; it must not use the old default.

- [ ] **Step 5: Route messages through a pure access boundary**

```rust
#[derive(Clone, Copy)]
pub enum CommandFeature {
    General,
    Event,
    Combat,
}

pub enum IncomingContext {
    Group { group_id: u64 },
    Private,
}

pub fn message_allowed(
    database: &Database,
    policy: &RuntimePolicy,
    context: IncomingContext,
    user_id: u64,
    message: &str,
) -> Result<bool, DatabaseError> {
    match context {
        IncomingContext::Group { group_id } => policy.group_message_allowed(
            database.connection(),
            group_id,
            CommandFeature::from_message(message),
        ),
        IncomingContext::Private => Ok(policy.admin_ids().contains(&user_id)),
    }
}
```

`RuntimePolicy` is `Arc<RwLock<PolicyState>>` shared by the message loop and admin server. Group feature rows use
`general`、`event` and `combat`; a missing feature row defaults to enabled only after the group total switch passes.
The configuration API atomically writes `config.toml` and swaps the validated `PolicyState`, so administrator QQ
and command policy changes take effect without restart.

Call `message_allowed` in `plugin_main` before `handle_message_with_config`. A rejected message returns silently.

- [ ] **Step 6: Verify access behavior**

Run: `cargo test --test access_control`

Expected: PASS; rejected messages leave player count at zero.

- [ ] **Step 7: Commit**

```powershell
git add config/config.toml src/config.rs src/database/group.rs src/lib.rs tests/access_control.rs
git commit -m "feat: enforce group access control"
```

---

### Task 3: Implement Token Lifecycle and Port Selection

**Files:**

- Modify: `Cargo.toml`
- Create: `src/admin/mod.rs`
- Create: `src/admin/auth.rs`
- Test: `tests/admin_auth.rs`
- Test: `tests/admin_ports.rs`

- [ ] **Step 1: Add dependencies and failing tests**

Add:

```toml
tiny_http = "0.12"
getrandom = "0.3"
base64 = "0.22"
```

Tests:

```rust
#[test]
fn token_is_created_once_and_can_be_rotated() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("admin.token");

    let first = AdminToken::load_or_create(&path).unwrap();
    let initial = std::fs::read_to_string(&path).unwrap();
    let second = AdminToken::load_or_create(&path).unwrap();
    assert!(first.verify(initial.trim()));
    assert!(second.verify(initial.trim()));

    second.rotate("a-secure-replacement-token-with-32-characters", &path).unwrap();
    assert!(!second.verify(initial.trim()));
    assert!(second.verify("a-secure-replacement-token-with-32-characters"));
}

#[test]
fn port_probe_tries_exactly_ten_ports() {
    let occupied = occupy_ports(18_765..=18_773);
    let listener = bind_admin("127.0.0.1", 18_765).unwrap();
    assert_eq!(listener.port(), 18_774);
    drop(occupied);
}
```

- [ ] **Step 2: Run tests and verify they fail**

Run: `cargo test --test admin_auth --test admin_ports`

Expected: compile failure because `AdminToken` and `bind_admin` do not exist.

- [ ] **Step 3: Implement Token storage**

`AdminToken` stores only `[u8; 32]` SHA-256 digest in memory. `load_or_create` uses
`getrandom::fill`, Base64 URL-safe encoding and `OpenOptions::create_new(true)`. Rotation validates trimmed
length `>= 32`, writes a sibling temporary file, calls `sync_all`, atomically replaces the token file and only
then swaps the in-memory digest. Verification XORs every digest byte before testing equality.

```rust
pub struct AdminToken {
    digest: RwLock<[u8; 32]>,
}

pub fn load_or_create(path: &Path) -> Result<Self, AdminError>;
pub fn verify(&self, candidate: &str) -> bool;
pub fn rotate(&self, replacement: &str, path: &Path) -> Result<(), AdminError>;
```

- [ ] **Step 4: Implement exact ten-port probing**

```rust
pub fn candidate_ports(base: u16) -> impl Iterator<Item = u16> {
    base..=base + 9
}
```

Bind `0.0.0.0:<port>` using `tiny_http::Server::http`. Retry only `AddrInUse`; return other errors immediately.

- [ ] **Step 5: Run focused tests**

Run: `cargo test --test admin_auth --test admin_ports`

Expected: PASS, including all-ten-occupied failure.

- [ ] **Step 6: Commit**

```powershell
git add Cargo.toml Cargo.lock src/admin/mod.rs src/admin/auth.rs tests/admin_auth.rs tests/admin_ports.rs
git commit -m "feat: add admin token and port fallback"
```

---

### Task 4: Add Transactional Admin Database Operations

**Files:**

- Create: `src/database/admin.rs`
- Modify: `src/database/mod.rs`
- Modify: `src/database/connection.rs`
- Modify: `src/database/cultivation.rs`
- Modify: `src/database/inventory.rs`
- Test: `tests/admin_database.rs`

- [ ] **Step 1: Write failing transaction tests**

```rust
#[test]
fn wallet_adjustment_and_audit_commit_together() {
    let mut database = fixture_database();
    let transaction = database.immediate_transaction().unwrap();
    player::find_or_create(&transaction, 10001).unwrap();
    admin::adjust_wallet(
        &transaction,
        "web",
        10001,
        "coins",
        500,
        "initial correction",
        "admin:test:wallet:1",
    )
    .unwrap();
    transaction.commit().unwrap();

    assert_eq!(balance(&database, 10001, "coins"), 500);
    assert_eq!(audit_count(&database, "wallet.adjust"), 1);
}

#[test]
fn failed_player_edit_rolls_back_audit_and_state() {
    let mut database = fixture_database();
    let transaction = database.immediate_transaction().unwrap();
    let result = admin::update_cultivation(
        &transaction,
        "web",
        10001,
        "missing-system",
        999,
        0,
        "invalid request",
    );
    assert!(result.is_err());
    drop(transaction);

    assert_eq!(audit_count(&database, "cultivation.update"), 0);
}
```

- [ ] **Step 2: Run and verify failure**

Run: `cargo test --test admin_database`

Expected: FAIL because admin operations do not exist.

- [ ] **Step 3: Implement focused operations**

Create typed request/result structs and these functions:

```rust
pub fn overview(connection: &Connection) -> DatabaseResult<Overview>;
pub fn list_groups(connection: &Connection, query: &ListQuery) -> DatabaseResult<Page<GroupRow>>;
pub fn list_players(connection: &Connection, query: &ListQuery) -> DatabaseResult<Page<PlayerRow>>;
pub fn player_detail(connection: &Connection, player_id: u64) -> DatabaseResult<PlayerDetail>;
pub fn adjust_wallet(transaction: &Transaction<'_>, request: WalletAdjustment<'_>) -> DatabaseResult<()>;
pub fn update_profile(transaction: &Transaction<'_>, request: ProfileUpdate<'_>) -> DatabaseResult<()>;
pub fn update_cultivation(transaction: &Transaction<'_>, request: CultivationUpdate<'_>) -> DatabaseResult<()>;
pub fn add_item(transaction: &Transaction<'_>, request: ItemGrant<'_>) -> DatabaseResult<i64>;
pub fn remove_item(transaction: &Transaction<'_>, request: ItemRemoval<'_>) -> DatabaseResult<()>;
pub fn update_statistic(transaction: &Transaction<'_>, request: StatisticUpdate<'_>) -> DatabaseResult<()>;
pub fn list_audit(connection: &Connection, query: &AuditQuery) -> DatabaseResult<Page<AuditRow>>;
```

Every successful mutating function inserts `admin_audit_log` in the same transaction. Validate IDs, string
lengths, allowed systems, realm bounds, currency codes, quantities and statistic values before mutation.

- [ ] **Step 4: Expose short-lived admin connections**

```rust
pub fn open_request(path: impl AsRef<Path>) -> DatabaseResult<Self> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE,
    )?;
    configure(&connection)?;
    Ok(Self { connection })
}
```

This method never creates files or runs migrations. The main startup connection remains responsible for migration
and integrity checks.

- [ ] **Step 5: Run domain tests**

Run: `cargo test --test admin_database`

Expected: PASS for overview, pagination, ownership, wallet idempotency, audit and rollback cases.

- [ ] **Step 6: Commit**

```powershell
git add src/database tests/admin_database.rs
git commit -m "feat: add audited admin operations"
```

---

### Task 5: Implement Authenticated JSON API

**Files:**

- Create: `src/admin/router.rs`
- Create: `src/admin/handlers.rs`
- Modify: `src/admin/mod.rs`
- Test: `tests/admin_http.rs`

- [ ] **Step 1: Write failing HTTP tests**

Start the server on `127.0.0.1:0` in tests and assert:

```rust
#[test]
fn protected_endpoints_require_bearer_token() {
    let server = AdminFixture::start();

    assert_eq!(server.get("/api/health").status(), 200);
    assert_eq!(server.get("/api/overview").status(), 401);
    assert_eq!(server.get_with_token("/api/overview", "wrong").status(), 401);
    assert_eq!(server.get_with_token("/api/overview", server.token()).status(), 200);
}

#[test]
fn disabled_group_can_be_enabled_through_api() {
    let server = AdminFixture::start();
    let response = server.put_json(
        "/api/groups/123",
        json!({"enabled": true, "reason": "enable test group", "confirm": "group:123"}),
    );

    assert_eq!(response.status(), 200);
    assert!(server.group_enabled(123));
    assert_eq!(server.audit_count("group.update"), 1);
}
```

- [ ] **Step 2: Run tests and verify failure**

Run: `cargo test --test admin_http`

Expected: compile failure because the router is absent.

- [ ] **Step 3: Implement bounded request parsing and responses**

```rust
const MAX_BODY_BYTES: usize = 1024 * 1024;

#[derive(Serialize)]
struct ApiEnvelope<T> {
    ok: bool,
    data: T,
}

#[derive(Serialize)]
struct ApiErrorEnvelope {
    ok: bool,
    error: ApiErrorBody,
}
```

Reject bodies larger than 1 MiB before JSON parsing. Set `Content-Type`, `Cache-Control: no-store`,
`X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY` and a same-origin Content Security Policy. Do not emit
`Access-Control-Allow-Origin: *`.

- [ ] **Step 4: Implement routes from the approved design**

Route login, health, overview, groups, group features, players, profile, wallet, cultivation, items, statistics,
configuration, definitions, audit, backup and Token rotation. Enforce method, authentication, validation, `reason`
and server-side `confirm` for every write endpoint.

Static definition updates parse JSON/TOML using existing structured parsers, write a sibling temporary file,
`sync_all`, retain one `.bak` copy and atomically replace the active file. Reject unknown definition kinds.
Runtime configuration updates validate a complete candidate `RuntimeConfig`, atomically replace `config.toml`, then
swap the shared `RuntimePolicy`; a failed write or parse leaves both file and live policy unchanged.

- [ ] **Step 5: Verify HTTP behavior**

Run: `cargo test --test admin_http`

Expected: PASS for authentication, input limits, confirmations, CRUD, structured errors and no sensitive output.

- [ ] **Step 6: Commit**

```powershell
git add src/admin src/database/admin.rs tests/admin_http.rs
git commit -m "feat: add authenticated admin API"
```

---

### Task 6: Build the Embedded Management Page

**Files:**

- Create: `src/admin/ui.rs`
- Modify: `src/admin/router.rs`
- Test: `tests/admin_http.rs`

- [ ] **Step 1: Add failing static-page assertions**

```rust
#[test]
fn admin_page_is_embedded_and_has_complete_navigation() {
    let server = AdminFixture::start();
    let response = server.get("/");

    assert_eq!(response.status(), 200);
    assert!(response.body().contains("Luo Realm 管理后台"));
    for section in ["仪表盘", "群管理", "玩家管理", "世界配置", "审计日志", "运维"] {
        assert!(response.body().contains(section));
    }
}
```

- [ ] **Step 2: Run the test and verify failure**

Run: `cargo test --test admin_http admin_page_is_embedded_and_has_complete_navigation`

Expected: FAIL because `/` has no page.

- [ ] **Step 3: Implement one self-contained page**

`ui.rs` exposes `pub const HTML: &str`. The page contains:

- Login view using `sessionStorage` and Bearer headers.
- Fixed sidebar and compact responsive content area.
- Dashboard health and counts.
- Searchable group table with enable and feature toggles.
- Searchable player table and profile/wallet/cultivation/items/statistics tabs.
- Structured forms for global settings and a validated text editor for static definitions.
- Paginated audit table.
- Backup action, Token generation/rotation and runtime information.
- Reusable confirmation dialog showing target, before value, after value and required reason.
- Loading, empty, success and error states without layout shifts.

Use native controls, inline CSS, inline JavaScript and no external resources. Escape all server-originated values with
`textContent`; never inject API strings through `innerHTML`.

- [ ] **Step 4: Verify page and API wiring**

Run: `cargo test --test admin_http`

Expected: PASS for static navigation, CSP headers, login flow and representative write actions.

- [ ] **Step 5: Commit**

```powershell
git add src/admin/ui.rs src/admin/router.rs tests/admin_http.rs
git commit -m "feat: add embedded admin console"
```

---

### Task 7: Start Admin Server and Publish Runtime State

**Files:**

- Modify: `src/admin/mod.rs`
- Modify: `src/lib.rs`
- Test: `tests/admin_ports.rs`

- [ ] **Step 1: Add failing lifecycle tests**

```rust
#[test]
fn runtime_file_contains_actual_fallback_port() {
    let fixture = AdminServerFixture::with_occupied_base_port();
    let running = fixture.start().unwrap();
    let runtime: serde_json::Value = serde_json::from_slice(
        &std::fs::read(fixture.runtime_path()).unwrap(),
    )
    .unwrap();

    assert_eq!(runtime["bind"], "127.0.0.1");
    assert_eq!(runtime["port"], running.port());
    assert!(runtime["pid"].as_u64().is_some());
}
```

- [ ] **Step 2: Run and verify failure**

Run: `cargo test --test admin_ports runtime_file_contains_actual_fallback_port`

Expected: FAIL because lifecycle publication is absent.

- [ ] **Step 3: Implement startup and runtime publication**

At plugin startup:

```rust
if config.admin.enabled {
    let admin_root = root.clone();
    let admin_config = config.admin.clone();
    std::thread::spawn(move || {
        if let Err(error) = admin::start(admin_root, admin_config) {
            eprintln!("[Luo Realm] admin startup failed: {error}");
        }
    });
}
```

`admin::start` loads or creates Token, probes ten ports, atomically writes `admin.runtime.json`, then serves requests.
On bind failure it removes only a runtime file whose recorded PID matches the current process.

- [ ] **Step 4: Verify lifecycle tests**

Run: `cargo test --test admin_ports`

Expected: PASS for base port, fallback port, ten-port exhaustion and runtime metadata.

- [ ] **Step 5: Commit**

```powershell
git add src/admin/mod.rs src/lib.rs tests/admin_ports.rs
git commit -m "feat: start Luo Realm admin server"
```

---

### Task 8: Document and Verify the Complete Workflow

**Files:**

- Modify: `README.md`
- Modify: `.gitignore`
- Test: all Rust tests

- [ ] **Step 1: Update deployment documentation**

Document:

```text
Admin URL: read data/luo_realm/admin.runtime.json
Initial Token: data/luo_realm/admin.token
Default port attempts: 18765 through 18774
Bind address: 0.0.0.0
Group policy: missing and disabled groups are denied
Private policy: only configured admin_ids are allowed
Security: use only on trusted LAN or behind an HTTPS reverse proxy
```

Describe backup behavior, Token rotation, group enablement and the fact that the runtime file is not an authentication
secret. Ignore runtime token, runtime metadata, SQLite sidecars and backup output in `.gitignore`.

- [ ] **Step 2: Run all quality gates**

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
python -m unittest discover -s tools/tests -v
```

Expected: all commands exit `0`, and `target/release/luo_realm.dll` exists.

- [ ] **Step 3: Perform local HTTP smoke verification**

Start a test-only admin server and verify:

```text
GET  /api/health                         -> 200 without Token
GET  /api/overview                       -> 401 without Token
POST /api/login                          -> 200 with valid Token
PUT  /api/groups/123                     -> 200 and groups.enabled=1
POST /api/players/10001/wallet           -> 200 and one wallet/audit row
POST /api/backup                          -> 200 and independently valid SQLite file
POST /api/token/rotate                    -> 200; old Token then returns 401
```

- [ ] **Step 4: Run final branding and secret scans**

```powershell
rg -n -i "plugin_subh|data/subh|subh-v1|/sub" src config migrations README.md
rg -n "admin\.token|Authorization" src/admin README.md
git diff --check
```

Expected: no active old runtime identifier; Token is referenced only as a path or header name, never embedded as a
literal secret.

- [ ] **Step 5: Commit**

```powershell
git add README.md .gitignore src tests migrations config Cargo.toml Cargo.lock
git commit -m "docs: document Luo Realm administration"
```

Do not add `.codex`, do not copy the DLL into the bot installation, and do not run `git push`.
