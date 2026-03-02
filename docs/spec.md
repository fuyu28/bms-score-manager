# Codex実装用 指示書（Rust / BMSローカル管理 + 難易度表取り込み + 重複整理）

この指示書は、ローカルにある大量のBMSデータを管理するGUIアプリのバックエンド実装（Rust）をCodexに依頼するための仕様まとめ。
**要件・実装方針・DB・取り込みパターン・安全対策**を含む。

---

## 0. 目的（何を完成させるか）

- ローカルのBMS譜面をスキャンしてDB化し、GUIで検索できるようにする
- 難易度表（複数パターン）をURLから取得してDB化し、ローカル譜面と **MD5一致**で所持/未所持を出せるようにする
- 重複候補を検出して、安全に統合/削除できるようにする（削除はWindowsゴミ箱）

---

## 1. 対象データ（ローカル）

### 1.1 Chart（譜面ファイル）拡張子（MUST）

- `.bms .bme .bml .pms`（大小文字無視）

### 1.2 参照資産拡張子（MUST）

- 音声：`.wav .ogg`
- 画像：`.bmp .png .jpg`
- 動画：`.mpg .mp4`（`.MPG` など大文字もあり）

### 1.3 パッケージ（Package）定義（MUST）

- **Chartを含むディレクトリをPackageとする（“葉フォルダ基準”）**
  - あるディレクトリ直下にChartが1つ以上存在 → そのディレクトリがPackage
  - Chartを含まない中間フォルダ/空フォルダはPackageにしない

### 1.4 パス処理（MUST）

- **ワイルドカード/glob解釈を使わず、リテラルパスで走査・操作する**
  - `[]` や記号、日本語が多く、glob解釈で列挙エラー実績あり

---

## 2. 非機能要件（MUST）

### 2.1 性能

- UIブロック禁止：スキャン/解析/表取り込みはバックグラウンド
- DB更新は **バッチINSERT + トランザクション**（1件ずつ禁止）
- 検索は SQLite FTS5 で高速化 + ページング

### 2.2 安全

- 破壊的操作（削除/統合）はプレビュー必須
- 削除は **Windowsのゴミ箱へ移動**をデフォルト
  - 実装例：`trash` crate（OSのRecycle Bin/Trashへ移動）
  - ゴミ箱移動に失敗した場合は **直削除しない**（エラーで中断）
- ルート跨ぎの統合/削除はデフォルト禁止（設定で解除 + 強警告 + 2段階確認）

### 2.3 ログ

- ログは **JSON Lines（1行1イベントJSON）**（MUST）
  - 例：scan_start/scan_done/table_fetch/table_parse/db_commit/dedupe_execute 等
  - 機械集計・デバッグに強い
- 主要項目：timestamp, event, root_id, package_id, table_id, url, final_url, counts, duration_ms, error

---

## 3. 技術スタック（追記：Rust + Tauri）

### 3.1 アプリ全体（MUST）

- **Rust + Tauri（v2推奨）**
  - Rust側：スキャン/解析/DB/難易度表fetch/重複整理を実装
  - フロント側：検索UI・表UI・重複整理UI（Web技術で実装）

### 3.2 永続化/検索（MUST）

- DB：SQLite（WAL有効） + FTS5

### 3.3 難易度表取り込み（推奨）

- bmstable互換をまず対応（`bms-table` crate活用可）
  - ※ただし「拡張ヘッダ」「API/リダイレクト」「スキーマ揺れ」は自前で吸収する

### 3.4 OS統合（推奨）

- Windowsゴミ箱：`trash` crate

### 3.5 通信/解析（推奨）

- HTTP：reqwest等（リダイレクト追従、Accept: application/json）
- HTML解析：scraper等（meta bmstable抽出）
- JSON：serde_json
- SQLite：rusqlite or sqlx（どちらでも可、バッチとトランザクション必須）

---

## 4. DB論理モデル（MUST）

### 4.1 roots

- id, path, enabled, created_at

### 4.2 packages（葉フォルダ）

- id, root_id, path, mtime, total_size, file_count, chart_count, last_scanned_at

### 4.3 charts（譜面）

