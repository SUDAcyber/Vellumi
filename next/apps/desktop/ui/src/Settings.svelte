<script lang="ts">
  // Settings (⌘,): every persisted option, written through settings_patch so
  // Rust validates, saves and broadcasts to every window.
  import { onMount } from "svelte";

  import { isChinese } from "./lib/i18n";
  import { api, on, type Settings } from "./lib/ipc";

  const L = (zh: string, en: string) => (isChinese ? zh : en);

  type Section = "appearance" | "typography" | "editing" | "files" | "shortcuts";
  const sections: { id: Section; label: string }[] = [
    { id: "appearance", label: L("外观", "Appearance") },
    { id: "typography", label: L("排印", "Typography") },
    { id: "editing", label: L("编辑", "Editing") },
    { id: "files", label: L("文件与图片", "Files & Images") },
    { id: "shortcuts", label: L("快捷键", "Shortcuts") },
  ];

  let section = $state<Section>("appearance");
  let settings = $state<Settings | null>(null);
  let pending: ReturnType<typeof setTimeout> | undefined;
  let queued: Partial<Settings> = {};

  onMount(async () => {
    settings = await api.settings();
    on<Settings>("settings:changed", (next) => (settings = next));
  });

  /** Sliders fire per tick: apply locally at once, send at most ~10/s. */
  function set<K extends keyof Settings>(key: K, value: Settings[K]): void {
    if (!settings) return;
    settings[key] = value;
    queued[key] = value;
    clearTimeout(pending);
    pending = setTimeout(async () => {
      const patch = queued;
      queued = {};
      settings = await api.patchSettings(patch);
    }, 100);
  }

  const shortcuts: [string, string][] = [
    [L("新建 / 打开 / 快速打开", "New / Open / Quick Open"), "⌘N  ⌘O  ⇧⌘P"],
    [L("项目", "Projects"), "⇧⌘0"],
    [L("存储 / 存储为 / 关闭", "Save / Save As / Close"), "⌘S  ⇧⌘S  ⌘W"],
    [L("查找替换 / 下一个 / 上一个", "Find / Next / Previous"), "⌘F  ⌘G  ⇧⌘G"],
    [L("在文件夹中搜索", "Search in Folder"), "⇧⌘F"],
    [L("正文 / 标题 1–6", "Body / Heading 1–6"), "⌘0  ⌘1–⌘6"],
    [L("粗体 / 斜体 / 行内代码 / 链接", "Bold / Italic / Code / Link"), "⌘B  ⌘I  ⌘E  ⌘K"],
    [L("删除线", "Strikethrough"), "⇧⌘X"],
    [L("无序 / 有序列表 / 引用", "Bullets / Numbers / Quote"), "⇧⌘U  ⇧⌘O  ⇧⌘Q"],
    [L("分隔线", "Horizontal Rule"), "⌥⌘-"],
    [L("大纲 / 文件树 / 侧栏", "Outline / Files / Sidebar"), "⌥⌘1  ⌥⌘2  ⌃⌘S"],
    [L("源码模式", "Source Mode"), "⌘/"],
    [L("打字机 / 专注模式", "Typewriter / Focus"), "⌥⌘T  ⌥⌘F"],
    [L("放大 / 缩小 / 实际大小", "Zoom In / Out / Actual"), "⌘=  ⌘-  ⌃⌘0"],
    [L("设置", "Settings"), "⌘,"],
  ];

  const fonts: Record<string, string> = {
    default: '-apple-system, "PingFang SC", sans-serif',
    serif: 'Georgia, "Songti SC", serif',
    sans: '-apple-system, "Helvetica Neue", "PingFang SC", sans-serif',
  };
</script>

