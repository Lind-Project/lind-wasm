# Clamping
Clamping is a composition mechanism that allows a grate to selectively route syscalls to other grates based on some condition. Rather than all calls flowing through every grate in the stack unconditionally (as with stacking), a clamping grate evaluates a routing rule and only sends matching calls through the clamped grates. Non-matching calls bypass them entirely.
The routing condition is up to the clamping grate. It could be based on path prefix, syscall type, file descriptor properties, cage identity, or anything else the clamping grate can inspect. The mechanism is the same regardless of the condition.

## Command-line syntax
`%{` and `%}` delimiters on the command line mark what's clamped. We use these instead of parentheses or braces to avoid conflicts with shell syntax in bash and other shells. Everything inside the delimiters is conditionally applied based on the clamping grate's routing rule. Everything outside runs unconditionally.
A useful way to read these is as if/endif blocks. The command line reads bottom-to-top as a stack, with the application on top. Note that the if condition's arguments are specified before the `%{` (on the command line), not after the `%}` as you might expect from traditional if/then syntax:

`namespace-grate --prefix /tmp %{ imfs-grate strace-grate %} python` reads as:

```
namespace-grate --prefix /tmp %{
imfs-grate
strace-grate
%}
python
```

which translates to:

```
python
if --prefix /tmp
    strace-grate
    imfs-grate
endif
```

Matching calls (/tmp paths) flow through strace then imfs. Non-matching calls skip both and go to the next grate or kernel.

## How it works
A clamping grate interposes on `register_handler`, `exec`, `fork`, and `exit`.

### exec interposition
Both clamping and non-clamping grates follow the same basic pattern: consume arguments, fork a child, and exec the rest of the command line as the child's arguments. Grates don't parse or understand anything after their own arguments. They just pass it along. The only difference is that a clamping grate also interposes on register_handler, exec, fork, and exit to set up and manage its routing table.
The `%}` is literally part of the command line that gets passed down through execs. When a cage eventually tries to exec `%}`, the clamping grate intercepts it for two reasons: it strips the `%}` and rewrites the exec to whatever comes after, and it sets an internal flag to stop intercepting `register_handler` calls from subsequent descendants. Everything before the `%}` exec is inside the clamp; everything after is outside. Without exec interposition, the clamping grate has no way to observe this boundary and know when to stop treating new `register_handler` calls as belonging to clamped grates.

There is also a rare corner case that happens for grates that fork and exec new grates.  If a clamped grate dynamically registers new handlers later, the clamping grate still intercepts those, since its register_handler interposition remains active for descendants of cages inside the clamp.

### fork and exit interposition
The clamping grate interposes on `fork` to track cage lineage. When a clamped grate forks, the resulting child cage is also inside the clamp — clamp membership is inherited. The clamping grate records the new cage ID at fork time so it knows to intercept that cage's `register_handler` calls and apply routing to its syscalls. Cages forked from outside the clamp are not recorded and are left alone.

The clamping grate also interposes on `exit` to clean up state associated with cages that are no longer running.

### register_handler interposition
The clamping grate intercepts `register_handler` calls from clamped grates and builds its routing table incrementally, as is described below.
When a clamped grate calls `register_handler(open, handler)` for its child, the clamping grate intercepts this and instead:
- Registers `open` in the child's handler table pointing to the clamping grate. Now when that cage calls open, it hits the clamping grate first.
- Registers `alt_open` (an unused syscall number) in the clamping grate's handler table targeting the clamped grate. This gives the clamping grate a way to forward calls when the routing rule matches.
This happens for each syscall the clamped grate registers. At runtime, when a descendant calls open, it hits the clamping grate's handler. The clamping grate evaluates its routing condition. If it matches, it makes the originating cage call alt_open, which hits the clamping grate's table and gets dispatched to the clamped grate. If it doesn't match, the call passes to the next grate or kernel.
When multiple grates are inside the clamp, the clamping grate stacks them incrementally. As each clamped grate registers handlers, the clamping grate chains them so that matching calls flow through all clamped grates in their stack order.

## Examples
The following examples use a namespace grate as the clamping grate. The first routes by path prefix (only `/tmp` paths go to IMFS), the later examples also include one that routes by syscall type (only reads go to strace). These are just two possible routing strategies to illustrate the mechanism.

