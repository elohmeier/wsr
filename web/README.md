# Browser host

The Svelte interface runs the Rust engine in a Web Worker. A shared-memory bridge allows the engine to wait for modal UI replies. Build with `npm ci --ignore-scripts`, `npm run check` and `npm run build` in this directory. Rust's `wasm32-unknown-unknown` target is required.

## Local resources

Import a supported installer in the browser. `installer.mjs` validates the installer container, temporarily reads the required binaries, then uses `content-manifest.json` to extract resource records. Only data files, documentation, bitmap resources and `ENGINE-CONTENT.WSR` are retained in IndexedDB. Extraction and game processing stay local; using the optional learning coach sends selected text and visible game information to your configured model provider.

The catalog format is `WSRCP002`, a little-endian u32 record count, records of `(u32 id, u32 byte length, raw bytes)`, and a trailing CRC32 of the preceding bytes. IDs and the complete required set are shared with the Rust loader through a generated schema. Source-image SHA-256 hashes identify the supported binary version. Windows-1252 bytes are preserved. Catalog replacement is validated before the live catalog or stored file changes.

The manifest is extraction metadata. Assembly recipes combine local source ranges with structural punctuation, format placeholders or independently authored test inputs. They must not contain original narrative text. After editing IDs, run `python3 tools/generate_content_schema.py` from the repository root.

`node web/extract-content.mjs extracted/app engine/target/owner-content.pack` creates a native test catalog. Generated catalogs contain original resources and must stay private.

## Hosting

The browser requires cross-origin isolation and `SharedArrayBuffer`. `python3 web/serve.py 8899` serves the production build with isolation headers. Vite's development and preview servers also supply them.

Static hosting must provide isolation headers or use the included isolation bootstrap. Build assets use a relative base so project subpaths can resolve their worker and WASM files. A first visit on a headerless static host may require one automatic reload after service-worker activation. A secure origin (HTTPS or localhost) is required.

Some hard reload and developer-tool modes bypass service workers; a normal reload restores
the fallback. Hosting with response headers avoids this dependency. The public hosting smoke
test uses a cold browser profile, a headerless server and a normal reload under `/wsr/`.

## Validation

`npm run test:content` and `npm run test:client` run without original assets. `npm run test:installer` compares local extraction with independently extracted files. `npm run test:content:browser -- INSTALLER COMPLETE_SAVE URL` exercises import, save loading, reload and recovery from missing or malformed cached catalogs without losing saves. The browser scripts use the `agent-browser` CLI and isolated sessions.

The root README describes opt-in original-engine comparisons and the remaining public-release provenance issue.

## Learning coach

Choose **Learning coach**, open **Connection**, and enter your provider's model ID and your own API
key. OpenRouter and browser-accessible endpoints implementing the chat-completions API are supported.
Custom endpoints need HTTPS (HTTP is allowed on localhost), CORS access from this app and streaming
tool-call support. No application backend or native Pi process is needed. Keys and conversations
stay in memory for this page session; no shared API key is included in the build.

The coach uses Pi in a worker and a separate just-bash worker to search the imported help, glossary
and strategy manual with `cat`, `grep` and `jq`, and inspect a frozen player-visible game snapshot
through the `wsr` CLI. Only retrieved excerpts and queried visible state enter model requests.
Full saves, raw globals, other human players' private finances and unrevealed events are excluded.
The assistant has no trading, dialog-answering, save/load or time-advancing tool.

Open the coach from **Learning coach** in the desk menu. The ticker stays suspended while the coach
is open. A dialog appearing during snapshot capture uses the last completed snapshot
plus its visible text; later player actions mark earlier analysis as outdated. A new game, save
load or player change clears the conversation. **Stop** cancels the current response.

Source citations open a local viewer with numbered lines. **Inspection details** shows the commands
used and their bounded output. **Add tutorial** imports a text, Markdown or HTML tutorial for this
page session; the external tutorial website is not fetched automatically. Strategy help remains
conversational, without a strategy notebook or dedicated calculators.

The runtime imports are loaded when the coach is used. The UI uses the existing Nova/Phosphor shadcn
controls, including MessageScroller, Message, Bubble, InputGroup, Trace, Marker, Sheet and Dialog.

```sh
npm --prefix web run test:coach
cargo test --manifest-path engine/Cargo.toml -p wsr-wasm --lib observer::tests
npm --prefix web run test:coach:browser -- ../wsr985.exe /path/to/fixtures/GAME49.DAT http://localhost:8899/
```

The browser regression uses the actual Pi streaming transport, shell worker and WASM engine against
a local scripted model endpoint, with no paid API calls. Provide a complete unfinished human-player
save; the `browser_fixtures` example generates a suitable `GAME49.DAT`. It checks documentation citations,
HTML tutorial normalization, read-only state access, scrolling, cancellation, uncluttered game dialogs,
mobile layout and conversation reset. No live model account is used by this regression.
