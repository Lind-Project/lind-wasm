"""Core policy, 3i routing, and simulated runtime mechanisms; no native execution."""

from dataclasses import dataclass, field
from typing import Any


OPERATIONS = {
    "BelongsToRuntime", "GetHostAddress", "GetGuestAddress", "GetCageABI",
    "GetRuntimeXYZfromLindXYZ", "GetLindXYZfromRuntimeXYZ", "SetIovecBase",
    "SetGuestIovecBase", "InitCage", "ApplyStagingCage", "DuplicateCage",
    "TerminateCage", "CreateThread", "DuplicateThread", "TerminateThread",
    "SendSignal", "RunHandler", "DoMmap", "DoMprotect", "DoMunmap",
    "3i::make_syscall",
}

LIMITATIONS = {
    "bootstrap": "InitCage requests its reservation through 3i before any guest thread exists. "
                 "The demo marks target_tid=null for that bootstrap request; the draft "
                 "still needs a precise threadless/sentinel identity convention.",
    "capture": "Wasm Asyncify cannot capture synchronously inside the current hostcall. "
               "This model assumes a driver/stack-switch boundary for syscall duplication; "
               "raw interrupted capture is rejected for wasm.",
    "targeting": "Thread-only STOP and asynchronous on-behalf-of capture are not finalized. "
                 "Raw capture here requires an already cage-stopped source and an explicit "
                 "no-supervisor-writes assertion; it injects no fork results.",
    "exec": "ApplyStagingCage error shape is open. The demo uses EIO before commit and "
            "EEXEC_COMMITTED plus cage death after the point of no return.",
    "fault": "Forced handler-fault propagation is open. Demo policy cancels return edges "
             "and kills suspended callers recursively, once, without waiting on handlers.",
    "memory": "CopyOutcome is provisional: {copied, data_hex, fault}. Copies use a source "
              "snapshot for aliases; real fault containment/mapping locks are not implemented. "
              "No application-consistent concurrent fork snapshot is promised.",
    "abi": "Demo ABI layouts are little-endian 32-bit wasm and 64-bit mpk iovecs only. "
           "Exec increments an image generation; stale ABI tokens are rejected.",
    "scheduler": "Deterministic ticks model independent per-runtime workers, not actual host "
                 "threads. This cannot prove fairness, blocking-syscall progress, deadlock "
                 "freedom, isolation, signal-frame correctness, or race safety.",
    "clone": "Only ordinary fork and pthread clone are modeled. Arbitrary Linux clone flags, "
             "vfork, fd semantics, per-thread signal masks and full TLS are out of scope.",
    "handler": "HandlerReturn/exec/fault types remain open. Calls stay visibly pending until "
               "return_handler; no ordinary result is delivered before then. A fork child "
               "cannot return through its inherited handler frame. Demo exec conservatively "
               "rejects all active invocations; safe handler-exec handoff needs a richer model.",
}


class DemoError(Exception):
    def __init__(self, code: str, message: str):
        super().__init__(message)
        self.code = code


def require(condition: bool, code: str, message: str) -> None:
    if not condition:
        raise DemoError(code, message)


@dataclass
class Mapping:
    address: int
    data: bytearray
    protection: str = "rw"
    shared: bool = False


@dataclass
class Thread:
    tid: int
    held: int | None = None
    blocked: list[int] = field(default_factory=list)
    handlers: list[int] = field(default_factory=list)
    forbidden_return: bool = False
    result: int | None = None
    tls: str = "initial"
    clear_tid: int | None = None


@dataclass
class Cage:
    cid: int
    runtime: str
    image: str
    parent: int | None = None
    staging: bool = False
    stopped: bool = False
    generation: int = 1
    base: int = 0
    size: int = 65536
    mappings: list[Mapping] = field(default_factory=list)
    threads: dict[int, Thread] = field(default_factory=dict)
    pending: list[tuple[str, int | None]] = field(default_factory=list)
    root: bool = False


@dataclass
class Invocation:
    iid: int
    caller: int
    caller_tid: int
    provider: int
    provider_tid: int
    target: int
    target_tid: int
    syscall: str


class Runtime:
    def __init__(self, name: str, suffix: str, width: int, raw_capture: bool):
        self.name, self.suffix, self.width = name, suffix, width
        self.raw_capture = raw_capture

    def call(self, world: "World", operation: str, **details: Any) -> None:
        world.emit(f"runtime:{self.name}", operation, **details)


