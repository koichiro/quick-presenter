//! User-owned TOML configuration; all file IO is performed by the initialization worker.
use serde::Deserialize;
use std::{fs::File, io::Read, path::PathBuf};

#[derive(Clone)]
pub struct RelayEndpoint {
    pub origin: url::Url,
    pub create_token: String,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    audience: Option<Audience>,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Audience {
    transport: Option<String>,
    relay: Option<String>,
    create_token: Option<String>,
}

pub fn load(explicit: Option<PathBuf>) -> Result<Option<RelayEndpoint>, &'static str> {
    load_from(
        explicit,
        crate::settings_storage::platform_config_dir().map(|p| p.join("config.toml")),
    )
}

fn load_from(
    explicit: Option<PathBuf>,
    default: Option<PathBuf>,
) -> Result<Option<RelayEndpoint>, &'static str> {
    let required = explicit.is_some();
    let Some(path) = explicit.or(default) else {
        return Ok(None);
    };
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) if !required && e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("Cannot open Audience configuration."),
    };
    let metadata = file
        .metadata()
        .map_err(|_| "Cannot inspect Audience configuration.")?;
    if !metadata.is_file() {
        return Err("Audience configuration must be a regular file.");
    }
    let mut text = String::new();
    file.take(16385)
        .read_to_string(&mut text)
        .map_err(|_| "Cannot read Audience configuration.")?;
    if text.len() > 16384 {
        return Err("Audience configuration is too large.");
    }
    let endpoint = parse(&text)?;
    #[cfg(unix)]
    if endpoint.is_some() {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
            return Err("Audience configuration must be owner-only (chmod 600).");
        }
    }
    Ok(endpoint)
}

fn parse(text: &str) -> Result<Option<RelayEndpoint>, &'static str> {
    let bad = "Invalid Audience TOML configuration.";
    let config: Config = toml::from_str(text).map_err(|_| bad)?;
    let a = config.audience.unwrap_or_default();
    match a.transport.as_deref().unwrap_or("local") {
        "local" if a.relay.is_none() && a.create_token.is_none() => Ok(None),
        "remote" => {
            let origin = url::Url::parse(a.relay.as_deref().ok_or(bad)?).map_err(|_| bad)?;
            let create_token = a.create_token.ok_or(bad)?;
            let loopback = origin.host_str().is_some_and(|h| {
                h.trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
            });
            if origin.host_str().is_none()
                || !origin.username().is_empty()
                || origin.password().is_some()
                || origin.path() != "/"
                || origin.query().is_some()
                || origin.fragment().is_some()
                || !(origin.scheme() == "https" || origin.scheme() == "http" && loopback)
                || create_token.len() != 43
                || !create_token
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            {
                return Err(bad);
            }
            Ok(Some(RelayEndpoint {
                origin,
                create_token,
            }))
        }
        _ => Err(bad),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn remote(origin: &str) -> String {
        format!(
            "[audience]\ntransport='remote'\nrelay='{origin}'\ncreate_token='{}'\n",
            "A".repeat(43)
        )
    }
    #[test]
    fn strict_modes_origins_and_credentials() {
        assert!(parse("").unwrap().is_none());
        assert!(parse("[audience]\ntransport='local'").unwrap().is_none());
        for origin in [
            "https://relay.example",
            "http://127.0.0.1:1234",
            "http://[::1]:1234",
        ] {
            assert!(parse(&remote(origin)).unwrap().is_some());
        }
        for origin in [
            "http://relay.example",
            "http://localhost:1234",
            "https://user@relay.example",
            "https://relay.example/base",
            "https://relay.example/?k=a",
            "https://relay.example/#a",
            "ftp://relay.example",
        ] {
            assert!(parse(&remote(origin)).is_err());
        }
        for text in [
            "unknown=1".into(),
            "[audience]\ntransport='bad'".into(),
            "[audience]\nrelay='https://relay.example'".into(),
            "[audience]\ntransport='remote'".into(),
            remote("https://relay.example").replace(&"A".repeat(43), "secret"),
            format!("{}transport='remote'", remote("https://relay.example")),
            format!("{}unknown=1", remote("https://relay.example")),
        ] {
            assert!(parse(&text).is_err());
        }
    }
    #[test]
    fn default_absence_is_local_but_explicit_file_replaces_without_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let default = dir.path().join("config.toml");
        let explicit = dir.path().join("alternate.toml");
        assert!(load_from(None, None).unwrap().is_none());
        assert!(load_from(None, Some(default.clone())).unwrap().is_none());
        std::fs::write(&default, "[audience]\ntransport='local'").unwrap();
        assert!(load_from(Some(explicit.clone()), Some(default.clone())).is_err());
        std::fs::write(&explicit, "invalid?").unwrap();
        assert!(load_from(Some(explicit.clone()), Some(default.clone())).is_err());
        std::fs::write(&default, "invalid?").unwrap();
        std::fs::write(&explicit, "[audience]\ntransport='local'").unwrap();
        assert!(load_from(Some(explicit), Some(default.clone()))
            .unwrap()
            .is_none());
        assert!(load_from(None, Some(default)).is_err());
    }

    #[test]
    fn explicit_file_errors_and_bounds_are_redacted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secret.toml");
        assert_eq!(
            load(Some(path.clone())).err(),
            Some("Cannot open Audience configuration.")
        );
        std::fs::write(&path, "?").unwrap();
        assert!(load(Some(path.clone())).is_err());
        std::fs::write(&path, " ".repeat(16385)).unwrap();
        assert_eq!(
            load(Some(path.clone())).err(),
            Some("Audience configuration is too large.")
        );
        std::fs::write(&path, "[audience]\ntransport='local'").unwrap();
        assert!(load(Some(path.clone())).unwrap().is_none());
        std::fs::write(&path, remote("https://relay.example")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(load(Some(path.clone())).is_err());
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        assert!(load(Some(path)).unwrap().is_some());
        assert!(load(Some(dir.path().into())).is_err());
    }
}
