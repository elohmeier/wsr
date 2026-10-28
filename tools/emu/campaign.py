"""Compare independently retained Rust and WSR.EXE campaigns; stop at the first divergence.

Run with the repository's .venv/bin/python -I tools/emu/campaign.py --save GAME01.DAT --steps 100.
No engine state or RNG is copied between peers after initialization. The original stays in one
Unicorn instance, retaining its x87 EXT storage, virtual files, heap and runtime state.
"""
import argparse
import datetime as dt
from decimal import Decimal, localcontext
import hashlib
import json
import math
from pathlib import Path
import re
import selectors
import subprocess
import struct
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
# Explicit local import also works under Python's isolated (-I) mode.
sys.path.insert(0, str(Path(__file__).resolve().parent))

# Window/GDI handles have no browser equivalent. No financial or simulation scratch fields are hidden.
UI_KEYS = {f"{n:06x}" for n in range(0x6c3530, 0x6c35a4, 4)} | {"6c376c", "6c3f18", "6c4000"}
# These are Windows helper/codec intermediates, not durable save or preference files.
TRANSIENT_FILES = {"TMP_QM.DAT", "OLDGAME.DAT"}
OPS = {"state", "tick", "entity", "end_turn", "select", "trade", "pause", "save", "reload", "clock", "notice", "interest_rates"}


def dump(path, value):
    Path(path).write_text(json.dumps(value, ensure_ascii=True, indent=2, allow_nan=False) + "\n")


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def read_assets(directory):
    files = {}
    for path in sorted(Path(directory).iterdir()):
        if path.is_file() and path.suffix.upper() in {".DAT", ".WSR"}:
            key = path.name.upper()
            if key in files:
                raise ValueError(f"Ambiguous case-insensitive asset name: {key}")
            files[key] = path.read_bytes()
    return files


