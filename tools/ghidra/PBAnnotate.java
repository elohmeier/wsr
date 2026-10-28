// Annotate the PowerBASIC 8 runtime in WSR.EXE: names and register-based signatures for the hot
// helpers, a pbrt_ prefix for the rest of the runtime, and string data for every pooled literal.
//
// PB codegen notes (see README "Decompilation"):
//  - Each SUB/FUNCTION keeps a string expression stack in its own frame: slots at [ebp-0x5c+i],
//    index at [ebp-0x78]. Helpers push/pop on it, so string dataflow is invisible to Ghidra.
//  - Helper args are passed in registers (EDX string, EBX target address, EAX/ECX ints, ST0 floats).
//  - [ebp-0x7f] bit 7 = pending error, [ebp-0x7c] = ON ERROR handler.
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.*;
import ghidra.program.model.data.*;
import ghidra.program.model.lang.Register;
import ghidra.program.model.listing.*;
import ghidra.program.model.listing.Function.FunctionUpdateType;
import ghidra.program.model.mem.*;
import ghidra.program.model.symbol.*;
import ghidra.program.database.SpecExtension;
import java.util.*;

public class PBAnnotate extends GhidraScript {
    static final long RT_START = 0x6b71beL, RT_END = 0x6c2135L;
    static final long POOL = 0x6c8724L, DATA_END = 0x75f3c0L;

