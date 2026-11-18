import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { RoomCore, pausedAt } from "../src/room.js";
import { Club, mismatches, SAVE_EVERY, MAX_PLAYERS } from "../src/club.js";
import worker, { readNewClub, cleanClubName } from "../src/index.js";
import { fakeCtx, fakeNamespace, FakeSocket, memorySql } from "./doshim.mjs";
import { memoryDb } from "./d1shim.mjs";

const vectors = JSON.parse(readFileSync(new URL("./room_vectors.json", import.meta.url), "utf8"));

test("the room matches the game's (shared vectors)", () => {
  const room = new RoomCore(pausedAt(vectors.start.tick, vectors.start.ms));
  for (const s of vectors.steps) assert.deepEqual(room.handle(s.ms, s.player, s.in), s.out, JSON.stringify(s.in));
});

// a game's record (mp/version.rs Compat)
const compat = (over = {}) => ({
  protocol: 1, version: "0.9.0", log: 3, fix_bugs: true, random_trees: false, map: 74, extras: 0, packs: [], ...over,
});
const OWNER = "00000000000000aa", P2 = "00000000000000bb";
const hello = (player, over = {}) => JSON.stringify({ Hello: { protocol: 1, player, nick: player.slice(-2), compat: compat(), tick: 40, ...over } });

let clock = 1_000_000;
const realNow = Date.now;
Date.now = () => clock;

function club(env = {}) {
  const ctx = fakeCtx();
  const c = new Club(ctx, env);
  return { ctx, c };
}

async function made(env = {}) {
  const k = club(env);
  await k.c.fetch(new Request("https://club/init", { method: "POST", body: JSON.stringify({ id: "x".repeat(64), name: "Pine", compat: compat(), owner: OWNER, listed: true }) }));
  return k;
}

async function join(k, player, over) {
  const ws = new FakeSocket();
  k.ctx.acceptWebSocket(ws);
  await k.c.webSocketMessage(ws, hello(player, over));
  return ws;
}

test("a new club starts from its owner's game", async () => {
  clock = 1_000_000;
  const k = await made();
  const early = await join(k, P2);
  assert.equal(early.take()[0].Refused.reason, "the club is starting: try again in a minute");
  const o = await join(k, OWNER);
  const got = o.take();
  assert.deepEqual(got[0].Welcome.clock, pausedAt(40, clock));
  assert.deepEqual(got[1], { SaveAsk: 40 });
  await k.c.webSocketMessage(o, JSON.stringify({ Save: { tick: 40, data: "AAAA" } }));
  const go = o.take();
  assert.equal(go[0].Clock.rate, 1, "the club runs once it has its save");
  assert.deepEqual(go[1].Act.action, { speed: 1 });
});

test("a joiner gets the save and every action since it, in order", async () => {
  clock = 1_000_000;
  const k = await made();
  const o = await join(k, OWNER);
  await k.c.webSocketMessage(o, JSON.stringify({ Save: { tick: 40, data: "SAVE" } }));
  clock += 870;
  await k.c.webSocketMessage(o, JSON.stringify({ Act: { paint: [1, 2] } }));
  await k.c.webSocketMessage(o, JSON.stringify({ Act: "undo" })); // refused
  await k.c.webSocketMessage(o, JSON.stringify({ Act: { cheat: 3 } })); // refused
  o.take();
  const j = await join(k, P2);
  const got = j.take();
  assert.equal(got[0].Welcome.me, 2);
  assert.deepEqual(got[1], { Save: { tick: 40, data: "SAVE" } });
  assert.deepEqual(got.slice(2, 4).map((m) => m.Act.action), [{ speed: 1 }, { paint: [1, 2] }]);
  assert.deepEqual(got[4], { Players: [{ me: 1, nick: "aa" }, { me: 2, nick: "bb" }] });
  assert.equal(got.length, 5);
  // both get the next action, stamped ahead
  clock += 87;
  await k.c.webSocketMessage(j, JSON.stringify({ Act: "sync" }));
  const a = o.take().at(-1), b = j.take().at(-1);
  assert.deepEqual(a, b);
  assert.equal(a.Act.player, 2);
  assert.equal(a.Act.tick, 40 + 11 + 8);
});

test("a game that can't stay in step is refused, saying why", async () => {
  const k = await made();
  await join(k, OWNER);
  const ws = await join(k, P2, { compat: compat({ version: "0.8.9", random_trees: true }) });
  assert.deepEqual(ws.take()[0].Refused.parts, ["version", "rules"]);
  assert.ok(ws.closed);
  assert.deepEqual(mismatches(compat({ packs: ["a@1.0.0"] }), compat()), ["packs"]);
  const old = await join(k, P2, { protocol: 2 });
  assert.equal(old.take()[0].Refused.reason, "update the game");
});

test("nobody on: the club stops, and goes on when someone joins", async () => {
  clock = 2_000_000;
  const k = await made();
  const o = await join(k, OWNER);
  await k.c.webSocketMessage(o, JSON.stringify({ Save: { tick: 40, data: "S" } }));
  await k.c.webSocketMessage(o, JSON.stringify({ Speed: 4 }));
  clock += 870; // 10 ticks at 4x = 40
  o.close();
  await k.c.webSocketClose(o);
  const stopped = k.c.room.clock;
  assert.equal(stopped.rate, 0);
  assert.equal(stopped.base_tick, 80);
  clock += 3_600_000;
  // the object is made again (hibernation): the clock comes back from storage
  const again = new Club(k.ctx, {});
  assert.deepEqual(again.room.clock, stopped);
  const k2 = { ctx: k.ctx, c: again };
  const j = await join(k2, P2);
  const got = j.take();
  assert.equal(got[0].Welcome.clock.base_tick, 80);
  const resumed = got.find((m) => m.Clock);
  assert.equal(resumed.Clock.rate, 4, "at the speed it had");
});

