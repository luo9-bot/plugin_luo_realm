<script setup lang="ts">
import { onMounted, ref } from "vue";
import { getBattles, type Battles } from "../api";

const battles = ref<Battles | null>(null);

onMounted(async () => {
  battles.value = await getBattles();
});

function when(startedAt: number): string {
  const ended = new Date(startedAt * 1000);
  return `${ended.getMonth() + 1}月${ended.getDate()}日 ${String(ended.getHours()).padStart(2, "0")}:${String(ended.getMinutes()).padStart(2, "0")}`;
}
</script>

<template>
  <div class="card">
    <h3>最近决斗</h3>
    <div class="list">
      <div v-for="battle in battles?.battles ?? []" :key="battle.combat_id" class="row">
        <span>
          {{ when(battle.started_at) }}
          <span class="muted">· 战力 {{ battle.power }} · 规则 v{{ battle.rule_version }}</span>
        </span>
        <b :style="{ color: battle.team === battle.winner_team ? 'var(--ok)' : 'var(--danger)' }">
          {{ battle.team === battle.winner_team ? "胜利" : "败北" }}
        </b>
      </div>
      <div v-if="!battles?.battles.length" class="row">
        <span class="muted">还没有战斗记录</span>
      </div>
    </div>
  </div>
</template>
