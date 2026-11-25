// The Worker's entry: only the handler and the Durable Object class may be exported here (index.js has the rest, which
// the tests import).
//
// Copyright the OpenSGolf contributors. GPL-2.0-or-later.

import handler from "./index.js";

export { Club } from "./club.js";
export default handler;
