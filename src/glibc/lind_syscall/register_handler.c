#include <stdint.h> // For uint64_t definition
#include <syscall-template.h> // For make_syscall definition

int register_handler(uint64_t targetcage, uint64_t targetcallnum, uint64_t handlefunc_index_in_this_grate, uint64_t this_grate_id) {
    return REGISTER_HANDLER_SYSCALL(targetcage, targetcallnum, handlefunc_index_in_this_grate, this_grate_id);
}
