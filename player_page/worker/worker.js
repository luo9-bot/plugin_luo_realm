/**
 * Luo Realm 玩家页面 Worker（参考实现，可按需调整后部署）。
 *
 * 职责与安全边界：
 * - POST /api/plugin/sync    插件 → CF：接收档案快照。凭据 = PLUGIN_TOKEN
 *   （常量时间比较），载荷上限 256 KB，仅允许写入自己的 D1。
 * - GET  /api/state          页面 → CF：凭据 = 页面令牌（查 D1 校验有效期），
 *   只返回该令牌对应的快照，绝不返回任何其他玩家数据。
 * - GET  /api/player/asset/* 页面 → CF → 源站：素材（图标 / 形象）按前缀
 *   透传，不转发任何凭据，带一天浏览器缓存。
 * - POST /api/command        页面 → CF → 源站：令牌校验通过后，转发到
 *   PLUGIN_URL（环境变量，玩家不可见）。只转发固定路径，强制 HTTPS，
 *   响应透传但不回显任何密钥。
 * - 跨源：仅 PAGES_ORIGIN 白名单内的页面来源获得 CORS 响应头。
 *
 * 不记录密钥与令牌；所有 SQL 使用预编译语句。
 */

const COMMAND_PATH = "/api/player/command";
const MAX_SYNC_BODY_BYTES = 256 * 1024;
const MAX_COMMAND_BODY_BYTES = 4 * 1024;

function constantTimeEquals(left, right) {
  const leftBytes = new TextEncoder().encode(left);
  const rightBytes = new TextEncoder().encode(right);
  if (leftBytes.length !== rightBytes.length) {
    return false;
  }
  let difference = 0;
  for (let index = 0; index < leftBytes.length; index += 1) {
    difference |= leftBytes[index] ^ rightBytes[index];
  }
  return difference === 0;
}

function bearerOf(request) {
  const header = request.headers.get("Authorization") ?? "";
  return header.startsWith("Bearer ") ? header.slice(7) : "";
}

function json(status, payload) {
  return new Response(JSON.stringify(payload), {
    status,
    headers: {
      "Content-Type": "application/json; charset=utf-8",
      // 经外部 CDN 回源时禁止缓存：快照与会话状态绑定，缓存会导致串号。
      "Cache-Control": "no-store",
    },
  });
}

