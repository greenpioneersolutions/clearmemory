# assets/ — Brand Assets & ClearPathAI Integration Files

This directory contains brand assets (icons, logos, SVGs) for Clear Memory and integration files consumed by the [ClearPathAI](https://github.com/clearpathai/clearpathai) Electron desktop app. These files are **not part of the Rust build** — they exist here so that the Clear Memory repository is the single source of truth for its visual identity and ClearPathAI integration snippets.

---

## Directory Structure

```
assets/
├── icons/                       # App icons at standard sizes for packaging and favicons
│   ├── favicon.ico              # Browser tab icon (ICO format)
│   ├── icon-16.png              # 16x16 — favicon fallback
│   ├── icon-32.png              # 32x32 — favicon standard
│   ├── icon-64.png              # 64x64 — small UI contexts
│   ├── icon-128.png             # 128x128 — macOS dock (low-res)
│   ├── icon-256.png             # 256x256 — Windows app icon
│   ├── icon-512.png             # 512x512 — macOS dock (Retina)
│   └── icon-512.svg             # Vector source — scale to any size
├── logos/
│   ├── github-banner.svg        # README/GitHub social preview banner
│   ├── logo-full.svg            # Full logo with data stack + wordmark (marketing materials)
│   ├── logo-navbar.svg          # Horizontal icon + wordmark (app header bar)
│   ├── logo-footer.svg          # Small icon + wordmark + tagline (app footer)
│   └── logo-wordmark.svg        # Text-only wordmark (minimal contexts)
├── integration/                 # ClearPathAI Electron app integration files
│   ├── LogoComponents.jsx       # React components (NavbarLogo, FooterLogo) for the Electron GUI
│   ├── brand-tokens.js          # Brand color palette, font stack, and tagline constants
│   ├── electron-icon-setup.js   # Electron BrowserWindow icon configuration (macOS/Windows)
│   └── head-snippet.html        # HTML <head> meta tags, favicon links, and SEO metadata
└── README.md                    # This file
```

---

## Brand Colors (shared across the Clear product family)

| Role | Color | Hex |
|------|-------|-----|
| Primary (backgrounds, buttons) | Purple | `#5B4FC4` |
| "Memory" text accent | Light purple | `#7F77DD` |
| "AI" text / interactive accent | Teal | `#1D9E75` |
| Traversal lines / beacon glow | Light teal | `#5DCAA5` |
| Data blocks / structural elements | Neural blue | `#85B7EB` |

---

## Usage

**For the Rust binary:** These assets are not compiled into the binary. The Rust build ignores this directory entirely.

**For ClearPathAI (Electron):** Import the integration files directly:
```js
import { NavbarLogo, FooterLogo } from 'assets/integration/LogoComponents';
import { BRAND } from 'assets/integration/brand-tokens';
```

**For GitHub/documentation:** The SVG logos and banner are referenced from README.md and documentation files.

---

## Part of the Clear Family

| Product | Purpose |
|---------|---------|
| **Clear Memory** | Memory engine — store, retrieve, and inject AI conversation context |
| **ClearPathAI** | Desktop app — wraps Copilot CLI and Claude Code CLI with GUI and memory |
