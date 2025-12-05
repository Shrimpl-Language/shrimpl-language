# Shrimpl 0.5 Programming Language

[![Shrimpl banner](assets/shrimpl_banner.png)](https://shrimpl.dev)

<p align="center">
  <a href="https://shrimpl.dev" target="_blank" style="padding-right: 12px;">
    <img 
      src="https://img.shields.io/badge/Visit%20Website-005BBB?style=for-the-badge&logo=google-chrome&logoColor=white" 
      alt="Shrimpl Website"
    />
  </a>

  <a href="https://discord.gg/V2qWcNHqvY" target="_blank" style="padding-left: 12px;">
    <img 
      src="https://img.shields.io/badge/Join%20Discord-5865F2?style=for-the-badge&logo=discord&logoColor=white" 
      alt="Discord"
    />
  </a>
</p>

--------------------------------------

## Introduction

Shrimpl is a beginner‑friendly programming language designed to bridge the gap between visual languages (like Scratch) and general‑purpose languages (like Python or JavaScript). It aims to be:

* **Readable**: English‑like keywords, minimal punctuation, and indentation instead of braces.
* **Approachable**: Designed so kids and new programmers can learn concepts gradually.
* **Practical**: Powerful enough to build web APIs, perform data analysis, run simple machine‑learning workflows, and experiment with AI helpers powered by OpenAI.

Shrimpl programs are interpreted by a Rust‑based runtime that can run on most platforms. A companion Language Server (LSP) and browser‑based API Studio make it easy to experiment, debug, and explore programs interactively.

Shrimpl 0.5.x adds several important features:

* Optional **AI helpers** to call OpenAI models (`openai_chat`, `openai_chat_json`, `openai_mcp_call`).
* New **control‑flow expressions**: booleans, comparisons, logical operators, `if / elif / else` expressions, and `repeat N times` loops.
* A simple **configuration system** (`config/config.<env>.json`) for server options, JWT auth, request validation, and optional static type annotations.
* Built‑in **JWT‑aware HTTP server** with protected paths and request‑scoped variables such as `jwt_sub`.
* Per‑endpoint **JSON Schema validation** and automatic input sanitization.
* An optional **static type checker** driven by config‑based annotations.
* A small **lockfile** (`shrimpl.lock`) capturing version and hash information.

The goal is to keep Shrimpl simple enough for kids and beginners, while gradually introducing real‑world concepts like APIs, authentication, validation, and types.

---

## Goals

### Accessibility

* Use **plain words** and **minimal punctuation**.
* Rely on **indentation** instead of braces.
* Provide **clear error messages** and **friendly diagnostics**.
* Follow documentation practices inspired by writethedocs.org: short sentences, clear examples, and progressive disclosure of complexity.

### Ease of Use

* Provide **first‑class constructs** for building HTTP servers and endpoints.
* Avoid heavy frameworks or complex configuration.
* Make the default experience “type code, run, see results.”

### Expressiveness

Shrimpl supports:

* Variables and expressions
* Functions and classes (with static methods)
* Control flow expressions (`if / elif / else`, `repeat`)
* Built‑in helpers for text, numbers, vectors/tensors, dataframes, and linear regression
* HTTP client utilities for calling external APIs
* Optional AI helpers for calling OpenAI models (chat‑style responses and JSON payloads)
* Optional static type annotations configured in JSON

### Safety

* Runtime checks catch errors wherever possible and report **clear messages**.
* Static analysis warns about:

  * unused path parameters
  * unused function/method parameters
  * duplicate endpoints with the same method and path
* JSON Schema validation rejects malformed requests before Shrimpl code runs.
* A simple type checker can detect type mismatches between annotated functions and their bodies.

Diagnostics are visible both in the terminal, in `__shrimpl/diagnostics`, and inside editor integrations via the LSP.

---

## Getting Started

### Installing the Shrimpl Interpreter

1. **Install Rust** (if it is not already installed):

   * Visit the official Rust website and install via `rustup`.

2. **Install Shrimpl using Cargo**:

   ```bash
   cargo install shrimpl
   ```

   Alternatively, build the interpreter from source in this repository:

   ```bash
   cargo build --release
   ```

   The resulting binary will be in `target/release/shrimpl`.

3. **Replit deployments** (optional):

   * Ensure the Rust toolchain is available in `replit.nix`.
   * Add any necessary Rust dependencies.
   * Configure `.replit` to run the Shrimpl server (for example `cargo run --bin shrimpl -- --file app.shr run`).

### Creating Your First Program

1. In the project directory, create a file named **`app.shr`**. Shrimpl scripts always use the `.shr` extension.

2. Put this minimal program in `app.shr`:

   ```shrimpl
   server 3000

   endpoint GET "/": "Hello, Shrimpl!"
   ```

3. Run the program:

   ```bash
   shrimpl --file app.shr run
   ```

   This will:

   * Parse `app.shr`.
   * Perform basic checks.
   * Start a web server on port `3000`.

4. Open a browser and navigate to:

   ```text
   http://localhost:3000/
   ```

   The response should be:

   ```text
   Hello, Shrimpl!
   ```

5. To only check syntax and diagnostics (without running the server), use:

   ```bash
   shrimpl --file app.shr check
   ```

6. To inspect all static diagnostics as JSON, use:

   ```bash
   shrimpl --file app.shr diagnostics
   ```

   This is the same data shown in the API Studio diagnostics panel.

---

## Project Files and Environments

### Single Entry File and Imports

Shrimpl programs start from a **single entry file**, usually `app.shr`.

To keep larger projects organized, Shrimpl supports a simple import mechanism:

```shrimpl
# app.shr
server 3000

import "users.shr"
import "math_utils.shr"

endpoint GET "/": "Welcome to Shrimpl"
```

In `users.shr`:

```shrimpl
# users.shr

func user_greeting(name):
  "Hello, " + name

endpoint GET "/hello/:name":
  user_greeting(name)
```

Notes:

* `import "relative/path/file.shr"` inlines the contents of the other file.
* Paths are resolved **relative to the file that performs the import**.
* Each file is loaded only once. Cycles (`a.shr` importing `b.shr` which imports `a.shr`) are ignored after the first visit.

### Config Files (`config/config.<env>.json`)

Shrimpl loads configuration from a JSON file based on the current environment:

* Environment name is taken from `SHRIMPL_ENV` or defaults to `"dev"`.
* Config file path: `config/config.<env>.json` (for example `config/config.dev.json`).

A typical config file:

```json
{
  "server": {
    "port": 3000,
    "tls": false
  },
  "auth": {
    "jwt_secret_env": "SHRIMPL_JWT_SECRET",
    "protected_paths": ["/secure", "/admin"],
    "allow_missing_on": ["/health"]
  },
  "validation": {
    "schemas": {
      "/login": {
        "type": "object",
        "required": ["email", "password"],
        "properties": {
          "email": { "type": "string", "format": "email" },
          "password": { "type": "string", "minLength": 8 }
        }
      }
    }
  },
  "types": {
    "functions": {
      "add": {
        "params": ["number", "number"],
        "result": "number"
      }
    }
  },
  "secrets": {
    "env": {
      "OPENAI": "SHRIMPL_OPENAI_API_KEY"
    }
  },
  "values": {
    "greeting": "Hello Shrimpl",
    "threshold": 0.75,
    "debug": true
  }
}
```

What each section controls:

* `server`: Port and TLS flag (can override `server` declaration in Shrimpl code).
* `auth`: JWT configuration and which paths require authentication.
* `validation`: Per‑path JSON Schemas for request body validation.
* `types`: Type annotations for functions (used by the static type checker).
* `secrets.env`: Mapping from logical secret names to environment variable names.
* `values`: Arbitrary key/value pairs accessible via built‑ins (if enabled in the runtime).

### Lockfile (`shrimpl.lock`)

When the program runs, the runtime may create a `shrimpl.lock` file that captures:

* Shrimpl CLI version.
* Environment name (`SHRIMPL_ENV`).
* Entry path (`app.shr`).
* SHA‑256 hash of the entry file.
* Timestamp of generation.

This file is informational. It is safe to delete; it will be regenerated when needed.

---

## Servers and TLS

A Shrimpl program that exposes HTTP endpoints must declare a **server**:

```shrimpl
server 3000
```

* The number is the **port** the server listens on.
* Exactly **one** `server` declaration is allowed per program.
* The server must appear **before** any `endpoint` declarations.

Configuration from `config/config.<env>.json` can override the port and TLS flag if present.

### Enabling TLS (HTTPS)

To serve HTTPS directly from Shrimpl:

```shrimpl
server 3000 tls
```

In this mode the runtime expects certificate files, configured via environment variables:

* `SHRIMPL_TLS_CERT` (defaults to `cert.pem`)
* `SHRIMPL_TLS_KEY` (defaults to `key.pem`)

Both files should be PEM‑encoded. When TLS is enabled, the server binds HTTPS on `0.0.0.0:<port>`.

---

## Endpoints

Endpoints declare HTTP routes. An endpoint has:

* An **HTTP method**: `GET` or `POST`.
* A **path**: a quoted string, optionally including path parameters like `"/hello/:name"`.
* A **body**: an expression that determines the response.

The body expression can live on the same line as the endpoint or on the following indented line.

### Basic Example

```shrimpl
endpoint GET "/": "Hello, Shrimpl!"
```

### Path Parameters

Path segments starting with `:` become variables in the endpoint body:

```shrimpl
endpoint GET "/hello/:name":
  "Hello " + name
```

* Request: `GET /hello/Aisen`
* Variable `name` becomes `"Aisen"`.
* Response: `"Hello Aisen"`.

### Query Parameters

Query parameters are also exposed as variables:

```shrimpl
endpoint GET "/greet":
  "Hello " + name
```

* Request: `/greet?name=Aisen`
* Variable `name` is set to `"Aisen"`.

If a path parameter and query parameter share a name, the **path** parameter wins.

### POST Endpoints and the `body` Variable

For `POST` endpoints, the request body is exposed as a special variable named `body`:

```shrimpl
endpoint POST "/echo":
  body
```

* The server reads the raw body bytes.
* If validation (JSON Schema) is configured for this path, the body is parsed as JSON, validated, sanitized, and then re‑serialized back into `body`.
* If no validation schema is configured, `body` is simply the raw text.

A typical JSON endpoint:

```shrimpl
endpoint POST "/login":
  # Assuming validation ensures email/password are present
  # and that body is a well‑formed JSON string.
  "Received login payload: " + body
```

### Text vs JSON Endpoints

The endpoint body can return **text** or **JSON**.

```shrimpl
# Plain text
endpoint GET "/text": "Just a string"

# Constant JSON
endpoint GET "/info":
  json { "name": "Shrimpl", "version": 0.5 }
```

When using `json { ... }`, the body must be a **constant** JSON object; expressions are **not** evaluated inside the JSON literal.

When using AI helpers (`openai_chat`, `openai_chat_json`), the return value is a string. The server sends it as a text response unless it is itself JSON.

### JWT‑Aware Variables (`jwt_sub`, `jwt_scope`, `jwt_role`)

When JWT auth is enabled (see the **Authentication and JWT** section), Shrimpl automatically injects three variables into every request:

* `jwt_sub`: subject / user id (string)
* `jwt_scope`: optional scope string
* `jwt_role`: optional role string

These variables always exist and default to empty strings if no token is present or if the path is not protected.

Example:

```shrimpl
endpoint GET "/secure/profile":
  if jwt_sub == "":
    "No user bound to this token"
  else:
    "Hello user " + jwt_sub
```

This allows endpoints to read identity information without worrying about missing variables.

---

## Expressions and Data Types

Shrimpl expressions are intentionally small and consistent. This version introduces booleans, comparisons, logical operators, and expression‑level control flow.

### Literal Values

Supported literals:

* Numbers: `42`, `3.14`, `-10`
* Strings: `"Hello"`, `"abc123"`
* Booleans: `true`, `false`
* Constant JSON: `json { "key": 123 }`

### Boolean Values and Truthiness

Booleans are first‑class values in Shrimpl:

```shrimpl
endpoint GET "/bools":
  if true:
    "This is always returned"
  else:
    "Never reached"
```

Truthiness rules used in `if`, `and`, `or`, and `repeat`:

* `Bool`: `true` and `false` behave as expected.
* `Number`: `0.0` is false; any other number is true.
* `String`: `""` is false; any other string is true.

### Variables

Names start with a letter or underscore and may contain letters, digits, or underscores.

Common sources:

* Path parameters (`:id` → `id`)
* Query parameters (`?foo=bar` → `foo`)
* Function parameters
* Method parameters
* Special variables: `body`, `jwt_sub`, `jwt_scope`, `jwt_role`

### Operators and Precedence

Shrimpl supports arithmetic, comparison, and logical operators.

Arithmetic:

* `+`, `-`, `*`, `/`
* If either operand of `+` is a string, Shrimpl performs string concatenation instead of numeric addition.

Comparison operators:

* `==`, `!=`, `<`, `<=`, `>`, `>=`

Logical operators:

* `and`, `or`

Operator precedence (from tightest to loosest):

1. `*`, `/`
2. `+`, `-`
3. Comparisons: `==`, `!=`, `<`, `<=`, `>`, `>=`
4. `and`
5. `or`

Example:

```shrimpl
endpoint GET "/logic":
  if 2 * 3 + 1 == 7 and true:
    "Math works"
  else:
    "Something is off"
```

### Control‑Flow Expressions

#### `if / elif / else` as an Expression

Shrimpl uses an expression‑oriented `if`:

```shrimpl
if condition1: expr1
elif condition2: expr2
else: expr3
```

This form can appear anywhere an expression is allowed:

```shrimpl
endpoint GET "/age":
  if number(age) < 13:
    "child"
  elif number(age) < 18:
    "teen"
  else:
    "adult"
```

Rules:

* Conditions are evaluated in order using truthiness rules.
* The first branch whose condition is true returns its expression value.
* If no branch matches and there is no `else`, the result is an empty string (`""`).

#### `repeat N times: expr`

A bounded loop expression:

```shrimpl
repeat N times: body_expr
```

Example:

```shrimpl
func repeat_greet(name, n):
  repeat number(n) times:
    "Hello " + name  # result of last iteration is returned

endpoint GET "/repeat-greet":
  repeat_greet(name, n)
```

Behavior:

* `N` is evaluated once and converted to a number.
* Negative values are treated as zero.
* There is a hard safety cap (for example 10,000 iterations) to avoid runaway loops.
* The result is the value of the **last** iteration, or `""` if `N == 0`.

---

## Functions

Define reusable computations with the `func` keyword:

```shrimpl
func greet(name):
  "Hello " + name
```

Rules:

* Syntax: `func name(param1, param2, ...): expression`
* The body is a **single expression** whose value is returned.
* Parameters are local to the function.
* Unused parameters trigger a static **warning**.

Example usage:

```shrimpl
endpoint GET "/welcome/:name":
  greet(name) + "!"
```

AI‑friendly wrapper:

```shrimpl
func tutor(topic):
  openai_chat("Explain this topic for a beginner: " + topic)

endpoint GET "/tutor/:topic":
  tutor(topic)
```

---

## Classes

Shrimpl supports **classes with static methods** for grouping related functions:

```shrimpl
class Math:
  double(x): x * 2
  square(x): x * x
```

* Syntax: `class Name:` followed by indented method definitions.
* Methods: `methodName(params): expression`.
* Methods have no `self` and act like static helpers.

Usage:

```shrimpl
endpoint GET "/double/:n":
  Math.double(number(n))
```

Classes can also be used to group domain‑specific helpers, such as formatting routines or domain logic.

---

## Built‑In Libraries

### Core Built‑ins

These helpers operate on basic values:

| Built‑in                              | Description                                               |
| ------------------------------------- | --------------------------------------------------------- |
| `number(x)`                           | Convert string or number `x` to a floating‑point number.  |
| `string(x)`                           | Convert any value to a string.                            |
| `len(x)`                              | Length of a string.                                       |
| `upper(x)`                            | String to uppercase.                                      |
| `lower(x)`                            | String to lowercase.                                      |
| `sum(a,b,...)`                        | Sum of numbers.                                           |
| `avg(a,b,...)`                        | Average of numbers.                                       |
| `min(a,b,...)`                        | Minimum of numbers.                                       |
| `max(a,b,...)`                        | Maximum of numbers.                                       |
| `openai_set_api_key(k)`               | Set/override the OpenAI API key used by AI helpers.       |
| `openai_set_system_prompt(p)`         | Set a global system prompt (role) for AI helpers.         |
| `openai_chat(msg)`                    | Call an OpenAI chat model; return reply text.             |
| `openai_chat_json(msg)`               | Call an OpenAI chat model; return full JSON as text.      |
| `openai_mcp_call(server, tool, args)` | Experimental helper for MCP/tool‑calling style workflows. |

### HTTP Client

Helpers for calling external APIs:

| Function             | Description                                                               |
| -------------------- | ------------------------------------------------------------------------- |
| `http_get(url)`      | Send HTTP GET to `url`, return raw body as a string.                      |
| `http_get_json(url)` | GET `url`, parse response as JSON, and return pretty‑printed JSON string. |

Example:

```shrimpl
endpoint GET "/pokemon/:id":
  http_get_json("https://pokeapi.co/api/v2/pokemon/" + id)
```

### Vector and Tensor Operations

Helpers for numeric arrays:

| Function           | Description                                                                  |
| ------------------ | ---------------------------------------------------------------------------- |
| `vec(a, b, ...)`   | Create a JSON array `[a, b, ...]`. Numeric strings are converted to numbers. |
| `tensor_add(a, b)` | Element‑wise add two JSON arrays of equal length; returns a JSON array.      |
| `tensor_dot(a, b)` | Dot product of two JSON arrays of equal length; returns a number.            |

Example:

```shrimpl
endpoint GET "/dot":
  tensor_dot(
    vec(number(ax), number(ay)),
    vec(number(bx), number(by))
  )
```

### DataFrames (Pandas‑like)

Dataframes are represented as JSON objects:

```json
{
  "columns": ["name", "age"],
  "rows": [["Alice", 30], ["Bob", 25]]
}
```

| Function                   | Description                                                                |
| -------------------------- | -------------------------------------------------------------------------- |
| `df_from_csv(url)`         | Download CSV from `url` and return dataframe JSON. Numbers become floats.  |
| `df_head(df_json, n)`      | Return first `n` rows of the dataframe.                                    |
| `df_select(df_json, cols)` | Return new dataframe with only specified columns (comma‑separated string). |

Examples:

```shrimpl
endpoint GET "/load":
  df_from_csv("https://people.sc.fsu.edu/~jburkardt/data/csv/hw_200.csv")

endpoint GET "/head":
  df_head(df, 5)
```

### Machine Learning (Linear Regression)

Simple linear regression is supported.

Model JSON shape:

```json
{ "kind": "linreg", "a": 2.0, "b": 0.0 }
```

| Function                        | Description                                                     |
| ------------------------------- | --------------------------------------------------------------- |
| `linreg_fit(xs_json, ys_json)`  | Train regression model from arrays `xs` and `ys` (JSON arrays). |
| `linreg_predict(model_json, x)` | Predict `y` from a model and input `x`; returns a number.       |

Examples:

```shrimpl
endpoint GET "/train":
  linreg_fit(xs, ys)

endpoint GET "/predict":
  linreg_predict(model, number(x))
```

---

## AI Helpers (OpenAI Integration)

AI helpers are optional built‑ins that talk to OpenAI models.

If no API key is configured and a program calls these helpers, they return an error string instead of crashing. This keeps the language safe for classrooms.

### Configuring the OpenAI API Key

The helpers look for an API key in this order:

1. `SHRIMPL_OPENAI_API_KEY`
2. `OPENAI_API_KEY`
3. A value set at runtime via `openai_set_api_key("...")`

The simplest setup:

```bash
export SHRIMPL_OPENAI_API_KEY="sk-example..."
shrimpl --file app.shr run
```

Alternatively, set the key from Shrimpl code (for example in a teacher‑only endpoint):

```shrimpl
endpoint POST "/setup":
  openai_set_api_key(secret)
```

### Setting a System Prompt (Role)

Models behave differently depending on their system prompt. Shrimpl exposes this via:

```shrimpl
openai_set_system_prompt("You are a friendly Shrimpl tutor for kids.")
```

Once set, this prompt is included in all subsequent AI calls.

### `openai_chat(message)`

`openai_chat` is the simplest helper:

1. Builds a chat request with the current system prompt (if any).
2. Sends it to a chat model (for example `gpt-4.1-mini`).
3. Returns the reply text as a string.

Example endpoint:

```shrimpl
server 3000

endpoint GET "/chat/:msg":
  openai_chat("Student says: " + msg)
```

Calling:

```text
GET /chat/What%20is%20a%20variable%3F
```

returns an explanation generated by the model.

### `openai_chat_json(message)`

`openai_chat_json` is similar to `openai_chat`, but returns the **full JSON response** as a pretty‑printed string. This is useful for debugging or advanced teaching.

Example:

```shrimpl
endpoint GET "/raw_chat/:msg":
  openai_chat_json(msg)
```

### `openai_mcp_call(server_id, tool_name, args)` (Experimental)

`openai_mcp_call` is designed for advanced tool‑calling/MCP workflows.

Signature:

```shrimpl
openai_mcp_call(server, tool, args)
```

* `server`: which tool server/config to talk to.
* `tool`: tool name.
* `args`: arguments as a JSON string.

Example (simplified):

```shrimpl
endpoint POST "/tools/query":
  openai_mcp_call("math-server", "solve_equation", args)
```

For most beginner use cases, focusing on `openai_chat` and `openai_chat_json` is enough.

### Error Handling for AI Calls

If an AI call fails (missing key, network issues, etc.), the helpers return an error message string. It can be shown directly or wrapped:

```shrimpl
endpoint GET "/safe_chat/:msg":
  "AI says: " + openai_chat(msg)
```

---

## Authentication and JWT

Shrimpl supports optional JWT‑based authentication configured via `config/config.<env>.json`.

### Configuring JWT Auth

In the config file:

```json
"auth": {
  "jwt_secret_env": "SHRIMPL_JWT_SECRET",
  "protected_paths": ["/secure", "/admin"],
  "allow_missing_on": ["/health"]
}
```

Fields:

* `jwt_secret_env`: name of the environment variable that holds the HMAC secret for verifying JWTs.
* `protected_paths`: list of path prefixes that **require** a valid JWT (for example `"/secure"`).
* `allow_missing_on`: list of path prefixes that are always accessible even if they overlap with protected paths (for example `"/health"`).

At runtime:

* Protected paths expect an `Authorization: Bearer <token>` header.
* Tokens are validated using the configured secret.
* On failure, the server returns `401` JSON errors:

  * `{"error":"missing bearer token"}`
  * `{"error":"unauthorized","detail":"..."}`

### JWT Claims Exposed to Shrimpl Code

Valid tokens are decoded into a claim set that includes:

* `sub` → exposed as `jwt_sub`
* `scope` → exposed as `jwt_scope`
* `role` → exposed as `jwt_role`

These variables are always defined and default to `""` when no token is present or when auth is not required.

Example:

```shrimpl
endpoint GET "/secure/hello":
  if jwt_sub == "":
    "You are not authenticated"
  else:
    "Hello, user " + jwt_sub
```

This allows beginners to work with authentication concepts without needing to parse headers manually.

---

## Request Validation and Sanitization

Shrimpl can validate JSON request bodies using JSON Schema defined in the config file.

### Declaring Schemas

In `config/config.<env>.json`:

```json
"validation": {
  "schemas": {
    "/login": {
      "type": "object",
      "required": ["email", "password"],
      "properties": {
        "email": { "type": "string", "format": "email" },
        "password": { "type": "string", "minLength": 8 }
      }
    }
  }
}
```

* Keys under `schemas` are Shrimpl endpoint paths (for example `"/login"`).
* Schema format is based on JSON Schema Draft 7.

### How Validation Works

For a `POST` to a path with a schema:

1. The raw body is read as a string.
2. The server attempts to parse it as JSON.
3. If parsing fails → `400` `{"error":"invalid_json","detail":"..."}`.
4. The JSON is validated against the schema.
5. If validation fails → `400` with `{"error":"validation_failed","detail":"..."}`.
6. If validation succeeds, the JSON is **sanitized** and re‑serialized.
7. The resulting sanitized string is stored in the `body` variable and passed to Shrimpl code.

Sanitization currently:

* Trims whitespace from all string values recursively (objects and arrays).

Example endpoint that assumes valid JSON:

```shrimpl
endpoint POST "/login":
  "Sanitized login payload: " + body
```

If a schema is misconfigured, the server responds with `500` and `{"error":"schema_compile_error","detail":"..."}` to avoid blaming the user.

---

## Optional Static Type Checker

Shrimpl includes an optional type checker that uses annotations defined in `config/config.<env>.json`.

### Declaring Function Types

Under `"types"` in config:

```json
"types": {
  "functions": {
    "add": {
      "params": ["number", "number"],
      "result": "number"
    },
    "greet": {
      "params": ["string"],
      "result": "string"
    }
  }
}
```

Supported type names:

* `number`, `float`, `int`, `integer` → numeric
* `string`, `str` → string
* `bool`, `boolean` → boolean
* Anything else → `any`

### What the Type Checker Does

For each annotated function:

1. Checks that the number of parameters in Shrimpl matches `params` length.
2. Builds a parameter type environment from `params`.
3. Infers a simple type for the function body using:

   * Literals (`Number`, `String`, `Bool`).
   * Variables (looked up in the environment or treated as `any`).
   * Binary operations (arithmetic yields `number`, comparisons yield `bool`, `and`/`or` yield `bool`).
   * Calls to annotated functions (using their declared result type).
   * `if` expressions (join of branch types; if mixed, treated as `any`).
   * `repeat` expressions (type of the body or `any`).
4. If the inferred body type is not assignable to the declared `result`, an error is produced.

Types are **assignable** if:

* The expected type is `any`, or
* `actual == expected`, or
* `actual` is `any`.

Diagnostics are reported as JSON objects with fields like:

```json
{
  "kind": "error",
  "scope": "function",
  "name": "add",
  "message": "Return type mismatch: expected number, got string"
}
```

### Viewing Type Diagnostics

Type diagnostics are included in:

* `shrimpl --file app.shr diagnostics`
* `GET /__shrimpl/diagnostics` (JSON)
* API Studio diagnostics panel

Because Shrimpl is still dynamically typed at runtime, type annotations are always optional. They provide **extra feedback**, not hard compilation barriers.

---

## JSON Responses

To return constant JSON, use the `json` prefix:

```shrimpl
endpoint GET "/info":
  json { "name": "Shrimpl", "version": 0.5 }
```

Notes:

* The runtime does not evaluate expressions inside `json { ... }`.
* Use this style for metadata, capability descriptions, or simple constant payloads.

For AI‑driven endpoints, typical patterns are:

```shrimpl
# Plain text from AI
endpoint GET "/chat/:msg":
  openai_chat(msg)

# Pretty‑printed JSON from AI
endpoint GET "/chat_json/:msg":
  openai_chat_json(msg)
```

---

## Diagnostics and Warnings

Shrimpl’s static analyzer helps keep programs clean.

Current checks include:

* **Unused path parameters**: `endpoint GET "/:id"` where `id` is not used.
* **Unused function parameters**.
* **Unused method parameters** in classes.
* **Duplicate endpoints**: same method and path more than once.
* Simple **type checking diagnostics** for functions annotated in config.

The analyzer understands all expression variants, including:

* `Expr::Bool` (boolean literals)
* `Expr::If` (`if / elif / else` expressions)
* `Expr::Repeat` (`repeat` loops)

Diagnostics appear in:

1. `shrimpl --file app.shr check`
2. `shrimpl --file app.shr diagnostics`
3. `GET /__shrimpl/diagnostics`
4. API Studio diagnostics panel
5. Editors that use the Shrimpl LSP

---

## Shrimpl API Studio (Web UI)

When a Shrimpl server is running, open:

```text
http://localhost:<port>/__shrimpl/ui
```

API Studio includes:

* **Endpoint Explorer**

  * Lists all endpoints with method and path.
* **Request Panel**

  * Fill in path parameters and query strings.
  * Send requests and see responses.
* **Response Panel**

  * Shows status code and body.
  * Pretty‑prints JSON.
* **Source Panel**

  * Fetches and displays `app.shr` with syntax highlighting.
* **Diagnostics Panel**

  * Shows static diagnostics (warnings, errors), including type checker results.

Additional internal endpoints:

* `GET /__shrimpl/schema` → machine‑readable schema for endpoints.
* `GET /__shrimpl/diagnostics` → diagnostics as JSON.
* `GET /__shrimpl/source` → raw `app.shr` contents.
* `GET /health` → simple health check returning JSON.

API Studio is ideal for:

* Exploring endpoints.
* Testing AI helpers with different prompts.
* Showing the relationship between code, requests, and responses.

---

## Language Server Protocol (LSP) Support

Shrimpl ships with a Language Server, **`shrimpl-lsp`**, that provides editor features.

### Capabilities

* **Live diagnostics** (syntax, static checks, type hints).
* **Hover information** (`server`, `endpoint`, functions, classes).
* **Completions** (keywords like `server`, `endpoint`, `func`, `class`, `GET`, `POST`).
* **Document symbols** (outline of endpoints, functions, classes).

### Running the LSP

Build and run:

```bash
cargo build --bin shrimpl-lsp
cargo run --bin shrimpl-lsp
```

The server speaks JSON‑RPC 2.0 over stdio, which most editors can connect to.

### Editor Integrations

Sample configurations live under `editors/`:

* **VS Code** (`editors/vscode/`)
* **Neovim** (`editors/nvim-shrimpl/`)
* **Sublime Text** (`editors/sublime/`)
* **JetBrains** (`editors/jetbrains/`)

Each setup wires `.shr` files to `shrimpl-lsp` and provides syntax highlighting plus diagnostics.

---

## Logging

Shrimpl logs each HTTP request as a JSON line on stdout, including:

* Timestamp
* Level (`"info"`)
* Kind (`"http-request"`)
* HTTP method
* Path
* Status code
* Client address
* Elapsed time in milliseconds
* `auth_ok` flag indicating whether the request had valid auth claims

This format makes it easy to feed logs into other tools or to demonstrate structured logging in teaching environments.

---

## Best Practices

* **Use meaningful names** for endpoints and functions.
* **Keep functions small** and focused on a single idea.
* **Validate external input** using JSON Schema where possible.
* **Handle errors explicitly** in endpoint bodies (missing params, invalid values).
* **Use comments** (`#`) to explain intention, especially in tutorial code.
* **Stay consistent** with indentation (two spaces) and naming.
* **Introduce AI helpers gradually** after students are comfortable with basic endpoints.

---

## Implementation Notes (High‑Level)

The repository is organized into clear layers:

* **Parser (`src/parser/`)**

  * Tokenizes and parses `.shr` source into an abstract syntax tree (AST).
  * Supports boolean literals, comparison and logical operators, `if / elif / else`, and `repeat` expressions.

* **AST / Core Model (`src/parser/ast.rs`)**

  * Types for `Program`, `EndpointDecl`, `FunctionDef`, `ClassDef`, `Expr`, and more.

* **Interpreter (`src/interpreter/`)**

  * Evaluates Shrimpl expressions.
  * Hosts the Actix‑Web HTTP server.
  * Implements JWT auth, validation, sanitization, and logging.
  * Integrates built‑in libraries (HTTP client, vectors, dataframes, linear regression, AI helpers).

* **Config (`src/config.rs`)**

  * Loads `config/config.<env>.json`.
  * Exposes server, auth, validation, types, secrets, and values sections.

* **Lockfile (`src/lockfile.rs`)**

  * Computes and writes `shrimpl.lock` with version, environment, entry path, and hash.

* **Docs and Diagnostics (`src/docs.rs`)**

  * Builds the schema for `/__shrimpl/schema`.
  * Computes static diagnostics, including type checker output.
  * Embeds the HTML/JS for `/__shrimpl/ui`.

* **CLI (`src/main.rs`)**

  * Parses command‑line arguments.
  * Provides `run`, `check`, and `diagnostics` modes.

* **Language Server (`src/bin/shrimpl_lsp.rs`)**

  * Implements LSP features on top of the parser and docs modules.

This separation keeps language design and teaching concerns clear while allowing the runtime to grow with features like JWT auth, validation, types, and AI integration.

---

## Conclusion

Shrimpl 0.5.x combines the simplicity of a teaching language with practical features drawn from real‑world API development:

* Server‑side programming and HTTP endpoints
* Control‑flow with expressions (`if`, `elif`, `else`, `repeat`)
* Data manipulation with vectors and dataframes
* Basic machine learning with linear regression
* Optional AI‑assisted endpoints with OpenAI helpers
* Optional JWT auth, JSON Schema validation, and static type checking

Learners can start with a few lines of code, see immediate results in the browser, and progressively discover more advanced ideas without leaving the language.
