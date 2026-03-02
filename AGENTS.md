# Repository Guidelines

## Project Structure & Module Organization
- `src/`: React + Tailwind UI (shadcn/ui components in `src/components/ui/`).
- `src-tauri/`: Rust backend (scanning, table import, dedupe, logging) and Tauri config.
- `src-tauri/src/scan`: filesystem walk + BMS解析 + MD5計算。
- `src-tauri/src/tables`: 難易度表 fetch/classify/parse/upsert。
- `src-tauri/src/dedupe`: 重複検出と安全削除（trash）。
- Assets/Icons: `public/`, `src-tauri/icons/`.

## Build, Test, and Development Commands
- `bun install` — install frontend + tooling deps.
- `bun run dev` — Vite dev server。
- `bun run tauri dev` — Tauri開発モード（frontend+backend）。
- `bun run build:frontend` / `bun run build:backend` / `bun run build:all` — 個別/一括ビルド。
- `bun run build:tauri` — 製品ビルド。
- `bun run check:all` — TS format+lint, Rust fmt check, clippy（作業終了前に必須）。
- `bun run fix:all` — TS整形+lint修正, Rust fmt + clippy。

## Coding Style & Naming Conventions
- Format: `oxfmt` for TS/JSON/Tailwind, `cargo fmt` for Rust。
- Lint: `oxlint --react-plugin`, `cargo clippy -D warnings`。
- Naming: snake_case for Rust, camelCase for TS。Conventional Commits（日本語可, 例: `feat(ui): ...`）。
- Avoid glob path handling in scan（仕様上禁止）。

## Testing Guidelines
- 現状自動テスト未整備。手動検証: `bun run check:all` + `bun run tauri dev` で主要フロー確認。
- 追加テストは `src-tauri` に統合予定。テスト作成時は `cargo test` ベースを想定。

## Commit & Pull Request Guidelines
- コミット: Conventional Commits。例: `fix(scan): ...`, `chore(tooling): ...`。
- PR: 目的、変更概要、動作確認手順、スクリーンショット（UI変更時）を記載。`bun run check:all` 結果を添付。

## Architecture Notes
- データベース: SQLite (WAL) + FTS5。スキーマは `src-tauri/src/db/mod.rs` 参照。
- ログ: JSON Lines（イベントごとに `events.jsonl` へ）。
- 進捗通知: スキャンは Tauri イベント `scan_progress` を emit、UIステータスバーで表示。

## Security & Configuration Tips
- 削除は `trash` 経由でゴミ箱移動。root跨ぎ削除はデフォルト不可。
- ネットワーク: 表取得は `reqwest`（Accept: json, リダイレクト追従）。
- 実行前に `tauri.conf.json` のバンドル設定を確認（署名/アイコン等）。
