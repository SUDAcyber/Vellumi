<script lang="ts">
  // Projects (⇧⌘0), the workspace screen from the design canvas: project
  // cards with their latest files, recents, and search across every project.
  import { onMount } from "svelte";

  import Icon from "./components/Icon.svelte";
  import { isChinese } from "./lib/i18n";
  import { api, errorMessage, on, type FileMatches, type IndexStatus, type ProjectInfo, type Settings } from "./lib/ipc";

  const L = (zh: string, en: string) => (isChinese ? zh : en);

  let view = $state<"projects" | "recents">("projects");
  let projects = $state<ProjectInfo[]>([]);
  let settings = $state<Settings | null>(null);
  let index = $state<IndexStatus | null>(null);
  let query = $state("");
  let results = $state<FileMatches[] | null>(null);
  let searching = $state(false);
  let renaming = $state<string | null>(null);
  let renameText = $state("");
  let failure = $state<string | null>(null);

  const totalFiles = $derived(projects.reduce((sum, p) => sum + p.fileCount, 0));
  const home = (path: string) => path.replace(/^\/Users\/[^/]+/, "~");
  const fileName = (path: string) => path.split("/").at(-1) ?? path;
  const parentName = (path: string) => path.split("/").at(-2) ?? "";

  function ago(ms: number): string {
    if (!ms) return "";
    const minutes = Math.round((Date.now() - ms) / 60000);
    if (minutes < 1) return L("刚刚", "just now");
    if (minutes < 60) return L(`${minutes} 分钟前`, `${minutes} min ago`);
    const hours = Math.round(minutes / 60);
    if (hours < 24) return L(`${hours} 小时前`, `${hours} h ago`);
    const days = Math.round(hours / 24);
    if (days === 1) return L("昨天", "yesterday");
    if (days < 7) return L(`${days} 天前`, `${days} days ago`);
    const date = new Date(ms);
    return isChinese ? `${date.getMonth() + 1} 月 ${date.getDate()} 日` : date.toLocaleDateString();
  }

  async function refresh(): Promise<void> {
    try {
      [projects, settings, index] = await Promise.all([api.projects(), api.settings(), api.indexStatus()]);
    } catch (error) {
      failure = errorMessage(error);
    }
  }

  onMount(() => {
    void refresh();
    on<Settings>("settings:changed", (next) => {
      settings = next;
      void api.projects().then((list) => (projects = list));
    });
    on<IndexStatus>("index:status", (status) => (index = status));
    // Files change underneath us; refresh whenever the window comes back.
    window.addEventListener("focus", () => void refresh());
  });

  async function add(): Promise<void> {
    try {
      await api.addProject();
      await refresh();
    } catch (error) {
      failure = errorMessage(error);
    }
  }

  async function runSearch(): Promise<void> {
    const text = query.trim();
    if (!text) {
      results = null;
      return;
    }
    searching = true;
    try {
      results = await api.search(text, false);
    } catch (error) {
      failure = errorMessage(error);
    } finally {
      searching = false;
    }
  }

  async function commitRename(id: string): Promise<void> {
    await api.renameProject(id, renameText);
    renaming = null;
    await refresh();
  }

  const indexLabel = $derived.by(() => {
    if (!index) return "";
    if (index.state === "ready") return L(`索引就绪 · ${index.files} 个文件`, `Index ready · ${index.files} files`);
    if (index.state === "indexing" || index.state === "starting") return L("正在建立索引…", "Indexing…");
    return L("索引不可用 · 使用逐个扫描", "Index unavailable · scanning files");
  });
</script>

