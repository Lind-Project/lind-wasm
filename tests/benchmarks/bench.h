// Helper macros for IPC and FS test message sizes.
#define KiB(x) ((size_t)(x) << 10)
#define MiB(x) ((size_t)(x) << 20)

// Monotonic timer in nanoseconds for microbenchmarks.
long long gettimens();

// Print one benchmark row in benchrunner.py's tab-delimited format:
// <test>\t<param>\t<loops>\t<avg_ns>
void emit_result(char* test, int param, long long average, int loops);
