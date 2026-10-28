"""Run the original PowerBASIC extension DLLs with the existing Windows API emulator.

The DLLs own independent runtime state and RNGs. Unlike WSR.EXE they initialize
through DllMain(DLL_PROCESS_ATTACH), then expose stdcall functions.
"""
from game import *

class DllEmu(Emu):
    def call(self, addr, args):
        # Repeated exports replace the trampoline's embedded destination. Unicorn can retain
        # its translated block after mem_write, so invalidate it before each call.
        self.uc.ctl_remove_cache(TRAMP, TRAMP + 0x100)
        return super().call(addr, args)

    def boot(self):
        self.call(BASE + self.pe.OPTIONAL_HEADER.AddressOfEntryPoint,
                  [("v", BASE), ("v", 1), ("v", 0)])
        if self.stub_error or self.bad_access or self.uc.reg_read(UC_X86_REG_EIP) != SENTINEL_END:
            raise EmuFault("DLL initialization failed: %s, %s" % (self.stub_error, self.bad_access))
        self.boot_log, self.log, self.booted = self.log, [], True

if __name__ == "__main__":
    import argparse, json
    ap = argparse.ArgumentParser()
    ap.add_argument("dll")
    ap.add_argument("export")
    ap.add_argument("args", nargs="*")
    ap.add_argument("--answer", action="append", default=[])
    ap.add_argument("--calls", type=int, default=1)
    o = ap.parse_args()
    e = DllEmu(o.dll, answers=o.answer, budget=5000000)
    pe = pefile.PE(o.dll)
    export = next(x for x in pe.DIRECTORY_ENTRY_EXPORT.symbols if x.name.decode() == o.export)
    results=[]
    for _ in range(o.calls):
        r=e.call(BASE+export.address,[parse_arg(a) for a in o.args])
        if r[0]:
            raise EmuFault(r[0])
        results.append({"result":r,"text":e.ret_str.decode("latin-1") if e.ret_str is not None else None})
    print(json.dumps({"calls":results,"log":e.log,"dialogs":e.wm.models(),"files":{k:v.decode("latin-1") for k,v in e.vfs.files.items()}},ensure_ascii=False,
                     default=lambda x: x.decode("latin-1") if isinstance(x,bytes) else str(x)))
