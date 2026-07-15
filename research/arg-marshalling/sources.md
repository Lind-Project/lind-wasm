# Sources — Argument-Marshalling Survey

Status legend: ✅ full PDF in `papers/` · 📄 slides only · 🌐 web doc/spec (no paper) · 🔒 paywalled (abstract/secondary only).

## Automatic partitioning & marshalling inference
- ✅ **KSplit: Automating Device Driver Isolation** — Huang, Narayanan, Detweiler, K. Huang, Tan, Jaeger, Burtsev. OSDI 2022. `papers/ksplit_osdi22.pdf` — https://www.usenix.org/conference/osdi22/presentation/huang-yongzhe
- 📄 **PtrSplit: Supporting General Pointers in Automatic Program Partitioning** — Liu, Tan, Jaeger. CCS 2017. `papers/ptrsplit_ccs17.pdf` (author *slides*; full treatment in the dissertation below). https://dl.acm.org/doi/10.1145/3133956.3134066
- ✅ **Shen Liu, PhD Dissertation (PtrSplit + follow-on)** — Penn State. `papers/SHEN_LIU_Dissertation.pdf`
- ✅ **Glamdring: Automatic Application Partitioning for Intel SGX** — Lind et al. USENIX ATC 2017. `papers/glamdring_atc17.pdf`
- ✅ **Program-mandering: Quantitative Privilege Separation** — Liu et al. CCS 2019. `papers/program-mandering-ccs2019.pdf`

## Library / driver / process sandboxing
- ✅ **RLBox: Retrofitting Fine Grain Isolation in the Firefox Renderer** — Narayan, Disselkoen, Garfinkel, et al. USENIX Security 2020. `papers/rlbox_sec20.pdf`
- ✅ **Sandcrust: Automatic Sandboxing of Unsafe Components in Rust** — Lamowski, Weinhold, Lackorzynski, Härtig. PLOS@SOSP 2017. `papers/sandcrust_plos17.pdf`
- ✅ **LXFI: Software Fault Isolation with API Integrity and Multi-Principal Modules** — Mao, Chen, Zhou, Wang, Zeldovich, Kaashoek. SOSP 2011. `papers/lxfi_sosp11.pdf` *(KSplit ref [62])*
- ✅ **LXDs: Towards Isolation of Kernel Subsystems** — Narayanan, Balasubramanian, Jacobsen, Spall, Bauer, Quigley, Hussain, Younis, Shen, Bhattacharyya, Burtsev. USENIX ATC 2019. `papers/lxds_atc19.pdf` *(KSplit ref [66]; origin of the **"projection"** field-subset IDL concept and the hardware-isolation/synchronized-copy driver framework KSplit builds its IDL compiler on)* — https://www.usenix.org/system/files/atc19-narayanan.pdf
- ✅ **XFI: Software Guards for System Address Spaces** — Erlingsson, Abadi, et al. OSDI 2006. `papers/xfi_osdi06.pdf` *(KSplit ref [25])*
- ✅ **BGI: Fast Byte-Granularity Software Fault Isolation** — Castro, Costa, Martin, Peinado, Akritidis, Donnelly, Barham, Black. SOSP 2009. `papers/bgi_sosp09.pdf` *(KSplit ref [18]; byte-granularity per-access SFI — the per-access-check model KSplit contrasts its synchronized-copy approach against)* — https://www.sigops.org/s/conferences/sosp/2009/papers/castro-sosp09.pdf
- ✅ **Wedge: Splitting Applications into Reduced-Privilege Compartments** — Bittau, Marchenko, Handley, Karp. NSDI 2008. `papers/wedge_nsdi08.pdf`
- ✅ **Privtrans: Automatically Partitioning Programs for Privilege Separation** — Brumley, Song. USENIX Security 2004. `papers/privtrans_sec04.pdf`
- ✅ **Cali: Compiler-Assisted Library Isolation** — Bauer, Rossow. AsiaCCS 2021. (in Bauer dissertation) `papers/bauer_thesis_cali.pdf` — https://publications.cispa.de/articles/conference_contribution/Cali_Compiler_Assisted_Library_Isolation/24613602
- ✅ **Donky: Domain Keys – Efficient In-Process Isolation for RISC-V and x86** — Schrammel et al. USENIX Security 2020. `papers/donky_sec20.pdf` *(enforcement, not marshalling)*
- ✅ **Hodor: Intra-Process Isolation for High-Throughput Data-Plane Libraries** — Hedayati et al. USENIX ATC 2019. `papers/hodor_atc19.pdf` *(enforcement)*

