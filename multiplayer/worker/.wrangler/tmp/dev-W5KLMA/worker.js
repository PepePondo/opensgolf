var __defProp = Object.defineProperty;
var __name = (target, value) => __defProp(target, "name", { value, configurable: true });

// src/challenge.js
var MEASURES = ["cash", "fun", "members", "skill"];
var THEMES = ["parkland", "desert", "tropical", "links"];
var DIFFICULTIES = ["easy", "moderate", "difficult"];
var END_YEAR = 2001;
function fnv1a(s) {
  let h = 2166136261;
  for (const b of new TextEncoder().encode(s)) {
    h ^= b;
    h = Math.imul(h, 16777619) >>> 0;
  }
  return h >>> 0;
}
__name(fnv1a, "fnv1a");
function weekId(d) {
  const t = new Date(Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate()));
  const day = t.getUTCDay() || 7;
  t.setUTCDate(t.getUTCDate() + 4 - day);
  const year = t.getUTCFullYear();
  const jan1 = Date.UTC(year, 0, 1);
  const week = Math.floor((t - jan1) / 864e5 / 7) + 1;
  return `${year}-W${String(week).padStart(2, "0")}`;
}
__name(weekId, "weekId");
function isWeekId(id) {
  return typeof id === "string" && /^\d{4}-W(0[1-9]|[1-4]\d|5[0-3])$/.test(id);
}
__name(isWeekId, "isWeekId");
function previousWeekId(d) {
  return weekId(new Date(d.getTime() - 7 * 864e5));
}
__name(previousWeekId, "previousWeekId");
function challenge(id) {
  const h = fnv1a(`opensgolf-challenge/${id}`);
  return {
    id,
    measure: MEASURES[h % 4],
    theme: THEMES[(h >>> 2) % 4],
    difficulty: DIFFICULTIES[(h >>> 4) % 3],
    // the new game's seed (the land, the golfers' arrivals)
    seed: h >>> 8,
    endYear: END_YEAR
  };
}
__name(challenge, "challenge");
function score(c, facts) {
  return facts[c.measure];
}
__name(score, "score");

// src/names.js
var NICK_MIN = 3;
var NICK_MAX = 16;
var BLOCKED = [
  "fuck",
  "shit",
  "cunt",
  "bitch",
  "nigg",
  "fag",
  "rape",
  "nazi",
  "hitler",
  "whore",
  "slut",
  "dick",
  "cock",
  "pussy",
  "penis",
  "vagina",
  "porn",
  "retard",
  "kike",
  "spic",
  "chink",
  "wank",
  "twat",
  "bastard",
  "asshole"
];
var FOLD = { 0: "o", 1: "i", 3: "e", 4: "a", 5: "s", 7: "t", 8: "b", "@": "a", $: "s", "!": "i", "|": "i" };
function clean(s) {
  const f = folded(s);
  return !BLOCKED.some((w) => f.includes(w));
}
__name(clean, "clean");
function folded(s) {
  return [...s.toLowerCase()].map((c) => FOLD[c] ?? c).filter((c) => c >= "a" && c <= "z").join("");
}
__name(folded, "folded");
function cleanNick(nick) {
  if (typeof nick !== "string") return null;
  const n = nick.trim().replace(/\s+/g, " ");
  if (n.length < NICK_MIN || n.length > NICK_MAX) return null;
  if (!/^[A-Za-z0-9 _.\-]+$/.test(n)) return null;
  const f = folded(n);
  if (BLOCKED.some((w) => f.includes(w))) return null;
  return n;
}
__name(cleanNick, "cleanNick");
var FIRST = [
  "Birdie",
  "Eagle",
  "Bogey",
  "Fairway",
  "Bunker",
  "Caddie",
  "Divot",
  "Mulligan",
  "Putter",
  "Wedge",
  "Links",
  "Albatross"
];
var SECOND = ["Fan", "Pro", "Ace", "Walker", "Swinger", "Chipper", "Driver", "Golfer", "Hacker", "Champ"];
function generatedNick(player) {
  const h = fnv1a(`nick/${player}`);
  return `${FIRST[h % FIRST.length]} ${SECOND[(h >>> 8) % SECOND.length]} ${String((h >>> 16) % 1e3).padStart(3, "0")}`;
}
__name(generatedNick, "generatedNick");

