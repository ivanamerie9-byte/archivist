# Archivist

A Windows TUI tool that scans a media library for movies and TV series that are missing cover art (`folder.jpg`) or the Windows folder-icon plumbing (`desktop.ini` + `.ico`), looks them up on TMDB, and fixes them in batch.

It is a Rust rewrite of the single-title HTML helper that previously generated a Python script per title. Now you point the binary at your library, see every problem in one tree, tick the ones you want, and pick the right TMDB candidate when the match is ambiguous.

## Features

- Scans both movie and series libraries in parallel
- Reports every missing artifact per title and per season (`folder.jpg`, `desktop.ini`, `.ico`, `+S` attribute on the folder)
- Wraps orphan video files (`Movie.2024.2160p.mkv` lying loose in the library root) into proper `Title (Year)\` folders without modifying their contents
- Wraps non-canonical scene-release folders into a canonical `Title (Year)\` parent without modifying anything inside them &mdash; nothing in your BD/Remux/episode/subtitle layout is touched
- Auto-matches unambiguous TMDB hits, prompts you to pick from 2&ndash;4 candidates otherwise
- Generates `folder.jpg` (512px wide), a multi-size `.ico` (256/48/32/16) and a UTF-16 LE BOM `desktop.ini`, then sets `+S` on the directory and `+H +S` on the ini &mdash; identical pipeline to the proven HTML/Python original
- Idempotent: re-runs skip work that is already present (SHA-256 check on the poster JPG)
- `--dry-run` flag and an in-app preview of the planned actions

## Install / Build

Requires Rust 1.80+ on Windows.

```powershell
git clone https://github.com/ivanamerie9-byte/archivist.git
cd archivist
cargo build --release
copy config.example.toml config.toml
# edit config.toml: set tmdb_api_key and library paths
.\target\release\archivist.exe
```

The release binary is a single self-contained `.exe` (rustls TLS, no OpenSSL) &mdash; copy it anywhere alongside its `config.toml`.

## Get a TMDB API key (free, ~3 minutes)

1. Sign up at <https://www.themoviedb.org/signup>
2. Verify your email
3. Open <https://www.themoviedb.org/settings/api>
4. Click **Create** &rarr; **Developer**
5. Fill the form &mdash; any non-commercial use is fine
6. Copy the **API Key (v3 auth)** value into `config.toml` or paste it in the app's Settings screen

## Usage

```powershell
archivist.exe                    # scan both libraries (default)
archivist.exe --lib movies       # only Cinema Theatre
archivist.exe --lib series       # only Origial Series
archivist.exe --dry-run          # plan actions without touching the FS
archivist.exe --config path.toml # alternate config file
```

### Key bindings

| Key | Action |
|-----|--------|
| `q` / `Esc` | Back / quit |
| `?` | Help overlay |
| `k` | Settings (API key, library paths) |
| `Space` | Toggle checkbox in Issue list |
| `a` | Toggle all checkboxes |
| `&larr;` / `&rarr;` | Collapse / expand tree node |
| `o` | Open selected poster in default viewer |
| `Enter` | Apply selected (Issue list) / Accept (Modal) |
| `s` | Skip current candidate |
| `r` | Retry search with edited query |

## Interface

Seven screens, all in the terminal. Navigation flow:

```
            ┌──────────┐
            │ Settings │◀──── press [k] from anywhere
            └────┬─────┘
                 │ Esc
                 ▼
   LibrarySelect ─▶ Scanning ─▶ IssueList ⇄ CandidateModal
                                     │
                                     ▼
                                  Applying ─▶ Report
```

### Settings
On first launch (or whenever the TMDB key fails) you land here. Right column is a built-in guide for getting a free TMDB key in ~3 minutes.

```
 Settings    Configure TMDB API key and library paths

┌ Fields ──────────────────────────┐ ┌ How to get a TMDB API key ───────┐
│                                  │ │ 1. Sign up at themoviedb.org     │
│ ▸ TMDB API Key                   │ │ 2. Verify your email             │
│   ************************       │ │ 3. Open Settings → API           │
│                                  │ │ 4. Click "Create" → "Developer"  │
│   Movies root                    │ │ 5. Fill the form (non-commercial)│
│   V:\library\Cinema Theatre      │ │ 6. Copy "API Key (v3 auth)" into │
│                                  │ │    the field on the left         │
│   Series root                    │ │                                  │
│   V:\library\Origial Series      │ │ It's free and takes ~3 minutes.  │
│                                  │ │                                  │
│ ✓ TMDB connection OK             │ │                                  │
└──────────────────────────────────┘ └──────────────────────────────────┘

[Tab] next field  [type] edit  [Ctrl+M] show/hide key  [Ctrl+T] test
[Ctrl+S] save  [Esc] back to Library Select
```

### Library Select

```
 Choose library to scan

┌ Library ─────────────────────────────────────────────────────────────┐
│   [M]ovies                                                           │
│   [S]eries                                                           │
│ ▸ [B]oth                                                             │
└──────────────────────────────────────────────────────────────────────┘

[↑↓] select  [Enter] scan  [k] settings  [q] quit
```

### Scanning

Progress through the file system. Live tail of folders being processed.

```
 Scanning libraries    [Series] The Walking Dead (2010)

┌ Progress ────────────────────────────────────────────────────────────┐
│ ███████████████████████████░░░░░░░░░░░░░░░░  76/118                  │
└──────────────────────────────────────────────────────────────────────┘

