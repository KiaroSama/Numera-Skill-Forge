use crate::{Error, Result, model::Source, storage, validation};
use std::path::{Path, PathBuf};

pub fn header(source: &Source, offline: bool) -> Result<Option<String>> {
    let Some(key) = &source.credential_reference else {
        return Ok(None);
    };
    if offline {
        return Ok(None);
    }
    if !source.repository.starts_with("https://") {
        return Err(Error::new(
            "credential-reference",
            "HTTP credentials cannot be used for SSH",
        ));
    }
    let value = std::env::var(key).map_err(|_| {
        Error::new(
            "credential-missing",
            "Selected credential environment key is missing or not UTF-8",
        )
    })?;
    if value.len() > 8192
        || value.len() < 8
        || !value.bytes().all(|b| (32..=126).contains(&b))
        || !(value.starts_with("Bearer ") || value.starts_with("Basic "))
        || value.ends_with(' ')
    {
        return Err(Error::new(
            "credential-format",
            "Selected key must contain a bounded Basic or Bearer authorization value without control characters",
        ));
    }
    Ok(Some(format!("Authorization: {value}")))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ssh {
    pub public_key_reference: String,
    pub known_hosts_reference: String,
    pub agent_reference: String,
}
fn key(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 64
        || !name.as_bytes()[0].is_ascii_alphabetic()
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
    {
        return Err(Error::new(
            "ssh-policy",
            "SSH references must be public environment key names",
        ));
    }
    Ok(())
}
pub fn ssh(source: &Source) -> Result<Option<Ssh>> {
    let Some(value) = source.metadata.get("ssh_authentication") else {
        return Ok(None);
    };
    if !source.repository.starts_with("ssh://") || source.credential_reference.is_some() {
        return Err(Error::new(
            "ssh-policy",
            "SSH authentication requires an SSH repository without HTTP credentials",
        ));
    }
    let policy: Ssh = serde_json::from_value(value.clone()).map_err(|_| {
        Error::new(
            "ssh-policy",
            "Invalid explicit SSH authentication references",
        )
    })?;
    for name in [
        &policy.public_key_reference,
        &policy.known_hosts_reference,
        &policy.agent_reference,
    ] {
        key(name)?;
    }
    Ok(Some(policy))
}
fn public_file(reference: &str) -> Result<Vec<u8>> {
    let path = PathBuf::from(std::env::var_os(reference).ok_or_else(|| {
        Error::new(
            "ssh-identity-missing",
            "Selected public SSH identity/known-host environment key is missing",
        )
    })?);
    read_public(&path)
}
fn read_public(path: &Path) -> Result<Vec<u8>> {
    if !path.is_absolute() {
        return Err(Error::new(
            "ssh-policy",
            "Selected public SSH files must have absolute paths",
        ));
    }
    for ancestor in path.ancestors() {
        validation::reject_link(ancestor)?;
    }
    if !std::fs::symlink_metadata(path)?.is_file() {
        return Err(Error::new(
            "ssh-policy",
            "Selected public SSH input must be a regular file",
        ));
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        #[cfg(target_os = "linux")]
        let flags = 0o4000 | 0o400000;
        #[cfg(not(target_os = "linux"))]
        let flags = 0x0004 | 0x0100;
        options.custom_flags(flags);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > 65536 {
        return Err(Error::new(
            "ssh-policy",
            "Public SSH input is not a bounded regular file",
        ));
    }
    use std::io::Read;
    let mut bytes = Vec::new();
    file.take(65537).read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        return Err(Error::new("ssh-policy", "Public SSH input exceeds limit"));
    }
    Ok(bytes)
}
fn config_path(value: &str) -> Result<String> {
    if value.contains(['%', '$', '\0', '\r', '\n']) {
        return Err(Error::new(
            "ssh-policy",
            "SSH path contains unsupported expansion tokens",
        ));
    }
    let value = if cfg!(windows) {
        value.replace('\\', "/")
    } else {
        value.to_owned()
    };
    Ok(format!(
        "\"{}\"",
        value.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}
fn quote(value: &str) -> Result<String> {
    if value.contains(['\0', '\r', '\n']) {
        return Err(Error::new("ssh-policy", "Invalid SSH execution argument"));
    }
    Ok(format!("'{}'", value.replace('\'', "'\\''")))
}
fn public_identity(public: &[u8]) -> Result<String> {
    let text = std::str::from_utf8(public)
        .map_err(|_| Error::new("ssh-policy", "Public key must be UTF-8"))?;
    let fields = text.split_whitespace().collect::<Vec<_>>();
    if fields.len() < 2
        || text.lines().filter(|line| !line.trim().is_empty()).count() != 1
        || !matches!(
            fields[0],
            "ssh-ed25519"
                | "ssh-rsa"
                | "ecdsa-sha2-nistp256"
                | "ecdsa-sha2-nistp384"
                | "ecdsa-sha2-nistp521"
        )
        || !fields[1]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='))
    {
        return Err(Error::new(
            "ssh-policy",
            "Selected identity must be one public SSH key record, never bundled private material",
        ));
    }
    Ok(format!("{} {}\n", fields[0], fields[1]))
}
pub fn command(source: &Source, home: &Path) -> Result<Option<String>> {
    if !source.repository.starts_with("ssh://") {
        return Ok(None);
    }
    let policy = ssh(source)?.ok_or_else(|| {
        Error::new(
            "ssh-policy",
            "Supply explicit SSH public-key, known-host and agent references",
        )
    })?;
    let public = public_file(&policy.public_key_reference)?;
    let identity = public_identity(&public)?;
    let hosts = public_file(&policy.known_hosts_reference)?;
    let host = source
        .repository
        .strip_prefix("ssh://")
        .and_then(|url| url.split_once('/'))
        .and_then(|(authority, _)| authority.split_once('@'))
        .map(|(_, host)| host)
        .ok_or_else(|| Error::new("ssh-policy", "Invalid canonical SSH host"))?;
    let text = std::str::from_utf8(&hosts)
        .map_err(|_| Error::new("ssh-policy", "Pinned known-host input must be UTF-8"))?;
    let mut selected = String::new();
    for line in text
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
    {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() < 3
            || parts[0] != host
            || !matches!(
                parts[1],
                "ssh-ed25519"
                    | "ssh-rsa"
                    | "ecdsa-sha2-nistp256"
                    | "ecdsa-sha2-nistp384"
                    | "ecdsa-sha2-nistp521"
            )
            || !parts[2]
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='))
        {
            return Err(Error::new(
                "ssh-policy",
                "Known-host file must pin only the exact selected host with explicit public keys",
            ));
        }
        selected.push_str(line);
        selected.push('\n');
    }
    if selected.is_empty() {
        return Err(Error::new("ssh-policy", "Pinned host key is missing"));
    }
    let agent = std::env::var(&policy.agent_reference).map_err(|_| {
        Error::new(
            "ssh-identity-missing",
            "Selected agent environment key is missing or not UTF-8",
        )
    })?;
    if agent.is_empty()
        || !Path::new(&agent).is_absolute()
        || agent.contains(['%', '\0', '\r', '\n'])
    {
        return Err(Error::new(
            "ssh-policy",
            "Selected SSH agent must be an explicit absolute socket/pipe path",
        ));
    }
    storage::atomic(&home.join("ssh_config"), b"")?;
    storage::atomic(&home.join("identity.pub"), identity.as_bytes())?;
    storage::atomic(&home.join("known_hosts"), selected.as_bytes())?;
    let path = |name: &str| super::providers::external_path(&home.join(name).to_string_lossy());
    let mut args = vec![
        "ssh".to_owned(),
        "-F".into(),
        path("ssh_config"),
        "-i".into(),
        path("identity.pub"),
    ];
    for option in [
        "BatchMode=yes",
        "IdentitiesOnly=yes",
        "StrictHostKeyChecking=yes",
        "UpdateHostKeys=no",
        "PasswordAuthentication=no",
        "KbdInteractiveAuthentication=no",
        "ForwardAgent=no",
        "ClearAllForwardings=yes",
        "ConnectionAttempts=1",
        "ConnectTimeout=15",
        "CertificateFile=none",
        "IdentityFile=none",
        "KnownHostsCommand=none",
    ] {
        args.extend(["-o".into(), option.into()]);
    }
    args.extend([
        "-o".into(),
        format!("IdentityAgent={}", config_path(&agent)?),
        "-o".into(),
        format!("UserKnownHostsFile={}", config_path(&path("known_hosts"))?),
        "-o".into(),
        "GlobalKnownHostsFile=none".into(),
    ]);
    Ok(Some(
        args.iter()
            .map(|arg| quote(arg))
            .collect::<Result<Vec<_>>>()?
            .join(" "),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_identity_does_not_copy_bundled_private_material_or_comments() {
        assert!(
            public_identity(
                b"ssh-ed25519 AAAA public\n-----BEGIN OPENSSH PRIVATE KEY-----\nfixture\n"
            )
            .is_err()
        );
        assert_eq!(
            public_identity(b"ssh-ed25519 AAAA private-comment-not-copied\n").unwrap(),
            "ssh-ed25519 AAAA\n"
        );
        assert!(config_path("/tmp/${HOME}/socket").is_err());
    }
    #[cfg(unix)]
    #[test]
    fn special_public_file_is_refused_before_read() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.ci-work/tests")
            .join(crate::transaction::operation_id());
        std::fs::create_dir_all(&path).unwrap();
        let fifo = path.join("public.fifo");
        use std::os::unix::ffi::OsStrExt;
        let cpath = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        #[cfg(target_os = "macos")]
        type Mode = u16;
        #[cfg(not(target_os = "macos"))]
        type Mode = u32;
        unsafe extern "C" {
            fn mkfifo(path: *const std::ffi::c_char, mode: Mode) -> i32;
        }
        assert_eq!(unsafe { mkfifo(cpath.as_ptr(), 0o600) }, 0);
        let result = read_public(&fifo);
        std::fs::remove_dir_all(path).unwrap();
        assert_eq!(result.unwrap_err().code, "ssh-policy");
    }
}
