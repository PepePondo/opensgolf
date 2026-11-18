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
