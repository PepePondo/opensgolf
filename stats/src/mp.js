// The stats page's Multiplayer section: counts from the multiplayer server's database (multiplayer/worker, bound here as
// BOARD, read only). Only counts leave this file: never a player id, nickname, club name or league code.
// Copyright the OpenSGolf contributors. GPL-2.0-or-later.

export const MODES = [["club", "Shared clubs"], ["league", "Neighbours leagues"], ["challenge", "Weekly challenge"]];

/** Days of daily history shown, and weeks of weekly. */
export const MP_DAYS = 30, MP_WEEKS = 8;

/** A club counts as having players on when its row changed this recently (a running club saves every few minutes). */
export const ON_NOW_MS = 60 * 60e3;

const dayAgo = (now, n) => new Date(now.getTime() - n * 86400e3).toISOString().slice(0, 10);

/** The Multiplayer numbers, or null when this Worker has no BOARD database. */
export async function mpStats(board, now = new Date()) {
  if (!board) return null;
  const all = async (sql, ...p) => (await board.prepare(sql).bind(...p).all()).results || [];
  const one = async (sql, ...p) => (await board.prepare(sql).bind(...p).first()) || {};
  // activity without the clubs an admin hid (the pre-launch test clubs)
  const act = `(SELECT * FROM activity WHERE NOT (mode = 'club' AND place IN (SELECT id FROM clubs WHERE hidden = 1)))`;
  const today = dayAgo(now, 0), d7 = dayAgo(now, 6), d30 = dayAgo(now, 29), from = dayAgo(now, MP_DAYS - 1);

  const players = async (since) => (await one(`SELECT COUNT(DISTINCT player) AS n FROM ${act} WHERE day >= ?1`, since)).n || 0;
  const byMode = await all(`
    SELECT mode, COUNT(DISTINCT player) AS players, COUNT(DISTINCT place) AS places, SUM(n) AS events
    FROM ${act} WHERE day >= ?1 GROUP BY mode`, d7);
  const dailyModes = await all(`SELECT day, mode, COUNT(DISTINCT player) AS players FROM ${act} WHERE day >= ?1 GROUP BY day, mode`, from);
  const dailyAll = await all(`SELECT day, COUNT(DISTINCT player) AS players FROM ${act} WHERE day >= ?1 GROUP BY day`, from);
  const weekly = await all(`
    SELECT date(day, '-6 days', 'weekday 1') AS week, COUNT(DISTINCT player) AS players FROM ${act}
    GROUP BY week ORDER BY week DESC LIMIT ?1`, MP_WEEKS);
  const clubs = await one(`
    SELECT COUNT(*) AS total, COALESCE(SUM(listed), 0) AS public,
      COALESCE(SUM(CASE WHEN players > 0 AND at >= ?1 THEN 1 ELSE 0 END), 0) AS busy,
      COALESCE(SUM(CASE WHEN players > 0 AND at >= ?1 THEN players ELSE 0 END), 0) AS on_now,
      COALESCE(SUM(CASE WHEN made >= ?2 THEN 1 ELSE 0 END), 0) AS new_7d
    FROM clubs WHERE hidden = 0`, new Date(now.getTime() - ON_NOW_MS).toISOString(), d7);
  const leagues = await one(`
    SELECT (SELECT COUNT(*) FROM leagues) AS total, (SELECT COUNT(*) FROM league_players) AS seats,
      (SELECT COUNT(*) FROM leagues WHERE made >= ?1) AS new_7d`, d7);
  const weeks = await all(`
    SELECT challenge AS week, COUNT(*) AS players, SUM(CASE WHEN grp <> '' THEN 1 ELSE 0 END) AS in_groups
    FROM results WHERE hidden = 0 GROUP BY challenge ORDER BY challenge DESC LIMIT ?1`, MP_WEEKS);
  const sent = await all(`SELECT place AS week, SUM(n) AS results FROM activity WHERE mode = 'challenge' GROUP BY place`);
  const sentBy = new Map(sent.map((r) => [r.week, r.results]));

  const days = [];
  for (let d = new Date(from + "T00:00:00Z"); d.toISOString().slice(0, 10) <= today; d.setUTCDate(d.getUTCDate() + 1)) {
    const k = d.toISOString().slice(0, 10);
    const row = { day: k, players: (dailyAll.find((r) => r.day === k) || { players: 0 }).players };
    for (const [m] of MODES) row[m] = (dailyModes.find((r) => r.day === k && r.mode === m) || { players: 0 }).players;
    days.push(row);
  }
  return {
    players: { today: await players(today), d7: await players(d7), d30: await players(d30), all: await players("") },
    modes_7d: MODES.map(([mode, label]) => {
      const r = byMode.find((x) => x.mode === mode) || {};
      return { mode, label, players: r.players || 0, places: r.places || 0, events: r.events || 0 };
    }),
    daily: days,
    weekly: weekly.reverse(),
    clubs: { total: clubs.total || 0, public: clubs.public || 0, busy: clubs.busy || 0, on_now: clubs.on_now || 0, new_7d: clubs.new_7d || 0 },
    leagues: { total: leagues.total || 0, seats: leagues.seats || 0, new_7d: leagues.new_7d || 0 },
    challenge_weeks: weeks.map((w) => ({ week: w.week, players: w.players, in_groups: w.in_groups, results: sentBy.get(w.week) || 0 })),
  };
}
