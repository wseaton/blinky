use std::{
    env, fmt,
    fs::OpenOptions,
    io::{BufRead, BufReader, Write},
    os::unix::process::parent_id,
    path::Path,
    process::{self, Child, Command, ExitCode, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

use blinky::{Blynclight, Color, FlashSpeed};
use color_eyre::eyre::{Result, WrapErr, eyre};
use signal_hook::{
    consts::{SIGHUP, SIGINT, SIGTERM},
    iterator::Signals,
};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

const TOUCH_TIMEOUT: Duration = Duration::from_secs(60);
const TOUCH_COLOR: Color = Color {
    red: 128,
    green: 0,
    blue: 128,
};
const NOTIFICATION_TITLE: &str = "YubiKey touch needed";
const NOTIFIER: &str = "terminal-notifier";
const TTY: &str = "/dev/tty";
const MAX_ANCESTORS: usize = 16;
const PLUMBING: [&str; 7] = ["ssh", "git", "sh", "bash", "zsh", "fish", "env"];

#[derive(Debug, PartialEq, Eq)]
enum Prompt {
    Touch,
    Echoed,
    Secret,
}

impl Prompt {
    fn classify(hint: Option<&str>, text: &str) -> Self {
        match hint {
            Some("none") => Self::Touch,
            Some("confirm") => Self::Echoed,
            _ if text.contains("(yes/no") => Self::Echoed,
            _ => Self::Secret,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum SshRequest {
    Push(String),
    Fetch(String),
    Other(String),
}

impl SshRequest {
    fn from_args(args: &[String]) -> Self {
        let line = args.join(" ");
        if let Some((_, repo)) = line.split_once("git-receive-pack ") {
            Self::Push(repo_name(repo))
        } else if let Some((_, repo)) = line.split_once("git-upload-pack ") {
            Self::Fetch(repo_name(repo))
        } else {
            Self::Other(command_line(args))
        }
    }
}

impl fmt::Display for SshRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Push(repo) => write!(f, "git push {repo}"),
            Self::Fetch(repo) => write!(f, "git fetch {repo}"),
            Self::Other(command) => f.write_str(command),
        }
    }
}

fn repo_name(quoted: &str) -> String {
    let path = quoted.trim().trim_matches('\'').trim_start_matches('/');
    path.strip_suffix(".git").unwrap_or(path).to_owned()
}

fn command_line(args: &[String]) -> String {
    let Some((program, rest)) = args.split_first() else {
        return "ssh".to_owned();
    };
    let program = Path::new(program).file_name().map_or_else(
        || program.clone(),
        |name| name.to_string_lossy().into_owned(),
    );
    std::iter::once(program.as_str())
        .chain(rest.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, PartialEq, Eq)]
struct Ancestor {
    name: String,
    args: Vec<String>,
}

fn ancestors() -> Result<Vec<Ancestor>> {
    let mut system = System::new().wrap_err("failed to read the process table")?;
    let refresh = ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always);
    let mut chain = Vec::new();
    let mut pid = Pid::from_u32(parent_id());
    while chain.len() < MAX_ANCESTORS && pid.as_u32() > 1 {
        system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), false, refresh);
        let Some(process) = system.process(pid) else {
            break;
        };
        chain.push(Ancestor {
            name: process.name().to_string_lossy().into_owned(),
            args: process
                .cmd()
                .iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect(),
        });
        let Some(parent) = process.parent() else {
            break;
        };
        pid = parent;
    }
    Ok(chain)
}

fn is_plumbing(name: &str) -> bool {
    PLUMBING.contains(&name) || name.starts_with("git-")
}

fn requester(chain: &[Ancestor]) -> Option<&str> {
    chain
        .iter()
        .map(|ancestor| ancestor.name.trim_start_matches('-'))
        .find(|name| !name.is_empty() && !is_plumbing(name))
}

fn display_dir(dir: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| dir.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => format!("~/{}", rest.display()),
        None => dir.display().to_string(),
    }
}

