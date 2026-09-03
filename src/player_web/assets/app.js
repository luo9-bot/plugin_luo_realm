/* Luo Realm 玩家档案页：只读消费 /api/player 只读接口。 */
"use strict";

const STORAGE_KEY = "lr-player-session";
const VIEWS = ["wallet", "skills", "equipment", "battles"];

const state = { session: null };

function gate(message) {
  document.getElementById("gate").classList.remove("hidden");
  document.getElementById("content").classList.add("hidden");
  document.getElementById("gate-message").textContent = message;
}

function enter(session) {
  state.session = session;
  sessionStorage.setItem(STORAGE_KEY, session);
  document.getElementById("gate").classList.add("hidden");
  document.getElementById("content").classList.remove("hidden");
  document.getElementById("session-note").textContent = "会话有效";
  loadProfile();
  loadView("wallet");
}

async function api(path, options) {
  const response = await fetch(path, {
    ...options,
    headers: {
      ...(options && options.headers),
      Authorization: `Bearer ${state.session}`,
    },
  });
  const payload = await response.json().catch(() => null);
  if (!response.ok || !payload || payload.ok !== true) {
    const code = payload && payload.error ? payload.error.code : "request_failed";
    if (response.status === 401) {
      sessionStorage.removeItem(STORAGE_KEY);
    }
    const error = new Error(code);
    error.status = response.status;
    throw error;
  }
  return payload.data;
}

async function exchangeTicket(ticket) {
  const response = await fetch("/api/player/session", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ ticket }),
  });
  const payload = await response.json().catch(() => null);
  if (!response.ok || !payload || payload.ok !== true) {
    throw new Error(payload && payload.error ? payload.error.code : "exchange_failed");
  }
  return payload.data.session_token;
}

async function loadProfile() {
  try {
    const profile = await api("/api/player/profile");
    document.getElementById("hero-avatar").textContent =
      (profile.display_name || "玄").slice(0, 1);
    document.getElementById("profile-name").textContent = profile.display_name;
    document.getElementById("profile-line").textContent =
      `${profile.system_name} · ${profile.realm_name}`;
    document.getElementById("profile-power").textContent = Math.round(profile.power);
    const daily = profile.daily_state;
    document.getElementById("daily-name").textContent =
      daily ? `${daily.name}（v${daily.rule_version}）` : "尚未生成";
    document.getElementById("daily-desc").textContent =
      daily ? daily.description : "发送 今日状态 生成今日状态。";
  } catch (error) {
    if (error.status === 401) { gate("会话已过期，请在群内重新发送「主页」。"); }
  }
}

async function loadView(name) {
  try {
    if (name === "wallet") { renderWallet(await api("/api/player/wallet")); }
    if (name === "skills") { renderSkills(await api("/api/player/skills")); }
    if (name === "equipment") { renderEquipment(await api("/api/player/equipment")); }
    if (name === "battles") { renderBattles(await api("/api/player/battles")); }
  } catch (error) {
    if (error.status === 401) { gate("会话已过期，请在群内重新发送「主页」。"); }
  }
}

function renderWallet(view) {
  document.getElementById("wallet-balances").innerHTML = view.balances.map((balance) => `
    <div class="balance">
      <div class="amount">${balance.amount}</div>
      <div class="currency">${currencyName(balance.currency)}</div>
    </div>`).join("");
  document.getElementById("wallet-transactions").innerHTML = view.transactions.map((tx) => `
    <li>
      <span class="title">${reasonName(tx.reason)}</span>
      <span class="meta">
        <span class="${tx.delta >= 0 ? "delta-pos" : "delta-neg"}">${tx.delta >= 0 ? "+" : ""}${tx.delta}</span>
        · 余额 ${tx.balance_after}
      </span>
    </li>`).join("") || "<li><span class='title muted'>暂无流水</span></li>";
}

function renderSkills(view) {
  const tactics = {
    balanced: "均衡", aggressive: "强攻", defensive: "守御",
    sustain: "续航", control: "控制",
  };
  document.getElementById("tactic-line").textContent =
    `当前战术：${tactics[view.tactic] || view.tactic}`;
  document.getElementById("skill-list").innerHTML = view.skills.map((skill) => `
    <li>
      <span class="title">${skill.name}</span>
      <span class="meta">熟练度 ${skill.mastery}/3</span>
    </li>`).join("") || "<li><span class='title muted'>尚未掌握技能</span></li>";
}

function renderEquipment(view) {
  document.getElementById("item-list").innerHTML = view.items.map((item) => `
    <li>
      <span class="title">${item.definition_id}${item.level ? ` +${item.level}` : ""}</span>
      <span class="meta">${item.quantity} 件 · ${item.equipped_slot ? slotName(item.equipped_slot) : "未装备"}</span>
    </li>`).join("") || "<li><span class='title muted'>背包空空如也</span></li>";
}

function renderBattles(view) {
  document.getElementById("battle-list").innerHTML = view.battles.map((battle) => {
    const won = battle.team === battle.winner_team;
    const ended = new Date(battle.started_at * 1000);
    const when = `${ended.getMonth() + 1}月${ended.getDate()}日 ${String(ended.getHours()).padStart(2, "0")}:${String(ended.getMinutes()).padStart(2, "0")}`;
    return `
    <li>
      <span class="title">${when} · 战力 ${battle.power} · 规则 v${battle.rule_version}</span>
      <span class="meta ${won ? "win" : "loss"}">${won ? "胜利" : "败北"}</span>
    </li>`;
  }).join("") || "<li><span class='title muted'>还没有战斗记录</span></li>";
}

function currencyName(code) {
  return { coins: "金币", marks: "刻印" }[code] || code;
}

function reasonName(code) {
  const names = {
    daily_checkin: "每日签到", duel_reward: "决斗奖励", group_world_event: "世界事件",
    ascii_fpv_reward: "御空试炼", admin_adjustment: "管理员调整",
  };
  return names[code] || code;
}

function slotName(code) {
  const names = {
    main_hand: "主手", off_hand: "副手", head: "头部", body: "身体",
    hands: "手部", feet: "足部", accessory_1: "饰品一", accessory_2: "饰品二",
  };
  return names[code] || code;
}

document.getElementById("tabs").addEventListener("click", (event) => {
  const button = event.target.closest("button[data-view]");
  if (!button) { return; }
  document.querySelectorAll("#tabs button").forEach((tab) => {
    tab.classList.toggle("active", tab === button);
  });
  VIEWS.forEach((view) => {
    document.getElementById(`view-${view}`).classList.toggle("hidden", view !== button.dataset.view);
  });
  loadView(button.dataset.view);
});

(async function boot() {
  const params = new URLSearchParams(location.search);
  const ticket = params.get("ticket");
  if (ticket) {
    history.replaceState(null, "", location.pathname);
    try {
      enter(await exchangeTicket(ticket));
      return;
    } catch (error) {
      gate(`票据无效：${error.message === "player_web.ticket_already_used" ? "该链接已被使用" : "已过期或不合法"}。请在群内重新发送「主页」。`);
      return;
    }
  }
  const stored = sessionStorage.getItem(STORAGE_KEY);
  if (stored) {
    state.session = stored;
    try {
      await api("/api/player/profile");
      enter(stored);
      return;
    } catch (error) { /* fall through to gate */ }
  }
  gate("缺少访问凭据。请在群聊中发送「主页」获取一次性进入链接。");
})();
