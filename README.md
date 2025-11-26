# Shrimpl 0.5 Programming Language

[![Shrimpl banner](assets/shrimpl_banner.png)](https://shrimpl.dev)

<p align="center">
  <a href="https://shrimpl.dev" target="_blank" style="padding-right: 12px;">
    <img 
      src="https://img.shields.io/badge/Visit%20Website-005BBB?style=for-the-badge&logo=google-chrome&logoColor=white" 
      alt="Shrimpl Website"
    />
  </a>

  <a href="https://discord.gg/svnFRwfz" target="_blank" style="padding-left: 12px;">
    <img 
      src="https://img.shields.io/badge/Join%20Discord-5865F2?style=for-the-badge&logo=discord&logoColor=white" 
      alt="Discord"
    />
  </a>
</p>

## Introduction

Shrimpl is a small, beginner‑friendly programming language designed to bridge the gap between visual languages (like Scratch) and general‑purpose languages (like Python or JavaScript). It aims to be:

* **Readable**: English‑like keywords, minimal punctuation, and indentation instead of braces.
* **Approachable**: Designed so kids and new programmers can learn concepts gradually.
* **Practical**: Powerful enough to build web APIs, perform data analysis, run simple machine‑learning workflows, and now experiment with AI helpers powered by OpenAI.

Shrimpl programs are interpreted by a Rust‑based runtime that can run on most platforms. A companion Language Server (LSP) and browser‑based API Studio make it easy to experiment, debug, and explore programs interactively.

Shrimpl 0.5.2 adds optional **AI helpers** that allow endpoints to call OpenAI models using simple built‑in functions such as `openai_chat` and `openai_chat_json`. These helpers are designed so that:

* Teachers or parents can configure an API key once (via environment variable or a setup call).
* Students can use short, readable expressions to have Shrimpl "talk" to an AI model.

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
* Optional AI helpers for calling OpenAI models (chat‑style responses and JSON payloads)

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
* A **body**: an expression that determines the response.

The body expression can live on the same line as the endpoint or on the following indented line. Internally, Shrimpl parses the body as a normal expression, so you can call any built‑in or user‑defined function (including AI helpers like `openai_chat`).

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

### AI‑Powered Endpoints (OpenAI Helpers)

Shrimpl 0.5.2 introduces optional OpenAI helpers that let an endpoint call an AI model and return its answer. These helpers are available as **built‑in functions** that you can use anywhere you can write an expression.

A minimal AI‑powered endpoint looks like this:

```shrimpl
server 3000

endpoint GET "/chat/:msg":
  openai_chat("User said: " + msg)
```

If an appropriate OpenAI API key is configured (see the AI Helpers section below), calling:

```text
GET /chat/Hello
```

will send the prompt `"User said: Hello"` to an OpenAI model and return the model’s reply as plain text.

Because endpoint bodies are just expressions, you can combine AI helpers with other functions, such as formatting, numeric conversions, or even other API calls.

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

AI helpers follow the same rules. For example:

```shrimpl
openai_chat("Explain Shrimpl in one sentence.")
```

is just a function call whose result is a string.

---

## Variables and Data Types

Shrimpl is **dynamically typed**. Values can be:

* Numbers
* Strings
* JSON arrays/objects

### Core Built‑ins

These functions convert between data types or operate on them:

| Built‑in                              | Description                                             |
| ------------------------------------- | ------------------------------------------------------- |
| `number(x)`                           | Convert string or number `x` to a floating‑point.       |
| `string(x)`                           | Convert any value to a string.                          |
| `len(x)`                              | Length of a string.                                     |
| `upper(x)`                            | Convert a string to uppercase.                          |
| `lower(x)`                            | Convert a string to lowercase.                          |
| `sum(a,b,...)`                        | Sum of numbers.                                         |
| `avg(a,b,...)`                        | Average of numbers.                                     |
| `min(a,b,...)`                        | Minimum of numbers.                                     |
| `max(a,b,...)`                        | Maximum of numbers.                                     |
| `openai_set_api_key(k)`               | Set or override the OpenAI API key used by AI helpers.  |
| `openai_set_system_prompt(p)`         | Set a global system prompt (role) for AI helpers.       |
| `openai_chat(msg)`                    | Call an OpenAI chat model and return the reply as text. |
| `openai_chat_json(msg)`               | Call an OpenAI chat model and return full JSON as text. |
| `openai_mcp_call(server, tool, args)` | Experimental helper for Responses/MCP flows.            |

The OpenAI helpers are described in more detail in the **AI Helpers (OpenAI Integration)** section below.

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

You can also wrap AI helpers inside functions so that students can call them with simpler signatures. For example:

```shrimpl
func tutor(topic):
  openai_chat("Explain this topic for a beginner: " + topic)

endpoint GET "/tutor/:topic":
  tutor(topic)
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

You can also group AI helpers in a class if you want a more “object‑like” style, though the built‑ins are usually enough for beginner programs.

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

## AI Helpers (OpenAI Integration)

Shrimpl 0.5.2 adds **optional OpenAI helpers** that let programs talk to AI models using simple function calls. These helpers are meant to be used in educational settings where an adult configures an API key and students experiment with friendly prompts.

> Important: AI helpers are optional. Shrimpl runs perfectly fine without any API key set. If no OpenAI key is configured and a program calls one of these helpers, the runtime will return an error message string instead of crashing.

### Configuring the OpenAI API Key

The OpenAI helpers look for an API key in this order:

1. Environment variable `SHRIMPL_OPENAI_API_KEY`
2. Environment variable `OPENAI_API_KEY`
3. A key set at runtime via `openai_set_api_key("...")`

The simplest setup for a workshop or classroom is to set an environment variable **before** running Shrimpl:

```bash
export SHRIMPL_OPENAI_API_KEY="sk‑example..."

shrimpl --file app.shr run
```

Alternatively, you can call the helper inside your Shrimpl code (for example, in a setup endpoint that only teachers can hit):

```shrimpl
endpoint POST "/setup":
  openai_set_api_key(secret)
```

In that pattern, the teacher would send a POST request with a `secret` parameter and the server would remember the key for later calls.

### Setting a System Prompt (Role)

AI models behave differently depending on their “system prompt” (sometimes called the role). Shrimpl exposes this via:

```shrimpl
openai_set_system_prompt("You are a friendly Shrimpl tutor for kids.")
```

Once this is set, calls to `openai_chat` and `openai_chat_json` will prepend that system message to the conversation, so the model consistently answers in that role.

A typical pattern is to call this once when the program starts or in a dedicated configuration endpoint:

```shrimpl
endpoint POST "/configure_ai":
  openai_set_system_prompt("Explain things clearly in one or two paragraphs.")
```

### `openai_chat(message)`

`openai_chat` is the most student‑friendly helper. It:

1. Builds a chat request with the current system prompt (if any) and the user message.
2. Sends it to an OpenAI chat model (by default `gpt‑4.1‑mini`).
3. Returns the model’s reply text as a normal Shrimpl string.

Example endpoint:

```shrimpl
server 3000

endpoint GET "/chat/:msg":
  openai_chat("Student says: " + msg)
```

If you call:

```text
GET /chat/What%20is%20a%20variable%3F
```

the endpoint will return a short explanation generated by the model.

Because `openai_chat` returns a string, you can further process or format it with other functions, such as `upper`, `lower`, or `string`.

### `openai_chat_json(message)`

Sometimes you want to see the **full JSON response** from the model (for debugging, teaching, or building custom tools). `openai_chat_json` works like `openai_chat`, but returns the whole JSON response as a pretty‑printed string.

Example:

```shrimpl
endpoint GET "/raw_chat/:msg":
  openai_chat_json(msg)
```

This is useful in classrooms when you want to show students what the model actually returns: choices, usage, and other metadata.

### `openai_mcp_call(server_id, tool_name, args)` (Experimental)

`openai_mcp_call` is an **experimental helper** meant for more advanced setups where Shrimpl is used together with tool‑calling or MCP‑style servers.

Its signature is:

```shrimpl
openai_mcp_call(server, tool, args)
```

* `server`: a string that identifies which tool server or configuration to talk to.
* `tool`: the name of the tool to call.
* `args`: a string containing JSON for the tool arguments.

The helper formats these into a prompt and sends a request using OpenAI’s Responses API. The raw JSON result is returned as a pretty‑printed string.

Example (simplified):

```shrimpl
endpoint POST "/tools/query":
  openai_mcp_call("math‑server", "solve_equation", args)
```

For most beginner and classroom scenarios, you can ignore this helper and focus on `openai_chat` and `openai_chat_json`.

### Safety and Error Messages

If something goes wrong during an AI call (for example, no API key is configured or the network fails), the helper will return a string that describes the error. You can either show this directly to the user or wrap it in your own message:

```shrimpl
endpoint GET "/safe_chat/:msg":
  "AI says: " + openai_chat(msg)
```

Teachers may also want to guide students on responsible prompt writing and explain that AI outputs are not always correct.

---

## JSON Responses

To return JSON, prefix the body with `json` and provide a constant JSON object:

```shrimpl
endpoint GET "/info":
  json { "name": "Shrimpl", "version": 0.5 }
```

* The interpreter does **not** evaluate expressions inside `json { ... }`.
* Use this style for configuration‑like responses or metadata.

For AI‑driven endpoints, the most common pattern is to return text (`openai_chat`) or pretty‑printed JSON (`openai_chat_json`). If you need more structured output, you can use `openai_chat_json` and then post‑process the JSON in another system.

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

AI helpers are just built‑in functions, so they participate in diagnostics the same way as any other call. For example, a parameter like `:prompt` that is never passed to `openai_chat(prompt)` will be flagged as unused.

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

When experimenting with AI helpers, the API Studio is a convenient way to:

* Try different prompts quickly.
* Compare responses when you change the system prompt.
* Show students how HTTP requests and AI responses relate.

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

Future versions of the LSP may provide richer hints for AI helpers (for example, showing a short description when you hover over `openai_chat`).

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
  * For AI helpers, treat unexpected responses or error messages as an opportunity to teach debugging and resilience.

* **Comment intention, not mechanics**

  * Use `#` comments to explain *why* something exists, especially in tutorial code.

* **Stay consistent**

  * Indent with two spaces.
  * Stick to one naming convention for endpoints and parameters.
  * Use a consistent pattern for AI endpoints (for example, always prefix with `/ai/` or `/chat/`).

---

## Implementation Notes (High‑Level)

Shrimpl’s implementation (in this repository) is roughly organized as follows:

* **Parser (`src/parser/`)**

  * Tokenizes and parses `.shr` source into an abstract syntax tree (AST).
  * The AST types live in `parser/ast.rs`.
  * Endpoint bodies are parsed as general expressions, which means AI helpers like `openai_chat` and `openai_chat_json` are treated just like any other function call.

* **AST / Core Model (`src/ast.rs`, `src/parser/ast.rs`)**

  * Defines core entities such as `Program`, `Endpoint`, `FunctionDef`, `ClassDef`, and `Expr`.

* **Interpreter (`src/interpreter/`)**

  * Evaluates Shrimpl expressions.
  * Implements HTTP server wiring and endpoint dispatch.
  * Integrates built‑in libraries (HTTP client, vectors, tensors, dataframes, linear regression).
  * Contains the AI helper logic: a small configuration layer that reads the OpenAI API key from the environment or from `openai_set_api_key`, and helper functions that send chat/JSON requests to the OpenAI API.

* **Docs and Diagnostics (`src/docs.rs`)**

  * Builds a JSON schema for the API Studio (`/__shrimpl/schema`).
  * Computes static diagnostics (unused params, duplicate endpoints, etc.).

* **CLI / Entry Point (`src/main.rs`)**

  * Parses command‑line flags such as `--file`, `run`, `check`, `diagnostics`.
  * Prints a small startup banner when running a server, including the URL of the API Studio.
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

This architecture keeps the **language core** (parser + interpreter + docs) separate from **presentation layers** (CLI, LSP, API Studio, editor plugins). The AI helpers live in the interpreter layer and are exposed to programs as simple built‑in functions, so teachers and students can use them without needing to know anything about HTTP clients, JSON payloads, or authentication headers.

---

## Conclusion

Shrimpl 0.5 combines the simplicity of visual programming with the power of modern languages. It offers a gentle introduction to:

* Server‑side programming and HTTP APIs
* Data manipulation using dataframes and vectors
* Basic machine learning with linear regression
* AI‑assisted programming using OpenAI helpers for chat‑style responses and JSON payloads

The interpreter, diagnostics engine, API Studio, and LSP work together to create a friendly learning environment. Learners can:

1. Write simple `.shr` files.
2. See immediate results in the browser.
3. Get real‑time feedback from their editor.
4. Grow into more advanced topics (dataframes, ML, external APIs, AI helpers) without leaving Shrimpl.

For further examples and community discussion, visit the project repository, share your own Shrimpl programs, and help shape future versions of the language.
