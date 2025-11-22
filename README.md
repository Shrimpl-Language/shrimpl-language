# Shrimpl 0.5 Programming Language

![Shrimpl logo](https://raw.githubusercontent.com/adl5423/shrimpl-language/main/assets/shrimpl-banner.png)

## Introduction

Shrimpl is a small, beginner‑friendly programming language designed to bridge the gap between visual languages (like Scratch) and general‑purpose languages (like Python or JavaScript). It aims to be:

* **Readable**: English‑like keywords, minimal punctuation, and indentation instead of braces.
* **Approachable**: Designed so kids and new programmers can learn concepts gradually.
* **Practical**: Powerful enough to build web APIs, perform data analysis, and run simple machine‑learning workflows.

Shrimpl programs are interpreted by a Rust‑based runtime that can run on most platforms. A companion Language Server (LSP) and browser‑based API Studio make it easy to experiment, debug, and explore programs interactively.

---

## Goals

### Accessibility

* Use **plain words** and **minimal punctuation**.
* Rely on **indentation** to express structure instead of braces.
* Favor **clear error messages** and **friendly diagnostics**.
* Follow documentation guidelines inspired by writethedocs.org: short sentences, clear examples, and progressive disclosure of complexity.

### Ease of Use

* Provide **first‑class constructs** for building HTTP servers and endpoints.
* Avoid heavy frameworks or configuration files.
* Make the default experience “type code, run, see results.”

### Expressiveness

Shrimpl supports:

* Variables and expressions
* Functions and classes (with static methods)
* Built‑in helpers for text, numbers, vectors/tensors, dataframes, and linear regression
* HTTP client utilities for calling external APIs

### Safety

* Runtime checks catch errors whenever possible and report **clear messages**.
* Static analysis warns about:

  * unused path parameters
  * unused function/method parameters
  * duplicate endpoints with the same method and path

These diagnostics are visible both in the terminal and inside editor integrations via the LSP.

---

## Getting Started

### Installing the Shrimpl Interpreter

1. **Install Rust** (if you do not already have it):

   * Visit the Rust website and install via `rustup`.

2. **Install Shrimpl using Cargo**:

   ```bash
   cargo install shrimpl
   ```

   Alternatively, you can build the interpreter from source in this repository using:

   ```bash
   cargo build --release
   ```

   The resulting binary will be in `target/release/shrimpl`.

3. **Replit deployments**:

   * Ensure the Rust toolchain is available in `replit.nix`.
   * Add any necessary Rust dependencies.
   * Configure `.replit` to run the Shrimpl server (for example using `cargo run --bin shrimpl` or the provided run command).

### Creating Your First Program

1. In your project directory, create a file named **`app.shr`**. Shrimpl scripts always use the `.shr` extension.

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

   You should see:

   ```text
   Hello, Shrimpl!
   ```

5. To only check syntax and diagnostics (without running the server), use:

   ```bash
   shrimpl --file app.shr check
   ```

---

## Language Overview

### Indentation and Structure

Shrimpl uses **indentation** to define blocks. Each statement is on its own line; nested blocks are indented consistently (two spaces is typical). Examples:

```shrimpl
server 3000

endpoint GET "/":
  "Hello, Shrimpl!"  # body is indented
```

Mixing spaces and tabs is discouraged and may cause parsing issues. Choose one (spaces are recommended) and keep it consistent.

---

## Servers

A Shrimpl program that exposes HTTP endpoints must declare a **server**:

```shrimpl
server 3000
```

* The number is the **port** the server listens on.
* Only **one** `server` declaration is allowed per program.
* The server must appear **before** any `endpoint` declarations.

If multiple servers are declared, the interpreter will issue a warning.

---

## Endpoints

Endpoints declare HTTP routes. An endpoint has:

* An **HTTP method**: `GET` or `POST`.
* A **path**: a quoted string, optionally including path parameters like `"/hello/:name"`.
* A **body**: an indented expression that determines the response.

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
* The variable `name` becomes `"Aisen"`.
* Response: `"Hello Aisen"`.

### Query Parameters

Query parameters are also exposed as variables:

```shrimpl
endpoint GET "/greet":
  "Hello " + name
```

* Request: `/greet?name=Aisen`
* The variable `name` is set to `"Aisen"`.

### Text vs JSON Endpoints

The endpoint body can return **text** or **JSON**:

```shrimpl
# Plain text
endpoint GET "/text": "Just a string"

# JSON
endpoint GET "/info":
  json { "name": "Shrimpl", "version": 0.5 }
```

When using `json { ... }`, the body must be a **constant** JSON object (expressions are not evaluated inside the JSON literal).

---

## Expressions

Shrimpl supports a small but expressive set of expression features:

### Literals

* Numbers: `42`, `3.14`
* Strings: `"Hello"`
* JSON objects: `json { "key": 123 }`

### Variables

Names starting with a letter or underscore. Common sources:

* Path parameters (e.g., `:id` -> `id`)
* Query parameters (e.g., `?foo=bar` -> `foo`)
* Function parameters
* Method parameters

### Operators

* Arithmetic: `+`, `-`, `*`, `/` (normal precedence)
* String concatenation: `+` (if either side is a string, result is a string)

### Function and Method Calls

* Function call: `funcName(arg1, arg2)`
* Class method call: `ClassName.methodName(arg1, arg2)`

---

## Variables and Data Types

Shrimpl is **dynamically typed**. Values can be:

* Numbers
* Strings
* JSON arrays/objects

### Core Built‑ins

These functions convert between data types or operate on them:

| Built‑in       | Description                                       |
| -------------- | ------------------------------------------------- |
| `number(x)`    | Convert string or number `x` to a floating‑point. |
| `string(x)`    | Convert any value to a string.                    |
| `len(x)`       | Length of a string.                               |
| `upper(x)`     | Convert a string to uppercase.                    |
| `lower(x)`     | Convert a string to lowercase.                    |
| `sum(a,b,...)` | Sum of numbers.                                   |
| `avg(a,b,...)` | Average of numbers.                               |
| `min(a,b,...)` | Minimum of numbers.                               |
| `max(a,b,...)` | Maximum of numbers.                               |

---

## Functions

Define reusable calculations with the `func` keyword:

```shrimpl
func greet(name):
  "Hello " + name
```

Rules:

* Syntax: `func name(param1, param2, ...): <expression>`
* The body is a **single expression** whose value is returned.
* Parameters are local variables inside the function body.
* Unused parameters trigger a **warning** from the static analyzer.

Example usage inside an endpoint:

```shrimpl
endpoint GET "/welcome/:name":
  greet(name) + "!"
```

---

## Classes

Shrimpl supports simple **classes with static methods**. Use them to group related functionality:

```shrimpl
class Math:
  double(x): x * 2
  square(x): x * x
```

* Syntax: `class Name:` followed by indented method definitions.
* Methods: `methodName(params): expression`.
* Methods do not have access to `self` or instance data; they are purely static.

Usage:

```shrimpl
endpoint GET "/double/:n":
  Math.double(number(n))
```

---

## Built‑In Libraries

### HTTP Client

Use these to call external APIs:

| Function             | Description                                                               |
| -------------------- | ------------------------------------------------------------------------- |
| `http_get(url)`      | Send HTTP GET request to `url`, return **raw body** as a string.          |
| `http_get_json(url)` | GET `url`, parse response as JSON, and return pretty‑printed JSON string. |

Example:

```shrimpl
endpoint GET "/pokemon/:id":
  http_get_json("https://pokeapi.co/api/v2/pokemon/" + id)
```

### Vector and Tensor Operations

The runtime includes helpers for numeric arrays:

| Function           | Description                                                                  |
| ------------------ | ---------------------------------------------------------------------------- |
| `vec(a, b, ...)`   | Create a JSON array `[a, b, ...]`. Numeric strings are converted to numbers. |
| `tensor_add(a, b)` | Elementwise add two JSON arrays of equal length; returns a JSON array.       |
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

| Function                   | Description                                                               |
| -------------------------- | ------------------------------------------------------------------------- |
| `df_from_csv(url)`         | Download CSV from `url` and return dataframe JSON. Numbers become floats. |
| `df_head(df_json, n)`      | Return first `n` rows of the dataframe.                                   |
| `df_select(df_json, cols)` | Return new dataframe with only specified columns (`"col1,col2"`).         |

Examples:

```shrimpl
endpoint GET "/load":
  df_from_csv("https://people.sc.fsu.edu/~jburkardt/data/csv/hw_200.csv")

endpoint GET "/head":
  df_head(df, 5)
```

### Machine Learning (Linear Regression)

Shrimpl 0.5 includes basic linear regression support. Models are JSON objects:

```json
{ "kind": "linreg", "a": 2.0, "b": 0.0 }
```

| Function                        | Description                                                     |
| ------------------------------- | --------------------------------------------------------------- |
| `linreg_fit(xs_json, ys_json)`  | Train regression model from arrays `xs` and `ys` (JSON arrays). |
| `linreg_predict(model_json, x)` | Predict `y` from model and input `x`; returns a number.         |

Examples:

```shrimpl
endpoint GET "/train":
  linreg_fit(xs, ys)

endpoint GET "/predict":
  linreg_predict(model, number(x))
```

---

## JSON Responses

To return JSON, prefix the body with `json` and provide a constant JSON object:

```shrimpl
endpoint GET "/info":
  json { "name": "Shrimpl", "version": 0.5 }
```

* The interpreter does **not** evaluate expressions inside `json { ... }`.
* Use this style for configuration‑like responses or metadata.

---

## Diagnostics and Warnings

Shrimpl performs static analysis to help keep programs clean and predictable. The diagnostics engine currently checks for:

* **Unused path parameters**: `endpoint GET "/:id"` where `id` never appears in the body.
* **Unused function parameters**: `func foo(x, y)` where `y` is never used.
* **Unused method parameters** in classes.
* **Duplicate endpoints**: same HTTP method and path appearing more than once.

Diagnostics appear in several places:

1. **Command line** via `shrimpl --file app.shr check`.
2. **API Studio** (browser UI) under the “Diagnostics” panel.
3. **Editors** that use the Shrimpl LSP (listed below), using standard LSP diagnostics.

---

## Shrimpl API Studio (Web UI)

When a Shrimpl server is running, you can open the interactive API Studio:

```text
http://localhost:<port>/__shrimpl/ui
```

The UI provides:

* **Endpoint Explorer**

  * Lists all endpoints with method, path, and body type.
* **Request Panel**

  * Fill in path parameters and query strings.
  * Send requests and inspect responses without leaving the browser.
* **Response Panel**

  * Shows status code, timestamp, and body.
  * Pretty‑prints JSON responses.
* **Source Panel**

  * Displays `app.shr` with syntax highlighting.
  * Highlighting mirrors the editor grammars (keywords, methods, strings, numbers, comments).
* **Diagnostics Panel**

  * Lists warnings and (future) errors.
  * Each item includes a `kind`, `scope` (endpoint/function/method), and a human‑readable message.

This UI is designed for learners who benefit from immediate feedback: change code, refresh the page, and see how behavior and diagnostics change.

---

## Language Server Protocol (LSP) Support

Shrimpl ships with a dedicated Language Server, **`shrimpl-lsp`**, which provides IDE‑style features for `.shr` files.

### Capabilities

The Shrimpl LSP currently implements:

* **Live diagnostics**

  * Parses the open document and reports syntax errors.
  * Runs the same static checks as the Shrimpl docs module (unused parameters, duplicate endpoints, etc.).
* **Hover information**

  * Hover over `server`, `endpoint`, function names, class names, or methods to see a short description.
* **Completions**

  * Keyword stubs such as `server`, `endpoint`, `func`, `class`, `GET`, `POST`.
  * Can be extended with snippets and common patterns.
* **Document symbols (outline)**

  * Exposes servers, endpoints, functions, and classes as `DocumentSymbol`s.
  * Lets editors show an outline/tree view for `.shr` files.

### Running the LSP Server

Build and run the LSP binary from this repo:

```bash
cargo build --bin shrimpl-lsp

# or run directly
cargo run --bin shrimpl-lsp
```

The LSP speaks JSON‑RPC 2.0 over stdio (the standard model used by most editors).

### Editor Integrations

In the repository, the `editors/` folder contains sample configurations:

* **VS Code** (`editors/vscode/`)

  * `package.json`: VS Code extension manifest (language id, activation events, LSP wiring).
  * `extension.ts`: connects VS Code to the `shrimpl-lsp` binary and registers the language.
  * `syntaxes/shrimpl.tmLanguage.json`: TextMate grammar mirroring the API Studio highlighting.
  * `language-configuration.json`: comment syntax (`#`), bracket pairs, word patterns, etc.
  * To use locally:

    1. Open `editors/vscode` in VS Code.
    2. Run `npm install`.
    3. Use `F5` to start an “Extension Development Host” and open a folder with `.shr` files.

* **Neovim** (`editors/nvim-shrimpl/shrimpl.lua`)

  * Lua configuration that registers `shrimpl-lsp` with `nvim-lspconfig`.
  * You can copy or adapt the config into your own Neovim setup.

* **Sublime Text** (`editors/sublime/`)

  * Example `LSP-shrimpl.sublime-settings` for the Sublime LSP plugin.
  * Settings point the plugin at the `shrimpl-lsp` binary and restrict it to `*.shr` files.

* **JetBrains** (`editors/jetbrains/shrimpl-lsp.json`)

  * Sample configuration for JetBrains IDEs with LSP support, mapping `.shr` to the Shrimpl language server.

These examples are intended as starting points; you can customize paths, commands, and additional capabilities as needed.

---

## Best Practices

* **Use meaningful names**

  * Name functions, classes, and parameters descriptively to reduce the need for comments.

* **Keep functions small**

  * Each `func` or method should evaluate a single expression.
  * Break complex logic into smaller helper functions.

* **Validate external input**

  * When calling external APIs with `http_get`/`df_from_csv`, ensure the response format matches what your code expects.

* **Handle errors gracefully**

  * Consider returning error strings or JSON objects when user input is missing or invalid.

* **Comment intention, not mechanics**

  * Use `#` comments to explain *why* something exists, especially in tutorial code.

* **Stay consistent**

  * Indent with two spaces.
  * Stick to one naming convention for endpoints and parameters.

---

## Implementation Notes (High‑Level)

Shrimpl’s implementation (in this repository) is roughly organized as follows:

* **Parser (`src/parser/`)**

  * Tokenizes and parses `.shr` source into an abstract syntax tree (AST).
  * The AST types live in `parser/ast.rs`.

* **AST / Core Model (`src/ast.rs`, `src/parser/ast.rs`)**

  * Defines core entities such as `Program`, `Endpoint`, `FunctionDef`, `ClassDef`, and `Expr`.

* **Interpreter (`src/interpreter/`)**

  * Evaluates Shrimpl expressions.
  * Implements HTTP server wiring and endpoint dispatch.
  * Integrates built‑in libraries (HTTP client, vectors, tensors, dataframes, linear regression).

* **Docs and Diagnostics (`src/docs.rs`)**

  * Builds a JSON schema for the API Studio (`/__shrimpl/schema`).
  * Computes static diagnostics (unused params, duplicate endpoints, etc.).

* **CLI / Entry Point (`src/main.rs`)**

  * Parses command‑line flags such as `--file`, `run`, `check`, `diagnostics`.
  * Invokes the interpreter or diagnostics mode accordingly.

* **Language Server (`src/bin/shrimpl_lsp.rs`)**

  * Implements the LSP server using `tower-lsp`.
  * Reuses the parser and docs diagnostics to provide:

    * `textDocument/publishDiagnostics`
    * `textDocument/hover`
    * `textDocument/completion`
    * `textDocument/documentSymbol`

* **API Studio UI (`src/docs.rs` embedded HTML)**

  * Serves the `__shrimpl/ui` page.
  * Fetches schema, diagnostics, and source from HTTP endpoints exposed by the interpreter.
  * Renders interactive panels in the browser using vanilla HTML/CSS/JS.

This architecture keeps the **language core** (parser + interpreter + docs) separate from **presentation layers** (CLI, LSP, API Studio, editor plugins).

---

## Conclusion

Shrimpl 0.5 combines the simplicity of visual programming with the power of modern languages. It offers a gentle introduction to:

* Server‑side programming and HTTP APIs
* Data manipulation using dataframes and vectors
* Basic machine learning with linear regression

The interpreter, diagnostics engine, API Studio, and LSP work together to create a friendly learning environment. Learners can:

1. Write simple `.shr` files.
2. See immediate results in the browser.
3. Get real‑time feedback from their editor.
4. Grow into more advanced topics (dataframes, ML, external APIs) without leaving Shrimpl.

For further examples and community discussion, visit the project repository, share your own Shrimpl programs, and help shape future versions of the language.
