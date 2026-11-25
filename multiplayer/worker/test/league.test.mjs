import test from "node:test";
import assert from "node:assert/strict";
import { memoryDb } from "./d1shim.mjs";
import worker from "../src/index.js";
import { isLeagueCode, newLeagueCode, readFacts, LEAGUE_SEATS } from "../src/league.js";

const P = (i) => (0x1000 + i).toString(16).padStart(16, "0");
const d = (o) => Buffer.from(JSON.stringify(o)).toString("base64url");
const get = async (env, path) => {
  const r = await worker.fetch(new Request(`https://w${path}`), env);
  return { status: r.status, body: await r.json() };
};
const facts = (cash) => ({ year: 2001, month: 6, cash, holes: 4, fun: 300, skill: 120, members: 10, rank: 0 });

test("league codes", () => {
  assert.ok(isLeagueCode(newLeagueCode()));
  assert.ok(!isLeagueCode("ABC0EF") && !isLeagueCode("abcdef") && !isLeagueCode("ABCDE"));
  assert.equal(readFacts({ ...facts(1), month: 12 }), null);
});

test("a league is made, joined, reported to and looked round", async () => {
  const env = { DB: memoryDb() };
  const made = await get(env, `/leagues/new?d=${d({ protocol: 1, player: P(0), nick: "Mike", name: "Valley Four", property: 3, difficulty: 1, measure: "cash" })}`);
  assert.equal(made.status, 200, JSON.stringify(made.body));
  const code = made.body.league.code;
  assert.ok(isLeagueCode(code));
  assert.equal(made.body.players.length, 1);
  assert.ok(made.body.players[0].mine);
  for (let i = 1; i < LEAGUE_SEATS; i++) {
    const j = await get(env, `/leagues/${code}/join?d=${d({ protocol: 1, player: P(i), nick: `P${i}` })}`);
    assert.equal(j.body.seat, i);
  }
  // joining again keeps the seat; a fifth player is refused
  assert.equal((await get(env, `/leagues/${code}/join?d=${d({ protocol: 1, player: P(2), nick: "P2" })}`)).body.seat, 2);
  assert.equal((await get(env, `/leagues/${code}/join?d=${d({ protocol: 1, player: P(9), nick: "P9" })}`)).body.error, "the league is full");
  assert.equal((await get(env, `/leagues/${code}/report?d=${d({ player: P(1), facts: facts(5000), cheats: false })}`)).body.ok, true);
  assert.equal((await get(env, `/leagues/${code}/report?d=${d({ player: P(9), facts: facts(5000), cheats: false })}`)).body.ok, false);
  assert.equal((await get(env, `/leagues/${code}/report?d=${d({ player: P(1), facts: facts(5000), cheats: true })}`)).status, 400);
  const t = await get(env, `/leagues/${code}?p=${P(1)}`);
  assert.deepEqual(t.body.players[1].facts, facts(5000));
  assert.ok(t.body.players[1].mine && !t.body.players[0].mine);
  // a save, and a neighbour fetching it
  const bytes = new Uint8Array([1, 2, 3, 250]);
  const up = await worker.fetch(new Request(`https://w/leagues/${code}/save?p=${P(1)}`, { method: "POST", body: bytes }), env);
  assert.equal((await up.json()).ok, true);
  const down = await worker.fetch(new Request(`https://w/leagues/${code}/save/1`), env);
  assert.deepEqual(new Uint8Array(await down.arrayBuffer()), bytes);
  assert.equal((await worker.fetch(new Request(`https://w/leagues/${code}/save/0`), env)).status, 404);
  assert.equal((await get(env, `/leagues/${code}?p=${P(0)}`)).body.players[1].save, true);
  assert.equal((await get(env, "/leagues/ZZZZZZ")).status, 404);
  // counted for the stats page: made + 3 joins + a join again + one good report; refused ones not at all
  const act = env.DB.raw.prepare(`SELECT player, n FROM activity WHERE mode = 'league' AND place = ? ORDER BY player`).all(code);
  assert.deepEqual(act.map((r) => [r.player, r.n]), [[P(0), 1], [P(1), 2], [P(2), 2], [P(3), 1]]);
});