    // addr, name, return ("reg:type" or ""), params ("reg:type:name" ..., STK = stack arg in push order),
    // stack purge, comment. str_pops/pushes are recorded in tools/pbbasic.py.
    static final String[][] HELPERS = {
        // string expression stack
        {"6bf0c0", "PB_StrPushLit", "", "EDX:char*:s", "0", "push string literal on expression stack"},
        {"6bf0ac", "PB_StrPushVar", "", "EDX:char*:s", "0", "push string variable (borrowed) on expression stack"},
        {"6bf0d4", "PB_StrPushTemp", "", "EAX:char*:s", "0", "push owned temp string (e.g. FUNCTION result)"},
        {"6bf0fb", "PB_StrPushEmpty", "", "", "0", "push \"\""},
        {"6be8f5", "PB_StrConcat", "", "", "0", "a$ + b$ (pops 2, pushes 1)"},
        {"6be7b9", "PB_StrPopAssign", "", "EBX:char**:dst", "0", "dst$ = pop()"},
        {"6be82c", "PB_StrPopTo", "", "EBX:char**:dst", "0", "pop into temp slot (BYVAL string arg)"},
        {"6be86b", "PB_StrPopToArg", "", "EBX:char**:slot", "0", "pop into temp slot and push it on the CPU stack (API arg)"},
        {"6bf5a3", "PB_StrPopFree", "", "", "0", "discard pop()"},
        {"6be966", "PB_StrEqual", "ZF:bool", "", "0", "a$ = b$ (pops b$, then a$); result in ZF (CF = a$ < b$)"},
        {"6be9a1", "PB_StrCmpVar", "ZF:bool", "STK:char*:a EDX:char*:b", "4", "a$ = b$ on two variables, no string stack; result in ZF"},
        {"6bef89", "PB_AsciizAssign", "", "EBX:char*:dst ECX:int:size", "0", "ASCIIZ dst = pop()"},
        {"6bf16f", "PB_StrVarPtrLen", "EBX:char*", "EBX:char**:var", "0", "STRPTR/LEN of a string variable (len in EDX)"},
        // errors
        {"6bccd7", "PB_ErrCheck", "", "", "0", "jump to ON ERROR handler if error pending"},
        {"6bccde", "PB_ErrCheckKeepFlags", "", "", "0", "as PB_ErrCheck, preserves flags"},
        {"6bf0e2", "PB_OnErrorSet", "", "EAX:void*:handler", "0", "ON ERROR GOTO (function prologue)"},
        {"6b7537", "PB_Err", "EAX:int", "", "0", "ERR"},
        {"6b72a8", "PB_Error", "", "EAX:int:code", "0", "ERROR code"},
        {"6bd6df", "PB_Resume", "", "EAX:int:mode", "0", "RESUME (0 = label, 2 = NEXT)"},
        {"6ba0c3", "PB_FuncName", "", "", "0", "push FUNCNAME$"},
        {"6b7406", "pbrt_RaiseError", "", "EAX:int:code", "0", "internal: set ERR = AL, return with CF"},
        {"6befdc", "PB_FrameRelease", "", "", "0", "free dynamic locals at SUB/FUNCTION exit"},
        {"6bccc6", "PB_StrFree", "", "EAX:char*:s", "0", "free temp string"},
        // string functions
        {"6bf094", "PB_Len", "EAX:int", "", "0", "LEN(pop())"},
        {"6bd709", "PB_Asc", "EAX:int", "", "0", "ASC(pop())"},
        {"6bd6bb", "PB_Val", "ST0:float10", "", "0", "VAL(pop())"},
        {"6bd503", "PB_StrNum", "", "ST0:float10:x", "0", "push STR$(x) (15 digits)"},
        {"6bd4ef", "PB_StrNum7", "", "ST0:float10:x", "0", "push STR$(x) (7 digits: INTEGER/SINGLE)"},
        {"6bd517", "PB_StrNumDigits", "", "ST0:float10:x EAX:int:digits", "0", "push STR$(x, digits)"},
        {"6bd4d0", "PB_FormatNum", "", "ST0:float10:x", "0", "push FORMAT$(x)"},
        {"6b9006", "PB_Format", "", "ST0:float10:x", "0", "push FORMAT$(x, pop())"},
        {"6bd16f", "PB_LTrim", "", "", "0", "LTRIM$"},
        {"6bd1c0", "PB_RTrim", "", "", "0", "RTRIM$"},
        {"6ba227", "PB_Right", "", "EAX:int:n", "0", "RIGHT$(pop(), n)"},
        {"6ba1c9", "PB_Mid", "", "ECX:int:start EAX:int:count", "0", "MID$(pop(), start, count)"},
        {"6ba15a", "PB_MidAssign", "", "EBX:char*:buf EDX:int:len ECX:int:start EAX:int:count", "0", "MID$(buf, start, count) = pop()"},
        {"6bd993", "PB_Parse", "", "EAX:int:n", "0", "PARSE$(s, d, n) (pops 2)"},
        {"6bd9e6", "PB_ParseAny", "", "EAX:int:n", "0", "PARSE$(s, ANY d, n) (pops 2)"},
        {"6bd90c", "PB_ParseCount", "EAX:int", "", "0", "PARSECOUNT(s, d) (pops 2)"},
        {"6bd935", "PB_ParseCountAny", "EAX:int", "", "0", "PARSECOUNT(s, ANY d) (pops 2)"},
        {"6b8be4", "PB_Instr", "EAX:int", "EAX:int:start", "0", "INSTR(start, s, find) (pops 2)"},
        {"6bd2ee", "PB_Replace", "", "", "0", "REPLACE a WITH b IN s (pops 3, pushes result)"},
        {"6bd44b", "PB_ReplaceAny", "", "", "0", "REPLACE ANY a WITH b IN s (pops 3, pushes result)"},
        {"6be36a", "PB_Remove", "", "", "0", "REMOVE$(s, sub) (pops 2)"},
        {"6be447", "PB_RemoveAny", "", "", "0", "REMOVE$(s, ANY chars) (pops 2)"},
        {"6bc210", "PB_StringFill", "", "EBX:int:n", "0", "STRING$(n, pop())"},
        {"6bc1e5", "PB_StringChr", "", "EBX:int:n EAX:int:code", "0", "push STRING$(n, code)"},
        {"6bc054", "PB_Space", "", "EAX:int:n", "0", "push SPACE$(n)"},
        {"6b9ea0", "PB_UCase", "", "EAX:int:mode", "0", "UCASE$"},
        {"6b9e5c", "PB_LCase", "", "EAX:int:mode", "0", "LCASE$"},
        {"6b9eef", "PB_MCase", "", "EAX:int:mode", "0", "MCASE$"},
        {"6b7766", "PB_VarArgCount", "", "EAX:int:x", "0", "vararg helper for CHR$(a, b, ...)"},
        {"6b76ff", "PB_Chr", "", "", "0", "push CHR$(args...)"},
        {"6b7973", "PB_Time", "", "", "0", "push TIME$"},
        {"6b7856", "PB_Date", "", "", "0", "push DATE$"},
        {"6bcda7", "PB_Command", "", "", "0", "push COMMAND$"},
        {"6b7d9f", "PB_Dir", "", "EAX:int:attr", "0", "DIR$(pop(), attr)"},
        {"6ba271", "PB_ChDir", "", "", "0", "CHDIR pop()"},
        {"6ba2de", "PB_ChDrive", "", "", "0", "CHDRIVE pop()"},
        {"6bcfc5", "PB_SetAttr", "", "EAX:int:attr", "0", "SETATTR pop(), attr"},
        {"6b8027", "PB_ReadStr", "", "EAX:int:n ECX:void*:table", "0", "push READ$(n) (DATA table)"},
        // numbers
        {"6bc3d7", "PB_Rnd", "ST0:float10", "", "0", "RND"},
        {"6bc3fe", "PB_RndInt", "EAX:int", "EAX:int:a ECX:int:b", "0", "RND(a, b)"},
        {"6bc3d0", "PB_Randomize", "", "ST0:float10:seed", "0", "RANDOMIZE seed"},
        {"6bd659", "PB_Timer", "ST0:float10", "", "0", "TIMER"},
        {"6bc01a", "PB_IntToFloat", "ST0:float10", "EAX:int:x", "0", ""},
        {"6bc021", "PB_UIntToFloat", "ST0:float10", "EAX:uint:x", "0", ""},
        {"6bc00f", "PB_ShortToFloat", "ST0:float10", "AX:short:x", "0", ""},
        {"6bcc93", "PB_CurLoad", "ST0:float10", "EBX:longlong*:p", "0", "load CURRENCY (int64 / 10000)"},
        {"6bcc9c", "PB_CuxLoad", "ST0:float10", "EBX:longlong*:p", "0", "load CURRENCYX (int64 / 100)"},
        {"6bcca5", "PB_CurStore", "", "ST0:float10:x EBX:longlong*:p", "0", "store CURRENCY (x * 10000)"},
        {"6bccae", "PB_CuxStore", "", "ST0:float10:x EBX:longlong*:p", "0", "store CURRENCYX (x * 100)"},
        {"6bcd29", "PB_Int", "ST0:float10", "ST0:float10:x", "0", "INT(x) (floor)"},
        {"6bcd07", "PB_Fix", "ST0:float10", "ST0:float10:x", "0", "FIX(x) (truncate)"},
        {"6bccf8", "PB_Ceil", "ST0:float10", "ST0:float10:x", "0", "CEIL(x)"},
        {"6bcd16", "PB_Frac", "ST0:float10", "ST0:float10:x", "0", "FRAC(x)"},
        {"6bcbb4", "PB_Sqr", "ST0:float10", "ST0:float10:x", "0", "SQR(x)"},
        {"6be6ef", "PB_FMod", "ST0:float10", "ST0:float10:x ST1:float10:y", "0", "x MOD y (floats)"},
        {"6bcabc", "PB_Pow", "ST0:float10", "ST1:float10:base ST0:float10:exp", "0", "base ^ exp"},
        {"6bea07", "PB_FpuReset", "", "", "0", "reload default FPU control word (after API calls)"},
        {"6be9fb", "PB_FpuSave", "", "EBX:int:count", "0", "spill x87 stack across a call"},
        {"6be9d4", "PB_FpuRestore", "", "EBX:int:count", "0", "reload spilled x87 values"},
        // arrays
        {"6bc34a", "PB_ArrayElem", "EBX:void*", "EBX:void*:desc EDX:int:ndims", "4", "&arr(i[,j...]); indices on stack (purge = 4*ndims)"},
        {"6b79ed", "PB_DimArray", "", "EBX:void*:desc", "0", "DIM arr(lo TO hi, ...); per-call-site signatures from PBOverrides"},
        {"6b7b91", "PB_ArrayErase", "", "EBX:void*:desc EAX:int:mode", "0", "ERASE arr() (mode 2, before DIM = REDIM) / RESET (mode 4)"},
        {"6bf6a9", "PB_ArraySort", "", "STK:void*:desc STK:int:type STK:int:r1 STK:int:r2 STK:int:r3 STK:int:r4 STK:int:r5 STK:int:r6 STK:int:r7 STK:int:count STK:void*:tagdesc STK:int:z1 STK:int:z2 STK:int:flags", "56", "ARRAY SORT desc() FOR count, TAGARRAY tagdesc() (pops collation$)"},
        {"6bc3cb", "PB_ChkRef", "", "EBX:void*:p", "0", "null check of BYREF param"},
        // DDT dialogs, controls, menus
        {"6b8061", "PB_MsgBox", "EAX:int", "EAX:int:style", "0", "MSGBOX text$, style, title$ (pops 2)"},
        {"6b80b9", "PB_InputBox", "", "ECX:int:x EAX:int:y", "0", "INPUTBOX$(prompt$, title$, default$, x, y) (pops 3, pushes 1)"},
        {"6bb365", "PB_GetDlgItem", "EAX:int", "ECX:int:hDlg EAX:int:id", "0", "GetDlgItem"},
        {"6bb36e", "PB_SendMessage", "EAX:int", "", "16", "SendMessageA(hwnd,msg,wp,lp) stdcall"},
        {"6bb3a0", "PB_ControlGetText", "", "EAX:int:hwnd", "0", "CONTROL GET TEXT -> push"},
        {"6bb3d4", "PB_ControlSetText", "", "EAX:int:hwnd", "0", "CONTROL SET TEXT pop()"},
        {"6bab4e", "PB_DialogEnd", "", "ECX:int:hDlg EAX:int:result", "0", "DIALOG END"},
        {"6ba499", "PB_DialogNew", "EAX:int", "STK:int:flags STK:int:hParent STK:int:x STK:int:y STK:int:w STK:int:h STK:int:style STK:int:exstyle", "32", "DIALOG NEW hParent, title$, x, y, w, h, style, exstyle TO hDlg (pops title$)"},
        {"6babbc", "PB_DialogShowModal", "", "ECX:int:hDlg EAX:void*:callback EBX:int*:result", "0", "DIALOG SHOW MODAL hDlg CALL callback"},
        {"6badc0", "PB_DialogShowState", "EAX:int", "ECX:int:hDlg EAX:int:state", "0", "DIALOG SHOW STATE hDlg, state"},
        {"6bae11", "PB_ControlAdd", "EAX:int", "EAX:int:kind STK:int:hDlg STK:int:id STK:void*:items STK:int:x STK:int:y STK:int:w STK:int:h STK:int:style STK:int:exstyle STK:void*:callback", "40", "CONTROL ADD <kind>, hDlg, id, caption$, x, y, w, h, style, exstyle CALL callback (pops class$, caption$); kind 0 custom class, 3 CHECKBOX, 4 FRAME, 5 LABEL, 6 LINE, 7 OPTION, 9 TEXTBOX, 10 IMAGE, 11 IMAGEX, 15 COMBOBOX, 16 LISTBOX"},
        {"6badf9", "PB_ControlEnable", "EAX:int", "ECX:int:hDlg EAX:int:id EBX:int:enable", "0", "CONTROL ENABLE/DISABLE hDlg, id"},
        {"6bb740", "PB_ControlSetCheck", "", "EBX:int:hDlg ECX:int:id EAX:int:state", "0", "CONTROL SET CHECK hDlg, id, state"},
        {"6bba13", "PB_ControlSetOption", "", "STK:int:hDlg STK:int:id ECX:int:first EAX:int:last", "8", "CONTROL SET OPTION hDlg, id, first, last"},
        {"6bb410", "PB_ControlSetFocus", "EAX:int", "ECX:int:hDlg EAX:int:id", "0", "CONTROL SET FOCUS hDlg, id"},
        {"6bb710", "PB_ControlKill", "", "ECX:int:hDlg EAX:int:id", "0", "CONTROL KILL hDlg, id"},
        {"6bbd14", "PB_MenuNew", "EAX:int", "EAX:int:popup", "0", "MENU NEW BAR (0) / POPUP (1) TO h"},
        {"6bbd27", "PB_MenuAddItem", "", "STK:int:hMenu STK:int:id STK:int:state STK:int:pos STK:void*:callback STK:int:flags", "24", "MENU ADD STRING/POPUP hMenu, pop(), id, state"},
        {"6bbe92", "PB_MenuSetText", "", "ECX:int:hMenu EAX:int:pos EDX:int:flags", "0", "MENU SET TEXT hMenu, pos, pop()"},
        {"6bb4da", "PB_ListBoxAdd", "", "ECX:int:hDlg EAX:int:id", "0", "LISTBOX ADD hDlg, id, pop()"},
        {"6bb424", "PB_ComboBoxAdd", "", "ECX:int:hDlg EAX:int:id", "0", "COMBOBOX ADD hDlg, id, pop()"},
        {"6bc342", "PB_Sleep", "", "EAX:int:ms", "0", "SLEEP ms"},
        {"6b7625", "PB_Shell", "EAX:int", "EAX:int:style ECX:int:inherit", "0", "SHELL(pop(), style)"},
        // rarely used (1-2 call sites each)
        {"6b8c54", "PB_InstrAny", "EAX:int", "EAX:int:start", "0", "INSTR(start, s, ANY chars) (pops 2)"},
        {"6be55c", "PB_RetainAny", "", "", "0", "RETAIN$(s, ANY chars) (pops 2, pushes 1)"},
        {"6bddc5", "PB_ParseArray", "", "EBX:void*:desc", "0", "PARSE s, d, arr() (pops 2)"},
        {"6bce7a", "PB_Remain", "", "EAX:int:start", "0", "REMAIN$(start, s, match) (pops 2, pushes 1)"},
        {"6bcdcf", "PB_Extract", "", "EAX:int:start", "0", "EXTRACT$(start, s, match) (pops 2, pushes 1)"},
        {"6bef43", "PB_StrPushAsciizPtr", "", "EBX:char*:p", "0", "push @p (ASCIIZ PTR)"},
        {"6bb465", "PB_ComboBoxGetText", "", "ECX:int:hDlg EAX:int:id", "0", "COMBOBOX GET TEXT -> push"},
        {"6bb4ad", "PB_ComboBoxReset", "EAX:int", "ECX:int:hDlg EAX:int:id", "0", "COMBOBOX RESET"},
        {"6bb4c2", "PB_ComboBoxSelect", "EAX:int", "EBX:int:hDlg ECX:int:id EAX:int:index", "0", "COMBOBOX SELECT"},
        {"6bb51b", "PB_ListBoxGetText", "", "ECX:int:hDlg EAX:int:id", "0", "LISTBOX GET TEXT -> push"},
        {"6bb563", "PB_ListBoxReset", "EAX:int", "ECX:int:hDlg EAX:int:id", "0", "LISTBOX RESET"},
        {"6be70c", "PB_SwapMem", "", "EBX:void*:a EDX:void*:b EAX:int:size", "0", "SWAP a, b"},
        {"6be6d8", "PB_IDiv", "ST0:float10", "ST0:float10:a ST1:float10:b", "0", "a \\ b (integer division)"},
        {"6be696", "PB_And", "ST0:float10", "ST0:float10:a ST1:float10:b", "0", "a AND b"},
        {"6b7549", "PB_ErrClear", "", "", "0", "ERRCLEAR"},
        {"6bccce", "PB_Beep", "", "", "0", "BEEP"},
        {"6bc2c2", "PB_DialogDoEvents", "EAX:int", "EAX:int:sleep", "0", "DIALOG DOEVENTS"},
        {"6bbdb6", "PB_MenuAttach", "EAX:int", "ECX:int:hMenu EAX:int:hDlg", "0", "MENU ATTACH"},
        {"6ba07b", "PB_UBound", "EAX:int", "EBX:void*:desc EAX:int:dim", "0", "UBOUND(arr, dim)"},
        {"6bb86e", "PB_DialogGetSize", "EAX:int", "ECX:int:hDlg EAX:int:id", "0", "DIALOG GET SIZE TO w (ECX), h (EAX)"},
        {"6bb89f", "PB_DialogSetSize", "", "STK:int:hDlg STK:int:id ECX:int:w EAX:int:h", "8", "DIALOG/CONTROL SET SIZE"},
        {"6bba38", "PB_DialogPixelsToUnits", "EAX:int", "EBX:int:hDlg ECX:int:x EAX:int:y", "0", "DIALOG PIXELS TO UNITS (x in ECX, y in EAX)"},
        {"6bad1b", "PB_DialogShowModeless", "", "ECX:int:hDlg EAX:void*:callback EBX:int*:result", "0", "DIALOG SHOW MODELESS"},
        // printer
        {"6c08e5", "PB_XPrintFont", "", "STK:int:points STK:int:style STK:int:charset STK:int:pitch STK:int:escapement", "20", "XPRINT FONT pop(), points, style"},
        {"6c03b9", "PB_XPrintAttach", "", "EAX:int:mode", "0", "XPRINT ATTACH (pops printer$, job$)"},
        {"6c11f4", "PB_XPrintTextSize", "ST0:float10", "", "0", "XPRINT TEXT SIZE pop() TO w (ST0), h (ST1)"},
        {"6c0254", "PB_XPrintName", "", "", "0", "push XPRINT$"},
        {"6c1263", "PB_XPrintGetPpi", "EAX:int", "", "0", "XPRINT GET PPI TO x (ECX), y (EAX)"},
        {"6c1361", "PB_XPrintGetMargin", "ST0:float10", "", "0", "XPRINT GET MARGIN TO l, t, r, b (ST0..ST3)"},
        {"6c11d2", "PB_XPrintGetClient", "ST0:float10", "", "0", "XPRINT GET CLIENT TO w (ST0), h (ST1)"},
        {"6c0d4f", "PB_XPrintSetPos", "", "ST1:float10:x ST0:float10:y EDX:int:step", "0", "XPRINT SET POS (x, y)"},
        {"6c01c7", "PB_XPrintFormFeed", "", "", "0", "XPRINT FORMFEED"},
        {"6b72bb", "pbrt_MainInit", "", "", "0", "internal: builds PBMAIN's frame (called once from the entry stub)"},
        {"6c0197", "PB_XPrintStr", "", "", "0", "XPRINT pop();"},
        {"6c01bf", "PB_XPrintEnd", "", "", "0", "end of XPRINT statement"},
        {"6c029c", "PB_XPrintCancel", "", "", "0", "XPRINT CANCEL"},
        // File I/O. The file number is passed in the caller's frame slot [ebp-0x36] (shows as local_3a).
        {"6bf055", "PB_FreeFile", "EAX:int", "", "0", "FREEFILE"},
        {"6b8cc8", "PB_Open", "", "DX:short:mode ECX:int:reclen EAX:int:base EBX:int:handle", "0", "OPEN pop() FOR mode AS #[ebp-0x36] LEN reclen; mode 0x1a1 = INPUT, 0x261 = OUTPUT"},
        {"6b84bb", "PB_LineInput", "", "", "0", "LINE INPUT #[ebp-0x36] -> push"},
        {"6b85e3", "PB_InputItem", "", "", "0", "INPUT #[ebp-0x36] string item -> push"},
        {"6ba3ea", "PB_InputNum", "ST0:float10", "", "0", "INPUT #[ebp-0x36] numeric item"},
        {"6b7b17", "PB_Eof", "EAX:int", "AX:short:filenum", "0", "EOF(n)"},
        {"6bcd40", "PB_Close", "", "AX:short:filenum ECX:int:modemask", "0", "CLOSE #n"},
        {"6b7d2f", "PB_WriteStr", "", "", "0", "WRITE #[ebp-0x36], pop() (quoted)"},
        {"6b7d0b", "PB_WriteNum", "", "ST0:float10:x", "0", "WRITE #[ebp-0x36], x"},
        {"6b7d5d", "PB_WriteSep", "", "", "0", "WRITE # item separator (\",\")"},
        {"6bc0f6", "PB_PrintStr", "", "", "0", "PRINT #[ebp-0x36], pop();"},
        {"6b8fe8", "PB_PrintEnd", "", "", "0", "end of WRITE #/PRINT # statement (CRLF)"},
        {"6bd584", "PB_Kill", "", "", "0", "KILL pop()"},
        {"6b8401", "PB_Rgb", "EAX:int", "EAX:int:r ECX:int:g EBX:int:b", "0", "RGB(r, g, b)"},
        {"6bc5d9", "pbrt_StrPtrLen", "", "", "0", "internal: ESI=ptr, ECX=len"},
        {"6bc473", "pbrt_RndNext", "", "", "0", "internal: LCG step, state at 0x6c3280"},
    };