// src/room.js
var TICK_MS = 87;
var LEAD_MS = 700;
var pausedAt = /* @__PURE__ */ __name((tick, ms) => ({ base_tick: tick, base_ms: ms, rate: 0 }), "pausedAt");
function tickAt(c, ms) {
  if (c.rate === 0 || ms <= c.base_ms) return c.base_tick;
  return c.base_tick + Math.floor((ms - c.base_ms) * c.rate / TICK_MS);
}
__name(tickAt, "tickAt");
var withRate = /* @__PURE__ */ __name((c, ms, rate) => ({ base_tick: tickAt(c, ms), base_ms: ms, rate }), "withRate");
var clampRate = /* @__PURE__ */ __name((r) => r >= 4 ? 4 : r >= 2 ? 2 : r >= 1 ? 1 : 0, "clampRate");
var RoomCore = class {
  static {
    __name(this, "RoomCore");
  }
  constructor(clock, nextSeq = 0) {
    this.clock = clock;
    this.nextSeq = nextSeq;
  }
  stamp(ms, player, action) {
    this.nextSeq += 1;
    const tick = Math.max(tickAt(this.clock, ms + LEAD_MS), tickAt(this.clock, ms) + 1);
    return { Act: { tick, seq: this.nextSeq, player, action } };
  }
  /** Sets the clock's rate at `ms`: the Clock message, and (when it goes on) the speed as a stamped action. */
  setRate(ms, player, rate) {
    this.clock = withRate(this.clock, ms, clampRate(rate));
    const out = [{ Clock: this.clock }];
    if (this.clock.rate > 0) out.push(this.stamp(ms, player, { speed: this.clock.rate }));
    return out;
  }
  /** A game's {"Act": ...} or {"Speed": n} at server time `ms`: what everyone is sent, in order (Room::handle). */
  handle(ms, player, m) {
    if (m.Act !== void 0) {
      if (m.Act && typeof m.Act === "object" && "speed" in m.Act) return this.setRate(ms, player, m.Act.speed);
      return [this.stamp(ms, player, m.Act)];
    }
    if (m.Speed !== void 0) return this.setRate(ms, player, m.Speed);
    return [];
  }
};