- id, package_id, rel_path, ext, file_size, mtime
- meta：title, subtitle, artist, subartist, genre, playlevel, bpm, total, player
- refs：wav_count, bmp_count, wav_list_json, bmp_list_json
- hashes：file_md5 (MUST), bms_norm_hash (SHOULD)
- stats：object_stats_json (optional)
- indexes：(package_id, rel_path), (file_md5)

### 4.4 songs / song_links（論理楽曲）

- songs(id, canonical_title, canonical_artist)
- song_links(song_id, chart_id, confidence, user_confirmed)

### 4.5 table_sources（ユーザー登録した表）

- id, input_url, enabled, last_fetch_at, last_success_at, last_error

### 4.6 tables（表の実体：最新取り込み状態）

- id, source_id
- page_url_resolved（HTML取得後の最終URL）, header_url, data_url, data_final_url（リダイレクト後）
- name, symbol, tag(optional), mode(optional), level_order_json(optional), attr_json(optional)
- header_raw, data_raw（任意：容量注意。最低でもhash/抽出後raw_json保持は必須）
- header_hash, data_hash, updated_at

### 4.7 table_entries（譜面カタログ/難易度表エントリ）

- id, table_id
- md5 (TEXT, lowercase, NOT NULL)
- sha256(optional)
- level_text(optional)
- title(optional)
- artist(optional)
- charter(optional)
- url(optional), url_diff(optional)
- comment(optional)
- raw_json (TEXT)  ※必須
- UNIQUE(table_id, md5)

### 4.8 table_groups（コース/段位/グループ共通）

- id, table_id
- group_type: "course" | "grade"
- group_set_index (INT)  ※course外側配列index（ない場合0）
- name, style(optional)
- constraints_json(optional), trophies_json(optional)
- raw_json

### 4.9 table_group_items

- group_id, md5, title_hint(optional)
- index：(group_id, md5)

---

## 5. ローカルスキャン仕様（MUST）

### 5.1 段階A：高速スキャン（構造だけ）

- ルート配下を再帰走査
- 各ディレクトリについて「直下にChartがあるか」を判定し、あるならPackageとして登録
- Chart（パス/サイズ/mtime/ext）を収集して登録
- 空フォルダ・中間フォルダは登録しない
- DB書き込みはトランザクション + バルク

### 5.2 段階B：解析（BMSパース + MD5算出）

- 各Chartを読み、最低限のヘッダメタを抽出
- **file_md5 を算出してDBへ保存**（難易度表MD5照合の要）
- 参照（#WAV/#BMP）を抽出（存在確認はSHOULD）
- 正規化ハッシュ（bms_norm_hash）はSHOULD（重複検出の補助）

---

## 6. 難易度表取り込み仕様（最重要）

### 6.1 入口（bmstableページ）

- ユーザーは表URLを登録（一般にHTMLページ）
- HTMLを取得し `<meta name="bmstable" content="...">` を抽出
- contentを **ページURL基準**で解決して header_url を作る
- header_json を取得 → data_url を読む
- data_url は **header_url基準**で解決して取得
- 取得はバックグラウンド。失敗しても既存データは保持

### 6.2 data_url取得（HTTP共通要件）

- リダイレクト追従（例：最大10回）
- `Accept: application/json`
- Content-TypeがJSONでなくても本文がJSONならパースを試行
- final_url（リダイレクト後）を保存
- 更新判定は本文hash（SHA-256等）を必須（ETag/Last-Modifiedは補助）

---

## 7. 難易度表パターン分類と実装方針

### Pattern A：Classic（header + data がフラット配列）

**判定**：headerに `course`/`grade`/`mode` 等がない、dataが配列

- header：name, symbol, data_url, level_order, last_update 等
- data：`[{md5, level, title, artist, url, url_diff, ...}]`

**取り込み**

- entry.md5（lowercase）
- level_text = entry.level（文字列のまま）
- title/artist/url/url_diff/comment 等の既知キーを抜く
- 残りは raw_json に保存

### Pattern B：Analyzer（course + score.jsonカタログ）

**判定**：headerに `course` があり、grade/mode等はない

- header.course：ネスト配列が多い、course itemに md5[]/trophy/constraint 等
- data（score.json）：譜面カタログ（md5,title,artist,url,sha256 等）

**取り込み**

