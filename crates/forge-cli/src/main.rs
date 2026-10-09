use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "numera-forge",
    version,
    about = "Source-aware extension distribution; explicit trust, honest compatibility"
)]
struct Cli {
    #[arg(long, global = true, default_value = ".")]
    repository: PathBuf,
    #[arg(long, global = true)]
    json: bool,
    #[arg(long, global = true)]
    non_interactive: bool,
    #[arg(long, global = true)]
    offline: bool,
    #[arg(long, global = true, default_value = "default")]
    profile: String,
    #[arg(long, global = true, default_value_t = 120)]
    timeout: u64,
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    Init,
    Doctor,
    Logs,
    Status,
    Source {
        #[command(subcommand)]
        command: Sources,
    },
    Discover {
        source: String,
    },
    Approve {
        source: String,
        #[arg(long)]
        digest: String,
        #[arg(long)]
        allow_executable: bool,
    },
    Catalog {
        #[command(subcommand)]
        command: Catalog,
    },
    Package {
        #[command(subcommand)]
        command: Packages,
    },
    Harness {
        #[command(subcommand)]
        command: Harnesses,
    },
    Compatibility {
        package: String,
        #[arg(long)]
        target: String,
    },
    Sync {
        #[arg(long)]
        check: bool,
        #[arg(long)]
        apply: bool,
        #[arg(long, default_value_t = 1)]
        concurrency: u8,
    },
    Build {
        package: String,
        #[arg(long)]
        target: String,
        #[arg(long)]
        locked: bool,
    },
    Validate {
        #[arg(long)]
        all: bool,
    },
    Install {
        package: String,
        #[arg(long)]
        target: String,
        #[arg(long, default_value = "project")]
        scope: String,
        #[arg(long)]
        destination: PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
    Installed,
    Update {
        #[arg(long)]
        installed: bool,
    },
    Disable {
        receipt: String,
    },
    Enable {
        receipt: String,
    },
    Uninstall {
        receipt: String,
    },
    Rollback {
        operation: String,
    },
    Publish {
        #[arg(long, default_value = "preview")]
        mode: String,
    },
    Request {
        file: PathBuf,
    },
}
#[derive(Subcommand)]
enum Sources {
    Add {
        repository: String,
        #[arg(long)]
        id: String,
        #[arg(long)]
        root: Vec<String>,
        #[arg(long)]
        target: Vec<String>,
    },
    List,
    Inspect {
        source: String,
    },
    UpdatePolicy {
        source: String,
        #[arg(long)]
        policy: String,
        #[arg(long)]
        enabled: bool,
    },
}
#[derive(Subcommand)]
enum Catalog {
    List,
}
#[derive(Subcommand)]
enum Packages {
    Inspect { package: String },
    Add { file: PathBuf },
}
#[derive(Subcommand)]
enum Harnesses {
    List,
}
fn request(cli: &Cli) -> forge_core::Result<Value> {
    Ok(match &cli.command {
        Commands::Init => json!({"action":"init"}),
        Commands::Doctor => json!({"action":"doctor"}),
        Commands::Logs => json!({"action":"logs"}),
        Commands::Status => json!({"action":"status"}),
        Commands::Source { command } => match command {
            Sources::Add {
                repository,
                id,
                root,
                target,
            } => {
                json!({"action":"source-add","repository":repository,"id":id,"roots":root,"targets":target})
            }
            Sources::List => json!({"action":"source-list"}),
            Sources::Inspect { source } => json!({"action":"source-inspect","source":source}),
            Sources::UpdatePolicy {
                source,
                policy,
                enabled,
            } => {
                json!({"action":"source-policy","source":source,"update":policy,"enabled":enabled})
            }
        },
        Commands::Discover { source } => {
            json!({"action":"discover","source":source,"offline":cli.offline})
        }
        Commands::Approve {
            source,
            digest,
            allow_executable,
        } => {
            json!({"action":"approve","source":source,"digest":digest,"allow_executable":allow_executable})
        }
        Commands::Catalog { .. } => json!({"action":"catalog"}),
        Commands::Package { command } => match command {
            Packages::Inspect { package } => json!({"action":"package-inspect","package":package}),
            Packages::Add { file } => {
                json!({"action":"package-add","package":forge_core::storage::read::<Value>(file)?})
            }
        },
        Commands::Harness { .. } => json!({"action":"harnesses"}),
        Commands::Compatibility { package, target } => {
            json!({"action":"compatibility","package":package,"target":target})
        }
        Commands::Sync {
            apply, concurrency, ..
        } => {
            if *concurrency != 1 {
                return Err(forge_core::Error::new(
                    "concurrency",
                    "Only sequential source checks are supported",
                ));
            }
            json!({"action":"sync","apply":apply,"offline":cli.offline})
        }
        Commands::Build {
            package, target, ..
        } => json!({"action":"build","package":package,"target":target}),
        Commands::Validate { .. } => json!({"action":"validate"}),
        Commands::Install {
            package,
            target,
            destination,
            dry_run,
            scope,
        } => {
            if scope != "project" {
                return Err(forge_core::Error::new(
                    "scope",
                    "Only explicit project destinations are certified in this version",
                ));
            }
            json!({"action":"install","package":package,"target":target,"destination":destination,"dry_run":dry_run})
        }
        Commands::Installed => json!({"action":"installed"}),
        Commands::Update { .. } => json!({"action":"update"}),
        Commands::Disable { receipt } => json!({"action":"disable","receipt":receipt}),
        Commands::Enable { receipt } => json!({"action":"enable","receipt":receipt}),
        Commands::Uninstall { receipt } => json!({"action":"uninstall","receipt":receipt}),
        Commands::Rollback { operation } => json!({"action":"rollback","operation":operation}),
        Commands::Publish { mode } => json!({"action":"publish","mode":mode}),
        Commands::Request { file } => forge_core::storage::read(file)?,
    })
}
fn main() {
    let cli = Cli::parse();
    let root = match std::fs::canonicalize(&cli.repository) {
        Ok(p) => p,
        Err(_) => {
            eprintln!("workspace: selected repository does not exist");
            std::process::exit(3)
        }
    };
    let log = forge_core::logging::RunLog::start("numera-forge", &root)
        .map_err(|e| eprintln!("WARNING logging: {}", e.code))
        .ok();
    let result = request(&cli).and_then(|r| forge_core::service::execute(&root, r));
    match result {
        Ok(value) => {
            let partial = value["status"] == "partial";
            if let Some(log) = &log {
                log.event(
                    if partial { "WARNING" } else { "INFO" },
                    "operation",
                    "Application request completed",
                );
            }
            if cli.json {
                println!("{}", value);
            } else {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&value)
                        .unwrap_or_else(|_| "serialization failed".into())
                );
            }
            drop(log);
            if partial {
                std::process::exit(2)
            }
        }
        Err(e) => {
            if let Some(log) = &log {
                log.event("ERROR", "operation", e.code);
            }
            if cli.json {
                println!(
                    "{}",
                    json!({"status":"error","code":e.code,"message":e.message})
                );
            } else {
                eprintln!("{}", e);
            }
            drop(log);
            std::process::exit(
                if matches!(
                    e.code,
                    "approval-required"
                        | "trust-required"
                        | "unsupported-required"
                        | "publication-review"
                        | "update-review"
                ) {
                    2
                } else {
                    3
                },
            )
        }
    }
}
