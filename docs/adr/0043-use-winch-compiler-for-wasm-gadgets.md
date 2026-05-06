# 43. Use Winch compiler for WASM gadgets

Date: 2026-05-06

## Status

Accepted

## Context

After migrating built-in gadgets to WASM, the `torchsnap` process idle RSS
grew from ~45 MB to ~242 MB. The WebKit processes remained unchanged, placing
the increase entirely in the host process.

Switching Cranelift to `OptLevel::None` reduced idle RSS by only ~14 MB
(242 MB to 228 MB), confirming that the optimization passes are not the
dominant cost within Cranelift's codegen.

Switching to wasmtime's alternative compiler backend Winch reduced idle RSS to
~110 MB, recovering roughly two thirds of the regression.

### Benchmark data (2026-05-06)

All measurements taken at the "app started (idle)" phase with 7 WASM gadgets
loaded (bangs, calculator, emoji-picker, hello-world, open-url, template,
zerotier). Total release WASM size across all gadgets: ~5.2 MB.

| Configuration               | `torchsnap` idle RSS | Total (all processes) |
|-----------------------------|---------------------:|----------------------:|
| Pre-WASM baseline           |              45.4 MB |              92.1 MB  |
| Cranelift (default)         |             242.4 MB |             293.3 MB  |
| Cranelift `OptLevel::None`  |             227.5 MB |             272.2 MB  |
| **Winch**                   |         **109.5 MB** |         **160.8 MB**  |

Session stability (delta from first to last phase snapshot):

| Configuration       | `torchsnap` Δ | Total Δ   |
|---------------------|---------------:|----------:|
| Cranelift (default) |      -100.6 MB |  -54.5 MB |
| Winch               |        -1.5 MB |  +44.9 MB |

## Decision

Use Winch (`Strategy::Winch`) as the wasmtime compiler backend for gadget
compilation. The strategy is set explicitly in the `Engine` configuration
(`src-tauri/src/wasm/runtime/engine.rs`) so that Winch is used regardless of
which compiler backends happen to be available in the wasmtime feature flags.

## Consequences

Idle RSS of the `torchsnap` process dropped from ~242 MB (Cranelift) to
~110 MB (Winch). Memory stays stable throughout a session with Winch
(delta -1.5 MB over a full benchmark run), compared to Cranelift where RSS
started at 242 MB and gradually settled to ~142 MB.

Winch produces slower native code than Cranelift. If future gadgets show
performance problems, this decision can be revisited.
