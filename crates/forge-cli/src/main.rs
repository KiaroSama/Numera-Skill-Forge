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
    History,
    Support {
        #[arg(long)]
        include_logs: bool,
    },
    LogRetention {
        #[arg(long, default_value_t = 30)]
        days: u64,
        #[arg(long, default_value_t = 10)]
        keep: usize,
        #[arg(long)]
        apply: bool,
    },
    Job {
        #[command(subcommand)]
        command: Jobs,
    },
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
    Collection {
        #[command(subcommand)]
        command: Collections,
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
        #[arg(long)]
        expected_remote: Option<String>,
        #[arg(long)]
        expected_head: Option<String>,
        #[arg(long)]
        expected_remote_head: Option<String>,
        #[arg(long)]
        expected_account: Option<String>,
        #[arg(long)]
        publication_branch: Option<String>,
        #[arg(long)]
        author_name: Option<String>,
        #[arg(long)]
        author_email: Option<String>,
    },
    Request {
        file: PathBuf,
    },
}
#[derive(Subcommand)]
enum Jobs {
    List,
    Status {
        job: String,
    },
    Cancel {
        job: String,
    },
    Retry {
        job: String,
    },
    Run {
        file: PathBuf,
        #[arg(long)]
        id: Option<String>,
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
enum Collections {
    Sync {
        #[arg(long)]
        apply: bool,
    },
    Verify,
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
        Commands::History => json!({"action":"history"}),
        Commands::Support { include_logs } => {
            json!({"action":"support-bundle","include_logs":include_logs})
        }
        Commands::LogRetention { days, keep, apply } => {
            json!({"action":"log-retention","days":days,"keep":keep,"apply":apply})
        }
        Commands::Job { command } => match command {
            Jobs::List => json!({"action":"job-list"}),
            Jobs::Status { job } => json!({"action":"job-status","job":job}),
            Jobs::Cancel { job } => json!({"action":"job-cancel","job":job}),
            Jobs::Retry { job } => json!({"action":"job-retry","job":job}),
            Jobs::Run { file, id } => {
                json!({"action":"job-run","request":forge_core::storage::read::<Value>(file)?,"job":id,"timeout":cli.timeout})
            }
        },
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
        Commands::Collection { command } => match command {
            Collections::Sync { apply } => {
                json!({"action":"collection-sync","apply":apply,"offline":cli.offline})
            }
            Collections::Verify => json!({"action":"collection-verify"}),
        },
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
            json!({"action":"install","package":package,"target":target,"scope":scope,"destination":destination,"dry_run":dry_run})
        }
        Commands::Installed => json!({"action":"installed"}),
        Commands::Update { .. } => json!({"action":"update"}),
        Commands::Disable { receipt } => json!({"action":"disable","receipt":receipt}),
        Commands::Enable { receipt } => json!({"action":"enable","receipt":receipt}),
        Commands::Uninstall { receipt } => json!({"action":"uninstall","receipt":receipt}),
        Commands::Rollback { operation } => json!({"action":"rollback","operation":operation}),
        Commands::Publish {
            mode,
            expected_remote,
            expected_head,
            expected_remote_head,
            expected_account,
            publication_branch,
            author_name,
            author_email,
        } => {
            json!({"action":"publish","mode":mode,"expected_remote":expected_remote,"expected_head":expected_head,"expected_remote_head":expected_remote_head,"expected_account":expected_account,"publication_branch":publication_branch,"author_name":author_name,"author_email":author_email})
        }
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
    let selected = (|| -> forge_core::Result<PathBuf> {
        forge_core::validation::identifier(&cli.profile)?;
        if cli.profile == "default" {
            return Ok(root.clone());
        }
        for ancestor in root.ancestors() {
            forge_core::validation::reject_link(ancestor)?;
        }
        let state = forge_core::validation::contained(&root, ".forge-state")?;
        std::fs::create_dir_all(&state)?;
        let directory = forge_core::validation::contained(&state, "profiles")?;
        std::fs::create_dir_all(&directory)?;
        let path = forge_core::validation::contained(&directory, &cli.profile)?;
        std::fs::create_dir_all(&path)?;
        Ok(path)
    })();
    let log_root = selected.as_ref().unwrap_or(&root);
    let log = forge_core::logging::RunLog::start("numera-forge", log_root)
        .map_err(|e| eprintln!("WARNING logging: {}", e.code))
        .ok();
    let result = request(&cli)
        .and_then(|r| {
            let selected = selected?;
            if matches!(
                r["action"].as_str(),
                Some("sync" | "discover" | "build" | "collection-sync" | "update")
            ) {
                forge_core::jobs::run(&selected, r, cli.timeout, None)
            } else {
                forge_core::service::execute(&selected, r)
            }
        })
        .and_then(|value| {
            if matches!(&cli.command, Commands::Job { .. }) {
                return Ok(value);
            }
            if matches!(value["status"].as_str(), Some("failed" | "cancelled")) {
                Err(forge_core::Error::new(
                    match value["error"]["code"].as_str() {
                        Some("approval-required") => "approval-required",
                        Some("trust-required") => "trust-required",
                        Some("unsupported-required") => "unsupported-required",
                        Some("publication-review") => "publication-review",
                        Some("cancelled") => "cancelled",
                        Some("timeout") => "timeout",
                        _ => "job-failed",
                    },
                    forge_core::logging::redact(&value["error"].to_string()),
                ))
            } else if value.get("result").is_some() {
                Ok(value["result"].clone())
            } else {
                Ok(value)
            }
        });
    match result {
        Ok(value) => {
            let failed = matches!(
                &cli.command,
                Commands::Job {
                    command: Jobs::Run { .. } | Jobs::Retry { .. }
                }
            ) && matches!(
                value["status"].as_str(),
                Some("failed" | "cancelled" | "interrupted")
            );
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
            if failed {
                std::process::exit(3)
            }
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