#[derive(Debug, PartialEq, Eq)]
struct TouchRequest {
    ssh: SshRequest,
    dir: Option<String>,
    requester: Option<String>,
}

impl TouchRequest {
    fn from_context(chain: &[Ancestor], dir: Option<&Path>, home: Option<&Path>) -> Self {
        Self {
            ssh: SshRequest::from_args(chain.first().map_or(&[], |ssh| ssh.args.as_slice())),
            dir: dir.map(|dir| display_dir(dir, home)),
            requester: requester(chain).map(str::to_owned),
        }
    }

    fn detect() -> Self {
        let chain = report(ancestors()).unwrap_or_default();
        let dir = env::current_dir().ok();
        let home = env::home_dir();
        Self::from_context(&chain, dir.as_deref(), home.as_deref())
    }

    fn subtitle(&self) -> String {
        self.ssh.to_string()
    }

    fn message(&self) -> String {
        match (&self.dir, &self.requester) {
            (Some(dir), Some(requester)) => format!("{dir}, from {requester}"),
            (Some(dir), None) => dir.clone(),
            (None, Some(requester)) => format!("from {requester}"),
            (None, None) => "touch the key to continue".to_owned(),
        }
    }
}

fn notifier(args: &[&str]) -> Result<Child> {
    Command::new(NOTIFIER)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .wrap_err("failed to run terminal-notifier")
}

fn start_flashing() -> Result<Option<Blynclight>> {
    let light = Blynclight::open()?;
    if let Some(light) = &light {
        light.show(TOUCH_COLOR, Some(FlashSpeed::Fast))?;
    }
    Ok(light)
}

fn report<T>(result: Result<T>) -> Option<T> {
    result
        .map_err(|error| eprintln!("blinky-askpass: {error:#}"))
        .ok()
}

fn signal_touch() -> Result<()> {
    let mut signals =
        Signals::new([SIGTERM, SIGINT, SIGHUP]).wrap_err("failed to install signal handlers")?;
    let (stopped, stop) = mpsc::channel();
    thread::spawn(move || {
        if signals.forever().next().is_some() {
            let _ = stopped.send(());
        }
    });

    let request = TouchRequest::detect();
    let group = format!("blinky-touch-{}", process::id());
    let posting = report(notifier(&[
        "-title",
        NOTIFICATION_TITLE,
        "-subtitle",
        &request.subtitle(),
        "-message",
        &request.message(),
        "-group",
        &group,
    ]));
    let light = report(start_flashing()).flatten();

    let _ = stop.recv_timeout(TOUCH_TIMEOUT);

    if let Some(light) = light {
        report(light.off());
    }
    if let Some(mut posting) = posting {
        report(
            posting
                .wait()
                .wrap_err("failed to wait for terminal-notifier"),
        );
        report(notifier(&["-remove", &group]));
    }
    Ok(())
}

fn ask_echoed(prompt: &str) -> Result<String> {
    let mut tty = OpenOptions::new()
        .read(true)
        .write(true)
        .open(TTY)
        .wrap_err("no terminal to prompt on")?;
    tty.write_all(prompt.as_bytes())?;
    tty.flush()?;
    let mut answer = String::new();
    if BufReader::new(tty).read_line(&mut answer)? == 0 {
        return Err(eyre!("terminal closed before an answer was given"));
    }
    Ok(answer.trim_end_matches(['\n', '\r']).to_owned())
}

fn ask_secret(prompt: &str) -> Result<String> {
    rpassword::prompt_password(prompt).wrap_err("failed to read from the terminal")
}

fn run() -> Result<()> {
    let prompt = env::args().nth(1).unwrap_or_default();
    let hint = env::var("SSH_ASKPASS_PROMPT").ok();
    let answer = match Prompt::classify(hint.as_deref(), &prompt) {
        Prompt::Touch => return signal_touch(),
        Prompt::Echoed => ask_echoed(&prompt)?,
        Prompt::Secret => ask_secret(&prompt)?,
    };
    println!("{answer}");
    Ok(())
}

