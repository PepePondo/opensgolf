// A shared club's clock and the order of everyone's actions: the same logic as crates/simgolf/src/mp/session.rs Room,
// tested against the same vectors (test/room_vectors.json), so the server and the game cannot drift apart.
//
// Messages are the game's serde JSON (externally tagged enums): {"Act": <action>}, {"Speed": 2} from a game,
// {"Act": {tick, seq, player, action}} (an action is the log's form: {"speed": 2}, "sync", ...)
// and {"Clock": {base_tick, base_ms, rate}} to the games. The server never reads an
// action: it only stamps and orders it.
//
// Copyright the OpenSGolf contributors. GPL-2.0-or-later.

/** The game tick, in ms (sg_core::flight::TICK_MS). */
export const TICK_MS = 87;
/** How far ahead of the clock an action is stamped, in ms (session.rs LEAD_MS). */
export const LEAD_MS = 700;

export const pausedAt = (tick, ms) => ({ base_tick: tick, base_ms: ms, rate: 0 });

/** The clock's tick at server time `ms` (session.rs Clock::tick_at). */
export function tickAt(c, ms) {
  if (c.rate === 0 || ms <= c.base_ms) return c.base_tick;
  return c.base_tick + Math.floor((ms - c.base_ms) * c.rate / TICK_MS);
}

/** The clock from `ms` on at `rate` (Clock::with_rate). */
export const withRate = (c, ms, rate) => ({ base_tick: tickAt(c, ms), base_ms: ms, rate });

/** A speed asked for, as the clock's rate: 0, 1, 2 or 4. */
export const clampRate = (r) => (r >= 4 ? 4 : r >= 2 ? 2 : r >= 1 ? 1 : 0);

export class RoomCore {
  constructor(clock, nextSeq = 0) {
    this.clock = clock;
    this.nextSeq = nextSeq;
  }

  stamp(ms, player, action) {
    this.nextSeq += 1;
    const tick = Math.max(tickAt(this.clock, ms + LEAD_MS), tickAt(this.clock, ms) + 1);
    return { Act: { tick, seq: this.nextSeq, player, action } };
  }

  /** Sets the clock's rate at `ms`: the Clock message, and (when it goes on) the speed as a stamped action. */
  setRate(ms, player, rate) {
    this.clock = withRate(this.clock, ms, clampRate(rate));
    const out = [{ Clock: this.clock }];
    if (this.clock.rate > 0) out.push(this.stamp(ms, player, { speed: this.clock.rate }));
    return out;
  }

  /** A game's {"Act": ...} or {"Speed": n} at server time `ms`: what everyone is sent, in order (Room::handle). */
  handle(ms, player, m) {
    if (m.Act !== undefined) {
      if (m.Act && typeof m.Act === "object" && "speed" in m.Act) return this.setRate(ms, player, m.Act.speed);
      return [this.stamp(ms, player, m.Act)];
    }
    if (m.Speed !== undefined) return this.setRate(ms, player, m.Speed);
    return [];
  }
}
