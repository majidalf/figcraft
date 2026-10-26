# Attribution

Every asset bundled in this repository (images, icons, fonts, data files) is listed here with its
author, source and licence. `cargo xtask assets` fails if a file under `assets/`, `docs/brand/` or
`docs/images/` is missing from this list.

FigCraft's user-interface icons are drawn in code (`crates/ui-egui/src/icons.rs`): original work,
MIT OR Apache-2.0. The icon libraries offered to designs (Assets ▸ Icons, `icon.insert`) are three
open-source icon sets, listed below with their licences; they are content for documents, never the
app's own interface.

## Assets in this repository

| Path | Author | Source | Licence | Notes |
|---|---|---|---|---|
| `assets/app-icon/` (all files: `figcraft.svg`, `figcraft-small.svg`, `figcraft-*.png`, `figcraft.ico`, `hicolor/`) | The project owner (engraving-style drawing, vectorised) | Original work, see [`assets/app-icon/README.md`](assets/app-icon/README.md); derived files rendered by `cargo xtask icon` | MIT OR Apache-2.0, [`assets/app-icon/LICENSE.txt`](assets/app-icon/LICENSE.txt) | FigCraft app icon (chameleon) |
| `docs/brand/artcraft-logo.svg`, `docs/brand/artcraft-logo.png`, `docs/brand/artcraft-logo-white.svg`, `docs/brand/artcraft-logo-white.png`, `docs/brand/artcraft-mark.svg`, `docs/brand/artcraft-mark.png`, `docs/brand/artcraft-mark-black.svg`, `docs/brand/artcraft-mark-black.png` | ArtCraft Team | storytold/craftrules `assets/brand/` | ArtCraft brand terms (`docs/brand/LICENSE-brand.txt`), not open source | Trademarks; unmodified use within FigCraft only |
| `docs/images/` (all screenshots and exports: `shot-*.png`, `art-*.png`) | FigCraft contributors | Screenshots of FigCraft showing synthetic demo documents made in FigCraft, and PNG exports of those documents by FigCraft's renderer | MIT OR Apache-2.0 | No third-party artwork |
| `assets/icon-sets/lucide.txt.gz` | Lucide Contributors (and Cole Bemis for the Feather icons it grew from) | npm `lucide-static@1.55.0`, https://lucide.dev, https://github.com/lucide-icons/lucide; packed by `cargo xtask icon-sets` (pinned sha512) | ISC (Feather portions MIT), [`assets/icon-sets/LICENSE-lucide.txt`](assets/icon-sets/LICENSE-lucide.txt) | 2,141 icons: SVG bodies and tags, unmodified apart from whitespace |
| `assets/icon-sets/phosphor.txt.gz` | Phosphor Icons (Tobias Fried, Helena Zhang) | npm `@phosphor-icons/core@2.1.1`, https://phosphoricons.com, https://github.com/phosphor-icons/core; packed by `cargo xtask icon-sets` (pinned sha512) | MIT, [`assets/icon-sets/LICENSE-phosphor.txt`](assets/icon-sets/LICENSE-phosphor.txt) | 1,512 icons × 6 weights: SVG bodies and tags, unmodified apart from whitespace |
| `assets/icon-sets/material.txt.gz` | Google (Material Symbols); npm packaging by Ravindra Marella | npm `@material-symbols/svg-400@0.48.0`, https://fonts.google.com/icons, https://github.com/google/material-design-icons, https://github.com/marella/material-symbols; packed by `cargo xtask icon-sets` (pinned sha512) | Apache-2.0, [`assets/icon-sets/LICENSE-material.txt`](assets/icon-sets/LICENSE-material.txt) | 3,929 icons at weight 400: outlined, rounded and sharp, each plain and filled; SVG bodies unmodified apart from whitespace |
| `examples/*.figcraft` | FigCraft contributors | Synthetic demo documents built through FigCraft's command API; invented copy, shapes and vectors drawn from scratch, no images or embedded fonts | MIT OR Apache-2.0 | Text uses Inter (OFL, from craft-fonts or the system); no third-party artwork |

## Fonts (build input, not in this repository)

Release builds embed fonts from the optional craft-fonts build input
([storytold/craft-fonts](https://github.com/storytold/craft-fonts), `CRAFT_FONTS_DIR`). Its own
`ATTRIBUTION.md` and licence files are authoritative; the packages ship each font's licence.

| Family | Licence | Upstream |
|---|---|---|
| Inter | SIL Open Font License 1.1 | https://github.com/rsms/inter |
| Archivo | SIL Open Font License 1.1 | [Google Fonts / Archivo](https://github.com/google/fonts/tree/6c70c829f09ea345d3590406693220ea35c6553f/ofl/archivo) |
| Roboto, Montserrat, Lato, Open Sans, Poppins, Nunito Sans, Merriweather, Playfair Display, Carlito, Caladea | SIL Open Font License 1.1 | Google Fonts / upstream repositories (see craft-fonts) |
| Liberation Sans, Serif, Mono | SIL Open Font License 1.1 | https://github.com/liberationfonts |
| BIZ UDPGothic, BIZ UDMincho, Shippori Mincho, Noto Sans Arabic, Noto Sans Georgian | SIL Open Font License 1.1 | Google Fonts (see craft-fonts) |

The central catalog also contains Adobe-authored Source Sans 3 and Source Serif 4.
FigCraft filters both families from every build in accordance with its asset policy.

egui's default fonts (compiled into the `egui`/`epaint` crates) keep their own licences (OFL / Apache-2.0 / Ubuntu Font Licence); see those crates.

## egui dependency font icons

| Asset | Author | Source | Licence | Use |
|---|---|---|---|---|
| `epaint_default_fonts::EGUI_ICONS`, `epaint_default_fonts::EMOJI_ICON` (dependency font inputs) | John Slegers | [egui 6d518fc7](https://github.com/storytold/egui/tree/6d518fc799681571963842f95b55a6b68431b15a/crates/epaint_default_fonts/fonts), [emoji-icon-font](https://github.com/jslegers/emoji-icon-font) | MIT (Copyright (c) 2014 John Slegers; full upstream text in `NOTICE`) | egui-icons.ttf is the five-glyph subset, included by default_fonts independently of monochrome_emoji_fonts; emoji-icon-font.ttf is the full monochrome fallback. Dependency assets unmodified here; no font file copied into this repository. |
