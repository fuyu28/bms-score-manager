import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { FolderOpen, RefreshCcw, Search, Trash2 } from "lucide-react";
import { type ReactNode, useEffect, useMemo, useState } from "react";

import { Badge } from "./components/ui/badge";
import { Button } from "./components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "./components/ui/card";
import { Input } from "./components/ui/input";
import { Textarea } from "./components/ui/textarea";

type TabKey = "main" | "dedupe" | "settings";

type RootRow = { id: number; path: string; enabled: boolean; created_at: string };
type ScanResult = {
  root_id: number;
  package_count: number;
  chart_count: number;
  parsed_count: number;
};
type TableSource = {
  id: number;
  input_url: string;
  enabled: boolean;
  last_fetch_at?: string | null;
  last_success_at?: string | null;
  last_error?: string | null;
};
type ImportResult = {
  source_id: number;
  table_id: number;
  pattern: string;
  entry_count: number;
  group_count: number;
  skipped_by_hash: boolean;
};
type OwnershipSummary = {
  table_id: number;
  total_entries: number;
  owned_entries: number;
  missing_entries: number;
};
type ChartRow = {
  chart_id: number;
  title?: string | null;
  artist?: string | null;
  rel_path: string;
  file_md5?: string | null;
  root_path: string;
  package_path: string;
};
type DuplicateGroup = {
  key: string;
  kind: string;
  charts: Array<{
    chart_id: number;
    full_path: string;
    root_id: number;
    title?: string | null;
    artist?: string | null;
  }>;
};
type DedupePreview = {
  keep_chart_id: number;
  remove_count: number;
  cross_root: boolean;
  targets: string[];
  operations: Array<{ source_path: string; backup_path: string; backup_conflict: boolean }>;
  confirmation_phrase: string;
};

const fmt = (s?: string | null) => (s ? new Date(s).toLocaleString("ja-JP") : "-");

