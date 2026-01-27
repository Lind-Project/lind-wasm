# Signal Implementation Design Documentation

## 1. Binary Rewriting

As we do not have a way to interrupt a running WebAssembly thread without directly using kernel functions like `pthread_kill`, our approach inserts signal checks into the Wasm binary. These checks allow the binary to spontaneously callback to the host when the host indicates there are pending signals via an [epoch mechanism](https://docs.wasmtime.dev/examples-interrupting-wasm.html). The inserted signal checks detect changes in the epoch value, which is managed by Wasmtime’s existing epoch insertion infrastructure at the Cranelift IR level.

However, this approach is incompatible with Asyncify, the tool we use to support multi-processing, as Asyncify operates at the Wasm level. To have both epoch-based signal handling and Asyncify-based multi-processing work together, we had two options:

* Modifying Wasmtime’s IR-level epoch insertion to make it compatible with Asyncify.
* Implementing our own Wasm-level epoch insertion.

We chose the latter, as implementing our own Wasm-level epoch insertion is simpler and ensures compatibility with Asyncify automatically.

## 2. Epoch Management

Epoch management must be carefully handled to ensure correct signal delivery and processing. The epoch can be in one of three states:

* **`Normal` state**: No pending signals.
* **`Signal` state**: A pending signal needs to be handled.
* **`Kill` state**: The thread needs to be terminated.

When the epoch transitions to either the `signal` state or the `kill` state, execution jumps to a callback function in the host. The host then determines the appropriate action based on the signal type. For example, a `SIGKILL` will immediately terminate the process, whereas other signals may invoke custom guest-defined handlers.

During the execution of a signal handler, the epoch must be reset to the `normal` state to prevent unintended interruptions inside the handler. However, this reset only occurs when there are no more unblocked pending signals. In other words, additional pending signals will continue to interrupt the current signal handler until all the signals are handled.

Whenever a new signal is delivered, the following occurs:

1. If the signal has no disposition set and its default action is to ignore, the signal is dropped immediately and is not added to the pending signal list.
2. If the signal is not blocked, the epoch state is immediately set to the `signal` state, ensuring the signal is processed promptly.
3. When the epoch is triggered, the host retrieves the first unblocked signal from the pending list and invokes the corresponding signal handler.
4. The epoch state remains in the `signal` state until all pending (unblocked) signals are processed.
5. New signals received during handler execution are appended to the pending list and will be processed before earlier signals, mirroring Linux’s behavior.

In case of a new signal delivered during the execution of the signal handler, we do not need to take any special consideration. It will be appended to the pending signal list normally and switch epoch to the `signal` state. This will always make the latest signal being handled first, similar to Linux’s behavior.

## 3. Epoch-Based Signal Handling with Asyncify

One key challenge in integrating epoch-based signal handling with Asyncify is ensuring compatibility, particularly when the system is in rewind state. If a signal handler interacts with Asyncify, the entire call stack—including the epoch callback function within the host—must be compatible with Asyncify’s transformation logic.

To achieve this, we manually apply Asyncify transformation to our host epoch callback function. Since we support recursive signal handling, we must maintain a record of:

* The order of executed signal handlers within the call stack.
* The parameters passed to each handler.

If a signal handler returns due to an Asyncify unwind operation, we must detect this condition and immediately break the loop processing pending signals, returning control to the unwinding call stack.

If the epoch callback function is reached while in Asyncify rewind state, we must detect the rewind state and skip the normal epoch handling logic, resuming the call stack by directly invoking the last active signal handler with its remembered parameters.

## 4. `sigaction` and `sigprocmask`

The `sigaction` and `sigprocmask` syscalls do not need to directly interact with the epoch mechanism. Instead, they can be safely stored in the process’s cage structure until needed by other syscalls. The `sigprocmask` information is checked when processing pending signals to determine whether a signal should be skipped due to being blocked.

A notable aspect of `sigprocmask` handling is managing signals that are unblocked. If a signal is unblocked while it is still pending, the epoch state should immediately transition to the `signal` state, similar to when a new signal is received.

When a signal handler is executed, we must temporarily block signals specified in the `sa_mask` field of `sigaction`. Once the signal handler finishes execution, we restore the signal mask to its previous state, overriding any modifications made during the execution of the signal handler. This behavior is consistent with Linux, which forcibly restores the signal mask after a handler completes—even if `sigprocmask` was modified inside the handler.

By default, the same signal will be blocked during its execution of its handler. Therefore, we explicitly add the same signal to `sa_mask`, preventing the handler from being re-entered while it is executing. To support `SA_NODEFER`, we can simply unset the same signal in the mask.

We also support `SA_RESETHAND` by resetting the signal handler to its default state once the signal is handled, ensuring the handler is executed only once.

## 5. Threads Termination

We introduced the **`kill` state** in the epoch mechanism primarily to enable terminating all running threads within a cage when necessary. The `kill` state uses the same host callback function as the `signal` state, but it is explicitly checked at the beginning of the callback function to perform a suicide operation if required.

To support this, we need to store the epoch handler for each thread in the cage to be able to update the epoch state for all threads simultaneously.

The suicide operation is implemented using Wasmtime’s internal trap mechanism. By raising a special trap within the thread, Wasmtime can intercept and distinguish it from regular traps caused by faults such as segmentation faults. If the trap originates from the epoch mechanism, it is ignored, and the WebAssembly instance exits cleanly as if it terminated normally.

Thread termination is essential for handling signals like `SIGKILL` correctly, as `SIGKILL` must terminate all threads in a process.

## 6. Main Thread Management

Since per-thread signals are not currently supported, signal-related structures such as the `sigaction` state and signal mask state are shared among all threads within a cage. When a signal is delivered, one thread must handle it while the others continue running normally. To achieve this, we designate a **main thread** responsible for processing all signals.

By default, the main thread is the first thread spawned in the cage. However, if the main thread exits while other threads are still running, a new main thread must be selected. In this case, we can simply choose a random running thread as the new main thread.

## TODOs

* **Use the new epoch-based method for implementing the exit syscall**: Since we already have the infrastructure to terminate all threads within a cage, this mechanism should be applicable for handling the exit syscall. However, a minor issue remains regarding how to properly propagate the exit code upstream, which has not yet been implemented in the existing codebase.
* **Add an epoch check in the host immediately after a syscall completes and before returning to the guest**: Linux performs a signal check before transitioning from kernel mode to user mode, and we can adopt a similar approach to align our implementation more closely with Linux. One challenge is ensuring compatibility with Asyncify in the syscall path, as introducing another function in the call stack requires careful manual Asyncify transformation.
* **Support for `SIGSTOP` and `SIGCONT`**: We intend to support `SIGSTOP` and `SIGCONT`, which can be implemented by making the WebAssembly thread sleep and wake up accordingly. This should be straightforward.
* **Enable signal interruption during syscalls**: To support signal handling during blocking syscalls, we can modify all blocking syscalls to use a timeout-based version that periodically checks for signals. Additionally, the `SA_RESTART` flag could be a useful feature to implement in the future.
