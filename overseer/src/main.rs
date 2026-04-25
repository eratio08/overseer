use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand, Stdio};

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};

mod commands;
mod core;
mod db;
mod error;
mod id;
mod output;
mod types;
use commands::{
    data, learning, task, DataCommand, DataResult, LearningCommand, LearningResult, TaskCommand,
    TaskResult,
};
use output::Printer;

#[derive(Parser)]
#[command(name = "os")]
#[command(version)]
#[command(
    about = "Overseer - Agent task management CLI",
    long_about = r#"
Overseer (os) - Task orchestration for AI coding agents.

Features:
  • 3-level task hierarchy: milestone → task → subtask
  • Dependency management with cycle detection
  • Learning capture and inheritance

Environment:
  OVERSEER_DB_PATH  Override database location
  NO_COLOR          Disable colored output
"#
)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Output in JSON format (for programmatic use)
    #[arg(long, global = true)]
    json: bool,

    /// Disable colored output
    #[arg(long, global = true)]
    no_color: bool,

    /// Override database path (default: CWD/.overseer/tasks.db)
    #[arg(long, global = true)]
    db: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Command {
    /// Task management (CRUD, workflow, queries)
    #[command(subcommand)]
    Task(TaskCommand),

    /// Learning management
    #[command(subcommand)]
    Learning(LearningCommand),

    /// Data import/export
    #[command(subcommand)]
    Data(DataCommand),

    /// Generate shell completions
    #[command(
        about = "Generate shell completions",
        long_about = r#"
Generate shell completions for os CLI.

Examples:
  os completions bash > ~/.local/share/bash-completion/completions/os
  os completions zsh > ~/.zfunc/_os
  os completions fish > ~/.config/fish/completions/os.fish
  os completions powershell > os.ps1
"#
    )]
    Completions {
        /// Shell to generate completions for (bash, zsh, fish, powershell, elvish)
        shell: Shell,
    },

    /// Initialize database in current directory
    #[command(
        about = "Initialize database",
        long_about = r#"
Initialize the Overseer database.

The database is created at:
  1. OVERSEER_DB_PATH (if set)
  2. CWD/.overseer/tasks.db

Usually runs automatically on first command.
"#
    )]
    Init,

    /// Start the Task Viewer UI server
    #[command(
        about = "Start Task Viewer UI",
        long_about = r#"
Start the Overseer Task Viewer web UI.

Opens a local HTTP server for viewing and managing tasks in your browser.
The server runs until interrupted (Ctrl+C).

Requires Node.js and the @overseer/host package.
"#
    )]
    Ui {
        /// HTTP port (default: 6969)
        #[arg(long, short, default_value = "6969")]
        port: u16,
    },

    /// Start the MCP server (for AI agents)
    #[command(
        about = "Start MCP server",
        long_about = r#"
Start the Overseer MCP (Model Context Protocol) server.

The MCP server communicates over stdio and provides a codemode API
for AI agents to manage tasks programmatically.

Requires Node.js and the @overseer/host package.
"#
    )]
    Mcp,
}

/// Run the Node host server for UI or MCP mode.
///
/// Resolves paths relative to the binary location and spawns Node.
fn run_host_server(mode: &str, port: u16) {
    // Get path to current executable
    let exe_path = std::env::current_exe().unwrap_or_else(|e| {
        eprintln!("Error: cannot determine executable path: {}", e);
        std::process::exit(1);
    });

    // CLI path is the current executable
    let cli_path = exe_path.to_string_lossy().to_string();

    // Working directory for CLI commands
    let cwd = std::env::current_dir().unwrap_or_else(|e| {
        eprintln!("Error: cannot determine current directory: {}", e);
        std::process::exit(1);
    });

    // Find the host package relative to the binary
    // In dev: binary is at target/release/os, host is at ../../../host/dist/index.js
    // In npm: binary is at bin/os-{platform}, host is at ../host/dist/index.js
    let host_script = find_host_script(&exe_path).unwrap_or_else(|| {
        eprintln!("Error: cannot find @overseer/host package");
        eprintln!("Make sure Node.js is installed and run: npm install -g overseer");
        std::process::exit(1);
    });

    // For UI mode, we also need to find the static files
    let static_root = if mode == "ui" {
        find_static_root(&exe_path).unwrap_or_else(|| {
            eprintln!("Error: cannot find UI static files");
            eprintln!("Make sure the UI has been built: cd ui && npm run build");
            std::process::exit(1);
        })
    } else {
        String::new()
    };

    // Build arguments for Node
    let mut args = vec![
        host_script.clone(),
        mode.to_string(),
        "--cli-path".to_string(),
        cli_path,
        "--cwd".to_string(),
        cwd.to_string_lossy().to_string(),
    ];

    if mode == "ui" {
        args.push("--static-root".to_string());
        args.push(static_root);
        args.push("--port".to_string());
        args.push(port.to_string());
    }

    // Spawn Node and wait for it
    let status = StdCommand::new("node")
        .args(&args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();

    match status {
        Ok(s) => {
            if !s.success() {
                std::process::exit(s.code().unwrap_or(1));
            }
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                eprintln!("Error: Node.js not found. Please install Node.js.");
            } else {
                eprintln!("Error: failed to run Node.js: {}", e);
            }
            std::process::exit(1);
        }
    }
}