    // Helpers preserve every register except their declared output (PB codegen never relies on more).
    static final String GPR = "EAX EBX ECX EDX ESI EDI EBP";
    // CPU flags are listed as preserved too (PB never reads flags across a helper, except PB_ErrCheckKeepFlags
    // which really preserves them); PB_StrEqual produces flags, so it gets a model without them.
    static final String FL = " CF PF AF ZF SF OF";
    // Void helpers get a dummy output register that PB code never touches (an ST0 output would make the
    // decompiler treat every live x87 value as the helper's return value).
    static final String[][] MODELS = { {"__pbreg", "DR0", GPR + FL, ""}, {"__pbreg_eax", "EAX", "EBX ECX EDX ESI EDI EBP" + FL, ""},
        {"__pbreg_ebx", "EBX", "EAX ECX EDX ESI EDI EBP" + FL, ""}, {"__pbreg_st0", "ST0", GPR + FL, ""},
        {"__pbreg_cmp", "ZF", GPR, ""},
        {"__pbreg_fin", "DR0", GPR + FL, ""},
        {"__pb_arr1", "EBX", "EAX ECX EDX ESI EDI EBP" + FL, "8"}, {"__pb_arr2", "EBX", "EAX ECX EDX ESI EDI EBP" + FL, "12"},
        // PB_DimArray call sites (PBOverrides): EBX = descriptor, stack = dataptr, typeword, elemsize, 2 bounds per dim
        {"__pb_dim1", "DR0", "EAX ECX EDX ESI EDI EBP" + FL, "24"}, {"__pb_dim2", "DR0", "EAX ECX EDX ESI EDI EBP" + FL, "32"},
        {"__pb_dim3", "DR0", "EAX ECX EDX ESI EDI EBP" + FL, "40"}, {"__pb_dim4", "DR0", "EAX ECX EDX ESI EDI EBP" + FL, "48"} };

