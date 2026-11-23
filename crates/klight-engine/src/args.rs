//! Command line arguments (no extra dependency needed for this handful).

use std::net::SocketAddr;
use std::path::PathBuf;

use klight_api::{Permission, TokenGrant};

pub const USAGE: &str = "usage: klight-engine [options]
  --show <file>         show file (default: show.klight.json)
  --restore             restore the runtime state saved next to the show
  --bind <addr:port>    API address (default: 0.0.0.0:9400)
  --ui <dir>            web UI directory served to remote clients
  --fixtures <dir>      .qxf directory (repeatable; default: fixtures)
  --profiles <dir>      controller profiles (default: profiles/controllers)
  --qlcplus-profiles <dir>  QLC+ input profiles, .qxi (default: profiles/qlcplus)
  --token <tok>[:perm]  remote client token; perm = full|playback|readonly (repeatable)
  --log-dir <dir>       write rotating log files to this directory
  --exit-on-stdin-close shut down cleanly when stdin closes (used by the supervisor)";

#[derive(Debug, Clone)]
pub struct Args {
    pub show: PathBuf,
    pub restore: bool,
    pub bind: SocketAddr,
    pub ui_dir: Option<PathBuf>,
    pub fixture_dirs: Vec<PathBuf>,
    pub profiles_dir: PathBuf,
    pub qlcplus_profiles_dir: PathBuf,
    pub tokens: Vec<TokenGrant>,
    pub log_dir: Option<PathBuf>,
    pub exit_on_stdin_close: bool,
}

impl Args {
    pub fn parse(mut it: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut a = Args {
            show: PathBuf::from("show.klight.json"),
            restore: false,
            bind: SocketAddr::from(([0, 0, 0, 0], 9400)),
            ui_dir: None,
            fixture_dirs: Vec::new(),
            profiles_dir: PathBuf::from("profiles/controllers"),
            qlcplus_profiles_dir: PathBuf::from("profiles/qlcplus"),
            tokens: Vec::new(),
            log_dir: None,
            exit_on_stdin_close: false,
        };
        while let Some(flag) = it.next() {
            let mut value = |name: &str| it.next().ok_or_else(|| format!("{name} needs a value"));
            match flag.as_str() {
                "--show" => a.show = PathBuf::from(value("--show")?),
                "--restore" => a.restore = true,
                "--exit-on-stdin-close" => a.exit_on_stdin_close = true,
                "--bind" => {
                    a.bind = value("--bind")?
                        .parse()
                        .map_err(|e| format!("--bind: {e}"))?
                }
                "--ui" => a.ui_dir = Some(PathBuf::from(value("--ui")?)),
                "--fixtures" => a.fixture_dirs.push(PathBuf::from(value("--fixtures")?)),
                "--profiles" => a.profiles_dir = PathBuf::from(value("--profiles")?),
                "--qlcplus-profiles" => {
                    a.qlcplus_profiles_dir = PathBuf::from(value("--qlcplus-profiles")?)
                }
                "--log-dir" => a.log_dir = Some(PathBuf::from(value("--log-dir")?)),
                "--token" => {
                    let v = value("--token")?;
                    let (token, perm) = v.split_once(':').unwrap_or((v.as_str(), "full"));
                    let permission = match perm {
                        "full" => Permission::Full,
                        "playback" => Permission::PlaybackOnly,
                        "readonly" => Permission::ReadOnly,
                        other => return Err(format!("unknown permission '{other}'")),
                    };
                    if token.len() < 4 {
                        return Err("tokens must be at least 4 characters".into());
                    }
                    a.tokens.push(TokenGrant {
                        token: token.to_owned(),
                        permission,
                    });
                }
                "-h" | "--help" => return Err("help requested".into()),
                other => return Err(format!("unknown argument '{other}'")),
            }
        }
        if a.fixture_dirs.is_empty() {
            a.fixture_dirs.push(PathBuf::from("fixtures"));
        }
        Ok(a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flags() {
        let a = Args::parse(
            ["--show", "a.json", "--restore", "--token", "abcd:playback"]
                .into_iter()
                .map(String::from),
        )
        .unwrap();
        assert_eq!(a.show, PathBuf::from("a.json"));
        assert!(a.restore);
        assert_eq!(a.tokens[0].permission, Permission::PlaybackOnly);
        assert!(Args::parse(["--bogus"].into_iter().map(String::from)).is_err());
    }
}
