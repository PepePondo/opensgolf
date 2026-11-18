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