    void addModels() throws Exception {
        SpecExtension ext = new SpecExtension(currentProgram);
        for (String[] m : MODELS) {
            StringBuilder x = new StringBuilder();
            boolean arr = m[0].startsWith("__pb_arr"), dim = m[0].startsWith("__pb_dim");
            x.append("<prototype name=\"").append(m[0]).append("\" extrapop=\"").append(arr || dim ? m[3] : "unknown").append("\" stackshift=\"4\">");
            x.append("<input>");
            if (dim) x.append("<pentry minsize=\"1\" maxsize=\"4\"><register name=\"EBX\"/></pentry>");
            if (arr) x.append("<pentry minsize=\"1\" maxsize=\"4\"><register name=\"EBX\"/></pentry><pentry minsize=\"1\" maxsize=\"4\"><register name=\"EDX\"/></pentry>");
            x.append("<pentry minsize=\"1\" maxsize=\"500\" align=\"4\"><addr offset=\"4\" space=\"stack\"/></pentry></input>");
            x.append("<output><pentry minsize=\"1\" maxsize=\"").append(m[1].equals("ST0") ? "10" : m[1].equals("ZF") ? "1" : "4")
             .append("\"").append(m[1].equals("ST0") ? " metatype=\"float\"" : "").append("><register name=\"").append(m[1]).append("\"/></pentry></output>");
            x.append("<unaffected>");
            // x87: Ghidra models the FPU stack as ST0..ST7 registers that shift on push/pop. A prototype cannot
            // express that, so helpers that push or pop are reached through inlined shims (see fpuShims) and
            // every helper is declared to preserve the registers it does not write: ST1-ST7 for ST0-returning
            // helpers, all of ST0-ST7 otherwise.
            String fpu = m[0].equals("__pbreg_st0") ? " ST1 ST2 ST3 ST4 ST5 ST6 ST7" : " ST0 ST1 ST2 ST3 ST4 ST5 ST6 ST7";
            for (String r : (m[2] + " ESP DF" + fpu).split(" ")) x.append("<register name=\"").append(r).append("\"/>");
            x.append("</unaffected></prototype>");
            ext.addReplaceCompilerSpecExtension(x.toString(), monitor);
        }
    }

