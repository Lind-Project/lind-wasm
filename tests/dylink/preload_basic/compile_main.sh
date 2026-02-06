clang \
    -pthread \
    -fPIC \
    --target=wasm32-unknown-wasi \
    --sysroot /home/lind/lind-wasm/src/glibc/sysroot \
    -Wl,-pie \
    -Wl,--import-table \
    -Wl,--import-memory \
    -Wl,--export-memory \
    -Wl,--max-memory=67108864 \
    -Wl,--export=__stack_pointer \
    -Wl,--export=__stack_low \
    -Wl,--allow-undefined \
    -Wl,--unresolved-symbols=import-dynamic \
    main.c -g -O0 -o main.wasm

# emscripten reference:
# /home/lind/emscripten_dl/emsdk/upstream/bin/clang \
#     -target wasm32-unknown-emscripten \
#     -fignore-exceptions \
#     -fPIC \
#     -fvisibility=default \
#     -mllvm -combiner-global-alias-analysis=false \
#     -mllvm -enable-emscripten-sjlj \
#     -mllvm -disable-lsr \
#     --sysroot=/home/lind/emscripten_dl/emsdk/upstream/emscripten/cache/sysroot \
#     -DEMSCRIPTEN \
#     -Xclang -iwithsysroot/include/fakesdl \
#     -Xclang -iwithsysroot/include/compat \
#     -v \
#     -c main.c \
#     -o /tmp/emscripten_temp_220l95uj/main_0.o

# /home/lind/emscripten_dl/emsdk/upstream/bin/wasm-ld \
#     -o main.wasm \
#     /tmp/emscripten_temp_220l95uj/main_0.o \
#     libtmp.so \
#     -L/home/lind/emscripten_dl/emsdk/upstream/emscripten/cache/sysroot/lib/wasm32-emscripten/pic \
#     -L/home/lind/emscripten_dl/emsdk/upstream/emscripten/src/lib \
#     -lGL-getprocaddr \
#     -lal \
#     -lhtml5 \
#     -lstubs-debug \
#     -lnoexit \
#     -lc-debug \
#     -ldlmalloc-debug \
#     -lcompiler_rt \
#     -lc++-noexcept \
#     -lc++abi-debug-noexcept \
#     -lsockets \
#     -mllvm -combiner-global-alias-analysis=false \
#     -mllvm -enable-emscripten-sjlj \
#     -mllvm -disable-lsr \
#     --export-if-defined=setThrew --export-if-defined=_emscripten_stack_restore --export-if-defined=emscripten_stack_get_current --export-if-defined=calloc --export-if-defined=_emscripten_stack_alloc --export-if-defined=_emscripten_tempret_get --export-if-defined=_emscripten_tempret_set --export-if-defined=strerror --export-if-defined=htons --export-if-defined=ntohs --export-if-defined=malloc --export-if-defined=htonl --export-if-defined=_emscripten_timeout --export-if-defined=emscripten_stack_get_base --export-if-defined=emscripten_stack_get_end --export-if-defined=free --export-if-defined=__cxa_can_catch --export-if-defined=__cxa_increment_exception_refcount --export-if-defined=__cxa_get_exception_ptr --export-if-defined=__cxa_decrement_exception_refcount --export-if-defined=fileno --export-if-defined=emscripten_builtin_memalign --export-if-defined=__dl_seterr --export-if-defined=memcmp --export-if-defined=memcpy --export-if-defined=realloc --export-if-defined=__errno_location --export-if-defined=__cxa_demangle /tmp/tmpq23lywijlibemscripten_js_symbols.so --import-memory --strip-debug --export=emscripten_stack_get_end --export=emscripten_stack_get_free --export=emscripten_stack_get_base --export=emscripten_stack_get_current --export=emscripten_stack_set_limits --export=_emscripten_stack_alloc --export=__wasm_call_ctors --export=setThrew --export=_emscripten_stack_restore --export=calloc --export=_emscripten_tempret_get --export=_emscripten_tempret_set --export=strerror --export=htons --export=ntohs --export=malloc --export=htonl --export=_emscripten_timeout --export=free --export=__cxa_can_catch --export=__cxa_increment_exception_refcount --export=__cxa_get_exception_ptr --export=__cxa_decrement_exception_refcount --export=fileno --export=emscripten_builtin_memalign --export=__dl_seterr --export=memcmp --export=memcpy --export=realloc --export=__errno_location --export=__cxa_demangle --export-if-defined=__start_em_asm --export-if-defined=__stop_em_asm --export-if-defined=__start_em_lib_deps --export-if-defined=__stop_em_lib_deps --export-if-defined=__start_em_js --export-if-defined=__stop_em_js --export-if-defined=main --export-if-defined=__main_argc_argv --export-if-defined=__wasm_apply_data_relocs --export-if-defined=fflush --export-if-defined=__memory_base --export-if-defined=__stack_pointer --export-if-defined=__table_base --export-if-defined=data --export-if-defined=lib_function --export-if-defined=printf --export-if-defined=var \
#     --experimental-pic \
#     --unresolved-symbols=import-dynamic \
#     -pie \
#     --no-export-dynamic \
#     -z stack-size=65536 \
#     --no-growable-memory \
#     --initial-memory=16777216 \
#     --no-entry \
#     --stack-first