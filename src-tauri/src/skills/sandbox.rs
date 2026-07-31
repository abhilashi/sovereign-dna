//! Sandboxed WASM skill runtime (Phase 2.3) — **security-load-bearing**.
//!
//! Declarative manifest skills (2.2) cover simple variant panels safely because
//! they cannot execute code. But some skills need real computation (PRS with LD,
//! HLA typing, ancestry PCA). Those run as **WebAssembly** with **no ambient
//! capability whatsoever**, which enforces the core privacy invariant:
//!
//! > *a skill can compute on your genome but can never exfiltrate it.*
//!
//! ## Why this is a hard guarantee, not a code-review promise
//!
//! WebAssembly has **no ambient authority**: a guest module can only do what its
//! *imports* let it do. We instantiate every skill module with an **empty
//! [`wasmi::Linker`]** — we provide **zero** host functions. Therefore the guest
//! has no way to open a socket, read a file, read the clock, or obtain
//! randomness. A module that even *declares* an import (e.g. `env::http_get`)
//! fails to instantiate (there is a test for exactly this). The genome bytes we
//! hand in via linear memory, and the result bytes we read back, are the only
//! things that cross the boundary — and both stay on-device.
//!
//! Additional hardening:
//! * **Fuel metering** bounds total execution — a malicious/buggy skill cannot
//!   hang the app with an infinite loop (it traps with [`WasmError::FuelExhausted`]).
//! * **Memory cap** — modules declaring more than `max_memory_pages` are rejected.
//! * **I/O size caps** — input and output lengths are bounded.
//!
//! ## Host ABI (host-managed buffers, no guest allocator required)
//!
//! A skill module must export:
//! * `memory` — its linear memory;
//! * `process(in_ptr: i32, in_len: i32, out_ptr: i32, out_cap: i32) -> i32` —
//!   read `in_len` input bytes at `in_ptr`, write the result (≤ `out_cap` bytes)
//!   at `out_ptr`, and return the number of bytes written, or a negative value
//!   on error (e.g. output would exceed `out_cap`).
//!
//! The host lays input at offset 0 and the output region immediately after it,
//! growing memory (within the cap) as needed. Keeping buffer management on the
//! host side means the simplest possible guest and no trust in a guest allocator.
//!
//! ## Remaining work (tracked, not shipped here)
//! Wiring a `SkillMethod::Wasm` variant + carrying the `.wasm` payload through the
//! signed manifest / registry, and publishing an author SDK, are follow-ups. This
//! module ships the *runtime + its security guarantees* with tests.

use wasmi::{Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

/// Resource limits applied to every sandboxed skill run.
#[derive(Debug, Clone, Copy)]
pub struct WasmLimits {
    /// Max units of execution (wasmi "fuel"). Bounds CPU / prevents infinite loops.
    pub fuel: u64,
    /// Max input length (bytes).
    pub max_input_len: usize,
    /// Max output length (bytes) the host will read back.
    pub max_output_len: usize,
    /// Max linear-memory pages (64 KiB each) a module may declare.
    pub max_memory_pages: u32,
}

impl Default for WasmLimits {
    fn default() -> Self {
        // Conservative defaults suitable for per-variant / small-vector skills.
        Self {
            fuel: 50_000_000,
            max_input_len: 8 * 1024 * 1024,
            max_output_len: 8 * 1024 * 1024,
            max_memory_pages: 256, // 16 MiB
        }
    }
}

const WASM_PAGE: usize = 64 * 1024;

/// Errors from the WASM sandbox.
#[derive(Debug)]
pub enum WasmError {
    /// The `.wasm` bytes are not a valid module.
    Compile(String),
    /// The module declares an import — rejected, because we grant no capabilities.
    ImportsForbidden(String),
    /// The module declares more memory than allowed.
    MemoryTooLarge { declared: u32, max: u32 },
    /// The module does not expose the required ABI.
    AbiMismatch(String),
    /// Execution ran out of fuel (likely an infinite loop).
    FuelExhausted,
    /// The guest trapped or otherwise failed at runtime.
    Trap(String),
    /// Input exceeded `max_input_len`, or the guest reported an output error.
    Io(String),
}

impl std::fmt::Display for WasmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WasmError::Compile(m) => write!(f, "wasm compile error: {m}"),
            WasmError::ImportsForbidden(n) => {
                write!(f, "skill declares forbidden import '{n}' (no host capabilities are granted)")
            }
            WasmError::MemoryTooLarge { declared, max } => {
                write!(f, "skill declares {declared} memory pages (max {max})")
            }
            WasmError::AbiMismatch(m) => write!(f, "skill ABI mismatch: {m}"),
            WasmError::FuelExhausted => write!(f, "skill exceeded its execution budget"),
            WasmError::Trap(m) => write!(f, "skill trapped: {m}"),
            WasmError::Io(m) => write!(f, "skill I/O error: {m}"),
        }
    }
}