    String modelFor(String[] h) {
        if (h[1].equals("PB_StrEqual")) return "__pbreg_cmp";
        if (h[2].isEmpty() && h[3].contains("ST0:")) return "__pbreg_fin";
        return modelFor(h[2]);
    }

    String modelFor(String ret) {
        if (ret.startsWith("ZF")) return "__pbreg_cmp";
        if (ret.startsWith("EAX")) return "__pbreg_eax";
        if (ret.startsWith("EBX")) return "__pbreg_ebx";
        if (ret.startsWith("ST0")) return "__pbreg_st0";
        return "__pbreg";
    }

    @Override
    public void run() throws Exception {
        FunctionManager fm = currentProgram.getFunctionManager();
        addModels();
        // 1. Prefix every runtime function.
        int rt = 0;
        for (Function f : fm.getFunctions(toAddr(RT_START), true)) {
            if (f.getEntryPoint().getOffset() >= RT_END) break;
            if (f.getName().startsWith("FUN_")) {
                f.setName("pbrt_" + f.getEntryPoint().toString().replaceFirst("^0+", ""), SourceType.ANALYSIS);
                rt++;
            }
        }
        // 1b. Unnamed runtime helpers: Ghidra invents stack params from the callers' zero-init pushes,
        //     which corrupts the callers' stack depth. Derive the real purge from the RET instructions.
        int fixed = 0;
        for (Function f : fm.getFunctions(toAddr(RT_START), true)) {
            if (f.getEntryPoint().getOffset() >= RT_END) break;
            int purge = -1;
            boolean consistent = true;
            for (Instruction i : currentProgram.getListing().getInstructions(f.getBody(), true)) {
                if (!i.getMnemonicString().equals("RET")) continue;
                int p = i.getNumOperands() == 0 ? 0 : (int) i.getScalar(0).getValue();
                if (purge >= 0 && p != purge) consistent = false;
                purge = p;
            }
            if (purge < 0 || !consistent) continue;
            f.setCallingConvention(purge == 0 ? "__pbreg_eax" : "__stdcall");
            List<Variable> params = new ArrayList<>();
            for (int k = 0; k < purge / 4; k++)
                params.add(new ParameterImpl("arg" + (k + 1), IntegerDataType.dataType, currentProgram));
            f.updateFunction(purge == 0 ? "__pbreg_eax" : "__stdcall",
                purge == 0 ? new ReturnParameterImpl(IntegerDataType.dataType, reg("EAX"), currentProgram)
                    : new ReturnParameterImpl(IntegerDataType.dataType, currentProgram),
                params, purge == 0 ? FunctionUpdateType.CUSTOM_STORAGE : FunctionUpdateType.DYNAMIC_STORAGE_ALL_PARAMS,
                true, SourceType.ANALYSIS);
            f.setStackPurgeSize(purge);
            fixed++;
        }
        println("runtime purge fixed: " + fixed);
        // 2. Named helpers with register signatures.
        for (String[] h : HELPERS) {
            Address a = toAddr(Long.parseLong(h[0], 16));
            Function f = fm.getFunctionAt(a);
            if (f == null) f = createFunction(a, null);
            if (f == null) { println("cannot create " + h[1]); continue; }
            f.setName(h[1], SourceType.USER_DEFINED);
            if (!h[0].equals("6bb36e")) f.setCallingConvention(modelFor(h));
            List<Variable> params = new ArrayList<>();
            int stk = h[3].isEmpty() ? 0 : (int) Arrays.stream(h[3].split(" ")).filter(x -> x.startsWith("STK:")).count();
            if (!h[3].isEmpty()) for (String p : h[3].split(" ")) {
                String[] r = p.split(":");
                if (r[0].equals("STK")) {
                    // stack args are listed in push order: the last one pushed sits just above the return address
                    DataType dt = type(r[1]);
                    params.add(new ParameterImpl(r[2], dt, new VariableStorage(currentProgram, 4 * stk--, dt.getLength()), currentProgram));
                } else {
                    params.add(new ParameterImpl(r[2], type(r[1]), reg(r[0]), currentProgram));
                }
            }
            f.updateFunction(h[0].equals("6bb36e") ? "__stdcall" : modelFor(h),
                h[2].isEmpty() ? new ReturnParameterImpl(VoidDataType.dataType, currentProgram)
                    : new ReturnParameterImpl(type(h[2].split(":")[1]), reg(h[2].split(":")[0]), currentProgram),
                params, FunctionUpdateType.CUSTOM_STORAGE, true, SourceType.USER_DEFINED);
            f.setStackPurgeSize(Integer.parseInt(h[4]));
            if (!h[5].isEmpty()) f.setComment(h[5]);
        }
        arrayElem(fm);
        println("x87 shims: " + fpuShims(fm));
        println("ordered string compares: " + orderedStrCompares(fm));
        int up = userParams(fm);
        println("user function param counts set from RET N: " + up);
        // 3. String data for each pooled literal: [len:4][text][\0], 4-byte aligned.
        Memory mem = currentProgram.getMemory();
        Listing listing = currentProgram.getListing();
        long p = POOL;
        int strings = 0;
        while (p + 4 < DATA_END) {
            int len = mem.getInt(toAddr(p));
            if (len >= 0 && len < 0x10000 && p + 4 + len < DATA_END && mem.getByte(toAddr(p + 4 + len)) == 0 && printable(mem, p + 4, len)) {
                Address s = toAddr(p + 4);
                listing.clearCodeUnits(toAddr(p), toAddr(p + 4 + len), false);
                listing.createData(toAddr(p), DWordDataType.dataType);
                Data d = listing.createData(s, new TerminatedStringDataType(), len + 1);
                MutabilitySettingsDefinition.DEF.setChoice(d, MutabilitySettingsDefinition.CONSTANT);
                strings++;
                p = (p + 4 + len + 1 + 3) & ~3L;
            } else {
                p += 4;
            }
        }
        int cmps = annotateStrCompares(fm);
        int gosubs = nameGosubs(fm);
        println("runtime functions prefixed: " + rt + ", helpers named: " + HELPERS.length + ", literals: " + strings
            + ", string compares: " + cmps + ", gosubs: " + gosubs);
    }

