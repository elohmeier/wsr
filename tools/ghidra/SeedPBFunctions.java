// Seed functions at every PowerBASIC SUB/FUNCTION prologue (push ebp; mov ebp,esp; push ebx; push esi; push edi)
// that auto-analysis missed, then let analysis follow the new code. Repeats until nothing new is found.
import ghidra.app.script.GhidraScript;
import ghidra.app.cmd.disassemble.DisassembleCommand;
import ghidra.program.model.address.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.mem.*;

public class SeedPBFunctions extends GhidraScript {
    static final byte[] PRO = { 0x55, (byte) 0x8b, (byte) 0xec, 0x53, 0x56, 0x57 };

    @Override
    public void run() throws Exception {
        MemoryBlock text = currentProgram.getMemory().getBlock(".text");
        Listing listing = currentProgram.getListing();
        for (int pass = 1; pass <= 5; pass++) {
            int added = 0;
            Address a = text.getStart();
            while (a != null && a.compareTo(text.getEnd()) < 0) {
                a = currentProgram.getMemory().findBytes(a, text.getEnd(), PRO, null, true, monitor);
                if (a == null) break;
                if (listing.getFunctionAt(a) == null && listing.getInstructionContaining(a) == null) {
                    new DisassembleCommand(a, null, true).applyTo(currentProgram, monitor);
                    if (createFunction(a, null) != null) added++;
                }
                a = a.add(1);
            }
            analyzeChanges(currentProgram);
            println("pass " + pass + ": seeded " + added + ", total functions " + currentProgram.getFunctionManager().getFunctionCount());
            if (added == 0) break;
        }
    }
}
