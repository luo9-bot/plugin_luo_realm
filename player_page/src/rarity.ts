/** 品阶注册表的前端镜像：颜色、星数与显示名全部来自服务端规则注册表。 */
import { ref } from "vue";
import { API_BASE } from "./api";

export interface RarityTier {
  code: string;
  display: string;
  color: string;
  stars: number;
}

const tiers = ref<RarityTier[]>([]);
const loaded = ref(false);

export async function loadRarity(): Promise<void> {
  if (loaded.value) {
    return;
  }
  const response = await fetch(`${API_BASE}/api/player/meta/rarity`);
  const payload = (await response.json()) as {
    ok: boolean;
    data?: { tiers: RarityTier[] };
  };
  if (payload.ok && payload.data) {
    tiers.value = payload.data.tiers;
    loaded.value = true;
  }
}

export function tierOf(code: string): RarityTier | undefined {
  return tiers.value.find((tier) => tier.code === code);
}

/** 物品格的行内样式：品阶色环直接取注册表颜色。 */
export function tierBorderStyle(code: string): string {
  const tier = tierOf(code);
  return tier ? `border-color: ${tier.color}` : "";
}
