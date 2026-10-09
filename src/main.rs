mod open;
mod sidebar;
use clap::{Parser, ValueEnum};
use std::{path::PathBuf, process::Command};
pub type Result<T> = std::result::Result<T, String>;
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Mode {
    Configure,
    Open,
    Edit,
    Web,
    Sidebar,
    Render,
    Cleanup,
    GitSearch,
}
#[derive(Parser, Debug)]
#[command(version, about)]
pub struct Args {
    #[arg(value_enum)]
    pub mode: Mode,
    #[arg(long)]
    pub socket: PathBuf,
    #[arg(long)]
    pub pane: Option<String>,
    #[arg(long)]
    pub window: Option<String>,
    #[arg(long)]
    pub directory: Option<PathBuf>,
    #[arg(long)]
    pub launcher: Option<PathBuf>,
    #[arg(long)]
    pub text: Option<String>,
    #[arg(long)]
    pub focus: bool,
    #[arg(long, default_value = "tmux")]
    pub tmux_bin: PathBuf,
}
pub fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
pub fn t(a: &Args, words: &[&str]) -> Result<String> {
    let out = Command::new(&a.tmux_bin)
        .arg("-S")
        .arg(&a.socket)
        .args(words)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().into());
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .trim_end_matches('\n')
        .into())
}
pub fn option(a: &Args, key: &str) -> Result<String> {
    t(a, &["show-option", "-gqv", key])
}
pub fn pane(a: &Args) -> Result<String> {
    a.pane
        .clone()
        .or_else(|| std::env::var("TMUX_PANE").ok())
        .ok_or("pane required".into())
}
pub fn field(a: &Args, target: &str, key: &str) -> Result<String> {
    t(
        a,
        &[
            "display-message",
            "-p",
            "-t",
            target,
            &format!("#{{{key}}}"),
        ],
    )
}
pub fn message(a: &Args, text: &str) -> Result<()> {
    t(a, &["display-message", "-l", text])?;
    Ok(())
}
fn call(a: &Args, mode: &str, extra: &[&str]) -> Result<String> {
    let launcher = a.launcher.as_ref().ok_or("launcher required")?;
    let mut args = vec![
        launcher.to_string_lossy().into_owned(),
        mode.into(),
        a.socket.to_string_lossy().into_owned(),
    ];
    args.extend(extra.iter().map(|s| (*s).into()));
    Ok(format!(
        "exec {}",
        args.iter().map(|s| quote(s)).collect::<Vec<_>>().join(" ")
    ))
}
fn configure(a: &Args) -> Result<()> {
    let v = t(a, &["display-message", "-p", "#{version}"])?;
    let parts = v
        .split('.')
        .take(2)
        .map(|p| {
            p.chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse::<u32>()
                .unwrap_or(0)
        })
        .collect::<Vec<_>>();
    if parts.len() != 2 || (parts[0], parts[1]) < (3, 6) {
        return Err("tmux 3.6+ required".into());
    }
    let overrides = option(a, "@tmux-actions-search-patterns")?;
    let overrides: std::collections::BTreeMap<String, String> = if overrides.is_empty() {
        std::collections::BTreeMap::new()
    } else {
        serde_json::from_str(&overrides).map_err(|e| e.to_string())?
    };
    for (key, pattern) in [
        (
            "C-u",
            r"(https?://|git@|git://|ssh://|ftp://|file:///)[^[:space:]，。！？；：、（）【】《》〈〉「」『』〔〕［］｛｝“”‘’│┃║]+",
        ),
        ("C-d", r"[[:digit:]]+"),
        ("M-i", r"[[:digit:]]{1,3}(\.[[:digit:]]{1,3}){3}"),
        ("M-h", r"[0-9a-f]{7,64}"),
        (
            "C-f",
            r"(~|\.\.?|[[:alnum:]_-]+)?/[][[:alnum:]_.#$%&+=/@-]+",
        ),
    ] {
        let pattern = overrides.get(key).map(String::as_str).unwrap_or(pattern);
        let commands = format!(
            "copy-mode ; send-keys -X clear-selection ; send-keys -X search-backward {}",
            quote(pattern)
        );
        t(a, &["bind-key", key, &commands])?;
    }
    let commands = format!(
        "copy-mode ; command-prompt -T search -p Regex {}",
        quote(r#"send-keys -X search-backward "%%""#)
    );
    t(a, &["bind-key", "/", &commands])?;
    let git = call(a, "git-search", &["--pane", "#{pane_id}"])?;
    t(a, &["bind-key", "C-g", "run-shell", "-b", &git])?;
    for table in ["copy-mode-vi", "copy-mode"] {
        for (key, action) in [("n", "search-again"), ("N", "search-reverse")] {
            t(
                a,
                &["bind-key", "-T", table, key, "send-keys", "-X", action],
            )?;
        }
        for (key, mode) in [("o", "open"), ("C-o", "edit"), ("S", "web")] {
            let cmd = call(a, mode, &["--pane", "#{pane_id}"])?;
            t(
                a,
                &[
                    "bind-key",
                    "-T",
                    table,
                    key,
                    "send-keys",
                    "-X",
                    "copy-pipe-and-cancel",
                    &cmd,
                ],
            )?;
        }
    }
    for table in ["copy-mode-vi", "copy-mode"] {
        for key in ["q", "Escape", "C-c"] {
            let binding = t(a, &["list-keys", "-T", table, key]).unwrap_or_default();
            if binding.contains("tmux-copycat/") {
                t(
                    a,
                    &["bind-key", "-T", table, key, "send-keys", "-X", "cancel"],
                )?;
            }
        }
    }
    t(
        a,
        &[
            "bind-key",
            "-T",
            "copy-mode-vi",
            "Enter",
            "send-keys",
            "-X",
            "copy-selection-and-cancel",
        ],
    )?;
    for (key, focus) in [("Tab", false), ("Bspace", true)] {
        let extra = if focus {
            vec!["--pane", "#{pane_id}", "--focus"]
        } else {
            vec!["--pane", "#{pane_id}"]
        };
        let cmd = call(a, "sidebar", &extra)?;
        t(a, &["bind-key", key, "run-shell", "-b", &cmd])?;
    }
    t(
        a,
        &[
            "set-option",
            "-gq",
            "@tmux-actions-version",
            env!("CARGO_PKG_VERSION"),
        ],
    )?;
    Ok(())
}
fn git_search(a: &Args) -> Result<()> {
    let p = pane(a)?;
    let cwd = field(a, &p, "pane_current_path")?;
    let out = Command::new("git")
        .args(["status", "--porcelain=v1", "-z"])
        .current_dir(cwd)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err("Not a Git working directory".into());
    }
    let text = String::from_utf8(out.stdout).map_err(|_| "Git filename is not UTF-8")?;
    let mut records = text.split('\0');
    let mut names = Vec::new();
    while let Some(record) = records.next() {
        if record.len() < 4 {
            continue;
        }
        let escaped = record[3..]
            .chars()
            .map(|c| {
                if ".[]{}()\\*+?^$|".contains(c) {
                    format!("\\{c}")
                } else {
                    c.to_string()
                }
            })
            .collect::<String>();
        names.push(escaped);
        if record[..2].contains(['R', 'C']) {
            records.next();
        }
    }
    if names.is_empty() {
        return message(a, "No changed files");
    }
    let pattern = format!("({})", names.join("|"));
    t(a, &["copy-mode", "-t", &p])?;
    t(a, &["send-keys", "-t", &p, "-X", "clear-selection"])?;
    t(
        a,
        &["send-keys", "-t", &p, "-X", "search-backward", &pattern],
    )?;
    Ok(())
}
fn run(a: &Args) -> Result<()> {
    match a.mode {
        Mode::Configure => configure(a),
        Mode::Open | Mode::Edit | Mode::Web => open::run(a),
        Mode::GitSearch => git_search(a),
        Mode::Sidebar => sidebar::toggle(a),
        Mode::Render => sidebar::render(a),
        Mode::Cleanup => sidebar::cleanup(a),
    }
}
fn main() {
    let a = Args::parse();
    if let Err(e) = run(&a) {
        let _ = message(&a, &e);
        eprintln!("tmux-actions: {e}");
        std::process::exit(1);
    }
}
