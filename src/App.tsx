import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  FolderOpen,
  Plus,
  RefreshCcw,
  Search,
  Settings2,
  ShieldAlert,
  Trash2,
  X,
} from "lucide-react";
import { type ReactNode, useEffect, useMemo, useState } from "react";

import { Badge } from "./components/ui/badge";
import { Button } from "./components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "./components/ui/card";
import { Input } from "./components/ui/input";

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
  root_id: number;
  title?: string | null;
  artist?: string | null;
  rel_path: string;
  file_md5?: string | null;
  root_path: string;
  package_path: string;
};
type DuplicateChart = {
  chart_id: number;
  full_path: string;
  root_id: number;
  title?: string | null;
  artist?: string | null;
};
type DuplicateGroup = {
  key: string;
  kind: string;
  charts: DuplicateChart[];
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

  const [tableUrlInput, setTableUrlInput] = useState("");
  const [tableUrlList, setTableUrlList] = useState<string[]>([]);
  const [sources, setSources] = useState<TableSource[]>([]);
  const [lastImport, setLastImport] = useState<ImportResult | null>(null);
  const [ownership, setOwnership] = useState<OwnershipSummary | null>(null);

  const [query, setQuery] = useState("");
  const [charts, setCharts] = useState<ChartRow[]>([]);

  const [duplicates, setDuplicates] = useState<DuplicateGroup[]>([]);
  const [dedupeLoaded, setDedupeLoaded] = useState(false);
  const [activeGroup, setActiveGroup] = useState<DuplicateGroup | null>(null);
  const [mergeKeepId, setMergeKeepId] = useState<number | null>(null);
  const [mergePreview, setMergePreview] = useState<DedupePreview | null>(null);

  const [scanStatus, setScanStatus] = useState<string>("");
  const [selectedRootId, setSelectedRootId] = useState<number | null>(null);

  const visibleCharts = useMemo(
    () => (selectedRootId == null ? charts : charts.filter((c) => c.root_id === selectedRootId)),
    [charts, selectedRootId],
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
    void Promise.all([loadRoots(), loadSources(), searchCharts("")]);
    const unlisten = listen("scan_progress", (event) => {
      const payload = event.payload as {
        phase?: string;
        root_id?: number;
        packages?: number;
        charts?: number;
        done?: number;
        total?: number;
        parsed?: number;
      };
      if (payload.phase === "parsing" && payload.total && payload.done !== undefined) {
        const pct = Math.min(100, Math.floor(((payload.done as number) / payload.total) * 100));
        setScanStatus(`解析中 ${payload.done}/${payload.total} (${pct}%)`);
      } else if (payload.phase === "parse_done" && payload.total) {
        setScanStatus(`解析完了 ${payload.parsed}/${payload.total}`);
      } else if (payload.phase === "structure_done") {
        setScanStatus(`構造走査完了 package ${payload.packages} chart ${payload.charts}`);
      }
    });
    return () => {
      void unlisten.then((f) => f());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (selectedRootId == null && roots.length > 0) {
      setSelectedRootId(roots[0].id);
    }
  }, [roots, selectedRootId]);

  useEffect(() => {
    if (tab !== "dedupe" || dedupeLoaded) return;
    setDedupeLoaded(true);
    void loadDuplicates();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab, dedupeLoaded]);

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
      setScanStatus(`root #${rootId} をスキャン中...`);
      const result = await invoke<ScanResult>("scan_root", { rootId });
      setScanLog(result);
      setScanStatus(
        `root #${rootId} 完了: package ${result.package_count}, chart ${result.chart_count}, parsed ${result.parsed_count}`,
      );
      await searchCharts(query);
      if (tab === "dedupe") await loadDuplicates();
    });

  const addTableSourcesBulk = () =>
    wrap("add-table-bulk", async () => {
      const urls = Array.from(new Set(tableUrlList.map((v) => v.trim()).filter(Boolean)));
      for (const inputUrl of urls) {
        await invoke<number>("register_table_source", { inputUrl });
      }
      setTableUrlInput("");
      setTableUrlList([]);
      await loadSources();
    });

  const addTableUrlItem = () => {
    const next = tableUrlInput.trim();
    if (!next) return;
    if (tableUrlList.includes(next)) return;
    setTableUrlList((prev) => [...prev, next]);
    setTableUrlInput("");
  };

  const removeTableUrlItem = (url: string) => {
    setTableUrlList((prev) => prev.filter((v) => v !== url));
  };

  const importSource = (sourceId: number) =>
    wrap(`import-${sourceId}`, async () => {
      const result = await invoke<ImportResult>("import_table_source", { sourceId });
      setLastImport(result);
      setOwnership(await invoke<OwnershipSummary>("ownership_summary", { tableId: result.table_id }));
      await loadSources();
    });

  const importAllSources = () =>
    wrap("import-all", async () => {
      for (const src of sources) {
        const result = await invoke<ImportResult>("import_table_source", { sourceId: src.id });
        setLastImport(result);
        setOwnership(await invoke<OwnershipSummary>("ownership_summary", { tableId: result.table_id }));
      }
      await loadSources();
    });

  const getRemoveIds = (group: DuplicateGroup, keepId: number) =>
    group.charts.map((c) => c.chart_id).filter((id) => id !== keepId);

  const openMergeModal = (group: DuplicateGroup) => {
    const defaultKeep = group.charts[0]?.chart_id ?? null;
    setActiveGroup(group);
    setMergeKeepId(defaultKeep);
    setMergePreview(null);
  };

  const closeMergeModal = () => {
    setActiveGroup(null);
    setMergeKeepId(null);
    setMergePreview(null);
  };

  const previewMerge = () =>
    wrap("dedupe-preview", async () => {
      if (!activeGroup || !mergeKeepId) return;
      const removeIds = getRemoveIds(activeGroup, mergeKeepId);
      if (removeIds.length === 0) return;
      const result = await invoke<DedupePreview>("preview_dedupe", {
        req: { keep_chart_id: mergeKeepId, remove_chart_ids: removeIds },
      });
      setMergePreview(result);
    });

  const executeMerge = () =>
    wrap("dedupe-exec", async () => {
      if (!activeGroup || !mergeKeepId || !mergePreview) return;
      const removeIds = getRemoveIds(activeGroup, mergeKeepId);
      await invoke("execute_dedupe", {
        req: {
          keep_chart_id: mergeKeepId,
          remove_chart_ids: removeIds,
          allow_cross_root: false,
          confirmation_text: mergePreview.confirmation_phrase,
        },
      });
      closeMergeModal();
      await Promise.all([loadDuplicates(), searchCharts(query)]);
      setMessage("重複削除を実行しました。バックアップ後にゴミ箱へ移動済みです。");
    });

  return (
    <main className="bms-shell">
      <header className="bms-topbar">
        <div className="flex items-center gap-3">
          <h1 className="font-['Chakra_Petch'] text-xl font-semibold tracking-wide">BeMusic Browser</h1>
          <div className="hidden h-4 w-px bg-border/80 md:block" />
          <div className="hidden text-xs text-muted-foreground md:block">BMS Score Manager</div>
        </div>
        <div className="bms-topnav">
          <NavButton active={tab === "main"} onClick={() => setTab("main")}>譜面一覧</NavButton>
          <NavButton active={tab === "dedupe"} onClick={() => setTab("dedupe")}>
            <ShieldAlert className="mr-1 inline h-4 w-4" />重複整理
          </NavButton>
          <NavButton active={tab === "settings"} onClick={() => setTab("settings")}>
            <Settings2 className="mr-1 inline h-4 w-4" />設定
          </NavButton>
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="secondary"
            className="border border-border bg-secondary/70"
            onClick={() =>
              void Promise.all([
                loadRoots(),
                loadSources(),
                searchCharts(query),
                tab === "dedupe" ? loadDuplicates() : Promise.resolve(),
              ])
            }
          >
            <RefreshCcw className="mr-2 h-4 w-4" />全体更新
          </Button>
        </div>
      </header>

      <div className="bms-body">
        <aside className="bms-left-tree">
          <div className="mb-3 text-sm font-semibold tracking-wide text-muted-foreground">ROOT TREE</div>
          <div className="space-y-2">
            <div className="flex gap-2">
              <Input value={rootPath} onChange={(e) => setRootPath(e.currentTarget.value)} placeholder="例: D:/BMS" />
              <Button variant="outline" onClick={() => void pickRootDirectory()}>
                <FolderOpen className="h-4 w-4" />
              </Button>
            </div>
            <Button onClick={() => void addRoot()} disabled={!rootPath || !!loading} className="w-full">
              ルート追加
            </Button>
          </div>
          <div className="mt-3 max-h-[62vh] space-y-2 overflow-auto pr-1">
            {roots.map((r) => (
              <div
                key={r.id}
                className={[
                  "rounded-md border p-2",
                  selectedRootId === r.id ? "border-primary/60 bg-primary/10" : "border-border/70 bg-background/70",
                ].join(" ")}
              >
                <button type="button" className="block w-full text-left" onClick={() => setSelectedRootId(r.id)}>
                  <div className="truncate text-sm font-medium">{r.path}</div>
                  <div className="mt-1 text-[11px] text-muted-foreground">{fmt(r.created_at)}</div>
                </button>
                <Button size="sm" className="mt-2 w-full" onClick={() => void scanRoot(r.id)} disabled={!!loading}>
                  スキャン
                </Button>
              </div>
            ))}
          </div>
          {scanLog ? (
            <div className="mt-3 rounded-md bg-secondary/40 p-2 text-xs">
              root#{scanLog.root_id} package {scanLog.package_count} chart {scanLog.chart_count} parsed {scanLog.parsed_count}
            </div>
          ) : null}
        </aside>

        <section className="bms-workspace">
          {message ? (
            <div className="mb-3 rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
              {message}
            </div>
          ) : null}

          {tab === "main" ? (
            <Card className="bms-panel border-0 bg-transparent shadow-none">
              <CardHeader className="px-0 pb-3">
                <CardTitle className="font-['Chakra_Petch'] text-xl tracking-wide">譜面一覧</CardTitle>
                <CardDescription>エクスプローラー形式で曲データを高速に参照します。</CardDescription>
              </CardHeader>
              <CardContent className="space-y-3 px-0">
                <div className="bms-hero">
                  <div className="bms-hero-copy">
                    <div className="font-['Chakra_Petch'] text-2xl font-semibold tracking-wider">BMS COLLECTION</div>
                    <div className="mt-1 text-sm text-muted-foreground">ルート管理・難易度表・重複整理を横断して統合管理</div>
                  </div>
                  <img src="/tauri.svg" alt="hero" className="h-16 w-16 opacity-80 sm:h-20 sm:w-20" />
                </div>
                <div className="flex gap-2">
                  <Input value={query} onChange={(e) => setQuery(e.currentTarget.value)} placeholder="タイトル / アーティスト / パス" />
                  <Button onClick={() => void searchCharts(query)} disabled={!!loading}>
                    <Search className="mr-2 h-4 w-4" />検索
                  </Button>
                </div>
                <div className="overflow-hidden rounded-md border border-border/80 bg-card/80">
                  <div className="bms-grid-header grid grid-cols-[90px_minmax(160px,1.4fr)_minmax(140px,1fr)_minmax(240px,2fr)_minmax(180px,1.5fr)] px-3 py-2 text-[11px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
                    <div>ID</div><div>Title</div><div>Artist</div><div>Path</div><div>MD5</div>
                  </div>
                  <div className="max-h-[68vh] overflow-auto">
                    <div className="min-w-[720px] divide-y divide-border">
                      {visibleCharts.map((c) => (
                        <div key={c.chart_id} className="bms-grid-row grid grid-cols-[90px_minmax(160px,1.4fr)_minmax(140px,1fr)_minmax(240px,2fr)_minmax(180px,1.5fr)] px-3 py-2 text-sm">
                          <div className="font-mono text-xs text-muted-foreground">#{c.chart_id}</div>
                          <div className="truncate" title={c.title ?? c.rel_path}>{c.title || c.rel_path}</div>
                          <div className="truncate text-muted-foreground">{c.artist || "-"}</div>
                          <div className="truncate text-muted-foreground" title={`${c.root_path}/${c.package_path}/${c.rel_path}`}>
                            {c.root_path}/{c.package_path}/{c.rel_path}
                          </div>
                          <div className="truncate font-mono text-xs text-muted-foreground">{c.file_md5 || "-"}</div>
                        </div>
                      ))}
                    </div>
                  </div>
                </div>
              </CardContent>
            </Card>
          ) : null}

          {tab === "dedupe" ? (
            <Card className="bms-panel">
              <CardHeader>
                <CardTitle>重複候補一覧</CardTitle>
                <CardDescription>候補を選ぶと比較モーダルが開き、マージ先を決めて実行できます。</CardDescription>
              </CardHeader>
              <CardContent>
                <Button variant="secondary" onClick={() => void loadDuplicates()} disabled={!!loading}>
                  候補を再検出
                </Button>
                <div className="mt-3 max-h-[66vh] space-y-2 overflow-auto pr-1">
                  {duplicates.map((g) => (
                    <button
                      key={g.key}
                      type="button"
                      className="w-full rounded-md border border-border/70 bg-background/60 p-3 text-left transition hover:bg-accent/35"
                      onClick={() => openMergeModal(g)}
                    >
                      <div className="mb-1 flex items-center justify-between gap-2">
                        <code className="min-w-0 break-all text-xs">{g.key}</code>
                        <div className="flex items-center gap-2">
                          <Badge variant="outline">{g.kind}</Badge>
                          <Badge variant="secondary">{g.charts.length}件</Badge>
                        </div>
                      </div>
                      <div className="text-xs text-muted-foreground">#{g.charts.map((c) => c.chart_id).join(", #")}</div>
                    </button>
                  ))}
                </div>
              </CardContent>
            </Card>
          ) : null}

          {tab === "settings" ? (
            <div className="grid gap-4 md:grid-cols-1 lg:grid-cols-1">
              <Card className="bms-panel">
                <CardHeader>
                  <CardTitle>難易度表取り込み設定</CardTitle>
                  <CardDescription>URLをリストに追加し、不要な項目を削除してから登録できます。</CardDescription>
                </CardHeader>
                <CardContent className="space-y-3">
                  <div className="flex gap-2">
                    <Input
                      value={tableUrlInput}
                      onChange={(e) => setTableUrlInput(e.currentTarget.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") {
                          e.preventDefault();
                          addTableUrlItem();
                        }
                      }}
                      placeholder="https://example.com/table-a.html"
                    />
                    <Button variant="outline" onClick={addTableUrlItem} disabled={!tableUrlInput.trim()}>
                      <Plus className="mr-1 h-4 w-4" />追加
                    </Button>
                  </div>
                  <div className="max-h-40 space-y-2 overflow-auto rounded-md border border-border/70 bg-background/60 p-2">
                    {tableUrlList.length === 0 ? (
                      <div className="text-xs text-muted-foreground">追加予定URLはありません。</div>
                    ) : (
                      tableUrlList.map((url) => (
                        <div key={url} className="flex items-center justify-between gap-2 rounded-md border border-border/70 bg-card/80 px-2 py-1.5">
                          <span className="truncate text-xs">{url}</span>
                          <Button
                            variant="ghost"
                            size="sm"
                            className="h-7 px-2 text-muted-foreground hover:text-destructive"
                            onClick={() => removeTableUrlItem(url)}
                          >
                            <X className="h-4 w-4" />
                          </Button>
                        </div>
                      ))
                    )}
                  </div>
                  <div className="flex gap-2">
                    <Button onClick={() => void addTableSourcesBulk()} disabled={tableUrlList.length === 0 || !!loading}>
                      リストを登録
                    </Button>
                    <Button variant="secondary" onClick={() => void importAllSources()} disabled={sources.length === 0 || !!loading}>
                      全件取り込み
                    </Button>
                  </div>
                  <div className="max-h-[42vh] space-y-2 overflow-auto pr-1">
                    {sources.map((src) => (
                      <div key={src.id} className="rounded-md border border-border/70 bg-background/60 p-3">
                        <div className="truncate text-sm font-medium">{src.input_url}</div>
                        <div className="mt-1 text-xs text-muted-foreground">成功: {fmt(src.last_success_at)} / 取得: {fmt(src.last_fetch_at)}</div>
                        {src.last_error ? <div className="mt-1 text-xs text-destructive">{src.last_error}</div> : null}
                        <div className="mt-2">
                          <Button size="sm" onClick={() => void importSource(src.id)} disabled={!!loading}>
                            取り込み
                          </Button>
                        </div>
                      </div>
                    ))}
                  </div>
                  {lastImport ? (
                    <div className="rounded-md bg-accent/40 p-2 text-xs">
                      table#{lastImport.table_id} pattern {lastImport.pattern} entries {lastImport.entry_count}
                    </div>
                  ) : null}
                  {ownership ? (
                    <div className="rounded-md bg-secondary/40 p-2 text-xs">
                      所持 {ownership.owned_entries} / 未所持 {ownership.missing_entries} / 合計 {ownership.total_entries}
                    </div>
                  ) : null}
                </CardContent>
              </Card>
            </div>
          ) : null}
        </section>
      </div>

      {activeGroup ? (
        <DedupeMergeModal
          group={activeGroup}
          keepId={mergeKeepId}
          preview={mergePreview}
          loading={loading}
          onClose={closeMergeModal}
          onKeepChange={(id) => {
            setMergeKeepId(id);
            setMergePreview(null);
          }}
          onPreview={() => void previewMerge()}
          onExecute={() => void executeMerge()}
        />
      ) : null}

      <StatusBar loading={loading} scanStatus={scanStatus} />
    </main>
  );
}