def json_model(value):
    if isinstance(value, bytes):
        return value.decode("latin1")
    if isinstance(value, dict):
        return {key: json_model(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [json_model(item) for item in value]
    return value


def byte_text(value):
    """Protocol strings are the game's CP1252 bytes represented as Latin-1 codepoints."""
    return value.encode("cp1252").decode("latin1")


def finite(value):
    if isinstance(value, str) and value.startswith("x87:") and len(value) == 24:
        return int(value[4:8], 16) & 0x7fff != 0x7fff
    return isinstance(value, (int, float)) and math.isfinite(value)


def decimal_value(value):
    if isinstance(value, str) and value.startswith("x87:"):
        se, mant = int(value[4:8], 16), int(value[8:], 16)
        exponent = max(se & 0x7fff, 1) - 16383 - 63
        result = Decimal(mant) * Decimal(2) ** exponent
        return -result if se & 0x8000 else result
    return Decimal(value)


def close_float(a, b, rel_tol, abs_tol, exact=False):
    if exact:
        return ext_bits(a) == ext_bits(b)
    if a == b:
        return True
    with localcontext() as context:
        context.prec = 80
        a, b = decimal_value(a), decimal_value(b)
        difference = abs(a - b)
        return difference == 0 if exact else difference <= max(Decimal(abs_tol), Decimal(rel_tol) * max(abs(a), abs(b)))


def ext_bits(value):
    if isinstance(value, str) and value.startswith("x87:"):
        return int(value[4:8], 16), int(value[8:], 16)
    bits = struct.unpack(">Q", struct.pack(">d", float(value)))[0]
    sign, exponent, fraction = bits >> 63 << 15, bits >> 52 & 0x7ff, bits & ((1 << 52) - 1)
    if exponent == 0:
        if fraction == 0:
            return sign, 0
        width = fraction.bit_length()
        return sign | (width - 1 - 1074 + 16383), fraction << (64 - width)
    if exponent == 0x7ff:
        return sign | 0x7fff, (1 << 63) | fraction << 11
    return sign | (exponent - 1023 + 16383), ((1 << 52) | fraction) << 11


def differences(left, right, layout, rel_tol=1e-9, abs_tol=1e-9, ignore=UI_KEYS, limit=50, allowed_nonfinite=(), exact_ext=False):
    """Exact strings/integers/CURRENCY, tolerant floating storage; never accept non-finite values."""
    found, count = [], 0
    for key, name, typ, bounds in layout:
        if key in ignore:
            continue
        if key not in left or key not in right:
            count += 1
            if len(found) < limit:
                found.append({"field": name, "address": key, "error": "missing field"})
            continue
        a, b = left.get(key), right.get(key)
        pairs = zip(a, b) if bounds and isinstance(a, list) and isinstance(b, list) else [(a, b)]
        expected = math.prod(hi - lo + 1 for lo, hi in bounds) if bounds else None
        if bounds and (not isinstance(a, list) or not isinstance(b, list) or len(a) != expected or len(b) != expected):
            count += 1
            if len(found) < limit:
                found.append({"field": name, "address": key, "error": "array shape differs"})
            continue
        if bounds and a == b and not (exact_ext and typ == "EXT"):
            if typ not in {"EXT", "DOUBLE", "SINGLE"}:
                continue
            try:
                if all(map(finite, a)):
                    continue
            except TypeError:
                pass
        for index, (p, q) in enumerate(pairs):
            floating = typ in {"EXT", "DOUBLE", "SINGLE"}
            if not finite(p) and not finite(q) and (key, index) in allowed_nonfinite:
                continue
            valid = not floating or finite(p) and finite(q)
            same = valid and (close_float(p, q, rel_tol, abs_tol, exact_ext and typ == "EXT") if floating else p == q)
            if same:
                continue
            count += 1
            if len(found) >= limit:
                continue
            item = {"field": name, "address": key, "rust": p, "original": q}
            if bounds:
                rem, subs = index, []
                for lo, hi in bounds:
                    subs.append(lo + rem % (hi - lo + 1))
                    rem //= hi - lo + 1
                item.update(index=subs, flat_index=index)
            if not valid:
                item["error"] = "non-finite or missing financial value"
            elif floating:
                with localcontext() as context:
                    context.prec = 80
                    pnum, qnum = decimal_value(p), decimal_value(q)
                    item["absolute_error"] = str(abs(pnum - qnum))
                    item["relative_error"] = str(abs(pnum - qnum) / max(abs(pnum), abs(qnum), Decimal("1e-4932")))
            found.append(item)
    return count, found


def initial_nonfinite(initial, layout):
    """An explicit baseline exemption never permits a newly invalid cell elsewhere."""
    allowed = set()
    for key, _, typ, bounds in layout:
        if typ not in {"EXT", "DOUBLE", "SINGLE"}:
            continue
        values = initial[key] if bounds else [initial[key]]
        allowed.update((key, index) for index, value in enumerate(values) if not finite(value))
    return allowed


def retained_nonfinite(allowed, left, right):
    def cell(state, key, index):
        value = state[key]
        return value[index] if isinstance(value, list) else value
    return {(key, index) for key, index in allowed
            if not finite(cell(left, key, index)) and not finite(cell(right, key, index))}


def normalize_reports(events):
    out = []
    for e in events:
        if e["kind"] != "report":
            continue
        lines = list(e["lines"])
        while lines and not lines[-1].strip():
            lines.pop()
        out.append({"title": e["title"], "lines": lines})
    return out


def ui_differences(rust, original):
    # Custom DDT choices and report layout are different host presentations. Compare all common
    # prompts/replies and reports in per-kind order, with optional choices listed in the trace.
    result = []
    for kind in ("msgbox", "inputbox", "notice"):
        a = [e for e in rust if e["kind"] == kind]
        b = [e for e in original if e["kind"] == kind]
        if a != b:
            result.append({"kind": kind, "rust": a, "original": b})
    a, b = normalize_reports(rust), normalize_reports(original)
    if a != b:
        result.append({"kind": "reports", "rust": a, "original": b})
    return result


def file_differences(rust, original):
    result = []
    for name in sorted((rust.keys() | original.keys()) - TRANSIENT_FILES):
        a, b = rust.get(name), original.get(name)
        if name in rust and name in original and a == b:
            continue
        item = {"name": name, "rust": None if a is None else hashlib.sha256(a).hexdigest(),
                "original": None if b is None else hashlib.sha256(b).hexdigest(),
                "rust_present": name in rust, "original_present": name in original}
        if a is not None and b is not None:
            first = next((i for i, (x, y) in enumerate(zip(a, b)) if x != y), min(len(a), len(b)))
            item.update(first_byte_difference=first, rust_size=len(a), original_size=len(b))
        result.append(item)
    return result


def expand_steps(steps):
    for step in steps:
        if not isinstance(step, dict) or step.get("op") not in OPS:
            raise ValueError(f"Unsupported operation: {step!r}")
        if set(step) - {"op", "arg", "mode", "answers", "repeat", "text"}:
            raise ValueError("Unsupported command fields")
        repeat = step.get("repeat", 1)
        if not isinstance(repeat, int) or isinstance(repeat, bool) or repeat < 1:
            raise ValueError("repeat must be a positive integer")
        if not isinstance(step.get("answers", []), list) or not all(isinstance(a, str) for a in step.get("answers", [])):
            raise ValueError("answers must be strings")
        if "text" in step and not isinstance(step["text"], str):
            raise ValueError("text must be a string")
        if any(k in step and (not isinstance(step[k], int) or isinstance(step[k], bool)) for k in ("arg", "mode")):
            raise ValueError("arg and mode must be integers")
        command = {k: v for k, v in step.items() if k != "repeat"}
        if command["op"] == "select" and not 11 <= command.get("arg", 0) <= 1600:
            raise ValueError("select requires arg 11..1600")
        if command["op"] == "trade" and not (1 <= command.get("mode", 0) <= 7 and 1 <= command.get("arg", 0) <= 16):
            raise ValueError("trade requires mode 1..7 and arg (button) 1..16")
        if command["op"] == "clock" and command.get("arg") not in (0, 1):
            raise ValueError("clock arg must be 0 or 1")
        for _ in range(repeat):
            yield command.copy()


class Native:
    def __init__(self, binary, initial, assets, output, timeout):
        self.output, self.timeout = Path(output), timeout
        self.output.mkdir()
        self.stderr = (self.output / "stderr.log").open("w")
        self.p = subprocess.Popen([str(binary), "serve", str(initial), str(assets), str(self.output)],
                                  stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.stderr,
                                  text=True, bufsize=1)
        self.selector = selectors.DefaultSelector()
        self.selector.register(self.p.stdout, selectors.EVENT_READ)

    def command(self, request):
        self.p.stdin.write(json.dumps(request, ensure_ascii=True) + "\n")
        self.p.stdin.flush()
        if not self.selector.select(self.timeout):
            raise TimeoutError(f"Rust command timed out after {self.timeout}s")
        line = self.p.stdout.readline()
        if not line:
            raise RuntimeError(f"Rust peer exited: {(self.output / 'stderr.log').read_text()[-4000:]}")
        return json.loads(line)

    def state(self):
        return json.loads((self.output / "current.json").read_text())

    def close(self):
        self.p.stdin.close()
        try:
            self.p.wait(timeout=2)
        except subprocess.TimeoutExpired:
            self.p.kill()
            self.p.wait()
        self.selector.close()
        self.stderr.close()


def original_peer(game, initial, files, timeout, hooks):
    class Peer(game.Emu):
        def api(self, name, args, ret):
            if name == "CreateProcessA":
                cmd = self.cstr(args[1]).decode("latin1") if args[1] else ""
                if re.search(r"(?:^|[\\/])qm\.exe(?: |$)", cmd, re.I):
                    data = self.vfs.lookup("TMP_QM.DAT")
                    if data is not None:
                        text = bytes(data).decode("latin1").rstrip("\r\n").replace("@@", "\r\n\r\n").replace("##", "\r\n")
                        self.events.append({"kind": "notice", "text": text})
            value = super().api(name, args, ret)
            if name == "MessageBoxA":
                self.events.append({"kind": "msgbox", "title": self.cstr(args[2]).decode("latin1"),
                                    "text": self.cstr(args[1]).decode("latin1"), "style": args[3], "reply": value})
            elif name == "DialogBoxIndirectParamA" and args[3] == game.INPUTBOX_DLGPROC:
                self.events.append({"kind": "inputbox", "reply": self.read_bstr(value).decode("latin1")})
            return value
    emu = Peer(timeout=int(timeout * 1e6), files=files, default_files=False, hooks=hooks)
    emu.events = []
    emu.write_state(initial)  # The only whole-state write in the original trajectory.
    return emu


def original_command(game, emu, command, now):
    emu.log, emu.events, emu.warnings = [], [], []
    emu.vfs.changed.clear()
    emu.local_time = (now.year, now.month, (now.weekday() + 1) % 7, now.day,
                      now.hour, now.minute, now.second, now.microsecond // 1000)
    before_dialogs = len(emu.wm.order)
    op, arg = command["op"], command.get("arg", 0)
    address, args = {
        "state": (game.SENTINEL, []), "tick": (0x41ef3f, [1]), "entity": (0x4c6bc1, []),
        "end_turn": (0x424fd6, []), "select": (0x4227d5, []), "trade": (0x552f0b, [arg]),
        "pause": (0x46e001, [("f", arg)]), "save": (0x40c01b, [0]), "reload": (0x40ae7f, []),
        "clock": (game.SENTINEL, []),
        "notice": (0x46e851, [("s", byte_text(command.get("text", "")))]),
        "interest_rates": (0x45c781, []),
    }[op]
    mapping = {name: key for key, name, _, _ in game.layout()}
    if op == "select":
        emu.write_state({mapping["TargetCorp"]: arg})
    if op == "trade":
        emu.write_state({mapping["ActionMenuMode"]: command["mode"]})
    if op == "clock":
        emu.write_state({mapping["ClockState"]: "RUN" if arg else "STOP", mapping["TickerOn"]: arg,
                         mapping["TickerStopReq"]: "N"})
    error, regs, _ = emu.call(address, args)
    for window in emu.wm.models()[before_dialogs:]:
        control = next((c for c in window["controls"] if c["id"] == 301 and c["class"].upper() == b"LISTBOX"), None)
        if control:
            emu.events.append({"kind": "report", "title": window["title"].decode("latin1"),
                               "lines": [line.decode("latin1") for line in control["items"]]})
    return error, regs


def extra_ext_bits(game, emu, diffs):
    layout = {key: (typ, bounds) for key, _, typ, bounds in game.layout()}
    for item in diffs:
        typ, bounds = layout[item["address"]]
        if typ != "EXT":
            continue
        ptr = int(item["address"], 16)
        if bounds:
            ptr = emu.u32(ptr) + 10 * item.get("flat_index", 0)
        item["original_ext80"] = emu.rd(ptr, 10)[::-1].hex()


def run(options):
    import game
    now0 = dt.datetime.fromisoformat(options.clock)
    assets = Path(options.assets).resolve()
    save = Path(options.save).resolve() if options.save else None
    snapshot = Path(options.snapshot).resolve() if options.snapshot else None
    if not Path(game.DEFAULT_EXE).is_file():
        raise ValueError(f"Original executable missing: {game.DEFAULT_EXE}; extract the owner's installer first")
    if not assets.is_dir() or not (save or snapshot):
        raise ValueError("Provide --save or --snapshot and an extracted asset directory")
    if not (save or snapshot).is_file():
        raise ValueError(f"Input file does not exist: {save or snapshot}")
    data = read_assets(assets)
    plan = getattr(options, "replay_plan", None) or (json.loads(Path(options.plan).read_text()) if options.plan else {"version": 1, "steps": [{"op": "tick", "repeat": options.steps}]})
    if not isinstance(plan, dict) or plan.get("version") != 1:
        raise ValueError("Unsupported plan version")
    if not isinstance(plan.get("steps"), list) or not plan["steps"]:
        raise ValueError("Plan needs a nonempty steps list")
    commands = list(expand_steps(plan["steps"]))
    binary = Path(options.driver).resolve() if options.driver else ROOT / "engine/target/debug/examples/campaign_driver"
    if not options.driver:
        subprocess.run(["cargo", "build", "--manifest-path", str(ROOT / "engine/Cargo.toml"), "-p", "wsr-wasm",
                        "--example", "campaign_driver", "--profile", "test"], check=True)
    if not binary.is_file():
        raise ValueError(f"Rust driver missing: {binary}")
    output = Path(options.out or ROOT / "engine/target/campaign" / dt.datetime.now().strftime("%Y%m%d-%H%M%S-%f")).resolve()
    if output.exists() and any(output.iterdir()):
        raise ValueError(f"Output directory is not empty: {output}")
    output.mkdir(parents=True, exist_ok=True)
    initial_path = output / "initial.json"
    if save:
        prepared = output / "prepared"
        subprocess.run([str(binary), "prepare", str(save), str(assets), str(prepared)], check=True)
        initial = json.loads((prepared / "current.json").read_text())
        # Slot selection belongs to the host and is absent from GAMExx.DAT itself.
        match = re.fullmatch(r"GAME(\d+)\.DAT", save.name, re.I)
        names = {name: key for key, name, _, _ in game.layout()}
        initial[names["GameNumber"]] = int(match[1]) if match and 1 <= int(match[1]) <= 50 else 1
    else:
        initial = json.loads(snapshot.read_text())
    names = {name: key for key, name, _, _ in game.layout()}
    for name, value in plan.get("initial_overrides", {}).items():
        if name not in names:
            raise ValueError(f"Unknown initial field: {name}")
        initial[names[name]] = value
    dump(initial_path, initial)
    if save:
        slot = initial[names["GameNumber"]]
        data[f"GAME{slot:02}.DAT"] = save.read_bytes()
        data["LASTGAME.DAT"] = f"{slot}\r\n".encode("ascii")
    # Retain the exact initial directory, including the selected save slot, for both peers and
    # replay. Subsequent file changes stay independent in each peer's own virtual directory.
    common_files = output / "initial-files"
    common_files.mkdir()
    for name, content in data.items():
        (common_files / name).write_bytes(content)
    provenance = {"version": 1, "save": str(save) if save else None, "snapshot": str(snapshot) if snapshot else None,
                  "assets": str(common_files), "source_assets": str(assets), "plan": plan, "rng": options.rng,
                  "clock": options.clock, "step_ms": options.step_ms,
                  "quarters": options.quarters, "until_game_over": options.until_game_over,
                  "rel_tol": options.rel_tol, "abs_tol": options.abs_tol, "exact_ext": options.exact_ext,
                  "hooks": options.hooks,
                  "original_sha256": sha(game.DEFAULT_EXE), "driver_sha256": sha(binary),
                  "replayed_from": options.replay,
                  "source_sha256": {str(p.relative_to(ROOT)): sha(p) for p in [Path(__file__), ROOT / "tools/emu/game.py", ROOT / "engine/wsr-wasm/examples/campaign_driver.rs", ROOT / "engine/wsr-core/src/state.rs"]},
                  "input_sha256": sha(save or snapshot), "initial_sha256": sha(initial_path),
                  "assets_sha256": {k: hashlib.sha256(v).hexdigest() for k, v in data.items()},
                  "ignored_windows_fields": [name for key, name, _, _ in game.layout() if key in UI_KEYS],
                  "ignored_transient_files": sorted(TRANSIENT_FILES), "skipped_routines": [],
                  "external_dlls": "existing emulator stubs; this is not a DLL parity test",
                  "initialization": "common loaded snapshot; loading the initial save is not compared"}
    baseline_nonfinite = initial_nonfinite(initial, game.layout())
    provenance["nonfinite_cells_in_input"] = sorted([list(cell) for cell in baseline_nonfinite])
    provenance["allow_initial_nonfinite"] = options.allow_initial_nonfinite
    dump(output / "run.json", provenance)
    if baseline_nonfinite and not options.allow_initial_nonfinite:
        dump(output / "result.json", {"status": "invalid-input", "compared_commands": 0,
             "error": "Input contains non-finite values; inspect run.json. --allow-initial-nonfinite is diagnostic only.",
             "nonfinite_cells": provenance["nonfinite_cells_in_input"]})
        print(f"invalid-input: {len(baseline_nonfinite)} non-finite cells; {output / 'result.json'}", flush=True)
        return 2
    if baseline_nonfinite:
        print(f"Diagnostic baseline: tolerating {len(baseline_nonfinite)} pre-existing non-finite cells; new invalid cells still fail", flush=True)
    native = Native(binary, initial_path, common_files, output / "rust", options.timeout)
    trace = (output / "trace.jsonl").open("w")
    def request(c, now):
        answers = c.get("answers", [])
        return {**{k: byte_text(v) if k == "text" else v for k, v in c.items() if k != "answers"}, "year": now.year, "month": now.month, "day": now.day, "hour": now.hour,
                "minute": now.minute, "second": now.second, "millis": now.microsecond // 1000,
                "answer_count": len(answers), **{f"answer{i}": byte_text(a) for i, a in enumerate(answers)}}
    compared, last_progress, t0 = 0, time.monotonic(), time.monotonic()
    result = {"status": "harness-error", "compared_commands": 0, "requested_commands": len(commands), "output": str(output), "initial_nonfinite_cells": len(baseline_nonfinite)}
    target = (initial[names["CurYear"]] * 4 + initial[names["CurQuarter"]] - 1 + options.quarters) if options.quarters else None
    result["target"] = {"quarters": options.quarters, "until_game_over": options.until_game_over}
    try:
        native.command(request({"op": "init", "rng": options.rng}, now0))
        emu = original_peer(game, initial, data, options.timeout, options.hooks)
        emu.w32(game.RNG_STATE, options.rng)
        before_rust, before_original = native.state(), emu.read_state()
        initial_count, initial_diffs = differences(before_rust, before_original, game.layout(), 0, 0,
            allowed_nonfinite=baseline_nonfinite if options.allow_initial_nonfinite else (), exact_ext=options.exact_ext)
        if initial_count:
            raise ValueError(f"Initial states do not match: {initial_diffs[:3]}")
        original_fingerprint = emu.fingerprint()
        live_nonfinite = set(baseline_nonfinite) if options.allow_initial_nonfinite else set()
        provenance["original_initial_warnings"] = list(emu.warnings)
        dump(output / "run.json", provenance)
        before_rng = (options.rng, options.rng)
        for number, command in enumerate(commands, 1):
            now = now0 + dt.timedelta(milliseconds=number * options.step_ms)
            result.update(current_command=number, command=command, clock=now.isoformat(timespec="milliseconds"))
            rust = native.command(request(command, now))
            if rust["unused"]:
                raise ValueError(f"Command {number} left scripted answers unused: {rust['unused']}")
            # Host replies, not game state, are translated into equivalent Win32 dialog actions.
            emu.answers = [byte_text(answer) for answer in rust["answers"]]
            error, regs = original_command(game, emu, command, now)
            rust_state = native.state()
            fingerprint = emu.fingerprint()
            changed_keys = {key for key, value in fingerprint.items() if original_fingerprint.get(key) != value}
            original_state = dict(before_original)
            original_state.update(emu.read_state(changed_keys))
            original_fingerprint = fingerprint
            count, diffs = differences(rust_state, original_state, game.layout(), options.rel_tol, options.abs_tol,
                                       allowed_nonfinite=live_nonfinite, exact_ext=options.exact_ext)
            extra_ext_bits(game, emu, diffs)
            rust_files = {f["name"]: None if f["deleted"] else (native.output / f["name"]).read_bytes() for f in rust["files"]}
            original_files = {game.vkey(k): v for k, v in emu.vfs.outputs().items()}
            files = file_differences(rust_files, original_files)
            ui = ui_differences(rust["events"], emu.events)
            runtime_errors = [line for line in emu.log if line.startswith("PBERR")]
            forced_dialogs = [line for line in emu.log if "FORCED END" in line]
            bad = error or runtime_errors or forced_dialogs or emu.warnings or count or files or ui or emu.answers or rust["rng"] != emu.u32(game.RNG_STATE)
            entry = {"command": number, "operation": command, "clock": now.isoformat(timespec="milliseconds"),
                     "rust_rng": rust["rng"], "original_rng": emu.u32(game.RNG_STATE), "year": rust["year"],
                     "quarter": rust["quarter"], "position": rust["position"], "state_differences": count,
                     "ui_differences": len(ui), "file_differences": len(files), "runtime_errors": runtime_errors,
                     "original_seconds": emu.elapsed}
            trace.write(json.dumps(entry) + "\n"); trace.flush()
            if bad:
                dump(output / "before-rust.json", before_rust); dump(output / "before-original.json", before_original)
                dump(output / "after-rust.json", rust_state); dump(output / "after-original.json", original_state)
                dump(output / "rust-ui.json", rust["events"]); dump(output / "original-ui.json", emu.events)
                dump(output / "original-dialogs.json", json_model(emu.wm.models()))
                (output / "original.log").write_text("\n".join(emu.log) + "\n")
                (output / "original-files").mkdir()
                for name, content in original_files.items():
                    if content is not None:
                        (output / "original-files" / name).write_bytes(content)
                result.update(status="runtime-error" if runtime_errors else "harness-error" if error or forced_dialogs or emu.warnings else "invalid-state" if any(d.get("error", "").startswith("non-finite") for d in diffs) else "diverged",
                              first_divergence=number, command=command, clock=entry["clock"],
                              previous_rng={"rust": before_rng[0], "original": before_rng[1]},
                              rng={"rust": rust["rng"], "original": entry["original_rng"]},
                              state_difference_count=count, state_differences=diffs, file_differences=files,
                              ui_differences=ui, runtime_errors=runtime_errors, error=error,
                              unused_original_answers=emu.answers, forced_dialogs=forced_dialogs, registers=regs, warnings=emu.warnings)
                break
            compared = number
            live_nonfinite = retained_nonfinite(live_nonfinite, rust_state, original_state)
            before_rust, before_original = rust_state, original_state
            before_rng = (rust["rng"], emu.u32(game.RNG_STATE))
            if ((options.until_game_over and rust["game_over"]) or
                (target is not None and rust["year"] * 4 + rust["quarter"] - 1 >= target)):
                result.update(status="passed", termination="target-reached")
                break
            if time.monotonic() - last_progress >= 10 or number == len(commands):
                print(f"Compared {number}/{len(commands)} commands: {rust['year']} Q{rust['quarter']} position {rust['position']}", flush=True)
                last_progress = time.monotonic()
        else:
            result.update(status="incomplete" if target is not None or options.until_game_over else "passed",
                          termination="commands-exhausted")
    except Exception as e:
        result.update(status="harness-error", error=str(e))
    finally:
        result.update(compared_commands=compared, elapsed_seconds=round(time.monotonic() - t0, 3))
        dump(output / "result.json", result)
        trace.close(); native.close()
    print(f"{result['status']}: {compared}/{len(commands)} commands matched; {output / 'result.json'}", flush=True)
    if result.get("error"):
        print(str(result["error"])[:4000], file=sys.stderr)
    if result.get("state_differences"):
        for d in result["state_differences"][:8]:
            print(f"  {d['field']}{d.get('index', '')}: {d.get('rust')} vs {d.get('original')}", flush=True)
    return 0 if result["status"] == "passed" else 1


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__)
    source = ap.add_mutually_exclusive_group(required=True)
    source.add_argument("--save"); source.add_argument("--snapshot"); source.add_argument("--replay", help="Replay a previous run.json from its retained initial snapshot")
    ap.add_argument("--assets", default=str(ROOT / "extracted/app")); ap.add_argument("--plan")
    ap.add_argument("--steps", type=int, default=64); ap.add_argument("--rng", type=lambda s: int(s, 0), default=123456)
    target = ap.add_mutually_exclusive_group()
    target.add_argument("--quarters", type=int, help="Require this many simulated quarters before the command limit")
    target.add_argument("--until-game-over", action="store_true", help="Require both engines to reach game over before the command limit")
    ap.add_argument("--clock", default="2000-01-03T10:00:00"); ap.add_argument("--step-ms", type=int, default=2000)
    ap.add_argument("--rel-tol", type=float, default=1e-9); ap.add_argument("--abs-tol", type=float, default=1e-9)
    ap.add_argument("--exact-ext", action="store_true", help="Require exact x87 EXT values as well as exact integer and saved-file values")
    ap.add_argument("--allow-initial-nonfinite", action="store_true", help="Diagnostic: tolerate only invalid cells already present in the input")
    ap.add_argument("--timeout", type=float, default=120); ap.add_argument("--out"); ap.add_argument("--driver")
    ap.add_argument("--hook", dest="hooks", action="append", type=lambda s: int(s, 16), default=[])
    o = ap.parse_args(argv)
    if o.replay:
        manifest_path = Path(o.replay).resolve()
        manifest = json.loads(manifest_path.read_text())
        initial = manifest_path.parent / "initial.json"
        if sha(initial) != manifest["initial_sha256"]:
            ap.error("Retained initial snapshot has changed")
        files = {p.name.upper(): p for p in Path(manifest["assets"]).iterdir() if p.is_file()}
        for name, digest in manifest["assets_sha256"].items():
            if name not in files or sha(files[name]) != digest:
                ap.error(f"Replay asset has changed: {name}")
        original_path = ROOT / "extracted/app/WSR.EXE"
        if sha(original_path) != manifest["original_sha256"]:
            ap.error("Original executable has changed")
        o.snapshot, o.save, o.replay_plan = str(initial), None, manifest["plan"]
        for name in ("assets", "rng", "clock", "step_ms", "rel_tol", "abs_tol", "exact_ext", "allow_initial_nonfinite", "hooks", "quarters", "until_game_over"):
            setattr(o, name, manifest.get(name, [] if name == "hooks" else getattr(o, name)))
    if not 0 <= o.rng <= 0xffffffff or o.steps < 1 or o.step_ms < 0 or o.timeout <= 0:
        ap.error("rng must be a u32, steps positive, step-ms nonnegative, timeout positive")
    if o.quarters is not None and o.quarters < 1:
        ap.error("quarters must be positive")
    if not all(math.isfinite(x) and x >= 0 for x in (o.rel_tol, o.abs_tol)):
        ap.error("tolerances must be finite and nonnegative")
    try:
        return run(o)
    except (ValueError, OSError, ImportError, subprocess.CalledProcessError) as e:
        print(f"harness-error: {e}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
