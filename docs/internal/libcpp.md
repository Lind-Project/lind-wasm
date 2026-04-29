# Start from commit [commit 337a15a](https://github.com/Lind-Project/lind-wasm/commit/337a15abc0d5a97a5050a46e50d1b69550181842) of libcpp-alice branch

## Overarching Goal
To be able to compile .wasm binary (and the .cwasm ELF files as a by-product) with clang++ compiler inside the Lind sandbox. 
Where we start with
Alice's issue [#245](https://github.com/Lind-Project/lind-wasm/issues/245) describes a slightly outdated solution to the above problem, Alice later started a new branch (libcpp-alice) and basically formalized that solution. This documentation by Ren will attempt to explain her solution (grossly) and document what is exactly done to achieve the overarching goal. 

## Existing problem
Because we are really mixing the compilation environment for LLVM, Clang, and the binary developers will eventually want to compile from their own source code, we must introduce external dependencies, specifically LLVM ver 18 and its compiled binary. Moreover, there are specific source code modifications made to the compiled llvm headers done by Alice. The workflow of curl llvm --> modification of llvm source code --> compilation --> cp the archive files into sysroot is overcomplicated.

## Proposed solution
Solution proposed by both Alice and Tasha: on local repo, curl, modify and compile llvm; then, create an artifacts/ directory that only stores necessary compiled archives; remove llvm and all other non-essential compiled artifacts and only preserve the necessary ones; then add Makefile command to push-buton copy over the archive artifacts to a compiled sysroot whenever needed. This workflow is relatively small, and easier to implemenent with Github Actions for CICD purposes.

