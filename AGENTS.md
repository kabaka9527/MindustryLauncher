# MindustryLauncher — Agent Notes

Portable Tauri 2 desktop launcher for Mindustry/MindustryX. React 19 + TS + Vite front-end, Rust back-end in `src-tauri/`.

## Env

Source `scripts/dev-env.ps1` before any pnpm/cargo command — pins caches under repo.

## Commands

Prefix every command with `. .\scripts\dev-env.ps1;`

| Task | Command |
|------|---------|
| Install deps | `pnpm install` |
| Dev | `pnpm tauri dev` |
| Dev (web) | `pnpm dev:web` |
| Check | `pnpm check` |
| Build | `pnpm build` |
| Build web | `pnpm build:web` |
| Rust tests | `cargo test --manifest-path src-tauri\Cargo.toml` |

Build → `mindustry-launcher.exe` (`tauri build --no-bundle`, no installer).

## Quick ref

- Models: `models.rs` ↔ `types.ts` — keep in sync
- Commands: `lib.rs` invoke_handler + `api.ts` invoke/Channel
- Errors: `AppResult`/`AppError`, serialized as string
- Network: `NetworkClient` (proxy, timeout, retry, etag)
- Downloads: static HashMap, pause/resume/cancel via task ID, Channel\<TaskEvent\>
- Theme: `useTheme.ts`, localStorage `mindustry-launcher-theme`

## Rules

- TS: no semicolons, strict mode. Rust: 2021, thiserror, serde camelCase.
- UI strings in Chinese. Env: only `VITE_*` / `TAURI_*` exposed.

## Gotchas

- `debugMode: true` → debug-log window + `logs/debug.log`
- Channel/toggle filter: single-select + `showBe` flag — tread carefully
- Release: `.github/workflows/release.yml`, 3 version files must match

<!-- ponytail: full — merged layout/arch/env sections, dropped per-module file lists (agents can look), trimmed arch descriptions to one-liners, removed release detail (only needed for releases) -->
