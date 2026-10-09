# tmux-actions

Native tmux regex search plus Rust file/URL opening and a directory sidebar on
Linux/macOS. No background daemon. tmux handles search, highlighting, next and
previous matches, wrapped lines and copying the match at the cursor.

```tmux
set -g @plugin 'lidongpeng36/tmux-actions'
```

Replace tmux-copycat, tmux-open and tmux-sidebar with this plugin. TPM installs
it; the entry point automatically downloads a pinned prebuilt binary and
verifies SHA-256. Cached reloads work offline. Failed first installations retry
on client attach. Requires tmux **3.6+** and `less` for the sidebar; macOS already
includes less, and typical Linux installations provide it. No Rust compiler or
`tree`/`gawk` is required.

## Keys

| Context | Key | Action |
| --- | --- | --- |
| Prefix | `/` | Native regex search prompt |
| Prefix | `C-u` | URLs |
| Prefix | `C-f` | Paths |
| Prefix | `C-d` | Numbers |
| Prefix | `M-h` | Hex hashes |
| Prefix | `M-i` | IPv4 addresses |
| Prefix | `C-g` | Changed Git filenames, with literal regex escaping |
| Copy mode | `n` / `N` | Next / previous match |
| vi copy mode | `Enter` | Copy selection or current match and exit |
| Copy mode | `o` | Open selected file or URL |
| Copy mode | `C-o` | Open selected file in an editor window |
| Copy mode | `S` | Web search for selected text |
| Prefix | `Tab` | Toggle sidebar without moving focus |
| Prefix | `Bspace` | Toggle sidebar and focus it |
| Sidebar | `q` | Quit pager, close sidebar and restore layout |

Existing copy-pipe/yank bindings remain configurable in tmux. Search results can
be copied without beginning a separate selection. Older copycat cancellation
wrappers are cleaned up when detected; user copy bindings are kept.

Opening uses argument vectors, not shell evaluation of selected text. Editors
open in a new window; `file:line[:column]` supports the line number (column is
currently ignored). Files with spaces, quotes, Unicode, `$()` and tmux format
syntax are passed literally. Web query text is encoded. GUI opening uses macOS
`open` or Linux `xdg-open`; a headless SSH session needs an explicitly configured
opener to route URLs elsewhere. It does not automatically launch a browser on
the SSH client.

## Optional overrides

Defaults require no appearance or command configuration. Command overrides
accept JSON argument arrays or shell-style **argument quoting**; pipelines,
redirects, variable substitution and selected-text shell evaluation are absent.

```tmux
set -g @tmux-actions-editor '["nvim"]'
set -g @tmux-actions-opener '["/path/to/opener"]'
set -g @tmux-actions-search-url 'https://www.google.com/search?q='
set -g @tmux-actions-sidebar-width 40
set -g @tmux-actions-sidebar-side left
set -g @tmux-actions-sidebar-hidden off
set -g @tmux-actions-search-patterns '{"C-u":"https?://[^[:space:]]+"}'
```

Legacy `@open-S`, `@sidebar-tree-width` and `@sidebar-tree-position` are read when
new options are absent. Keys can be rebound with normal tmux `bind-key` commands
after plugin loading. Custom sidebar commands are replaced by a built-in bounded
listing; an optional `@tmux-actions-pager` command can replace less.

## Sidebar behavior

One sidebar per window. Its initial width is 40 columns, capped at half the
source pane width; panes narrower than 71 columns are refused. Manual resizing
is remembered per directory, including when quitting the pager. Widths survive
tmux restarts in an atomic cache under XDG_CACHE_HOME or the platform's user
cache directory; up to 128 directory preferences are retained. Set
`@tmux-actions-width-cache` to override the cache file.

Opening does not change focus unless requested. Closing restores the recorded
layout if the original pane set still exists. User-added or removed panes are
preserved instead of applying an obsolete layout. Renderer/pager processes exist
only while the sidebar is open and terminate when its pane/server exits.

The listing uses walkdir, does not follow symlinks, caps depth at six and entries
at 2000, and skips `.git`, `node_modules` and `target`. Other dotfiles are hidden by default; opt in with
`@tmux-actions-sidebar-hidden on`. Git ignore rules are not applied. Control characters in names are escaped.

## Development

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked --release
python3 tests/installer.py
python3 tests/cold_install_lifecycle.py
python3 tests/actions.py
python3 tests/keys.py
python3 tests/sidebar.py
```

Real tmux/PTY tests cover native matching, wrapped Unicode URLs, mock opener
argument integrity, query encoding, editor line numbers, layout/focus/width
memory, real less interaction, and active pager cleanup. Test caches/snapshots
use temporary directories; mock openers do not launch browsers or real editors.

MIT licensed. Reuses clap, url, shlex, walkdir, serde and signal-hook.