export default function App() {
  const [tab, setTab] = useState<TabKey>("main");
  const [message, setMessage] = useState<string>("");
  const [loading, setLoading] = useState<string | null>(null);

  const [roots, setRoots] = useState<RootRow[]>([]);
  const [rootPath, setRootPath] = useState("");
  const [scanLog, setScanLog] = useState<ScanResult | null>(null);

  const [tableUrlBulk, setTableUrlBulk] = useState("");
  const [sources, setSources] = useState<TableSource[]>([]);
  const [lastImport, setLastImport] = useState<ImportResult | null>(null);
  const [ownership, setOwnership] = useState<OwnershipSummary | null>(null);

  const [query, setQuery] = useState("");
  const [charts, setCharts] = useState<ChartRow[]>([]);

  const [duplicates, setDuplicates] = useState<DuplicateGroup[]>([]);
  const [keepChartId, setKeepChartId] = useState("");
  const [removeChartIds, setRemoveChartIds] = useState("");
  const [preview, setPreview] = useState<DedupePreview | null>(null);

  const removeIds = useMemo(
    () =>
      removeChartIds
        .split(/[\s,]+/)
        .map((v) => Number(v))
        .filter((v) => Number.isFinite(v) && v > 0),
    [removeChartIds],
  );

  const wrap = async (key: string, fn: () => Promise<void>) => {
    setLoading(key);
    setMessage("");
    try {
      await fn();
    } catch (e) {
      setMessage(String(e));
    } finally {
      setLoading(null);
    }
  };

  const loadRoots = () =>
    wrap("roots", async () => setRoots(await invoke<RootRow[]>("list_roots")));
  const loadSources = () =>
    wrap("sources", async () => setSources(await invoke<TableSource[]>("list_table_sources")));
  const searchCharts = (q: string) =>
    wrap("search", async () => {
      const rows = await invoke<ChartRow[]>("search_charts", {
        params: { query: q, limit: 200, offset: 0 },
      });
      setCharts(rows);
    });
  const loadDuplicates = () =>
    wrap("dedupe-detect", async () =>
      setDuplicates(await invoke<DuplicateGroup[]>("detect_duplicates")),
    );

  useEffect(() => {
    void Promise.all([loadRoots(), loadSources(), searchCharts(""), loadDuplicates()]);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const pickRootDirectory = () =>
    wrap("pick-root", async () => {
      const selected = await open({ directory: true, multiple: false });
      if (typeof selected === "string") setRootPath(selected);
    });

  const addRoot = () =>
    wrap("add-root", async () => {
      const path = rootPath.trim();
      if (!path) return;
      await invoke<number>("add_root", { path });
      setRootPath("");
      await loadRoots();
    });

  const scanRoot = (rootId: number) =>
    wrap(`scan-${rootId}`, async () => {
      const result = await invoke<ScanResult>("scan_root", { rootId });
      setScanLog(result);
      await searchCharts(query);
      await loadDuplicates();
    });

  const addTableSourcesBulk = () =>
    wrap("add-table-bulk", async () => {
      const urls = Array.from(
        new Set(
          tableUrlBulk
            .split(/\r?\n/)
            .map((v) => v.trim())
            .filter(Boolean),
        ),
      );
      for (const inputUrl of urls) {
        await invoke<number>("register_table_source", { inputUrl });
      }
      setTableUrlBulk("");
      await loadSources();
    });

  const importSource = (sourceId: number) =>
    wrap(`import-${sourceId}`, async () => {
      const result = await invoke<ImportResult>("import_table_source", { sourceId });
      setLastImport(result);
      setOwnership(
        await invoke<OwnershipSummary>("ownership_summary", { tableId: result.table_id }),
      );
      await loadSources();
    });

  const importAllSources = () =>
    wrap("import-all", async () => {
      for (const src of sources) {
        const result = await invoke<ImportResult>("import_table_source", { sourceId: src.id });
        setLastImport(result);
        setOwnership(
          await invoke<OwnershipSummary>("ownership_summary", { tableId: result.table_id }),
        );
      }
      await loadSources();
    });

  const previewDedupe = () =>
    wrap("dedupe-preview", async () => {
      const result = await invoke<DedupePreview>("preview_dedupe", {
        req: { keep_chart_id: Number(keepChartId), remove_chart_ids: removeIds },
      });
      setPreview(result);
    });

  const executeDedupe = () =>
    wrap("dedupe-exec", async () => {
      await invoke("execute_dedupe", {
        req: {
          keep_chart_id: Number(keepChartId),
          remove_chart_ids: removeIds,
          allow_cross_root: false,
          confirmation_text: preview?.confirmation_phrase,
        },
      });
      setPreview(null);
      await Promise.all([loadDuplicates(), searchCharts(query)]);
      setMessage("重複削除を実行しました。バックアップ後にゴミ箱へ移動済みです。");
    });

  return (
    <main className="mx-auto flex min-h-screen w-full max-w-[1500px] gap-4 px-3 py-4 sm:px-4">
      <aside className="w-56 shrink-0 rounded-2xl border border-border/80 bg-card/80 p-3 backdrop-blur">
        <h1 className="mb-4 font-['Space_Grotesk'] text-xl font-bold">BMS Explorer</h1>
        <div className="space-y-2">
          <NavButton active={tab === "main"} onClick={() => setTab("main")}>
            メイン
          </NavButton>
          <NavButton active={tab === "dedupe"} onClick={() => setTab("dedupe")}>
            重複整理
          </NavButton>
          <NavButton active={tab === "settings"} onClick={() => setTab("settings")}>
            設定
          </NavButton>
        </div>
        <div className="mt-4 border-t border-border pt-3">
          <Button
            variant="secondary"
            className="w-full"
            onClick={() =>
              void Promise.all([loadRoots(), loadSources(), searchCharts(query), loadDuplicates()])
            }
          >
            <RefreshCcw className="mr-2 h-4 w-4" />
            全体更新
          </Button>
        </div>
      </aside>

      <section className="min-w-0 flex-1 rounded-2xl border border-border/80 bg-card/70 p-4 backdrop-blur">
        {message ? (
          <div className="mb-3 rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
            {message}
          </div>
        ) : null}

        {tab === "main" ? (
          <Card className="border-0 bg-transparent shadow-none">
            <CardHeader className="px-0">
              <CardTitle>譜面一覧</CardTitle>
              <CardDescription>エクスプローラー風に検索結果を一覧表示します。</CardDescription>
            </CardHeader>
            <CardContent className="space-y-3 px-0">
              <div className="flex gap-2">
                <Input
                  value={query}
                  onChange={(e) => setQuery(e.currentTarget.value)}
                  placeholder="タイトル / アーティスト / パス"
                />
                <Button onClick={() => void searchCharts(query)} disabled={!!loading}>
                  <Search className="mr-2 h-4 w-4" />
                  検索
                </Button>
              </div>
              <div className="overflow-hidden rounded-xl border border-border/80">
                <div className="grid grid-cols-[90px_1.4fr_1fr_2fr_1.5fr] bg-muted/60 px-3 py-2 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                  <div>ID</div>
                  <div>Title</div>
                  <div>Artist</div>
                  <div>Path</div>
                  <div>MD5</div>
                </div>
                <div className="max-h-[68vh] divide-y divide-border overflow-auto">
                  {charts.map((c) => (
                    <div
                      key={c.chart_id}
                      className="grid grid-cols-[90px_1.4fr_1fr_2fr_1.5fr] px-3 py-2 text-sm hover:bg-accent/25"
                    >
                      <div className="font-mono text-xs text-muted-foreground">#{c.chart_id}</div>
                      <div className="truncate" title={c.title ?? c.rel_path}>
                        {c.title || c.rel_path}
                      </div>
                      <div className="truncate text-muted-foreground">{c.artist || "-"}</div>
                      <div
                        className="truncate text-muted-foreground"
                        title={`${c.root_path}/${c.package_path}/${c.rel_path}`}
                      >
                        {c.root_path}/{c.package_path}/{c.rel_path}
                      </div>
                      <div className="truncate font-mono text-xs text-muted-foreground">
                        {c.file_md5 || "-"}
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            </CardContent>
          </Card>
        ) : null}

        {tab === "dedupe" ? (
          <div className="grid gap-4 lg:grid-cols-[1.35fr_0.9fr]">
            <Card>
              <CardHeader>
                <CardTitle>重複候補一覧</CardTitle>
                <CardDescription>同一 file_md5 の候補を一覧表示します。</CardDescription>
              </CardHeader>
              <CardContent>
                <Button
                  variant="secondary"
                  onClick={() => void loadDuplicates()}
                  disabled={!!loading}
                >
                  候補を再検出
                </Button>
                <div className="mt-3 max-h-[62vh] space-y-2 overflow-auto pr-1">
                  {duplicates.map((g) => (
                    <div
                      key={g.key}
                      className="rounded-lg border border-border/70 bg-background/60 p-3"
                    >
                      <div className="mb-1 flex items-center justify-between gap-2">
                        <code className="truncate text-xs">{g.key}</code>
                        <Badge variant="secondary">{g.charts.length}件</Badge>
                      </div>
                      <div className="space-y-1">
                        {g.charts.map((c) => (
                          <div
                            key={c.chart_id}
                            className="truncate text-xs text-muted-foreground"
                            title={c.full_path}
                          >
                            #{c.chart_id} {c.full_path}
                          </div>
                        ))}
                      </div>
                    </div>
                  ))}
                </div>
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle>重複解消</CardTitle>
                <CardDescription>プレビュー後に実行します（root跨ぎは不可）。</CardDescription>
              </CardHeader>
              <CardContent className="space-y-3">
                <Input
                  value={keepChartId}
                  onChange={(e) => setKeepChartId(e.currentTarget.value)}
                  placeholder="保持する chart_id"
                />
                <Textarea
                  value={removeChartIds}
                  onChange={(e) => setRemoveChartIds(e.currentTarget.value)}
                  placeholder="削除する chart_id（カンマ/改行区切り）"
                />
                <div className="flex gap-2">
                  <Button
                    variant="outline"
                    onClick={() => void previewDedupe()}
                    disabled={!keepChartId || removeIds.length === 0 || !!loading}
                  >
                    プレビュー
                  </Button>
                  <Button
                    variant="destructive"
                    onClick={() => void executeDedupe()}
                    disabled={!preview || preview.cross_root || !!loading}
                  >
                    <Trash2 className="mr-2 h-4 w-4" />
                    実行
                  </Button>
                </div>
                {preview ? (
                  <div className="rounded-md border border-border bg-background/60 p-3 text-xs">
                    <div>削除対象: {preview.remove_count}件</div>
                    <div>
                      確認フレーズ: <code>{preview.confirmation_phrase}</code>
                    </div>
                    <div>root跨ぎ: {preview.cross_root ? "あり（実行不可）" : "なし"}</div>
                  </div>
                ) : null}
              </CardContent>
            </Card>
          </div>
        ) : null}

        {tab === "settings" ? (
          <div className="grid gap-4 lg:grid-cols-2">
            <Card>
              <CardHeader>
                <CardTitle>ルート設定</CardTitle>
                <CardDescription>フォルダ選択または直接入力でルートを追加します。</CardDescription>
              </CardHeader>
              <CardContent className="space-y-3">
                <div className="flex gap-2">
                  <Input
                    value={rootPath}
                    onChange={(e) => setRootPath(e.currentTarget.value)}
                    placeholder="例: D:/BMS"
                  />
                  <Button variant="outline" onClick={() => void pickRootDirectory()}>
                    <FolderOpen className="mr-2 h-4 w-4" />
                    選択
                  </Button>
                </div>
                <Button onClick={() => void addRoot()} disabled={!rootPath || !!loading}>
                  ルート追加
                </Button>
                <div className="max-h-[48vh] space-y-2 overflow-auto pr-1">
                  {roots.map((r) => (
                    <div
                      key={r.id}
                      className="rounded-lg border border-border/70 bg-background/60 p-3"
                    >
                      <div className="truncate text-sm font-medium">{r.path}</div>
                      <div className="mt-1 flex items-center justify-between text-xs text-muted-foreground">
                        <span>登録: {fmt(r.created_at)}</span>
                        <Button size="sm" onClick={() => void scanRoot(r.id)} disabled={!!loading}>
                          スキャン
                        </Button>
                      </div>
                    </div>
                  ))}
                </div>
                {scanLog ? (
                  <div className="rounded-md bg-secondary/40 p-2 text-xs">
                    root#{scanLog.root_id} package {scanLog.package_count} chart{" "}
                    {scanLog.chart_count} parsed {scanLog.parsed_count}
                  </div>
                ) : null}
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle>難易度表取り込み設定</CardTitle>
                <CardDescription>URLを複数行で登録し、一括取り込みできます。</CardDescription>
              </CardHeader>
              <CardContent className="space-y-3">
                <Textarea
                  value={tableUrlBulk}
                  onChange={(e) => setTableUrlBulk(e.currentTarget.value)}
                  placeholder={"https://example.com/table-a.html\nhttps://example.com/table-b.html"}
                />
                <div className="flex gap-2">
                  <Button
                    onClick={() => void addTableSourcesBulk()}
                    disabled={!tableUrlBulk.trim() || !!loading}
                  >
                    複数登録
                  </Button>
                  <Button
                    variant="secondary"
                    onClick={() => void importAllSources()}
                    disabled={sources.length === 0 || !!loading}
                  >
                    全件取り込み
                  </Button>
                </div>
                <div className="max-h-[42vh] space-y-2 overflow-auto pr-1">
                  {sources.map((src) => (
                    <div
                      key={src.id}
                      className="rounded-lg border border-border/70 bg-background/60 p-3"
                    >
                      <div className="truncate text-sm font-medium">{src.input_url}</div>
                      <div className="mt-1 text-xs text-muted-foreground">
                        成功: {fmt(src.last_success_at)} / 取得: {fmt(src.last_fetch_at)}
                      </div>
                      {src.last_error ? (
                        <div className="mt-1 text-xs text-destructive">{src.last_error}</div>
                      ) : null}
                      <div className="mt-2">
                        <Button
                          size="sm"
                          onClick={() => void importSource(src.id)}
                          disabled={!!loading}
                        >
                          取り込み
                        </Button>
                      </div>
                    </div>
                  ))}
                </div>
                {lastImport ? (
                  <div className="rounded-md bg-accent/40 p-2 text-xs">
                    table#{lastImport.table_id} pattern {lastImport.pattern} entries{" "}
                    {lastImport.entry_count}
                  </div>
                ) : null}
                {ownership ? (
                  <div className="rounded-md bg-secondary/40 p-2 text-xs">
                    所持 {ownership.owned_entries} / 未所持 {ownership.missing_entries} / 合計{" "}
                    {ownership.total_entries}
                  </div>
                ) : null}
              </CardContent>
            </Card>
          </div>
        ) : null}
      </section>
    </main>
  );
}

function NavButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={[
        "w-full rounded-lg px-3 py-2 text-left text-sm transition",
        active
          ? "bg-primary text-primary-foreground"
          : "bg-background/60 text-foreground hover:bg-accent",
      ].join(" ")}
    >
      {children}
    </button>
  );
}
