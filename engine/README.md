# wsr-engine — Rust port of the Wall Street Raider game logic

`wsr-core` is a UI- and OS-independent port of the game logic in `WSR.EXE` (PowerBASIC 8). It builds
for `wasm32-unknown-unknown` (the browser product, via `wsr-wasm`) and native test targets.

```sh
mise install && mise run setup                        # toolchain + .venv for the emulator harness (repo root)
mise run test                                         # public synthetic-resource tests, no original files
mise run test-strict                                  # extract local resources and run all original comparisons
cargo build -p wsr-wasm --target wasm32-unknown-unknown --release
```

## Layout

Game resources are supplied by the host's runtime catalog. Native hosts install an immutable
`Arc<ContentPack>` on their engine thread with `content::install`; the compatibility tests can
instead set `WSR_CONTENT_PACK` to a locally generated pack. A host thread represents one session.
Each function retains its catalog while borrowing resource data, including across modal UI calls.
The WASM binding validates and installs the catalog when the user imports the installer.

Default Cargo tests use synthetic catalogs. Differential suites require the `original-tests`
feature, local extracted assets, the generated catalog and the Python emulator dependencies.

| Path | What |
|---|---|
| `wsr-core/src/state.rs` | `Globals`: every global of the original, with address, PB type and `DIM` bounds. **Generated** by `tools/gen_state.py`; field names come from `tools/names.tsv`. |
| `wsr-core/src/pb/` | Exact reimplementations of the PowerBASIC runtime the game depends on: types and rounding, arrays, strings, `RND`, number formatting (`STR$`, `FORMAT$`, `WRITE #`, `INPUT #`), file I/O, the save/text obfuscation. |
| `wsr-core/src/save.rs` | `SaveGame` / `LoadGame`. |
| `wsr-core/src/logic/` | Ported game routines, grouped by subsystem. |
| `wsr-core/src/ui.rs` | The `Ui` trait: the only way the engine talks to the user (`MSGBOX`, `INPUTBOX`, reports, news). |
| `wsr-core/tests/fixtures/` | Test vectors produced by running the original runtime routines under CPU emulation (`tools/emu/*.py`). |
| `wsr-wasm/` | Browser binding (plain C-ABI exports, no dependencies). |

## Porting rules

1. **Compatibility first.** A ported routine must reproduce the original's behaviour, including its bugs.
   Correctness is checked against the original: runtime routines against emulator vectors, game routines
   (later) against save files produced by the real game under Wine (quarter-by-quarter state diffs).
2. **Fixes are opt-in.** Known game bugs are listed in [`BUGS.md`](BUGS.md). A fix lives behind a flag
   (`Fixes` setting) so the compat build keeps matching the original.
3. **Traceability.** Every ported routine carries the original address and name in its doc comment
   (`/// Port of SaveGame (0x4857af)`). Things not ported yet are marked `// PORT-TODO(0xADDR): Name`.
4. **Blocking UI is fine.** Game code calls `ui.msgbox(…)` synchronously, like the original. The web
   binding runs the engine in a Web Worker that blocks on `Atomics.wait` until the page answers.
5. **Numbers.** `pb::types::Ext` stores the complete x87 sign/exponent and 64-bit significand.
   Simulation arithmetic, decimal parsing, formatting, saved-game I/O and persistent snapshots retain
   those bits. SINGLE and DOUBLE fields and locals round explicitly at their original stores; integer
   stores use the x87 nearest-even helpers. See [NUMBERS.md](NUMBERS.md) for the representation,
   rounding boundaries, transcendental limits and lossless JSON format.

6. **Strings** are byte strings (`PbStr`, Windows-1252), compared byte-wise like the original.

## Status