- score.json → table_entries（md5単位）
- header.course → table_groups(type=course) + table_group_items(md5)
- 所持判定は md5 JOIN

### Pattern C：拡張ヘッダ（grade/course/mode/tag/attr + rich entries）

**判定**：headerに `grade` がある、または `mode/attr/tag` のような拡張キーがある

- grade：name/style/md5[] または charts[{md5,title}]
- course：外側配列で複数セット（通常/派生）になることがある
- data：entryが多キー（note/total/judge/state/tag/proposer 等）

**取り込み**

- headerの grade → table_groups(type=grade) + items
- headerの course → table_groups(type=course, group_set_indexで外側配列index保持) + items
- constraintsは空文字要素を除去して保存
- data entryは既知キーだけ抜いて、raw_json必須
- level_order は "11+" "12-" "！" "？" 等があるので文字列配列として保持し、数値化しない

### Pattern D：API/リダイレクト（Google Apps Script等）

**判定**：data_url がスクリプト/エンドポイントっぽい、取得時にfinal_urlが変わる等

- header：courseがあるが、dataはAPIから返る
- data：`[{level,title,song_artist,charter,md5,...}]` など別スキーマ

**取り込み**

- artistが空なら song_artist を採用
- charter を table_entries.charter へ保存
- final_url 保存、hash差分更新

---

## 8. ローカル所持判定（MUST）

- ローカルChartの `file_md5` と、各 table_entries の `md5` を一致させて所持判定
- 表側がsha256を持つ場合、sha256も保持（将来の補助照合に使える）

---

## 9. 重複検出・解消（MVP）

### 9.1 重複候補検出（MUST）

- まず「同一譜面」：`file_md5` が一致 → 同一譜面
- 次に「同曲っぽい」：title/artist正規化一致（候補提示レベル）

### 9.2 解消フロー（MUST）

1. 重複候補グループ表示
2. 保持先Package選択
3. プレビュー（移動対象一覧 / 競合の扱い）
4. 実行：まず移動/コピー → 成功確認 → 不要側をゴミ箱
5. DBを該当Packageのみ再スキャン/再解析

### 9.3 ルート跨ぎガード（MUST）

- 同一root内のみデフォルト許可
- root跨ぎはデフォルト禁止（解除時は強警告+2段階確認+ログ）

---

## 10. 受け入れ条件（MVP）

- [ ] ルート追加→スキャン→譜面一覧がDBに入り検索できる
- [ ] 表URLを登録→取り込み→table_entriesが入り、所持/未所持が出る
- [ ] Pattern A/B/C/D の少なくとも1例ずつで取り込みが成功し、raw_jsonが保存される
- [ ] 表取り込みの更新はhash差分でスキップでき、失敗しても既存表データは残る
- [ ] 重複候補が出せて、Windowsゴミ箱による削除が安全に動く
- [ ] すべての主要処理でJSONLログが出る

---

## 11. 実装タスク分割（Codex向け）

### 11.1 モジュール案

- `scan/`：ルート走査、Package判定、Chart列挙
- `bms_parse/`：ヘッダ抽出、refs抽出、MD5算出
- `db/`：migrations、DAO、バルクinsert、FTS
- `tables/`：
  - `fetch.rs`（HTML→meta→header→data、リダイレクト/Accept/Content-Type/ハッシュ）
  - `classify.rs`（Pattern判定）
  - `parse_a.rs` `parse_b.rs` `parse_c.rs` `parse_d.rs`（マッピング）
  - `upsert.rs`（ステージング→切替）
- `dedupe/`：候補生成、プレビュー、実行（trash）
- `logging/`：JSONL logger

### 11.2 外部crate候補

- ゴミ箱：`trash`
- HTTP：reqwest（リダイレクト）
- HTML：scraper
- JSON：serde_json
- SQLite：rusqlite or sqlx（どちらでも可、バッチとトランザクション必須）
- （任意）bmstable：`bms-table`（メタ抽出などに活用）

---

## 12. 注意（実装の約束事）

- パスにglobを絶対に使わない
- 表データは **未知キーが増える前提**：raw_json必須、既知キーだけ抜く
- levelは文字列扱い
- constraint配列の空要素除去
- MD5は小文字統一
- 失敗してもDB破壊しない（テーブル更新はステージング→スワップ）
