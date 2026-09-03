<script setup lang="ts">
import { onMounted, ref } from "vue";
import { getEquipment, itemIcon, type Equipment } from "../api";

const equipment = ref<Equipment | null>(null);

const SLOTS: [string, string][] = [
  ["main_hand", "主手"],
  ["off_hand", "副手"],
  ["head", "头部"],
  ["body", "身体"],
  ["hands", "手部"],
  ["feet", "足部"],
  ["accessory_1", "饰品一"],
  ["accessory_2", "饰品二"],
];

onMounted(async () => {
  equipment.value = await getEquipment();
});
</script>

<template>
  <div class="card">
    <h3>装备栏</h3>
    <div class="inv-grid">
      <div
        v-for="[code, label] in SLOTS"
        :key="code"
        class="item"
        :class="
          equipment?.items.some((item) => item.equipped_slot === code)
            ? `rarity-${equipment.items.find((item) => item.equipped_slot === code)?.quality}`
            : ''
        "
      >
        <template v-if="equipment?.items.some((item) => item.equipped_slot === code)">
          <img
            :src="itemIcon(equipment.items.find((item) => item.equipped_slot === code)!.definition_id)"
            :alt="equipment.items.find((item) => item.equipped_slot === code)!.definition_id"
          />
          <div class="name">{{ equipment.items.find((item) => item.equipped_slot === code)!.definition_id }}</div>
        </template>
        <template v-else>
          <div class="glyph">空</div>
          <div class="name">{{ label }}</div>
        </template>
      </div>
    </div>
  </div>
  <div class="card" style="margin-top: 14px">
    <h3>背包</h3>
    <div v-if="equipment?.items.some((item) => !item.equipped_slot)" class="inv-grid">
      <div
        v-for="item in equipment.items.filter((item) => !item.equipped_slot)"
        :key="item.item_id"
        class="item"
        :class="`rarity-${item.quality}`"
      >
        <img :src="itemIcon(item.definition_id)" :alt="item.definition_id" />
        <span v-if="item.quantity > 1" class="qty">×{{ item.quantity }}</span>
        <div class="name">{{ item.definition_id }}</div>
      </div>
    </div>
    <p v-else class="muted">背包空空如也。</p>
    <p class="muted" style="margin-bottom: 0">
      群内指令：<code>装备</code> 查看卡片 · <code>装备 查看 &lt;编号&gt;</code> 看详情 ·
      <code>装备 穿戴 &lt;编号&gt; &lt;槽位&gt;</code>
    </p>
  </div>
</template>
