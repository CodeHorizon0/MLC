# MLC Kernel Core

## Features

- Persistent Python and Lua workers.
- Reused runtimes for repeated short executions.
- Stable long running execution through isolated worker queues.
- Dependency graph discovery for Python and Lua local modules.
- Absolute and relative path normalization.
- Windows and Unix path input support.
- Inline code execution support.

## Running

```bash
cargo run
```

The demo starts:

1. Python file execution from `python_app/main.py`.
2. Lua file execution from `lua_app/main.lua`.
3. Inline Python execution.
4. Inline Lua execution.

## Path handling

The kernel accepts:

- `python_app/main.py`
- `./python_app/main.py`
- `python_app\\main.py`
- absolute paths

Paths are normalized before loading and checked against the filesystem.

## Worker model

Python and Lua use dedicated long-lived threads:

- startup cost is paid once;
- runtime state can be reused;
- task queues prevent unlimited memory growth;
- repeated executions avoid recreating interpreters.

## Examples

Python:

```python
print("hello")
```

Lua:

```lua
print("hello")
```
