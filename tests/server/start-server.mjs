// Starts a Cantara server for the browser tests of the server mode, with the
// web build it serves.
//
// What it needs is a PostgreSQL the tests may wipe: CANTARA_BROWSER_DATABASE_URL
// names a database, and its `public` schema is dropped and made afresh on
// every start, so each run begins with a server that has never been used.
//
// Like `tests/browser/serve-web.mjs`, the port only opens once everything is
// built: Playwright reads an open port as "ready", and a server that answered
// while the WebAssembly was still compiling would be tested half-built.
//
//   CANTARA_BROWSER_DATABASE_URL=postgres://cantara@127.0.0.1:5432/cantara_browser \
//     npx playwright test -c playwright.server.config.js

import { execFileSync, spawn } from 'node:child_process';
import { cpSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
export const PORT = 8095;

const database = process.env.CANTARA_BROWSER_DATABASE_URL;
if (!database) {
  console.error('CANTARA_BROWSER_DATABASE_URL must name a PostgreSQL database the tests may wipe');
  process.exit(2);
}

const run = (command, args, env = {}) =>
  execFileSync(command, args, { cwd: root, stdio: 'inherit', env: { ...process.env, ...env } });

// A server that has never been used.
run('psql', [database, '-q', '-c', 'DROP SCHEMA IF EXISTS public CASCADE; CREATE SCHEMA public;']);

// The server, and the web build in its server flavour — at the root rather
// than under `/Cantara`, and talking to the server it is loaded from. Copied
// away, because the stream tests' web build lands in the same directory.
run('cargo', ['build', '-p', 'cantara-server']);
run('dx', ['build', '--platform', 'web', '--features', 'server-client', '--base-path', '/']);
const web = join(root, 'target', 'server-web');
rmSync(web, { recursive: true, force: true });
cpSync(join(root, 'target/dx/cantara/debug/web/public'), web, { recursive: true });

const blobs = mkdtempSync(join(tmpdir(), 'cantara-blobs-'));
const server = spawn(join(root, 'target/debug/cantara-server'), [], {
  cwd: root,
  stdio: 'inherit',
  env: {
    ...process.env,
    DATABASE_URL: database,
    CANTARA_LISTEN: `127.0.0.1:${PORT}`,
    CANTARA_PUBLIC_URL: `http://127.0.0.1:${PORT}`,
    CANTARA_WEB_DIR: web,
    CANTARA_BLOB_DIR: blobs,
    CANTARA_BOOTSTRAP_ADMIN_EMAIL: 'admin@example.org',
    CANTARA_BOOTSTRAP_ADMIN_PASSWORD: 'admin-password',
    CANTARA_ALLOW_REGISTRATION: 'false',
    RUST_LOG: 'warn',
  },
});

const stop = () => {
  server.kill();
  rmSync(blobs, { recursive: true, force: true });
};
process.on('SIGTERM', () => { stop(); process.exit(0); });
process.on('SIGINT', () => { stop(); process.exit(0); });
server.on('exit', (code) => process.exit(code ?? 0));