    // PB_ArrayElem(desc in EBX, ndims in EDX, indices on the stack) -> element address in EBX.
    // 1-D is the default prototype; 2-D call sites (mov edx,2) get a per-call-site signature override.
    // Helpers by x87 stack effect. PUSH: returns a new ST0 (old ST0.. move down). POP: consumes ST0.
    // POP1: combines ST0 and ST1 into ST0 (pops one). In-place helpers (INT, FIX, SQR, ...) need no shim.
    static final long[] FPU_PUSH = {0x6bc3d7, 0x6bd6bb, 0x6bd659, 0x6bc01a, 0x6bc021, 0x6bc00f, 0x6bcc93, 0x6bcc9c, 0x6ba3ea};
    static final long[] FPU_POP = {0x6bd503, 0x6bd4ef, 0x6bd517, 0x6bd4d0, 0x6b9006, 0x6b7d0b, 0x6bcca5, 0x6bccae, 0x6bc3d0};
    static final long[] FPU_POP1 = {0x6be6ef, 0x6bcabc, 0x6be6d8, 0x6be696};
    static final long SHIMS = 0x800000L;

    // Give every push/pop helper an inlined shim in a new block and retarget user-code calls to it:
    //   push: fld st(0); call H; ret     (the dup makes room, H overwrites ST0)
    //   pop:  call H; fstp st(0); ret    (H is declared to leave ST0-ST7 alone; the fstp pops)
    //   pop1: call H; fstp st(1); ret
    // The decompiler inlines the shim, so call sites show the plain helper call with correct x87 dataflow.
    // Ghidra hard-inlines a given function at most once per caller, so every call site gets its own shim.
    int fpuShims(FunctionManager fm) throws Exception {
        Memory mem = currentProgram.getMemory();
        MemoryBlock b = mem.createInitializedBlock(".pbshim", toAddr(SHIMS), 0x10000, (byte) 0xcc, monitor, false);
        b.setRead(true); b.setExecute(true); b.setWrite(false);
        long sh = SHIMS;
        int sites = 0;
        long[][] groups = {FPU_PUSH, FPU_POP, FPU_POP1};
        for (int g = 0; g < 3; g++) for (long h : groups[g]) {
            Function hf = fm.getFunctionAt(toAddr(h));
            List<Address> from = new ArrayList<>();
            for (Reference r : currentProgram.getReferenceManager().getReferencesTo(toAddr(h))) {
                if (r.getReferenceType().isCall() && r.getFromAddress().getOffset() < RT_START) from.add(r.getFromAddress());
            }
            for (Address a : from) {
                if ((mem.getByte(a) & 0xff) != 0xe8) continue;
                byte[] code;
                if (g == 0) code = new byte[] {(byte) 0xd9, (byte) 0xc0, (byte) 0xe8, 0, 0, 0, 0, (byte) 0xc3};
                else code = new byte[] {(byte) 0xe8, 0, 0, 0, 0, (byte) 0xdd, (byte) (g == 1 ? 0xd8 : 0xd9), (byte) 0xc3};
                int callAt = g == 0 ? 2 : 0;
                int rel = (int) (h - (sh + callAt + 5));
                for (int i = 0; i < 4; i++) code[callAt + 1 + i] = (byte) (rel >>> (8 * i));
                mem.setBytes(toAddr(sh), code);
                disassemble(toAddr(sh));
                Function f = createFunction(toAddr(sh), "shim_" + (hf != null ? hf.getName() : Long.toHexString(h)) + "_" + a);
                f.setInline(true);
                int r2 = (int) (sh - (a.getOffset() + 5));
                byte[] c = {(byte) 0xe8, (byte) r2, (byte) (r2 >>> 8), (byte) (r2 >>> 16), (byte) (r2 >>> 24)};
                clearListing(a, a.add(4));
                mem.setBytes(a, c);
                disassemble(a);
                sh += 8;
                sites++;
            }
        }
        return sites;
    }