## Accelerator / GPU API remoting
- ✅ **Cricket: A virtualization layer for distributed execution of CUDA applications with checkpoint/restart** — Eiling, Baude, Lankes, Monti. CC:P&E 2022. `papers/cricket_cpe22.pdf`
- ✅ **On the Virtualization of CUDA Based GPU Remoting on ARM and X86 (GVirtuS)** — Montella et al. 2016. `papers/gvirtus_arm_2016.pdf`
- ✅ **Acceleration-as-a-Service: Exploiting Virtualised GPUs (rCUDA context)** — arXiv 1508.02558. `papers/rcuda_aaas_arxiv.pdf`
- ✅ **Characterizing Network Requirements for GPU API Remoting in AI Applications** — arXiv 2401.13354. `papers/2401.13354_gpu_api_remoting_network.pdf`
- 🔒 rCUDA core papers (Duato et al.; "A complete and efficient CUDA-sharing solution", Parallel Computing 2014) — paywalled; used via secondary descriptions + Wikipedia.
- 🔒 DS-CUDA, vCUDA — referenced via Cricket's related-work; not retrieved.

## IDL / RPC annotation languages (specs/docs)
- 🌐 **Microsoft RPC / MIDL** — `[in]/[out]`, `[size_is]/[length_is]/[max_is]/[first_is]/[last_is]`, `[string]`, pointer kinds `ref`/`unique`/`ptr`, `[switch_is]/[switch_type]`. https://learn.microsoft.com/en-us/windows/win32/midl/size-is · https://learn.microsoft.com/en-us/windows/win32/rpc/pointers
- 🌐 **Sun RPC / XDR** — External Data Representation Standard. RFC 4506. https://www.rfc-editor.org/rfc/rfc4506 ; rpcgen.
- 🌐 **Mach Interface Generator (MIG)** — `.defs`, in/out/inout, out-of-line (ool) memory, port rights as capabilities. https://www.gnu.org/software/hurd/microkernel/mach/mig.html *(fetch rate-limited; cited from documented behavior)*
- 🌐 **Protocol Buffers / gRPC** — message schema, varint/length-delimited wire format, `oneof`. https://protobuf.dev/programming-guides/encoding/
- 🌐 Apache Thrift, Cap'n Proto, FlatBuffers — modern IDLs; value-only, zero-copy arenas (Cap'n Proto/FlatBuffers).

## Syscall / API description & tracing
- 🌐 **syzkaller syzlang** — `ptr[dir,T]`, `buffer[dir]`, `array`, `string/stringnoz`, `len[]/bytesize[]`, `resource` (opaque handles), per-field direction, `(if[expr])`. https://github.com/google/syzkaller/blob/master/docs/syscall_descriptions_syntax.md
- 🌐 strace / ltrace — per-syscall/prototype argument decoders.

## FFI / binding generators
- 🌐 **SWIG typemaps** — `in/out/argout/check/freearg`, `numinputs=0` out-params, multi-argument typemaps for `(buf,len)`. https://www.swig.org/Doc4.0/Typemaps.html
- 🌐 Python **ctypes** / **cffi** — `argtypes/restype`, `c_char_p`/`c_void_p`/`POINTER`/`CFUNCTYPE`, `byref`; cffi parses C declarations.
- 🌐 Rust **bindgen** / Go **cgo** / .NET **P/Invoke** — type translation without size/direction semantics.

---
### Retrieval notes
- Failed/wrong first attempts that were corrected: an arXiv ID guessed for "Cricket" returned an unrelated math paper (removed); Cricket later retrieved from RWTH open repository. ERIM and BreakApp PDFs not retrieved (URLs unstable) — neither is central to argument categorization.
- The deep-research workflow crashed on a harness/structured-output error after its fetch agents had already downloaded several PDFs (KSplit, Program-mandering, GVirtuS, the 2024 GPU paper, the Shen Liu dissertation); those were kept and de-duplicated against the manually fetched set.
