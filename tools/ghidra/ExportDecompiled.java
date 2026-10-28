// Headless Ghidra script: decompile every function and write C + an index.
// Usage: analyzeHeadless <proj> <name> -import WSR.EXE -postScript ExportDecompiled.java <outdir>
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.app.decompiler.parallel.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.*;
import ghidra.program.model.address.*;
import ghidra.program.model.data.StringDataInstance;
import ghidra.util.task.TaskMonitor;
import generic.concurrent.QCallback;
import java.io.*;
import java.nio.file.*;
import java.util.*;
import java.util.stream.*;

public class ExportDecompiled extends GhidraScript {
    @Override
    public void run() throws Exception {
        Path out = Paths.get(getScriptArgs()[0]);
        Files.createDirectories(out.resolve("c"));
        Listing listing = currentProgram.getListing();
        List<Function> funcs = new ArrayList<>();
        listing.getFunctions(true).forEach(f -> { if (!f.getName().startsWith("shim_")) funcs.add(f); });
        println("functions: " + funcs.size());

        DecompileConfigurer cfg = d -> {
            DecompileOptions o = new DecompileOptions();
            o.grabFromProgram(currentProgram);
            d.setOptions(o);
            d.toggleCCode(true);
            d.toggleSyntaxTree(false);
            d.setSimplificationStyle("decompile");
        };
        List<DecompInterface> all = Collections.synchronizedList(new ArrayList<>());
        ThreadLocal<DecompInterface> tl = ThreadLocal.withInitial(() -> {
            DecompInterface d = new DecompInterface();
            cfg.configure(d);
            d.openProgram(currentProgram);
            all.add(d);
            return d;
        });
        QCallback<Function, String[]> cb = (f, m) -> {
            DecompileResults r = tl.get().decompileFunction(f, 300, m);
            String c = r.decompileCompleted() ? r.getDecompiledFunction().getC()
                : "/* DECOMPILE FAILED: " + r.getErrorMessage() + " */\n";
            return new String[] { f.getEntryPoint().toString(), c };
        };
        // Return addresses of every CALL: statements storing one of these into a stack slot are artifacts
        // of the decompiler aliasing the return-address push with a local/array; they are dropped below.
        Set<Long> retAddrs = new HashSet<>();
        for (Instruction i : listing.getInstructions(true))
            if (i.getFlowType().isCall()) retAddrs.add(i.getFallThrough() == null ? -1L : i.getFallThrough().getOffset());
        java.util.regex.Pattern artifact = java.util.regex.Pattern.compile("^\\s*[\\w\\[\\].>-]+ = (\\([^()]*\\))?0x([0-9a-f]+);$");
        long[] dropped = {0};
        List<String[]> results = ParallelDecompiler.decompileFunctions(cb, funcs, monitor);
        all.forEach(DecompInterface::dispose);
        Map<String, String> code = new HashMap<>();
        for (String[] r : results) {
            StringBuilder sb = new StringBuilder();
            for (String line : r[1].split("\n", -1)) {
                java.util.regex.Matcher m = artifact.matcher(line);
                if (m.matches() && retAddrs.contains(Long.parseLong(m.group(2), 16))) { dropped[0]++; continue; }
                // x87 shims (PBAnnotate): inlining notes and the return address of the shim's inner call
                if (line.startsWith("/* WARNING: Inlined function: shim_")) continue;
                if (line.matches("\\s*\\w+ = (\\([\\w *]+\\))?\\(?shim_\\w+ \\+ \\d+\\)?;")) { dropped[0]++; continue; }
                sb.append(line).append('\n');
            }
            code.put(r[0], sb.toString());
        }
        println("dropped return-address artifacts: " + dropped[0]);

        // Index: address, name, size, #callers, #callees, referenced strings
        try (PrintWriter idx = new PrintWriter(Files.newBufferedWriter(out.resolve("functions.tsv")))) {
            idx.println("addr\tname\tsize\tcallers\tcallees\tstrings");
            ReferenceManager rm = currentProgram.getReferenceManager();
            int chunk = 0, inChunk = 0;
            PrintWriter cw = null;
            for (Function f : funcs) {
                if (cw == null || inChunk >= 200) {
                    if (cw != null) cw.close();
                    cw = new PrintWriter(Files.newBufferedWriter(out.resolve(String.format("c/part%03d.c", chunk++))));
                    inChunk = 0;
                }
                Set<String> strs = new LinkedHashSet<>();
                for (Address a : f.getBody().getAddresses(true)) {
                    for (Reference ref : rm.getReferencesFrom(a)) {
                        Data dd = listing.getDataAt(ref.getToAddress());
                        if (dd != null && dd.hasStringValue()) {
                            String s = String.valueOf(dd.getValue()).replaceAll("[\\t\\r\\n]", " ");
                            if (s.length() > 2) strs.add(s.length() > 80 ? s.substring(0, 80) : s);
                        }
                    }
                }
                idx.printf("%s\t%s\t%d\t%d\t%d\t%s%n", f.getEntryPoint(), f.getName(), f.getBody().getNumAddresses(),
                    f.getCallingFunctions(monitor).size(), f.getCalledFunctions(monitor).size(),
                    strs.stream().limit(8).collect(Collectors.joining(" | ")));
                cw.printf("// ==== %s %s (size %d)%n", f.getEntryPoint(), f.getName(), f.getBody().getNumAddresses());
                cw.println(code.getOrDefault(f.getEntryPoint().toString(), "/* missing */"));
                inChunk++;
            }
            if (cw != null) cw.close();
        }
        println("done");
    }
}