class World:
    """Commands advance a single event loop; ticks expose scheduling, not real time."""

    def __init__(self):
        self.runtimes = {
            "wasm": Runtime("wasm", ".wasm", 4, False),
            "mpk": Runtime("mpk", ".elf", 8, True),
        }
        self.cages: dict[int, Cage] = {}
        self.routes: dict[tuple[int, str], int] = {}
        self.invocations: dict[int, Invocation] = {}
        self.barriers: dict[int, list[tuple[int, int]]] = {}
        self.exits: dict[int, int] = {}
        self.zombies: dict[int, dict[int, int]] = {}
        self.root_results: dict[int, int] = {}
        self.events: list[dict[str, Any]] = []
        self.last: Any = None
        self.next_id = 1
        self.next_base = 0x10000000
        for runtime in self.runtimes:
            self.emit(f"runtime:{runtime}", "worker.sleep", worker=f"{runtime}-worker")

    def emit(self, actor: str, operation: str, **details: Any) -> None:
        self.events.append(dict(n=len(self.events) + 1, actor=actor, op=operation, **details))

    def cage(self, cid: int) -> Cage:
        require(cid in self.cages, "ESRCH", f"no live cage {cid}")
        return self.cages[cid]

    def thread(self, cid: int, tid: int) -> Thread:
        cage = self.cage(cid)
        require(tid in cage.threads, "ESRCH", f"no thread {cid}:{tid}")
        return cage.threads[tid]

    def rt(self, cid: int) -> Runtime:
        return self.runtimes[self.cage(cid).runtime]

    def identifier(self) -> int:
        value = self.next_id
        self.next_id += 1
        return value

    def runnable(self, cage: Cage, thread: Thread) -> bool:
        waiting = any(iid not in thread.handlers for iid in thread.blocked)
        return not (cage.staging or cage.stopped or thread.held is not None or waiting)

    def init(self, cage: int, image: str, runtime: str | None = None,
             staging: bool = False, parent: int | None = None, root: bool = False) -> int:
        require(cage > 0 and cage not in self.cages and cage not in self.exits,
                "EEXIST", "cage IDs must be positive and unused")
        matches = []
        for candidate in self.runtimes.values():
            candidate.call(self, "BelongsToRuntime", image=image)
            if image.endswith(candidate.suffix):
                matches.append(candidate.name)
        require(bool(matches), "ENOEXEC", "no provider recognizes this image")
        runtime = runtime or matches[0]
        require(runtime in matches, "ENOEXEC", "runtime does not support image")
        if parent is not None:
            self.cage(parent)
        self.emit("core", "publish.records", cage=cage, staging=staging, root=root)
        record = Cage(cage, runtime, image, parent=parent, staging=staging, root=root)
        self.cages[cage] = record
        self.rt(cage).call(self, "InitCage", cage=cage, image=image, isStaging=staging)
        self.emit(f"runtime:{runtime}", "request.reservation", cage=cage, via="3i")
        self.emit("3i", "3i::make_syscall", caller=cage, target=cage, target_tid=None,
                  syscall="mmap", isReservation=True, bootstrap=True)
        self.emit("core", "authorize.reservation", cage=cage)
        record.base = self.next_base
        self.next_base += 0x100000
        self.rt(cage).call(self, "DoMmap", cage=cage, host=record.base,
                           size=record.size, isReservation=True)
        return cage

    def create(self, cage: int, tid: int = 1, fail: bool = False,
               run_before_return: bool = False, exit_before_return: int | None = None) -> int:
        c = self.cage(cage)
        require(tid > 0 and not c.threads, "EINVAL", "CreateThread needs an empty image and positive TID")
        self.emit("core", "lifetime.accounted", cage=cage, tid=tid)
        self.rt(cage).call(self, "CreateThread", cage=cage, tid=tid)
        if fail:
            self.emit("core", "lifetime.rollback", cage=cage, tid=tid)
            raise DemoError("EAGAIN", "injected worker/setup failure; no thread published")
        c.threads[tid] = Thread(tid)
        if run_before_return and self.runnable(c, c.threads[tid]):
            self.emit(f"runtime:{c.runtime}", "guest.step", cage=cage, tid=tid,
                      worker=f"{c.runtime}-worker", before_CreateThread_return=True)
        if exit_before_return is not None and self.runnable(c, c.threads[tid]):
            self.exit_thread(cage, tid, status=exit_before_return)
        schedulable = cage in self.cages and tid in c.threads and self.runnable(c, c.threads[tid])
        self.emit("core", "CreateThread.return", cage=cage, schedulable=schedulable)
        return tid

    def launch(self, cage: int, image: str, runtime: str | None = None) -> int:
        self.init(cage, image, runtime, root=True)
        self.create(cage)
        return cage

    def apply(self, staging: int, target: int, fail: bool = False) -> None:
        replacement = self.cage(staging)
        original = self.cage(target)
        require(staging != target and replacement.staging and bool(replacement.threads),
                "EINVAL", "apply needs a separate staging cage with its initial thread")
        require(not original.staging, "EINVAL", "exec target must be published")
        require(target not in self.routes.values(), "EBUSY",
                "exec would invalidate registered handler code")
        require(not any(i.provider == target or i.caller == target for i in self.invocations.values()),
                "EBUSY", "demo requires active invocations to finish before exec")
        self.rt(target).call(self, "SendSignal", cage=target, signal="STOP", internal_exec=True)
        self.emit("core", "exec.point_of_no_return", cage=target)
        self.rt(staging).call(self, "ApplyStagingCage", staging=staging, target=target)
        if fail:
            self.terminate(staging, force=True, record_exit=False)
            self.terminate(target, force=True, status=127)
            raise DemoError("EEXEC_COMMITTED", "post-commit failure; old continuation cannot return")
        self.rt(target).call(self, "TerminateCage", cage=target, old_image_only=True)
        self.discard_barriers(target)
        del self.cages[staging]
        replacement.cid, replacement.parent = target, original.parent
        replacement.staging, replacement.stopped = False, original.stopped
        replacement.generation, replacement.root = original.generation + 1, original.root
        self.cages[target] = replacement
        self.emit("core", "exec.publish", cage=target, runtime=replacement.runtime,
                  old_continuation="discarded", stopped=replacement.stopped)

    def map(self, cage: int, address: int, size: int, protection: str = "rw",
            shared: bool = False) -> int:
        c = self.cage(cage)
        require(size > 0 and 0 <= address < address + size <= c.size,
                "EINVAL", "mapping outside cage reservation")
        require(set(protection) <= set("rwx"), "EINVAL", "protection uses r/w/x or empty")
        require(not any(address < m.address + len(m.data) and m.address < address + size
                        for m in c.mappings), "EEXIST", "overlapping mapping")
        self.rt(cage).call(self, "DoMmap", cage=cage, host=c.base + address,
                           size=size, isReservation=False, shared=shared)
        c.mappings.append(Mapping(address, bytearray(size), protection, shared))
        return address

    def mapping(self, cage: int, address: int, size: int) -> Mapping:
        for m in self.cage(cage).mappings:
            if m.address <= address and address + size <= m.address + len(m.data):
                return m
        raise DemoError("EFAULT", f"unmapped cage {cage} address {address}")

    def protect(self, cage: int, address: int, size: int, protection: str) -> None:
        m = self.mapping(cage, address, size)
        require(address == m.address and size == len(m.data), "ENOTSUP",
                "demo protection/unmap requires a whole mapping")
        require(set(protection) <= set("rwx"), "EINVAL", "invalid protection")
        self.rt(cage).call(self, "DoMprotect", cage=cage,
                           host=self.cage(cage).base + address, size=size, protection=protection)
        m.protection = protection

    def unmap(self, cage: int, address: int, size: int) -> None:
        m = self.mapping(cage, address, size)
        require(address == m.address and size == len(m.data), "ENOTSUP",
                "demo protection/unmap requires a whole mapping")
        self.rt(cage).call(self, "DoMunmap", cage=cage,
                           host=self.cage(cage).base + address, size=size)
        self.cage(cage).mappings.remove(m)

    def translate(self, cage: int, address: int, reverse: bool = False) -> int:
        c = self.cage(cage)
        self.rt(cage).call(self, "GetGuestAddress" if reverse else "GetHostAddress",
                           cage=cage, address=address)
        guest = address - c.base if reverse else address
        require(0 <= guest < c.size, "EFAULT", "address outside reservation")
        return guest if reverse else c.base + guest

    def abi(self, cage: int) -> dict[str, Any]:
        require(cage in self.cages, "ENOENT", f"no ABI for cage {cage}")
        c = self.cage(cage)
        self.rt(cage).call(self, "GetCageABI", cage=cage)
        return dict(cage=cage, generation=c.generation, runtime=c.runtime,
                    width=self.rt(cage).width)

    def convert(self, cage: int, direction: str = "encode", base: int = 0,
                length: int = 0, data_hex: str = "", generation: int | None = None) -> Any:
        token = self.abi(cage)
        require(generation is None or generation == token["generation"], "ESTALE",
                "ABI token belongs to an old image")
        width = token["width"]
        require(direction in ("encode", "decode"), "EINVAL", "direction is encode or decode")
        operation = "GetRuntimeXYZfromLindXYZ" if direction == "encode" else "GetLindXYZfromRuntimeXYZ"
        self.emit("abi-library", operation, cage=cage, layout=f"iovec{width * 8}")
        if direction == "decode":
            data = bytes.fromhex(data_hex)
            require(len(data) == width * 2, "EFAULT", "incomplete local iovec buffer")
            return dict(base=int.from_bytes(data[:width], "little"),
                        length=int.from_bytes(data[width:], "little"))
        require(0 <= base < 1 << (width * 8) and 0 <= length < 1 << (width * 8),
                "EOVERFLOW", "iovec does not fit target ABI")
        return (base.to_bytes(width, "little") + length.to_bytes(width, "little")).hex()

    def access(self, cage: int, address: int, length: int, mode: str) -> tuple[bytes, dict | None]:
        self.cage(cage)
        require(0 <= length <= 65536 and address >= 0, "EINVAL", "invalid demo memory interval")
        result = bytearray()
        for offset in range(length):
            try:
                m = self.mapping(cage, address + offset, 1)
                require(mode in m.protection, "EFAULT", "permission denied")
            except DemoError as error:
                if error.code != "EFAULT":
                    raise
                return bytes(result), dict(owner=cage, address=address + offset,
                                           access=mode, cause=str(error))
            result.append(m.data[address + offset - m.address])
        return bytes(result), None

    def write(self, cage: int, address: int, data_hex: str) -> dict[str, Any]:
        self.emit("toolkit", "CopyToGuest", owner=cage, address=address)
        data = bytes.fromhex(data_hex)
        _, fault = self.access(cage, address, len(data), "w")
        count = len(data) if fault is None else fault["address"] - address
        for offset, value in enumerate(data[:count]):
            m = self.mapping(cage, address + offset, 1)
            m.data[address + offset - m.address] = value
        return dict(copied=count, fault=fault)

    def read(self, cage: int, address: int, length: int) -> dict[str, Any]:
        self.emit("toolkit", "CopyFromGuest", owner=cage, address=address)
        data, fault = self.access(cage, address, length, "r")
        return dict(copied=len(data), data_hex=data.hex(), fault=fault)

    def probe(self, cage: int, address: int, length: int, mode: str = "r") -> dict[str, Any]:
        require(mode in ("r", "w", "x"), "EINVAL", "access mode is r, w or x")
        self.emit("toolkit", "AccessGuestMemory", owner=cage, address=address, mode=mode,
                  stable_guard=False)
        data, fault = self.access(cage, address, length, mode)
        return dict(accessible=len(data), fault=fault, stable_guard=False)

    def copy(self, source: int, destination: int, source_address: int,
             destination_address: int, length: int) -> dict[str, Any]:
        self.cage(source)
        self.cage(destination)
        self.emit("3i", "copy_data_between_cages", source=source, destination=destination,
                  source_address=source_address, destination_address=destination_address, length=length)
        data, source_fault = self.access(source, source_address, length, "r")
        outcome = self.write(destination, destination_address, data.hex())
        outcome["data_hex"] = data[:outcome["copied"]].hex()
        outcome["fault"] = outcome["fault"] or source_fault
        return outcome

    def set_iovec(self, cage: int, address: int, base: int, remote: bool = False,
                  index: int = 0, data_hex: str = "") -> Any:
        width = self.rt(cage).width
        require(index >= 0 and 0 <= base < 1 << (width * 8), "EINVAL", "invalid iovec field")
        self.emit("abi-library", "SetGuestIovecBase" if remote else "SetIovecBase",
                  owner=cage, index=index)
        offset = index * width * 2
        encoded = base.to_bytes(width, "little")
        if not remote:
            data = bytearray.fromhex(data_hex)
            require(offset + width * 2 <= len(data), "EFAULT", "incomplete local iovec")
            data[offset:offset + width] = encoded
            return data.hex()
        self.emit("3i", "copy_data_between_cages", owner=cage, direction="accessor-write")
        return self.write(cage, address + offset, encoded.hex())

    def dispatch(self, caller: int, target: int, tid: int, syscall: str, **details: Any) -> None:
        self.cage(caller)
        self.thread(target, tid)
        self.emit("3i", "3i::make_syscall", caller=caller, target=target,
                  target_tid=tid, syscall=syscall, **details)

    def fork(self, source: int, child: int, tid: int = 1, caller: int | None = None,
             mode: str = "syscall", no_supervisor_writes: bool = False,
             fail: str | None = None, visible_result: int | None = None) -> int | None:
        parent = self.cage(source)
        thread = self.thread(source, tid)
        require(mode in ("syscall", "raw"), "EINVAL", "mode is syscall or raw")
        require(fail in (None, "memory", "thread"), "EINVAL", "unknown fork fault point")
        require(child > 0 and child not in self.cages and child not in self.exits,
                "EEXIST", "child ID already used")
        caller = source if caller is None else caller
        forwarding = mode == "syscall" and any(
            i.caller == source and i.caller_tid == tid and i.provider == caller
            for i in self.invocations.values()
        )
        require(thread.held is None and (not thread.blocked or forwarding),
                "EBUSY", "source continuation busy")
        require(mode != "syscall" or caller == source or forwarding, "ENOTSUP",
                "on-behalf syscall fork needs a suspended caller invocation; otherwise use raw capture")
        if mode == "raw":
            require(parent.stopped and no_supervisor_writes, "EBUSY",
                    "raw capture requires STOP and no_supervisor_writes=true")
            require(self.rt(source).raw_capture, "ENOTSUP",
                    "wasm raw interrupted capture is not available")
        self.dispatch(caller, source, tid, "fork", mode=mode)
        self.rt(source).call(self, "DuplicateCage", cage=source, newCage=child)
        duplicate = Cage(child, parent.runtime, parent.image, parent=source,
                         stopped=parent.stopped, generation=parent.generation, base=self.next_base)
        self.next_base += 0x100000
        duplicate.mappings = [
            Mapping(m.address, m.data if m.shared else bytearray(m.data), m.protection, m.shared)
            for m in parent.mappings
        ]
        self.cages[child] = duplicate
        for (owner, syscall), provider in list(self.routes.items()):
            if owner == source:
                self.routes[child, syscall] = provider
        if fail is not None:
            if fail == "thread":
                self.rt(source).call(self, "DuplicateThread", source=source, source_tid=tid,
                                     destination=child, new_tid=1, mode=mode)
            self.terminate(child, force=True, record_exit=False)
            raise DemoError("ENOMEM" if fail == "memory" else "EAGAIN", "fork rolled back")
        barrier = self.identifier() if mode == "syscall" else None
        self.rt(source).call(self, "DuplicateThread", source=source, source_tid=tid,
                             destination=child, new_tid=1, mode=mode, barrier=barrier)
        duplicate.threads[1] = Thread(1, held=barrier, forbidden_return=bool(thread.handlers),
                                      result=0 if mode == "syscall" else thread.result, tls=thread.tls)
        if barrier is not None:
            thread.held = barrier
            thread.result = child if visible_result is None else visible_result
            self.barriers[barrier] = [(source, tid), (child, 1)]
            self.emit("core", "TID.writes", parent=source, child=child, after="DuplicateThread",
                      before="3i.unwind", parent_result=thread.result, child_result=0)
        self.emit("core", "fork.publish", child=child, parent=source, mode=mode)
        return barrier

    def clone(self, cage: int, tid: int, new_tid: int, clear_tid: int | None = None) -> int:
        c = self.cage(cage)
        source = self.thread(cage, tid)
        require(new_tid > 0 and new_tid not in c.threads, "EEXIST", "TID already exists")
        require(source.held is None and not source.blocked, "EBUSY", "source busy")
        if clear_tid is not None:
            _, fault = self.access(cage, clear_tid, 4, "w")
            require(fault is None, "EFAULT", "clear-TID word inaccessible")
        self.dispatch(cage, cage, tid, "clone")
        barrier = self.identifier()
        self.rt(cage).call(self, "DuplicateThread", source=cage, source_tid=tid,
                           destination=cage, new_tid=new_tid, barrier=barrier, fresh_TLS=True)
        source.held = barrier
        source.result = new_tid
        c.threads[new_tid] = Thread(new_tid, held=barrier, result=0, tls=f"fresh:{new_tid}",
                                    clear_tid=clear_tid, forbidden_return=bool(source.handlers))
        self.barriers[barrier] = [(cage, tid), (cage, new_tid)]
        if clear_tid is not None:
            self.write(cage, clear_tid, new_tid.to_bytes(4, "little").hex())
        self.emit("core", "TID.writes", cage=cage, tid=new_tid, after="DuplicateThread",
                  before="3i.unwind")
        return barrier

    def unwind(self, barrier: int) -> None:
        require(barrier in self.barriers, "ENOENT", "unknown scheduling barrier")
        pairs = self.barriers[barrier]
        require(not any(self.thread(c, t).handlers or self.thread(c, t).blocked for c, t in pairs
                        if c in self.cages and t in self.cage(c).threads),
                "EBUSY", "unwind must leave all interposed handler frames first")
        self.emit("3i", "3i.unwind", barrier=barrier)
        for cid, tid in self.barriers.pop(barrier):
            if cid in self.cages and tid in self.cage(cid).threads:
                self.thread(cid, tid).held = None
        self.emit("core", "scheduling.point", barrier=barrier)

    def signal(self, cage: int, signal: str, tid: int | None = None) -> None:
        c = self.cage(cage)
        require(signal in ("STOP", "CONT", "KILL", "USR1", "TERM"), "EINVAL",
                "demo signals: STOP CONT KILL USR1 TERM")
        require(tid is None or signal not in ("STOP", "CONT"), "ENOTSUP",
                "thread-only stop convention remains unspecified")
        if tid is not None:
            self.thread(cage, tid)
        self.rt(cage).call(self, "SendSignal", cage=cage, tid=tid, signal=signal)
        if signal == "STOP":
            c.stopped = True
            self.emit("core", "STOP.ack", cage=cage, includes_handlers=True)
        elif signal == "CONT":
            c.stopped = False
        elif signal == "KILL":
            self.terminate(cage, force=True, status=137)
        else:
            c.pending.append((signal, tid))
            self.emit("core", "signal.accepted", cage=cage, signal=signal, delivered=False)

    def register(self, cage: int, syscall: str, provider: int) -> None:
        c = self.cage(provider)
        self.cage(cage)
        require(not c.staging, "EBUSY", "cannot publish staging handler code")
        self.routes[cage, syscall] = provider
        self.emit("3i", "register_handler", cage=cage, syscall=syscall, provider=provider)

    def unregister(self, cage: int, syscall: str) -> None:
        require((cage, syscall) in self.routes, "ENOENT", "no such handler")
        del self.routes[cage, syscall]
        self.emit("3i", "unregister_handler", cage=cage, syscall=syscall)

    def call(self, caller: int, syscall: str, target: int | None = None, tid: int = 1,
             caller_tid: int = 1, provider_tid: int = 1,
             arguments: list[dict[str, Any]] | None = None) -> Any:
        target = caller if target is None else target
        calling = self.thread(caller, caller_tid)
        require(self.runnable(self.cage(caller), calling), "EBUSY", "caller not runnable")
        self.dispatch(caller, target, tid, syscall, arguments=arguments or [])
        provider = self.routes.get((target, syscall))
        if provider is None:
            self.emit("rawposix", "syscall.return", target=target, syscall=syscall, result=0)
            return 0
        c = self.cage(provider)
        handler = self.thread(provider, provider_tid)
        require(self.runnable(c, handler), "EBUSY", "handler provider stopped or blocked")
        iid = self.identifier()
        self.invocations[iid] = Invocation(iid, caller, caller_tid, provider, provider_tid,
                                           target, tid, syscall)
        calling.blocked.append(iid)
        handler.handlers.append(iid)
        self.rt(provider).call(self, "RunHandler", invocation=iid, caller=caller,
                               cage=provider, tid=provider_tid, target=target, target_tid=tid,
                               arguments=arguments or [], synchronous=True)
        return iid

    def return_handler(self, invocation: int, result: int = 0, cage: int | None = None) -> int:
        require(invocation in self.invocations, "ENOENT", "invocation was cancelled or returned")
        i = self.invocations[invocation]
        cid = i.provider if cage is None else cage
        t = self.thread(cid, i.provider_tid if cid == i.provider else 1)
        require(not t.forbidden_return and cid == i.provider, "EPERM",
                "fork child cannot return into original caller")
        require(not self.cage(cid).stopped and not any(iid not in t.handlers for iid in t.blocked), "EBUSY",
                "handler stopped or waiting on nested call")
        require(t.handlers and t.handlers[-1] == invocation, "EBUSY", "nested handler must finish first")
        t.handlers.pop()
        self.thread(i.caller, i.caller_tid).blocked.remove(invocation)
        del self.invocations[invocation]
        self.emit(f"runtime:{self.cage(cid).runtime}", "RunHandler.return",
                  invocation=invocation, result=result)
        return result

    def exec(self, cage: int, image: str, fail: str | None = None) -> None:
        self.cage(cage)
        require(fail in (None, "prepare", "commit"), "EINVAL", "unknown exec fault point")
        require(not any(provider == cage for provider in self.routes.values()), "EBUSY",
                "exec would invalidate registered handler code")
        require(not any(i.provider == cage or i.caller == cage for i in self.invocations.values()),
                "EBUSY", "demo requires active invocations to finish before exec")
        staging = max([0, *self.cages, *self.exits]) + 1
        self.init(staging, image, staging=True)
        self.create(staging)
        if fail == "prepare":
            self.terminate(staging, force=True, record_exit=False)
            raise DemoError("EIO", "pre-commit exec failure; old image intact")
        self.apply(staging, cage, fail=fail == "commit")

    def exit_thread(self, cage: int, tid: int, status: int = 0) -> None:
        c = self.cage(cage)
        t = self.thread(cage, tid)
        require(not t.handlers and not t.forbidden_return, "EBUSY",
                "single-thread exit inside handler is forbidden")
        require(not t.blocked and t.held is None, "EBUSY", "thread has not reached exit")
        if len(c.threads) == 1:
            require(cage not in self.routes.values(), "EBUSY", "incoming handler dependencies")
        self.rt(cage).call(self, "TerminateThread", cage=cage, tid=tid)
        self.clear_thread_tid(cage, t)
        del c.threads[tid]
        if not c.threads:
            self.terminate(cage, status=status)

    def clear_thread_tid(self, cage: int, thread: Thread) -> None:
        if thread.clear_tid is not None:
            outcome = self.write(cage, thread.clear_tid, "00000000")
            self.emit("core", "clear_tid.futex_wake", cage=cage, address=thread.clear_tid, **outcome)

    def terminate(self, cage: int, force: bool = False, status: int = 0,
                  record_exit: bool = True) -> None:
        c = self.cage(cage)
        dependencies = cage in self.routes.values()
        active = any(i.provider == cage or i.caller == cage for i in self.invocations.values())
        require(force or not (dependencies or active), "EBUSY", "handler dependencies still active")
        affected = []
        for iid, i in list(self.invocations.items()):
            if i.provider == cage or i.caller == cage:
                self.invocations.pop(iid)
                for cid, tid, attribute in [(i.caller, i.caller_tid, "blocked"),
                                             (i.provider, i.provider_tid, "handlers")]:
                    if cid in self.cages and tid in self.cages[cid].threads:
                        values = getattr(self.cages[cid].threads[tid], attribute)
                        if iid in values:
                            values.remove(iid)
                self.emit("3i", "invocation.cancel", invocation=iid)
                if i.provider == cage and i.caller != cage:
                    affected.append(i.caller)
        self.routes = {key: value for key, value in self.routes.items()
                       if key[0] != cage and value != cage}
        self.rt(cage).call(self, "TerminateCage", cage=cage, force=force)
        for thread in c.threads.values():
            self.clear_thread_tid(cage, thread)
        self.discard_barriers(cage)
        del self.cages[cage]
        if record_exit and not c.staging:
            self.exits[cage] = status
            if c.root:
                self.root_results[cage] = status
            if c.parent is not None and c.parent in self.cages:
                self.zombies.setdefault(c.parent, {})[cage] = status
            self.emit("core", "exit.accounted", cage=cage, status=status)
        for caller in affected:
            if caller in self.cages:
                self.terminate(caller, force=True, status=139)

    def discard_barriers(self, cage: int) -> None:
        for barrier, pairs in list(self.barriers.items()):
            surviving = [(cid, tid) for cid, tid in pairs if cid != cage]
            if surviving:
                self.barriers[barrier] = surviving
            else:
                del self.barriers[barrier]

    def sigreturn(self, cage: int, tid: int = 1, valid_frame: bool = True) -> None:
        self.dispatch(cage, cage, tid, "rt_sigreturn")
        if valid_frame:
            self.emit(f"runtime:{self.cage(cage).runtime}", "signal.restore", cage=cage, tid=tid)
        else:
            self.emit("core", "MemoryFault", cage=cage, action="cage fault, not EFAULT")
            self.terminate(cage, force=True, status=139)

    def wait(self, cage: int, child: int) -> int:
        self.cage(cage)
        require(child in self.zombies.get(cage, {}), "EAGAIN", "no exited child to reap")
        result = self.zombies[cage].pop(child)
        self.emit("core", "wait.reap", parent=cage, child=child, status=result)
        return result

    def tick(self) -> list[list[int]]:
        ran = []
        for name in self.runtimes:
            runnable = [(c, t) for c in self.cages.values() if c.runtime == name
                        for t in c.threads.values() if self.runnable(c, t)]
            if not runnable:
                self.emit(f"runtime:{name}", "worker.sleep", worker=f"{name}-worker")
            for c, t in runnable:
                if c.cid not in self.cages:
                    continue
                for signal, target_tid in c.pending[:]:
                    if target_tid is not None and target_tid != t.tid:
                        continue
                    c.pending.remove((signal, target_tid))
                    self.emit(f"runtime:{name}", "signal.deliver", cage=c.cid, signal=signal, tid=t.tid)
                    if signal == "TERM":
                        self.terminate(c.cid, force=True, status=143)
                        break
                if c.cid not in self.cages:
                    continue
                self.emit(f"runtime:{name}", "guest.step", cage=c.cid, tid=t.tid,
                          worker=f"{name}-worker", handler=bool(t.handlers))
                ran.append([c.cid, t.tid])
        return ran

    def state(self) -> dict[str, Any]:
        return {
            "cages": {
                str(cid): dict(runtime=c.runtime, image=c.image, stopped=c.stopped,
                               staging=c.staging, generation=c.generation, parent=c.parent,
                               host_base=c.base,
                               mappings=[dict(address=m.address, size=len(m.data),
                                              protection=m.protection, shared=m.shared)
                                         for m in c.mappings],
                               threads={str(tid): dict(runnable=self.runnable(c, t), held=t.held,
                                                       blocked=t.blocked[:], handlers=t.handlers[:],
                                                       result=t.result, tls=t.tls)
                                        for tid, t in c.threads.items()},
                               pending=[signal for signal, _ in c.pending])
                for cid, c in self.cages.items()
            },
            "live_threads": sum(len(c.threads) for c in self.cages.values() if not c.staging),
            "complete": not any(c.threads for c in self.cages.values() if not c.staging),
            "root_results": {str(cid): result for cid, result in self.root_results.items()},
            "invocations": list(self.invocations),
            "barriers": list(self.barriers),
            "routes": [dict(cage=cid, syscall=syscall, provider=provider)
                       for (cid, syscall), provider in self.routes.items()],
            "zombies": {str(cid): {str(child): status for child, status in children.items()}
                        for cid, children in self.zombies.items()},
        }

    def check(self, path: str, equals: Any) -> None:
        value: Any = {"state": self.state(), "last": self.last}
        try:
            for key in path.split("."):
                value = value[int(key)] if isinstance(value, list) else value[key]
        except (KeyError, IndexError, TypeError, ValueError) as error:
            raise DemoError("INPUT", f"no such assertion path {path!r}") from error
        require(value == equals, "ASSERT", f"{path}: expected {equals!r}, got {value!r}")

    def invariant(self) -> None:
        for i in self.invocations.values():
            assert i.iid in self.thread(i.caller, i.caller_tid).blocked
            assert i.iid in self.thread(i.provider, i.provider_tid).handlers
        for (cid, _), provider in self.routes.items():
            assert cid in self.cages and provider in self.cages
        for barrier, pairs in self.barriers.items():
            for cid, tid in pairs:
                if cid in self.cages and tid in self.cages[cid].threads:
                    assert self.thread(cid, tid).held == barrier
