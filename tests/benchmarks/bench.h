// Helper macros for IPC and FS test message sizes.
#define KiB(x) ((size_t)(x) << 10)
#define MiB(x) ((size_t)(x) << 20)

// Iteration constants
#define IO_LOOPS_SMALL 10000
#define IO_LOOPS_LARGE 1000000
#define IO_THRESHOLD 4096

#define IO_LOOP_COUNT(size) ((size) > IO_THRESHOLD ? IO_LOOPS_SMALL : IO_LOOPS_LARGE)

#define FS_SIZE_COUNT	(sizeof(fs_sizes)/sizeof(fs_sizes[0]))
#define IPC_SIZE_COUNT	(sizeof(ipc_sizes)/sizeof(ipc_sizes[0]))

extern int fs_sizes[4];
extern int ipc_sizes[4];

// Monotonic timer in nanoseconds for microbenchmarks.
long long gettimens();

// Print one benchmark row in benchrunner.py's tab-delimited format:
// <test>\t<param>\t<loops>\t<avg_ns>
void emit_result(char* test, int param, long long average, int loops);

void emit_result_string(char* test, char* param, long long average, int loops);