/// Find the host script relative to the binary.
/// Tries multiple locations for dev vs production installs.
fn find_host_script(exe_path: &PathBuf) -> Option<String> {
    let exe_dir = exe_path.parent()?;

    // Candidate paths (relative to binary location)
    let candidates = [
        // Dev: binary at overseer/target/release/os
        exe_dir.join("../../../host/dist/index.js"),
        // npm: binary in @dmmulroy/overseer-{platform}/, host in @dmmulroy/overseer/host/
        // node_modules/@dmmulroy/overseer-darwin-arm64/os -> node_modules/@dmmulroy/overseer/host/dist/index.js
        exe_dir.join("../@dmmulroy/overseer/host/dist/index.js"),
        exe_dir.join("../overseer/host/dist/index.js"),
        // npm global with different layout
        exe_dir.join("../../@dmmulroy/overseer/host/dist/index.js"),
        // Local development with npm link
        exe_dir.join("../../../../host/dist/index.js"),
    ];

    for candidate in &candidates {
        if let Ok(path) = candidate.canonicalize() {
            if path.exists() {
                return Some(path.to_string_lossy().to_string());
            }
        }
    }

    None
}

/// Find the static root for UI files.
fn find_static_root(exe_path: &PathBuf) -> Option<String> {
    let exe_dir = exe_path.parent()?;

    // Candidate paths (relative to binary location)
    let candidates = [
        // Dev: binary at overseer/target/release/os
        exe_dir.join("../../../ui/dist"),
        // npm: binary in @dmmulroy/overseer-{platform}/, ui in @dmmulroy/overseer/ui/
        exe_dir.join("../@dmmulroy/overseer/ui/dist"),
        exe_dir.join("../overseer/ui/dist"),
        // npm global with different layout
        exe_dir.join("../../@dmmulroy/overseer/ui/dist"),
        // Local development
        exe_dir.join("../../../../ui/dist"),
    ];

    for candidate in &candidates {
        if let Ok(path) = candidate.canonicalize() {
            if path.exists() && path.join("index.html").exists() {
                return Some(path.to_string_lossy().to_string());
            }
        }
    }

    None
}