impl std::error::Error for WasmError {}

/// Run a WASM skill in the sandbox: feed it `input`, return its output bytes.
///
/// The module is instantiated with **no host functions** (see module docs), so
/// it has no network/filesystem/clock/RNG access; execution is fuel-bounded and
/// memory-capped.
pub fn run_wasm_skill(
    wasm: &[u8],
    input: &[u8],
    limits: &WasmLimits,
) -> Result<Vec<u8>, WasmError> {
    if input.len() > limits.max_input_len {
        return Err(WasmError::Io(format!(
            "input {} bytes exceeds max {}",
            input.len(),
            limits.max_input_len
        )));
    }

    let mut config = Config::default();
    config.consume_fuel(true);
    let engine = Engine::new(&config);

    let module = Module::new(&engine, wasm).map_err(|e| WasmError::Compile(e.to_string()))?;

    // Belt-and-suspenders: reject any import up front with a clear message.
    // (Instantiation with an empty linker would fail anyway, but this names the
    // offending import and documents the guarantee explicitly.)
    if let Some(import) = module.imports().next() {
        return Err(WasmError::ImportsForbidden(format!(
            "{}::{}",
            import.module(),
            import.name()
        )));
    }

    // A store-level resource limiter hard-caps total linear memory, so a module
    // that grows memory itself (even one declaring no explicit maximum) can never
    // exceed the budget.
    let limit_bytes = limits.max_memory_pages as usize * WASM_PAGE;
    let store_limits = StoreLimitsBuilder::new().memory_size(limit_bytes).build();
    let mut store = Store::new(&engine, store_limits);
    store.limiter(|s| s);
    store
        .set_fuel(limits.fuel)
        .map_err(|e| WasmError::Trap(e.to_string()))?;

    // Empty linker == zero granted capabilities.
    let linker: Linker<StoreLimits> = Linker::new(&engine);
    let instance = linker
        .instantiate(&mut store, &module)
        .map_err(|e| WasmError::Trap(e.to_string()))?
        .ensure_no_start(&mut store)
        .map_err(|e| WasmError::Trap(e.to_string()))?;

    let memory = instance
        .get_memory(&store, "memory")
        .ok_or_else(|| WasmError::AbiMismatch("module must export `memory`".into()))?;

    // Fast-fail: if the module declares an *explicit* maximum larger than our
    // budget, reject it up front with a clear message. Modules with no declared
    // maximum are allowed here and capped at runtime by the store limiter above.
    if let Some(declared_max) = memory.ty(&store).maximum_pages().map(u32::from) {
        if declared_max > limits.max_memory_pages {
            return Err(WasmError::MemoryTooLarge {
                declared: declared_max,
                max: limits.max_memory_pages,
            });
        }
    }

    // Lay out buffers: input at 0, output region right after (8-byte aligned).
    let in_ptr: usize = 0;
    let out_ptr: usize = (input.len() + 7) & !7;
    let needed = out_ptr + limits.max_output_len;
    ensure_capacity(&memory, &mut store, needed, limits.max_memory_pages)?;

    memory
        .write(&mut store, in_ptr, input)
        .map_err(|e| WasmError::Io(e.to_string()))?;

    let process = instance
        .get_typed_func::<(i32, i32, i32, i32), i32>(&store, "process")
        .map_err(|_| {
            WasmError::AbiMismatch(
                "module must export `process(i32,i32,i32,i32)->i32`".into(),
            )
        })?;

    let out_len = match process.call(
        &mut store,
        (
            in_ptr as i32,
            input.len() as i32,
            out_ptr as i32,
            limits.max_output_len as i32,
        ),
    ) {
        Ok(n) => n,
        Err(e) => {
            // Distinguish fuel exhaustion for a clearer message.
            if e.as_trap_code() == Some(wasmi::core::TrapCode::OutOfFuel) {
                return Err(WasmError::FuelExhausted);
            }
            return Err(WasmError::Trap(e.to_string()));
        }
    };

    if out_len < 0 {
        return Err(WasmError::Io(format!(
            "skill reported output error (code {out_len}); output may exceed {} bytes",
            limits.max_output_len
        )));
    }
    let out_len = out_len as usize;
    if out_len > limits.max_output_len {
        return Err(WasmError::Io(format!(
            "skill returned {out_len} bytes, exceeds max {}",
            limits.max_output_len
        )));
    }

    let mut out = vec![0u8; out_len];
    memory
        .read(&store, out_ptr, &mut out)
        .map_err(|e| WasmError::Io(e.to_string()))?;
    Ok(out)
}