test("joining twice drops the first socket", async () => {
  const k = await made();
  const a = await join(k, OWNER);
  const b = await join(k, OWNER);
  assert.ok(a.closed);
  assert.equal(b.take()[0].Welcome.me, 1);
  assert.equal(k.c.players().length, 1);
});

test("the club is full at 8", async () => {
  const k = await made();
  const o = await join(k, OWNER);
  await k.c.webSocketMessage(o, JSON.stringify({ Save: { tick: 40, data: "S" } }));
  for (let i = 1; i < MAX_PLAYERS; i++) await join(k, (0x100 + i).toString(16).padStart(16, "0"));
  const ws = await join(k, "ffffffffffffffff");
  assert.equal(ws.take()[0].Refused.reason, "the club is full");
});

test("saves are asked for now and then, and keep the log short", async () => {
  clock = 3_000_000;
  const k = await made();
  const o = await join(k, OWNER);
  await k.c.webSocketMessage(o, JSON.stringify({ Save: { tick: 40, data: "S" } }));
  const p = await join(k, P2);
  clock += 87 * 10;
  await k.c.webSocketMessage(p, JSON.stringify({ Act: "sync" }));
  o.take();
  clock += 87 * SAVE_EVERY;
  await k.c.webSocketMessage(p, JSON.stringify({ Act: "sync" }));
  const ask = o.take().find((m) => m.SaveAsk !== undefined);
  assert.ok(ask, "the longest-on player is asked");
  assert.equal(p.take().find((m) => m.SaveAsk !== undefined), undefined);
  // a save from someone not asked is ignored; from ahead of the clock too
  await k.c.webSocketMessage(p, JSON.stringify({ Save: { tick: ask.SaveAsk, data: "P" } }));
  await k.c.webSocketMessage(o, JSON.stringify({ Save: { tick: ask.SaveAsk + 1000, data: "F" } }));
  assert.equal(k.c.saved().data, "S");
  clock += 87 * 10;
  await k.c.webSocketMessage(o, JSON.stringify({ Save: { tick: ask.SaveAsk, data: "NEW" } }));
  assert.equal(k.c.saved().tick, ask.SaveAsk);
  const left = k.c.sql.exec(`SELECT tick FROM log`).toArray();
  assert.ok(left.every((r) => r.tick >= ask.SaveAsk), "the log keeps only what came after the save");
  // the asked player leaving hands the ask on
  clock += 87 * SAVE_EVERY * 2;
  await k.c.webSocketMessage(p, JSON.stringify({ Act: "sync" }));
  p.take();
  o.close();
  await k.c.webSocketClose(o);
  assert.ok(p.take().some((m) => m.SaveAsk !== undefined));
});

test("a game out of step is told to load again", async () => {
  const k = await made();
  const o = await join(k, OWNER);
  await k.c.webSocketMessage(o, JSON.stringify({ Save: { tick: 40, data: "S" } }));
  const p = await join(k, P2);
  o.take(); p.take();
  await k.c.webSocketMessage(o, JSON.stringify({ Check: { tick: 100, hash: "abc" } }));
  await k.c.webSocketMessage(p, JSON.stringify({ Check: { tick: 100, hash: "abd" } }));
  assert.deepEqual(p.take(), [{ Reload: 100 }]);
  assert.deepEqual(o.take(), []);
});

test("a ping is answered with the server's time", async () => {
  const k = await made();
  const ws = new FakeSocket();
  await k.c.webSocketMessage(ws, JSON.stringify({ Ping: 12.5 }));
  assert.deepEqual(ws.take(), [{ Pong: [12.5, clock] }]);
  await k.c.webSocketMessage(ws, JSON.stringify({ Act: "sync" }));
  assert.equal(ws.take()[0].Refused.reason, "say hello first");
});

test("clubs are made through the Worker and listed", async () => {
  clock = realNow();
  const env = { DB: memoryDb() };
  env.CLUB = fakeNamespace(Club, env);
  const d = Buffer.from(JSON.stringify({ protocol: 1, player: OWNER, nick: "M", name: "Pine Valley", compat: compat() })).toString("base64url");
  const r = await worker.fetch(new Request(`https://w/clubs/new?d=${d}`), env);
  const { id } = await r.json();
  assert.match(id, /^[0-9a-f]{64}$/);
  const list = await (await worker.fetch(new Request("https://w/clubs"), env)).json();
  assert.equal(list.clubs.length, 1);
  assert.equal(list.clubs[0].name, "Pine Valley");
  assert.deepEqual(list.clubs[0].compat, compat());
  assert.equal(list.clubs[0].players, 0);
  // someone on: the list says so
  const o = env.CLUB.objects.get(id);
  const ws = new FakeSocket();
  o.ctx.acceptWebSocket(ws);
  await o.obj.webSocketMessage(ws, hello(OWNER));
  const list2 = await (await worker.fetch(new Request("https://w/clubs"), env)).json();
  assert.equal(list2.clubs[0].players, 1);
  assert.equal((await worker.fetch(new Request(`https://w/clubs/${"0".repeat(64)}/state`), env)).status, 401);
  assert.equal((await worker.fetch(new Request(`https://w/clubs/nope/ws`), env)).status, 404);
  assert.equal(cleanClubName("  The   Old Course "), "The Old Course");
  assert.equal(cleanClubName("ab"), null);
  assert.equal(cleanClubName("Sh1t Hills"), null);
  assert.equal(readNewClub({ protocol: 1, player: OWNER, name: "Fine", compat: { ...compat(), protocol: 9 } }), "bad game record");
});