See the root `README.md` §5 for the decompilation this port is based on. Progress:
`tools/port_status.py` (ported routines, `PORT-TODO` markers). Done: runtime support (`pb/`), state model,
save/load, the financial/valuation core, report screens, new-game setup, the economy and
`SimProcessNextEntity`, the turn loop with everything it schedules, the player actions and the
computer players (`logic/`), plus the menu dispatcher, trading and research screens and host shell.
Earnings-tip events, futures physical delivery and pre-7.9 commodity-unit conversion are connected to
their simulation and save-loading callers; ETF industry projections include the fund-strategies
report. Modal ticker pauses use the host clock; menu destruction keeps the original's nested callback
behavior. Quarter-end autosaves and excess-loss recapture headlines now reach their callers.
The browser binding exposes the trading desk, new games, reports, choices, charts, ticker steps and a
virtual data directory backed by IndexedDB. See [`../web/README.md`](../web/README.md) to run it.
The integrated regression suite covers runtime behavior and original-game comparisons. Binding
regressions cover failed loads, financial guards and save/autosave ordering. Browser gameplay checks include
installer import, new games, trading, save/reload, ticker start/stop, reports and charts on desktop and
mobile layouts. The `extensions`
module ports VARICHEK high scores, WSRDLL remarks and the complete LAWDLL selection dialog; dedicated
tests run the original DLL exports and compare their messages, return values and saved bytes.
The routine-size coverage report does not include these DLLs.

`mise run test` runs four test cases at a time with at most four emulator workers per case. Set
`WSR_SEED_THREADS` for a direct Cargo invocation to control the latter. Emulator results are cached
under `target/emu-cache`; `WSR_EMU_CACHE_MB` limits that cache (the mise task allows 20,000 MiB).
`WSR_EMU_CACHE_DIR` selects an isolated cache for harness diagnostics. Cache reads retain all outputs
and file bytes for later comparisons; incomplete entries from older harness versions are replaced.
An instruction budget is a debugging aid: full trading and ticker cases can legitimately exceed
300 million instructions, so the normal regression run uses the emulator's wall-clock limit.

`mise run test-strict` fails when the shared differential harness cannot run the original or a seed
filter selects no cases. Consecutive-tick tests fail on PowerBASIC runtime errors rather than silently
ending their trajectory. The shared comparator checks complete array shapes, finite floating values
and exact integer/CURRENCY tokens, including values above 2^53. Damaged-input tests must explicitly
identify their initial non-finite cells; this does not exempt newly invalid values.

For independently retained engine trajectories, use the [campaign comparison harness](CAMPAIGNS.md).
It compares state, RNG, UI and durable files after every command, retains the original's 80-bit globals
between calls, and saves a replayable first-divergence report. Its self-tests are available through
`mise run test-campaign`. A matching bounded run is distinct from reaching a requested campaign end.

Browser sessions opt into `NewGameOptions::repair_initial_shares`, correcting original bug 67's
zero-share setup before dividend calculation. The default core API retains the original behavior
for differential tests. Browser loads also reset non-finite dividends from older affected saves to
zero and show a recovery notice. Manual-save confirmation in the browser follows the durable storage
acknowledgement; native hosts retain the original modal confirmation.

The browser also enables `Ui::safety_checks`: ETF startup/migration preflights eligible company pools
and bounds every random draw and repeated holding search. Stable load/save boundaries validate all
floating storage, required references and record counts. Loads publish a candidate only after fixups
pass; rejected loads preserve live state, RNG and virtual files. Manual saves and autosaves validate
both before and after recalculation so invalid results cannot replace a good file. Default core hosts
retain original behavior unless they enable these guards.

Hosts implement `Ui::local_time` for modal ticker deadlines and `Ui::write_file` for saves initiated
inside simulation ticks. Shell calls also record file changes in `Env::files`; persist those after
each call. The WASM binding records both against the command's file view, preserving write order when
a manual save and a subsequent autosave target the same slot. Failed loads and cancelled new games
retain the current session's game and RNG.
Hosts retain `extensions::remarks::Remarks` and `extensions::high_score::HighScores` across games,
and expose them through `Ui::random_remark` and `Ui::high_score`. The browser host does this; headless
hosts may omit those decorative and file-backed features. `Remarks::next` receives a validated `content::ContentPack`;
its text is supplied by the host at runtime. See the runtime-content prototype in `web/README.md`.

Signature conventions: `g: &mut Globals` first; `ui: &mut dyn Ui` when the routine (or a callee) shows
UI or posts news; `rng: &mut pb::rng::Rng` when it draws `RND` (the RND state is not part of `Globals`).
