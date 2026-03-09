mod config;
mod settings;

use clap::{Parser, Subcommand};
use colored::*;
use std::process;
use std::process::Command;

#[derive(Parser)]
#[command(name = "ccs", about = "Claude Code API Switch - quickly switch API configurations")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// List all available API configurations
    List,
    /// Show current active configuration
    Current,
}

struct ParsedArgs {
    mode: Mode,
    api_name: String,
    run: Option<bool>, // None = not specified (use default), Some(true/false) = explicit
}

/// Parse switch-style arguments from raw args (before clap), since clap
/// doesn't handle `ccs -s my-api` well with subcommands.
fn parse_raw_args() -> Option<ParsedArgs> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.is_empty() {
        return None;
    }

    // Check if first arg is a known subcommand
    if args[0] == "list" || args[0] == "current" || args[0] == "--help" || args[0] == "-h" {
        return None;
    }

    let mut mode = Mode::Session; // Default to Session mode
    let mut api_name: Option<String> = None;
    let mut run: Option<bool> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-s" | "--session" => mode = Mode::Session,
            "-l" | "--local" => mode = Mode::Local,
            "-g" | "--global" => mode = Mode::Global,
            "-r" | "--run" => run = Some(true),
            "--no-run" => run = Some(false),
            "-h" | "--help" => return None,
            arg if !arg.starts_with('-') => api_name = Some(arg.to_string()),
            _ => {
                eprintln!("{} Unknown flag: {}", "error:".red().bold(), args[i]);
                process::exit(1);
            }
        }
        i += 1;
    }

    api_name.map(|name| ParsedArgs {
        mode,
        api_name: name,
        run,
    })
}

#[derive(Clone, PartialEq)]
enum Mode {
    Global,
    Local,
    Session,
}

fn main() {
    // First try to parse as switch command (ccs [-s|-l|-g] [-r|--no-run] <api-name>)
    if let Some(parsed) = parse_raw_args() {
        // Determine whether to launch claude after switching:
        // - Session mode: default to run (unless --no-run)
        // - Global/Local mode: default to not run (unless -r/--run)
        let should_run = match parsed.run {
            Some(explicit) => explicit,
            None => parsed.mode == Mode::Session,
        };

        let result = match parsed.mode {
            Mode::Session => cmd_session(&parsed.api_name, should_run),
            Mode::Local => cmd_local(&parsed.api_name, should_run),
            Mode::Global => cmd_global(&parsed.api_name, should_run),
        };
        if let Err(e) = result {
            eprintln!("{} {}", "error:".red().bold(), e);
            process::exit(1);
        }
        return;
    }

    // Fall through to clap for subcommands
    let cli = Cli::parse();

    let result = match &cli.command {
        Some(Commands::List) => cmd_list(),
        Some(Commands::Current) => cmd_current(),
        None => {
            eprintln!("{}", "Usage: ccs <API_NAME> or ccs <COMMAND>".yellow());
            eprintln!("Run 'ccs --help' for more information.");
            process::exit(1);
        }
    };

    if let Err(e) = result {
        eprintln!("{} {}", "error:".red().bold(), e);
        process::exit(1);
    }
}

/// List all available API configurations.
fn cmd_list() -> Result<(), String> {
    let configs = config::load_config()?;

    if configs.is_empty() {
        println!("{}", "No API configurations found.".yellow());
        return Ok(());
    }

    println!("{}", "Available API configurations:".bold());
    println!();

    let mut names: Vec<&String> = configs.keys().collect();
    names.sort();

    for name in names {
        let api = &configs[name];
        let url = api.base_url.as_deref().unwrap_or("-");
        let model = api.model.as_deref().unwrap_or("-");
        println!(
            "  {} {} {}",
            name.green().bold(),
            format!("({})", url).dimmed(),
            format!("[{}]", model).dimmed()
        );
    }
    println!();

    Ok(())
}

