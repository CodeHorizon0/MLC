# kernel_core

Rust kernel foundation for an application that embeds Python and Lua scripts.

## What this version adds

- `run` accepts either inline code or a path to a file
- `run_python` and `run_lua` accept either inline source code or a file path string
- automatic dependency discovery for local Python imports
- automatic dependency discovery for Lua `require(...)`
- automatic preload/import registration for discovered local modules
- shared host API for both languages
- worker-based isolation for Python and Lua execution

## Notes

- Python uses PyO3 embedding and registers the `host` module before interpreter initialization.
- Lua uses `mlua` with safe standard libraries and per-module `package.preload` hooks.
- The dependency scanner is intentionally lightweight and focused on normal project-style imports.

## Example

```rust
use kernel_core::{Kernel, KernelConfig};

let kernel = Kernel::new(KernelConfig::default())?;
let report = kernel.run_python("examples/python_app/main.py")?;
println!("{:?}", report);
```

## Important

This project is structured as a foundation. The import resolver is practical and safe, but not a full language parser. It is designed to cover common multi-file application layouts without manually listing every dependency.
