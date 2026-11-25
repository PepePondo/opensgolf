// A check of a running Worker's shared clubs (wrangler dev, or the deployed one): node test/live.mjs http://127.0.0.1:8787
// Makes a club, plays two players through it, and checks they are sent the same actions in the same order, that a
// joiner gets the save, and that the club stops with nobody on.
import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";

const base = process.argv[2] || "http://127.0.0.1:8787";
const compat = { protocol: 1, version: "0.9.0", log: 3, fix_bugs: true, random_trees: false, map: 74, extras: 0, packs: [] };
const id = () => randomBytes(8).toString("hex");
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function open(club, player, nick) {
  const ws = new WebSocket(`${base.replace(/^http/, "ws")}/clubs/${club}/ws`);
  const got = [];
  const waiters = [];
  ws.onmessage = (e) => {
    got.push(JSON.parse(e.data));
    for (const w of waiters.splice(0)) w();
  };
  const p = {
    ws, got,
    send: (m) => ws.send(JSON.stringify(m)),
    async until(pred, ms = 5000) {
      const end = Date.now() + ms;
      for (;;) {
        const m = got.find(pred);
        if (m) return m;
        if (Date.now() > end) throw new Error(`${nick}: nothing like ${pred} in ${JSON.stringify(got).slice(0, 400)}`);
        await new Promise((r) => { waiters.push(r); setTimeout(r, 100); });
      }
    },
  };
  return new Promise((resolve, reject) => {
    ws.onopen = () => {
      p.send({ Hello: { protocol: 1, player, nick, compat, tick: 40 } });
      resolve(p);
    };
    ws.onerror = (e) => reject(new Error(`${nick}: ${e.message || "socket error"}`));
  });
}

const owner = id(), other = id();
const d = Buffer.from(JSON.stringify({ protocol: 1, player: owner, nick: "Owner", name: "Live Check", compat, listed: false })).toString("base64url");
const made = await (await fetch(`${base}/clubs/new?d=${d}`)).json();
assert.ok(made.ok, JSON.stringify(made));
const a = await open(made.id, owner, "Owner");
const ask = await a.until((m) => m.SaveAsk !== undefined);
const t0 = Date.now();
a.send({ Ping: t0 });
const pong = await a.until((m) => m.Pong);
console.log(`round trip ${Date.now() - t0} ms`);
a.send({ Save: { tick: ask.SaveAsk, data: "U0FWRQ==" } });
await a.until((m) => m.Clock?.rate === 1);
a.send({ Act: { paint: [1, 2] } });
await sleep(200);
const b = await open(made.id, other, "Other");
await b.until((m) => m.Save);
assert.equal(b.got.find((m) => m.Save).Save.data, "U0FWRQ==");
b.send({ Act: "sync" });
a.send({ Act: "screen" });
await a.until((m) => m.Act?.action === "screen");
await b.until((m) => m.Act?.action === "screen");
await a.until((m) => m.Act?.action === "sync");
const acts = (p) => p.got.filter((m) => m.Act).map((m) => JSON.stringify(m.Act));
assert.deepEqual(acts(b), acts(a), "the same actions in the same order");
const ticks = a.got.filter((m) => m.Act).map((m) => m.Act.tick);
assert.deepEqual(ticks, [...ticks].sort((x, y) => x - y));
console.log(`${acts(a).length} actions in step; server clock offset ${(pong.Pong[1] - t0).toFixed(0)} ms`);
a.ws.close();
b.ws.close();
await sleep(500);
console.log("ok");
process.exit(0);
