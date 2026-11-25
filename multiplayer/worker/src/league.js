// Neighbours leagues (multiplayer/README.md "Neighbours"): 2 to 4 players each run their own course on their own plot of
// the same property, on their own computer, and the league's table compares them as they play. The game reports its
// numbers now and then and uploads its latest save, so neighbours can look round each other's course. Nothing is shared
// live: each plot is its own game, so nothing can fall out of step.
//
// Copyright the OpenSGolf contributors. GPL-2.0-or-later.

import { clean, cleanNick, generatedNick } from "./names.js";

/** Players in a league. */
export const LEAGUE_SEATS = 4;
/** What a league's table sorts by (challenge.js MEASURES). */
export const LEAGUE_MEASURES = ["cash", "fun", "members", "skill"];
/** A league's season ends at the end of this game year. */
export const LEAGUE_END_YEAR = 2002;
/** The largest save kept for a neighbour to look round. */
export const LEAGUE_SAVE_MAX = 512 * 1024;
/** Letters a league's code is made of (no 0/O, 1/I). */
const CODE_LETTERS = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

const isPlayer = (s) => typeof s === "string" && /^[0-9a-f]{16,32}$/.test(s);
const int = (v, lo, hi) => Number.isInteger(v) && v >= lo && v <= hi;

/** Whether `c` looks like a league's code. */
export function isLeagueCode(c) {
  return typeof c === "string" && new RegExp(`^[${CODE_LETTERS}]{6}$`).test(c);
}

/** A new code (random). */
export function newLeagueCode(random = (n) => crypto.getRandomValues(new Uint8Array(n))) {
  return Array.from(random(6), (b) => CODE_LETTERS[b % CODE_LETTERS.length]).join("");
}

/** A league's name, tidied, or null (as a club's). */
export function cleanLeagueName(name) {
  if (typeof name !== "string") return null;
  const n = name.trim().replace(/\s+/g, " ");
  if (n.length < 3 || n.length > 32 || !/^[A-Za-z0-9 _.,'&!\-]+$/.test(n)) return null;
  return clean(n) ? n : null;
}

/** A new league's body, checked: what to store, or a string saying why not. */
export function readNewLeague(b) {
  if (!b || typeof b !== "object") return "not a league";
  if (!isPlayer(b.player)) return "bad player id";
  const name = cleanLeagueName(b.name);
  if (!name) return "bad league name";
  if (!int(b.property, 0, 15)) return "bad property";
  if (!int(b.difficulty, 0, 3)) return "bad difficulty";
  if (!LEAGUE_MEASURES.includes(b.measure)) return "bad measure";
  const nick = cleanNick(b.nick) || generatedNick(b.player);
  return { name, property: b.property, difficulty: b.difficulty, measure: b.measure, player: b.player, nick };
}

/** A report's facts, checked (the weekly board's checks, plus the game date reached), or null. */
export function readFacts(f) {
  if (!f || typeof f !== "object") return null;
  if (!(int(f.year, 2001, 2100) && int(f.month, 0, 11) && int(f.cash, -1e9, 1e10) && int(f.holes, 0, 36)
    && int(f.fun, -1e6, 1e6) && int(f.skill, -1e6, 1e6) && int(f.members, 0, 1e5) && int(f.rank, 0, 3))) return null;
  const { year, month, cash, holes, fun, skill, members, rank } = f;
  return { year, month, cash, holes, fun, skill, members, rank };
}

/** Makes a league; its maker takes the first seat. */
export async function makeLeague(db, l, now, code = newLeagueCode(), seed = (crypto.getRandomValues(new Uint32Array(1))[0] >>> 8)) {
  const at = now.toISOString();
  await db.batch([
    db.prepare(`INSERT INTO leagues (code, name, property, difficulty, measure, seed, owner, made) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)`)
      .bind(code, l.name, l.property, l.difficulty, l.measure, seed, l.player, at),
    db.prepare(`INSERT INTO league_players (code, seat, player, nick, at) VALUES (?1, 0, ?2, ?3, ?4)`).bind(code, l.player, l.nick, at),
  ]);
  return code;
}

/** A league and its players (`player`'s row marked), or null. */
export async function league(db, code, player) {
  const l = await db.prepare(`SELECT code, name, property, difficulty, measure, seed, made FROM leagues WHERE code = ?1`).bind(code).first();
  if (!l) return null;
  const { results } = await db.prepare(
    `SELECT seat, player, nick, facts, at, save_at FROM league_players WHERE code = ?1 ORDER BY seat`,
  ).bind(code).all();
  const players = results.map((r) => ({
    seat: r.seat, nick: r.nick, facts: r.facts ? JSON.parse(r.facts) : null, at: r.at, save: !!r.save_at, mine: r.player === player,
  }));
  return { league: { ...l, endYear: LEAGUE_END_YEAR }, players };
}

/** Takes a seat (the player's own if they have one): the seat, or a string saying why not. */
export async function joinLeague(db, code, player, nick, now) {
  if (!isPlayer(player)) return "bad player id";
  const l = await db.prepare(`SELECT code FROM leagues WHERE code = ?1`).bind(code).first();
  if (!l) return "no league has that code";
  const { results } = await db.prepare(`SELECT seat, player FROM league_players WHERE code = ?1`).bind(code).all();
  const mine = results.find((r) => r.player === player);
  if (mine) return mine.seat;
  if (results.length >= LEAGUE_SEATS) return "the league is full";
  const taken = new Set(results.map((r) => r.seat));
  let seat = 0;
  while (taken.has(seat)) seat++;
  const n = cleanNick(nick) || generatedNick(player);
  await db.prepare(`INSERT INTO league_players (code, seat, player, nick, at) VALUES (?1, ?2, ?3, ?4, ?5)`)
    .bind(code, seat, player, n, now.toISOString()).run();
  return seat;
}

/** A player's latest numbers. False when they have no seat. */
export async function report(db, code, player, facts, now) {
  const r = await db.prepare(`UPDATE league_players SET facts = ?3, at = ?4 WHERE code = ?1 AND player = ?2`)
    .bind(code, player, JSON.stringify(facts), now.toISOString()).run();
  return (r.meta && r.meta.changes) > 0;
}

/** A player's latest save (bytes), for neighbours to look round. False when they have no seat. */
export async function storeSave(db, code, player, bytes, now) {
  const r = await db.prepare(`UPDATE league_players SET save = ?3, save_at = ?4 WHERE code = ?1 AND player = ?2`)
    .bind(code, player, bytes, now.toISOString()).run();
  return (r.meta && r.meta.changes) > 0;
}

/** A seat's latest save, or null. */
export async function seatSave(db, code, seat) {
  const r = await db.prepare(`SELECT save FROM league_players WHERE code = ?1 AND seat = ?2 AND save IS NOT NULL`).bind(code, seat).first();
  return r ? new Uint8Array(r.save) : null;
}
