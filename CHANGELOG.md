# Changelog

## [0.5.4] – 2025-12-04

### Config, auth, and server

* Added environment-driven server overrides via `config/config.<env>.json` `server` section; `config::apply_server_to_program` now patches the parsed `Program.server.port` and `Program.server.tls` before HTTP startup so a single `app.shr` can be reused across environments.
* Introduced an `auth` configuration section with `jwt_secret_env`, `protected_paths`, and `allow_missing_on` fields, enabling declarative JWT protection by path prefix while still allowing public endpoints such as `/health`.
* Added `config::jwt_secret_from_env`, which resolves the configured `jwt_secret_env` logical key into the actual secret from the process environment and is used by the HTTP server’s JWT verifier.

### HTTP server, TLS, and request lifecycle

* Refactored `interpreter::http::run` to take the configured port/TLS flags from the `Program` (after config overrides) and to construct the Actix `App` in a single factory, reducing cloning and ensuring consistent server behavior.
* Implemented per-request structured JSON logging via `log_request`, capturing timestamp, method, path, status, client IP, elapsed milliseconds, and whether a valid JWT was present; all user and system routes now report through this logger.
* Added a built-in `/health` endpoint that returns a JSON `{"status":"ok"}` payload for liveness checks and quick local verification.
* Exposed internal diagnostics and documentation endpoints:

  * `/__shrimpl/schema` returns the derived schema for the current program.
  * `/__shrimpl/diagnostics` returns static analysis (and future type-check) diagnostics.
  * `/__shrimpl/ui` serves the browser-based API Studio.
  * `/__shrimpl/source` streams the raw `app.shr` source file for inspection.
* Modernized TLS handling to `rustls` 0.23 and `rustls-pemfile` 2.x:

  * `load_tls_config` now consumes `CertificateDer` and `PrivatePkcs8KeyDer`, converts to `PrivateKeyDer`, and uses `bind_rustls_0_23` for server binding.
  * Certificate and key paths are read from `SHRIMPL_TLS_CERT` and `SHRIMPL_TLS_KEY`, defaulting to `cert.pem` and `key.pem` when unset.

### JWT authentication

* Added `path_requires_auth`, which consults `config.auth.protected_paths` and `config.auth.allow_missing_on` to decide whether a given Shrimpl path must present a JWT.
* Implemented `extract_bearer_token` and `verify_jwt` helpers:

  * Accepts `Authorization: Bearer <token>` headers.
  * Validates tokens using HS256 and the configured secret from `jwt_secret_env`.
  * Produces consistent error messages such as `"missing bearer token"` and `"unauthorized"` with structured JSON bodies.
* Introduced `verify_jwt_if_required`, used by all HTTP routes to:

  * Return early with `401` responses when a token is missing or invalid on protected paths.
  * Skip JWT work entirely on unprotected paths.
  * Propagate parsed claims (`sub`, `scope`, `role`) for downstream use.
* Ensured that request variable maps always contain JWT-related keys:

  * Every request (even unauthenticated) now receives `jwt_sub`, `jwt_scope`, and `jwt_role` defaulted to empty strings.
  * When a valid JWT is present, these keys are overwritten with the actual claim values so Shrimpl code can safely interpolate them without `Unknown variable` errors.

### Request body validation and sanitization

* Added an optional JSON Schema validation layer driven by `config.validation.schemas`:

  * `config::validation_schema_for_path` looks up a schema by exact Shrimpl path (for example, `"/orders/create"`).
  * `validate_and_sanitize_body` parses the incoming JSON body, compiles the schema with `jsonschema` Draft 7, and validates the payload.
* Defined clear HTTP error behavior around validation:

  * Malformed JSON produces `400` with `"invalid_json"` error and a parse detail.
  * Schema compile failures (misconfigured server) produce `500` with `"schema_compile_error"`.
  * Validation failures produce `400` with `"validation_failed"` and the first error plus instance path.
* Implemented conservative sanitization via `sanitize_json`:

  * Trims leading/trailing whitespace on all string fields.
  * Recurses through arrays and objects while leaving numeric and boolean values untouched.
