-- The rival-courses leaderboard (multiplayer/worker/README.md). Safe to run again: everything is IF NOT EXISTS.

-- One row per player and weekly challenge: the player's best result that week. `player` is a random id the game makes once
-- and keeps in settings.json (no account, no address); `nick` the name they chose; `grp` a friends' group code ('' for
-- the public board). `score` is the week's measure (challenge.js), the one the board sorts by.
CREATE TABLE IF NOT EXISTS results (
  challenge TEXT NOT NULL,            -- 'YYYY-Www', the ISO week
  player    TEXT NOT NULL,
  nick      TEXT NOT NULL,
  grp       TEXT NOT NULL DEFAULT '',
  score     INTEGER NOT NULL,
  cash      INTEGER NOT NULL,
  fun       INTEGER NOT NULL,
  skill     INTEGER NOT NULL,          -- hundredths
  members   INTEGER NOT NULL,
  holes     INTEGER NOT NULL,
  rank      INTEGER NOT NULL,          -- 0 municipal .. 3 championship
  platform  TEXT NOT NULL,
  version   TEXT NOT NULL,
  at        TEXT NOT NULL,             -- ISO time (UTC) of the best result
  hidden    INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (challenge, player)
);
CREATE INDEX IF NOT EXISTS results_board ON results (challenge, grp, hidden, score DESC, at);

-- Nicknames an admin hid: a player whose nick is here is shown under a generated name.
CREATE TABLE IF NOT EXISTS hidden_nicks (
  nick TEXT PRIMARY KEY COLLATE NOCASE,
  at   TEXT NOT NULL
);

-- The shared clubs' list (multiplayer/worker/src/club.js keeps each club's row up to date). A club itself lives in its
-- Durable Object; this is only what the list shows. `compat` is the game record a joiner must match (mp/version.rs).
CREATE TABLE IF NOT EXISTS clubs (
  id      TEXT PRIMARY KEY,          -- the Durable Object's id (64 hex)
  name    TEXT NOT NULL,
  version TEXT NOT NULL,             -- the game version it plays
  compat  TEXT NOT NULL,             -- JSON
  listed  INTEGER NOT NULL DEFAULT 1,
  hidden  INTEGER NOT NULL DEFAULT 0, -- hidden by an admin
  players INTEGER NOT NULL DEFAULT 0, -- on now
  tick    INTEGER NOT NULL DEFAULT 0,
  made    TEXT NOT NULL,
  at      TEXT NOT NULL              -- last change (someone joined or left, or a save)
);
CREATE INDEX IF NOT EXISTS clubs_list ON clubs (listed, hidden, at);

-- Neighbours leagues (src/league.js): 2 to 4 players each run their own course on their own plot of one property. A
-- player's row keeps their latest numbers and their latest save (for the neighbours to look round).
CREATE TABLE IF NOT EXISTS leagues (
  code       TEXT PRIMARY KEY,       -- 6 letters/digits, what friends type to join
  name       TEXT NOT NULL,
  property   INTEGER NOT NULL,       -- 0..15, the game's property slot
  difficulty INTEGER NOT NULL,       -- 0 easy .. 3 impossible
  measure    TEXT NOT NULL,          -- what the table sorts by: cash, fun, members, skill
  seed       INTEGER NOT NULL,       -- the land's seed (each seat's plot its own from it)
  owner      TEXT NOT NULL,
  made       TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS league_players (
  code    TEXT NOT NULL,
  seat    INTEGER NOT NULL,          -- 0..3, which plot
  player  TEXT NOT NULL,
  nick    TEXT NOT NULL,
  facts   TEXT,                      -- JSON: the latest numbers and game date
  at      TEXT NOT NULL,             -- last report (or joining)
  save    BLOB,                      -- the latest save
  save_at TEXT,
  PRIMARY KEY (code, seat),
  UNIQUE (code, player)
);

-- Multiplayer use by day (src/activity.js), for the play-time stats page's Multiplayer section, which shows only counts.
-- `place` is the club id, league code or challenge week; `n` how many times that day (club joins, league reports, results).
CREATE TABLE IF NOT EXISTS activity (
  day    TEXT NOT NULL,              -- YYYY-MM-DD (UTC)
  mode   TEXT NOT NULL,              -- club, league, challenge
  place  TEXT NOT NULL,
  player TEXT NOT NULL,
  n      INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY (day, mode, place, player)
);