## What i did
### Replicate the error
Of course, the first step of dealing with a problem is to make sure we do have a problem. We start with what Nick points out in issue [#740](https://github.com/Lind-Project/lind-wasm/issues/740#issuecomment-3910086697): that some dummy .cpp file that has # include <algorithm> will fail to compile as the header libraries are just entirely missing, if a user attempts to compile .wasm binary. Thus, I put a super simple dummy hello.cpp file in tests/unit-tests/cpp/ dir, which uses algorithm header:

```trial/hello.cpp
#include <algorithm>
#include <iostream>
#include <vector>

int main(int argc, char **argv) {
    (void)argc;
    (void)argv;
    std::vector<int> v = {3, 1, 2};
    std::sort(v.begin(), v.end());

    if (v != std::vector<int>{1, 2, 3}) {
        std::cout << "LIBCPP_SORT_FAIL" << std::endl;
        return 1;
    }

    std::cout << "LIBCPP_SORT_OK 1 2 3" << std::endl;
    return 0;
}
```
note: main function parameter list signature is very rigid as omiting it causes compiler warnings (not fatal); a simple stdout is used for easier test report generation. 

### Side note
Compiling .wasm binary involves setting some pretty length and boring flags for clang++ command, the lind-wasm repo already provides a pretty convenient compile script in ``scripts/lind_compile`` which basically bundles up those flags for you. Here are a couple caveats:
The script can only be used inside docker container
The script call signature is iffy, you must call it as: ``./scripts/lind_compile main.cpp`` and cannot assume its global availability: ``./lind_compile main.cpp`` **will not do what you want**.
The script is intended to be used on .c files only. I have made a spin-off ``lind_compile_cpp`` script based on it. A future improvevement would be to look at all flags allowed by that script and verify their functionalities.


So, the necessary steps here are: creating and stepping into a docker container, creating the dummy hello.cpp file (you can do this before stepping into the docker container step; it really doesn't matter), and attempt to compile with the script and just watch it fail.

```bash
lind@e9a40a72b750:~/lind-wasm$ scripts/lind_compile trial/hello.cpp 
/home/lind/lind-wasm/trial/hello.cpp:1:10: fatal error: 'algorithm' file not found
    1 | #include <algorithm>
      |          ^~~~~~~~~~~
1 error generated.
```

This error is also recorded in issue [#740](https://github.com/Lind-Project/lind-wasm/issues/740#issuecomment-3910086697) by Nick, as mentioned above.

## Docker Container 
Here I must assume you have your SSH and IDE set up already. If not, refer to the first tab of this documentation file. And really just ask for help from other PHD students. We start with you already ssh connected to the server, and already git-cloned the project repo on libcpp-alice branch.

```bash
docker run --privileged --ipc=host --cap-add=SYS_PTRACE --name WHATEVER -it securesystemslab/lind-wasm-dev /bin/bash
```
Just remember to name your container whatever unique name you want. In the future, you can find it by simply running docker ps and whenever you exit and want to come back to it, simply:

```bash
docker exec -it WHATEVER /bin/bash
```
Here, obviously, we never really pull down a docker container – one of the many perks of working on a remote SSH. 

Creating the dummy hello.cpp, and try to compile
This step is pretty self-explanatory. Just remember that we want to compile while assuming everything requires absolute path, so:

```bash
/home/lind/lind-wasm/scripts/lind_compile /home/lind/lind-wasm/trial/hello.cpp
```
 Then you will quickly see the error described in issue [#740](https://github.com/Lind-Project/lind-wasm/issues/740#issuecomment-3910086697).

## Retrieving external dependencies
If you look at the .gitignore, here and here we see clang+llvm ignored; I retrieved this via curl command from llvm; and you have llvm-project ignored, which is just outright the entire llvm repo, so git clone. We really don have a good reason behind the specific versions we picked for both of them, it is just a hassle must-do. The other suspicious looking gitignored items are runtime generated and I will go over them soon. For now, make sure you are in ``/lind-wasm`` directory. Run:

```bash
git clone --branch release/18.x --single-branch https://github.com/llvm/llvm-project.git
```
The above step is also mentioned in issue [#245](https://github.com/Lind-Project/lind-wasm/issues/245); and it will create ``/llvm-project`` dir for us, resolving one of the two missing dependencies issue. Again, specifically ver 18 is used. (contrary to the issue [#245](https://github.com/Lind-Project/lind-wasm/issues/245) which claims ver 16)

Next, clang+llvm compiled library, this is trickier:

```bash
mkdir -p ~/tools/llvm18 && cd ~/tools/llvm18
curl -L -o llvm18.tar.xz "https://github.com/llvm/llvm-project/releases/download/llvmorg-18.1.8/clang+llvm-18.1.8-x86_64-linux-gnu-ubuntu-18.04.tar.xz"
tar -xf llvm18.tar.xz
```
Where specifically the compiled 18.1.8 version is used. Again, don ask me why. I have no answer to that.
 
Now, both external dependencies are retrieved. Note that these are big, clunky, and will be removed once we get the desired archive files compiled and stored somewhere else.

## Modification of LLVM source code

This modification, according to Alice, is developed retroactively after encountering compilation errors regarding return type of a certain util function. I am not sure how much "move fast and break things" we have wreaked by doing it. Modify llvm-project/libcxx/src/filesystem/time_utils.h
and somewhere near line 266 (exactly this line at the moment) you should see the definition of convert_to_timespec() function, its return statement should be modified by adding reinterpret_cast<long*>  and you should end up with something like this:

```/home/lind/lind-wasm/llvm-project/libcxx/src/filesystem/time_utils.h

return set_times_checked(reinterpret_cast<long*>(&dest.tv_sec),
                         reinterpret_cast<long*>(&dest.tv_nsec),
                         tp);
```

And, contrary to issue #245, we need no more modifications on other external library source code. (Such as on clang+llvm's xlocale.h)

## Call build script and migrate .a archives
Now, with both our external dependencies in good shape, we can finally use ./build-llvm.sh to compile the libcxx-wasi library. Once you call this script, /libcxx-wasi-install dir is created. Next, we need to manually copy over the generated archives into sysroot:

```bash
cp -r /home/lind/lind-wasm/libcxx-wasi-install/include/c++ \
      /home/lind/lind-wasm/build/sysroot/include/wasm32-wasi/

cp /home/lind/lind-wasm/libcxx-wasi-install/lib/libc++.a \
   /home/lind/lind-wasm/libcxx-wasi-install/lib/libc++abi.a \
   /home/lind/lind-wasm/build/sysroot/lib/wasm32-wasi/

```

### Side note
We are missing a libunwind.a archive; it is, for native cpp binary, needed to handle throw-except syntax. I tried to modify our CMake script and the build script to have this archive generated and then linked against the compilation process, and discovered that for .wasm binary, libunwind is not the correct dependency used to provide that syntax support. Currently I have no solution to it.

## Last step: test compile and it should work now
At this point, we have all we need to make the .wasm compilation work. Manually setting the correct clang++ flags is too much work, and luckily we have a scripts/lind_compile script which, originally designed for .c compilation, is almost entirely reusable directly for our .cpp compilation. Again, Alice already made the necessary changes to it in her commit to repurpose it for .cpp compilation only, and so we only need to use it. One more thing: remember we cannot support throw-exception? We do need some manual flag-setting to suppress that part. Luckily our simple dummy program does not need the throw-except syntax anyways. now make sure your are in lind-wasm dir (or just use absolute path for bash script below if you are not – by this point you should be really familiar with the project file hierarchy already.)

```bash
scripts/lind_compile_cpp tests/unit-tests/cpp/hello.cpp
```
note that, I have added ``-fno-exceptions`` flag in that script. The throw-exception syntax is not supported at the moment, and that would be a **major area of improvement in the future**
note you can also additionally add -fno-rtti flag to save memory and speed up the compilation a bit more, but the compilation is quite slow regardless (takes ~1 minute)

And you should now have compiled result file:

tests/unit-tests/cpp/hello.cpp.wasm

### side note

exact fail message without ``-fno-exceptions`` flag:

```bash
lind@e9a40a72b750:~/lind-wasm$ scripts/lind_compile trial/hello.cpp # deprecated 
wasm-ld: warning: function signature mismatch: main
>>> defined as (i32, i32, i32) -> i32 in /home/lind/lind-wasm/build/sysroot/lib/wasm32-wasi/crt1.o
>>> defined as (i32, i32) -> i32 in /tmp/hello-d53fbc.o

wasm-ld: error: /tmp/hello-d53fbc.o: undefined symbol: __cxa_allocate_exception
wasm-ld: error: /tmp/hello-d53fbc.o: undefined symbol: __cxa_throw
wasm-ld: error: /tmp/hello-d53fbc.o: undefined symbol: __cxa_allocate_exception
wasm-ld: error: /tmp/hello-d53fbc.o: undefined symbol: __cxa_throw
clang++: error: linker command failed with exit code 1 (use -v to see invocation)
```

which is a result of missing libunwind.a archive support. I looked into it and it seems it cannot be simply fixed by tweaking the compile flags in the Toolchain-WASI.cmake. This is something to be worked on in the future

Also, the lind_run command on the compiled binary currently fails. This is another important thing to look into:
```bash
lind@e9a40a72b750:~/lind-wasm$ lind_run tests/unit-tests/cpp/hello.cpp.cwasm 
failed to compile module

Caused by:
    0: failed to read input file: tests/unit-tests/cpp/hello.cpp.cwasm
    1: No such file or directory (os error 2)
```

**And that is the entire workflow to get .wasm binary compiled!**

related issue: [#795](https://github.com/Lind-Project/lind-wasm/issues/795)

## Local Testing

Short path to validate the integrated libc++ smoke check (including native-vs-wasm parity):

```bash
# 1) Prepare runtime libs used by lind_run preloads
make sysroot

# 2) Run only wasm harness (this also runs libc++ integration)
python3 scripts/test_runner.py --harness wasmtestreport
```

Expected result in `reports/wasm.json`:
- top-level `libcpp` object exists
- `libcpp.number_of_failures` is `0`
- `libcpp.test_cases["tests/unit-tests/cpp/hello.cpp"].output` contains:
  - `Native/Wasm parity verified`
  - `LIBCPP_SORT_OK 1 2 3`