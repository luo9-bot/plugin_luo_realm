/** 玩家网页 API：一次性票据换会话，之后以 Bearer 会话调用只读接口。 */

export const API_BASE: string = import.meta.env.VITE_API_BASE ?? "";

/**
 * 数据面模式：
 * - `direct`（默认）：页面直连插件接口（本地联调）。
 * - `cf`：页面只与 Cloudflare Worker 通信，档案读自插件推送的快照，
 *   写操作经 Worker 以服务端环境变量转发回源站。
 */
export const DATA_MODE: "direct" | "cf" =
  (import.meta.env.VITE_DATA_MODE as "direct" | "cf" | undefined) ?? "direct";

const SESSION_KEY = "lr-player-session";

export interface Profile {
  display_name: string;
  character_id: string;
  biography: string;
  system_id: string;
  system_name: string;
  realm_index: number;
  realm_name: string;
  power: number;
  registered_at: number;
  daily_state: {
    id: string;
    name: string;
    description: string;
    rule_version: number;
  } | null;
}

export interface Balance {
  currency: string;
  amount: number;
}

export interface Transaction {
  reason: string;
  delta: number;
  balance_after: number;
  created_at: number;
}

export interface Wallet {
  balances: Balance[];
  transactions: Transaction[];
}

export interface Skill {
  id: string;
  name: string;
  mastery: number;
}

export interface Skills {
  skills: Skill[];
  tactic: string;
}

export interface Item {
  item_id: number;
  definition_id: string;
  quantity: number;
  quality: string;
  level: number;
  equipped_slot: string | null;
}

export interface Equipment {
  items: Item[];
}

export interface Battle {
  combat_id: number;
  started_at: number;
  team: number;
  winner_team: number;
  end_reason: string;
  rule_version: number;
  power: number;
}

export interface Battles {
  battles: Battle[];
}

export function sessionToken(): string | null {
  return sessionStorage.getItem(SESSION_KEY);
}

/** Cloudflare 模式：`主页` 链接直接携带页面会话令牌。 */
export function adoptPageToken(token: string): void {
  sessionStorage.setItem(SESSION_KEY, token);
}

export function dropSession(): void {
  sessionStorage.removeItem(SESSION_KEY);
}

export async function exchangeTicket(ticket: string): Promise<void> {
  const payload = await request<{ session_token: string }>(
    "POST",
    "/api/player/session",
    { ticket },
  );
  sessionStorage.setItem(SESSION_KEY, payload.session_token);
}

/** 是否已持有会话（不校验有效性，有效性由首次请求验证）。 */
export function hasSession(): boolean {
  return sessionToken() !== null;
}

async function request<T>(
  method: string,
  path: string,
  body?: unknown,
): Promise<T> {
  const response = await fetch(API_BASE + path, {
    method,
    headers: {
      ...(body ? { "Content-Type": "application/json" } : {}),
      ...(sessionToken() ? { Authorization: `Bearer ${sessionToken()}` } : {}),
    },
    body: body ? JSON.stringify(body) : undefined,
  });
  const payload = (await response.json().catch(() => null)) as
    | { ok: boolean; data?: T; error?: { code: string; message: string } }
    | null;
  if (!response.ok || !payload || payload.ok !== true || payload.data === undefined) {
    const error = new Error(
      payload?.error?.code ?? `request_failed_${response.status}`,
    ) as Error & { status: number };
    error.status = response.status;
    if (response.status === 401) {
      dropSession();
    }
    throw error;
  }
  return payload.data;
}

interface StateSnapshot {
  profile: Profile;
  wallet: Wallet;
  skills: Skills;
  equipment: Equipment;
  battles: Battles;
}

let stateSnapshot: StateSnapshot | null = null;
let statePromise: Promise<void> | null = null;

async function ensureState(): Promise<void> {
  if (stateSnapshot) {
    return;
  }
  if (!statePromise) {
    statePromise = request<StateSnapshot>("GET", "/api/state")
      .then((payload) => {
        stateSnapshot = payload;
      })
      .finally(() => {
        statePromise = null;
      });
  }
  await statePromise;
}

export async function refreshState(): Promise<void> {
  if (DATA_MODE !== "cf") {
    return;
  }
  stateSnapshot = null;
  await ensureState();
}

export function getProfile(): Promise<Profile> {
  if (DATA_MODE === "cf") {
    return ensureState().then(() => stateSnapshot!.profile);
  }
  return request("GET", "/api/player/profile");
}

export function getWallet(): Promise<Wallet> {
  if (DATA_MODE === "cf") {
    return ensureState().then(() => stateSnapshot!.wallet);
  }
  return request("GET", "/api/player/wallet");
}

export function getSkills(): Promise<Skills> {
  if (DATA_MODE === "cf") {
    return ensureState().then(() => stateSnapshot!.skills);
  }
  return request("GET", "/api/player/skills");
}

export function getEquipment(): Promise<Equipment> {
  if (DATA_MODE === "cf") {
    return ensureState().then(() => stateSnapshot!.equipment);
  }
  return request("GET", "/api/player/equipment");
}

export function getBattles(): Promise<Battles> {
  if (DATA_MODE === "cf") {
    return ensureState().then(() => stateSnapshot!.battles);
  }
  return request("GET", "/api/player/battles");
}

export function getPortraits(): Promise<{ portraits: string[] }> {
  return request("GET", "/api/player/portraits");
}

export async function setCharacter(
  characterId: string,
): Promise<{ character_id: string }> {
  if (DATA_MODE === "cf") {
    const result = await request<{ character_id: string }>("POST", "/api/command", {
      token: sessionToken(),
      action: "set_character",
      payload: { character_id: characterId },
    });
    stateSnapshot = null;
    await ensureState();
    return result;
  }
  return request("POST", "/api/player/character", { character_id: characterId });
}

/** 物品图标地址：服务端按与群内卡片一致的规则挑选素材。 */
export function itemIcon(definitionId: string): string {
  return `${API_BASE}/api/player/asset/icon/${encodeURIComponent(definitionId)}.png`;
}

/** 玩家形象地址。 */
export function portraitUrl(characterId: string): string {
  return `${API_BASE}/api/player/asset/portrait/${encodeURIComponent(characterId)}.png`;
}
