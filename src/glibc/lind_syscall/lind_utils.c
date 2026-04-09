__attribute__((export_name("__get_aligned_tls_size")))
int __get_aligned_tls_size()
{
    int tls_size = __builtin_wasm_tls_size();
    int tls_align = __builtin_wasm_tls_align();

    int tls_align_mask = tls_align - 1;
    return (tls_size + tls_align_mask) & (~tls_align_mask);
}
