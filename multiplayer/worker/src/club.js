// A shared club: one Durable Object per club, holding the players' WebSockets (hibernating: an idle club costs nothing),
// the club's clock and the order of actions (room.js), the latest save and every action since it.
//
// The server never runs the game. Every player's game plays the same ticks with the same actions in the same order
// (crates/simgolf/src/mp/lockstep.rs), so any player's save is everyone's game. A joiner is sent the latest save and the
// actions since it and plays up to the clock; the room asks the longest-on player for a new save now and then, so a
// joiner has little to catch up. Nobody on: the clock stops (the club is paused), and goes on when someone joins.
//
// Messages: crates/simgolf/src/mp/session.rs ToServer and ToGame, as serde JSON.
//
// Copyright the OpenSGolf contributors. GPL-2.0-or-later.

import { RoomCore, pausedAt, tickAt, withRate, LEAD_MS, TICK_MS } from "./room.js";

/** The network protocol (mp/version.rs PROTOCOL). */
export const PROTOCOL = 1;
/** Players on at once. */
export const MAX_PLAYERS = 8;
/** The largest message from a game, in characters (a save is the big one). */
export const MAX_MESSAGE = 4 * 1024 * 1024;
/** The largest action, in characters. */
export const MAX_ACTION = 4096;
/** Ticks between saves asked for (about 3 minutes at normal speed). */
export const SAVE_EVERY = 2000;
/** Hashes kept per club, for the out-of-step check. */
const CHECKS_KEPT = 32;

