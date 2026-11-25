// Multiplayer use, for the play-time stats page (stats/src/mp.js reads it): one row per day, mode, club/league/week and
// player, counting what that player did there that day. Only counts: the stats page never shows ids or nicknames.
// Copyright the OpenSGolf contributors. GPL-2.0-or-later.

/** The modes: a shared club joined, a Neighbours league made, joined or reported to, a weekly challenge result sent. */
export const MODES = new Set(["club", "league", "challenge"]);

/** Counts one thing a player did. Never throws: a count that fails must not fail what the player asked for. */
export async function noteActivity(db, mode, place, player, now = new Date()) {
  if (!db || !MODES.has(mode) || typeof place !== "string" || typeof player !== "string" || !place || !player) return;
  try {
    await db.prepare(
      `INSERT INTO activity (day, mode, place, player, n) VALUES (?1, ?2, ?3, ?4, 1)
       ON CONFLICT (day, mode, place, player) DO UPDATE SET n = n + 1`,
    ).bind(now.toISOString().slice(0, 10), mode, place.slice(0, 64), player.slice(0, 32)).run();
  } catch {
    // the table may not be there yet (schema.sql not applied): nothing to do
  }
}
