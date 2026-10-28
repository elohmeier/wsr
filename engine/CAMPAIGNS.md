# Independent campaign comparisons

`tools/emu/campaign.py` compares the Rust engine with the owner's original `WSR.EXE` under
Unicorn. Both peers retain their own game, RNG and virtual files between commands. The original
also retains its 80-bit globals and PowerBASIC runtime memory. Neither peer receives the other's
state after initialization. Snapshots are observations, rather than the next tick's inputs.

From the repository root, after `mise run setup` and extracting the owner's installer:

```sh
.venv/bin/python -I tools/emu/campaign.py --save /path/to/GAME01.DAT --steps 64
.venv/bin/python -I tools/emu/campaign.py --save /path/to/GAME01.DAT --steps 10000 --quarters 1
.venv/bin/python -I tools/emu/campaign.py --save /path/to/GAME01.DAT --steps 100000 --until-game-over
```

The command builds the native driver (`wsr-wasm/examples/campaign_driver.rs`, optimized test
profile) without adding dependencies to the engine. Output defaults to a new directory under
`engine/target/campaign/`. `--out DIRECTORY` selects an empty directory; existing results are
never overwritten. The original executable is emulated and is never launched natively.

A passing bounded run verifies only the commands it actually compared. `--quarters` and
`--until-game-over` require the requested simulated endpoint; exhausting the command limit is
`incomplete`, even when every comparison matched. AI turns may advance multiple simulation
positions, so command counts are not game dates. Runs print progress while they work.

## Initialization and replay

`--save` loads a shared starting snapshot with the Rust decoder and sets the ticker running. This
initial load is **outside** the comparison. `--snapshot` accepts a complete address-keyed snapshot
instead. Initial field overrides in a plan are applied once, before either peer starts. The exact
initial files, selected save slot and `LASTGAME.DAT` are retained locally for both peers.

Replay a result from its original state, clock, RNG, plan and virtual files:

```sh
.venv/bin/python -I tools/emu/campaign.py --replay engine/target/campaign/RUN/run.json
```

Replay verifies the retained snapshot, asset hashes and original executable. Source and driver
hashes are recorded, but a rebuilt Rust driver is allowed so a correction can be tested against
the same case. The previous results remain intact. Both trajectories start again from the
beginning; the snapshots are observations and do not reconstruct all PowerBASIC runtime memory.

Non-finite input values fail with `invalid-input`. For diagnosis, `--allow-initial-nonfinite`
permits only cells already invalid in the input and lists them in `run.json`. An exemption retires
when that cell becomes finite. Newly invalid cells still fail. This option does not establish
financial validity of the input. The current local browser fixture has 40 pre-existing non-finite
`DividendPerShare` cells; it requires this explicit option to investigate later behavior.

## Plans and answers

Use `--plan tools/emu/campaigns/turn-save-reload.json`, or supply a version 1 JSON plan:

```json
{
  "version": 1,
  "steps": [
    {"op": "tick", "repeat": 32},
    {"op": "save"},
    {"op": "reload"},
    {"op": "clock", "arg": 1},
    {"op": "tick", "repeat": 1600}
  ]
}
```

| Operation | Engine entry or host input |
|---|---|
| `tick` | Complete `TickerSimSteps(1)`, including scheduled events and AI calls |
| `entity` | `SimProcessNextEntity` without the surrounding ticker |
| `end_turn` | `EndTurn` |
| `select`, `arg` 11–1600 | Set `TargetCorp`, then `SelectTargetCorp` |
| `trade`, `mode` 1–7, `arg` 1–16 | Action-menu mode and button through `DoTransactionMenuAction` |
| `save` / `reload` | Save prompt / saved-game menu; each peer uses its own files |
| `clock`, `arg` 0 or 1 | Shared host request to stop or resume the clock |
| `interest_rates` | Interest-rate report and its modal callback |
| `pause`, `arg` seconds | Ticker pause, useful for clock validation |
| `notice`, `text` | Quick message, useful for CP1252 and host-adapter validation |
| `state` | Observation with no game action |

Commands can have an `answers` list. Message boxes consume `IDYES`, `IDNO`, `IDOK`, `IDCANCEL` or
numeric button ids; input boxes consume text. `pick:N` selects a report's Nth linked line and
`sym:TEXT` answers the symbol-entry dialog. Defaults match the existing emulator: Yes/OK,
empty input, report OK. Actual consumed replies are translated into equivalent Windows dialog
actions. Unused planned or original replies fail the run. Optional custom choices are canceled;
explicit choices in specialized dialogs need an additional control adapter. Forced dialog closes
are harness errors, not successful coverage.

`clock` defaults to `2000-01-03T10:00:00` and advances by 2,000 ms per command. `--clock` and
`--step-ms` configure it, including midnight transitions. Both engines see the same fixed time
throughout each command. No game routines are skipped. External DLL calls use the existing emulator
stubs; the separate DLL differential tests remain necessary. This harness compares engine entry
points, rather than the complete Windows `WinMain` or browser session lifecycle.

## Comparisons and artifacts

Every command checks all generated globals except an explicit list of Windows/GDI handles.
Integers, raw CURRENCY values, strings and durable file bytes compare exactly. Floating values
use `--rel-tol` and `--abs-tol` (both default to `1e-9`). RNG states compare exactly. Common message
boxes, input replies, notices and reports compare in order within each kind; report window padding
is removed. The Windows `OLDGAME.DAT` codec intermediate and `TMP_QM.DAT` helper file are excluded.
The allowlists and all input/source hashes appear in `run.json`.

The first failure retains `before-*.json`, `after-*.json`, UI transcripts, original dialog models,
the original call log, changed files and `result.json`. Floating differences include original
80-bit bytes. `trace.jsonl` records command numbers, game dates, RNG states, timings and failures.
PowerBASIC runtime errors, emulator warnings/timeouts, driver failures and new non-finite values
cannot pass silently. Exit codes are 0 for a completed comparison, 1 for a divergent, incomplete
or failed run, and 2 for invalid input or setup.

The first local diagnostic, with RNG 123456, found a 5% valuation gap at command 39 for company 534.
Its holding had an invalid dividend; the original's unordered comparison skipped it, but the port
let it invalidate cash-flow totals. The corrected comparison passes all 64 retained commands.
Save confirmations now also match the original. Integer and currency save items and typed loads
preserve their 80-bit intermediates instead of passing through DOUBLE first.

The f80 migration now passes the retained first save byte for byte, followed by reload, a player
handoff and subsequent ticker steps: 101 commands in total. The handoff also exposed an omitted
PauseTicker(0) in RebuildStreamingQuotesPanel; its host-clock behavior is now preserved. The longer
campaign and multi-year bit equality remain unproven.

EXT observations use a JSON number when exactly representable as DOUBLE, otherwise a string
`"x87:SSSSMMMMMMMMMMMMMMMM"` carrying the sign/exponent and significand in hexadecimal. Legacy
numeric snapshots still load. Both comparators handle the full extended exponent range and low bits
without narrowing to DOUBLE. `--exact-ext` requires bit equality for EXT cells, including signed zero;
the default tolerance remains available for trajectory diagnosis. Raw observations do not change the
byte-exact saved-file comparison.
