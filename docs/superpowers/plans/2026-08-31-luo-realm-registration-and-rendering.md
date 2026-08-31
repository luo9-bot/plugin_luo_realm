# Luo Realm Registration and Rendering Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 增加持久化两阶段注册和全局玩法门禁，保证决斗双方均为完整角色，并使用旧 SUB.H 素材重做角色卡与逐回合战斗 GIF。

**Architecture:** SQLite migration 3 为玩家增加独立注册阶段，`database::player` 负责注册状态机，新的 `command` 模块在所有玩法之前执行统一门禁。战斗核心返回完整动作帧，渲染器只消费领域 DTO 并可选加载旧素材；素材或字体失败不会回滚已提交业务事务。

**Tech Stack:** Rust 2024、rusqlite WAL、serde/serde_json、image、imageproc、ab_glyph、luo9_sdk。

---

## File Map

**Create**

- `migrations/0003_registration.sql`：注册阶段与旧玩家兼容迁移。
- `src/command/mod.rs`：命令解析、注册门禁与分发。
- `src/command/registration.rs`：注册、选系和改名命令。
- `src/render/profile.rs`：旧素材角色卡合成。
- `src/render/battle.rs`：逐回合战斗 GIF 合成。
- `src/render/assets.rs`：素材与字体定位、稳定形象选择和降级。

**Modify**

- `Cargo.toml`：增加文字绘制依赖。
- `src/database/migrations.rs`：注册 migration 3。
- `src/database/player.rs`：显式注册状态机，移除普通玩法的隐式创建能力。
- `src/database/cultivation.rs`：首次体系激活接口。
- `src/database/combat.rs`：持久化完整战斗帧。
- `src/database/admin.rs`：后台展示注册阶段并在修行修正时同步激活。
- `src/core.rs`：战斗结果增加动作帧。
- `src/engine/mod.rs`：战斗档案携带体系技能列表。
- `src/lib.rs`：只保留框架适配与后台启动，委托给 `command`。
- `src/render.rs`：改为渲染模块入口和 DTO。
- `README.md`：注册流程、素材目录和降级行为。

**Local verification only; never add to Git**

- `tests/registration_gate.rs`
- `tests/duel_registration.rs`

### Task 1: Persist Two-Stage Registration

**Files:**

- Create: `migrations/0003_registration.sql`
- Modify: `src/database/migrations.rs`
- Modify: `src/database/player.rs`
- Modify: `src/database/cultivation.rs`
- Local test only: `tests/registration_gate.rs`

- [ ] **Step 1: Create migration 3**

```sql
ALTER TABLE players ADD COLUMN registration_state TEXT NOT NULL DEFAULT 'active'
    CHECK (registration_state IN ('pending_system', 'active'));

CREATE INDEX players_registration_state
ON players(registration_state, status);
```

Existing rows receive `active`; new registration code explicitly inserts `pending_system`.

