use crate::{Args, Mode, Result, field, option, pane, t};
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
pub fn spec(raw: &str, default: Vec<String>) -> Result<Vec<String>> {
    if raw.is_empty() {
        return Ok(default);
    }
    let args = if raw.starts_with('[') {
        serde_json::from_str::<Vec<String>>(raw).map_err(|e| e.to_string())?
    } else {
        shlex::split(raw).ok_or("Invalid command quoting")?
    };
    if args.is_empty() {
        return Err("Empty command".into());
    }
    Ok(args)
}
fn input(a: &Args, cwd: &Path) -> Result<String> {
    let raw = if let Some(s) = &a.text {
        s.clone()
    } else {
        let mut s = String::new();
        std::io::stdin()
            .take(1024 * 1024)
            .read_to_string(&mut s)
            .map_err(|e| e.to_string())?;
        s
    };
    let raw = raw.trim_end_matches(['\r', '\n']);
    let value = if cwd.join(raw).exists() {
        raw
    } else {
        raw.trim()
    };
    if value.is_empty() {
        return Err("No selection".into());
    }
    if value
        .chars()
        .any(|c| c == '\0' || (c.is_control() && !matches!(a.mode, Mode::Web)))
    {
        return Err("Selection contains control characters".into());
    }
    Ok(value.into())
}
fn file_target(cwd: &Path, text: &str) -> Option<(PathBuf, Option<u32>)> {
    let p = cwd.join(text);
    if p.exists() {
        return Some((p, None));
    }
    let mut rest = text;
    let mut nums = Vec::new();
    while let Some((head, tail)) = rest.rsplit_once(':') {
        if let Ok(n) = tail.parse::<u32>() {
            nums.push(n);
            rest = head;
            if nums.len() == 2 {
                break;
            }
        } else {
            break;
        }
    }
    let p = cwd.join(rest);
    if !nums.is_empty() && p.is_file() {
        Some((p, nums.last().copied()))
    } else {
        None
    }
}
fn opener(a: &Args, target: &str, cwd: &Path) -> Result<()> {
    let default = if cfg!(target_os = "macos") {
        vec!["/usr/bin/open".into()]
    } else {
        vec!["xdg-open".into()]
    };
    let cmd = spec(&option(a, "@tmux-actions-opener")?, default)?;
    let status = Command::new(&cmd[0])
        .args(&cmd[1..])
        .arg(target)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("Opener unavailable: {e}"))?;
    if !status.success() {
        return Err("Opener failed; headless SSH needs a configured opener".into());
    }
    Ok(())
}
pub fn run(a: &Args) -> Result<()> {
    let p = pane(a)?;
    let cwd = PathBuf::from(field(a, &p, "pane_current_path")?);
    let text = input(a, &cwd)?;
    if matches!(a.mode, Mode::Web) {
        let new = option(a, "@tmux-actions-search-url")?;
        let legacy = option(a, "@open-S")?;
        let base = if !new.is_empty() {
            new
        } else if !legacy.is_empty() {
            legacy
        } else {
            "https://www.google.com/search?q=".into()
        };
        let encoded = url::form_urlencoded::byte_serialize(text.as_bytes()).collect::<String>();
        let target = format!("{base}{encoded}");
        url::Url::parse(&target).map_err(|_| "Invalid search URL")?;
        return opener(a, &target, &cwd);
    }
    if matches!(a.mode, Mode::Edit) {
        let (file, line) = file_target(&cwd, &text).ok_or("Selected file does not exist")?;
        let preferred = std::env::var("VISUAL")
            .or_else(|_| std::env::var("EDITOR"))
            .unwrap_or_else(|_| "vi".into());
        let mut cmd = spec(
            &option(a, "@tmux-actions-editor")?,
            spec(&preferred, vec!["vi".into()])?,
        )?;
        if let Some(n) = line {
            cmd.push(format!("+{n}"));
        }
        cmd.extend(["--".into(), file.to_string_lossy().into_owned()]);
        let session = field(a, &p, "session_id")?;
        let mut words = vec![
            "new-window".into(),
            "-t".into(),
            format!("{session}:"),
            "-n".into(),
            "edit".into(),
            "-c".into(),
            cwd.to_string_lossy().into_owned(),
        ];
        words.extend(cmd);
        t(a, &words.iter().map(String::as_str).collect::<Vec<_>>())?;
        return Ok(());
    }
    if let Some((file, _)) = file_target(&cwd, &text) {
        return opener(a, &file.to_string_lossy(), &cwd);
    }
    let url = url::Url::parse(&text).map_err(|_| "Selection is neither a file nor a URL")?;
    opener(a, url.as_str(), &cwd)
}