* Wired POST routes to inject sanitized request bodies into Shrimpl endpoints:

  * When a schema is configured, `body` is the sanitized JSON string.
  * When no schema is configured, `body` is the raw request body, preserving existing behavior.

### Program loading and lockfiles

* Introduced a simple import-aware loader in `loader.rs`:

  * `load_with_imports(entry)` recursively resolves `import "relative/path.shr"` statements, canonicalizes paths, and prevents cycles by tracking a visited set.
  * Imported files are inlined into a single logical program while omitting the original `import` lines, making multi-file Shrimpl projects possible without changing the interpreter.
* Added a lightweight lockfile mechanism in `lockfile.rs`:

  * `write_lockfile` writes `shrimpl.lock` alongside the program with:

    * Shrimpl CLI version.
    * Logical environment (`SHRIMPL_ENV`).
    * Entry path.
    * SHA-256 hash of the entry file.
    * Generation timestamp (seconds since UNIX epoch).
  * This enables reproducibility and external tooling to detect when the source or environment has changed.
  * Included a `load_lockfile` helper for future tooling, keeping it available behind a dead-code allow so it does not interfere with current Clippy settings.

### Optional type system and annotations

* Extended the configuration schema with a `types` section:

  * `types.functions.<name>.params` holds parameter types (for example, `"number"`, `"string"`, `"bool"`, `"any"`).
  * `types.functions.<name>.result` holds the declared return type.
* Added a new `typecheck` module that consumes this configuration:

  * Maps simple type names into an internal `Ty` enum (`Number`, `String`, `Bool`, `Any`) with permissive assignment rules (anything assignable to `Any`, and `Any` assignable to more specific types).
  * Exposes `build_type_diagnostics(program)` to generate structured JSON diagnostics for functions that have annotations.
* Implemented expression-level type inference in `infer_expr_type`:

  * Supports literals (`number`, `string`, `bool`), variables, lists, maps, arithmetic, comparisons, logical operators, `if / elif / else`, `repeat`, and `try / catch / finally`.
  * Emits warnings when numeric operators are used with non-numeric operands.
* Added call-site checking for annotated functions:

  * Validates argument counts against the annotation and emits errors when they do not match.
  * Checks argument types by inferring each argument and comparing against the expected parameter type.
  * Verifies that function bodies are compatible with declared return types and produces `"Return type mismatch"` errors when they are not.
* Structured type diagnostics in the same JSON format used elsewhere so diagnostics panels and editor tooling can attribute issues precisely to function definitions or call sites.

### Sample app and new endpoints

* Expanded the 0.5.4 sample `app.shr` to exercise configuration, environment, HTTP client, auth, and validation features:

  * Added configuration helpers:

    * `config_set_value`, `config_get_value`, `config_get_or_default`, and `config_has_key` wrap `config_set`, `config_get`, and `config_has`, with corresponding `/config/*` endpoints.
    * `read_env` exposes `env(name)` through the `/env` endpoint, returning a single environment variable.
  * Added HTTP client helpers:

    * `http_proxy_get` and `http_proxy_get_json` use `http_get` / `http_get_json` to proxy external HTTP calls via `/http/get` and `/http/get-json`.
  * Rounded out the OpenAI helper surface:

    * `set_openai_api_key` and `set_openai_system` for one-shot key/system prompt configuration via `/ai/set-key` and `/ai/set-system`.
    * `chat_with_openai` and `chat_with_openai_json` for `/ai/chat` and `/ai/chat-json`.
    * A `/ai/test-simple` smoke test that verifies the OpenAI bridge.
    * `call_openai_mcp` powering `/ai/mcp-call` for tool-style MCP interactions.
* Added auth-focused endpoints that demonstrate JWT integration:

  * `/public/ping` is explicitly left unprotected and returns `"public-ok"` to confirm the server is running.
  * `/secure/profile` reflects `jwt_sub`, `jwt_role`, and `jwt_scope`, and gracefully degrades to a `"profile: anonymous"` message when no JWT is present.
  * `/secure/admin` demonstrates simple role-based gating, returning `"admin access granted"` only when `jwt_role == "admin"`.