async function handle(request, env) {
    const url = new URL(request.url);
    const now = Math.floor(Date.now() / 1000);

    if (request.method === "POST" && url.pathname === "/api/plugin/sync") {
      if (!constantTimeEquals(bearerOf(request), env.PLUGIN_TOKEN)) {
        return json(403, { ok: false, error: { code: "channel_forbidden", message: "forbidden" } });
      }
      const raw = await request.arrayBuffer();
      if (raw.byteLength > MAX_SYNC_BODY_BYTES) {
        return json(413, { ok: false, error: { code: "body_too_large", message: "payload too large" } });
      }
      let payload;
      try {
        payload = JSON.parse(new TextDecoder().decode(raw));
      } catch {
        return json(400, { ok: false, error: { code: "invalid_json", message: "invalid json" } });
      }
      const { token, expires_at: expiresAt, player_id: playerId, state } = payload ?? {};
      if (
        typeof token !== "string" || token.length !== 43 ||
        !Number.isInteger(expiresAt) || expiresAt <= now ||
        !Number.isInteger(playerId) || typeof state !== "object"
      ) {
        return json(400, { ok: false, error: { code: "invalid_snapshot", message: "invalid snapshot" } });
      }
      await env.DB.prepare(
        "INSERT INTO player_state(token, player_id, state_json, expires_at) VALUES(?1, ?2, ?3, ?4) " +
          "ON CONFLICT(token) DO UPDATE SET state_json=excluded.state_json, expires_at=excluded.expires_at",
      )
        .bind(token, playerId, JSON.stringify(state), expiresAt)
        .run();
      await env.DB.prepare("DELETE FROM player_state WHERE expires_at <= ?1").bind(now).run();
      return json(200, { ok: true });
    }

    if (request.method === "GET" && url.pathname === "/api/state") {
      const token = bearerOf(request);
      if (token.length !== 43) {
        return json(401, { ok: false, error: { code: "session_invalid", message: "invalid session" } });
      }
      const row = await env.DB.prepare(
        "SELECT state_json, expires_at FROM player_state WHERE token=?1 AND expires_at > ?2",
      )
        .bind(token, now)
        .first();
      if (!row) {
        return json(401, { ok: false, error: { code: "session_invalid", message: "invalid session" } });
      }
      return json(200, { ok: true, data: JSON.parse(row.state_json) });
    }

    // 素材（物品图标 / 玩家形象）由 <img> 直接引用，无法携带 Authorization，
    // 源站对素材端点本身公开；Worker 仅按前缀透传，不转发任何凭据。
    if (request.method === "GET" && url.pathname.startsWith("/api/player/asset/")) {
      const origin = (env.PLUGIN_URL ?? "").trim();
      if (!origin.startsWith("https://") || origin.includes("?") || origin.includes("#")) {
        return json(500, { ok: false, error: { code: "origin_misconfigured", message: "server error" } });
      }
      try {
        const upstream = await fetch(`${origin}${url.pathname}${url.search}`);
        return new Response(upstream.body, {
          status: upstream.status,
          headers: {
            "Content-Type": upstream.headers.get("Content-Type") ?? "application/octet-stream",
            "Cache-Control": "public, max-age=86400",
          },
        });
      } catch {
        return json(502, { ok: false, error: { code: "origin_unreachable", message: "server error" } });
      }
    }

    if (request.method === "POST" && url.pathname === "/api/command") {
      const pluginToken = bearerOf(request);
      if (!constantTimeEquals(pluginToken, env.PLUGIN_TOKEN)) {
        return json(403, { ok: false, error: { code: "channel_forbidden", message: "forbidden" } });
      }
      const raw = await request.arrayBuffer();
      if (raw.byteLength > MAX_COMMAND_BODY_BYTES) {
        return json(413, { ok: false, error: { code: "body_too_large", message: "payload too large" } });
      }
      let payload;
      try {
        payload = JSON.parse(new TextDecoder().decode(raw));
      } catch {
        return json(400, { ok: false, error: { code: "invalid_json", message: "invalid json" } });
      }
      const { token } = payload ?? {};
      if (typeof token !== "string" || token.length !== 43) {
        return json(401, { ok: false, error: { code: "session_invalid", message: "invalid session" } });
      }
      const row = await env.DB.prepare(
        "SELECT expires_at FROM player_state WHERE token=?1 AND expires_at > ?2",
      )
        .bind(token, now)
        .first();
      if (!row) {
        return json(401, { ok: false, error: { code: "session_invalid", message: "invalid session" } });
      }

      const origin = (env.PLUGIN_URL ?? "").trim();
      if (!origin.startsWith("https://") || origin.includes("?") || origin.includes("#")) {
        return json(500, { ok: false, error: { code: "origin_misconfigured", message: "server error" } });
      }
      let forwarded;
      try {
        forwarded = await fetch(`${origin}${COMMAND_PATH}`, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            Authorization: `Bearer ${env.PLUGIN_TOKEN}`,
          },
          body: JSON.stringify(payload),
        });
      } catch {
        return json(502, { ok: false, error: { code: "origin_unreachable", message: "server error" } });
      }
      const text = await forwarded.text();
      return new Response(text, {
        status: forwarded.status,
        headers: {
          "Content-Type": "application/json; charset=utf-8",
          "Cache-Control": "no-store",
        },
      });
    }

    return json(404, { ok: false, error: { code: "not_found", message: "not found" } });
}

/**
 * 跨源边界：Pages 站点（PAGES_ORIGIN 白名单）跨源调用 API 时，浏览器对
 * 带 Authorization / JSON body 的请求先发预检，由统一入口短路应答；命中
 * 白名单的业务响应统一补 CORS 头。素材 <img> 属 no-cors 请求，不涉及预检。
 */
export default {
  async fetch(request, env) {
    const origin = request.headers.get("Origin") ?? "";
    const allowed = origin !== "" && origin === (env.PAGES_ORIGIN ?? "").trim();
    if (request.method === "OPTIONS") {
      if (!allowed) {
        return json(403, { ok: false, error: { code: "origin_forbidden", message: "forbidden" } });
      }
      return new Response(null, {
        status: 204,
        headers: {
          "Access-Control-Allow-Origin": origin,
          "Access-Control-Allow-Methods": "GET, POST, OPTIONS",
          "Access-Control-Allow-Headers": "Authorization, Content-Type",
          "Access-Control-Max-Age": "86400",
        },
      });
    }
    const response = await handle(request, env);
    if (allowed) {
      response.headers.set("Access-Control-Allow-Origin", origin);
      response.headers.set("Vary", "Origin");
    }
    return response;
  },
};
