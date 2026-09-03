<script setup lang="ts">
defineEmits<{ open: [section: "profile" | "equipment" | "skills" | "wallet" | "battles"] }>();
</script>

<template>
  <div class="menu-grid">
    <div class="card" @click="$emit('open', 'profile')">
      <div class="icon">◈</div><b>角色卡</b>
      <div class="muted">查看角色当前状态与今日状态</div>
    </div>
    <div class="card" @click="$emit('open', 'equipment')">
      <div class="icon">◇</div><b>装备</b>
      <div class="muted">装备栏与背包</div>
    </div>
    <div class="card" @click="$emit('open', 'skills')">
      <div class="icon">✦</div><b>技能</b>
      <div class="muted">已掌握技能与战术</div>
    </div>
    <div class="card" @click="$emit('open', 'wallet')">
      <div class="icon">◎</div><b>资产</b>
      <div class="muted">金币刻印与最近流水</div>
    </div>
    <div class="card" @click="$emit('open', 'battles')">
      <div class="icon">⚔</div><b>战斗</b>
      <div class="muted">最近的决斗记录</div>
    </div>
    <div class="card">
      <div class="icon">☯</div><b>群聊操作</b>
      <div class="muted">签到 / 修行行动 / 决斗 仍在群聊完成</div>
    </div>
  </div>
</template>