### Simple: single clamped grate
`namespace-grate --prefix /tmp %{ imfs-grate %} python`
```
python
if --prefix /tmp
    imfs-grate
endif
```
Spawn order: namespace > imfs > python. Namespace spawns imfs. When imfs tries to exec `%} python`, namespace intercepts, rewrites to exec `python`.
- `write('/tmp/foo')` -- matches /tmp, routed to IMFS
- `read('/tmp/foo')` -- matches /tmp, routed to IMFS
- `read('/etc/passwd')` -- no match, goes to the next grate or kernel. IMFS never sees it.

### Stacking grates inside the clamp
`namespace-grate --prefix /tmp %{ imfs-grate strace-grate %} python`
```
python
if --prefix /tmp
    strace-grate
    imfs-grate
endif
```
Spawn order: namespace > imfs > strace > python. Namespace spawns imfs, imfs spawns strace. When strace tries to exec `%} python`, namespace intercepts, rewrites to exec `python`.
Both imfs and strace are inside the clamp. Namespace intercepts registrations from both and stacks them. For matching calls, python's syscalls flow through strace then imfs. For non-matching calls, both are skipped entirely.
- `write('/tmp/foo')` -- matches /tmp. Strace sees it, IMFS sees it.
- `read('/tmp/foo')` -- matches /tmp. Strace sees it, IMFS sees it.
- `read('/etc/passwd')` -- no match. Neither sees it.

### Separate clamps
`namespace-grate --prefix /tmp %{ imfs-grate %} namespace-grate --syscall=read %{ strace-grate %} python`
```
python
if syscall=read
    strace-grate
endif
if --prefix /tmp
    imfs-grate
endif
```
Two separate namespace grates in series. The first clamps imfs for /tmp paths. The second clamps strace for read syscalls only. These operate independently.
- `write('/tmp/foo')` -- not a read, skips strace's clamp. Matches /tmp, routed to IMFS. **Strace doesn't see it, IMFS sees it.**
- `read('/tmp/foo')` -- is a read, strace sees it. Matches /tmp, routed to IMFS. **Both see it.**
- `read('/etc/passwd')` -- is a read, strace sees it. No /tmp match, goes to the next grate or kernel. **Strace sees it, IMFS doesn't.**

### Nested clamps
`namespace-grate --prefix /tmp %{ imfs-grate namespace-grate --syscall=read %{ strace-grate %} %} python`
```
python
if --prefix /tmp
    if syscall=read
        strace-grate
    endif
    imfs-grate
endif
```
The strace clamp is nested inside the /tmp clamp. Strace only sees calls that match both conditions.
- `write('/tmp/foo')` -- matches /tmp, enters outer clamp. Not a read, skips inner clamp. IMFS sees it. **Strace doesn't see it, IMFS sees it.**
- `read('/tmp/foo')` -- matches /tmp, enters outer clamp. Is a read, enters inner clamp. Strace sees it, IMFS sees it. **Both see it.**
- `read('/etc/passwd')` -- no /tmp match, skips everything. **Neither sees it.**
The difference between separate and nested: with separate clamps, `read('/etc/passwd')` reaches strace because strace's clamp is independent. With nested clamps, strace is inside the /tmp clamp, so only /tmp reads reach it.

## Teeing
Teeing is a composition mechanism that duplicates a syscall across two or more independent stacks. Where clamping routes a call to one path or another based on a condition, teeing sends it through both.

The tee grate interposes on `register_handler` to capture handler registrations from both stacks and builds two independent routing tables. When a syscall arrives, it dispatches to both chains via `make_syscall` and reconciles the return values. The reconciliation strategy is up to the tee grate — it might take the first success, wait for both, or fail if either fails.

Calls with process-level side effects like `fork` are forwarded only once to avoid duplicating the originating cage.


### Example
`tee-grate %{ imfs-grate %} %{ remote-store-grate %} python`

Every `write` from python goes to both IMFS and the remote store. Neither stack is aware of the other.


## fd table management (path-based clamping)
For clamping grates that route based on path prefix, some syscalls like write and read take an fd rather than a path. The clamping grate needs to know which fd maps to which path to make routing decisions for these calls. So it also interposes on fork, dup, open, close, and similar calls to maintain its own per-cage fd table.
When python calls `write(fd=3, data)`, the clamping grate looks up fd 3 in its fd table to determine the associated path. If it was opened under `/tmp`, it routes to IMFS via alt_write. If not, it passes to the next grate or kernel. Fork is intercepted so the clamping grate can duplicate fd state for new cages, then forwarded to IMFS so IMFS can do the same.