// src/club.js
var PROTOCOL = 1;
var MAX_PLAYERS = 8;
var MAX_MESSAGE = 4 * 1024 * 1024;
var MAX_ACTION = 4096;
var SAVE_EVERY = 2e3;
var CHECKS_KEPT = 32;
function mismatches(ours, club) {
  if (!ours || typeof ours !== "object") return ["protocol"];
  const parts = [];
  if (ours.protocol !== club.protocol) parts.push("protocol");
  if (ours.version !== club.version) parts.push("version");
  if (ours.log !== club.log) parts.push("log");
  if (ours.fix_bugs !== club.fix_bugs || ours.random_trees !== club.random_trees) parts.push("rules");
  if (ours.map !== club.map) parts.push("map");
  if (ours.extras !== club.extras) parts.push("extras");
  if (JSON.stringify(ours.packs) !== JSON.stringify(club.packs)) parts.push("packs");
  return parts;
}
__name(mismatches, "mismatches");
function isCompat(c) {
  return !!c && typeof c === "object" && c.protocol === PROTOCOL && typeof c.version === "string" && c.version.length <= 20 && Number.isInteger(c.log) && typeof c.fix_bugs === "boolean" && typeof c.random_trees === "boolean" && Number.isInteger(c.map) && Number.isInteger(c.extras) && Array.isArray(c.packs) && c.packs.length <= 64 && c.packs.every((p) => typeof p === "string" && p.length <= 80);
}
__name(isCompat, "isCompat");
var isPlayerId = /* @__PURE__ */ __name((s) => typeof s === "string" && /^[0-9a-f]{16,32}$/.test(s), "isPlayerId");
var Club = class {
  static {
    __name(this, "Club");
  }
  constructor(ctx, env) {
    this.ctx = ctx;
    this.env = env;
    this.sql = ctx.storage.sql;
    this.sql.exec(`CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v TEXT NOT NULL)`);
    this.sql.exec(`CREATE TABLE IF NOT EXISTS log (seq INTEGER PRIMARY KEY, tick INTEGER NOT NULL, msg TEXT NOT NULL)`);
    this.sql.exec(`CREATE TABLE IF NOT EXISTS save (id INTEGER PRIMARY KEY, tick INTEGER NOT NULL, data TEXT NOT NULL)`);
    this.sql.exec(`CREATE TABLE IF NOT EXISTS seats (player TEXT PRIMARY KEY, me INTEGER NOT NULL)`);
    this.checks = /* @__PURE__ */ new Map();
    this.asked = null;
    this.load();
  }
  // ------------------------------------------------------------ storage
  get(k) {
    const r = this.sql.exec(`SELECT v FROM meta WHERE k = ?`, k).toArray()[0];
    return r ? JSON.parse(r.v) : null;
  }
  put(k, v) {
    this.sql.exec(`INSERT INTO meta (k, v) VALUES (?, ?) ON CONFLICT (k) DO UPDATE SET v = excluded.v`, k, JSON.stringify(v));
  }
  load() {
    this.info = this.get("info");
    const room = this.get("room") || { clock: pausedAt(0, 0), seq: 0, want: 1 };
    this.room = new RoomCore(room.clock, room.seq);
    this.want = room.want;
  }
  saveRoom() {
    this.put("room", { clock: this.room.clock, seq: this.room.nextSeq, want: this.want });
  }
  saved() {
    return this.sql.exec(`SELECT tick, data FROM save WHERE id = 1`).toArray()[0] || null;
  }
  savedTick() {
    const r = this.sql.exec(`SELECT tick FROM save WHERE id = 1`).toArray()[0];
    return r ? r.tick : null;
  }
  /** This player's number in the club (kept, so a player keeps theirs). */
  seat(player) {
    const r = this.sql.exec(`SELECT me FROM seats WHERE player = ?`, player).toArray()[0];
    if (r) return r.me;
    const n = this.sql.exec(`SELECT COALESCE(MAX(me), 0) + 1 AS n FROM seats`).one().n;
    this.sql.exec(`INSERT INTO seats (player, me) VALUES (?, ?)`, player, n);
    return n;
  }
  // ------------------------------------------------------------ sockets
  /** The sockets of players who have been welcomed, oldest first. */
  players(except) {
    return this.ctx.getWebSockets().filter((ws) => ws !== except && ws.deserializeAttachment()?.me).sort((a, b) => a.deserializeAttachment().since - b.deserializeAttachment().since);
  }
  send(ws, m) {
    try {
      ws.send(JSON.stringify(m));
    } catch {
    }
  }
  broadcast(list) {
    if (!list.length) return;
    for (const ws of this.players()) for (const m of list) this.send(ws, m);
  }
  /** Stores and sends what the room said. */
  publish(out) {
    for (const m of out) {
      if (m.Act) this.sql.exec(`INSERT INTO log (seq, tick, msg) VALUES (?, ?, ?)`, m.Act.seq, m.Act.tick, JSON.stringify(m));
    }
    this.saveRoom();
    this.broadcast(out);
  }
  seats() {
    return this.players().map((ws) => {
      const a = ws.deserializeAttachment();
      return { me: a.me, nick: a.nick };
    });
  }
  /** The club list's row (D1, when the Worker has the database), after anyone joins or leaves or a save. */
  async list() {
    if (!this.env.DB || !this.info) return;
    const on = this.players().length;
    const tick = tickAt(this.room.clock, Date.now());
    await this.env.DB.prepare(
      `INSERT INTO clubs (id, name, version, compat, listed, players, tick, made, at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
       ON CONFLICT (id) DO UPDATE SET players = ?6, tick = ?7, at = ?9`
    ).bind(
      this.info.id,
      this.info.name,
      this.info.compat.version,
      JSON.stringify(this.info.compat),
      this.info.listed ? 1 : 0,
      on,
      tick,
      this.info.made,
      (/* @__PURE__ */ new Date()).toISOString()
    ).run();
  }
  // ------------------------------------------------------------ requests
  async fetch(request) {
    const url = new URL(request.url);
    if (url.pathname === "/init" && request.method === "POST") {
      if (this.info) return new Response("made already", { status: 409 });
      const b = await request.json();
      this.info = { id: b.id, name: b.name, compat: b.compat, owner: b.owner, listed: !!b.listed, made: (/* @__PURE__ */ new Date()).toISOString() };
      this.put("info", this.info);
      await this.list();
      return new Response("ok");
    }
    if (url.pathname === "/ws") {
      if (request.headers.get("Upgrade") !== "websocket") return new Response("a WebSocket", { status: 426 });
      if (!this.info) return new Response("no such club", { status: 404 });
      const pair = new WebSocketPair();
      this.ctx.acceptWebSocket(pair[1]);
      return new Response(null, { status: 101, webSocket: pair[0] });
    }
    if (url.pathname === "/state") {
      return Response.json({ info: this.info, clock: this.room.clock, saved: this.savedTick(), on: this.seats() });
    }
    return new Response("not found", { status: 404 });
  }
  async webSocketMessage(ws, text) {
    if (typeof text !== "string" || text.length > MAX_MESSAGE) return this.refuse(ws, "message too big");
    let m;
    try {
      m = JSON.parse(text);
    } catch {
      return this.refuse(ws, "not a message");
    }
    if (!m || typeof m !== "object") return this.refuse(ws, "not a message");
    const a = ws.deserializeAttachment();
    const now = Date.now();
    if (m.Ping !== void 0) return this.send(ws, { Pong: [Number(m.Ping) || 0, now] });
    if (m.Hello) return this.hello(ws, m.Hello, now);
    if (!a?.me) return this.refuse(ws, "say hello first");
    if (m.Act !== void 0) {
      if (JSON.stringify(m.Act).length > MAX_ACTION) return;
      if (m.Act === "undo" || m.Act && typeof m.Act === "object" && "cheat" in m.Act) return;
      if (m.Act && typeof m.Act === "object" && "speed" in m.Act) this.want = m.Act.speed;
      this.publish(this.room.handle(now, a.me, { Act: m.Act }));
    } else if (m.Speed !== void 0) {
      if (!Number.isInteger(m.Speed)) return;
      this.want = m.Speed;
      this.publish(this.room.handle(now, a.me, { Speed: m.Speed }));
    } else if (m.Save) {
      await this.takeSave(ws, a, m.Save, now);
    } else if (m.Check) {
      this.check(ws, m.Check);
    }
    this.maybeAsk(now);
  }
  refuse(ws, reason, parts = []) {
    this.send(ws, { Refused: { reason, parts } });
    try {
      ws.close(1008, reason.slice(0, 100));
    } catch {
    }
  }
  async hello(ws, h, now) {
    if (h.protocol !== PROTOCOL) return this.refuse(ws, "update the game", ["protocol"]);
    if (!isPlayerId(h.player)) return this.refuse(ws, "bad player id");
    const parts = mismatches(h.compat, this.info.compat);
    if (parts.length) return this.refuse(ws, "this club plays a different game", parts);
    for (const o of this.players(ws)) {
      if (o.deserializeAttachment().player === h.player) {
        o.serializeAttachment(null);
        this.refuse(o, "joined again elsewhere");
      }
    }
    const others = this.players(ws);
    if (others.length >= MAX_PLAYERS) return this.refuse(ws, "the club is full");
    const save = this.saved();
    if (!save && h.player !== this.info.owner) return this.refuse(ws, "the club is starting: try again in a minute");
    const me = this.seat(h.player);
    const nick = typeof h.nick === "string" ? h.nick.slice(0, 24) : "";
    ws.serializeAttachment({ me, player: h.player, nick, since: now });
    if (!save) {
      this.room.clock = pausedAt(Math.max(0, h.tick | 0), now);
      this.saveRoom();
      this.send(ws, { Welcome: { me, ms: now, clock: this.room.clock } });
      this.asked = { tick: this.room.clock.base_tick, me };
      this.send(ws, { SaveAsk: this.room.clock.base_tick });
    } else {
      this.send(ws, { Welcome: { me, ms: now, clock: this.room.clock } });
      this.send(ws, { Save: { tick: save.tick, data: save.data } });
      for (const r of this.sql.exec(`SELECT msg FROM log WHERE tick >= ? ORDER BY tick, seq`, save.tick)) this.send(ws, JSON.parse(r.msg));
      if (others.length === 0 && this.room.clock.rate === 0 && this.want > 0) this.publish(this.room.setRate(now, me, this.want));
    }
    this.broadcast([{ Players: this.seats() }]);
    await this.list();
  }
  async takeSave(ws, a, s, now) {
    if (!Number.isInteger(s.tick) || typeof s.data !== "string" || !s.data.length) return;
    const have = this.savedTick();
    if (have !== null && s.tick < have) return;
    if (s.tick > tickAt(this.room.clock, now)) return;
    if (this.asked && this.asked.me !== a.me) return;
    this.sql.exec(`INSERT INTO save (id, tick, data) VALUES (1, ?, ?) ON CONFLICT (id) DO UPDATE SET tick = excluded.tick,
      data = excluded.data`, s.tick, s.data);
    this.sql.exec(`DELETE FROM log WHERE tick < ?`, s.tick);
    this.asked = null;
    if (have === null && this.want > 0) this.publish(this.room.setRate(now, a.me, this.want));
    await this.list();
  }
  /** Asks the longest-on player for a save when the latest is old (or its asked player left). */
  maybeAsk(now) {
    const list = this.players();
    if (!list.length) return;
    if (this.asked && list.some((ws2) => ws2.deserializeAttachment().me === this.asked.me)) return;
    const have = this.savedTick();
    if (have === null && this.asked) this.asked = null;
    const tick = tickAt(this.room.clock, now);
    if (have !== null && tick - have < SAVE_EVERY) return;
    const ws = list[0];
    const at = Math.max(tickAt(this.room.clock, now + LEAD_MS), tick);
    this.asked = { tick: at, me: ws.deserializeAttachment().me };
    this.send(ws, { SaveAsk: at });
  }
  check(ws, c) {
    if (!Number.isInteger(c.tick) || typeof c.hash !== "string" || c.hash.length > 32) return;
    const first = this.checks.get(c.tick);
    if (first === void 0) {
      this.checks.set(c.tick, c.hash);
      if (this.checks.size > CHECKS_KEPT) this.checks.delete(this.checks.keys().next().value);
    } else if (first !== c.hash) {
      this.send(ws, { Reload: c.tick });
    }
  }
  async webSocketClose(ws, code) {
    try {
      ws.close(code === 1005 || code === 1006 ? 1e3 : code, "bye");
    } catch {
    }
    await this.left(ws);
  }
  async webSocketError(ws) {
    await this.left(ws);
  }
  async left(ws) {
    const a = ws.deserializeAttachment();
    ws.serializeAttachment(null);
    if (!a?.me) return;
    const now = Date.now();
    if (this.players(ws).length === 0) {
      if (this.room.clock.rate > 0) {
        this.room.clock = withRate(this.room.clock, now, 0);
        this.saveRoom();
      }
      this.asked = null;
    } else {
      this.broadcast([{ Players: this.seats() }]);
      this.maybeAsk(now);
    }
    await this.list();
  }
};