<div class="window">
  <nav aria-label={L("工作区", "Workspace")}>
    <button class="nav" aria-current={view === "projects" ? "page" : undefined} onclick={() => (view = "projects")}>
      <Icon name="folder" size={16} />{L("全部项目", "All Projects")}
    </button>
    <button class="nav" aria-current={view === "recents" ? "page" : undefined} onclick={() => (view = "recents")}>
      <Icon name="file" size={16} />{L("最近打开", "Recent")}
    </button>
    <div class="spacer"></div>
    <p class="note">
      {L("项目只是对文件夹的引用，文件始终是你磁盘上的普通 .md。", "A project is just a reference to a folder; your files stay plain .md on disk.")}
    </p>
    {#if settings?.draftsFolder}
      <p class="note">{L("草稿文件夹：", "Drafts: ")}{home(settings.draftsFolder)}</p>
    {/if}
  </nav>

  <main>
    <header>
      <h1>{view === "projects" ? L("全部项目", "All Projects") : L("最近打开", "Recent")}</h1>
      {#if view === "projects"}
        <span class="muted">{L(`${projects.length} 个项目 · ${totalFiles} 个文件`, `${projects.length} projects · ${totalFiles} files`)}</span>
      {/if}
      <span class="spacer"></span>
      <button class="button primary add" onclick={add}><Icon name="plus" size={14} />{L("添加文件夹", "Add Folder")}</button>
    </header>

    {#if failure}<p class="error" role="alert">{failure}</p>{/if}

    <div class="search">
      <Icon name="search" size={15} />
      <label class="visually-hidden" for="project-search">{L("在所有项目中搜索", "Search all projects")}</label>
      <input
        id="project-search"
        bind:value={query}
        placeholder={L("在所有项目中搜索正文，回车开始", "Search text in every project, then Return")}
        onkeydown={(e) => e.key === "Enter" && runSearch()}
      />
      <span class="muted small">{searching ? "…" : indexLabel}</span>
    </div>

    {#if results}
      <section class="results" aria-label={L("搜索结果", "Results")}>
        {#if results.length === 0}
          <p class="muted">{L("没有找到匹配", "No matches")}</p>
        {/if}
        {#each results as file (file.path)}
          <button class="hit" onclick={() => api.openPath(file.path)} title={file.path}>
            <span class="hit-name">{file.name} <span class="muted">· {parentName(file.path)} · {file.matches.length}</span></span>
            {#each file.matches.slice(0, 2) as match (match.line)}
              <span class="hit-line"><span class="line-no">L{match.line}</span> {match.text}</span>
            {/each}
          </button>
        {/each}
      </section>
    {/if}

    {#if view === "projects"}
      {#if projects.length === 0}
        <div class="empty">
          <p>{L("还没有项目。添加一个文件夹，就能在 ⇧⌘P 里跨文件夹快速打开、在这里全文搜索。", "No projects yet. Add a folder to quick-open (⇧⌘P) and search across it.")}</p>
          <button class="button primary" onclick={add}>{L("添加文件夹", "Add Folder")}</button>
        </div>
      {:else}
        <div class="grid">
          {#each projects as project (project.id)}
            <article class="card" class:missing={!project.available}>
              <div class="card-head">
                <div class="card-title">
                  {#if renaming === project.id}
                    <input
                      class="rename"
                      bind:value={renameText}
                      aria-label={L("项目名称", "Project name")}
                      onkeydown={(e) => {
                        if (e.key === "Enter") void commitRename(project.id);
                        if (e.key === "Escape") renaming = null;
                      }}
                      onblur={() => commitRename(project.id)}
                    />
                  {:else}
                    <h2 title={L("双击重命名", "Double-click to rename")} ondblclick={() => ((renaming = project.id), (renameText = project.name))}>{project.name}</h2>
                  {/if}
                  <div class="muted small" title={project.path}>{home(project.path)}</div>
                </div>
                {#if project.available}
                  <button class="button" onclick={() => api.newNote(project.id)}>{L("新建笔记", "New Note")}</button>
                {/if}
              </div>
              {#if project.available}
                <ul class="files">
                  {#each project.recent as file (file.path)}
                    <li>
                      <button class="file" onclick={() => api.openPath(file.path)} title={file.path}>
                        <span class="file-name">{file.name}</span><span class="muted small">{ago(file.modifiedMs)}</span>
                      </button>
                    </li>
                  {/each}
                </ul>
                <div class="card-foot">
                  <span class="muted small">{L(`${project.fileCount} 个文件 · 含子文件夹`, `${project.fileCount} files · incl. subfolders`)}</span>
                  <span class="spacer"></span>
                  <button class="icon-button" title={L("在 Finder 中显示", "Reveal in Finder")} aria-label={L("在 Finder 中显示", "Reveal in Finder")} onclick={() => api.reveal(project.path)}><Icon name="reveal" size={14} /></button>
                  <button class="icon-button" title={L("移除项目（不删除文件）", "Remove project (files stay)")} aria-label={L("移除项目", "Remove project")} onclick={() => api.removeProject(project.id).then(refresh)}><Icon name="close" size={14} /></button>
                </div>
              {:else}
                <p class="missing-text">{L("找不到这个文件夹。磁盘可能未挂载，或文件夹已移动。", "This folder can't be found. The disk may be unmounted, or the folder moved.")}</p>
                <div class="card-foot">
                  <button class="button" onclick={() => api.relocateProject(project.id).then(refresh)}>{L("重新定位…", "Locate…")}</button>
                  <button class="button" onclick={() => api.removeProject(project.id).then(refresh)}>{L("移除", "Remove")}</button>
                </div>
              {/if}
            </article>
          {/each}
        </div>
      {/if}
    {/if}

    {#if view === "recents" || projects.length > 0}
      <section class="recents">
        <h3>{L("最近打开", "Recent")}</h3>
        {#if !settings?.recents.length}
          <p class="muted">{L("还没有最近打开的文档。", "Nothing opened yet.")}</p>
        {:else}
          <div class="recent-list">
            {#each settings.recents.slice(0, view === "recents" ? 20 : 6) as path (path)}
              <button class="recent" onclick={() => api.openPath(path)} title={path}>
                <span class="file-name">{fileName(path)}</span>
                <span class="muted small">{home(path.slice(0, -fileName(path).length - 1))}</span>
              </button>
            {/each}
          </div>
        {/if}
      </section>
    {/if}
  </main>
</div>

<style>
  :global(body) {
    background: var(--chrome);
  }
  .window {
    display: flex;
    min-height: 100vh;
    font-size: 13px;
  }
  nav {
    width: 210px;
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 14px 10px;
    border-right: 1px solid var(--line);
  }
  .nav {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 34px;
    padding: 0 10px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--ink2);
    font-size: 13px;
    cursor: pointer;
    text-align: left;
  }
  .nav:hover {
    background: var(--hover);
  }
  .nav[aria-current="page"] {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 500;
  }
  .note {
    margin: 4px 0;
    padding: 10px;
    border-radius: 8px;
    background: var(--hover);
    color: var(--ink2);
    font-size: 12px;
    line-height: 1.55;
  }
  main {
    flex: 1;
    min-width: 0;
    padding: 22px 28px 40px;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  h1 {
    margin: 0;
    font-size: 22px;
  }
  .spacer {
    flex: 1;
  }
  .muted {
    color: var(--muted);
  }
  .small {
    font-size: 12px;
  }
  .add {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 32px;
  }
  .error {
    margin: 0;
    color: #d9534f;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 12px;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--surface);
    color: var(--muted);
  }
  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--ink);
    font-size: 13px;
  }
  .results {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 6px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--surface);
    max-height: 320px;
    overflow: auto;
  }
  .hit {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 8px 10px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--ink);
    text-align: left;
    cursor: pointer;
  }
  .hit:hover {
    background: var(--hover);
  }
  .hit-name {
    font-weight: 500;
  }
  .hit-line {
    font-size: 12px;
    color: var(--ink2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .line-no {
    font-family: var(--mono);
    color: var(--muted);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 16px;
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--surface);
    min-height: 168px;
  }
  .card:hover {
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.06);
  }
  .card.missing {
    background: var(--warn-bg);
    border-color: var(--warn-line);
  }
  .card-head {
    display: flex;
    align-items: flex-start;
    gap: 8px;
  }
  .card-title {
    flex: 1;
    min-width: 0;
  }
  h2 {
    margin: 0 0 4px;
    font-size: 15px;
    font-weight: 600;
    cursor: default;
  }
  .rename {
    width: 100%;
    height: 26px;
    margin-bottom: 4px;
    padding: 0 6px;
    border: 1px solid var(--accent);
    border-radius: 6px;
    background: var(--surface);
    color: var(--ink);
    font-size: 14px;
  }
  .files {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .file,
  .recent {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    min-height: 28px;
    padding: 0 6px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--ink2);
    text-align: left;
    cursor: pointer;
  }
  .file:hover,
  .recent:hover {
    background: var(--hover);
  }
  .file-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ink);
  }
  .card-foot {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: auto;
  }
  .missing-text {
    margin: 0;
    color: var(--warn-ink);
    line-height: 1.6;
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 12px;
    padding: 24px;
    border: 1px dashed var(--line);
    border-radius: 12px;
    color: var(--ink2);
  }
  .empty p {
    margin: 0;
  }
  .recents h3 {
    margin: 0 0 8px;
    font-size: 13px;
    font-weight: 600;
    color: var(--muted);
  }
  .recent-list {
    padding: 6px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--surface);
  }
  .recent {
    min-height: 40px;
    padding: 0 10px;
  }
</style>
