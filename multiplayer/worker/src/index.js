// The rival-courses leaderboard (README.md): a Cloudflare Worker with a D1 database.
//
// * POST /result: a player's result in this week's challenge (JSON, see readResult). Only their best of the week is kept.
// * GET /board?c=<week>&g=<group>&p=<player>: the top 50 of a week's board (the public one, or a friends' group), and
//   the asking player's own place.
// * GET /admin (password): the latest results, hidden ones too; POST /admin/hide hides a result or a nickname.
//
// Nothing about the player is kept but the random id their game made, the nickname they typed, their group code, the
// result's numbers and the game's platform and version: no address, no country, no cookies.
//
// Copyright the OpenSGolf contributors. GPL-2.0-or-later.

import { challenge, isWeekId, previousWeekId, score, weekId } from "./challenge.js";
import { cleanNick, generatedNick } from "./names.js";

/** Rows a board shows. */
export const BOARD_SIZE = 50;
/** Platforms the game names (as the play-time count does). */
export const PLATFORMS = new Set([
  "windows", "macos", "linux", "android", "web", "other",
  "windows-arm64", "macos-arm64", "linux-arm64", "other-arm64",
]);
/** The largest request body read. */
export const MAX_BODY = 4096;

const isPlayer = (s) => typeof s === "string" && /^[0-9a-f]{16,32}$/.test(s);
const isGroup = (s) => s === "" || (typeof s === "string" && /^[A-Z0-9]{6}$/.test(s));
const isVersion = (v) => typeof v === "string" && /^\d{1,3}\.\d{1,3}\.\d{1,3}$/.test(v);
const int = (v, lo, hi) => Number.isInteger(v) && v >= lo && v <= hi;

/** The weeks a result may be for at `now`: this one, and last week's (a week's game may end after midnight Sunday). */
export function openWeeks(now) {
  return [weekId(now), previousWeekId(now)];
}

/** A result's body, checked: what to store, or a string saying why not. */
export function readResult(body, now) {
  if (!body || typeof body !== "object") return "not a result";
  const { challenge: c, player, nick, group = "", platform, version, cheats, facts } = body;
  if (!isWeekId(c) || !openWeeks(now).includes(c)) return "not an open challenge";
  if (!isPlayer(player)) return "bad player id";
  if (!isGroup(group)) return "bad group code";
  if (!PLATFORMS.has(platform)) return "bad platform";
  if (!isVersion(version)) return "bad version";
  if (cheats !== false) return "cheats were on";
  if (!facts || typeof facts !== "object") return "no facts";
  const f = facts;
  if (!(int(f.year, 2001, 2100) && int(f.cash, -1e9, 1e10) && int(f.holes, 0, 36) && int(f.fun, -1e6, 1e6)
    && int(f.skill, -1e6, 1e6) && int(f.members, 0, 1e5) && int(f.rank, 0, 3))) return "bad facts";
  const ch = challenge(c);
  if (f.year <= ch.endYear) return "the challenge's year is not over";
  const n = cleanNick(nick);
  if (!n) return "bad nickname";
  return {
    challenge: c, player, nick: n, grp: group, score: score(ch, f), platform, version,
    cash: f.cash, fun: f.fun, skill: f.skill, members: f.members, holes: f.holes, rank: f.rank,
  };
}

/** Stores a result: a new row, or a better score for a row that is there (a worse or equal one changes nothing). */
export async function storeResult(db, r, now) {
  const at = now.toISOString();
  const res = await db.prepare(
    `INSERT INTO results (challenge, player, nick, grp, score, cash, fun, skill, members, holes, rank, platform, version, at)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
     ON CONFLICT (challenge, player) DO UPDATE SET nick = ?3, grp = ?4, score = ?5, cash = ?6, fun = ?7, skill = ?8,
       members = ?9, holes = ?10, rank = ?11, platform = ?12, version = ?13, at = ?14
     WHERE excluded.score > results.score`,
  ).bind(r.challenge, r.player, r.nick, r.grp, r.score, r.cash, r.fun, r.skill, r.members, r.holes, r.rank,
    r.platform, r.version, at).run();
  return (res.meta && res.meta.changes) > 0;
}

