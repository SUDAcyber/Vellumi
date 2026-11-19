# Vellumi 重写计划：Swift/AppKit → Tauri 2（Rust + TypeScript）

> 状态：**已确认，按建议执行**（2026-11-09）。代码在 `next/` 目录。

## 决策记录（2026-11-09，用户：“按照你的想法直接开干”）

| # | 结论 |
|---|---|
| D1 | 开发期 bundle id 用 `com.aaron.vellumi.next`，P7 再切回 `com.aaron.vellumi` |
| D2 | 去掉沙盒 |
| D3 | 首发用原生窗口标签（一个文档一个窗口）；Web 标签栏推迟到跨平台阶段 |
| D4 | 新增 ⇧⌘F 全文搜索 |
| D5 | Versions 改为自建快照（App 数据目录，每个文件保留最近 20 个版本）；去掉标题栏代理图标，改由命令面板提供重命名、移动、在 Finder 中显示 |
| D6 | 保存时保持原编码（UTF-8 / UTF-8 BOM / UTF-16LE / UTF-16BE） |
| D7 | 设置单向迁移，旧 plist 只读 |
| D8 | 外壳 UI 用 Svelte 5 + TypeScript |
| D9 | 查找改为 ProseMirror 内搜索 |
| D10 | 重写时直接修掉现存缺陷；.txt 正式支持打开（`decodeText` 本来就能处理） |
| D11 | PDF / 打印改为离屏渲染清理后的 HTML |
| D12 | 继续 ad-hoc 签名（目前没有 Developer ID）；架构上保留公证所需的 hardened runtime 开关 |
| D13 | 设计稿便签里的改动全部接受，但“窗口内标签栏”按 D3 推迟 |

## 进度（2026-11-09）

- **P1（部分完成）**：`vellumi-core` 已实现编码（含 D6 按原编码回写）、front matter、文档会话（脏标记、规范化基线、源码模式、A9 崩溃恢复修复）、字数统计（B20 和 CRLF 段落计数已修）、模糊匹配、资源路径与防穿越。`vellumi-fs` 已实现限量读取和原子写入。共 59 个测试，clippy `-D warnings` 无告警，代码里没有 unwrap。
- **P0.1 ✅**：Tauri 2.12 外壳加载同一份 Crepe 包。bridge-shim 把桥接改成通过 Tauri IPC 传递，编辑器只改动了传输方式和资源 URL 前缀两处。实测：打开文档后 ready → setMarkdown → normalized 往返正常，front matter 没进入编辑器；只打开不编辑时，磁盘上的文件逐字节不变。
- **已写但还没在界面上实测**：P0.2 菜单与快捷键路由、P0.3 原生标签（`tabbing_identifier`）、P0.4 `vellumi-asset://` 图片协议，以及保存/另存为、2 秒自动保存、关闭握手（3 秒超时兜底）、⌘Q 逐个关闭文档。界面控制权限被拒，这几项需要人工点一遍。
- **新发现（Swift 0.5.3 同样存在，因为是同一版 Crepe）**：规范化会**改变内容**，而不只是统一写法：
  - `![图标](…)` 的 alt 文本被改写成 `1.00`（图片块把缩放比例写进了 alt）
  - `$$E=mc^2$$` 块级公式被改成行内 `$E=mc^2$`
  - 紧凑列表变成松散列表（列表项之间插入空行）

  打开文件本身不会写盘，但只要编辑并保存一次，上面这些就会写进文件。这与硬性要求 1 冲突，列为 **P2 的阻断项**：需要给 Crepe 的序列化打补丁（保留 alt、保留块级公式），并补一组保真语料测试。

### P7 迁移与发布（2026-11-19）

- **旧版设置迁移**：`crates/vellumi-legacy` 只读地读取 0.5.x 的 UserDefaults plist，映射成新的设置（D7，单向迁移）。
  - 映射范围：外观、主题、缩放、排印、编辑选项、字数目标、图片文件夹、统计方式、侧栏状态（两个独立侧栏合并为带面板的一个），以及项目和草稿文件夹。
  - 项目和草稿文件夹原本存成书签，用 `NSURL resourceValuesForKeys:fromBookmarkData:` 直接读出书签里的路径，不解析书签、不需要沙盒权限；已用 Foundation 生成的真实书签验证，中文加空格的路径也能读出。
  - 只在新版第一次启动、还没有 `settings.json` 时迁移一次，迁移后显示一条提示。
  - 曾授权过的图片文件夹只作记录，不会自动变成项目。
  - 测试：5 个单元测试。另对你本机的真实配置用 `vellumi-desktop --legacy-report` 做了只读演练，结果符合预期，旧 plist 的修改时间和大小都没变。
- **发布配置**：
  - `tauri.release.conf.json`：Vellumi / `com.aaron.vellumi` / 0.6.0，打包 DMG，ad-hoc 签名。开发期仍用 `.next`（D1）。
  - `scripts/release.sh`：先过全部测试闸门，再构建 Universal 版本，校验 bundle id、版本号、架构和签名，最后用发布版二进制跑完整自测作为冒烟测试。**不做任何发布。**
- **自动更新**：用 `--features updater` 编译开关控制，并新增“先走完保存流程、再重启”的退出路径，安装更新时不会丢内容。需要你先生成签名密钥才会启用，步骤见 `docs/RELEASE.md`。
- **首个正式构建**：Vellumi 0.6.0。通过了全部测试闸门：
  - Rust / clippy / JS / 类型检查
  - 保真测试 6/6
  - 自测 64/64（debug 版）
  - 发布版二进制冒烟测试 64/64

  产物是 Universal（x86_64 + arm64）、ad-hoc 签名：
  - `next/dist/Vellumi-0.6.0.dmg`（20 MB，sha256 `174384b6…af0cb8`）
  - `Vellumi-0.6.0.app.zip`（18 MB）

  体积比 0.5.3 的 7 MB 大，主要是 mermaid 和 KaTeX 被打包进了界面产物，可以作为后续优化项。
- **构建中发现并修好的问题**：
  - macOS 自带的 bash 3.2 展开空数组会报错
  - Tauri 的 DMG 脚本要通过 AppleScript 控制 Finder，非交互环境下失败，改用 `hdiutil` 直接制作
  - 测试工具的竞态：多份文档同时完成时，进程提前退出导致输出文件缺失或被截断，压测 180 次无复现
  - 自测中的图片卡受到用户记住的选项（JPEG）干扰，以及文件树检查的时序问题
  - 深色模式下正文一栏单独发黑：Crepe 主题背景覆盖了透明设置
- **需要你本人做的事**（`docs/RELEASE.md`）：生成更新签名密钥、Sparkle → Tauri 的交接（必须先实机验证，即 P0.7）、上传 GitHub Release、更新 Homebrew cask。

### P6 导出（2026-11-19，已完成）

| 功能 | 做法 |
|---|---|
| 导出 HTML（⇧⌘E） | 生成自包含页面：[toc] 展开成带锚点的目录；样式全部内联；文档有公式时把 KaTeX 字体转成 data URI 内联，没有公式时删掉字体声明（审计 B21） |
| 导出 PDF | 在隐藏窗口里渲染清理后的页面，再调用 WebKit 原生 `createPDF`（D11）。不再截取编辑界面，所以长代码块不丢行、专注模式下不会变灰（审计 A10）；页面固定白底 |
| 打印（⌘P） | 打开预览窗口后弹出系统打印面板，可分页，也可在面板里另存为 PDF |
| 图片卡 | 移植 0.5.x 的 `card.html`（6 主题 / 3 尺寸 / PNG·JPEG / 缩进 / 水印 / 文件夹或 ZIP），放进新窗口，界面按设计稿；选项会记住 |

