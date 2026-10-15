# PS1 native HCB compiler status

A complete HCB-to-native compiler has not been implemented.

The rejected experiment emitted native helper-call trampolines while retaining
Context, Variant evaluation stacks and VM operations. It expanded code, reduced
the useful script cache and stalled on optical-disc reads. The experimental
engine, loader and converter path have been removed. The prior working release
and game discs are retained; rejected source and logs remain under the ignored
output/recovery directory. No native-AOT speedup is claimed.

The requested compiler must reconstruct expressions and control flow from the
HCB operand stack at compile time, then eliminate that stack from the runtime.
It should emit native variables, arithmetic, branches and function calls, with
constant propagation, dead-code removal and live-variable analysis. Strings
remain external resources loaded by engine helpers using stable indices. Engine
syscalls remain helper boundaries. Coroutine waits require explicit continuations
and only values live across suspension, preserving scheduling and script semantics.
There must be no per-opcode native trampoline or interpreter fallback in the
completed native backend. Combined native code, persistent state and transient
memory must be measured against the working PS1 release before deployment.

A fresh interpreter profiling run measured approximately 150 ms/frame inside
VM tick, 16.7 ms/frame in syscalls and zero script-page I/O during steady frames
500-600. This identifies substantial execution overhead, but is not a native
compiler result and is not proof of any projected AOT speedup.
