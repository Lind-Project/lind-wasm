// Real implementation linked into the grate so it can call toy_argv_len locally.
int toy_argv_len(const char **argv) {
    int total = 0;
    for (int i = 0; argv[i] != 0; i++) {
        const char *s = argv[i];
        while (*s) { total++; s++; }
    }
    return total;
}
