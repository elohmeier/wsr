// Pre-analysis fix for WSR.EXE (PowerBASIC 8): reproduce the runtime's start-up relocation of .data.
// The first thing the PB runtime does (0x6b72ee, called from entry) is a backward "rep movsb" that moves
// the data image up inside .data, using a header at the start of the section:
//   count = [.data+0x18] + [.data+0x20], src = .data+0x28, dst = .data + [.data+0x1c]
// For WSR.EXE that is 0x9b4a8 bytes from 0x6c3028 to 0x6c3f18 (a shift of 0xef0), ending exactly at the
// section's VirtualSize. Below dst the runtime keeps its own state and zero-initialised variables.
// Without this every global and every pooled literal/constant Ghidra shows is wrong (checked: all 15,594
// distinct string-literal references and the numeric constant pool line up after the move).
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.mem.*;

public class FixPBData extends GhidraScript {
    static final long DATA = 0x6c3000L;
    static final long POOL_FILE = 0x6c7834L; // first literal length field, as mapped from the file

    @Override
    public void run() throws Exception {
        Memory mem = currentProgram.getMemory();
        MemoryBlock data = mem.getBlock(toAddr(DATA));
        long end = DATA + data.getSize();
        // .data's raw size is smaller than its VirtualSize; make the whole range one initialized block.
        for (MemoryBlock b : mem.getBlocks()) {
            if (b.getEnd().getOffset() < DATA || b.getStart().getOffset() >= end) continue;
            if (!b.isInitialized()) mem.convertToInitialized(b, (byte) 0);
        }
        MemoryBlock next;
        while (data.getEnd().getOffset() < end - 1 && (next = mem.getBlock(data.getEnd().add(1))) != null) {
            data = mem.join(data, next);
        }
        long src = DATA + 0x28, dst = DATA + mem.getInt(toAddr(DATA + 0x1c));
        int count = mem.getInt(toAddr(DATA + 0x18)) + mem.getInt(toAddr(DATA + 0x20));
        // The block is page-rounded; the moved image ends at the section's VirtualSize (0x75f3c0).
        if (dst + count > end || end - (dst + count) >= 0x1000) throw new Exception(String.format("unexpected .data header: dst %#x count %#x end %#x", dst, count, end));
        byte[] buf = new byte[count];
        mem.getBytes(toAddr(src), buf);
        mem.setBytes(toAddr(dst), buf);
        // Runtime state and BSS below dst: zero it rather than leave stale pre-move bytes.
        mem.setBytes(toAddr(src), new byte[(int) (dst - src)]);
        long shift = dst - src;
        println(String.format("moved %#x bytes from %#x to %#x (shift %#x); block %s %s-%s", count, src, dst, shift,
            data.getName(), data.getStart(), data.getEnd()));
        patchLocalInitLoops(mem);
        // The literal/constant pool is never written (verified by scanning all absolute stores), so make it
        // read-only: the decompiler then inlines string literals and folds numeric constants.
        // Everything between dst and the pool holds globals and array descriptors and stays writable.
        Address poolStart = toAddr(POOL_FILE + shift);
        mem.split(data, poolStart);
        MemoryBlock pool = mem.getBlock(poolStart);
        pool.setName(".pbpool");
        pool.setWrite(false);
        println("pool block " + pool.getStart() + "-" + pool.getEnd() + " read-only");
    }

    // PB prologues zero-init locals with "mov ecx,N; push esi; dec ecx; jne $-4". Ghidra cannot track
    // ESP through that loop, which turns every later return-address push into a fake local store.
    // Replace it (in the Ghidra program only) with the stack-equivalent "sub esp,N*4; nop; nop; nop".
    // Note: this drops the zero-initialisation of locals from the decompiled view (PB zeroes all locals).
    void patchLocalInitLoops(Memory mem) throws Exception {
        MemoryBlock text = mem.getBlock(".text");
        byte[] tail = { 0x56, 0x49, 0x75, (byte) 0xfc };
        int patched = 0;
        Address a = text.getStart();
        while ((a = mem.findBytes(a, text.getEnd(), tail, null, true, monitor)) != null) {
            Address mov = a.subtract(5);
            if ((mem.getByte(mov) & 0xff) == 0xb9) {
                int count = mem.getInt(mov.add(1));
                byte[] repl = new byte[9];
                repl[0] = (byte) 0x81; repl[1] = (byte) 0xec;
                int sz = count * 4;
                for (int i = 0; i < 4; i++) repl[2 + i] = (byte) (sz >>> (8 * i));
                repl[6] = repl[7] = repl[8] = (byte) 0x90;
                mem.setBytes(mov, repl);
                patched++;
            }
            a = a.add(4);
        }
        println("patched local-init loops: " + patched);
    }
}
