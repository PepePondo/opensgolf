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

/** The challenge of a week id. */
export function challenge(id) {
  const h = fnv1a(`opensgolf-challenge/${id}`);
  return {
    id,
    measure: MEASURES[h % 4],
    theme: THEMES[(h >>> 2) % 4],
    difficulty: DIFFICULTIES[(h >>> 4) % 3],
    // the new game's seed (the land, the golfers' arrivals)
    seed: h >>> 8,
    endYear: END_YEAR,
  };
}

/** The week's score of a result's facts: the measure's own number. */
export function score(c, facts) {
  return facts[c.measure];
}
