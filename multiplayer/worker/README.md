# Rival-courses leaderboard (private until launch)

A Cloudflare Worker with a D1 database that keeps the weekly rival-courses challenge's board. Part of the multiplayer
work in `multiplayer/` (see the plan in the project's docs). Not deployed yet.

* **The weekly challenge** is worked out from the ISO week's id alone (`src/challenge.js`): the measure the board sorts
  by (cash, fun, members or skill at the end of the first game year), the course theme, the difficulty and the new game's
  seed. The game has the same function in Rust; both are tested against `test/vectors.json`, so they cannot drift apart.
  `GET /challenge` gives this week's, but the game never needs to ask.
* `POST /result`: `{protocol, challenge, player, nick, group, platform, version, cheats, facts}`. `player` is a random id the
  game makes once (16 to 32 hex digits); `group` a friends' code of 6 capitals or digits, or `""` for the public board;
  `facts` the year, cash, holes, fun, skill (hundredths), members and rank at the end of the challenge's year. Results
  are taken for this week's challenge and last week's. `GET /result?d=<the same JSON, base64url>` does the same for
  the game's own HTTP client, which only sends GETs, never with cheats on, and only with this Worker's `protocol` (1) and a game version at least the
  `MIN_VERSION` variable, when set; otherwise the game is told to update. Only a player's best of the week is kept, so
  a worse result writes nothing.
* `GET /board?c=<week>&g=<group>&p=<player>`: the top 50 and the asking player's own place. Without `p` the answer may be
  cached for a minute.
* `GET /admin` and `POST /admin/hide` (basic auth, any user name, the `ADMIN_PASSWORD` secret): the latest results, and
  hiding a result (`{challenge, player}`) or a nickname everywhere (`{nick}`; `show: true` undoes either). A hidden
  nickname shows as a generated one (`src/names.js`).

Kept per result: the player's random id, nickname, group code, the numbers above, the platform and the version. No
address, no country, no cookies.

## Free-plan use

One result is at most one row written (two with the index); a board view reads about 50 rows. The Worker's requests
count towards the account's 100,000 a day, shared with the play-time stats Worker.

## Deploy (when it is time)

```sh
cd multiplayer/worker
npx wrangler@4.152.0 d1 create opensgolf-board        # once; put the id in wrangler.toml
npx wrangler@4.152.0 d1 execute opensgolf-board --remote --file schema.sql
npx wrangler@4.152.0 secret put ADMIN_PASSWORD
npx wrangler@4.152.0 deploy
```

## Tests

`node --test multiplayer/worker/test/*.test.mjs` (Node 22.5 or later: the tests stand D1 in with node:sqlite).
