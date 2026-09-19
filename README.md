# KeyPrint

Connects to a VIA/Vial keyboard over USB and prints every layer of its
current keymap to a PDF — one page (or several, stacked) per layer, laid out
in the keyboard's real physical shape (encoders included, with a rotation
icon per direction), with keycap labels translated into whatever keyboard
layout is *active on your OS* right now (German, French, AZERTY, ... — read
live, no language file needed). Pages are always a real A4 sheet, scaled to
fit — never a custom oddly-sized page.

Preview GUI:

<img width="526" height="304" alt="grafik" src="https://github.com/user-attachments/assets/6f26f284-b491-4007-a413-a1cbfe0d6a7a" />


---
<details>
<summary>Preview Example PDF:</summary>
<img width="695" height="981" alt="grafik" src="https://github.com/user-attachments/assets/c96a05fd-23e7-4d84-9353-e1597b242ae6" />
</details>


## Credits

Built on two projects by [Stephan Rumswinkel (srwi)](https://github.com/srwi):

- **[qmk-via-api](https://github.com/srwi/qmk-via-api)** — used as a normal
  Cargo dependency for the VIA HID protocol (device scanning, raw HID
  read/write, keycode constants).
- **[keypeek](https://github.com/srwi/keypeek)** — several modules were
  **copied and adapted** from here (not just referenced): the Vial protocol
  handshake, the KLE/physical-layout parser, the large QMK keycode-to-label
  tables, and `os_layout` (reads the OS's active keyboard layout for label
  translation). keypeek itself is untouched by this project; the code lives
  here as its own copy.

Because of that direct code reuse, this project is licensed **GPL-3.0**,
same as keypeek and qmk-via-api — see [LICENSE](LICENSE).

## What you need

- **Rust** (`cargo`) to build it — [rustup.rs](https://rustup.rs)
- A **C toolchain**, needed by the `hidapi` dependency:
  - Linux: usually already present (`gcc`/`clang`)
  - Windows: comes with the MSVC Build Tools, which `rustup`'s default
    Windows install pulls in automatically
- **Linux only:** read/write permission on `/dev/hidraw*` for your VIA/Vial
  keyboard. If Vial's own desktop app or QMK Toolbox already works for you,
  you have this. Otherwise you need a udev rule granting your user access
  (search "qmk udev rules" — most keyboard projects ship one).
- Nothing else — the font is compiled into the binary, no system font or
  extra install required, on either OS.

## Build

```bash
git clone https://github.com/kolbenhans/keyprint
cd keyprint
cargo build --release
```

Produces two binaries — `keyprint` (CLI) and `gui` (GUI) — under
`target/release/` (Linux) or `target\release\` (Windows, `.exe`).

## GUI

Downloaded a [release](https://github.com/kolbenhans/keyprint/releases)
instead of building? Run **`gui`** (Linux) / **`gui.exe`** (Windows) —
`keyprint`/`.exe` is the CLI, not the GUI. Either binary is fully standalone:
extract just that one file and run it, nothing else from the zip/tarball is
required.

```bash
./target/release/gui          # Linux
.\target\release\gui.exe      # Windows
```

Pick a keyboard, portrait/landscape, layers per page, and where to save —
then hit **Export PDF**. Same options as the CLI flags below, no terminal
required. A live preview on the right shows page 1's layout (box shapes
only, no labels — too small to read at that scale) and updates as you change
any option. Run it on the machine your keyboard is actually plugged into,
under the keyboard layout you normally type with — that's the layout the
labels come from (see [How labels are translated](#how-labels-are-translated)).

On Windows, `gui.exe` is the only file in the release — no console window,
nothing else to extract.

## CLI

Scriptable, and useful for batch-exporting every connected keyboard at
once (the GUI exports one device at a time).

```bash
# Linux
./target/release/keyprint

# Windows (PowerShell / cmd)
.\target\release\keyprint.exe
```

With no arguments it lists every connected VIA/Vial keyboard and writes a
PDF for each (`<Product Name>.pdf`, in whichever directory you ran it from).
Pass a number to target just one:

```bash
./target/release/keyprint 0
```

### Options

| Flag | Default | What it does |
|---|---|---|
| `--portrait` | off | Prints on a portrait A4 page instead of landscape. The keyboard layout itself always stays upright/horizontal — only the page's orientation changes, and the layout is scaled to fit it. |
| `--center` | off | Centers the content vertically on the page instead of aligning it to the top. |
| `--layers-per-page=<N>` | `1` | Stacks N layers onto each page instead of one page per layer. |
| `<device index>` | all devices | Only export the Nth device from the device list. |

Example: `--portrait --layers-per-page=2 0`

## How labels are translated

Keycap labels come from `os_layout` (copied from keypeek): it asks the OS
what character its **currently active keyboard layout** produces for each
key, live, at export time — no language file, no `--lang` flag, no rebuild
to add a layout. Run keyprint on the machine the keyboard is plugged into,
with the layout you normally type under active, and the PDF matches what
the keys actually send.

Caveats inherited from keypeek's `os_layout`:

- **Linux**: reads the Wayland compositor's keymap when `WAYLAND_DISPLAY` is
  set, X11's active RMLVO config otherwise. A key with no OS mapping (rare —
  media/system keys mostly) falls back to the built-in US label.
- **Windows**: reads the foreground thread's layout (`GetKeyboardLayout`),
  so it follows a layout you've set per-app.
- **macOS**: the layout is snapshotted once at startup (`os_layout::init()`,
  called before the GUI/CLI does anything else) — a layout switch mid-run
  needs a restart to pick up.