* Added validation-focused endpoints that exercise the new JSON Schema layer:

  * `POST /orders/create` expects a schema in `config.validation.schemas["/orders/create"]` and echoes back the sanitized JSON `body` string on success.
  * `POST /orders/raw` accepts and returns the raw `body` without validation, making it easy to compare behavior with and without schemas.

### JSON rendering and diagnostics plumbing

* Updated JSON conversion for runtime values so string values are treated as JSON if they parse cleanly, falling back to plain strings otherwise; this makes it easier to return nested JSON from Shrimpl expressions.
* Ensured that diagnostics and logging paths remain exhaustive with respect to the extended expression set (including `bool`, `if`, `repeat`, and `try`), preventing non-exhaustive match warnings as the language evolves.

### CI and tooling

* Tightened Rust hygiene by enforcing `cargo fmt`, `cargo clippy --all-targets --all-features -D warnings`, and `cargo test --all` in continuous integration.
* Split GitHub Actions workflows so that:

  * The `main` branch runs full CI plus a crates.io dry run and publish step for tagged releases.
  * Feature and maintenance branches run the same compilation, test, formatting, and lint checks without attempting to publish, keeping iteration fast while guaranteeing code quality.

---

## [0.5.3] – 2025-11-28

### Language / syntax

* Added boolean literals and type:

  * `true` and `false` are now first-class values.
* Extended expression grammar with boolean and comparison operators:

  * Logical operators: `and`, `or`.
  * Comparison operators: `==`, `!=`, `<`, `<=`, `>`, `>=`.
  * Defined precedence so that:

    * `*` / `/` bind tighter than `+` / `-`.
    * Comparisons bind tighter than `and` / `or`.
    * `and` binds tighter than `or`.
* Introduced `if / elif / else` as an expression:

  * Syntax:

    ```shrimpl
    if cond1: expr1
    elif cond2: expr2
    else: expr3
    ```
  * Evaluates branches in order and returns the value of the first branch whose condition is truthy, or the `else` branch if provided. If no branch matches and there is no `else`, the result is an empty string.
* Added a safe, bounded loop expression:

  * Syntax:

    ```shrimpl
    repeat N times: expr
    ```
  * Evaluates the `count` expression once, floors it to an integer, and executes the body expression that many times.
  * Returns the value of the last iteration, or `""` if `N == 0`.
  * Includes a hard safety cap (10,000 iterations) to prevent runaway loops in teaching contexts.
* Defined truthiness rules used in conditionals and logical operators:

  * `Bool`: uses its value directly.
  * `Number`: `0.0` is false, anything else is true.
  * `String`: `""` is false, anything else is true.

### Interpreter / runtime

* Extended `ValueRuntime` to include booleans and implemented display formatting so booleans print as `true` / `false`.
* Implemented evaluation for new expression forms:

  * `Expr::Bool` is evaluated to `ValueRuntime::Bool`.
  * `Expr::If` walks each `(condition, body)` pair, using `as_bool` on the condition and evaluating the first matching body.
  * `Expr::Repeat` evaluates the count, checks non-negativity, enforces the iteration cap, and repeatedly evaluates the loop body.
* Introduced helper functions:

  * `as_bool` to map runtime values to truthiness (used by `if`, `and`, `or`).
  * Reused `as_number` to coerce values in arithmetic, comparisons, and repeat counts.
* Extended `eval_binary` to handle:

  * Boolean logic for `BinOp::And` and `BinOp::Or` using `as_bool`.
  * Numeric comparisons with clear error messages if operands are not numeric.
* Kept existing behavior where `+` between non-numeric values falls back to string concatenation, maintaining previous ergonomic patterns.

### Expression parser

* Extended tokenizer to recognize:

  * `==`, `!=`, `<`, `<=`, `>`, `>=`.
  * `:` as a significant token for `if`, `elif`, `else`, and `repeat` syntax.