- [ ] **Step 2: Define registration domain results**

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistrationState {
    Missing,
    PendingSystem,
    Active,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegisterResult {
    Created,
    PendingSystem,
    AlreadyActive,
    Unavailable,
}
```

Implement `registration_state(connection, user_id)` as a read-only query over `players.status` and
`players.registration_state`.

- [ ] **Step 3: Implement explicit registration**

```rust
pub fn register(
    transaction: &Transaction<'_>,
    user_id: u64,
    display_name: &str,
) -> DatabaseResult<RegisterResult>;

pub fn activate_system(
    transaction: &Transaction<'_>,
    user_id: u64,
    system_id: &str,
) -> DatabaseResult<bool>;
```

`register` validates the normalized name before inserts and never creates cultivation data.
`activate_system` updates only `pending_system`, inserts cultivation, and returns `false` for repeat selection.

- [ ] **Step 4: Make player loading explicit**

Replace gameplay calls to `find_or_create` with:

```rust
pub fn get_active(
    transaction: &Transaction<'_>,
    user_id: u64,
) -> DatabaseResult<Option<Player>>;
```

The query requires both status and registration state to be `active` and requires cultivation through an inner join.

- [ ] **Step 5: Run local registration checks**

```powershell
cargo test --test registration_gate
cargo test --test database_bootstrap
```

Expected: new user remains pending after registration, activation is atomic, existing v2 players migrate active.
Do not add test files to Git.

- [ ] **Step 6: Commit product files only**

```powershell
git add migrations/0003_registration.sql src/database/migrations.rs src/database/player.rs src/database/cultivation.rs
git commit -m "feat: add two-stage player registration"
```

### Task 2: Centralize Command Registration Gates

**Files:**

- Create: `src/command/mod.rs`
- Create: `src/command/registration.rs`
- Modify: `src/lib.rs`
- Local test only: `tests/registration_gate.rs`

- [ ] **Step 1: Classify command access**

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AccessLevel {
    Public,
    Named,
    Active,
}

fn required_access(command: &str) -> AccessLevel {
    match command {
        "菜单" | "帮助" | "help" | "体系" | "修行" | "cultivation" | "注册" => {
            AccessLevel::Public
        }
        "选择体系" | "改名" | "name" => AccessLevel::Named,
        _ => AccessLevel::Active,
    }
}
```

- [ ] **Step 2: Add one read-only gate before command branches**

Map `RegistrationState` and `AccessLevel` to either dispatch permission or one of these messages:

```text
尚未注册，请发送“注册 <名称>”创建角色。
角色尚未确定修行体系，请发送“体系”查看并选择。
当前角色已被停用，请联系管理员。
```

Unknown text still returns `None`; it must not create a player merely because the message is not a command.

- [ ] **Step 3: Implement registration commands**

`注册 <名称>` creates the pending player and returns the system list. `选择体系 <id>` activates only pending
players. `改名 <名称>` updates pending or active players, but never creates a player.

- [ ] **Step 4: Replace all gameplay implicit creation calls**

签到、状态、战力、每日事件和决斗 must load active players and treat missing data as a domain consistency error.
排行 is available only to active users, although the query itself remains group-wide.

- [ ] **Step 5: Verify command matrix locally**

```powershell
cargo test --test registration_gate
cargo test --test command_transactions
```

Expected: every state/command combination matches the design and rejected commands leave row counts unchanged.

- [ ] **Step 6: Commit product files only**

```powershell
git add src/command/mod.rs src/command/registration.rs src/lib.rs
git commit -m "feat: gate world commands behind registration"
```

### Task 3: Require Complete Duel Participants and Record Frames

**Files:**

- Modify: `src/core.rs`
- Modify: `src/database/combat.rs`
- Modify: `src/command/mod.rs`
- Local test only: `tests/duel_registration.rs`

- [ ] **Step 1: Add combat action frames**

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CombatFrame {
    pub round: u32,
    pub attacker_id: String,
    pub defender_id: String,
    pub skill: String,
    pub damage: i64,
    pub critical: bool,
    pub left_hp: i64,
    pub right_hp: i64,
}
```

`CombatResult` stores `Vec<CombatFrame>`. Extend `CombatProfile` with `skills: &'static [&'static str]`, and change
the simulator boundary to:

```rust
pub struct Combatant<'a> {
    pub player: &'a Player,
    pub skills: &'a [&'a str],
}

pub fn simulate_combat(
    left: Combatant<'_>,
    right: Combatant<'_>,
    seed: u64,
    max_rounds: u32,
) -> CombatResult;
```

`simulate_combat` pushes one frame per successful action and selects a deterministic skill from the supplied slice;
an empty slice uses `普通攻击`.

- [ ] **Step 2: Load both participants without creation**

The duel branch calls `get_active` for both IDs inside the same immediate transaction. Missing or incomplete target
returns `对方尚未完成角色注册，无法决斗。` before simulation or writes.

- [ ] **Step 3: Persist all frames**

Serialize each `CombatFrame` and insert it into `combat_rounds` with consecutive `round_index`. Participant rows,
rounds, rewards and statistics remain in the same transaction.

- [ ] **Step 4: Verify duel rollback locally**

```powershell
cargo test --test duel_registration
cargo test --test command_transactions duel_uses_and_records_selected_systems
```

Expected: invalid target creates no rows; valid duel stores frames and pays each participant once.

- [ ] **Step 5: Commit product files only**

```powershell
git add src/core.rs src/database/combat.rs src/command/mod.rs src/engine/mod.rs
git commit -m "feat: record complete registered duels"
```

### Task 4: Rebuild Profile and Battle Rendering

**Files:**

- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `src/render.rs`
- Create: `src/render/assets.rs`
- Create: `src/render/profile.rs`
- Create: `src/render/battle.rs`
- Modify: `src/command/mod.rs`

- [ ] **Step 1: Add renderer DTOs**

```rust
pub struct ProfileRenderData<'a> {
    pub player: &'a Player,
    pub system_name: &'a str,
    pub realm_name: &'a str,
    pub realm_index: u32,
    pub progress: u64,
    pub power: f64,
}

