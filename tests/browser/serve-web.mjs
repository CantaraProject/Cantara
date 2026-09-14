// Serves the finished web build to the browser tests.
//
// Not `dx serve`, and that is the whole point of this file.
//
// `dx serve` is a development server: it opens its port within two seconds and
// starts answering *while it builds*, with a shell that has no WebAssembly
// behind it or a page saying the app is being rebuilt — and it answers those
// with **200**. Playwright decides a web server is ready by reading a status
// code, so it cannot tell that apart from the finished application, and no
// choice of URL helps: the dev server answers 200 for missing assets too. In
// CI it declared the server ready at two seconds and ran the entire suite
// against a page that was still five minutes from existing. Locally, with a
// warm cache, the retries happened to cover the gap — which is worse, because
// it means the suite is green until the day the build is slow.
//
// So the tests are given a server that cannot lie: `dx build` finishes first,
// and only then does this open the port. The first thing it answers with is
// the real thing. Nothing here needs hot reloading — every test navigates.

import { createServer } from 'node:http';
import { createReadStream, statSync } from 'node:fs';
import { dirname, extname, join, normalize, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

/// Where `dx build --platform web` leaves the built application, and the base
/// path `Dioxus.toml` builds it for. Both are that file's decisions, not this
/// one's.
const PUBLIC = resolve(root, 'target/dx/cantara/debug/web/public');
const BASE = '/Cantara';
const PORT = 8080;

const TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.wasm': 'application/wasm',
  '.json': 'application/json; charset=utf-8',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.svg': 'image/svg+xml',
  '.ico': 'image/x-icon',
  '.woff2': 'font/woff2',
  '.pdf': 'application/pdf',
};

/// The file a request asks for, or `null` where it asks for something outside
/// the built application.
///
/// `normalize` before the prefix check, so that a path climbing out with `..`
/// is resolved before it is judged rather than after.
function fileFor(url) {
  const path = normalize(decodeURIComponent(new URL(url, 'http://x').pathname));
  if (!path.startsWith(`${BASE}/`) && path !== BASE) {
    return null;
  }

  const relative = path.slice(BASE.length).replace(/^\/+/, '');
  const candidate = join(PUBLIC, relative);
  if (!candidate.startsWith(PUBLIC)) {
    return null;
  }

  try {
    if (statSync(candidate).isFile()) {
      return candidate;
    }
  } catch {
    // Falls through to the application shell below.
  }

  // Every other address under the base path is a route of the application —
  // `/Cantara/detail`, and the elements under it — and the router resolves it
  // once the page is loaded.
  return join(PUBLIC, 'index.html');
}

try {
  statSync(join(PUBLIC, 'index.html'));
} catch {
  console.error(`no web build at ${PUBLIC} — run \`dx build --platform web\` first`);
  process.exit(1);
}

createServer((request, response) => {
  const file = fileFor(request.url);
  if (!file) {
    response.writeHead(404).end('not found');
    return;
  }

  response.writeHead(200, {
    'Content-Type': TYPES[extname(file)] ?? 'application/octet-stream',
    // The tests open the same page a few dozen times and the build does not
    // change underneath them, but a cached WebAssembly module surviving a
    // rebuild would be a confusing way to lose an afternoon.
    'Cache-Control': 'no-store',
  });
  createReadStream(file).pipe(response);
}).listen(PORT, '127.0.0.1', () => {
  console.log(`serving ${PUBLIC} at http://127.0.0.1:${PORT}${BASE}/`);
});
