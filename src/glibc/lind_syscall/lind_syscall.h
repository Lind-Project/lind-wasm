int lind_syscall (unsigned int callnumber, unsigned long long callname, unsigned long long arg1, unsigned long long arg2, unsigned long long arg3, unsigned long long arg4, unsigned long long arg5, unsigned long long arg6, int raw);
int lind_register_syscall(uint64_t targetcage, 
    uint64_t targetcallnum, 
    uint64_t this_grate_id,
    uint64_t register_flag);
int lind_cp_data(uint64_t thiscage, uint64_t targetcage, uint64_t srcaddr, uint64_t srccage, uint64_t destaddr, uint64_t destcage, uint64_t len, uint64_t copytype);