pub struct BattleRenderData<'a> {
    pub left: &'a CombatProfile,
    pub right: &'a CombatProfile,
    pub left_system: &'a str,
    pub right_system: &'a str,
    pub result: &'a CombatResult,
}
```

- [ ] **Step 2: Add optional legacy asset resolution**

Resolve in order:

1. `<plugin>/assets/legacy`
2. `<plugin>/SUBData/SUB-图片`
3. `<plugin-parent>/SUBData/SUB-图片`

Use a SHA-256-derived stable image index from player ID. Decode only PNG assets and return `Option<DynamicImage>`;
missing assets are normal fallback, not an error.

- [ ] **Step 3: Draw an informative profile card**

Use a 960x540 fixed canvas, a large left portrait, right-side hierarchy, realm badge, attribute rows and stable bars.
Try configured/system Chinese font through `ab_glyph`; without a font, keep geometric layout and rely on the complete
text reply.

- [ ] **Step 4: Draw battle frames from real combat data**

Use the two stable portraits, exact HP after each `CombatFrame`, skill icon when found, system colors, and a winner
frame repeated twice. Do not infer intermediate HP from the final winner.

- [ ] **Step 5: Make output replacement interruption-safe**

Encode to sibling `.new`, flush and close, replace the final PNG/GIF, and recover or remove stale temporary files on
the next render. Rendering errors are logged and converted to text-only replies after the gameplay transaction commits.

- [ ] **Step 6: Inspect generated files locally**

Generate one profile for sword/body systems and one duel GIF. Verify dimensions, nonblank pixels, portrait visibility,
HP bar change and final winner frame. Generated files stay under ignored `data/` and are never added to Git.

- [ ] **Step 7: Commit product files only**

```powershell
git add Cargo.toml Cargo.lock src/render.rs src/render/assets.rs src/render/profile.rs src/render/battle.rs src/command/mod.rs
git commit -m "feat: restore character and duel presentation"
```

### Task 5: Update Admin Data and Documentation

**Files:**

- Modify: `src/database/admin.rs`
- Modify: `src/admin/ui.rs`
- Modify: `README.md`

- [ ] **Step 1: Expose registration state**

Add `registration_state` to `PlayerRow`. Overview returns `pending_players` and `active_players`. Player detail marks
pending characters and does not assume cultivation exists, so list queries change cultivation joins to left joins.

- [ ] **Step 2: Keep admin cultivation edits consistent**

When an administrator writes a valid cultivation system, update `registration_state='active'` in the same transaction.
The audit `after_json` includes the resulting registration state.

- [ ] **Step 3: Update the management page**

Display registration state in player list/detail, disable wallet/item/statistic actions for pending players, and retain
the cultivation correction action as the explicit administrator completion path.

- [ ] **Step 4: Document the player journey and assets**

README documents:

```text
注册 <名称>
体系
选择体系 <体系标识>
签到 / 状态 / 战力 / 每日事件 / 决斗
```

Also document `assets/legacy` layout and text-only degradation.

- [ ] **Step 5: Run final verification**

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
python -m unittest discover -s tools/tests -v
```

Expected: all commands succeed with zero warnings and `target/release/luo_realm.dll` is produced.

- [ ] **Step 6: Confirm prohibited files are absent and commit**

```powershell
git diff --cached --name-only
git add src/database/admin.rs src/admin/ui.rs README.md
git commit -m "docs: explain registered world progression"
```

The staged list must not contain `.codex`, `.docs`, `test`, `tests`, `tests.rs`, test fixtures or snapshots.
