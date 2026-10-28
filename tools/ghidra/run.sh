#!/usr/bin/env bash
# Decompile WSR.EXE with Ghidra headless: fix .data layout, seed PB functions, annotate runtime, export C.
# Usage: tools/ghidra/run.sh [path/to/WSR.EXE] [outdir]
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
exe=$(realpath "${1:-$root/extracted/app/WSR.EXE}")
out=$(realpath -m "${2:-$root/decompiled}" 2>/dev/null || echo "${2:-$root/decompiled}")
mkdir -p "$out" "$root/ghidra-proj"
rm -rf "$out/c" "$out/functions.tsv"
export JAVA_HOME=${JAVA_HOME:-/opt/homebrew/opt/openjdk@25/libexec/openjdk.jdk/Contents/Home}
export GHIDRA_HEADLESS_MAXMEM=${GHIDRA_HEADLESS_MAXMEM:-24G}
"${GHIDRA_HEADLESS:-/opt/homebrew/opt/ghidra/libexec/support/analyzeHeadless}" \
  "$root/ghidra-proj" wsr -import "$exe" -overwrite -analysisTimeoutPerFile 7200 \
  -scriptPath "$root/tools/ghidra" \
  -preScript FixPBData.java \
  -postScript SeedPBFunctions.java \
  -postScript PBAnnotate.java \
  > "$out/ghidra.log" 2>&1
# Export in a second session: the decompiler only picks up the calling-convention spec extensions
# added by PBAnnotate after the program is reopened.
"${GHIDRA_HEADLESS:-/opt/homebrew/opt/ghidra/libexec/support/analyzeHeadless}" \
  "$root/ghidra-proj" wsr -process WSR.EXE -noanalysis \
  -scriptPath "$root/tools/ghidra" \
  -postScript PBOverrides.java \
  -postScript ExportDecompiled.java "$out" \
  >> "$out/ghidra.log" 2>&1
grep -E 'ERROR|moved|patched|pass |purge fixed|overrides|RET N|pool block|prefixed|functions:|done' "$out/ghidra.log" | grep -v VarnodeContext || true