**修掉的缺陷**：
- **A4**：两次分页同时进行、互相干扰，长文末尾被裁。现在渲染排队串行执行，截图前先等排队中的渲染完成。
- **B2**：ZIP 里的中文文件名在 Windows 上乱码。现在设置了 UTF-8 文件名标志。
- **新发现**：导出的正文里混进了编辑器的光标元素（第一张图片卡因此是空白页），还可能带进查找高亮、选中框。现在全部清理掉。
- 在源码模式下导出时，先退出源码模式，避免导出旧内容（0.5.x 是直接禁用这几个导出命令）。
- 图片卡里去掉 `[toc]` 标记段落。

**验证**：
- 自测 **63/63**，新增覆盖：
  - HTML：标题、目录、代码块、字体内联、不含编辑状态
  - PDF：文件头为 `%PDF`
  - 图片卡：实际渲染并存成 PNG，多张时编号正确
- 另外人工打开生成的 PDF 和卡片 PNG 看过内容。
- 测试时的卡片窗口放在屏幕外：卡片分页依赖 `requestAnimationFrame`，隐藏窗口不会触发它（0.5.x 的测试也是这么处理的）。

### P4 / P5（2026-11-09，已完成）

- **P4 图片**：自测走的是真实的粘贴代码路径。已验证：
  - 图片保存到 `assets/`，文件名经过清洗、按百分号编码写进 Markdown，例如 `截图 测试.png` 会保存为 `截图-测试.png`
  - 同名文件自动加 `-1`
  - 通过 `vellumi-asset://` 显示
  - `../` 越界路径被拒绝
- **P5 项目**：
  - 项目模型：`settings.projects`，只存路径，不用书签（D2）
  - 项目窗口（⇧⌘0，按设计稿）：项目卡片显示最近 3 个文件、文件数；提供新建笔记、双击重命名、在 Finder 中显示、移除；文件夹不存在时可重新定位
  - 最近打开的文档列表
  - 跨项目全文搜索
- **全文索引**：`crates/vellumi-index`（tantivy 0.27）
  - 按单字符切分，用短语查询实现子串语义，中英文都能搜到，命中后再对原文逐行确认
  - 索引放在缓存目录，可随时删除，损坏时自动重建
  - 启动时在后台增量同步；项目目录改动由 notify 递归监听，实时更新
  - 索引建好之前，以及项目之外的当前文件夹，回退到逐个扫描
  - 性能数据见 `docs/PERF.md`：1 万篇笔记，查询 p95 35 µs，逐个扫描 224 ms
- **快速打开 / 侧栏搜索 / 命令面板全文模式**：范围扩大到“所有项目（含子文件夹）+ 当前文件夹 + 最近打开”，修掉审计 B10。路径统一按真实路径去重，`/var` 和 `/private/var` 这类软链不会重复。
- **草稿文件夹**（设置 ▸ 文件）：设置后，⌘N 直接在该文件夹创建文件；新建笔记一个字没写就关闭，空文件会被删除（审计 B18）。
- **验证**：自测 **54/54**，保真测试 6/6，Rust 测试 78 个。

### P2 / P3（2026-11-09，已完成）

**P2 文档闭环**：

| 功能 | 实现位置 |
|---|---|
| 源码模式 ⌘/ | CodeMirror 6，显示含 front matter 的全文，带行号、Markdown 着色、自带查找面板；进出时保留滚动位置；源码模式下 ⌘Z 交给 CodeMirror |
| 外部改动 | `watcher.rs`：notify 监听文件所在目录，用指纹排除自己的写入。干净文档静默重载；有未保存改动时显示冲突横幅（载入磁盘版本 / 保留我的）；文件被删除时提示另存 |
| 恢复为已存储的版本 | 有改动时先确认 |
| 未命名文档崩溃恢复 | `recovery.rs`：每 2 秒写一份副本到 App 数据目录；下次启动时恢复，且仍标为“已编辑”（审计 A9）；存储或放弃后删除副本 |
| Front matter | 标题上方显示属性条，可在对话框中编辑；校验规则在 core |
| 原生“已编辑”圆点 | objc2 调用 `setDocumentEdited:` |
| 设置持久化 | `settings.json`（App 配置目录）。文件损坏时另存为 `.bad`、不覆盖；所有窗口实时同步 |
| 首次启动欢迎文档 | 只在第一次启动显示，之后启动打开空白文档 |

**P3 外壳 UI**（`apps/desktop/ui`，Svelte 5 + TypeScript）：

- 活动栏：文件树 / 大纲 / 文件夹搜索 / 命令面板 / 设置
- 侧栏：⌥⌘1、⌥⌘2、⇧⌘F、⌃⌘S
- 查找替换：ProseMirror 插件，显示“3 / 17”，支持区分大小写 / 全字 / 正则，可跨格式匹配，全部替换一步撤销（D9、审计 B11/B12）
- 状态栏：保存状态、可切换的统计方式、字数目标进度环、编码、缩放比例、规范化提示
- 命令面板 ⇧⌘P：文件 / 命令 / 全文三种模式，Tab 切换，输入 `>` 直接进入命令模式
- 设置窗口 ⌘,：外观、排印（带实时预览）、编辑、文件、快捷键
- 完整菜单：§1.7 的全部快捷键，外加 ⇧⌘F、⌃⌘S
- 打印样式：打印时去掉界面元素、取消专注模式的变暗

**验证方式**：
- `next/scripts/selftest.sh`：在隐藏窗口里驱动真实 UI，并校验最终写到磁盘的内容，**37/37 通过**
- `next/scripts/fidelity.sh`：6/6 通过
- Rust 测试 73 个、JS 测试 5 个
- clippy `-D warnings` 与 svelte-check 均为 0 问题

**仍需人工点一遍**（自测覆盖不到，需要可见窗口或系统对话框）：
- 设置窗口的外观
- 原生标签与“合并所有窗口”
- 关闭 / 退出时的“存储吗？”对话框
- 崩溃后的恢复流程
- 粘贴图片
- 快捷键实际触达菜单

**依赖上的教训**：Svelte 外壳和编辑器放在同一个 npm 工作区里时，mermaid 的旧版 micromark 被提升到顶层，导致**列表在解析时整段消失**（保真测试当场拦下）。现在编辑器包单独用 Swift 版发布时的 lockfile 安装（`npm run install:editor`，即 `npm ci`），Markdown 处理链与 0.5.3 逐版本一致。

### 保真修复（2026-11-09，已完成）

| 问题 | 根因 | 修法 | 位置 |
|---|---|---|---|
| 图片 alt 变成 `1.00` | Crepe 图片块把缩放比例存进 alt | 重新注册 `image-block`，增加 `alt` 属性：未缩放时原样写回；只有 alt 为空或本来就是数字时才写比例 | `packages/editor/src/fidelity.js` |
| `$$x$$` 变成 `$x$` | 单行 `$$` 被 remark-math 解析成行内公式，序列化时取最短的 `$` | remark 插件从源码记下 `$` 的个数；`math_inline` 增加 `fence` 属性，以原样 HTML 节点写回 `$$x$$` | 同上 |
| 紧凑列表变松散 | Milkdown 把 `spread` 解析成**字符串** `"false"`，序列化的 join 规则只认布尔值 | 用 toMarkdownExtension 注册一条字符串 spread 的 join 规则（不能用 `editor.config`：会和 init 竞态，实测不生效） | 同上 |
| `-` 列表变成 `*`，`1)` 变成 `1.` | 序列化器写死默认值 | 加载时检测文档自己用的列表符号，作为序列化选项传入 | 同上 |
| front matter 后的空行丢失、文末多一个空行 | 编辑器不输出开头的换行，结尾总是补一个空行 | `DocumentSession` 记住正文首尾的换行，把编辑器输出按原样补回 | `crates/vellumi-core/src/document.rs` |
| CRLF 正文变成 LF | 编辑器内部只用 LF | 正文全是 CRLF 时，把输出转回 CRLF；混合换行不处理 | 同上 |

