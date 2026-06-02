// Grate for auto-nested struct marshalling test.
// Intercepts toy_buf_checksum(const struct toy_buffer *b) using lind_marshal.h
// nested struct support.
//
// struct toy_buffer layout (wasm32):
//   offset 0: data  (char* — uint32_t WASM ptr)  → PTR IN, size from sibling field[1]
//   offset 4: len   (unsigned — uint32_t)        → SCALAR
//   struct_size = 8
//
// The handler receives a shadow of the struct with b->data pointing into grate
// shadow memory. It computes sum+1 to prove interception and local data access.
//
// Compile:
//   lind-clang -s --compile-grate auto-nested_grate.c -- -I..
#include <lind_syscall.h>
#include <stdio.h>
#include <sys/wait.h>
#include <unistd.h>
#include <assert.h>
#include <stdint.h>

#include "../lind_marshal.h"

// --- Layout for struct toy_buffer ---
// Field 0: data (PTR IN, size = sibling field[1])
// Field 1: len  (SCALAR)

static struct lind_arg_spec _data_spec = {
    .kind          = LIND_ARG_PTR,
    .ptr_direction = LIND_PTR_IN,
    .size_kind     = LIND_SIZE_FROM_ARG,
    .size_arg_index = 1,   // sibling field index 1 (len)
};
static struct lind_arg_spec _len_spec = {
    .kind = LIND_ARG_SCALAR,
};

static struct lind_field _buf_fields[2] = {
    { .offset = 0, .spec = &_data_spec, .touched = 1 },  // data
    { .offset = 4, .spec = &_len_spec,  .touched = 1 },  // len
};

static struct lind_layout _buf_layout = {
    .kind        = LIND_LO_STRUCT,
    .nfields     = 2,
    .fields      = _buf_fields,
    .struct_size = 8,
};

static struct lind_marshal_spec buf_checksum_spec = {
    .nargs = 1,
    .args = {
        {
            .kind          = LIND_ARG_PTR,
            .ptr_direction = LIND_PTR_IN,
            .size_kind     = LIND_SIZE_CONST,
            .const_size    = 8,         // sizeof(struct toy_buffer) in wasm32
            .layout        = &_buf_layout,
        },
    },
    .ret = { .kind = LIND_RET_SCALAR },
};

static uint64_t handler_buf_checksum(uint64_t b_u64, uint64_t a1, uint64_t a2,
                                      uint64_t a3, uint64_t a4, uint64_t a5) {
    (void)a1; (void)a2; (void)a3; (void)a4; (void)a5;
    // b_u64 points to the local shadow of struct toy_buffer.
    // b->data points to a local shadow of the data buffer (auto-marshalled).
    typedef struct { char *data; unsigned len; } ToyBuf;
    ToyBuf *b = (ToyBuf *)(uintptr_t)b_u64;

    int sum = 0;
    for (unsigned i = 0; i < b->len; i++)
        sum += (unsigned char)b->data[i];

    printf("[Grate|auto-nested] toy_buf_checksum intercepted: len=%u sum=%d -> %d\n",
           b->len, sum, sum + 1);
    return LIND_RET_INT(sum + 1);
}

LIND_DEFINE_MARSHAL_HANDLER(toy_buf_checksum, &buf_checksum_spec, handler_buf_checksum)

int pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid,
                    uint64_t arg1, uint64_t arg1cage,
                    uint64_t arg2, uint64_t arg2cage,
                    uint64_t arg3, uint64_t arg3cage,
                    uint64_t arg4, uint64_t arg4cage,
                    uint64_t arg5, uint64_t arg5cage,
                    uint64_t arg6, uint64_t arg6cage) {
    if (fn_ptr_uint == 0) { fprintf(stderr, "[Grate|auto-nested] invalid fn ptr\n"); assert(0); }
    int (*fn)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
              uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
              uint64_t, uint64_t, uint64_t) =
        (int (*)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
                 uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
                 uint64_t, uint64_t, uint64_t))(uintptr_t)fn_ptr_uint;
    return fn(cageid, arg1, arg1cage, arg2, arg2cage,
              arg3, arg3cage, arg4, arg4cage, arg5, arg5cage, arg6, arg6cage);
}

int main(int argc, char *argv[]) {
    if (argc < 2) { fprintf(stderr, "Usage: %s <cage>\n", argv[0]); assert(0); }
    int grateid = getpid();
    pid_t pid = fork();
    if (pid < 0) { perror("fork"); assert(0); }
    if (pid == 0) {
        int cageid = getpid();
        printf("[Grate|auto-nested] registering toy_buf_checksum for cage %d\n", cageid);
        int ret = register_lib_handler(cageid, "env", "toy_buf_checksum",
                      grateid, (uint64_t)(uintptr_t)&lind_mh_toy_buf_checksum);
        if (ret != 0) { fprintf(stderr, "[Grate|auto-nested] register failed\n"); assert(0); }
        if (execv(argv[1], &argv[1]) == -1) { perror("execv"); assert(0); }
    }
    int status;
    while (wait(&status) > 0) {}
    int child_exit = WIFEXITED(status) ? WEXITSTATUS(status) : -1;
    if (child_exit != 0) {
        fprintf(stderr, "[Grate|auto-nested] FAIL: cage exited %d\n", child_exit);
        return 1;
    }
    printf("[Grate|auto-nested] PASS\n");
    return 0;
}
