// Second-session pass: per-call-site signature overrides for 2-D PB_ArrayElem calls (mov edx,2).
// Runs after PBAnnotate in a fresh session so the __pb_arr2 spec extension is known to the data types.
import ghidra.app.script.GhidraScript;
import ghidra.program.model.data.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.*;
import java.util.*;

public class PBOverrides extends GhidraScript {
    // DIM: push lo1, hi1, ..., lo_n, hi_n, elemsize, typeword (ndims<<24 | type<<16 | flags), dataptr;
    // mov ebx, desc; call PB_DimArray. Read ndims from the typeword push and give the call n-dim arguments.
    int dimArray(FunctionManager fm) throws Exception {
        Function f = fm.getFunctionAt(toAddr(0x6b79edL));
        DataType vp = new PointerDataType(VoidDataType.dataType);
        int n = 0;
        for (Reference r : currentProgram.getReferenceManager().getReferencesTo(f.getEntryPoint())) {
            Instruction call = getInstructionAt(r.getFromAddress());
            if (call == null) continue;
            Instruction i = call.getPrevious();
            if (i != null && i.getMnemonicString().equals("MOV") && i.getDefaultOperandRepresentation(0).equals("EBX")) i = i.getPrevious();
            int pushes = 0;
            long typeword = -1;
            while (i != null && i.getMnemonicString().equals("PUSH") && pushes < 2) {
                if (++pushes == 2 && i.getScalar(0) != null) typeword = i.getScalar(0).getUnsignedValue();
                i = i.getPrevious();
            }
            int nd = (int) (typeword >>> 24);
            if (typeword < 0 || nd < 1 || nd > 4) continue;
            FunctionDefinitionDataType d = new FunctionDefinitionDataType(CategoryPath.ROOT, "PB_DimArray" + nd, currentProgram.getDataTypeManager());
            List<ParameterDefinition> ps = new ArrayList<>();
            ps.add(new ParameterDefinitionImpl("desc", vp, null));
            ps.add(new ParameterDefinitionImpl("dataptr", vp, null));
            ps.add(new ParameterDefinitionImpl("typeword", UnsignedIntegerDataType.dataType, null));
            ps.add(new ParameterDefinitionImpl("elemsize", IntegerDataType.dataType, null));
            for (int k = nd; k >= 1; k--) {
                ps.add(new ParameterDefinitionImpl("hi" + k, IntegerDataType.dataType, null));
                ps.add(new ParameterDefinitionImpl("lo" + k, IntegerDataType.dataType, null));
            }
            d.setArguments(ps.toArray(new ParameterDefinition[0]));
            d.setReturnType(VoidDataType.dataType);
            d.setCallingConvention("__pb_dim" + nd);
            Function caller = fm.getFunctionContaining(call.getAddress());
            if (caller == null) continue;
            ghidra.program.model.pcode.HighFunctionDBUtil.writeOverride(caller, call.getAddress(), d);
            n++;
        }
        return n;
    }

    @Override
    public void run() throws Exception {
        FunctionManager fm = currentProgram.getFunctionManager();
        Function f = fm.getFunctionAt(toAddr(0x6bc34aL));
        DataType vp = new PointerDataType(VoidDataType.dataType);
        FunctionDefinitionDataType two = new FunctionDefinitionDataType(CategoryPath.ROOT, "PB_ArrayElem2", currentProgram.getDataTypeManager());
        two.setReturnType(vp);
        two.setArguments(new ParameterDefinition[] { new ParameterDefinitionImpl("desc", vp, null),
            new ParameterDefinitionImpl("ndims", IntegerDataType.dataType, null),
            new ParameterDefinitionImpl("j", IntegerDataType.dataType, null),
            new ParameterDefinitionImpl("i", IntegerDataType.dataType, null) });
        two.setCallingConvention("__pb_arr2");
        int n = 0;
        for (Reference r : currentProgram.getReferenceManager().getReferencesTo(f.getEntryPoint())) {
            Instruction call = getInstructionAt(r.getFromAddress());
            if (call == null) continue;
            Instruction prev = call.getPrevious();
            if (prev == null || !prev.getMnemonicString().equals("MOV") || !prev.getDefaultOperandRepresentation(0).equals("EDX")) continue;
            if (prev.getScalar(1) == null || prev.getScalar(1).getValue() != 2) continue;
            Function caller = fm.getFunctionContaining(call.getAddress());
            if (caller == null) continue;
            ghidra.program.model.pcode.HighFunctionDBUtil.writeOverride(caller, call.getAddress(), two);
            n++;
        }
        println("PB_ArrayElem 2-D call-site overrides: " + n);
        println("PB_DimArray call-site overrides: " + dimArray(fm));
    }
}
