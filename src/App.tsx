import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { RefreshCcw, Search, Trash2 } from "lucide-react";

import { Badge } from "./components/ui/badge";
import { Button } from "./components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "./components/ui/card";
import { Input } from "./components/ui/input";
import { Textarea } from "./components/ui/textarea";

type RootRow = {
  id: number;
  path: string;
  enabled: boolean;
  created_at: string;
};

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
  const [rootPath, setRootPath] = useState("");
  const [roots, setRoots] = useState<RootRow[]>([]);
  const [scanLog, setScanLog] = useState<ScanResult | null>(null);

  const [tableUrl, setTableUrl] = useState("");
  const [sources, setSources] = useState<TableSource[]>([]);
  const [ownership, setOwnership] = useState<OwnershipSummary | null>(null);
  const [lastImport, setLastImport] = useState<ImportResult | null>(null);

  const [query, setQuery] = useState("");
  const [charts, setCharts] = useState<ChartRow[]>([]);

  const [duplicates, setDuplicates] = useState<DuplicateGroup[]>([]);
  const [keepChartId, setKeepChartId] = useState("");
  const [removeChartIds, setRemoveChartIds] = useState("");
  const [preview, setPreview] = useState<DedupePreview | null>(null);

  const [loading, setLoading] = useState<string | null>(null);
  const [message, setMessage] = useState<string>("");

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

  useEffect(() => {
    void Promise.all([loadRoots(), loadSources()]);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const loadRoots = () =>
    wrap("roots", async () => {
      const rows = await invoke<RootRow[]>("list_roots");
      setRoots(rows);
    });

  const addRoot = () =>
    wrap("add-root", async () => {
      await invoke<number>("add_root", { path: rootPath });
      setRootPath("");
      await loadRoots();
    });

  const scanRoot = (rootId: number) =>
    wrap(`scan-${rootId}`, async () => {
      const result = await invoke<ScanResult>("scan_root", { rootId });
      setScanLog(result);
    });

  const loadSources = () =>
    wrap("sources", async () => {
      const rows = await invoke<TableSource[]>("list_table_sources");
      setSources(rows);
    });

  const addTableSource = () =>
    wrap("add-table", async () => {
      await invoke<number>("register_table_source", { inputUrl: tableUrl });
      setTableUrl("");
      await loadSources();
    });

  const importSource = (sourceId: number) =>
    wrap(`import-${sourceId}`, async () => {
      const result = await invoke<ImportResult>("import_table_source", { sourceId });
      setLastImport(result);
      const summary = await invoke<OwnershipSummary>("ownership_summary", {
        tableId: result.table_id,
      });
      setOwnership(summary);
      await loadSources();
    });

  const doSearch = () =>
    wrap("search", async () => {
      const rows = await invoke<ChartRow[]>("search_charts", {
        params: { query, limit: 50, offset: 0 },
      });
      setCharts(rows);
    });

  const detectDedupe = () =>
    wrap("dedupe-detect", async () => {
      const rows = await invoke<DuplicateGroup[]>("detect_duplicates");
      setDuplicates(rows);
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
      await detectDedupe();
      setMessage("重複削除を実行しました（ゴミ箱移動）。");
    });

  return (
    <main className="mx-auto w-full max-w-7xl px-4 py-8 sm:px-6 lg:px-8">
      <section className="mb-8 flex flex-col gap-3 rounded-2xl border border-border/80 bg-card/70 p-6 backdrop-blur-sm">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <h1 className="font-['Space_Grotesk'] text-3xl font-bold tracking-tight text-foreground sm:text-4xl">
            BMS Score Manager
          </h1>
          <Button
            variant="secondary"
            onClick={() => void Promise.all([loadRoots(), loadSources()])}
          >
            <RefreshCcw className="mr-2 h-4 w-4" />
            再読込
          </Button>
        </div>
        <p className="max-w-3xl text-sm text-muted-foreground">
          ローカル譜面のスキャン、難易度表取り込み、MD5照合による所持判定、重複候補整理を一元管理します。
        </p>
        {message ? <p className="text-sm text-destructive">{message}</p> : null}
      </section>

      <section className="grid gap-6 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>1. ルート管理とスキャン</CardTitle>
            <CardDescription>ルートを追加してバックグラウンドでスキャンします。</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="flex gap-2">
              <Input
                value={rootPath}
                onChange={(e) => setRootPath(e.currentTarget.value)}
                placeholder="例: D:/BMS"
              />
              <Button onClick={() => void addRoot()} disabled={!rootPath || !!loading}>
                追加
              </Button>
            </div>
            <div className="space-y-2">
              {roots.map((root) => (
                <div
                  key={root.id}
                  className="rounded-lg border border-border/70 bg-background/60 p-3"
                >
                  <div className="mb-2 flex items-center justify-between gap-2">
                    <div className="text-sm font-medium">{root.path}</div>
                    <Badge variant={root.enabled ? "default" : "outline"}>
                      {root.enabled ? "有効" : "無効"}
                    </Badge>
                  </div>
                  <div className="flex items-center justify-between text-xs text-muted-foreground">
                    <span>登録: {fmt(root.created_at)}</span>
                    <Button size="sm" onClick={() => void scanRoot(root.id)} disabled={!!loading}>
                      スキャン
                    </Button>
                  </div>
                </div>
              ))}
            </div>
            {scanLog ? (
              <div className="rounded-lg bg-secondary/50 p-3 text-sm">
                root#{scanLog.root_id} / package {scanLog.package_count} / chart{" "}
                {scanLog.chart_count} / parsed {scanLog.parsed_count}
              </div>
            ) : null}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>2. 難易度表取り込み</CardTitle>
            <CardDescription>
              bmstableページURLを登録し取り込みます（Pattern A-D対応）。
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="flex gap-2">
              <Input
                value={tableUrl}
                onChange={(e) => setTableUrl(e.currentTarget.value)}
                placeholder="https://example.com/table.html"
              />
              <Button onClick={() => void addTableSource()} disabled={!tableUrl || !!loading}>
                登録
              </Button>
            </div>
            <div className="space-y-2">
              {sources.map((src) => (
                <div
                  key={src.id}
                  className="rounded-lg border border-border/70 bg-background/60 p-3"
                >
                  <div className="mb-2 truncate text-sm font-medium">{src.input_url}</div>
                  <div className="mb-2 text-xs text-muted-foreground">
                    成功: {fmt(src.last_success_at)} / 取得: {fmt(src.last_fetch_at)}
                  </div>
                  {src.last_error ? (
                    <div className="mb-2 text-xs text-destructive">{src.last_error}</div>
                  ) : null}
                  <Button size="sm" onClick={() => void importSource(src.id)} disabled={!!loading}>
                    取り込み
                  </Button>
                </div>
              ))}
            </div>
            {lastImport ? (
              <div className="rounded-lg bg-accent/50 p-3 text-sm text-accent-foreground">
                table#{lastImport.table_id} / pattern {lastImport.pattern} / entries{" "}
                {lastImport.entry_count} / groups {lastImport.group_count}
              </div>
            ) : null}
            {ownership ? (
              <div className="rounded-lg bg-secondary/50 p-3 text-sm">
                所持 {ownership.owned_entries} / 未所持 {ownership.missing_entries} / 合計{" "}
                {ownership.total_entries}
              </div>
            ) : null}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>3. 譜面検索</CardTitle>
            <CardDescription>
              SQLite FTS5でタイトル・アーティスト・パスを検索します。
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="flex gap-2">
              <Input
                value={query}
                onChange={(e) => setQuery(e.currentTarget.value)}
                placeholder="タイトル / アーティスト / パス"
              />
              <Button onClick={() => void doSearch()} disabled={!!loading}>
                <Search className="mr-2 h-4 w-4" />
                検索
              </Button>
            </div>
            <div className="max-h-72 space-y-2 overflow-auto pr-1">
              {charts.map((chart) => (
                <div
                  key={chart.chart_id}
                  className="rounded-lg border border-border/70 bg-background/70 p-3 text-sm"
                >
                  <div className="font-medium">{chart.title || chart.rel_path}</div>
                  <div className="text-xs text-muted-foreground">
                    {chart.artist || "(artistなし)"}
                  </div>
                  <div className="truncate text-xs text-muted-foreground">
                    {chart.root_path}/{chart.package_path}/{chart.rel_path}
                  </div>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>4. 重複整理（安全実行）</CardTitle>
            <CardDescription>プレビュー後にゴミ箱移動で削除します。</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <Button variant="secondary" onClick={() => void detectDedupe()} disabled={!!loading}>
              重複候補を検出
            </Button>
            <div className="max-h-48 space-y-2 overflow-auto pr-1">
              {duplicates.slice(0, 10).map((g) => (
                <div
                  key={g.key}
                  className="rounded-lg border border-border/70 bg-background/60 p-3 text-xs"
                >
                  <div className="mb-1 font-semibold">{g.key}</div>
                  {g.charts.map((c) => (
                    <div key={c.chart_id} className="truncate text-muted-foreground">
                      #{c.chart_id} {c.full_path}
                    </div>
                  ))}
                </div>
              ))}
            </div>
            <Input
              value={keepChartId}
              onChange={(e) => setKeepChartId(e.currentTarget.value)}
              placeholder="保持する chart_id"
            />
            <Textarea
              value={removeChartIds}
              onChange={(e) => setRemoveChartIds(e.currentTarget.value)}
              placeholder="削除する chart_id（カンマ区切りまたは改行）"
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
              <div className="rounded-lg border border-border bg-background/70 p-3 text-sm">
                <div>削除対象: {preview.remove_count}件</div>
                <div>root跨ぎ: {preview.cross_root ? "あり（実行不可）" : "なし"}</div>
                <div className="mt-2 font-mono text-xs text-muted-foreground">
                  確認フレーズ: {preview.confirmation_phrase}
                </div>
                <div className="mt-2 max-h-24 space-y-1 overflow-auto text-xs text-muted-foreground">
                  {preview.operations.slice(0, 5).map((op) => (
                    <div key={op.source_path} className="truncate">
                      backup: {op.backup_path}
                    </div>
                  ))}
                </div>
              </div>
            ) : null}
          </CardContent>
        </Card>
      </section>
    </main>
  );
}
