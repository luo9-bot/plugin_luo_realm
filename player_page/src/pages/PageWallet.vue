<script setup lang="ts">
import { onMounted, ref } from "vue";
import { getWallet, type Wallet } from "../api";

const wallet = ref<Wallet | null>(null);

const CURRENCIES: Record<string, string> = { coins: "金币", marks: "刻印" };
const REASONS: Record<string, string> = {
  daily_checkin: "每日签到",
  duel_reward: "决斗奖励",
  group_world_event: "世界事件",
  ascii_fpv_reward: "御空试炼",
  admin_adjustment: "管理员调整",
};

onMounted(async () => {
  wallet.value = await getWallet();
});
</script>

<template>
  <div class="grid three">
    <div v-for="balance in wallet?.balances ?? []" :key="balance.currency" class="card">
      <span class="muted">{{ CURRENCIES[balance.currency] ?? balance.currency }}</span>
      <div class="big gold">{{ balance.amount.toLocaleString() }}</div>
    </div>
  </div>
  <div class="card" style="margin-top: 14px">
    <h3>最近流水</h3>
    <div class="list">
      <div v-for="(tx, index) in wallet?.transactions ?? []" :key="index" class="row">
        <span>{{ REASONS[tx.reason] ?? tx.reason }}</span>
        <span>
          <b :style="{ color: tx.delta >= 0 ? 'var(--ok)' : 'var(--danger)' }">
            {{ tx.delta >= 0 ? "+" : "" }}{{ tx.delta }}
          </b>
          <span class="muted"> · 余额 {{ tx.balance_after }}</span>
        </span>
      </div>
      <div v-if="!wallet?.transactions.length" class="row">
        <span class="muted">暂无流水</span>
      </div>
    </div>
  </div>
</template>
