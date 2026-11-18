// Nicknames on the rival-courses board (README.md "Moderation"): what a player may type, and the generated name shown
// instead of one an admin hid.
//
// Copyright the OpenSGolf contributors. GPL-2.0-or-later.

import { fnv1a } from "./challenge.js";

/** Lengths a nickname may have. */
export const NICK_MIN = 3;
export const NICK_MAX = 16;

// Word stems never allowed, matched after folding look-alike digits and symbols to letters and dropping everything but
// letters. Kept short on purpose: the admin page hides anything this misses.
const BLOCKED = [
  "fuck", "shit", "cunt", "bitch", "nigg", "fag", "rape", "nazi", "hitler", "whore", "slut", "dick", "cock", "pussy",
  "penis", "vagina", "porn", "retard", "kike", "spic", "chink", "wank", "twat", "bastard", "asshole",
];
const FOLD = { 0: "o", 1: "i", 3: "e", 4: "a", 5: "s", 7: "t", 8: "b", "@": "a", $: "s", "!": "i", "|": "i" };

/** The letters a nickname reads as, for the blocklist. */
export function folded(s) {
  return [...s.toLowerCase()].map((c) => FOLD[c] ?? c).filter((c) => c >= "a" && c <= "z").join("");
}

/** A typed nickname, tidied: or null when it may not be used. */
export function cleanNick(nick) {
  if (typeof nick !== "string") return null;
  const n = nick.trim().replace(/\s+/g, " ");
  if (n.length < NICK_MIN || n.length > NICK_MAX) return null;
  if (!/^[A-Za-z0-9 _.\-]+$/.test(n)) return null;
  const f = folded(n);
  if (BLOCKED.some((w) => f.includes(w))) return null;
  return n;
}

const FIRST = ["Birdie", "Eagle", "Bogey", "Fairway", "Bunker", "Caddie", "Divot", "Mulligan", "Putter", "Wedge",
  "Links", "Albatross"];
const SECOND = ["Fan", "Pro", "Ace", "Walker", "Swinger", "Chipper", "Driver", "Golfer", "Hacker", "Champ"];

/** The name shown for a player whose nickname was hidden: the same one every time for the same player. */
export function generatedNick(player) {
  const h = fnv1a(`nick/${player}`);
  return `${FIRST[h % FIRST.length]} ${SECOND[(h >>> 8) % SECOND.length]} ${String((h >>> 16) % 1000).padStart(3, "0")}`;
}
