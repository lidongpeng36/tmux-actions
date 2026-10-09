use crate::{Args, Result, field, message, option, pane, quote, t};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, DirBuilder, File, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
#[derive(Serialize, Deserialize)]
struct Sidebar {
    pane: String,
    source: String,
    directory: String,
    layout: String,
    panes: Vec<String>,
}
fn runtime() -> Result<PathBuf> {
    let p = std::env::temp_dir().join(format!("tmux-actions-{}", unsafe { libc::getuid() }));
    match DirBuilder::new().mode(0o700).create(&p) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.to_string()),
    }
    let m = fs::symlink_metadata(&p).map_err(|e| e.to_string())?;
    if !m.is_dir() || m.uid() != unsafe { libc::getuid() } || m.mode() & 0o077 != 0 {
        return Err("Unsafe runtime directory".into());
    }
    Ok(p)
}
fn window_lock(a: &Args, w: &str) -> Result<File> {
    let id = w
        .strip_prefix('@')
        .and_then(|s| s.parse::<u32>().ok())
        .ok_or("Invalid window ID")?;
    let pid = t(a, &["display", "-p", "#{pid}"])?;
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(runtime()?.join(format!("{pid}-{id}.lock")))
        .map_err(|e| e.to_string())?;
    f.lock().map_err(|e| e.to_string())?;
    Ok(f)
}
fn panes(a: &Args, w: &str) -> Result<Vec<String>> {
    Ok(t(a, &["list-panes", "-t", w, "-F", "#{pane_id}"])?
        .lines()
        .map(str::to_owned)
        .collect())
}
fn state(a: &Args, w: &str) -> Result<Option<Sidebar>> {
    let raw = t(
        a,
        &["show-option", "-wqv", "-t", w, "@tmux-actions-sidebar"],
    )?;
    if raw.is_empty() {
        Ok(None)
    } else {
        serde_json::from_str(&raw)
            .map(Some)
            .map_err(|e| e.to_string())
    }
}
fn width_file(a: &Args) -> Result<PathBuf> {
    let custom = option(a, "@tmux-actions-width-cache")?;
    if let Some(tail) = custom.strip_prefix("~/") {
        return Ok(PathBuf::from(std::env::var_os("HOME").ok_or("HOME unavailable")?).join(tail));
    }
    if !custom.is_empty() {
        return Ok(PathBuf::from(custom));
    }
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME unavailable")?);
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            if cfg!(target_os = "macos") {
                home.join("Library/Caches")
            } else {
                home.join(".cache")
            }
        });
    Ok(base.join("tmux-actions/sidebar-widths.json"))
}
fn width_lock(a: &Args) -> Result<File> {
    let path = width_file(a)?;
    let parent = path.parent().ok_or("Invalid width cache path")?;
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)
        .map_err(|e| e.to_string())?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path.with_extension("lock"))
        .map_err(|e| e.to_string())?;
    file.lock().map_err(|e| e.to_string())?;
    Ok(file)
}
fn widths(a: &Args) -> Result<BTreeMap<String, usize>> {
    let path = width_file(a)?;
    match fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str(&raw).map_err(|e| e.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(e.to_string()),
    }
}
fn cache_width(a: &Args, p: &str, directory: &str) -> Result<()> {
    let width = field(a, p, "pane_width")?.parse::<usize>().unwrap_or(40);
    let _lock = width_lock(a)?;
    let mut saved = widths(a)?;
    saved.insert(directory.into(), width);
    while saved.len() > 128 {
        saved.pop_first();
    }
    let json = serde_json::to_string(&saved).unwrap();
    let path = width_file(a)?;
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)
        .map_err(|e| e.to_string())?;
    file.write_all(json.as_bytes()).map_err(|e| e.to_string())?;
    fs::rename(&temp, &path).map_err(|e| e.to_string())?;
    t(a, &["set-option", "-gq", "@tmux-actions-widths", &json])?;
    Ok(())
}

