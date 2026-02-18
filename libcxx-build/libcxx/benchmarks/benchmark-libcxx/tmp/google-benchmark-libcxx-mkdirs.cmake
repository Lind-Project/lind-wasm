# Distributed under the OSI-approved BSD 3-Clause License.  See accompanying
# file LICENSE.rst or https://cmake.org/licensing for details.

cmake_minimum_required(VERSION ${CMAKE_VERSION}) # this file comes with cmake

# If CMAKE_DISABLE_SOURCE_CHANGES is set to true and the source directory is an
# existing directory in our source tree, calling file(MAKE_DIRECTORY) on it
# would cause a fatal error, even though it would be a no-op.
if(NOT EXISTS "/home/ren/projects/lind-wasm/llvm-project/runtimes/../third-party/benchmark")
  file(MAKE_DIRECTORY "/home/ren/projects/lind-wasm/llvm-project/runtimes/../third-party/benchmark")
endif()
file(MAKE_DIRECTORY
  "/home/ren/projects/lind-wasm/libcxx-build/libcxx/benchmarks/benchmark-libcxx/src/google-benchmark-libcxx-build"
  "/home/ren/projects/lind-wasm/libcxx-build/libcxx/benchmarks/benchmark-libcxx"
  "/home/ren/projects/lind-wasm/libcxx-build/libcxx/benchmarks/benchmark-libcxx/tmp"
  "/home/ren/projects/lind-wasm/libcxx-build/libcxx/benchmarks/benchmark-libcxx/src/google-benchmark-libcxx-stamp"
  "/home/ren/projects/lind-wasm/libcxx-build/libcxx/benchmarks/benchmark-libcxx/src"
  "/home/ren/projects/lind-wasm/libcxx-build/libcxx/benchmarks/benchmark-libcxx/src/google-benchmark-libcxx-stamp"
)

set(configSubDirs )
foreach(subDir IN LISTS configSubDirs)
    file(MAKE_DIRECTORY "/home/ren/projects/lind-wasm/libcxx-build/libcxx/benchmarks/benchmark-libcxx/src/google-benchmark-libcxx-stamp/${subDir}")
endforeach()
if(cfgdir)
  file(MAKE_DIRECTORY "/home/ren/projects/lind-wasm/libcxx-build/libcxx/benchmarks/benchmark-libcxx/src/google-benchmark-libcxx-stamp${cfgdir}") # cfgdir has leading slash
endif()