    // Two sites test the CF result of PB_StrEqual (a$ < b$) as well:
    //   call PB_StrEqual; mov eax,-1; jbe/jae +1; inc eax     ->  eax = (a$ <= b$) / (a$ >= b$) ? -1 : 0
    // Replace each such sequence (in Ghidra only) with a call to a small helper that returns that value in EAX.
    int orderedStrCompares(FunctionManager fm) throws Exception {
        Memory mem = currentProgram.getMemory();
        long sh = SHIMS + 0xf000;
        Map<String, String> names = Map.of("JBE", "PB_StrLessEq", "JAE", "PB_StrGreaterEq", "JA", "PB_StrGreater",
            "JB", "PB_StrLess", "JNC", "PB_StrGreaterEq", "JC", "PB_StrLess");
        Map<String, Byte> ops = Map.of("JBE", (byte) 0x76, "JAE", (byte) 0x73, "JNC", (byte) 0x73, "JA", (byte) 0x77,
            "JB", (byte) 0x72, "JC", (byte) 0x72);
        Map<String, Address> made = new HashMap<>();
        int n = 0;
        List<Address> from = new ArrayList<>();
        for (Reference r : currentProgram.getReferenceManager().getReferencesTo(toAddr(0x6be966L)))
            if (r.getReferenceType().isCall() && r.getFromAddress().getOffset() < RT_START) from.add(r.getFromAddress());
        for (Address a : from) {
            Instruction call = getInstructionAt(a), mov = call == null ? null : call.getNext();
            Instruction j = mov == null ? null : mov.getNext(), inc = j == null ? null : j.getNext();
            if (inc == null || !mov.toString().equals("MOV EAX,0xffffffff") || !names.containsKey(j.getMnemonicString())
                || !inc.toString().equals("INC EAX") || inc.getAddress().getOffset() != a.getOffset() + 12) continue;
            String nm = names.get(j.getMnemonicString());
            Address shim = made.get(nm);
            if (shim == null) {
                shim = toAddr(sh);
                int rel = (int) (0x6be966L - (sh + 5));
                byte[] code = {(byte) 0xe8, (byte) rel, (byte) (rel >>> 8), (byte) (rel >>> 16), (byte) (rel >>> 24),
                    (byte) 0xb8, -1, -1, -1, -1, ops.get(j.getMnemonicString()), 0x01, 0x40, (byte) 0xc3};
                mem.setBytes(shim, code);
                disassemble(shim);
                Function f = createFunction(shim, nm);
                f.updateFunction("__pbreg_eax", new ReturnParameterImpl(IntegerDataType.dataType, reg("EAX"), currentProgram),
                    new ArrayList<Variable>(), FunctionUpdateType.CUSTOM_STORAGE, true, SourceType.USER_DEFINED);
                f.setComment("-1 if a$ " + (nm.contains("LessEq") ? "<=" : nm.contains("GreaterEq") ? ">=" : nm.contains("Less") ? "<" : ">")
                    + " b$ else 0 (pops 2 strings); stands in for PB_StrEqual + a CF/ZF test");
                made.put(nm, shim);
                sh += 16;
            }
            int r2 = (int) (shim.getOffset() - (a.getOffset() + 5));
            byte[] c = {(byte) 0xe8, (byte) r2, (byte) (r2 >>> 8), (byte) (r2 >>> 16), (byte) (r2 >>> 24),
                (byte) 0x90, (byte) 0x90, (byte) 0x90, (byte) 0x90, (byte) 0x90, (byte) 0x90, (byte) 0x90, (byte) 0x90};
            clearListing(a, a.add(12));
            mem.setBytes(a, c);
            disassemble(a);
            currentProgram.getListing().setComment(a, CodeUnit.PRE_COMMENT, null);
            n++;
        }
        return n;
    }