**验证**：`next/scripts/fidelity.sh` 用真实编辑器（隐藏窗口，`VELLUMI_ROUNDTRIP_OUT` 模式）跑 `fixtures/fidelity/input/` 下的 6 份语料，和人工审过的 `expected/` 对比，再把输出跑第二遍，证明保存结果是幂等的。结果全部通过：
- images、lists、math 三份语料**逐字节不变**
- misc-crlf、p0 只差表格分隔行

另外有 5 个 JS 规则单测，Rust 核心共 61 个测试。

**仍然存在、但不影响内容和渲染的写法差异**（属于可接受的 Typora 式规范化，§1.1）：
- 表格分隔行 `|---|` 写成 `| - |`
- 硬换行的两个空格写成 `\`
- 引用式链接被内联，定义行删除（链接目标不变）
- `* * *` 写成 `***`
- 行中的 `\#` 去掉转义

如果要保留引用式链接的写法，需要给 Milkdown 加 linkReference 节点，工作量中等，暂不做。
> 界面设计稿：<https://claude.ai/artifact/SwVbP8aAgd1j29UdJc5ESM>（7 张画板，与本文第 2.5 节对应）
> 依据：通读 `Mac/`、`Shared/`、`Editor/`、`Resources/imagecard/card.html`（结构）、`Tests/`、`docs/AUDIT-*.md`、`docs/PERF.md`，并核对了本机已安装版本的偏好设置键名（只读，未读取书签内容）。

---

## 0. 先纠正几个前提

| 你记得的 | 代码里的实际情况 |
|---|---|
| 实时预览 | 不是“左写右预览”，而是 **Typora 式单栏所见即所得**（Milkdown Crepe / ProseMirror）。另有 ⌘/ 源码模式。 |
| 标签 | **不存在**标签功能。Front matter 里可以写 `tags:`，但 App 不解析、不展示。 |
| 全文搜索 + SQLite 索引 | **没有任何索引、没有 SQLite**。全文搜索是回车触发的线性扫描（最多 1000 个文件、每个文件最多 50 处）。tantivy 是**新增能力**，不是迁移。 |
| 配置在 `~/Library/...` | App 开了沙盒，配置在 **`~/Library/Containers/com.aaron.vellumi/Data/Library/Preferences/com.aaron.vellumi.plist`**（UserDefaults）。没有其他配置文件。 |
| 现有技术栈 | Swift 5 + AppKit（少量 SwiftUI）+ WKWebView；编辑器是 esbuild 打包的 Milkdown Crepe 7.21.2（JS，约 6.4 MB）；Sparkle 2 自动更新；xcodegen 工程；macOS 13+；ad-hoc 签名，通过 DMG/zip/Homebrew cask 分发。 |

**关键结论**：Tauri 在 macOS 上用的仍是 **WKWebView**，和现在同一个引擎。编辑器（Crepe）可以原样复用，这是“文件格式无损兼容”最重要的保障：**文件字节由谁序列化，决定了兼容性**。见 §2.3 的原则一。

---

## 1. 功能清单

按模块列出，【隐藏】表示 README/菜单里看不出、只在代码里才有的行为。文件位置写到 Swift 源文件，方便迁移时对照。

### 1.1 文件读写（`Shared/Document/MoltenDocument.swift`）

- **可打开的类型**：仅 `net.daringfireball.markdown`（`.md` `.markdown`），Info.plist 声明为 Editor 角色、Default rank。
- **大小上限 4 MB**：超过就拒绝打开，并提示原因。README 写的 20 MB 已过时。【隐藏】外部改动重载、全文搜索也使用同一个上限。
- **解码规则**（严格，绝不猜编码）：
  - 空文件 → 空字符串
  - 以 `FF FE`/`FE FF` 开头 → 按 UTF-16 解码
  - 含 NUL 字节 → 拒绝（防止把无 BOM 的 UTF-16 当成 UTF-8 解出乱码）
  - 非法 UTF-8 → 拒绝
  - UTF-8 BOM → 去掉
  - 【隐藏】**保存时一律写成无 BOM 的 UTF-8**：UTF-16 文件和带 BOM 的 UTF-8 文件，被编辑保存一次后编码就静默改变了。
- **Front matter 字节级保护**（`MoltenFrontMatter.swift`）：
  - 第一行必须正好是 `---`，到下一个 `---` 或 `...` 行结束（允许 CRLF）；没闭合就整体算正文
  - `join(split(x)) == x` 对任意输入都成立
  - Front matter 不进入编辑器，每次保存原样拼回
- **Typora 式规范化**：编辑器每次加载都会重新序列化 Markdown（列表符号统一为 `-`、空行重排等）。
  - 【隐藏】打开文件时，把规范化后的文本当作“已保存基线”，所以只打开、不编辑的文件不会被改写。
  - 【隐藏】文本确实发生变化时，状态栏显示一次性提示（每个窗口一次，10 秒后消失）：“保存时将规范化本文档的 Markdown 书写 · ⌘/ 可查看原文”。
  - 【隐藏】正文里的 CRLF 会被编辑器转成 LF（front matter 里的 CRLF 保留）。
- **保存**：NSDocument 原地自动保存（`autosavesInPlace = true`），⌘S 等于“立即保存”。
  - 每次保存、另存为、关闭之前，先从编辑器拉取最新文本。
  - 【隐藏】保存流程里绝不能异步跳回主线程等待 JS，否则关窗时会死锁（注释里称为 DEADLOCK RULE）。
- **脏标记**：只要正文和 front matter 都等于基线就算干净。【隐藏】编辑后撤销回原样，“已编辑”标记会自动清除。
- **外部改动**（NSFilePresenter）：
  - 文档干净、磁盘 mtime 更新、文件不超过 4 MB 时，用协调读取自动重载，同时清空撤销历史
  - 文档脏时交给 NSDocument 的冲突对话框处理
  - 判断前先把编辑器里最后约 1 秒的输入同步过来
- **NSDocument 免费附带、迁移后需要自己实现的能力**【隐藏】：
  - 未命名文档自动保存到容器内的 `Autosave Information/`，崩溃或重启后恢复
  - “恢复为已存版本”（Revert）
  - “浏览所有版本”（Versions 历史）
  - 标题栏文档菜单：重命名、移动、复制、锁定
  - 标题栏代理图标（可拖拽，⌘-点击显示路径）
  - 原子保存
  - 关闭时提示保存
  - “最近使用的文档”和 Dock 菜单里的最近项
  - 系统窗口恢复（`Saved Application State`）
  - 文档窗口标题显示“已编辑”
- **首次启动欢迎文档**：中英文各一份，以未保存、非脏的未命名文档打开，关掉不留痕迹。
  - 【隐藏】只有启动时没有其他文档打开才显示（例如通过 Finder 双击 .md 启动时不显示），标志位 `Vellumi.hasShownWelcome`。
- **草稿文件夹**（`MoltenDocumentController.swift`）：可选。
  - 设置后，**只有显式的 ⌘N** 会在草稿文件夹创建真实的 `未命名.md`、`未命名 2.md`……
  - 启动或点击 Dock 自动出现的空白窗口仍然是临时缓冲区。
  - 【隐藏】放弃的 ⌘N 会留下 0 字节文件（审计 B18，仍未修）。

### 1.2 编辑器（`Editor/src/main.js`，Crepe 7.21.2，版本锁定）

- **Crepe 自带功能**：斜杠菜单、块拖拽手柄、浮动格式工具栏、链接提示框、图片块（上传、链接、说明文字）、表格工具、带语言选择器的 CodeMirror 代码块（全量语言注册）、KaTeX 行内与块级公式、任务列表、占位符。
- **额外插件**：
  - Mermaid 渲染：自写 nodeView，跟随深浅色，解析失败时显示源码，节点不可直接编辑
  - 表格列宽拖拽
  - Emoji `:shortcode:` 自动补全（gemoji，始终开启）
  - 智能标点（弯引号 … —），默认关闭；【隐藏】切换时会重建编辑器，撤销历史丢失
