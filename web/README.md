# Browser host

The Svelte interface runs the Rust/WASM engine in a Web Worker and stores imported
resources and saves in IndexedDB. [Play on GitHub Pages](https://elohmeier.github.io/wsr/)
or see the [project README](../README.md) for local setup and public checks.
All commands below run from the repository root.

## Live development

After installing dependencies with `npm --prefix web ci --ignore-scripts`:

```sh
npm --prefix web run build:wasm
npm --prefix web run dev
```

Open <http://localhost:8899/>. Vite reloads interface changes; rerun `build:wasm`
after editing Rust. `npm --prefix web run build` produces the complete static app
in `web/dist`, including dependency license notices.

## Hosting

The [CI workflow](../.github/workflows/ci.yml) checks every pull request and deploys
`web/dist` to [GitHub Pages](https://elohmeier.github.io/wsr/) after successful
pushes to `main`. It can also be run manually from Actions.

`public/manifest.webmanifest` defines the Home Screen app's name, standalone
display mode, icons and launch URL. Its paths are relative to keep installs scoped
to `/wsr/` on Pages and to the app's directory on other hosts. iOS uses the linked
180px Apple touch icon. The system keeps content within the screen's safe area.

The engine needs `SharedArrayBuffer` and cross-origin isolation on HTTPS or
localhost. On GitHub Pages, the included service worker enables isolation and may
reload the first visit once. Relative asset paths support the `/wsr/` subpath.
If a hard reload bypasses the service worker, reload normally.

For other hosts, send `Cross-Origin-Opener-Policy: same-origin` and
`Cross-Origin-Embedder-Policy: require-corp`, or keep the isolation bootstrap.
Vite and `python3 web/serve.py 8899` provide these headers locally.

## Runtime resources

`installer.mjs` reads the owner's `wsr985.exe` without executing it.
`content-manifest.json` identifies supported source binaries by SHA-256 and
describes extraction ranges. Only data, documentation, bitmap resources and the
validated `ENGINE-CONTENT.WSR` catalog are retained in browser storage.

The catalog uses `WSRCP002`, a little-endian u32 record count, records of
`(u32 id, u32 byte length, raw bytes)`, and a trailing CRC32. Windows-1252 bytes
are preserved. Manifest recipes must contain extraction metadata and structural
tokens, without original narrative text. After changing IDs, run
`python3 tools/generate_content_schema.py`.

For native comparisons, `node web/extract-content.mjs extracted/app
engine/target/owner-content.pack` creates a private resource pack. See the
[engine guide](../engine/README.md#original-engine-comparisons) for setup.

## Learning coach

Open **Learning coach** in the desk menu, then **Connection**. Enter your own
model ID and API key for OpenRouter or a browser-accessible chat-completions
endpoint. Custom endpoints need HTTPS (HTTP on localhost), CORS and streaming
tool-call support. Keys and conversations stay in memory for the page session.

The coach uses Pi and a just-bash worker to search imported documentation and
inspect a frozen snapshot of player-visible state. Retrieved excerpts and queried
visible state go to your provider; full saves, raw globals, other human players'
private finances and unrevealed events are excluded. Its tools are read-only.

The ticker pauses while the coach is open. Source citations open local documents;
**Inspection details** shows tool commands and output. **Add tutorial** imports
a local text, Markdown or HTML file for this session. **Stop** cancels a response.
Starting or loading a game, or changing players, clears the conversation.

## Browser validation

Browser scripts use the `agent-browser` CLI with isolated sessions. With the app
served locally, supply your installer and a complete unfinished human-player save:

```sh
node web/content-browser-test.mjs wsr985.exe /path/to/GAME01.DAT http://localhost:8899/
node web/browser-test.mjs wsr985.exe /path/to/GAME01.DAT http://localhost:8899/
node web/coach-browser-test.mjs wsr985.exe /path/to/GAME01.DAT http://localhost:8899/
```

The coach test uses a local scripted model endpoint and makes no paid API calls.
`npm --prefix web run test:installer` compares importer output with files in
`extracted/app`. For asset-free hosting validation, run
`node web/hosting-browser-test.mjs URL` against a headerless static host serving
the build under `/wsr/`.
