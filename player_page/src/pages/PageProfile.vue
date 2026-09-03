<script setup lang="ts">
import { computed } from "vue";
import { portraitUrl, type Profile } from "../api";

const props = defineProps<{ profile: Profile | null }>();

const power = computed(() =>
  props.profile ? Math.round(props.profile.power).toLocaleString() : "—",
);
</script>

<template>
  <div v-if="profile" class="grid side">
    <img
      v-if="profile.character_id"
      class="portrait"
      :src="portraitUrl(profile.character_id)"
      alt="角色形象"
    />
    <div v-else class="portrait-fallback">{{ profile.display_name.slice(0, 1) }}</div>
    <div class="grid">
      <div class="card">
        <span class="muted">姓名</span>
        <div class="big">{{ profile.display_name }}</div>
        <span class="tag">{{ profile.system_name }}</span>
        <span class="tag">{{ profile.realm_name }}</span>
      </div>
      <div class="card">
        <h3>当前状态</h3>
        <div class="grid three">
          <div>境界<br /><b>{{ profile.realm_name }}</b></div>
          <div>体系<br /><b>{{ profile.system_name }}</b></div>
          <div>战力<br /><b class="gold">{{ power }}</b></div>
        </div>
      </div>
      <div class="card">
        <h3>今日状态</h3>
        <template v-if="profile.daily_state">
          <b>{{ profile.daily_state.name }}</b>
          <p class="muted">{{ profile.daily_state.description }}</p>
        </template>
        <p v-else class="muted">尚未生成，在群内发送「今日状态」。</p>
      </div>
      <div v-if="profile.biography" class="card">
        <h3>传记</h3>
        <p class="muted">{{ profile.biography }}</p>
      </div>
    </div>
  </div>
</template>