// src/index.js
var BOARD_SIZE = 50;
var PLATFORMS = /* @__PURE__ */ new Set([
  "windows",
  "macos",
  "linux",
  "android",
  "web",
  "other",
  "windows-arm64",
  "macos-arm64",
  "linux-arm64",
  "other-arm64"
]);
var PROTOCOL2 = 1;
var MAX_BODY = 4096;
var isPlayer = /* @__PURE__ */ __name((s) => typeof s === "string" && /^[0-9a-f]{16,32}$/.test(s), "isPlayer");
var isGroup = /* @__PURE__ */ __name((s) => s === "" || typeof s === "string" && /^[A-Z0-9]{6}$/.test(s), "isGroup");
var isVersion = /* @__PURE__ */ __name((v) => typeof v === "string" && /^\d{1,3}\.\d{1,3}\.\d{1,3}$/.test(v), "isVersion");
function newer(a, b) {
  const pa = a.split(".").map(Number), pb = b.split(".").map(Number);
  for (let i = 0; i < 3; i++) if (pa[i] !== pb[i]) return pa[i] > pb[i];
  return false;
}
__name(newer, "newer");
var int = /* @__PURE__ */ __name((v, lo, hi) => Number.isInteger(v) && v >= lo && v <= hi, "int");
function openWeeks(now) {
  return [weekId(now), previousWeekId(now)];
}
__name(openWeeks, "openWeeks");
function readResult(body, now, minVersion = "") {
  if (!body || typeof body !== "object") return "not a result";
  const { protocol, challenge: c, player, nick, group = "", platform, version, cheats, facts } = body;
  if (protocol !== PROTOCOL2) return "update the game";
  if (!isWeekId(c) || !openWeeks(now).includes(c)) return "not an open challenge";
  if (!isPlayer(player)) return "bad player id";
  if (!isGroup(group)) return "bad group code";
  if (!PLATFORMS.has(platform)) return "bad platform";
  if (!isVersion(version)) return "bad version";
  if (minVersion && newer(minVersion, version)) return "update the game";
  if (cheats !== false) return "cheats were on";
  if (!facts || typeof facts !== "object") return "no facts";
  const f = facts;
  if (!(int(f.year, 2001, 2100) && int(f.cash, -1e9, 1e10) && int(f.holes, 0, 36) && int(f.fun, -1e6, 1e6) && int(f.skill, -1e6, 1e6) && int(f.members, 0, 1e5) && int(f.rank, 0, 3))) return "bad facts";
  const ch = challenge(c);
  if (f.year <= ch.endYear) return "the challenge's year is not over";
  const n = cleanNick(nick);
  if (!n) return "bad nickname";
  return {
    challenge: c,
    player,
    nick: n,
    grp: group,
    score: score(ch, f),
    platform,
    version,
    cash: f.cash,
    fun: f.fun,
    skill: f.skill,
    members: f.members,
    holes: f.holes,
    rank: f.rank
  };
}
__name(readResult, "readResult");
async function storeResult(db, r, now) {
  const at = now.toISOString();
  const res = await db.prepare(
    `INSERT INTO results (challenge, player, nick, grp, score, cash, fun, skill, members, holes, rank, platform, version, at)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
     ON CONFLICT (challenge, player) DO UPDATE SET nick = ?3, grp = ?4, score = ?5, cash = ?6, fun = ?7, skill = ?8,
       members = ?9, holes = ?10, rank = ?11, platform = ?12, version = ?13, at = ?14
     WHERE excluded.score > results.score`
  ).bind(
    r.challenge,
    r.player,
    r.nick,
    r.grp,
    r.score,
    r.cash,
    r.fun,
    r.skill,
    r.members,
    r.holes,
    r.rank,
    r.platform,
    r.version,
    at
  ).run();
  return (res.meta && res.meta.changes) > 0;
}
__name(storeResult, "storeResult");
async function board(db, c, grp, player) {
  const { results } = await db.prepare(
    `SELECT r.player, CASE WHEN h.nick IS NULL THEN r.nick ELSE '' END AS nick, r.score, r.cash, r.fun, r.skill,
       r.members, r.holes, r.rank, r.at
     FROM results r LEFT JOIN hidden_nicks h ON h.nick = r.nick
     WHERE r.challenge = ?1 AND r.grp = ?2 AND r.hidden = 0
     ORDER BY r.score DESC, r.at ASC LIMIT ?3`
  ).bind(c, grp, BOARD_SIZE).all();
  const rows = results.map((r, i) => ({
    place: i + 1,
    nick: r.nick || generatedNick(r.player),
    score: r.score,
    cash: r.cash,
    fun: r.fun,
    skill: r.skill,
    members: r.members,
    holes: r.holes,
    rank: r.rank,
    mine: r.player === player
  }));
  let me = null;
  if (isPlayer(player) && !rows.some((r) => r.mine)) {
    const mine = await db.prepare(
      `SELECT score, at FROM results WHERE challenge = ?1 AND grp = ?2 AND player = ?3 AND hidden = 0`
    ).bind(c, grp, player).first();
    if (mine) {
      const ahead = await db.prepare(
        `SELECT COUNT(*) AS n FROM results WHERE challenge = ?1 AND grp = ?2 AND hidden = 0
           AND (score > ?3 OR (score = ?3 AND at < ?4))`
      ).bind(c, grp, mine.score, mine.at).first();
      me = { place: ahead.n + 1, score: mine.score };
    }
  }
  return { challenge: challenge(c), rows, me };
}
__name(board, "board");
function authorised(request, env) {
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
__name(authorised, "authorised");
async function recent(db, limit = 100) {
  const { results } = await db.prepare(
    `SELECT challenge, player, nick, grp, score, platform, version, at, hidden FROM results ORDER BY at DESC LIMIT ?1`
  ).bind(limit).all();
  return results;
}
__name(recent, "recent");
async function hide(db, body, now) {
  if (body && isWeekId(body.challenge) && isPlayer(body.player)) {
    const r = await db.prepare(`UPDATE results SET hidden = ?3 WHERE challenge = ?1 AND player = ?2`).bind(body.challenge, body.player, body.show ? 0 : 1).run();
    return r.meta.changes > 0;
  }
  if (body && typeof body.nick === "string" && body.nick.length > 0 && body.nick.length <= 32) {
    if (body.show) await db.prepare(`DELETE FROM hidden_nicks WHERE nick = ?1`).bind(body.nick).run();
    else {
      await db.prepare(`INSERT INTO hidden_nicks (nick, at) VALUES (?1, ?2) ON CONFLICT (nick) DO NOTHING`).bind(body.nick, now.toISOString()).run();
    }
    return true;
  }
  return false;
}
__name(hide, "hide");
var CLUB_LIST = 100;
var CLUB_LIST_DAYS = 60;
function cleanClubName(name) {
  if (typeof name !== "string") return null;
  const n = name.trim().replace(/\s+/g, " ");
  if (n.length < 3 || n.length > 32 || !/^[A-Za-z0-9 _.,'&!\-]+$/.test(n)) return null;
  return clean(n) ? n : null;
}
__name(cleanClubName, "cleanClubName");
function readNewClub(b) {
  if (!b || typeof b !== "object") return "not a club";
  if (b.protocol !== PROTOCOL2) return "update the game";
  if (!isPlayer(b.player)) return "bad player id";
  const name = cleanClubName(b.name);
  if (!name) return "bad club name";
  if (!isCompat(b.compat)) return "bad game record";
  return { name, owner: b.player, compat: b.compat, listed: b.listed !== false };
}
__name(readNewClub, "readNewClub");
async function clubs(db, now) {
  const since = new Date(now.getTime() - CLUB_LIST_DAYS * 864e5).toISOString();
  const { results } = await db.prepare(
    `SELECT id, name, version, compat, players, tick, at FROM clubs WHERE listed = 1 AND hidden = 0 AND at >= ?1
     ORDER BY players > 0 DESC, at DESC LIMIT ?2`
  ).bind(since, CLUB_LIST).all();
  return results.map((r) => ({ ...r, compat: JSON.parse(r.compat) }));
}
__name(clubs, "clubs");
var isClubId = /* @__PURE__ */ __name((s) => typeof s === "string" && /^[0-9a-f]{64}$/.test(s), "isClubId");
var json = /* @__PURE__ */ __name((v, status = 200, extra = {}) => new Response(JSON.stringify(v), {
  status,
  headers: { "Content-Type": "application/json", "Access-Control-Allow-Origin": "*", ...extra }
}), "json");
async function readJson(request) {
  const text = await request.text();
  if (text.length > MAX_BODY) return null;
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}
__name(readJson, "readJson");
function fromBase64Url(d) {
  if (typeof d !== "string" || d.length > MAX_BODY) return null;
  try {
    const bin = atob(d.replace(/-/g, "+").replace(/_/g, "/"));
    return JSON.parse(new TextDecoder().decode(Uint8Array.from(bin, (c) => c.charCodeAt(0))));
  } catch {
    return null;
  }
}
__name(fromBase64Url, "fromBase64Url");
var src_default = {
  async fetch(request, env) {
    const url = new URL(request.url);
    const now = /* @__PURE__ */ new Date();
    if (request.method === "OPTIONS") {
      return new Response(null, { headers: {
        "Access-Control-Allow-Origin": "*",
        "Access-Control-Allow-Methods": "GET, POST",
        "Access-Control-Allow-Headers": "Content-Type"
      } });
    }
    if (url.pathname === "/result" && (request.method === "POST" || request.method === "GET")) {
      const r = readResult(
        request.method === "POST" ? await readJson(request) : fromBase64Url(url.searchParams.get("d")),
        now,
        env.MIN_VERSION || ""
      );
      if (typeof r === "string") return json({ ok: false, error: r }, 400);
      const better = await storeResult(env.DB, r, now);
      return json({ ok: true, better }, 200, { "Cache-Control": "no-store" });
    }
    if (url.pathname === "/board" && request.method === "GET") {
      const c = url.searchParams.get("c") || weekId(now);
      const g = url.searchParams.get("g") || "";
      if (!isWeekId(c) || !isGroup(g)) return json({ ok: false, error: "bad board" }, 400);
      const b = await board(env.DB, c, g, url.searchParams.get("p") || "");
      return json({ ok: true, ...b }, 200, { "Cache-Control": url.searchParams.get("p") ? "no-store" : "public, max-age=60" });
    }
    if (url.pathname === "/challenge" && request.method === "GET") {
      return json({ ok: true, challenge: challenge(weekId(now)) }, 200, { "Cache-Control": "public, max-age=3600" });
    }
    if (url.pathname === "/clubs" && request.method === "GET") {
      if (!env.CLUB) return json({ ok: false, error: "no shared clubs here" }, 404);
      return json({ ok: true, protocol: PROTOCOL2, clubs: await clubs(env.DB, now) }, 200, { "Cache-Control": "no-store" });
    }
    if (url.pathname === "/clubs/new" && request.method === "GET") {
      if (!env.CLUB) return json({ ok: false, error: "no shared clubs here" }, 404);
      const c = readNewClub(fromBase64Url(url.searchParams.get("d")));
      if (typeof c === "string") return json({ ok: false, error: c }, 400);
      if (env.MIN_VERSION && newer(env.MIN_VERSION, c.compat.version)) return json({ ok: false, error: "update the game" }, 400);
      const id = env.CLUB.newUniqueId();
      const r = await env.CLUB.get(id).fetch("https://club/init", {
        method: "POST",
        body: JSON.stringify({ id: id.toString(), ...c })
      });
      if (!r.ok) return json({ ok: false, error: "the club could not be made" }, 500);
      return json({ ok: true, id: id.toString() }, 200, { "Cache-Control": "no-store" });
    }
    const m = url.pathname.match(/^\/clubs\/([0-9a-f]+)\/(ws|state)$/);
    if (m && request.method === "GET" && env.CLUB) {
      if (!isClubId(m[1])) return json({ ok: false, error: "no such club" }, 404);
      let id;
      try {
        id = env.CLUB.idFromString(m[1]);
      } catch {
        return json({ ok: false, error: "no such club" }, 404);
      }
      if (m[2] === "state" && !authorised(request, env)) return new Response("password needed", { status: 401 });
      return env.CLUB.get(id).fetch(new Request(`https://club/${m[2]}`, request));
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
  }
};

// src/worker.js
var worker_default = src_default;

// ../../../../../root/.npm/_npx/d46824d0cc92ff9a/node_modules/wrangler/templates/middleware/middleware-ensure-req-body-drained.ts
var drainBody = /* @__PURE__ */ __name(async (request, env, _ctx, middlewareCtx) => {
  try {
    return await middlewareCtx.next(request, env);
  } finally {
    try {
      if (request.body !== null && !request.bodyUsed) {
        const reader = request.body.getReader();
        while (!(await reader.read()).done) {
        }
      }
    } catch (e) {
      console.error("Failed to drain the unused request body.", e);
    }
  }
}, "drainBody");
var middleware_ensure_req_body_drained_default = drainBody;

// ../../../../../root/.npm/_npx/d46824d0cc92ff9a/node_modules/wrangler/templates/middleware/middleware-miniflare3-json-error.ts
function reduceError(e) {
  return {
    name: e?.name,
    message: e?.message ?? String(e),
    stack: e?.stack,
    cause: e?.cause === void 0 ? void 0 : reduceError(e.cause)
  };
}
__name(reduceError, "reduceError");
var jsonError = /* @__PURE__ */ __name(async (request, env, _ctx, middlewareCtx) => {
  try {
    return await middlewareCtx.next(request, env);
  } catch (e) {
    const error = reduceError(e);
    const body = JSON.stringify(error);
    const headers = {
      "Content-Type": "application/json",
      "MF-Experimental-Error-Stack": "true"
    };
    const encoded = encodeURIComponent(body);
    if (encoded.length <= 8192) {
      headers["MF-Experimental-Error-Stack-Payload"] = encoded;
    }
    return new Response(body, { status: 500, headers });
  }
}, "jsonError");
var middleware_miniflare3_json_error_default = jsonError;

// .wrangler/tmp/bundle-Kvf3BN/middleware-insertion-facade.js
var __INTERNAL_WRANGLER_MIDDLEWARE__ = [
  middleware_ensure_req_body_drained_default,
  middleware_miniflare3_json_error_default
];
var middleware_insertion_facade_default = worker_default;

// ../../../../../root/.npm/_npx/d46824d0cc92ff9a/node_modules/wrangler/templates/middleware/common.ts
var __facade_middleware__ = [];
function __facade_register__(...args) {
  __facade_middleware__.push(...args.flat());
}
__name(__facade_register__, "__facade_register__");
function __facade_invokeChain__(request, env, ctx, dispatch, middlewareChain) {
  const [head, ...tail] = middlewareChain;
  const middlewareCtx = {
    dispatch,
    next(newRequest, newEnv) {
      return __facade_invokeChain__(newRequest, newEnv, ctx, dispatch, tail);
    }
  };
  return head(request, env, ctx, middlewareCtx);
}
__name(__facade_invokeChain__, "__facade_invokeChain__");
function __facade_invoke__(request, env, ctx, dispatch, finalMiddleware) {
  return __facade_invokeChain__(request, env, ctx, dispatch, [
    ...__facade_middleware__,
    finalMiddleware
  ]);
}
__name(__facade_invoke__, "__facade_invoke__");

// .wrangler/tmp/bundle-Kvf3BN/middleware-loader.entry.ts
var __Facade_ScheduledController__ = class ___Facade_ScheduledController__ {
  constructor(scheduledTime, cron, noRetry) {
    this.scheduledTime = scheduledTime;
    this.cron = cron;
    this.#noRetry = noRetry;
  }
  scheduledTime;
  cron;
  static {
    __name(this, "__Facade_ScheduledController__");
  }
  #noRetry;
  noRetry() {
    if (!(this instanceof ___Facade_ScheduledController__)) {
      throw new TypeError("Illegal invocation");
    }
    this.#noRetry();
  }
};
function wrapExportedHandler(worker) {
  if (__INTERNAL_WRANGLER_MIDDLEWARE__ === void 0 || __INTERNAL_WRANGLER_MIDDLEWARE__.length === 0) {
    return worker;
  }
  for (const middleware of __INTERNAL_WRANGLER_MIDDLEWARE__) {
    __facade_register__(middleware);
  }
  const fetchDispatcher = /* @__PURE__ */ __name(function(request, env, ctx) {
    if (worker.fetch === void 0) {
      throw new Error("Handler does not export a fetch() function.");
    }
    return worker.fetch(request, env, ctx);
  }, "fetchDispatcher");
  return {
    ...worker,
    fetch(request, env, ctx) {
      const dispatcher = /* @__PURE__ */ __name(function(type, init) {
        if (type === "scheduled" && worker.scheduled !== void 0) {
          const controller = new __Facade_ScheduledController__(
            Date.now(),
            init.cron ?? "",
            () => {
            }
          );
          return worker.scheduled(controller, env, ctx);
        }
      }, "dispatcher");
      return __facade_invoke__(request, env, ctx, dispatcher, fetchDispatcher);
    }
  };
}
__name(wrapExportedHandler, "wrapExportedHandler");
function wrapWorkerEntrypoint(klass) {
  if (__INTERNAL_WRANGLER_MIDDLEWARE__ === void 0 || __INTERNAL_WRANGLER_MIDDLEWARE__.length === 0) {
    return klass;
  }
  for (const middleware of __INTERNAL_WRANGLER_MIDDLEWARE__) {
    __facade_register__(middleware);
  }
  return class extends klass {
    #fetchDispatcher = /* @__PURE__ */ __name((request, env, ctx) => {
      this.env = env;
      this.ctx = ctx;
      if (super.fetch === void 0) {
        throw new Error("Entrypoint class does not define a fetch() function.");
      }
      return super.fetch(request);
    }, "#fetchDispatcher");
    #dispatcher = /* @__PURE__ */ __name((type, init) => {
      if (type === "scheduled" && super.scheduled !== void 0) {
        const controller = new __Facade_ScheduledController__(
          Date.now(),
          init.cron ?? "",
          () => {
          }
        );
        return super.scheduled(controller);
      }
    }, "#dispatcher");
    fetch(request) {
      return __facade_invoke__(
        request,
        this.env,
        this.ctx,
        this.#dispatcher,
        this.#fetchDispatcher
      );
    }
  };
}
__name(wrapWorkerEntrypoint, "wrapWorkerEntrypoint");
var WRAPPED_ENTRY;
if (typeof middleware_insertion_facade_default === "object") {
  WRAPPED_ENTRY = wrapExportedHandler(middleware_insertion_facade_default);
} else if (typeof middleware_insertion_facade_default === "function") {
  WRAPPED_ENTRY = wrapWorkerEntrypoint(middleware_insertion_facade_default);
}
var middleware_loader_entry_default = WRAPPED_ENTRY;
export {
  Club,
  __INTERNAL_WRANGLER_MIDDLEWARE__,
  middleware_loader_entry_default as default
};
//# sourceMappingURL=worker.js.map