function DedupeMergeModal({
  group,
  keepId,
  preview,
  loading,
  onClose,
  onKeepChange,
  onPreview,
  onExecute,
}: {
  group: DuplicateGroup;
  keepId: number | null;
  preview: DedupePreview | null;
  loading: string | null;
  onClose: () => void;
  onKeepChange: (id: number) => void;
  onPreview: () => void;
  onExecute: () => void;
}) {
  const keep = group.charts.find((c) => c.chart_id === keepId) ?? group.charts[0];
  const others = group.charts.filter((c) => c.chart_id !== keep?.chart_id);

  const diffClass = (a?: string | null, b?: string | null) =>
    (a ?? "") === (b ?? "") ? "text-muted-foreground" : "font-medium text-primary";

  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/45 p-3">
      <div className="w-full max-w-6xl rounded-md border border-border bg-card shadow-2xl">
        <div className="flex items-center justify-between border-b border-border px-4 py-3">
          <div>
            <div className="font-['Chakra_Petch'] text-lg font-semibold tracking-wide">重複解消プレビュー</div>
            <div className="text-xs text-muted-foreground">{group.key}</div>
          </div>
          <Button variant="ghost" onClick={onClose}>
            <X className="h-4 w-4" />
          </Button>
        </div>

        <div className="grid gap-4 p-4 lg:grid-cols-[320px_1fr]">
          <div className="rounded-md border border-border/70 bg-background/70 p-3">
            <div className="mb-2 text-sm font-semibold">残す譜面を選択</div>
            <div className="max-h-[52vh] space-y-2 overflow-auto pr-1">
              {group.charts.map((c) => (
                <button
                  key={c.chart_id}
                  type="button"
                  className={[
                    "w-full rounded-md border px-2 py-2 text-left",
                    c.chart_id === keep?.chart_id
                      ? "border-primary/70 bg-primary/10"
                      : "border-border/70 bg-card/80 hover:bg-accent/35",
                  ].join(" ")}
                  onClick={() => onKeepChange(c.chart_id)}
                >
                  <div className="text-sm font-medium">#{c.chart_id}</div>
                  <div className="truncate text-xs text-muted-foreground">{c.title || "(no title)"}</div>
                  <div className="truncate text-xs text-muted-foreground">{c.artist || "(no artist)"}</div>
                </button>
              ))}
            </div>
            <div className="mt-3 flex gap-2">
              <Button variant="outline" className="flex-1" onClick={onPreview} disabled={!keepId || !!loading}>
                プレビュー更新
              </Button>
              <Button
                variant="destructive"
                className="flex-1"
                onClick={onExecute}
                disabled={!preview || preview.cross_root || !!loading}
              >
                <Trash2 className="mr-1 h-4 w-4" />実行
              </Button>
            </div>
          </div>

          <div className="space-y-3">
            <div className="rounded-md border border-border/70 bg-background/70 p-3">
              <div className="mb-2 text-sm font-semibold">差分比較（基準: #{keep?.chart_id}）</div>
              <div className="max-h-[36vh] space-y-2 overflow-auto pr-1">
                {others.map((c) => (
                  <div key={c.chart_id} className="rounded-md border border-border/60 bg-card/70 p-2 text-xs">
                    <div className="mb-1 font-semibold">#{c.chart_id} と比較</div>
                    <div className="grid grid-cols-[80px_1fr_1fr] gap-2">
                      <div className="text-muted-foreground">項目</div>
                      <div className="text-muted-foreground">基準</div>
                      <div className="text-muted-foreground">比較先</div>

                      <div>title</div>
                      <div className="truncate">{keep?.title || "-"}</div>
                      <div className={`truncate ${diffClass(keep?.title, c.title)}`}>{c.title || "-"}</div>

                      <div>artist</div>
                      <div className="truncate">{keep?.artist || "-"}</div>
                      <div className={`truncate ${diffClass(keep?.artist, c.artist)}`}>{c.artist || "-"}</div>

                      <div>root_id</div>
                      <div>{keep?.root_id}</div>
                      <div className={keep?.root_id === c.root_id ? "text-muted-foreground" : "font-medium text-primary"}>{c.root_id}</div>

                      <div>path</div>
                      <div className="break-all">{keep?.full_path}</div>
                      <div className={`break-all ${diffClass(keep?.full_path, c.full_path)}`}>{c.full_path}</div>
                    </div>
                  </div>
                ))}
              </div>
            </div>

            {preview ? (
              <div className="rounded-md border border-border/70 bg-background/70 p-3 text-xs">
                <div>削除対象: {preview.remove_count}件</div>
                <div>root跨ぎ: {preview.cross_root ? "あり（実行不可）" : "なし"}</div>
                <div>
                  確認フレーズ: <code>{preview.confirmation_phrase}</code>
                </div>
                <div className="mt-2 max-h-28 overflow-auto pr-1">
                  {preview.operations.map((op) => (
                    <div key={op.source_path} className="mb-1 rounded border border-border/50 bg-card/70 p-2">
                      <div className="break-all">src: {op.source_path}</div>
                      <div className="break-all text-muted-foreground">backup: {op.backup_path}</div>
                    </div>
                  ))}
                </div>
              </div>
            ) : null}
          </div>
        </div>
      </div>
    </div>
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
        "bms-nav-button px-3 py-2 text-left text-sm transition",
        active ? "active" : "",
      ].join(" ")}
    >
      {children}
    </button>
  );
}

function StatusBar({ loading, scanStatus }: { loading: string | null; scanStatus: string }) {
  if (!loading) return null;
  const label =
    loading === "roots"
      ? "ルート取得中"
      : loading.startsWith("scan-")
        ? "スキャン中"
        : loading.startsWith("import-")
          ? "表取り込み中"
          : loading === "import-all"
            ? "表一括取り込み中"
            : loading === "dedupe-preview"
              ? "重複プレビュー"
              : loading === "dedupe-exec"
                ? "重複削除実行中"
                : "処理中";
  return (
    <div className="fixed bottom-3 left-1/2 z-30 -translate-x-1/2 rounded-md border border-border bg-card text-card-foreground shadow-xl shadow-black/20">
      <div className="flex items-center gap-3 px-4 py-2 text-sm font-medium">
        <div className="h-2 w-2 animate-pulse rounded-full bg-primary" />
        <div className="flex flex-col">
          <span>
            {label}
            {loading && loading !== label ? ` (${loading})` : ""}
          </span>
          {scanStatus ? <span className="text-[11px] opacity-80">{scanStatus}</span> : null}
        </div>
      </div>
    </div>
  );
}