fn finish(a: &Args, w: &str, s: &Sidebar, kill: bool) -> Result<()> {
    let current = panes(a, w)?;
    if current.contains(&s.pane) {
        cache_width(a, &s.pane, &s.directory)?;
        if kill {
            if field(a, &s.pane, "@tmux-actions-managed")? != "sidebar" {
                return Err("Sidebar ownership changed".into());
            }
            t(a, &["kill-pane", "-t", &s.pane])?;
        } else {
            return Ok(());
        }
    }
    t(a, &["set-option", "-wqu", "-t", w, "@tmux-actions-sidebar"])?;
    let mut remaining = panes(a, w)?;
    remaining.sort();
    let mut original = s.panes.clone();
    original.sort();
    if remaining == original {
        t(a, &["select-layout", "-t", w, &s.layout])?;
    }
    if remaining.contains(&s.source) {
        t(a, &["select-pane", "-t", &s.source])?;
    }
    Ok(())
}
pub fn toggle(a: &Args) -> Result<()> {
    let p = pane(a)?;
    let w = field(a, &p, "window_id")?;
    let _lock = window_lock(a, &w)?;
    if let Some(s) = state(a, &w)? {
        return finish(a, &w, &s, true);
    }
    let size = field(a, &p, "pane_width")?.parse::<usize>().unwrap_or(0);
    if size < 71 {
        return message(a, "Pane too narrow for sidebar");
    }
    let cwd = field(a, &p, "pane_current_path")?;
    let cached = widths(a)?.get(&cwd).copied();
    let preferred = option(a, "@tmux-actions-sidebar-width")?;
    let legacy = option(a, "@sidebar-tree-width")?;
    let width = cached
        .unwrap_or_else(|| preferred.parse().or_else(|_| legacy.parse()).unwrap_or(40))
        .clamp(8, size / 2);
    let mut s = Sidebar {
        pane: String::new(),
        source: p.clone(),
        directory: cwd.clone(),
        layout: field(a, &p, "window_layout")?,
        panes: panes(a, &w)?,
    };
    let side = option(a, "@tmux-actions-sidebar-side")?;
    let old = option(a, "@sidebar-tree-position")?;
    let side = if side.is_empty() {
        if old.is_empty() { "left" } else { &old }
    } else {
        &side
    };
    if !["left", "right"].contains(&side) {
        return Err("sidebar side must be left/right".into());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut words = vec![
        "split-window".into(),
        "-h".into(),
        "-d".into(),
        "-l".into(),
        width.to_string(),
        "-t".into(),
        p.clone(),
        "-c".into(),
        cwd.clone(),
        "-P".into(),
        "-F".into(),
        "#{pane_id}".into(),
    ];
    if side == "left" {
        words.push("-b".into());
    }
    words.extend([
        exe.to_string_lossy().into_owned(),
        "render".into(),
        "--socket".into(),
        a.socket.to_string_lossy().into_owned(),
        "--window".into(),
        w.clone(),
        "--directory".into(),
        cwd,
    ]);
    s.pane = t(a, &words.iter().map(String::as_str).collect::<Vec<_>>())?;
    t(
        a,
        &[
            "set-option",
            "-pq",
            "-t",
            &s.pane,
            "@tmux-actions-managed",
            "sidebar",
        ],
    )?;
    t(
        a,
        &[
            "set-option",
            "-wq",
            "-t",
            &w,
            "@tmux-actions-sidebar",
            &serde_json::to_string(&s).unwrap(),
        ],
    )?;
    if a.focus {
        t(a, &["select-pane", "-t", &s.pane])?;
    } else {
        t(a, &["select-pane", "-t", &p])?;
    }
    Ok(())
}
pub fn cleanup(a: &Args) -> Result<()> {
    let w = a.window.as_deref().ok_or("window required")?;
    let _lock = window_lock(a, w)?;
    if let Some(s) = state(a, w)?
        && a.pane.as_deref() == Some(&s.pane)
    {
        finish(a, w, &s, false)?;
    }
    Ok(())
}
fn tree(dir: &Path, show_hidden: bool) -> String {
    let mut children: BTreeMap<PathBuf, Vec<(PathBuf, bool)>> = BTreeMap::new();
    let walk = walkdir::WalkDir::new(dir)
        .max_depth(6)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            e.depth() == 0
                || ((!e.file_name().to_string_lossy().starts_with('.') || show_hidden)
                    && !matches!(
                        e.file_name().to_str(),
                        Some(".git" | "node_modules" | "target")
                    ))
        });
    let mut total = 0;
    for entry in walk.flatten().take(2001) {
        if entry.depth() == 0 {
            continue;
        }
        total += 1;
        children
            .entry(entry.path().parent().unwrap_or(dir).into())
            .or_default()
            .push((entry.path().into(), entry.file_type().is_dir()));
    }
    fn draw(
        root: &Path,
        prefix: &str,
        map: &BTreeMap<PathBuf, Vec<(PathBuf, bool)>>,
        out: &mut String,
    ) {
        if let Some(items) = map.get(root) {
            let mut items = items.clone();
            items.sort_by_key(|(p, d)| (!*d, p.file_name().map(|s| s.to_os_string())));
            for (i, (p, d)) in items.iter().enumerate() {
                let last = i + 1 == items.len();
                let name = p
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .chars()
                    .map(|c| {
                        if c.is_control() {
                            c.escape_default().to_string()
                        } else {
                            c.to_string()
                        }
                    })
                    .collect::<String>();
                out.push_str(&format!(
                    "{prefix}{} {name}{}\n",
                    if last { "└──" } else { "├──" },
                    if *d { "/" } else { "" }
                ));
                if *d {
                    draw(
                        p,
                        &format!("{prefix}{}", if last { "    " } else { "│   " }),
                        map,
                        out,
                    );
                }
            }
        }
    }
    let heading = dir
        .to_string_lossy()
        .chars()
        .map(|c| {
            if c.is_control() {
                c.escape_default().to_string()
            } else {
                c.to_string()
            }
        })
        .collect::<String>();
    let mut out = format!("{heading}\n");
    draw(dir, "", &children, &mut out);
    if total >= 2000 {
        out.push_str("… listing limited to 2000 entries\n");
    }
    out
}
pub fn render(a: &Args) -> Result<()> {
    let dir = a.directory.as_ref().ok_or("directory required")?;
    let w = a.window.as_deref().ok_or("window required")?;
    let own = std::env::var("TMUX_PANE").map_err(|e| e.to_string())?;
    let stop = Arc::new(AtomicBool::new(false));
    for sig in [libc::SIGTERM, libc::SIGHUP, libc::SIGINT] {
        signal_hook::flag::register(sig, stop.clone()).map_err(|e| e.to_string())?;
    }
    let show_hidden = option(a, "@tmux-actions-sidebar-hidden")? == "on";
    let cmd = crate::open::spec(
        &option(a, "@tmux-actions-pager")?,
        vec![
            "less".into(),
            "-R".into(),
            "-S".into(),
            "-~".into(),
            "-i".into(),
        ],
    )?;
    let mut child = Command::new(&cmd[0])
        .args(&cmd[1..])
        .env_remove("LESS")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Some(mut input) = child.stdin.take() {
        let _ = input.write_all(tree(dir, show_hidden).as_bytes());
    }
    loop {
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            break;
        }
        if stop.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            break;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    if !stop.load(Ordering::Relaxed)
        && let Ok(_lock) = window_lock(a, w)
    {
        let _ = cache_width(a, &own, &dir.to_string_lossy());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let args = [
        exe.to_string_lossy().into_owned(),
        "cleanup".into(),
        "--socket".into(),
        a.socket.to_string_lossy().into_owned(),
        "--window".into(),
        w.into(),
        "--pane".into(),
        own,
    ];
    let cmd = format!(
        "exec {}",
        args.iter().map(|s| quote(s)).collect::<Vec<_>>().join(" ")
    )
    .replace('#', "##");
    let _ = t(a, &["run-shell", "-b", "-d", "0.1", &cmd]);
    Ok(())
}
