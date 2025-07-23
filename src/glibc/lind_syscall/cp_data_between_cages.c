#include <stdint.h> // For uint64_t definition
#include <syscall-template.h> // For make_syscall definition

int cp_data_between_cages(uint64_t thiscage, uint64_t targetcage, uint64_t srcaddr, uint64_t srccage, uint64_t destaddr, uint64_t destcage, uint64_t len, uint64_t copytype) {
    return CP_DATA_SYSCALL(thiscage, targetcage, srcaddr, srccage, destaddr, destcage, len, copytype);
}