    void arrayElem(FunctionManager fm) throws Exception {
        Function f = fm.getFunctionAt(toAddr(0x6bc34aL));
        DataType vp = new PointerDataType(VoidDataType.dataType);
        List<Variable> ps = List.of(new ParameterImpl("desc", vp, currentProgram),
            new ParameterImpl("ndims", IntegerDataType.dataType, currentProgram),
            new ParameterImpl("i", IntegerDataType.dataType, currentProgram));
        f.updateFunction("__pb_arr1", new ReturnParameterImpl(vp, currentProgram), ps,
            FunctionUpdateType.DYNAMIC_STORAGE_ALL_PARAMS, true, SourceType.USER_DEFINED);
        // 2-D call sites are overridden by PBOverrides.java (needs a reopened program to see __pb_arr2).
    }

    // PB SUB/FUNCTIONs are stdcall: RET N pops N/4 (mostly BYREF) params. Fix Ghidra's guesses.
    int userParams(FunctionManager fm) throws Exception {
        int n = 0;
        for (Function f : fm.getFunctions(true)) {
            if (f.getEntryPoint().getOffset() >= RT_START || f.getName().startsWith("GOSUB_")) continue;
            int purge = -1; boolean ok = true;
            for (Instruction i : currentProgram.getListing().getInstructions(f.getBody(), true)) {
                if (!i.getMnemonicString().equals("RET")) continue;
                int p = i.getNumOperands() == 0 ? 0 : (int) i.getScalar(0).getValue();
                if (purge >= 0 && p != purge) ok = false;
                purge = p;
            }
            if (purge < 0 || !ok || f.getParameterCount() == purge / 4) continue;
            List<Variable> params = new ArrayList<>();
            for (int k = 0; k < purge / 4; k++)
                params.add(new ParameterImpl("param_" + (k + 1), Undefined4DataType.dataType, currentProgram));
            f.updateFunction("__stdcall", f.getReturn(), params, FunctionUpdateType.DYNAMIC_STORAGE_ALL_PARAMS, true, SourceType.ANALYSIS);
            f.setStackPurgeSize(purge);
            n++;
        }
        return n;
    }

    // PB_StrEqual leaves its result in CPU flags. It is modelled as returning ZF, which covers every
    // site that branches with JZ/JNZ (all but 2). Flag the rest, which test CF (a$ < b$) as well.
    int annotateStrCompares(FunctionManager fm) {
        Function cmp = fm.getFunctionAt(toAddr(0x6be966L));
        Set<String> zf = Set.of("JZ", "JNZ", "CALL");
        int n = 0;
        for (Reference r : currentProgram.getReferenceManager().getReferencesTo(cmp.getEntryPoint())) {
            Instruction call = getInstructionAt(r.getFromAddress());
            if (call == null || r.getFromAddress().getOffset() >= RT_START) continue;
            Instruction j = call.getNext();
            if (j != null && zf.contains(j.getMnemonicString())) continue;
            currentProgram.getListing().setComment(call.getAddress(), CodeUnit.PRE_COMMENT,
                "PB string compare: flags consumed by " + (j == null ? "?" : j.toString())
                + " (CF = a$ < b$, ZF = a$ = b$); the decompiled condition below is wrong");
            n++;
        }
        return n;
    }

    // GOSUB targets run on the caller's frame: no "push ebp; mov ebp,esp" prologue but EBP-relative access.
    int nameGosubs(FunctionManager fm) throws Exception {
        int n = 0;
        for (Function f : fm.getFunctions(true)) {
            long off = f.getEntryPoint().getOffset();
            if (off >= RT_START || !f.getName().startsWith("FUN_")) continue;
            Instruction first = getInstructionAt(f.getEntryPoint());
            if (first == null || first.toString().startsWith("PUSH EBP")) continue;
            boolean usesFrame = false;
            for (Instruction i : currentProgram.getListing().getInstructions(f.getBody(), true)) {
                if (i.toString().matches(".*\\[EBP \\+ 0xffff[0-9a-f]{4}\\].*")) { usesFrame = true; break; }
            }
            if (!usesFrame) continue;
            f.setName("GOSUB_" + f.getEntryPoint().toString().replaceFirst("^0+", ""), SourceType.ANALYSIS);
            f.setComment("PB GOSUB target: runs on the caller's stack frame (reads/writes caller locals)");
            n++;
        }
        return n;
    }

    boolean printable(Memory mem, long a, int len) throws Exception {
        byte[] b = new byte[len];
        mem.getBytes(toAddr(a), b);
        for (byte c : b) { int u = c & 0xff; if (u < 0x20 && u != 9 && u != 10 && u != 13) return false; }
        return true;
    }

    Register reg(String n) { return currentProgram.getRegister(n); }

    DataType type(String t) {
        switch (t) {
            case "int": return IntegerDataType.dataType;
            case "uint": return UnsignedIntegerDataType.dataType;
            case "short": return ShortDataType.dataType;
            case "bool": return BooleanDataType.dataType;
            case "float10": return Float10DataType.dataType;
            case "char*": return new PointerDataType(CharDataType.dataType);
            case "char**": return new PointerDataType(new PointerDataType(CharDataType.dataType));
            case "longlong*": return new PointerDataType(LongLongDataType.dataType);
            case "int*": return new PointerDataType(IntegerDataType.dataType);
            default: return new PointerDataType(VoidDataType.dataType);
        }
    }
}
