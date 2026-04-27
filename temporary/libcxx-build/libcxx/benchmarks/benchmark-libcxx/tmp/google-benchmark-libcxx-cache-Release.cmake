
set(CMAKE_C_COMPILER "/home/lind/lind-wasm/clang+llvm-18.1.8-x86_64-linux-gnu-ubuntu-18.04/bin/clang" CACHE STRING "Initial cache" FORCE)
set(CMAKE_CXX_COMPILER "/home/lind/lind-wasm/clang+llvm-18.1.8-x86_64-linux-gnu-ubuntu-18.04/bin/clang++" CACHE STRING "Initial cache" FORCE)
set(CMAKE_BUILD_TYPE "RELEASE" CACHE STRING "Initial cache" FORCE)
set(CMAKE_INSTALL_PREFIX "/home/lind/lind-wasm/libcxx-build/libcxx/benchmarks/benchmark-libcxx" CACHE PATH "Initial cache" FORCE)
set(CMAKE_CXX_FLAGS "-Wno-unused-command-line-argument -nostdinc++ -isystem /home/lind/lind-wasm/libcxx-build/include/c++/v1 -L/home/lind/lind-wasm/libcxx-build/lib -Wl,-rpath,/home/lind/lind-wasm/libcxx-build/lib -L/home/lind/lind-wasm/libcxx-build/lib -Wl,-rpath,/home/lind/lind-wasm/libcxx-build/lib" CACHE STRING "Initial cache" FORCE)
set(BENCHMARK_USE_LIBCXX "ON" CACHE BOOL "Initial cache" FORCE)
set(BENCHMARK_ENABLE_TESTING "OFF" CACHE BOOL "Initial cache" FORCE)