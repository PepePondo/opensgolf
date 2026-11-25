// Stand-ins for a Durable Object's context (SQLite storage, hibernating WebSockets) and namespace, for the tests and the
// local server (serve.mjs).
import { DatabaseSync } from "node:sqlite";
import { randomBytes } from "node:crypto";

/** ctx.storage.sql over node:sqlite. */
export function memorySql(raw = new DatabaseSync(":memory:")) {
  return {
    raw,
    exec(query, ...binds) {
      const st = raw.prepare(query);
      const reads = /^\s*(SELECT|WITH|PRAGMA)\b/i.test(query) || /\bRETURNING\b/i.test(query);
      const rows = reads ? st.all(...binds) : (st.run(...binds), []);
      return {
        toArray: () => rows,
        one: () => {
          if (rows.length !== 1) throw new Error(`expected one row, got ${rows.length}`);
          return rows[0];
        },
        [Symbol.iterator]: () => rows[Symbol.iterator](),
      };
    },
  };
}

/** A server-side socket: what the Durable Object sends lands in `sent` (parsed), or goes to `onSend`. */
export class FakeSocket {
  constructor(onSend) {
    this.sent = [];
    this.closed = null;
    this.att = null;
    this.onSend = onSend;
  }
  send(text) {
    if (this.closed) throw new Error("closed");
    if (this.onSend) this.onSend(text);
    else this.sent.push(JSON.parse(text));
  }
  close(code, reason) {
    if (!this.closed) this.closed = { code, reason };
    if (this.onClose) this.onClose(code, reason);
  }
  serializeAttachment(v) {
    this.att = v === null || v === undefined ? null : structuredClone(v);
  }
  deserializeAttachment() {
    return this.att === null ? null : structuredClone(this.att);
  }
  take() {
    return this.sent.splice(0);
  }
}

/** A Durable Object's ctx: its storage and the sockets it accepted (which survive the object being made again). */
export function fakeCtx(sql = memorySql()) {
  const sockets = [];
  return {
    storage: { sql },
    acceptWebSocket: (ws) => sockets.push(ws),
    getWebSockets: () => sockets.filter((ws) => !ws.closed),
    sockets,
  };
}

/** env.CLUB: a namespace making one object per id. */
export function fakeNamespace(Class, env) {
  const objects = new Map();
  const id = (hex) => ({ toString: () => hex, hex });
  return {
    objects,
    newUniqueId: () => id(randomBytes(32).toString("hex")),
    idFromString: (hex) => {
      if (!/^[0-9a-f]{64}$/.test(hex)) throw new Error("bad id");
      return id(hex);
    },
    get(i) {
      if (!objects.has(i.hex)) {
        const ctx = fakeCtx();
        objects.set(i.hex, { ctx, obj: new Class(ctx, env) });
      }
      const o = objects.get(i.hex);
      return { fetch: (req, init) => o.obj.fetch(typeof req === "string" ? new Request(req, init) : req) };
    },
  };
}