/** A week's board: the top rows and, for `player`, their own row and place (1 = first). */
export async function board(db, c, grp, player) {
  const { results } = await db.prepare(
    `SELECT r.player, CASE WHEN h.nick IS NULL THEN r.nick ELSE '' END AS nick, r.score, r.cash, r.fun, r.skill,
       r.members, r.holes, r.rank, r.at
     FROM results r LEFT JOIN hidden_nicks h ON h.nick = r.nick
     WHERE r.challenge = ?1 AND r.grp = ?2 AND r.hidden = 0
     ORDER BY r.score DESC, r.at ASC LIMIT ?3`,
  ).bind(c, grp, BOARD_SIZE).all();
  const rows = results.map((r, i) => ({
    place: i + 1, nick: r.nick || generatedNick(r.player), score: r.score, cash: r.cash, fun: r.fun, skill: r.skill,
    members: r.members, holes: r.holes, rank: r.rank, mine: r.player === player,
  }));
  let me = null;
  if (isPlayer(player) && !rows.some((r) => r.mine)) {
    const mine = await db.prepare(
      `SELECT score, at FROM results WHERE challenge = ?1 AND grp = ?2 AND player = ?3 AND hidden = 0`,
    ).bind(c, grp, player).first();
    if (mine) {
      const ahead = await db.prepare(
        `SELECT COUNT(*) AS n FROM results WHERE challenge = ?1 AND grp = ?2 AND hidden = 0
           AND (score > ?3 OR (score = ?3 AND at < ?4))`,
      ).bind(c, grp, mine.score, mine.at).first();
      me = { place: ahead.n + 1, score: mine.score };
    }
  }
  return { challenge: challenge(c), rows, me };
}

// ---------------------------------------------------------------- admin

/** Whether the request carries the admin password (basic auth, any user name). */
export function authorised(request, env) {
  const want = env.ADMIN_PASSWORD;
  if (!want) return false;
  const h = request.headers.get("Authorization") || "";
  if (!h.startsWith("Basic ")) return false;
  let pass = "";
  try {
    pass = atob(h.slice(6)).split(":").slice(1).join(":");
  } catch {
    return false;
  }
  if (pass.length !== want.length) return false;
  let diff = 0;
  for (let i = 0; i < want.length; i++) diff |= pass.charCodeAt(i) ^ want.charCodeAt(i);
  return diff === 0;
}

/** The latest results, hidden ones too, for the admin page. */
export async function recent(db, limit = 100) {
  const { results } = await db.prepare(
    `SELECT challenge, player, nick, grp, score, platform, version, at, hidden FROM results ORDER BY at DESC LIMIT ?1`,
  ).bind(limit).all();
  return results;
}

/** Hides (or shows again) one result, or a nickname everywhere. */
export async function hide(db, body, now) {
  if (body && isWeekId(body.challenge) && isPlayer(body.player)) {
    const r = await db.prepare(`UPDATE results SET hidden = ?3 WHERE challenge = ?1 AND player = ?2`)
      .bind(body.challenge, body.player, body.show ? 0 : 1).run();
    return r.meta.changes > 0;
  }
  if (body && typeof body.nick === "string" && body.nick.length > 0 && body.nick.length <= 32) {
    if (body.show) await db.prepare(`DELETE FROM hidden_nicks WHERE nick = ?1`).bind(body.nick).run();
    else {
      await db.prepare(`INSERT INTO hidden_nicks (nick, at) VALUES (?1, ?2) ON CONFLICT (nick) DO NOTHING`)
        .bind(body.nick, now.toISOString()).run();
    }
    return true;
  }
  return false;
}

// ---------------------------------------------------------------- the Worker

const json = (v, status = 200, extra = {}) => new Response(JSON.stringify(v), {
  status, headers: { "Content-Type": "application/json", "Access-Control-Allow-Origin": "*", ...extra },
});

async function readJson(request) {
  const text = await request.text();
  if (text.length > MAX_BODY) return null;
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    const now = new Date();
    if (request.method === "OPTIONS") {
      return new Response(null, { headers: {
        "Access-Control-Allow-Origin": "*", "Access-Control-Allow-Methods": "GET, POST", "Access-Control-Allow-Headers": "Content-Type",
      } });
    }
    if (url.pathname === "/result" && request.method === "POST") {
      const r = readResult(await readJson(request), now);
      if (typeof r === "string") return json({ ok: false, error: r }, 400);
      const better = await storeResult(env.DB, r, now);
      return json({ ok: true, better });
    }
    if (url.pathname === "/board" && request.method === "GET") {
      const c = url.searchParams.get("c") || weekId(now);
      const g = url.searchParams.get("g") || "";
      if (!isWeekId(c) || !isGroup(g)) return json({ ok: false, error: "bad board" }, 400);
      const b = await board(env.DB, c, g, url.searchParams.get("p") || "");
      // a board a minute old is fine: the edge may keep it that long
      return json({ ok: true, ...b }, 200, { "Cache-Control": url.searchParams.get("p") ? "no-store" : "public, max-age=60" });
    }
    if (url.pathname === "/challenge" && request.method === "GET") {
      return json({ ok: true, challenge: challenge(weekId(now)) }, 200, { "Cache-Control": "public, max-age=3600" });
    }
    if (url.pathname.startsWith("/admin")) {
      if (!authorised(request, env)) {
        return new Response("password needed", { status: 401, headers: { "WWW-Authenticate": 'Basic realm="board admin"' } });
      }
      if (url.pathname === "/admin" && request.method === "GET") return json({ ok: true, results: await recent(env.DB) });
      if (url.pathname === "/admin/hide" && request.method === "POST") {
        return json({ ok: await hide(env.DB, await readJson(request), now) });
      }
    }
    return json({ ok: false, error: "not found" }, 404);
  },
};
