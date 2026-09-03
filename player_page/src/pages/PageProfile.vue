<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import {
  getPortraits,
  portraitUrl,
  setCharacter,
  type Profile,
} from "../api";

const props = defineProps<{ profile: Profile | null }>();
const emit = defineEmits<{ refresh: [] }>();

const portraits = ref<string[]>([]);
const selected = ref<string | null>(null);
const saving = ref(false);
const message = ref("");

const currentId = computed(() => selected.value ?? props.profile?.character_id ?? "");

onMounted(async () => {
  try {
    portraits.value = (await getPortraits()).portraits;
  } catch {
    portraits.value = [];
  }
});

async function save(): Promise<void> {
  if (!selected.value || saving.value) {
    return;
  }
  saving.value = true;
  message.value = "";
  try {
    await setCharacter(selected.value);
    selected.value = null;
    message.value = "形象已更新。";
    emit("refresh");
  } catch (error) {
    message.value =
      error instanceof Error && error.message === "portrait_not_found"
        ? "该形象已不存在。"
        : "保存失败，请稍后再试。";
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <div class="grid side">
    <div>
      <img
        v-if="currentId"
        class="portrait"
        :src="portraitUrl(currentId)"
        alt="角色形象"
      />
      <div v-else class="portrait-fallback">
        {{ profile?.display_name.slice(0, 1) ?? "玄" }}
      </div>
      <p class="muted" style="text-align: center; margin-top: 8px">当前形象</p>
    </div>
    <div class="grid">
      <div class="card">
        <span class="muted">姓名</span>
        <div class="big">{{ profile?.display_name ?? "—" }}</div>
        <span class="tag">{{ profile?.system_name ?? "—" }}</span>
        <span class="tag">{{ profile?.realm_name ?? "—" }}</span>
      </div>
      <div class="card">
        <h3>当前状态</h3>
        <div class="grid three">
          <div>境界<br /><b>{{ profile?.realm_name ?? "—" }}</b></div>
          <div>体系<br /><b>{{ profile?.system_name ?? "—" }}</b></div>
          <div>战力<br /><b class="gold">{{ profile ? Math.round(profile.power).toLocaleString() : "—" }}</b></div>
        </div>
      </div>
      <div class="card">
        <h3>今日状态</h3>
        <template v-if="profile?.daily_state">
          <b>{{ profile.daily_state.name }}</b>
          <p class="muted">{{ profile.daily_state.description }}</p>
        </template>
        <p v-else class="muted">尚未生成，在群内发送「今日状态」。</p>
      </div>
    </div>
  </div>

  <div class="card" style="margin-top: 14px">
    <h3>更换形象</h3>
    <p class="muted">从素材库中选择形象，保存后群内卡片与网页同步生效。</p>
    <div v-if="portraits.length" class="picker-grid">
      <div
        v-for="id in portraits"
        :key="id"
        class="picker-item"
        :class="{ selected: currentId === id }"
        @click="selected = id"
      >
        <img :src="portraitUrl(id)" :alt="id" />
      </div>
    </div>
    <p v-else class="muted">素材库暂无可用形象，请联系管理员上传。</p>
    <p v-if="message" class="muted" style="margin-top: 10px">{{ message }}</p>
    <div style="margin-top: 12px">
      <button
        class="primary"
        :disabled="!selected || saving || selected === profile?.character_id"
        @click="save"
      >
        {{ saving ? "保存中……" : "保存形象" }}
      </button>
    </div>
  </div>
</template>