- **本地化**：斜杠菜单、占位符、图片块文案有中文版。【隐藏】语言取 App 的界面语言（由 Swift 注入 `window.__vellumiLanguage`），不取系统语言；同时设置 `<html lang>`，让 VoiceOver 选对语音。
- **变更上报**：
  - 防抖 150 ms，最长延迟 1 s
  - 【隐藏】自适应节流：序列化耗时 × 10 作为防抖（上限 2 s），× 40 作为最长延迟（上限 3 s）
  - 【隐藏】输入法组字期间不序列化
  - 序列化结果按文档对象缓存
- **打字机模式**：光标行保持在视口 45% 处，4 px 死区。【隐藏】鼠标操作或输入法组字期间不滚动。
- **专注模式**：除光标所在的顶层块外，其余块不透明度 0.3。
- **拼写检查**：开关作用于 contenteditable 的 spellcheck 属性，默认关闭。
- **图片**：
  - 粘贴或拖入图片，单张上限 32 MB；以 base64 传给 Swift，写入文档旁的 `assets/`（目录名可在设置里改）
  - 文件名清洗规则：非法字符与空白替换成 `-`，默认扩展名 png，重名时加 `-1`、`-2`
  - Markdown 里写入百分号编码的相对路径
  - 【隐藏】文档未保存时弹窗拒绝；首次写入某个文件夹会弹出一个**锁定在该文件夹**的授权面板
  - 显示时，相对路径改写为 `molten-asset://asset/…`，由 Swift 读取并返回
  - 【隐藏】防目录穿越：`../x.png` 这种指向文档目录之外的图片**不显示**；`http(s):`、`data:`、`file:`、绝对路径原样保留（沙盒内的绝对路径通常无法加载）
- **Web 进程崩溃自愈**：重新加载页面，再用 Swift 保存的文本恢复内容。
- **大纲数据**：遍历 ProseMirror 树得到 `{level, text, pos}`。点击大纲项时，光标移到该标题并滚动到顶部。

### 1.3 窗口与界面（`Mac/Editor/*.swift`）

- **窗口**：
  - 默认 900×720，最小 480×320
  - 透明标题栏、全尺寸内容
  - 【隐藏】所有文档窗口共用一个位置/尺寸记忆 `MoltenDocumentWindow`
  - **原生窗口标签**：`tabbingMode = .preferred`，即使系统设置为“从不”也会合并成标签
- **工具栏**：大纲按钮、文件树按钮、弹性空白、导出菜单（HTML / PDF / 图片卡 / 打印）。
- **大纲侧栏 ⌥⌘1**：
  - 宽 160–320，按级别缩进 12 pt，H1 加粗，空标题显示“—”
  - 支持方向键导航
  - 只在可见时随内容刷新
- **文件树侧栏 ⌥⌘2**：显示当前文档所在文件夹。
  - 递归，深度 ≤ 6，节点 ≤ 2000，最多扫描 20000 项
  - 跳过隐藏文件，剪掉不含 Markdown 的空目录
  - 文件夹在前，按自然排序
  - 当前文件加粗；右键“在 Finder 中显示”
  - 没有权限时显示授权按钮
  - 只在显示时和保存后刷新，**不监听磁盘**
- 两个侧栏的显示状态全局记忆（`Vellumi.showsOutline/showsFileTree`）。拖动分隔条收起也会同步记忆。【隐藏】动画期间冻结编辑器宽度，防止正文抖动。
- **查找栏 ⌘F**：
  - 浮在内容上方，不推挤正文
  - 搜索框按回车搜索，显示总匹配数（不显示当前是第几个）
  - 上一个/下一个、替换框、替换、全部替换（一步撤销）、完成（Esc）
  - 【隐藏】⌘G/⇧⌘G 在查找栏未打开时会先打开它；找不到时响一声
  - 【隐藏】始终**不区分大小写**；计数和替换不跨格式边界，但导航（`window.find`）跨（审计 B12）
- **状态栏**：
  - 点击循环三种显示：字数 + 阅读时间（400 字/分钟，向上取整，至少 1 分钟）→ 字符数 → 段落数，模式全局记忆
  - 字数目标 > 0 时追加“目标 N%（M 字）”
  - 统计在后台线程计算，延迟 200 ms
  - 【隐藏】字数规则：每个 CJK 字符算 1 个；拉丁字母、数字、下划线、Latin-1 扩展字母的连续串算 1 个。统计对象是 **Markdown 原文**，所以链接 URL、代码也会计入
  - 【隐藏】字符数不计空白，但 U+3000 全角空格会被计入（审计 B20）
