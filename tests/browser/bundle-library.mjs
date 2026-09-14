// Puts a library where the web build can compile it in.
//
// `build.rs` embeds `bundled_repos/{owner}/{repo}` into the WebAssembly when
// `CANTARA_BUNDLED_REPOS` names it, and that is how the browser tests get a
// library to search — see `tests/browser/library.js`. But `bundled_repos/` is
// *not* in the repository: it is `.gitignore`d, because in a release build CI
// clones real song repositories into it. On a fresh checkout it does not
// exist.
//
// That is worth spelling out, because the way it failed was quiet. `build.rs`
// takes the *list* of repositories from the environment variable and their
// *files* from the directory, and does not mind if the directory is missing —
// it emits the repository with no files behind it. `ensure_bundled_repos` then
// sees a bundled repository, skips the welcome wizard, and hands the user an
// application with an empty library. Every page the tests opened looked right:
// the search field was there, the route was right, and there was nothing to
// find. Fourteen tests failed on their last assertion rather than their first.
//
// So the library is copied here, from `testfiles/` — which is in the
// repository, and which the testing spec calls the library, "what a church
// would have".

import { cpSync, mkdirSync, rmSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const source = resolve(root, 'testfiles');

/// Must match `CANTARA_BUNDLED_REPOS` in `playwright.config.js`: the variable
/// names the repository, this is where `build.rs` looks for its files, and the
/// two disagreeing is the silent failure described above.
const target = resolve(root, 'bundled_repos/local/testsongs');

// Replaced rather than merged, so that a file removed from `testfiles` does
// not live on in a stale copy and keep a test passing.
rmSync(target, { recursive: true, force: true });
mkdirSync(dirname(target), { recursive: true });
cpSync(source, target, { recursive: true });

console.log(`bundled ${source} -> ${target}`);