/// Grow linear memory (within the cap) so at least `needed` bytes are addressable.
fn ensure_capacity(
    memory: &wasmi::Memory,
    store: &mut Store<StoreLimits>,
    needed: usize,
    max_pages: u32,
) -> Result<(), WasmError> {
    let current_pages: u32 = memory.size(&*store);
    let current_bytes = current_pages as usize * WASM_PAGE;
    if current_bytes >= needed {
        return Ok(());
    }
    let extra_pages = (((needed - current_bytes) + WASM_PAGE - 1) / WASM_PAGE) as u32;
    let target = current_pages.saturating_add(extra_pages);
    if target > max_pages {
        return Err(WasmError::MemoryTooLarge {
            declared: target,
            max: max_pages,
        });
    }
    memory
        .grow(&mut *store, extra_pages)
        .map_err(|e| WasmError::Io(format!("memory grow failed: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // A well-behaved skill: reverse the input bytes into the output buffer.
    const REVERSE_WAT: &str = r#"
    (module
      (memory (export "memory") 1)
      (func (export "process")
        (param $in i32) (param $inlen i32) (param $out i32) (param $outcap i32)
        (result i32)
        (local $i i32)
        (if (i32.gt_u (local.get $inlen) (local.get $outcap))
          (then (return (i32.const -1))))
        (block $done (loop $loop
          (br_if $done (i32.ge_u (local.get $i) (local.get $inlen)))
          (i32.store8
            (i32.add (local.get $out)
              (i32.sub (i32.sub (local.get $inlen) (i32.const 1)) (local.get $i)))
            (i32.load8_u (i32.add (local.get $in) (local.get $i))))
          (local.set $i (i32.add (local.get $i) (i32.const 1)))
          (br $loop)))
        (local.get $inlen)))
    "#;

    // A hostile skill that tries to import a network function.
    const EVIL_IMPORT_WAT: &str = r#"
    (module
      (import "env" "http_get" (func $h (param i32 i32) (result i32)))
      (memory (export "memory") 1)
      (func (export "process")
        (param i32 i32 i32 i32) (result i32)
        (i32.const 0)))
    "#;

    // A skill that loops forever — must be stopped by fuel metering.
    const INFINITE_WAT: &str = r#"
    (module
      (memory (export "memory") 1)
      (func (export "process")
        (param i32 i32 i32 i32) (result i32)
        (loop $l (br $l))
        (i32.const 0)))
    "#;

    fn wasm(wat: &str) -> Vec<u8> {
        wat::parse_str(wat).unwrap()
    }

    #[test]
    fn compute_only_skill_runs_and_returns_output() {
        let out = run_wasm_skill(&wasm(REVERSE_WAT), b"ACGT", &WasmLimits::default()).unwrap();
        assert_eq!(out, b"TGCA");
    }

    #[test]
    fn larger_input_reverses_correctly() {
        let input: Vec<u8> = (0u8..200).collect();
        let mut expected = input.clone();
        expected.reverse();
        let out = run_wasm_skill(&wasm(REVERSE_WAT), &input, &WasmLimits::default()).unwrap();
        assert_eq!(out, expected);
    }

    #[test]
    fn skill_declaring_a_network_import_is_rejected() {
        let err = run_wasm_skill(&wasm(EVIL_IMPORT_WAT), b"x", &WasmLimits::default()).unwrap_err();
        match err {
            WasmError::ImportsForbidden(n) => assert!(n.contains("http_get")),
            other => panic!("expected ImportsForbidden, got {other:?}"),
        }
    }

    #[test]
    fn infinite_loop_is_stopped_by_fuel() {
        let limits = WasmLimits {
            fuel: 100_000,
            ..WasmLimits::default()
        };
        let err = run_wasm_skill(&wasm(INFINITE_WAT), b"x", &limits).unwrap_err();
        assert!(matches!(err, WasmError::FuelExhausted), "got {err:?}");
    }

    #[test]
    fn missing_process_export_is_abi_mismatch() {
        let wat = r#"(module (memory (export "memory") 1))"#;
        let err = run_wasm_skill(&wasm(wat), b"x", &WasmLimits::default()).unwrap_err();
        assert!(matches!(err, WasmError::AbiMismatch(_)), "got {err:?}");
    }

    #[test]
    fn oversized_declared_memory_is_rejected() {
        // Declares a max of 1000 pages; our cap is 256.
        let wat = r#"
        (module
          (memory (export "memory") 1 1000)
          (func (export "process") (param i32 i32 i32 i32) (result i32) (i32.const 0)))
        "#;
        let err = run_wasm_skill(&wasm(wat), b"x", &WasmLimits::default()).unwrap_err();
        assert!(matches!(err, WasmError::MemoryTooLarge { .. }), "got {err:?}");
    }

    #[test]
    fn oversized_input_is_rejected_before_run() {
        let limits = WasmLimits {
            max_input_len: 4,
            ..WasmLimits::default()
        };
        let err = run_wasm_skill(&wasm(REVERSE_WAT), b"toolong", &limits).unwrap_err();
        assert!(matches!(err, WasmError::Io(_)), "got {err:?}");
    }
}