/// Show the current active configuration from global settings.
fn cmd_current() -> Result<(), String> {
    let path = settings::global_settings_path()?;
    let env = settings::read_current_env(&path)?;

    if env.is_empty() {
        println!("{}", "No CCS-managed environment variables set.".yellow());
        return Ok(());
    }

    println!("{}", "Current global configuration:".bold());
    println!();
    for (key, val) in &env {
        let display_val = if key.contains("TOKEN") {
            // Mask token for security
            let s = val.as_str().unwrap_or("");
            if s.len() > 8 {
                format!("{}...{}", &s[..4], &s[s.len() - 4..])
            } else {
                "****".to_string()
            }
        } else {
            val.as_str().unwrap_or("").to_string()
        };
        println!("  {} = {}", key.cyan(), display_val);
    }
    println!();

    // Try to match against known configs
    if let Ok(configs) = config::load_config() {
        for (name, api) in &configs {
            let matches = api.env_pairs().iter().all(|(env_key, cfg_val)| {
                match (cfg_val, env.get(*env_key)) {
                    (Some(cv), Some(ev)) => ev.as_str() == Some(cv),
                    (None, None) => true,
                    _ => false,
                }
            });
            if matches {
                println!("{} {}", "Active profile:".bold(), name.green().bold());
                return Ok(());
            }
        }
        println!(
            "{}",
            "Active profile: (no exact match found)".dimmed()
        );
    }

    Ok(())
}

/// Apply config globally (modify ~/.claude/settings.json).
fn cmd_global(name: &str, should_run: bool) -> Result<(), String> {
    let (_configs, api) = config::get_api_config(name)?;
    let path = settings::global_settings_path()?;

    settings::apply_config(&path, &api)?;

    println!(
        "{} Applied '{}' to {}",
        "✓".green().bold(),
        name.green().bold(),
        path.display().to_string().dimmed()
    );

    if should_run {
        launch_claude()?;
    }

    Ok(())
}

/// Apply config locally (modify .claude/settings.json).
fn cmd_local(name: &str, should_run: bool) -> Result<(), String> {
    let (_configs, api) = config::get_api_config(name)?;
    let path = settings::local_settings_path();

    settings::apply_config(&path, &api)?;

    println!(
        "{} Applied '{}' to {}",
        "✓".green().bold(),
        name.green().bold(),
        path.display().to_string().dimmed()
    );

    if should_run {
        launch_claude()?;
    }

    Ok(())
}

/// Session mode: set env vars in current process and launch claude as a child process.
fn cmd_session(name: &str, should_run: bool) -> Result<(), String> {
    let (_configs, api) = config::get_api_config(name)?;

    // Set environment variables in the current process
    for (env_key, value) in api.env_pairs() {
        match value {
            Some(v) => std::env::set_var(env_key, v),
            None => std::env::remove_var(env_key),
        }
    }

    println!(
        "{} Switched to '{}' {}",
        "✓".green().bold(),
        name.green().bold(),
        "(session)".dimmed()
    );

    if should_run {
        launch_claude()?;
    } else {
        // If not running claude, output export statements for eval usage
        let mut exports = Vec::new();
        for (env_key, value) in api.env_pairs() {
            match value {
                Some(v) => {
                    exports.push(format!("export {}=\"{}\";", env_key, v));
                }
                None => {
                    exports.push(format!("unset {};", env_key));
                }
            }
        }
        println!("{}", exports.join(" "));
    }

    Ok(())
}

/// Launch claude as a child process, inheriting the current environment.
/// Waits for claude to exit before returning.
fn launch_claude() -> Result<(), String> {
    println!("{}", "Launching claude...".dimmed());

    let status = Command::new("claude")
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "claude command not found. Make sure Claude Code is installed and in your PATH."
                    .to_string()
            } else {
                format!("Failed to launch claude: {}", e)
            }
        })?;

    if !status.success() {
        if let Some(code) = status.code() {
            process::exit(code);
        }
    }

    Ok(())
}