- **源码模式 ⌘/**：
  - NSTextView，等宽字体，大小 13 × 缩放（限制在 9–48 之间），关闭所有智能替换，使用系统查找栏
  - 显示**完整文件**（含 front matter），每次按键都同步回文档模型
  - 进出时保留滚动比例
  - 【隐藏】源码模式下禁用导出、打印和编辑 front matter；进入时清空撤销栈
- **编辑 Front matter**（文件菜单）：
  - 弹出表单；为空时预填 `---\ntitle: \n---\n`
  - 校验规则：必须是一个闭合的 YAML 块且后面没有内容；内容为空表示删除 front matter

### 1.4 项目、快速打开、全文搜索（`Shared/Projects`、`Mac/Projects`）

- **项目**：对文件夹的引用（安全作用域书签），存为 `Vellumi.projects.list`。
  - 存储格式：PropertyListEncoder 编码的 `[{id: UUID, name, folderBookmark: Data, dateAdded: Date}]`
  - 【隐藏】重复添加同一路径会替换旧项，并生成**新的 UUID**
- **项目面板 ⇧⌘0**：
  - 展开后**只列顶层文件，不递归**（审计 B10）
  - 扩展名 md / markdown / **txt**；按修改时间倒序，相同时按名称
  - 【隐藏】列表里有 .txt，但 App 打不开（审计 B6）
  - 右键菜单：新建笔记、重命名项目（只改显示名）、移除、打开、重命名文件、在 Finder 中显示
  - 【隐藏】重命名文件时保留扩展名，重名会弹窗；如果文件已打开，通过文档对象移动，避免产生分叉
  - 最近文档来自系统列表，任意窗口成为焦点时刷新
- **快速打开 ⇧⌘P**：
  - 搜索范围：各项目顶层文件 + 当前文档所在文件夹，按规范路径去重
  - 空查询时显示最近修改的 50 个
  - 模糊匹配打分：连续 +4、首字符 +6、单词边界 +5，最后 ×100 再减去长度
  - Esc 关闭，↑↓ 选择，回车打开
  - 【隐藏】重启后因沙盒作用域未启动，可能一个都搜不到（审计 A6）
- **全文搜索**（项目面板的搜索框）：
  - 回车触发，不区分大小写，查询词去掉首尾空白
  - 只显示文件名和命中数（行号和行内容算了但没显示）
  - 打开结果后把查询词交给编辑器的查找，最多重试 10 次、每次间隔 0.3 秒

### 1.5 导出（`Mac/Export`、`Resources/imagecard`）

- **HTML ⇧⌘E**：
  - 来源是编辑器渲染出的 DOM（已清理：代码块重建为 `<pre><code>`、表格去掉外层包装、列表恢复为标准 `<li>`、任务列表恢复为 checkbox、图片地址改回相对路径）
  - 内联 `editor.css` + **当前主题的浅色版**（即使在深色模式下）；正文最大宽度 760；标题做了转义
  - 【隐藏】Typora 风格的 `[toc]`：整段内容只有 `[toc]` 的段落会展开成目录；标题按 slug 生成 id 并去重
  - 【隐藏】KaTeX 字体是相对路径，导出后加载不到（审计 B21）
- **PDF**：直接对实时编辑视图调用 `createPDF`。【隐藏】长代码块会丢掉没滚动到过的行，专注模式下导出的是变灰的字（审计 A10）。
- **打印 ⌘P**：同样对实时视图打印，横向适配页宽。
- **图片卡**：
  - 选项：6 个主题（aurora / cream / dark / forest / minimal / sunset）、3 种尺寸（1080×1920、1080×1350、1242×2208）、PNG/JPEG、段首缩进（无 / 两字，默认两字）、水印、输出到文件夹或 ZIP
  - 【隐藏】选项不记忆
  - 【隐藏】文件名清洗后回退为 `MarkMac-Card`；页码补零，至少两位
  - 【隐藏】ZIP 只存储不压缩，且没有设置 UTF-8 文件名标志（审计 B2）
  - 渲染由 card.html 完成（html2canvas + highlight.js + KaTeX）

### 1.6 外观与设置（`MoltenPreferencesWindow.swift`、`MoltenViewSettings.swift`）

| UserDefaults 键 | 类型 / 默认值 | 含义 |
|---|---|---|
| `Vellumi.appearanceMode` | Int 0/1/2，默认 0 | 跟随系统 / 浅色 / 深色（作用于整个 App 的 `NSApp.appearance`） |
| `Vellumi.editorTheme` | `frame` / `nord` / `classic`，默认 frame | 编辑器主题，浅色和深色各一套 CSS |
| `Vellumi.editorZoom` | Double 0.5–3.0，0 视为 1.0 | 缩放：每步 ×1.1，⌘+ / ⌘- / ⌃⌘0 |
| `Vellumi.spellcheck` / `typewriter` / `focusMode` / `smartPunctuation` | Bool，默认 false | 编辑选项 |
| `Vellumi.fontScheme` | `default` / `serif` / `sans` | 字体方案 |
| `Vellumi.lineHeight` | 0 或 1.2–2.2 | 行距，0 表示主题默认 |
| `Vellumi.paragraphSpacing` | 0 或 0.2–2.0 em | 段间距，0 表示主题默认 |
| `Vellumi.lineWidth` | 0 或 560–1080 px | 行宽，0 表示自适应 `clamp(760px, 75%, 1200px)` |
| `Vellumi.wordGoal` | Int，默认 0 | 字数目标，0 表示关闭 |
| `Vellumi.imageFolderName` | String，默认 `assets` | 图片目录名。【隐藏】去掉 `/ \ :`，结果为空、`.` 或 `..` 时回退到 assets |
| `Vellumi.statsDisplayMode` | Int 0–2 | 状态栏统计方式 |
| `Vellumi.showsOutline` / `showsFileTree` | Bool | 侧栏显示状态 |
| `Vellumi.hasShownWelcome` | Bool | 欢迎文档是否已显示 |
| `Vellumi.projects.list` | Data（plist） | 项目列表 |
| `Vellumi.drafts.folderBookmark` | Data | 草稿文件夹书签 |
| `Vellumi.folderBookmark.<绝对路径>` | Data | 每个文件夹的访问授权。**路径就在键名里**，迁移时不需要解析书签也能拿到 |
| `NSWindow Frame MoltenDocumentWindow` 等 | String | 窗口位置 |
| `SU*` | — | Sparkle 自身状态（是否自动检查等） |

【隐藏】设置变更会广播给所有打开的文档，100 ms 合并一次。

### 1.7 快捷键（必须保持一致）

| 菜单 | 项 | 快捷键 |
|---|---|---|
| App | 设置 / 隐藏 / 隐藏其他 / 退出 | ⌘, / ⌘H / ⌥⌘H / ⌘Q |
| 文件 | 新建 / 打开 / 快速打开 | ⌘N / ⌘O / ⇧⌘P |
| 文件 | 关闭 / 保存 / 另存为 | ⌘W / ⌘S / ⇧⌘S |
| 文件 | 导出 HTML / 打印 | ⇧⌘E / ⌘P |
| 文件 | 恢复、浏览版本、编辑 Front matter、草稿文件夹、导出 PDF、导出图片卡 | 无 |
| 编辑 | 撤销 / 重做 | ⌘Z / ⇧⌘Z |
| 编辑 | 剪切 / 复制 / 粘贴 / 全选 | ⌘X/C/V/A |
| 编辑 | 查找 / 下一个 / 上一个 | ⌘F / ⌘G / ⇧⌘G |
| 格式 | 正文 / 标题 1–6 | ⌘0 / ⌘1–⌘6 |
| 格式 | 粗体 / 斜体 / 行内代码 / 删除线 / 链接 | ⌘B / ⌘I / ⌘E / ⇧⌘X / ⌘K |
| 格式 | 无序列表 / 有序列表 / 引用 / 分隔线 | ⇧⌘U / ⇧⌘O / ⇧⌘Q / ⌥⌘- |
| 显示 | 项目 / 大纲 / 文件树 / 源码模式 | ⇧⌘0 / ⌥⌘1 / ⌥⌘2 / ⌘/ |
| 显示 | 打字机 / 专注 | ⌥⌘T / ⌥⌘F |
| 显示 | 放大 / 缩小 / 实际大小 | ⌘+ / ⌘- / ⌃⌘0 |
| 显示 | 拼写检查、外观子菜单、主题子菜单 | 无 |
| 窗口 | 最小化 | ⌘M |
| 窗口 | 缩放、上一个/下一个标签、移到新窗口、合并所有窗口 | 无（系统另提供 ⌃Tab 切换标签） |
| 查找栏 | 关闭 | Esc |

另外还有 Crepe / ProseMirror 编辑器内置的键位（列表 Tab/⇧Tab 缩进、表格 Tab 跳格、Enter/Backspace 行为等），只要 Crepe 版本锁定在 7.21.2，就会原样保留。

【隐藏】AppKit 会自动往菜单里加一些项：编辑菜单的“听写”和“表情与符号”、显示菜单的“进入全屏幕”、App 菜单的“服务”，可能还有“打开最近使用”（需要实测确认）。这些在 Tauri 里需要手动添加。

### 1.8 发布与系统集成

- Sparkle 2：EdDSA 签名，feed 是 `raw.githubusercontent.com/SUDAcyber/Vellumi/master/appcast.xml`，App 菜单里有“检查更新”。
- Universal 二进制（arm64 + x86_64），ad-hoc 签名，hardened runtime 的取舍记录在 release.sh 里；Homebrew cask `SUDAcyber/tap/vellumi`。
- 日志写入 os.Logger，subsystem 为 `com.aaron.vellumi`，分类 document / editor。
- 中英双语，Localizable.strings 约 205 个键；Welcome.md 有两种语言。

### 1.9 审计中至今未修、经代码核对仍存在的缺陷

A6（快速打开在重启后为空）、A9（崩溃恢复的内容被当作已保存基线）、A10（PDF/打印对实时视图截取）、B1、B2、B6、B10、B11、B12、B18、B20、B21、C5（块级公式导出为代码块）、HTML 导出固定用浅色主题。A4/A5（card.html 分页问题）没有逐条复核。是否在重写时修掉，见 §5 决策 D10。

---

## 2. 架构设计

### 2.1 Workspace 目录结构

```
vellumi/
├─ Cargo.toml                 # [workspace]，统一 lints：clippy::unwrap_used / expect_used = deny（core 与 io crate）
├─ crates/
│  ├─ vellumi-core/           # 纯 Rust，无 Tauri、无 objc、无 tokio 运行时依赖
│  │   ├─ error.rs            #   CoreError（thiserror）
│  │   ├─ encoding.rs         #   严格解码 / 编码回写策略
│  │   ├─ front_matter.rs     #   split / join，字节级可逆
│  │   ├─ document.rs         #   DocumentSession：基线、脏标记、源码模式、规范化基线
│  │   ├─ stats.rs            #   字数 / 字符 / 段落 / 阅读时间
│  │   ├─ fuzzy.rs            #   快速打开打分
│  │   ├─ tree.rs             #   文件树构建（带预算）
│  │   ├─ projects.rs         #   项目模型与文件枚举
│  │   ├─ assets.rs           #   图片命名清洗、相对路径编码、防穿越解析
│  │   ├─ export/             #   toc.rs、html_page.rs、card_names.rs、zip.rs（纯函数）
│  │   ├─ settings.rs         #   Settings 结构、默认值、取值范围、schema 版本
│  │   └─ markdown.rs         #   comrak：标题提取、纯文本提取（只读，不负责写盘）
│  ├─ vellumi-fs/             # 依赖 core：原子写、外部改动检测（notify + mtime/size/hash）、自写抑制
│  ├─ vellumi-index/          # 依赖 core：tantivy 全文索引 + 线性扫描回退
│  ├─ vellumi-legacy/         # 依赖 core + plist：读取 Swift 版 UserDefaults，生成迁移报告（只读）
│  └─ vellumi-platform/       # trait 定义在 core，这里放各平台实现
│      ├─ macos.rs            #   objc2：安全作用域书签、NSFileCoordinator、WKWebView createPDF、最近文档、Dock
│      ├─ windows.rs          #   预留：WebView2 PrintToPdf 等
│      └─ linux.rs            #   预留
├─ apps/
│  ├─ desktop/
│  │   ├─ src-tauri/          # Tauri 2：commands/、events.rs、menu.rs、windows.rs、state.rs、autosave.rs
│  │   └─ src/                # TypeScript 前端（应用外壳 UI）
│  │       ├─ shell/          #   标签栏、活动栏、侧栏、状态栏、查找栏、命令面板、设置、项目
│  │       ├─ ipc/            #   commands.ts / events.ts（由 tauri-specta 生成类型）
│  │       └─ i18n/           #   由 Localizable.strings 转换而来
│  └─ sync-server/            # 预留：Axum，只依赖 core / index
├─ packages/
│  ├─ editor/                 # main.js 迁移为 TS：Crepe 7.21.2 锁版本 + 插件 + bridge 接口
│  ├─ source-editor/          # CodeMirror 6 源码模式
│  └─ card-renderer/          # card.html 迁移
├─ fixtures/                  # 兼容性语料（见 §3 P1/P3）
└─ docs/
```

**依赖方向**：`core ← fs/index/legacy ← platform ← src-tauri`；`sync-server` 只依赖 core 和 index。core 不知道 Tauri、窗口和 objc 的存在，这样将来服务端和移动端都能直接复用。

### 2.2 错误类型（硬性要求 3）

```text
vellumi-core::CoreError (thiserror)
  Io { op: IoOp, path: PathBuf, #[source] std::io::Error }
  TooLarge { path, size: u64, limit: u64 }
  Encoding { path, kind: EncodingError }        // ContainsNul | InvalidUtf8 { valid_up_to } | Utf16Odd
  FrontMatterInvalid { reason: &'static str }
  PathEscapesRoot { requested, root }           // 防目录穿越
  NotFound { path } / AlreadyExists { path }
  PermissionDenied { path, needs_grant: bool }  // 沙盒授权
  Conflict { path, disk: Fingerprint, ours: Fingerprint }
  Settings(SettingsError) / Index(IndexError) / Legacy(LegacyError)
```

- 所有面向用户数据的函数都返回 `Result<_, CoreError>`。core / fs / index / legacy 四个 crate 用 `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`。只有测试和 `build.rs` 例外。
- Tauri command 统一返回 `Result<T, IpcError>`，`IpcError` 序列化为 `{ kind, i18nKey, params, detail }`，前端按 `i18nKey` 显示本地化文案（沿用现有 `document.error.*` 等键）。
- 正则这类“编译期就确定正确”的常量用 `LazyLock` + 单元测试兜底，不在运行时 unwrap。

### 2.3 三条核心原则

1. **写盘的字节只来自 Crepe 的序列化器**（与现在相同）。comrak 只用于只读场景：大纲、搜索纯文本、服务端渲染。任何“用 comrak 重新输出 Markdown”的做法都会改变用户文件，禁止。
2. **Rust 永远不同步等待 JS**。保存、关闭、退出时由前端主动刷出最新文本再调用 command。Rust 侧的等待一律带超时（对应现在的 DEADLOCK RULE 和审计 C3）。
3. **绝不在用户的项目文件夹里写入元数据**。索引、缓存、恢复文件都放在 App 数据目录。用户文件夹里只会出现用户主动产生的东西：`assets/` 里的图片和新建的笔记。

### 2.4 Command / Event 接口

命名规则：command 用 `域_动作`，event 用 `域:事件`。类型由 `tauri-specta` 生成到 `ipc/commands.ts`，前后端共用同一份定义。

**Commands（前端 → Rust）**

| 域 | Command | 参数 → 返回 | 说明 |
|---|---|---|---|
| doc | `doc_open` | `path` → `DocSnapshot{id, path, displayName, frontMatter, body, encoding, fingerprint}` | 解码、大小检查、拆分 front matter |
| | `doc_new` | `{kind: Untitled\|Drafts\|Project(id)}` → `DocSnapshot` | 区分草稿文件夹和普通未命名 |
| | `doc_update_body` | `id, body` → `DirtyState` | 防抖后的 change |
| | `doc_adopt_normalized` | `id, body, changed` → `()` | 规范化基线（文档脏时不生效） |
| | `doc_set_front_matter` | `id, text` → `DirtyState` | 在 core 中校验 |
| | `doc_set_full_source` | `id, text` → `DirtyState` | 源码模式，重新拆分 |
| | `doc_save` / `doc_save_as` | `id, body, (path)` → `SaveResult` | 原子写；返回新的 fingerprint |
| | `doc_revert` / `doc_close` | `id` → `DocSnapshot` / `CloseOutcome` | 关闭时由 Rust 决定是否需要提示 |
| | `doc_move` | `id, newPath` → `()` | 重命名或移动已打开的文件 |
| | `doc_stats` | `text` → `Stats{words, chars, paragraphs, readingMinutes}` | 与 Swift 结果逐字一致 |
| asset | `asset_save_image` | 原始字节（`InvokeBody::Raw`）+ 头部 `docId, name` → `relPath` | 不再走 base64 |
| | 协议 `vellumi-asset://<docId>/<rel>` | — | `register_asynchronous_uri_scheme_protocol`，在 core 中做防穿越检查 |
| fs | `tree_build` | `folder` → `TreeNode[]` | 沿用相同预算 |
| | `folder_request_access` | `folder` → `bool` | 平台层实现（沙盒时弹授权面板） |
| | `reveal_in_finder` | `path` → `()` | |
| project | `projects_list` / `_add` / `_remove` / `_rename` / `_files` / `_relocate` | — | `_files` 可选是否递归，见决策 D10 |
| | `project_file_rename` | `projectId, path, input` → `newPath` | 保留扩展名，重名报错 |
| | `recents_list` / `recents_note` | — | 同时写入系统最近文档（macOS） |
| search | `quick_open_query` | `query, limit` → `FileHit[]` | 模糊打分在 Rust 里完成 |
| | `search_content` | `query, opts{caseSensitive,…}, channel` → 流式 `SearchHit` | tantivy，索引未就绪时回退到线性扫描 |
| | `index_status` / `index_rebuild` | — | |
| settings | `settings_get` / `settings_patch` | — / `Partial<Settings>` | 写入后广播事件 |
| export | `export_html` | `docId, bodyHtml, path` → `()` | 在 Rust 中做 TOC、拼页面、内联字体 |
| | `export_pdf` / `print_document` | `windowLabel, (path)` → `()` | 平台层实现（离屏渲染，见决策 D11） |
| | `export_cards_write` | `pages: bytes[], mode, dest` → `()` | 正确设置 ZIP 的 UTF-8 标志 |
| app | `app_info`、`welcome_content(lang)`、`updater_check`、`log` | — | |

**Events（Rust → 前端）**

| Event | 负载 | 触发时机 |
|---|---|---|
| `menu:action` | `{id, payload?}` | 所有自定义菜单项（格式、显示、查找……） |
| `doc:external-change` | `{id, kind: Reloaded{body, frontMatter} \| ConflictDirty \| Deleted \| Moved{path}}` | notify 检测到磁盘变化 |
| `doc:dirty-changed` / `doc:saved` | `{id, dirty}` / `{id, path, fingerprint}` | 驱动标签上的圆点和状态栏 |
| `doc:flush-request` | `{id, reason: Save\|Close\|Quit\|Autosave}` | 需要前端交出最新文本时 |
| `settings:changed` | `Settings` | 任意窗口修改设置后 |
| `fs:tree-changed` | `{folder}` | 文件树自动刷新（新增能力） |
| `index:progress` | `{indexed, total}` | 索引构建进度 |
| `app:open-files` | `{paths}` | Finder 双击打开、拖到 Dock |

**编辑器包接口**（`packages/editor`，相当于现在的 `window.moltenAPI`）：保留现有全部方法名，便于把 `MoltenEditorBridgeTests` 的 23 个用例直接移植过来。查找相关方法换成 ProseMirror 搜索插件，增加 `caseSensitive` 参数和“第几个/共几个”的返回值。

### 2.5 界面（对应设计稿）

- **窗口与标签**：
  - 首选方案：一个文档一个 WebviewWindow，加原生标签（`tabbing_identifier`），与现在一致
  - 设计稿画的是窗口内的 Web 标签栏，这需要单窗口多文档，架构差别很大，见决策 D3
- **左侧活动栏**：合并文件树、大纲、搜索、项目，替代两个独立侧栏。⌥⌘1 / ⌥⌘2 仍然直接打开对应面板。
- **编辑区**：所见即所得，界面上**没有模式切换按钮**（同 Typora），⌘/ 进入源码模式。Front matter 显示为标题上方的属性条，点击后可编辑。
- **状态栏**：保存状态、规范化说明（可点）、点击切换的统计、目标进度环、编码、缩放比例。
- **查找**：ProseMirror 内搜索，显示 “3 / 17”，提供 Aa（区分大小写）/ 全字 / 正则开关，滚动条上标出匹配位置。
- **⇧⌘P**：文件 / 命令 / 全文 三种模式，用 Tab 切换。全文模式提供 ⇧⌘F 入口（新增快捷键，见决策 D4）。
- 设置、项目工作区、图片卡导出的布局见画板。

---

## 3. 迁移计划

每个阶段都能独立验证。**在 P8 之前，Swift 版始终是你的日常主力**，新版用不同的 bundle id 并行试用（见决策 D1）。

### P0 · 技术验证（1–2 周）

逐项做最小原型，任何一项失败都要回到本文重新决策。

| # | 验证项 | 完成标准 |
|---|---|---|
| 0.1 | Tauri 2 加载现有 `Resources/dist`（Crepe） | 打开 Welcome.md，Mermaid、KaTeX、代码高亮、表格都正常；中文输入法组字无异常 |
| 0.2 | 菜单快捷键路由 | ⌘B、⌘1、⌘Z、⌥⌘- 在编辑器里生效；焦点在查找框时 ⌘Z 撤销的是输入框而不是文档 |
| 0.3 | 原生窗口标签 | 两个文档合并成标签，“合并所有窗口”可用 |
| 0.4 | 自定义协议显示图片 | `vellumi-asset://` 能显示含空格和中文的路径；`../` 被拒绝 |
| 0.5 | PDF | 在 macOS 上通过 `with_webview` 调用 `createPDF` 生成离屏 PDF，长代码块完整 |
| 0.6 | 旧数据可读性 | 非沙盒（或沙盒）的新 App 能读到旧容器里的 plist；从 `Vellumi.projects.list` 的书签中恢复出路径 |
| 0.7 | 更新链路 | 旧版 Sparkle 能把一个 Tauri 构建的 .app 当作更新包安装并启动（ad-hoc 签名） |

### P1 · core crate（2 周）

- 实现 §2.1 中 core 的全部模块，以及 legacy 的只读解析。
- **完成标准**：
  - 把 Swift 测试里的纯逻辑用例全部移植到 Rust：Document、FrontMatter、WordCount、ProjectStore、FuzzyMatch、ImageAttachment 的命名部分、AssetScheme 的解析部分、TOC、Exporter 文件名，约 60 个
  - 增加 `fixtures/` **差分语料**：用 Swift 版对 200 个以上真实或构造的输入（各种编码、BOM、CRLF、front matter 边界、中英混排、U+0130）输出结果作为基准，要求 Rust 结果完全相同
  - `cargo clippy -D warnings` 通过，core 中没有 unwrap / expect

### P2 · 单文档编辑闭环（3 周）

- 窗口、打开、编辑、规范化基线、自动保存、⌘S、另存为、Revert、关闭提示、外部改动、Web 进程崩溃自愈、源码模式（CodeMirror 6）、front matter 属性条与编辑、欢迎文档、Finder 打开文件。
- **完成标准**：
  - **字节兼容测试**：对 `fixtures/corpus/` 里的每个文件，只打开再关闭，磁盘字节和 mtime 都不变；执行一组固定编辑后保存，输出与 Swift 版逐字节相同（两边是同一版 Crepe，结果应当一致）
  - 外部改动的三种情形（文档干净、文档脏、mtime 未变）与 `MoltenExternalReloadTests` 对应用例的行为一致
  - 强制结束 WebContent 进程后，内容不丢失
  - 退出 App 时，所有窗口的最后一次输入都已落盘

### P3 · 工作区外壳（3 周）

- 完整菜单与 §1.7 的快捷键表、活动栏（文件树 / 大纲 / 搜索）、查找替换、状态栏、缩放、打字机与专注模式、拼写检查、主题、外观、排印、设置窗口、中英文界面。
- **完成标准**：
  - 按 §1.7 逐键人工验收，并做成自动化 E2E（WebDriver 或 Tauri 的测试工具）
  - 移植 `MoltenEditorBridgeTests` 的 23 个用例
  - 设置项与 §1.6 一一对应，取值范围和默认值相同

### P4 · 图片与资源（1 周）

- 粘贴、拖入图片保存，自定义图片目录名，文件夹授权（如果保留沙盒），防穿越，首次保存未命名文档后图片仍能正常显示（对应审计 A7 的回归）。
- **完成标准**：移植 `MoltenImageAttachmentTests` 和资源协议的测试；32 MB 上限生效；Markdown 中写入的路径与 Swift 版完全相同。

### P5 · 项目、快速打开、全文搜索（2–3 周）

- 项目增删改、新建笔记、重命名文件、最近文档、快速打开（模糊打分与 Swift 一致）、tantivy 索引（位于缓存目录，可以随时删掉重建）、notify 增量更新、文件树自动刷新。
- **完成标准**：
  - 模糊打分与 Swift 版在同一组语料上排序完全一致
  - 搜索 10k 个文件、500 MB 的语料时，首次建索引时间和查询 p95 < 50 ms 写进 PERF.md
  - 删除索引目录后能自动重建
  - 用户文件夹里没有出现任何新文件

### P6 · 导出（2 周）

- HTML（含 TOC、字体内联）、PDF 和打印（离屏渲染）、图片卡（card.html 迁移，选项记忆）。
- **完成标准**：移植 `MoltenTOCExportTests`、`MoltenExporterTests`、`MoltenImageCardRenderTests`；代码块、表格、任务列表、公式的导出效果与 Swift 版对比截图无回归；中文文件名的 ZIP 在 Windows 上解压正常。

### P7 · 迁移与分发（1–2 周）

- 首次启动导入旧设置（只读旧 plist，生成新配置并显示迁移报告）、文件关联、签名与公证策略、自动更新、Homebrew cask、release 冒烟测试（启动 3 秒没有崩溃）。
- **完成标准**：在一台装有 0.5.3、带着你真实配置的机器上，旧版通过 Sparkle 升级到新版，所有设置、项目、草稿文件夹和图片目录名都保留；旧 plist 字节未改动（便于回退）。

### P8 · 并行试用与切换（2 周以上）

- 新旧两版并行使用。对照 §1 全部功能清单逐条打勾，每一项写清楚“一致 / 有意改变 / 已修缺陷”三种状态之一。
- **完成标准**：清单 100% 有结论，连续两周日常使用没有出现数据问题，然后才把更新推给正式用户。

---

## 4. 风险点

| 级别 | 风险 | 说明 | 缓解办法 |
|---|---|---|---|
| 高 | NSDocument 功能需要重做 | 自动保存、未命名文档恢复、Revert、Versions、标题栏菜单、代理图标、冲突对话框、原子保存，Tauri 里一个都没有 | 在 P2 自己实现大部分；Versions 和代理图标见决策 D5 |
| 高 | 沙盒与旧数据 | 安全作用域书签只有原 App 身份能解析；改 bundle id 或去掉沙盒后，旧容器路径和 macOS 14+ 的“访问其他 App 数据”提示都可能挡住读取 | P0.6 实测；`Vellumi.folderBookmark.<路径>` 的路径就在键名里，可以兜底；项目书签只解析路径、不要作用域 |
| 高 | 菜单与焦点语义 | AppKit 的响应链会自动决定 ⌘Z、⌘B 发给谁、菜单项何时变灰；Tauri 的菜单事件只是一个 id | 前端写一个焦点感知的派发器，并在焦点变化时把启用/勾选状态推回菜单（P0.2） |
| 中 | PDF / 打印 | Tauri 没有提供 createPDF；每个平台的 API 都不同 | 平台 trait 实现；macOS 用 objc2 调 WKWebView；P0.5 验证 |
| 中 | 原生标签与窗口菜单 | `tabbing_identifier` 和“合并所有窗口”在 Tauri 2 中需要实测；Windows/Linux 没有原生标签 | 见决策 D3 |
| 中 | 自动更新衔接 | Sparkle → Tauri updater 的签名体系不同（EdDSA → minisign） | 最后一个 Swift 版通过 Sparkle 把新版送达；之后改用 tauri-plugin-updater 和新的 feed（P0.7） |
| 中 | 原生观感下降 | 侧栏、查找栏、设置从 AppKit/SwiftUI 换成 Web：毛玻璃效果、VoiceOver 读法、键盘焦点环需要手工打磨 | 用 window-vibrancy 处理窗口级毛玻璃；把无障碍检查写进 P3 验收 |
| 中 | 外部改动的读取一致性 | NSFileCoordinator 能避免在 iCloud / Dropbox 同步时读到写了一半的文件；notify 做不到 | macOS 平台层继续用 NSFileCoordinator 协调读取；其他平台用“两次读取结果一致”判断 |
| 低 | 多窗口内存 | 每个窗口一个 WebContent 进程，和现在相同（每个文档约 48 MB） | 不变 |
| 低 | Windows / Linux 引擎差异 | WebView2（Chromium）和 WebKitGTK 在 `window.find`、输入法、字体上与 WKWebView 有差异 | 查找改为 ProseMirror 实现（本来就计划要改）；首发只做 macOS |
| 低 | 包体积 | Tauri 外壳很小；编辑器 bundle 仍是 6.4 MB（mermaid 占一半） | 继续沿用 PERF.md 的结论，不拆分 |

---

## 5. 需要你决策的问题

每项都给了我的建议；没有特别说明时，“建议”就是默认方案。

- **D1 · bundle id**：继续用 `com.aaron.vellumi`（Sparkle 能原地升级，文件关联不变），还是开发期用 `com.aaron.vellumi.next`、到 P7 再切换？**建议：开发期用 `.next`，P7 切回原 id。**
- **D2 · 沙盒**：保留（为将来上 Mac App Store 留余地，但书签、授权面板、`assets/` 授权都要在 Rust 里重做），还是去掉（没有授权弹窗，实现简单，但不能上架 MAS）？**建议：去掉沙盒。** 现在的分发方式是 DMG + Homebrew，沙盒带来的主要是“首次粘贴图片要授权文件夹”这类摩擦。
- **D3 · 标签页形态**：原生窗口标签（与现在一致，只有 macOS 有），还是窗口内的 Web 标签栏（设计稿的画法，三个平台一致，但要改成单窗口多文档）？**建议：首发用原生标签，Web 标签栏放到跨平台阶段再做。**
- **D4 · 新增快捷键**：⇧⌘F 全文搜索。macOS 上没有冲突，但属于新增，需要你同意。
- **D5 · 无法完整复刻的 NSDocument 功能**：
  - “浏览所有版本”：去掉，还是在 App 数据目录里自己保存快照历史？
  - 标题栏代理图标和重命名菜单：去掉，还是用命令面板里的“重命名 / 移动 / 在 Finder 中显示”替代？
  - **建议：Versions 改为自建快照（每次保存保留最近 N 个版本）；代理图标去掉。**
- **D6 · 编码回写**：UTF-16 和带 BOM 的 UTF-8 文件，保存时保持原编码（更符合“格式保持兼容”），还是延续现在的做法统一转成 UTF-8？**建议：保持原编码。**
- **D7 · 设置迁移方向**：一次性单向迁移（新版写自己的 `settings.json`，旧 plist 保持只读，退回旧版时设置停在迁移那一刻），还是持续双向同步？**建议：单向迁移。**
- **D8 · 前端框架**：外壳 UI 用 Svelte 5、React 还是原生 TS？编辑器部分不受影响（Crepe 内部用 Vue，已经封装好）。**建议：Svelte 5。** 界面规模适中，运行时小，响应式写法和设计稿的状态模型最贴近。
- **D9 · 查找行为变化**：改为 ProseMirror 内搜索后，会显示当前序号、可以区分大小写、计数和替换也能跨格式匹配。这改变了 ⌘G 的细节行为（不再用 WebKit 的选区查找）。确认接受？
- **D10 · 现存缺陷**（§1.9）：在重写时直接修掉，还是先 1:1 复刻、再单独修？**建议：直接修**，并在 P8 的清单里标为“已修缺陷”。最可能影响你使用习惯的是：
  - 项目和快速打开改为递归扫描子文件夹（B10）
  - 去掉 .txt，或者正式支持 .txt（B6，二选一）
  - 被放弃的空草稿文件自动删除（B18）
- **D11 · PDF / 打印的输出来源**：改为离屏渲染清理过的 HTML（与 HTML 导出、图片卡统一，解决丢行和变灰的问题），还是继续对实时编辑视图截取？**建议：离屏渲染。**
- **D12 · 签名与公证**：继续 ad-hoc 签名（用户首次打开需要右键或去系统设置放行），还是申请 Apple Developer ID 并做公证？这会影响 hardened runtime 和更新的实现方式。
- **D13 · 设计稿里的其他有意改动**（画布左侧便签）：
  - 活动栏合并两个侧栏
  - Front matter 属性条
  - 命令面板合并三种模式
  - 图片卡选项记忆
  - 文件树自动刷新

  请逐条确认，或者告诉我哪些不要。

---

确认以上决策后，我会把结论写回本文，然后从 P0 开始。
