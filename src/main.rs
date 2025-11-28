// src/main.rs

use clap::{Parser, Subcommand};
use std::process::{Command, Stdio};
use std::{env, error::Error, fs};

mod ast;
mod docs;
mod interpreter;
mod parser;

use interpreter::http::run as run_server;
use parser::parse_program;

/// Shrimpl version (from Cargo.toml)
const SHRIMPL_VERSION: &str = env!("CARGO_PKG_VERSION");

/// ASCII banner shown when running `shrimpl` with no arguments.
const SHRIMPL_ASCII_BANNER: &str = r#"
:'######::'##::::'##:'########::'####:'##::::'##:'########::'##:::::::
'##... ##: ##:::: ##: ##.... ##:. ##:: ###::'###: ##.... ##: ##:::::::
 ##:::..:: ##:::: ##: ##:::: ##:: ##:: ####'####: ##:::: ##: ##:::::::
. ######:: #########: ########::: ##:: ## ### ##: ########:: ##:::::::
:..... ##: ##.... ##: ##.. ##:::: ##:: ##. #: ##: ##.....::: ##:::::::
'##::: ##: ##:::: ##: ##::. ##::: ##:: ##:.:: ##: ##:::::::: ##:::::::
. ######:: ##:::: ##: ##:::. ##:'####: ##:::: ##: ##:::::::: ########:
:......:::..:::::..::..:::::..::....::..:::::..::..:::::::::..........:

             ~ Shrimpl Language v{SHRIMPL_VERSION}~
"#;

/// Print the friendly welcome screen for `shrimpl` with no args.
fn print_welcome_screen() {
    println!("{SHRIMPL_ASCII_BANNER}");
    println!("Welcome to Shrimpl v{SHRIMPL_VERSION} :D");
    println!();
    println!("Shrimpl is an all-ages language for APIs, data, ML, and AI.");
    println!();
    println!("Common commands:");
    println!("  shrimpl --file app.shr run");
    println!("      Run the Shrimpl HTTP server defined in app.shr");
    println!();
    println!("  shrimpl --file app.shr check");
    println!("      Parse and check the Shrimpl program (no server start).");
    println!();
    println!("  shrimpl --file app.shr schema");
    println!("      Print JSON schema for Shrimpl API Studio.");
    println!();
    println!("  shrimpl --file app.shr diagnostics");
    println!("      Print static diagnostics JSON for endpoints/functions.");
    println!();
    println!("  shrimpl lsp");
    println!("      Start the Shrimpl language server (LSP) using `shrimpl-lsp`.");
    println!("      Use `shrimpl lsp --exe path/to/shrimpl-lsp` to point at a custom binary.");
    println!();
    println!("Tips:");
    println!("  • After `shrimpl --file app.shr run`, open:");
    println!("        http://localhost:3000/__shrimpl/ui");
    println!("    to use the Shrimpl API Studio visual UI.");
    println!("  • Set SHRIMPL_OPENAI_API_KEY before running if you use OpenAI helpers.");
    println!();
}

/// Shrimpl CLI
#[derive(Parser)]
#[command(name = "shrimpl")]
#[command(about = "Shrimpl language CLI", long_about = None)]
struct Cli {
    /// Path to the main Shrimpl file (default: app.shr)
    #[arg(global = true, short, long, default_value = "app.shr")]
    file: String,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the Shrimpl server
    Run,

    /// Check syntax and print diagnostics
    Check,

    /// Print schema JSON
    Schema,

    /// Print diagnostics JSON
    Diagnostics,

    /// Start the Shrimpl language server (LSP) binary
    ///
    /// By default this runs the `shrimpl-lsp` executable found in PATH.
    /// Use `--exe` to point at a specific path, for example:
    /// `shrimpl lsp --exe target/debug/shrimpl-lsp`
    Lsp {
        /// Path to the shrimpl-lsp executable (or name in PATH)
        #[arg(short, long, default_value = "shrimpl-lsp")]
        exe: String,
    },
}

/// Entry point that decides between the welcome screen and the real CLI.
fn main() -> Result<(), Box<dyn Error>> {
    let arg_count = env::args().count();

    // If the user just types `shrimpl` with no additional arguments,
    // show the ASCII banner + friendly help instead of immediately
    // starting the server.
    if arg_count == 1 {
        print_welcome_screen();
        return Ok(());
    }

    // Otherwise, run the normal CLI behavior.
    run_cli()
}

/// Actual CLI implementation (what used to be `main`).
fn run_cli() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    // Default behavior with no subcommand is the same as `run`.
    let command = cli.command.unwrap_or(Commands::Run);

    match command {
        Commands::Lsp { exe } => {
            // LSP mode: spawn the shrimpl-lsp binary and wire stdin/stdout/stderr through.
            start_lsp_subprocess(&exe)?;
        }

        Commands::Run => {
            let (source, program) = load_and_parse(&cli.file)?;
            // Avoid unused variable warning (source is also used by docs UI endpoint).
            let _ = source;

            // Read the configured port from the Shrimpl program so we can tell
            // the user exactly where the server will be listening.
            let port = program.server.port;

            // Friendly startup banner for `shrimpl --file app.shr run`.
            println!();
            println!("shrimpl run");
            println!("----------------------------------------");
            println!("Shrimpl server is starting on http://localhost:{port}");
            println!("Open one of these in your browser:");
            println!("  • http://localhost:{port}/           (root endpoint)");
            println!("  • http://localhost:{port}/__shrimpl/ui  (API Studio UI)");
            println!();
            println!("Press Ctrl+C to shut down the server.");
            println!("----------------------------------------");
            println!();

            // Run the HTTP server using actix.
            actix_web::rt::System::new().block_on(run_server(program))?;
        }

        Commands::Check => {
            let (_source, _program) = load_and_parse(&cli.file)?;
            // If parse succeeded, we are OK.
            println!("OK: {}", &cli.file);
        }

        Commands::Schema => {
            let (_source, program) = load_and_parse(&cli.file)?;
            let schema = docs::build_schema(&program);
            println!("{}", serde_json::to_string_pretty(&schema)?);
        }

        Commands::Diagnostics => {
            let (_source, program) = load_and_parse(&cli.file)?;
            let diags = docs::build_diagnostics(&program);
            println!("{}", serde_json::to_string_pretty(&diags)?);
        }
    }

    Ok(())
}

/// Read the Shrimpl source file and parse it into a Program.
fn load_and_parse(path: &str) -> Result<(String, ast::Program), Box<dyn Error>> {
    let source = fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {}", path, e))?;

    let program = parse_program(&source).map_err(|e| format!("Parse error in {}: {}", path, e))?;

    Ok((source, program))
}

/// Start the external shrimpl-lsp process and wait for it to exit.
fn start_lsp_subprocess(exe: &str) -> Result<(), Box<dyn Error>> {
    let mut child = Command::new(exe)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("Failed to start LSP server '{}': {}", exe, e))?;

    let status = child.wait()?;
    if !status.success() {
        return Err(format!("LSP server '{}' exited with status {}", exe, status).into());
    }

    Ok(())
}
