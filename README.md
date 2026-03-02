# bms-score-manager

ローカルBMS譜面の管理、難易度表取り込み、重複整理を行う Tauri アプリです。  
フロントエンドは React + Tailwind + shadcn/ui、バックエンドは Rust + SQLite で構成されています。

## 主な機能

- ローカルルート追加と譜面スキャン（leaf package 判定）
- BMSヘッダ解析と `file_md5` 保存
- 難易度表URL登録・取り込み（Pattern A/B/C/D）
- MD5 JOIN による表の所持/未所持判定
- 重複候補検出、プレビュー、バックアップ後のゴミ箱削除
- JSON Lines 形式ログ出力

## 技術スタック

- Frontend: React, TypeScript, Tailwind CSS, shadcn/ui
- Desktop: Tauri v2
- Backend: Rust
- DB: SQLite (WAL) + FTS5
- Lint / Format:
  - TypeScript: `oxlint`, `oxfmt`
  - Rust: `cargo clippy`, `cargo fmt`

## セットアップ

```bash
bun install
```

## 開発コマンド

```bash
bun run dev
```

```bash
bun run tauri dev
```

## ビルド

```bash
bun run build:all
```

```bash
bun run build:tauri
```

## 品質チェック

作業終了前に必ず以下を実行してください。

```bash
bun run check:all
```

自動修正込みで実行する場合:

```bash
bun run fix:all
```

## スクリプト一覧

- `bun run check:all`: TS format check + TS lint + Rust format check + Rust clippy
- `bun run fix:all`: TS format + TS lint fix + Rust format + Rust clippy
- `bun run build`: フロントエンドビルド
- `bun run build:all`: フロントエンド + Rustバックエンドを順にビルド
- `bun run build:tauri`: Tauri製品ビルド（`bun run tauri build`）
- `bun run tauri`: Tauri CLI

## ディレクトリ概要

- `src/`: React UI
- `src/components/ui/`: shadcn/ui コンポーネント
- `src-tauri/src/scan`: ローカル走査
- `src-tauri/src/bms_parse`: BMS最小パースとMD5
- `src-tauri/src/tables`: 難易度表 fetch/classify/parse/upsert
- `src-tauri/src/dedupe`: 重複検出と安全削除
- `src-tauri/src/db`: SQLite 初期化とスキーマ
