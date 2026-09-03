<script setup lang="ts">
import { onMounted, ref } from "vue";
import {
  exchangeTicket,
  hasSession,
  sessionToken,
  dropSession,
  getProfile,
  type Profile,
} from "./api";
import PageMenu from "./pages/PageMenu.vue";
import PageProfile from "./pages/PageProfile.vue";
import PageEquipment from "./pages/PageEquipment.vue";
import PageSkills from "./pages/PageSkills.vue";
import PageWallet from "./pages/PageWallet.vue";
import PageBattles from "./pages/PageBattles.vue";

type Section = "menu" | "profile" | "equipment" | "skills" | "wallet" | "battles";

const SECTIONS: { key: Section; label: string; group: string }[] = [
  { key: "menu", label: "菜单", group: "PLAYER" },
  { key: "profile", label: "角色卡", group: "PLAYER" },
  { key: "equipment", label: "装备", group: "PLAYER" },
  { key: "skills", label: "技能", group: "PLAYER" },
  { key: "wallet", label: "资产", group: "PLAYER" },
  { key: "battles", label: "战斗", group: "WORLD" },
];

const current = ref<Section>("menu");
const profile = ref<Profile | null>(null);
const gateMessage = ref("正在验证访问凭据……");
const gate = ref(true);

function gateWith(message: string): void {
  gate.value = true;
  gateMessage.value = message;
}

async function loadProfile(): Promise<void> {
  profile.value = await getProfile();
}

function enter(): void {
  gate.value = false;
  current.value = "menu";
  loadProfile().catch(() => gateWith("会话已失效，请在群内重新发送「主页」。"));
}

function open(section: Section): void {
  current.value = section;
  history.replaceState(null, "", `#${section}`);
}

function onSessionExpired(): void {
  dropSession();
  gateWith("会话已过期，请在群内重新发送「主页」。");
}

onMounted(async () => {
  const ticket = new URLSearchParams(location.search).get("ticket");
  if (ticket) {
    history.replaceState(null, "", location.pathname);
    try {
      await exchangeTicket(ticket);
      enter();
      return;
    } catch (error) {
      gateWith(
        error instanceof Error && error.message === "player_web.ticket_already_used"
          ? "该链接已被使用。请在群内重新发送「主页」。"
          : "票据无效或已过期。请在群内重新发送「主页」。",
      );
      return;
    }
  }
  if (hasSession()) {
    try {
      await loadProfile();
      enter();
      return;
    } catch (error) {
      if ((error as Error & { status?: number }).status !== 401) {
        enter();
        return;
      }
    }
  }
  gateWith("缺少访问凭据。请在群聊中发送「主页」获取一次性进入链接。");
});

const hash = location.hash.slice(1) as Section;
if (SECTIONS.some((section) => section.key === hash)) {
  current.value = hash;
}

function guarded(action: () => Promise<void>): () => void {
  return () => {
    if (!sessionToken()) {
      onSessionExpired();
      return;
    }
    action().catch(onSessionExpired);
  };
}

const refreshers: Record<Section, () => void> = {
  menu: () => {},
  profile: guarded(loadProfile),
  equipment: guarded(async () => {}),
  skills: guarded(async () => {}),
  wallet: guarded(async () => {}),
  battles: guarded(async () => {}),
};
</script>

<template>
  <div v-if="gate" class="gate">
    <div class="gate-card">
      <h1>Luo Realm · 修行档案</h1>
      <p>{{ gateMessage }}</p>
      <p class="muted">
        档案页只读展示角色资料；在群里发送 <code>主页</code> 可获取新的进入链接。
      </p>
    </div>
  </div>
  <div v-else class="app">
    <nav>
      <div class="logo">LUO<small>REALM</small></div>
      <template v-for="group in ['PLAYER', 'WORLD']" :key="group">
        <div class="nav-title">{{ group }}</div>
        <button
          v-for="section in SECTIONS.filter((item) => item.group === group)"
          :key="section.key"
          :class="{ active: current === section.key }"
          @click="open(section.key); refreshers[section.key]()"
        >
          {{ section.label }}
        </button>
      </template>
    </nav>
    <main>
      <div class="top">
        <div>
          <div class="eyebrow">LUO REALM</div>
          <div class="title">{{ SECTIONS.find((item) => item.key === current)?.label }}</div>
          <div class="desc">只读档案 · 修行操作请在群聊完成</div>
        </div>
        <div v-if="profile" class="muted" style="text-align: right">
          <b style="color: var(--text)">{{ profile.display_name }}</b><br />
          {{ profile.system_name }} · {{ profile.realm_name }}
        </div>
      </div>
      <PageMenu v-if="current === 'menu'" @open="open($event)" />
      <PageProfile v-else-if="current === 'profile'" :profile="profile" />
      <PageEquipment v-else-if="current === 'equipment'" />
      <PageSkills v-else-if="current === 'skills'" />
      <PageWallet v-else-if="current === 'wallet'" />
      <PageBattles v-else-if="current === 'battles'" />
      <div class="foot">Luo Realm · 只读档案页</div>
    </main>
  </div>
</template>
