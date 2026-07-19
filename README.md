Rust Functions Runtime aka CF Worker/AWS Functions clone

Features:
- JS and TypeScript functions stored as separate files in ./js
- automatic loading on startup
- HTTP proxy layer
- idle server waiting for requests
- request routing to JS workers
- isolated QuickJS contexts
- TypeScript files are transpiled to plain JavaScript before execution

## Run:
`cargo run`
or `cargo build --release`

## Calling:

JavaScript
GET http://localhost:8080/functions/hello

TypeScript :
GET http://localhost:8080/functions/typed