* Added top-level recognizers for control-flow expressions:

  * When an expression starts with `if`, it is parsed as an `if / elif / else` expression.
  * When an expression starts with `repeat`, it is parsed as a `repeat N times: expr` loop expression.
* Implemented `parse_if_expr`:

  * Parses `if` condition and body, zero or more `elif` condition/body pairs, and optional `else` body.
  * Produces an `Expr::If { branches, else_branch }`.
* Implemented `parse_repeat_expr`:

  * Parses `repeat <expr> times: <expr>` into `Expr::Repeat { count, body }`.
  * Provides targeted error messages if `times` or `:` are missing.
* Wired in boolean / logical operators at the correct precedence levels:

  * `parse_or` and `parse_and` layers for `or` and `and` expressions.
  * `parse_comparison` layer for `==`, `!=`, `<`, `<=`, `>`, `>=`.
* Ensured that the parser reports “Unexpected tokens after end of expression” if trailing tokens remain, preserving strict single-expression semantics.

### Diagnostics and API Studio

* Updated `collect_vars_expr` and diagnostics to account for new expression variants:

  * `Expr::Bool` is treated as a literal with no variable references.
  * Added arms for `Expr::If` and `Expr::Repeat` to keep matches exhaustive.
  * Left hooks in place to later traverse branches and loop bodies for variable usage analysis.
* Retained endpoint/function/method diagnostics:

  * Duplicate `(method, path)` endpoint detection.
  * Unused path parameters in endpoint bodies.
  * Unused parameters in functions and methods.
* API Studio UI (`/__shrimpl/ui`):

  * Modernized single-page HTML UI with:

    * Endpoint list and selector.
    * Request builder for path params and query strings.
    * Response viewer with status pill and JSON pretty-printing.
    * Code panel that fetches and syntax-highlights `app.shr`.
    * Diagnostics panel that renders static diagnostics (warnings/errors).
  * Syntax highlighting now covers:

    * Keywords: `server`, `func`, `class`, `endpoint`.
    * HTTP methods: `GET`, `POST`.
    * JSON keyword `json`.
    * Numbers, strings, and comments with distinct styles.

### CLI / UX

* Improved `shrimpl` `run` UX:

  * Running `shrimpl` or `shrimpl --file app.shr run` now prints a startup banner that includes:

    * The port configured in the `server` declaration.
    * Direct hints to open `http://localhost:<port>/__shrimpl/ui` to explore and test the API.
    * Clear instructions for shutting down the server with `Ctrl+C`.
* Kept `run` as the default behavior when no subcommand is provided.
* Maintained the `lsp` subcommand to launch `shrimpl-lsp`, wiring stdin/stdout/stderr through to support editor LSP integrations without additional setup.

### Sample app (`app.shr`)

* Extended the demo program to exercise new control-flow and boolean features:

  * `func classify_age(age)` using `if / elif / else` and numeric comparisons.
  * `func describe_adult_us(age, country)` combining comparisons with `and` / `or`.
  * `func flag_to_bool(flag)` mapping `"yes"` to `true` and other values to `false`.
  * `func repeat_greet(name, n)` using `repeat number(n) times: ...` to build repeated greetings.
* Added new endpoints showcasing these helpers:

  * `/age-category?age=...` → `"child" | "teen" | "adult"`.
  * `/adult-us?age=...&country=...` → `"adult-us" | "adult-non-us" | "not-adult"`.
  * `/flag?flag=...` → `true` / `false`.
  * `/repeat-greet?name=...&n=...` → repeated greeting string.
* Improved `/welcome` endpoint:

  * Uses an `if` expression to provide:

    * A generic message when `name` is empty.
    * A personalized uppercase greeting including name length when `name` is present.

### Internal fixes

* Resolved Rust non-exhaustive pattern warnings/errors by:

  * Updating `docs::collect_vars_expr` to handle all `Expr` variants, including `Bool`, `If`, and `Repeat`.
* Tightened error messaging across parser and evaluator paths to make syntax and runtime issues clearer for beginners.
