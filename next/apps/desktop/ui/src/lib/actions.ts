// One dispatcher for everything the shell does: menu shell:action events,
// command-palette entries and toolbar buttons all land here.
import { editor } from "./editor";
import { isChinese } from "./i18n";
import { api } from "./ipc";
import { exportCards, exportHtml, exportPdf, printDocument } from "./export";
import { replaceSourceText, sourceFind, toggleSource } from "./source";
import { openFind, openPalette, patchSettings, showPanel, togglePanel, ui } from "./store.svelte";

export interface Command {
  id: string;
  label: string;
  group: string;
  shortcut?: string;
}

const L = (zh: string, en: string) => (isChinese ? zh : en);

/** Every command the palette offers (ids match menu.rs). */
export const COMMANDS: Command[] = [
  { id: "file.new", label: L("新建文档", "New Document"), group: L("文件", "File"), shortcut: "⌘N" },
  { id: "file.open", label: L("打开…", "Open…"), group: L("文件", "File"), shortcut: "⌘O" },
  { id: "file.save", label: L("存储", "Save"), group: L("文件", "File"), shortcut: "⌘S" },
  { id: "file.saveAs", label: L("存储为…", "Save As…"), group: L("文件", "File"), shortcut: "⇧⌘S" },
  { id: "file.revert", label: L("恢复为已存储的版本", "Revert to Saved"), group: L("文件", "File") },
  { id: "file.exportHtml", label: L("导出 HTML…", "Export HTML…"), group: L("文件", "File"), shortcut: "⇧⌘E" },
  { id: "file.exportPdf", label: L("导出 PDF…", "Export PDF…"), group: L("文件", "File") },
  { id: "file.exportCards", label: L("导出为图片卡…", "Export Image Cards…"), group: L("文件", "File") },
  { id: "file.print", label: L("打印…", "Print…"), group: L("文件", "File"), shortcut: "⌘P" },
  { id: "file.frontMatter", label: L("编辑 Front Matter", "Edit Front Matter"), group: L("文件", "File") },
  { id: "edit.find", label: L("查找与替换", "Find and Replace"), group: L("编辑", "Edit"), shortcut: "⌘F" },
  { id: "view.search", label: L("在文件夹中搜索", "Search in Folder"), group: L("编辑", "Edit"), shortcut: "⇧⌘F" },
  { id: "view.projects", label: L("项目…", "Projects…"), group: L("显示", "View"), shortcut: "⇧⌘0" },
  { id: "view.sidebar", label: L("显示/隐藏侧栏", "Toggle Sidebar"), group: L("显示", "View"), shortcut: "⌃⌘S" },
  { id: "view.outline", label: L("大纲", "Outline"), group: L("显示", "View"), shortcut: "⌥⌘1" },
  { id: "view.files", label: L("文件树", "Files"), group: L("显示", "View"), shortcut: "⌥⌘2" },
  { id: "view.source", label: L("切换源码模式", "Toggle Source Mode"), group: L("显示", "View"), shortcut: "⌘/" },
  { id: "view.typewriter", label: L("切换打字机模式", "Toggle Typewriter Mode"), group: L("显示", "View"), shortcut: "⌥⌘T" },
  { id: "view.focus", label: L("切换专注模式", "Toggle Focus Mode"), group: L("显示", "View"), shortcut: "⌥⌘F" },
  { id: "view.spellcheck", label: L("切换拼写检查", "Toggle Spelling"), group: L("显示", "View") },
  { id: "view.zoomIn", label: L("放大", "Zoom In"), group: L("显示", "View"), shortcut: "⌘=" },
  { id: "view.zoomOut", label: L("缩小", "Zoom Out"), group: L("显示", "View"), shortcut: "⌘-" },
  { id: "view.zoomReset", label: L("实际大小", "Actual Size"), group: L("显示", "View"), shortcut: "⌃⌘0" },
  { id: "view.theme.frame", label: L("主题：画框", "Theme: Frame"), group: L("显示", "View") },
  { id: "view.theme.nord", label: L("主题：北欧", "Theme: Nord"), group: L("显示", "View") },
  { id: "view.theme.classic", label: L("主题：经典", "Theme: Classic"), group: L("显示", "View") },
  { id: "view.appearance.system", label: L("外观：跟随系统", "Appearance: System"), group: L("显示", "View") },
  { id: "view.appearance.light", label: L("外观：浅色", "Appearance: Light"), group: L("显示", "View") },
  { id: "view.appearance.dark", label: L("外观：深色", "Appearance: Dark"), group: L("显示", "View") },
  { id: "format.h1", label: L("标题 1", "Heading 1"), group: L("格式", "Format"), shortcut: "⌘1" },
  { id: "format.h2", label: L("标题 2", "Heading 2"), group: L("格式", "Format"), shortcut: "⌘2" },
  { id: "format.h3", label: L("标题 3", "Heading 3"), group: L("格式", "Format"), shortcut: "⌘3" },
  { id: "format.body", label: L("正文", "Body Text"), group: L("格式", "Format"), shortcut: "⌘0" },
  { id: "format.bullet", label: L("无序列表", "Bullet List"), group: L("格式", "Format"), shortcut: "⇧⌘U" },
  { id: "format.ordered", label: L("有序列表", "Numbered List"), group: L("格式", "Format"), shortcut: "⇧⌘O" },
  { id: "format.quote", label: L("引用", "Quote"), group: L("格式", "Format"), shortcut: "⇧⌘Q" },
  { id: "format.hr", label: L("分隔线", "Horizontal Rule"), group: L("格式", "Format"), shortcut: "⌥⌘-" },
  { id: "app.settings", label: L("设置…", "Settings…"), group: "Vellumi", shortcut: "⌘," },
];

const SHELL = new Set([
  "view.outline",
  "view.files",
  "view.search",
  "view.source",
  "view.sidebar",
  "edit.find",
  "edit.findNext",
  "edit.findPrev",
  "app.palette",
  "file.frontMatter",
  "file.revert",
  "file.exportHtml",
  "file.exportPdf",
  "file.exportCards",
  "file.print",
]);

export async function run(id: string): Promise<void> {
  if (SHELL.has(id)) {
    await shellAction(id);
  } else {
    await api.runCommand(id);
  }
}

export async function shellAction(id: string): Promise<void> {
  switch (id) {
    case "view.outline":
      return togglePanel("outline");
    case "view.files":
      return togglePanel("files");
    case "view.search":
      return showPanel("search");
    case "view.sidebar":
      return patchSettings({ sidebarVisible: !ui.settings?.sidebarVisible });
    case "view.source":
      return toggleSource();
    case "edit.find":
      if (!sourceFind("open")) openFind();
      return;
    case "edit.findNext":
    case "edit.findPrev": {
      const backwards = id === "edit.findPrev";
      if (sourceFind(backwards ? "previous" : "next")) return;
      if (!ui.findOpen) openFind();
      else ui.findResult = editor()?.searchStep(backwards) ?? ui.findResult;
      return;
    }
    case "app.palette":
      return openPalette("files");
    case "file.frontMatter":
      if (!ui.sourceMode) ui.frontMatterOpen = true;
      return;
    case "file.exportHtml":
      return exportHtml();
    case "file.exportPdf":
      return exportPdf();
    case "file.exportCards":
      await exportCards();
      return;
    case "file.print":
      return printDocument();
    case "file.revert": {
      const reverted = await api.revert();
      if (reverted && ui.sourceMode) {
        replaceSourceText(await api.sourceEnter(null));
      }
      return;
    }
  }
}
