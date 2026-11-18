# Lind-IF  
  
  
Data structures  
  
Cage  
Thread  
  
  
Lind -> Runtime Functions  
  
**///helpers**  
//checks whether the executable at path can be run with this runtime  
*BelongsToRuntime*(str exe_path) -> Result<Bool>  
  
*GetHostAddress*(void* guest_addr) -> Result<void*>  
  
//For each struct XYZ used by syscalls we need conversion functions  
//these are invoked by Rawposix to correctly parse the input to such syscalls  
*GetRuntimeXYZfromLindXYZ(struct LindXYZ* In, void* Out)*  
*GetLindXYZfromRuntimeXYZ<Runtime>(void* Out, struct LindXYZ* In)*  
  
//Runtime Maintainers might implement accessor Functions for grates to not have to do dynamic memory allocations or large stack allocations:  
*//This requires an easy way for grates to find out which structure layout is used/which Runtime the cage is running*  
*LindMpkSetIovecBase(void* iovec, int index, void* new_val)*  
  
  
  
**///lifecycle functions**  
  
//initializes a cage by loading the program from exe_path  
//**think**: Create a new VM from an image  
InitCage(int cageId, str exe_path, void* mem_start, unsigned mem_length) -> Result<>  
  
  
//**think**: snapshot and duplicate VM  
DuplicateCage(int cageId, int new_cageid) -> Result<>  
  
  
//create a new thread  
//**think**: tell the VM to create another thread for the isolee   
//Note the runtime is not required to implement threads by itself. It might just call pthread_create   
//to launch a new host thread  
//returns 0 for success, >0 for error that can be handled by user (e.g. OOM, …), failure for terminal  
//exception (should result in segfault for cage)   
CreateThread(int cageId, int threadId, struct LindCloneArgs) -> Result<int>  
  
  
//**think:** send a signal to the isolee in the VM  
SendSignal(int cageId, int threadId, struct LindSigInfo) -> Result<>  
  
  
//**think:** Make the VM run a certain handler function with certain args in the isolee  
RunHandler(int cageId, struct LindHandlerArgs)  
  
  
//**think:** kill the VM with the isolee inside  
TerminateCage(int cageId)  
  
  
//**think:** tell the VM to create another thread for the isolee, or terminate one of the VMs vCPUs if the runtime relies on host scheduling  
TerminateThread(int cageId, int threadId)  
  
  
//**think:** Set up the VM and host page tables and metadata so that the mmap becomes effective   
DoMmap(int cageId, struct Mmap_args)  
  
//**think:** Apply the mprotect operation inside the VM  
DoMprotect(int cageid, struct Mprotect_args)  
  
//**think:** Apply the munmap operation inside the VM  
DoMunmap(int cageid, struct Munmap_args)  
  
  
  
Runtime -> Lind functions  
  
//Note: addresses passed as arguments to this should be guest addresses.  
//An alternative would be to expect the runtime or the guest to convert any addresses to host addresses. This would require changes to the runtimes libc and its syscall mechanism which can cause significant implementation costs and should be avoided.   
3i::make_syscall(…)  
  
  
  
Tentative:   
DuplicateThread  
  
