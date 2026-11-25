// The weekly rival-courses challenge (multiplayer/README.md "Rival courses"). Every player gets the same start each ISO
// week, worked out from the week's id alone, so neither the game nor the Worker needs to fetch or store it. The game has
// the same function in Rust (crates/simgolf/src/mp/challenge.rs); test/challenge.test.mjs and that file's tests share
// the vectors in test/vectors.json, so the two can never drift apart.
//
// Copyright the OpenSGolf contributors. GPL-2.0-or-later.

/** What the board sorts by, in the order the seed picks them. */
export const MEASURES = ["cash", "fun", "members", "skill"];
/** Course themes, as scenario.toml names them. */
export const THEMES = ["parkland", "desert", "tropical", "links"];
/** Difficulties a challenge uses (never Impossible: a week is short). */
export const DIFFICULTIES = ["easy", "moderate", "difficult"];
/** A challenge ends at the end of this game year (2001 is the first). */
export const END_YEAR = 2001;

/** FNV-1a, 32 bits, over the string's UTF-8 bytes. */
export function fnv1a(s) {
  let h = 0x811c9dc5;
  for (const b of new TextEncoder().encode(s)) {
    h ^= b;
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h >>> 0;
}

/** The ISO week id ('YYYY-Www') of a Date, in UTC. */
export function weekId(d) {
  const t = new Date(Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate()));
  const day = t.getUTCDay() || 7; // Monday 1 .. Sunday 7
  t.setUTCDate(t.getUTCDate() + 4 - day); // the week's Thursday decides its year
  const year = t.getUTCFullYear();
  const jan1 = Date.UTC(year, 0, 1);
  const week = Math.floor((t - jan1) / 86400000 / 7) + 1;
  return `${year}-W${String(week).padStart(2, "0")}`;
}

/** Whether `id` looks like a week id. */
export function isWeekId(id) {
  return typeof id === "string" && /^\d{4}-W(0[1-9]|[1-4]\d|5[0-3])$/.test(id);
}

/** The week before `d`'s week, as an id. */
export function previousWeekId(d) {
  return weekId(new Date(d.getTime() - 7 * 86400000));
}

/** Kinds of week: the usual start, half the usual money, or a course of at most nine holes. */
export const KINDS = ["standard", "budget", "standard", "nine", "standard"];
/** The most holes a "nine" week's course may have (more keeps the result off the board). */
export const NINE_HOLES = 9;
/** The first week of the rotation: the weeks before it were drawn from the hash alone. */
export const ROTATION_FROM = "2026-W49";

/** Days since 1970-01-01 of a week id's Monday. */
export function mondayOf(id) {
  const year = Number(id.slice(0, 4));
  const week = Number(id.slice(6));
  const jan4 = Date.UTC(year, 0, 4) / 86400000;
  const day = new Date(jan4 * 86400000).getUTCDay() || 7;
  return jan4 - (day - 1) + 7 * (week - 1);
}

/**
 * The challenge of a week id. From ROTATION_FROM the weeks take turns: the measure every 4 weeks, the difficulty every 3
 * and the land type shifting so that two weeks in a row never share it, so 48 weeks play every measure, land type and
 * difficulty together once; the kind of week comes round every 5. The land itself (the seed) still comes from the hash.
 */
export function challenge(id) {
  const h = fnv1a(`opensgolf-challenge/${id}`);
  const n = (mondayOf(id) - mondayOf(ROTATION_FROM)) / 7;
  const turn = n >= 0;
  return {
    id,
    measure: turn ? MEASURES[n % 4] : MEASURES[h % 4],
    theme: turn ? THEMES[(n + Math.floor(n / 12) + 1) % 4] : THEMES[(h >>> 2) % 4],
    difficulty: turn ? DIFFICULTIES[n % 3] : DIFFICULTIES[(h >>> 4) % 3],
    kind: turn ? KINDS[n % 5] : "standard",
    // the new game's seed (the land, the golfers' arrivals)
    seed: h >>> 8,
    endYear: END_YEAR,
  };
}

/** Why a result's facts break the week's own rule (null: they don't). */
export function breaks(c, facts) {
  return c.kind === "nine" && facts.holes > NINE_HOLES ? `more than ${NINE_HOLES} holes` : null;
}

/** The week's score of a result's facts: the measure's own number. */
export function score(c, facts) {
  return facts[c.measure];
}