fn main() -> ExitCode {
    match report(run()) {
        Some(()) => ExitCode::SUCCESS,
        None => ExitCode::FAILURE,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::{
        Ancestor, Prompt, SshRequest, TouchRequest, command_line, display_dir, is_plumbing,
        repo_name, requester,
    };

    fn args(args: &[&str]) -> Vec<String> {
        args.iter().map(|&arg| arg.to_owned()).collect()
    }

    fn ancestor(name: &str, cmd: &[&str]) -> Ancestor {
        Ancestor {
            name: name.to_owned(),
            args: args(cmd),
        }
    }

    #[test]
    fn touch_hint_wins_over_prompt_text() {
        assert_eq!(
            Prompt::classify(
                Some("none"),
                "Confirm user presence for key ECDSA-SK SHA256:abc"
            ),
            Prompt::Touch
        );
        assert_eq!(Prompt::classify(Some("none"), "(yes/no)?"), Prompt::Touch);
    }

    #[test]
    fn confirmations_are_echoed() {
        assert_eq!(
            Prompt::classify(Some("confirm"), "Allow use of key?"),
            Prompt::Echoed
        );
        assert_eq!(
            Prompt::classify(
                None,
                "Are you sure you want to continue connecting (yes/no/[fingerprint])? "
            ),
            Prompt::Echoed
        );
    }

    #[test]
    fn everything_else_is_a_secret() {
        assert_eq!(
            Prompt::classify(None, "Enter passphrase for key '/home/me/.ssh/id': "),
            Prompt::Secret
        );
        assert_eq!(
            Prompt::classify(Some(""), "me@host's password: "),
            Prompt::Secret
        );
        assert_eq!(Prompt::classify(None, ""), Prompt::Secret);
    }

    #[test]
    fn push_and_fetch_are_read_from_the_remote_command() {
        assert_eq!(
            SshRequest::from_args(&args(&[
                "/opt/homebrew/bin/ssh",
                "-o",
                "SendEnv=GIT_PROTOCOL",
                "git@github.com",
                "git-receive-pack 'wseaton/blinky.git'",
            ])),
            SshRequest::Push("wseaton/blinky".to_owned())
        );
        assert_eq!(
            SshRequest::from_args(&args(&[
                "ssh",
                "git@github.com",
                "git-upload-pack '/srv/repos/thing.git'",
            ])),
            SshRequest::Fetch("srv/repos/thing".to_owned())
        );
        assert_eq!(
            SshRequest::from_args(&args(&[
                "ssh",
                "git@github.com",
                "git-upload-pack",
                "'wseaton/blinky.git'",
            ])),
            SshRequest::Fetch("wseaton/blinky".to_owned())
        );
    }

    #[test]
    fn other_ssh_commands_keep_their_arguments() {
        assert_eq!(
            SshRequest::from_args(&args(&["/usr/bin/ssh", "-T", "git@github.com"])),
            SshRequest::Other("ssh -T git@github.com".to_owned())
        );
        assert_eq!(
            SshRequest::from_args(&[]),
            SshRequest::Other("ssh".to_owned())
        );
    }

    #[test]
    fn requests_render_as_git_operations() {
        assert_eq!(
            SshRequest::Push("wseaton/blinky".to_owned()).to_string(),
            "git push wseaton/blinky"
        );
        assert_eq!(
            SshRequest::Fetch("wseaton/blinky".to_owned()).to_string(),
            "git fetch wseaton/blinky"
        );
        assert_eq!(
            SshRequest::Other("ssh homeassistant".to_owned()).to_string(),
            "ssh homeassistant"
        );
    }

    #[test]
    fn repo_names_lose_quotes_and_suffix() {
        assert_eq!(repo_name("'wseaton/blinky.git'"), "wseaton/blinky");
        assert_eq!(repo_name("wseaton/blinky"), "wseaton/blinky");
        assert_eq!(repo_name(" '/abs/path.git' "), "abs/path");
        assert_eq!(repo_name(""), "");
    }

    #[test]
    fn command_line_uses_the_program_basename() {
        assert_eq!(
            command_line(&args(&["/opt/homebrew/bin/ssh", "host"])),
            "ssh host"
        );
        assert_eq!(command_line(&args(&["ssh"])), "ssh");
        assert_eq!(command_line(&[]), "ssh");
    }

    #[test]
    fn plumbing_covers_shells_and_git_helpers() {
        for name in [
            "ssh",
            "git",
            "sh",
            "bash",
            "zsh",
            "fish",
            "env",
            "git-remote-https",
        ] {
            assert!(is_plumbing(name), "{name}");
        }
        for name in ["claude", "Ghostty", "tmux", "gitui", "sshd"] {
            assert!(!is_plumbing(name), "{name}");
        }
    }

    #[test]
    fn requester_is_the_first_ancestor_that_is_not_plumbing() {
        let chain = [
            ancestor("ssh", &["ssh", "git@github.com"]),
            ancestor("git", &["git", "push"]),
            ancestor("-zsh", &["-zsh"]),
            ancestor("claude", &["claude"]),
            ancestor("Ghostty", &["ghostty"]),
        ];
        assert_eq!(requester(&chain), Some("claude"));
    }

    #[test]
    fn requester_is_absent_when_only_plumbing_remains() {
        let chain = [
            ancestor("ssh", &["ssh"]),
            ancestor("git", &["git"]),
            ancestor("-fish", &["-fish"]),
            ancestor("", &[]),
        ];
        assert_eq!(requester(&chain), None);
        assert_eq!(requester(&[]), None);
    }

    #[test]
    fn directories_under_home_are_abbreviated() {
        let home = Path::new("/Users/me");
        assert_eq!(
            display_dir(Path::new("/Users/me/git/blinky"), Some(home)),
            "~/git/blinky"
        );
        assert_eq!(display_dir(home, Some(home)), "~");
        assert_eq!(display_dir(Path::new("/tmp/x"), Some(home)), "/tmp/x");
        assert_eq!(
            display_dir(Path::new("/Users/meow/x"), Some(home)),
            "/Users/meow/x"
        );
        assert_eq!(display_dir(Path::new("/Users/me/x"), None), "/Users/me/x");
    }

    #[test]
    fn touch_request_describes_a_git_push() {
        let chain = [
            ancestor(
                "ssh",
                &[
                    "ssh",
                    "git@github.com",
                    "git-receive-pack 'wseaton/blinky.git'",
                ],
            ),
            ancestor("git", &["git", "push"]),
            ancestor("zsh", &["zsh", "-c", "git push"]),
            ancestor("claude", &["claude"]),
        ];
        let request = TouchRequest::from_context(
            &chain,
            Some(Path::new("/Users/me/git/blinky")),
            Some(Path::new("/Users/me")),
        );
        assert_eq!(request.subtitle(), "git push wseaton/blinky");
        assert_eq!(request.message(), "~/git/blinky, from claude");
    }

    #[test]
    fn touch_request_degrades_without_context() {
        let bare = TouchRequest::from_context(&[], None, None);
        assert_eq!(bare.subtitle(), "ssh");
        assert_eq!(bare.message(), "touch the key to continue");

        let dir_only = TouchRequest::from_context(&[], Some(Path::new("/srv")), None);
        assert_eq!(dir_only.message(), "/srv");

        let chain = [
            ancestor("ssh", &["ssh", "host"]),
            ancestor("tmux", &["tmux"]),
        ];
        let requester_only = TouchRequest::from_context(&chain, None, None);
        assert_eq!(requester_only.subtitle(), "ssh host");
        assert_eq!(requester_only.message(), "from tmux");
    }
}