/** The parts of a game's Compat that differ from the club's (mp/version.rs Compat::check), by name. */
export function mismatches(ours, club) {
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

/** Whether a Compat record is well formed (what a new club is made with). */
export function isCompat(c) {
  return !!c && typeof c === "object" && c.protocol === PROTOCOL && typeof c.version === "string" && c.version.length <= 20
    && Number.isInteger(c.log) && typeof c.fix_bugs === "boolean" && typeof c.random_trees === "boolean"
    && Number.isInteger(c.map) && Number.isInteger(c.extras) && Array.isArray(c.packs) && c.packs.length <= 64
    && c.packs.every((p) => typeof p === "string" && p.length <= 80);
}

const isPlayerId = (s) => typeof s === "string" && /^[0-9a-f]{16,32}$/.test(s);

export class Club {
  constructor(ctx, env) {
    this.ctx = ctx;
    this.env = env;
    this.sql = ctx.storage.sql;
    this.sql.exec(`CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v TEXT NOT NULL)`);
    this.sql.exec(`CREATE TABLE IF NOT EXISTS log (seq INTEGER PRIMARY KEY, tick INTEGER NOT NULL, msg TEXT NOT NULL)`);
    this.sql.exec(`CREATE TABLE IF NOT EXISTS save (id INTEGER PRIMARY KEY, tick INTEGER NOT NULL, data TEXT NOT NULL, extra TEXT NOT NULL DEFAULT '')`);
    this.sql.exec(`CREATE TABLE IF NOT EXISTS seats (player TEXT PRIMARY KEY, me INTEGER NOT NULL)`);
    this.checks = new Map(); // tick -> the first hash sent (kept in memory: a lost one only skips a check)
    this.asked = null; // {tick, me}: a save asked for
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
    this.info = this.get("info"); // {id, name, compat, owner, listed, made}
    const room = this.get("room") || { clock: pausedAt(0, 0), seq: 0, want: 1 };
    this.room = new RoomCore(room.clock, room.seq);
    this.want = room.want; // the rate to go on at when someone joins
  }

  saveRoom() {
    this.put("room", { clock: this.room.clock, seq: this.room.nextSeq, want: this.want });
  }

  saved() {
    return this.sql.exec(`SELECT tick, data, extra FROM save WHERE id = 1`).toArray()[0] || null;
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
    return this.ctx.getWebSockets()
      .filter((ws) => ws !== except && ws.deserializeAttachment()?.me)
      .sort((a, b) => a.deserializeAttachment().since - b.deserializeAttachment().since);
  }

  send(ws, m) {
    try {
      ws.send(JSON.stringify(m));
    } catch {
      // the socket closed; its close event tidies up
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
       ON CONFLICT (id) DO UPDATE SET players = ?6, tick = ?7, at = ?9`,
    ).bind(this.info.id, this.info.name, this.info.compat.version, JSON.stringify(this.info.compat), this.info.listed ? 1 : 0,
      on, tick, this.info.made, new Date().toISOString()).run();
  }

  // ------------------------------------------------------------ requests

  async fetch(request) {
    const url = new URL(request.url);
    if (url.pathname === "/init" && request.method === "POST") {
      if (this.info) return new Response("made already", { status: 409 });
      const b = await request.json();
      this.info = { id: b.id, name: b.name, compat: b.compat, owner: b.owner, listed: !!b.listed, made: new Date().toISOString() };
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
    if (m.Ping !== undefined) return this.send(ws, { Pong: [Number(m.Ping) || 0, now] });
    if (m.Hello) return this.hello(ws, m.Hello, now);
    if (!a?.me) return this.refuse(ws, "say hello first");
    if (m.Act !== undefined) {
      if (JSON.stringify(m.Act).length > MAX_ACTION) return;
      if (m.Act === "undo" || (m.Act && typeof m.Act === "object" && "cheat" in m.Act)) return; // session.rs route: Refused
      if (m.Act && typeof m.Act === "object" && "speed" in m.Act) this.want = m.Act.speed;
      this.publish(this.room.handle(now, a.me, { Act: m.Act }));
    } else if (m.Speed !== undefined) {
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
      // closed already
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
      // a new club: its clock starts where the owner's game is, and the owner's game is the first save
      this.room.clock = pausedAt(Math.max(0, h.tick | 0), now);
      this.saveRoom();
      this.send(ws, { Welcome: { me, ms: now, clock: this.room.clock } });
      this.asked = { tick: this.room.clock.base_tick, me };
      this.send(ws, { SaveAsk: this.room.clock.base_tick });
    } else {
      this.send(ws, { Welcome: { me, ms: now, clock: this.room.clock } });
      this.send(ws, { Save: { tick: save.tick, data: save.data, extra: save.extra } });
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
    if (s.tick > tickAt(this.room.clock, now)) return; // a save from ahead of the clock can't be right
    if (this.asked && this.asked.me !== a.me) return; // only the player asked
    const extra = typeof s.extra === "string" ? s.extra : "";
    this.sql.exec(`INSERT INTO save (id, tick, data, extra) VALUES (1, ?, ?, ?) ON CONFLICT (id) DO UPDATE SET
      tick = excluded.tick, data = excluded.data, extra = excluded.extra`, s.tick, s.data, extra);
    this.sql.exec(`DELETE FROM log WHERE tick < ?`, s.tick);
    this.asked = null;
    if (have === null && this.want > 0) this.publish(this.room.setRate(now, a.me, this.want)); // the new club starts
    await this.list();
  }

  /** Asks the longest-on player for a save when the latest is old (or its asked player left). */
  maybeAsk(now) {
    const list = this.players();
    if (!list.length) return;
    if (this.asked && list.some((ws) => ws.deserializeAttachment().me === this.asked.me)) return;
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
    if (first === undefined) {
      this.checks.set(c.tick, c.hash);
      if (this.checks.size > CHECKS_KEPT) this.checks.delete(this.checks.keys().next().value);
    } else if (first !== c.hash) {
      this.send(ws, { Reload: c.tick });
    }
  }

  async webSocketClose(ws, code) {
    try {
      ws.close(code === 1005 || code === 1006 ? 1000 : code, "bye");
    } catch {
      // closed already
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
      // nobody on: the club stops until someone comes back
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
}

export { TICK_MS };