<div class="window">
  <nav aria-label={L("设置分类", "Settings sections")}>
    {#each sections as item (item.id)}
      <button class="nav" aria-current={section === item.id ? "page" : undefined} onclick={() => (section = item.id)}>{item.label}</button>
    {/each}
  </nav>

  {#if settings}
    <main>
      {#if section === "appearance"}
        <h1>{L("外观", "Appearance")}</h1>
        <div class="group">
          <div class="item">
            <span class="label">{L("外观", "Appearance")}</span>
            <div class="segmented" role="radiogroup" aria-label={L("外观", "Appearance")}>
              {#each [["system", L("跟随系统", "System")], ["light", L("浅色", "Light")], ["dark", L("深色", "Dark")]] as [value, label] (value)}
                <button role="radio" aria-checked={settings.appearance === value} onclick={() => set("appearance", value as Settings["appearance"])}>{label}</button>
              {/each}
            </div>
          </div>
          <div class="item">
            <span class="label">{L("编辑器主题", "Editor Theme")}<span class="hint">{L("浅色与深色各有一套", "Each has a light and a dark variant")}</span></span>
            <div class="segmented" role="radiogroup" aria-label={L("编辑器主题", "Editor Theme")}>
              {#each [["frame", L("画框", "Frame")], ["nord", L("北欧", "Nord")], ["classic", L("经典", "Classic")]] as [value, label] (value)}
                <button role="radio" aria-checked={settings.theme === value} onclick={() => set("theme", value as Settings["theme"])}>{label}</button>
              {/each}
            </div>
          </div>
          <div class="item">
            <label class="label" for="zoom">{L("缩放", "Zoom")}</label>
            <input id="zoom" type="range" min="0.5" max="3" step="0.05" value={settings.zoom} oninput={(e) => set("zoom", Number(e.currentTarget.value))} />
            <span class="value">{Math.round(settings.zoom * 100)}%</span>
          </div>
        </div>
      {:else if section === "typography"}
        <h1>{L("排印", "Typography")}</h1>
        <div class="split">
          <div class="group">
            <div class="item">
              <span class="label">{L("字体方案", "Font")}<span class="hint">{L("“默认”使用主题自带字体", "Default keeps the theme's font")}</span></span>
              <div class="segmented" role="radiogroup" aria-label={L("字体方案", "Font")}>
                {#each [["default", L("默认", "Default")], ["serif", L("衬线", "Serif")], ["sans", L("无衬线", "Sans")]] as [value, label] (value)}
                  <button role="radio" aria-checked={settings.fontScheme === value} onclick={() => set("fontScheme", value as Settings["fontScheme"])}>{label}</button>
                {/each}
              </div>
            </div>
            <div class="item">
              <label class="label" for="lh">{L("行距", "Line Height")}</label>
              <input id="lh" type="range" min="1.2" max="2.2" step="0.1" value={settings.lineHeight || 1.7} oninput={(e) => set("lineHeight", Number(e.currentTarget.value))} />
              <span class="value">{settings.lineHeight ? settings.lineHeight.toFixed(1) : L("主题默认", "Theme")}</span>
            </div>
            <div class="item">
              <label class="label" for="ps">{L("段间距", "Paragraph Spacing")}</label>
              <input id="ps" type="range" min="0.2" max="2" step="0.1" value={settings.paragraphSpacing || 1} oninput={(e) => set("paragraphSpacing", Number(e.currentTarget.value))} />
              <span class="value">{settings.paragraphSpacing ? `${settings.paragraphSpacing.toFixed(1)} em` : L("主题默认", "Theme")}</span>
            </div>
            <div class="item">
              <label class="label" for="lw">{L("行宽", "Line Width")}<span class="hint">{L("最右端为自适应", "Far right = adaptive")}</span></label>
              <input id="lw" type="range" min="560" max="1100" step="20" value={settings.lineWidth || 1100} oninput={(e) => { const v = Number(e.currentTarget.value); set("lineWidth", v > 1080 ? 0 : v); }} />
              <span class="value">{settings.lineWidth ? `${settings.lineWidth} px` : L("自适应", "Adaptive")}</span>
            </div>
            <div class="item">
              <span class="label"></span>
              <button class="button" onclick={() => { set("fontScheme", "default"); set("lineHeight", 0); set("paragraphSpacing", 0); set("lineWidth", 0); }}>{L("恢复主题默认", "Reset to Theme")}</button>
            </div>
          </div>
          <section class="preview" aria-label={L("预览", "Preview")}>
            <div style:font-family={fonts[settings.fontScheme]} style:line-height={settings.lineHeight || 1.8}>
              <h2>{L("一天的节奏", "The rhythm of a day")}</h2>
              <p style:margin-bottom="{settings.paragraphSpacing || 1}em">{L("一天的节奏，比一天的产量更重要。把写和改拆成两个时段：上午只写，下午只改。", "A day's rhythm matters more than its word count. Write in the morning, edit in the afternoon.")}</p>
              <p>{L("Slow writing is not about speed. It is about rhythm.", "The draft is allowed to be bad; the schedule is not.")}</p>
            </div>
          </section>
        </div>
      {:else if section === "editing"}
        <h1>{L("编辑", "Editing")}</h1>
        <div class="group">
          {#each [["smartPunctuation", L("智能标点", "Smart Punctuation"), L("输入时转为弯引号、省略号、破折号；切换会重建编辑器", "Curly quotes, ellipses and dashes while typing")], ["typewriter", L("打字机模式", "Typewriter Mode"), L("光标行保持在窗口中部 ⌥⌘T", "Keeps the caret line centered ⌥⌘T")], ["focusMode", L("专注模式", "Focus Mode"), L("淡化光标所在段落以外的内容 ⌥⌘F", "Dims everything but the current block ⌥⌘F")], ["spellcheck", L("拼写检查", "Check Spelling"), ""]] as [key, label, hint] (key)}
            <label class="item">
              <span class="label">{label}{#if hint}<span class="hint">{hint}</span>{/if}</span>
              <input type="checkbox" role="switch" checked={settings[key as "typewriter"]} onchange={(e) => set(key as "typewriter", e.currentTarget.checked)} />
            </label>
          {/each}
          <div class="item">
            <label class="label" for="goal">{L("字数目标", "Word Goal")}<span class="hint">{L("状态栏显示完成度；0 为关闭", "Shown in the status bar; 0 turns it off")}</span></label>
            <input id="goal" class="number" type="number" min="0" step="100" value={settings.wordGoal} onchange={(e) => set("wordGoal", Math.max(0, Math.round(Number(e.currentTarget.value) || 0)))} />
          </div>
        </div>
      {:else if section === "files"}
        <h1>{L("文件与图片", "Files & Images")}</h1>
        <div class="group">
          <div class="item">
            <label class="label" for="images">{L("图片文件夹", "Image Folder")}<span class="hint">{L("粘贴或拖入的图片存到文档旁的这个文件夹", "Pasted images go to this folder next to the document")}</span></label>
            <input id="images" class="text" value={settings.imageFolder} onchange={(e) => set("imageFolder", e.currentTarget.value)} />
          </div>
          <div class="item">
            <span class="label"
              >{L("草稿文件夹", "Drafts Folder")}<span class="hint"
                >{settings.draftsFolder
                  ? settings.draftsFolder.replace(/^\/Users\/[^/]+/, "~")
                  : L("未设置：⌘N 新建的是未存储的文档", "Not set: ⌘N creates an unsaved document")}</span
              ></span
            >
            {#if settings.draftsFolder}
              <button class="button" onclick={async () => (settings = await api.pickDrafts(true))}>{L("关闭", "Turn Off")}</button>
            {/if}
            <button class="button" onclick={async () => (settings = await api.pickDrafts(false))}>{L("选择…", "Choose…")}</button>
          </div>
        </div>
        <p class="footnote">
          {L(
            "设置草稿文件夹后，⌘N 会直接在那里建一个 .md 文件并自动保存；什么都没写就关掉的话，空文件会被删除。",
            "With a Drafts folder, ⌘N creates a real .md there that autosaves; closing it untouched removes the empty file.",
          )}
        </p>
      {:else}
        <h1>{L("快捷键", "Shortcuts")}</h1>
        <div class="group">
          {#each shortcuts as [label, keys] (label)}
            <div class="item"><span class="label">{label}</span><span class="keys">{keys}</span></div>
          {/each}
        </div>
      {/if}
    </main>
  {/if}
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
    width: 190px;
    flex: none;
    padding: 16px 10px;
    border-right: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .nav {
    height: 32px;
    padding: 0 10px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--ink2);
    font-size: 13px;
    text-align: left;
    cursor: pointer;
  }
  .nav:hover {
    background: var(--hover);
  }
  .nav[aria-current="page"] {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 500;
  }
  main {
    flex: 1;
    min-width: 0;
    padding: 22px 26px;
  }
  h1 {
    margin: 0 0 16px;
    font-size: 19px;
  }
  .split {
    display: flex;
    gap: 20px;
    flex-wrap: wrap;
  }
  .split > .group {
    flex: 1 1 360px;
  }
  .group {
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 12px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 14px;
    min-height: 52px;
    padding: 8px 16px;
    border-top: 1px solid var(--line);
  }
  .item:first-child {
    border-top: 0;
  }
  .label {
    flex: 1;
    min-width: 0;
    color: var(--ink);
  }
  .hint {
    display: block;
    margin-top: 2px;
    color: var(--muted);
    font-size: 12px;
  }
  .value {
    width: 72px;
    text-align: right;
    color: var(--ink2);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  input[type="range"] {
    width: 170px;
    accent-color: var(--accent);
  }
  input[type="checkbox"] {
    width: 18px;
    height: 18px;
    accent-color: var(--accent);
  }
  .number,
  .text {
    width: 120px;
    height: 30px;
    padding: 0 8px;
    border: 1px solid var(--line);
    border-radius: 7px;
    background: var(--surface);
    color: var(--ink);
    font-size: 13px;
    text-align: right;
  }
  .segmented {
    display: flex;
    gap: 2px;
    padding: 3px;
    border-radius: 9px;
    background: var(--hover);
  }
  .segmented button {
    height: 26px;
    padding: 0 12px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--ink2);
    font-size: 12px;
    cursor: pointer;
  }
  .segmented button[aria-checked="true"] {
    background: var(--surface);
    color: var(--ink);
    font-weight: 500;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.1);
  }
  .footnote {
    margin: 10px 4px 0;
    color: var(--muted);
    font-size: 12px;
    line-height: 1.6;
  }
  .keys {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--ink2);
    white-space: pre;
  }
  .preview {
    flex: 1 1 280px;
    padding: 24px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--surface);
    color: var(--ink);
    font-size: 15px;
  }
  .preview h2 {
    margin: 0 0 12px;
    font-size: 20px;
  }
</style>