/// Determine the default database path from the nearest existing Overseer database.
///
/// Resolution order:
fn find_existing_db_path(start: &Path) -> Option<PathBuf> {
    for dir in start.ancestors() {
        let candidate = dir.join(".overseer").join("tasks.db");
        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

/// 1. OVERSEER_DB_PATH env var (if set)
/// 2. Nearest existing ancestor .overseer/tasks.db
/// 3. Current working directory -> .overseer/tasks.db
fn default_db_path() -> PathBuf {
    // Check env override first
    if let Ok(path) = std::env::var("OVERSEER_DB_PATH") {
        return PathBuf::from(path);
    }

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    find_existing_db_path(&cwd).unwrap_or_else(|| cwd.join(".overseer").join("tasks.db"))
}

fn main() {
    let cli = Cli::parse();

    // PRECONDITION: Completions bypass normal output flow - raw shell script to stdout
    if let Command::Completions { shell } = &cli.command {
        generate(*shell, &mut Cli::command(), "os", &mut io::stdout());
        return;
    }

    // PRECONDITION: UI and MCP spawn Node processes, bypass normal run()
    if let Command::Ui { port } = &cli.command {
        run_host_server("ui", *port);
        return;
    }
    if let Command::Mcp = &cli.command {
        run_host_server("mcp", 0);
        return;
    }

    let db_path = cli.db.unwrap_or_else(default_db_path);

    let result = run(&cli.command, &db_path);

    match result {
        Ok(output) => {
            if cli.json {
                println!("{}", output);
            } else {
                let printer = Printer::new(cli.no_color);
                printer.print(&cli.command, &output);
            }
        }
        Err(e) => {
            if cli.json {
                let err = serde_json::json!({ "error": e.to_string() });
                eprintln!("{}", err);
            } else {
                let printer = Printer::new_for_stderr(cli.no_color);
                printer.print_error(&format!("Error: {}", e));
            }
            std::process::exit(1);
        }
    }
}

fn run(command: &Command, db_path: &PathBuf) -> error::Result<String> {
    match command {
        Command::Init => {
            db::open_db(db_path)?;
            Ok(serde_json::json!({ "initialized": true, "path": db_path }).to_string())
        }
        Command::Task(cmd) => {
            let conn = db::open_db(db_path)?;
            let cloned_cmd = clone_task_cmd(cmd);
            let result = task::handle(&conn, cloned_cmd)?;

            match result {
                TaskResult::One(t) => Ok(serde_json::to_string_pretty(&t)?),
                TaskResult::OneWithContext(t) => Ok(serde_json::to_string_pretty(&t)?),
                TaskResult::MaybeOneWithContext(opt) => Ok(serde_json::to_string_pretty(&opt)?),
                TaskResult::Many(ts) => Ok(serde_json::to_string_pretty(&ts)?),
                TaskResult::Deleted => Ok(serde_json::json!({ "deleted": true }).to_string()),
                TaskResult::Tree(tree) => Ok(serde_json::to_string_pretty(&tree)?),
                TaskResult::Trees(trees) => Ok(serde_json::to_string_pretty(&trees)?),
                TaskResult::Progress(progress) => Ok(serde_json::to_string_pretty(&progress)?),
            }
        }
        Command::Learning(cmd) => {
            let conn = db::open_db(db_path)?;
            match learning::handle(&conn, clone_learning_cmd(cmd))? {
                LearningResult::One(l) => Ok(serde_json::to_string_pretty(&l)?),
                LearningResult::Many(ls) => Ok(serde_json::to_string_pretty(&ls)?),
                LearningResult::Deleted => Ok(serde_json::json!({ "deleted": true }).to_string()),
            }
        }
        Command::Data(cmd) => {
            let conn = db::open_db(db_path)?;
            match data::handle(&conn, clone_data_cmd(cmd))? {
                DataResult::Exported {
                    path,
                    tasks,
                    learnings,
                } => Ok(serde_json::to_string_pretty(&serde_json::json!({
                    "exported": true,
                    "path": path,
                    "tasks": tasks,
                    "learnings": learnings
                }))?),
            }
        }
        // PRECONDITION: Completions handled in main() before run() is called
        Command::Completions { .. } => unreachable!("completions handled before run()"),
        // PRECONDITION: UI and MCP handled in main() before run() is called
        Command::Ui { .. } => unreachable!("ui handled before run()"),
        Command::Mcp => unreachable!("mcp handled before run()"),
    }
}

fn clone_task_cmd(cmd: &TaskCommand) -> TaskCommand {
    match cmd {
        TaskCommand::Create(args) => TaskCommand::Create(task::CreateArgs {
            description: args.description.clone(),
            context: args.context.clone(),
            parent: args.parent.clone(),
            priority: args.priority,
            blocked_by: args.blocked_by.clone(),
        }),
        TaskCommand::Get { id } => TaskCommand::Get { id: id.clone() },
        TaskCommand::List(args) => TaskCommand::List(task::ListArgs {
            parent: args.parent.clone(),
            ready: args.ready,
            completed: args.completed,
            milestones: args.milestones,
            tasks: args.tasks,
            subtasks: args.subtasks,
            archived: args.archived,
            all: args.all,
            flat: args.flat,
        }),
        TaskCommand::Update(args) => TaskCommand::Update(task::UpdateArgs {
            id: args.id.clone(),
            description: args.description.clone(),
            context: args.context.clone(),
            priority: args.priority,
            parent: args.parent.clone(),
        }),
        TaskCommand::Start { id } => TaskCommand::Start { id: id.clone() },
        TaskCommand::Complete(args) => TaskCommand::Complete(task::CompleteArgs {
            id: args.id.clone(),
            result: args.result.clone(),
            learnings: args.learnings.clone(),
        }),
        TaskCommand::Reopen { id } => TaskCommand::Reopen { id: id.clone() },
        TaskCommand::Cancel { id } => TaskCommand::Cancel { id: id.clone() },
        TaskCommand::Archive { id } => TaskCommand::Archive { id: id.clone() },
        TaskCommand::Delete { id } => TaskCommand::Delete { id: id.clone() },
        TaskCommand::Block(args) => TaskCommand::Block(task::BlockArgs {
            id: args.id.clone(),
            by: args.by.clone(),
        }),
        TaskCommand::Unblock(args) => TaskCommand::Unblock(task::UnblockArgs {
            id: args.id.clone(),
            by: args.by.clone(),
        }),
        TaskCommand::NextReady(args) => TaskCommand::NextReady(task::NextReadyArgs {
            milestone: args.milestone.clone(),
        }),
        TaskCommand::Tree(args) => TaskCommand::Tree(task::TreeArgs {
            id: args.id.clone(),
        }),
        TaskCommand::Search(args) => TaskCommand::Search(task::SearchArgs {
            query: args.query.clone(),
        }),
        TaskCommand::Progress(args) => TaskCommand::Progress(task::ProgressArgs {
            id: args.id.clone(),
        }),
    }
}

fn clone_learning_cmd(cmd: &LearningCommand) -> LearningCommand {
    match cmd {
        LearningCommand::Add(args) => LearningCommand::Add(learning::AddArgs {
            task_id: args.task_id.clone(),
            content: args.content.clone(),
            source: args.source.clone(),
        }),
        LearningCommand::List { task_id } => LearningCommand::List {
            task_id: task_id.clone(),
        },
        LearningCommand::Delete { id } => LearningCommand::Delete { id: id.clone() },
    }
}

fn clone_data_cmd(cmd: &DataCommand) -> DataCommand {
    match cmd {
        DataCommand::Export { output } => DataCommand::Export {
            output: output.clone(),
        },
    }
}