┌ Log ─────────────────────────────────────────────────────────────────┐
│ [Movies] Apocalypse Now (Redux, 1979)                                │
│ [Movies] Blade Runner (1982)                                         │
│ [Movies] Dune (2021)                                                 │
│ [Series] Arcane (2021)                                               │
│ [Series] DARK.S01.2160p.NF.WEBRip.DDP5.1.x264-NTb.TeamHD             │
│ [Series] The Walking Dead (2010)                                     │
└──────────────────────────────────────────────────────────────────────┘

[Esc] cancel
```

### Issue List

The heart of the app. Every problem the scanner found, grouped by library, with checkboxes. Match status appears next to each entry after you press Enter.

```
 Issues found                15/22 selected · row 7/22

┌ Items ───────────────────────────────────────────────────────────────┐
│   [x] [Movies] Blade Runner (1982)                                   │
│   [x] [Movies] Vampire.Hunter.D.Bloodlust.2001.BDRemux.1080p         │
│   [ ] [Movies] (orphan) Mickey.17.2025.2160p.UHD.mkv                 │
│   [x] [Movies] Solyaris 1972 1080p FRA Blu-ray  → Solaris (1972)     │
│   [x] [Series] Trinity Blood [2005]              → Trinity Blood (2005) │
│ ▸ [x] [Series] DARK.S01.2160p.NF.WEBRip          ? 3 candidates      │
│   [x] [Series] Arcane (2021) · Season 2                              │
│   [x] [Series] Person.of.Interest.S01.BDRip      → Person of Interest│
│   [x] [Series] Чернобыль.S01.WEB-DL.2160p        → Chernobyl (2019)  │
│   ...                                                                │
└──────────────────────────────────────────────────────────────────────┘

[↑↓] move  [PgUp/PgDn] page  [Home/End] jump  [Space] toggle  [a] all
[Enter] apply  [k] settings  [Esc] back
```

### Candidate Modal

When TMDB returns multiple plausible matches you choose by hand.

```
 Pick the right title    (3 more after this)

┌ Folder ──────────────────────────────────────────────────────────────┐
│ [Series] DARK.S01.2160p.NF.WEBRip                                    │
└──────────────────────────────────────────────────────────────────────┘

┌ Candidates ──────────────────────────────────────────────────────────┐
│ ▸ Dark (2017)                                       id:70523         │
│       In the present, a child's disappearance exposes the double…    │
│                                                                      │
│   Dark Matter (2024)                                id:84958         │
│       For physicist Jason Dessen, an inexplicable encounter sends…   │
│                                                                      │
│   Dark Angel (2000)                                 id:4624          │
│       In a post-apocalyptic America, Max is a genetically enhanced…  │
└──────────────────────────────────────────────────────────────────────┘

[↑↓] select  [Enter] accept  [s] skip  [r] retry  [Esc] skip all
```

### Applying

Pipeline runs: phase 1 (folder renames / moves) sequential, phase 2 (download poster, write folder.jpg + .ico + desktop.ini) parallel.

```
 Applying    Cover for The Walking Dead (2010) - Season 9

┌ Progress ────────────────────────────────────────────────────────────┐
│ ████████████████████████████████░░░░░░░░░░  18/22                    │
└──────────────────────────────────────────────────────────────────────┘

┌ Log ─────────────────────────────────────────────────────────────────┐
│ Rename: Solyaris 1972 1080p FRA → Solaris (1972)                     │
│ Wrap orphan: Johann Johannsson - Last and First Men (2020).mkv       │
│ Adopt: DARK.S01.2160p → Dark (2017)\Season 1                         │
│ Cover for Solaris (1972)                                             │
│ Cover for Trinity Blood (2005)                                       │
│ Cover for Dark (2017)                                                │
└──────────────────────────────────────────────────────────────────────┘

[Esc] cancel
```

### Report

```
 Report    18 succeeded · 2 failed · 2 skipped

┌ Details ─────────────────────────────────────────────────────────────┐
│ ✓ Rename: Solyaris 1972 1080p FRA → Solaris (1972)                   │
│ ✓ Cover for Solaris (1972)                                           │
│ ✓ Cover for Trinity Blood (2005)                                     │
│ ✓ Wrap orphan: Johann Johannsson - Last and First Men (2020).mkv     │
│ ✓ Adopt: DARK.S01.2160p → Dark (2017)\Season 1                       │
│ ✗ Wrap orphan: Predator.Badlands.2025.BDRemux.1080p.pk.mkv           │
│       source file is locked or in use (active download / open in…)   │
│ ✗ Wrap orphan: Дюна.2021.Hybrid.UHD.Blu-Ray.Remux.2160p.mkv          │
│       target directory already contains another video — resolve…     │
│ · Cover for Predator- Badlands (2025) (target folder was not created)│
└──────────────────────────────────────────────────────────────────────┘

[Enter] back to library select  [q] quit
```

## Library layout

```
V:\library\Cinema Theatre\
├── _covers\                              # auto-managed
│   ├── 12 Angry Men (1957).jpg
│   └── 12 Angry Men (1957).ico
└── 12 Angry Men (1957)\
    ├── folder.jpg                        # 512px poster
    ├── desktop.ini                       # UTF-16 LE BOM, hidden+system
    └── 12_Angry_Men.avi                  # your video, untouched

V:\library\Origial Series\
├── _covers\
│   ├── The Walking Dead (2010).ico
│   └── The Walking Dead (2010) - Season 1.ico
└── The Walking Dead (2010)\
    ├── folder.jpg
    ├── desktop.ini
    └── Season 1\
        ├── folder.jpg                    # season poster
        ├── desktop.ini
        └── s01e01_Days.Gone.Bye.avi      # your videos, untouched
```

## License

[MIT](LICENSE) &copy; 2026 Ivan Amerie
