mod contract;
#[cfg(test)]
mod control_tests;
mod evidence;
#[cfg(test)]
mod evidence_tests;
mod owner;
#[cfg(test)]
mod owner_tests;
mod projection;
mod sandbox;
mod worker;

fn argument(args: &[String], name: &str) -> contract::Result<String> {
    let index = args
        .iter()
        .position(|s| s == name)
        .ok_or("Required argument missing")?;
    Ok(args.get(index + 1).ok_or("Argument value missing")?.clone())
}
fn options(args: &[String], allowed: &[&str]) -> contract::Result<()> {
    if args.len() != allowed.len() * 2 + 1 {
        return Err("Unexpected argument count".into());
    }
    let mut found = std::collections::BTreeSet::new();
    for pair in args[1..].as_chunks::<2>().0 {
        if !allowed.contains(&pair[0].as_str()) || pair[1].is_empty() || !found.insert(&pair[0]) {
            return Err("Unsupported or repeated option".into());
        }
    }
    Ok(())
}
fn execute() -> contract::Result<()> {
    owner::initialize()?;
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("project") => {
            options(&args, &["--repository", "--revision", "--output"])?;
            let root = argument(&args, "--repository")?;
            let source = argument(&args, "--revision")?;
            let output = argument(&args, "--output")?;
            if std::path::Path::new(&output).exists() {
                return Err("Projection output already exists".into());
            }
            let projected = projection::project(
                std::path::Path::new(&root),
                &source,
                std::path::Path::new(&output),
            );
            let mut log = evidence::Log::new(std::path::Path::new(&output));
            log.event(
                "INFO",
                "projection",
                if projected.is_ok() {
                    "Exact source projected"
                } else {
                    "Projection refused"
                },
            );
            let manifest = projected?;
            println!("{}", serde_json::to_string(&manifest)?);
        }
        Some("run") => {
            options(&args, &["--image", "--source", "--mode", "--output"])?;
            sandbox::execute(
                &argument(&args, "--image")?,
                &argument(&args, "--source")?,
                &argument(&args, "--mode")?,
                std::path::Path::new(&argument(&args, "--output")?),
            )?;
        }
        Some("worker") if args.len() == 2 => worker::execute(&args[1])?,
        Some("verify") => {
            options(&args, &["--projection"])?;
            let root = std::path::PathBuf::from(argument(&args, "--projection")?);
            let bytes = std::fs::read(root.join("manifest.json"))?;
            if bytes.len() > 1024 * 1024 {
                return Err("Manifest oversized".into());
            }
            let checked = projection::verify(&root.join("input"), &serde_json::from_slice(&bytes)?);
            let mut log = evidence::Log::new(&root);
            log.event(
                if checked.is_ok() { "INFO" } else { "ERROR" },
                "integrity",
                if checked.is_ok() {
                    "Declared inputs verified"
                } else {
                    "Input drift refused"
                },
            );
            checked?;
        }
        _ => return Err("Unsupported operation".into()),
    }
    Ok(())
}
fn main() {
    if let Err(error) = execute() {
        // Never echo untrusted path, environment or captured tool output.
        eprintln!(
            "Controlled input operation refused or failed: {}",
            evidence::reason(error.as_ref())
        );
        std::process::exit(1);
    }
}
