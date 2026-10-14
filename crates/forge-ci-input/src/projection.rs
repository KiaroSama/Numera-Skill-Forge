use crate::contract::{self, InputFile, Manifest, Result};
use std::{fs, path::Path};

fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let root = root.to_str().ok_or("Repository path is not UTF-8")?;
    let mut argv = vec!["-C".to_owned(), root.to_owned()];
    argv.extend(args.iter().map(|s| (*s).to_owned()));
    Ok(forge_core::process::git_isolated(
        &argv,
        20,
        &[("GIT_NO_REPLACE_OBJECTS", "1")],
    )?)
}
pub fn project(root: &Path, source: &str, output: &Path) -> Result<Manifest> {
    contract::revision(source)?;
    let tree = git(root, &["ls-tree", "-rz", "--full-tree", source])?;
    if tree.len() > 8 * 1024 * 1024 {
        return Err("Git tree output exceeds limit".into());
    }
    let mut files = Vec::new();
    let mut total = 0u64;
    // Build a fresh projection; no checkout filters, filesystem aliases or ignored data.
    fs::create_dir(output)?;
    let input = output.join("input");
    fs::create_dir(&input)?;
    for record in tree.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let line = std::str::from_utf8(record)?;
        let (metadata, path) = line.split_once('\t').ok_or("Malformed Git tree")?;
        if !contract::selected(path) {
            continue;
        }
        contract::safe_path(path)?;
        let parts: Vec<_> = metadata.split(' ').collect();
        if parts.len() != 3 || parts[1] != "blob" || !matches!(parts[0], "100644" | "100755") {
            return Err(
                "Selected input must be a regular Git blob, not a link or submodule".into(),
            );
        }
        if files.len() >= contract::MAX_FILES {
            return Err("Input file count exceeds limit".into());
        }
        let size = String::from_utf8(git(root, &["cat-file", "-s", parts[2]])?)?
            .trim()
            .parse::<u64>()?;
        total = total.checked_add(size).ok_or("Input size overflow")?;
        if total > contract::MAX_SOURCE {
            return Err("Source projection exceeds 64 MiB".into());
        }
        let bytes = git(root, &["cat-file", "blob", parts[2]])?;
        if bytes.len() as u64 != size {
            return Err("Git blob size changed".into());
        }
        let destination = input.join(path);
        fs::create_dir_all(destination.parent().ok_or("Missing input parent")?)?;
        fs::write(&destination, &bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                &destination,
                fs::Permissions::from_mode(if parts[0] == "100755" { 0o755 } else { 0o644 }),
            )?;
        }
        files.push(InputFile {
            path: path.into(),
            mode: parts[0].into(),
            bytes: size,
            sha256: contract::digest(&bytes),
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let manifest = Manifest {
        schema: 1,
        contract: contract::CONTRACT.into(),
        source_revision: source.into(),
        files,
    };
    contract::validate(&manifest)?;
    fs::write(output.join("manifest.json"), serde_json::to_vec(&manifest)?)?;
    Ok(manifest)
}

pub fn verify(input: &Path, manifest: &Manifest) -> Result<()> {
    contract::validate(manifest)?;
    let root = fs::symlink_metadata(input)?;
    if !root.is_dir() || root.file_type().is_symlink() {
        return Err("Unsafe input root".into());
    }
    let expected: std::collections::BTreeSet<_> =
        manifest.files.iter().map(|f| f.path.as_str()).collect();
    let mut found = std::collections::BTreeSet::new();
    let mut directories = vec![input.to_path_buf()];
    let mut entries = 0;
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(directory)? {
            entries += 1;
            if entries > contract::MAX_FILES * 3 {
                return Err("Input traversal limit exceeded".into());
            }
            let entry = entry?;
            let path = entry.path();
            let relative = path
                .strip_prefix(input)?
                .to_str()
                .ok_or("Input path encoding")?
                .replace('\\', "/");
            // These are declared image dependency and private output domains, not source.
            if input == Path::new("/input")
                && matches!(
                    relative.as_str(),
                    "manifest.json"
                        | "control-write-probe"
                        | ".ci-work"
                        | ".forge-state"
                        | "apps/desktop/node_modules"
                        | "apps/desktop/dist"
                        | "apps/desktop/coverage"
                )
            {
                continue;
            }
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                return Err("Input symlink refused".into());
            }
            if metadata.is_dir() {
                if !expected
                    .iter()
                    .any(|p| p.starts_with(&format!("{relative}/")))
                {
                    return Err("Undeclared source directory".into());
                }
                directories.push(path);
            } else if metadata.is_file() && expected.contains(relative.as_str()) {
                found.insert(relative);
            } else {
                return Err("Undeclared or special source input".into());
            }
        }
    }
    if found.len() != expected.len() {
        return Err("Missing declared input".into());
    }
    for file in &manifest.files {
        let path = input.join(&file.path);
        let metadata = fs::symlink_metadata(&path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            let mode = if file.mode == "100755" { 0o755 } else { 0o644 };
            if metadata.nlink() != 1 || metadata.permissions().mode() & 0o777 != mode {
                return Err("Input mode or hardlink drift".into());
            }
        }
        if metadata.len() != file.bytes || contract::digest(&fs::read(&path)?) != file.sha256 {
            return Err("Declared input drift".into());
        }
    }
    Ok(())
}
