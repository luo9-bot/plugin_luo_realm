<script setup lang="ts">
import { onMounted, ref } from "vue";
import { getSkills, type Skills } from "../api";

const skills = ref<Skills | null>(null);

const TACTICS: Record<string, string> = {
  balanced: "均衡",
  aggressive: "强攻",
  defensive: "守御",
  sustain: "续航",
  control: "控制",
};

onMounted(async () => {
  skills.value = await getSkills();
});
</script>

<template>
  <div class="card">
    <h3>当前战术</h3>
    <span class="tag">{{ skills ? (TACTICS[skills.tactic] ?? skills.tactic) : "—" }}</span>
    <p class="muted" style="margin-bottom: 0">
      群内指令：<code>战术 &lt;代码&gt;</code> 可切换。
    </p>
  </div>
  <div class="card" style="margin-top: 14px">
    <h3>已掌握技能</h3>
    <div class="list">
      <div v-for="skill in skills?.skills ?? []" :key="skill.id" class="row">
        <span>{{ skill.name }}</span>
        <span class="pips">
          <i v-for="pip in 3" :key="pip" :class="{ on: skill.mastery >= pip }"></i>
        </span>
      </div>
      <div v-if="!skills?.skills.length" class="row"><span class="muted">尚未掌握技能</span></div>
    </div>
  </div>
</template>
