# 发版流程

2026-09-07 搭起来的：GitHub Actions 按标签打包、建 Release、生成官网下载页用的 `releases.json`。
这里记怎么发一版、各环节的依赖，以及官网怎么消费产物。本项目只发 Windows。

## 一次发版做什么

1. 改 `apps/windows/{server,tsf,settings,voice-worker,settings-egui}/Cargo.toml` 的 `version`（五处一起改；打包脚本与 workflow 读 `server` 那份）。
   别只改 `server`：`tsf`、`settings`、`voice-worker`、`settings-egui` 漏改会发出版本号不一致的工件（最近一次就栽在这里，补了一次 `chore(release): 锁文件对齐 …`），按这份清单五份一一覆盖。
   **发版之间版本号一直带 `-dev`**（Rust nightly / Firefox Nightly 那套）：Cargo.toml 写 `0.1.3-dev`，`build.ps1` 打包时再接上 git 短哈希，
   本地装的、CI 中间构建的都显示 `0.1.3-dev-1a2b3c4`（工作区有改动加 `+`），测试时一眼知道装的是哪个提交；版本号干净的一定是线上包；
   带 `-dev` 的标签 CI 直接拒绝。Inno 的 `VersionInfoVersion` 只认数字点号，`build.ps1` 去掉后缀再传，安装包与 DLL 文件名保留完整版本。
2. `CHANGELOG.md` 顶上加一节 `## <版本> · <日期> · <渠道>`（渠道是 `alpha` / `beta` / `rc` / `stable`），一行一条、面向用户的措辞。
   提交信息里有大量内部改动（拆模块、修 RefCell 重入、改 IPC 协议），用户看不懂也不关心，最终定稿必须**面向用户**——
   内部用词（「`Sentence` kind 置位」「`StepDown` 接管」「`StepDown` 事件」这类）一律换掉。
   起稿可以交给 LLM：发版前按上个标签以来的 `git log` 让模型列条目，再由维护者审一遍、剔掉内部词、把同质改动合并，最后人签字再定稿。
   模板本身就当系统提示给模型（见附录）。
3. 提交，打注释标签并推：`git tag -a windows-v0.1.3 -m "字在 Zizai Windows 0.1.3" && git push origin main windows-v0.1.3`
   （标签带平台前缀 `windows-v*`，与上游的 `macos-v*` 区分；旧的 `v*` 标签仍能被官网识别，向后兼容）。
4. 标签推出去之后紧接一个普通提交把版本号改成下一个开发版（只是改 Cargo.toml，不打标签、不建 Release；-dev 版本永远没有标签与 Release）。
5. `release.yml` 跑完后 GitHub Release 上有 `Zizai-<版本>-Setup.exe`、`SHA256SUMS`、`build-info.json`（提交、构建时间、工具链）、`releases.json`。

workflow 会核对 `apps/windows/server/Cargo.toml` 版本号与标签（去掉 `windows-v` 前缀后）一致，不一致直接失败，避免打出版本号错的包。

Rust 工具链由 `rust-toolchain.toml` 钉版本（现在 1.96.0），workflow 里 `dtolnay/rust-toolchain` 的 `toolchain:` 输入写同一个号；升级 Rust 时两处一起改。

## 发版 job 里做什么

`release.yml` 的 `windows` job 在 `windows-latest` 上：核对版本 → 下载 `data` Release 的产品数据并按 `SHA256SUMS` 校验
→ 装 Inno Setup 7.1.0（与开发机同版本，钉死 GitHub Release 上的安装程序并核对 SHA-256）→ `build.ps1` 打安装包
→ 建 Release（`Zizai-<版本>-Setup.exe` + `SHA256SUMS` + `build-info.json`）→ `publish-releases-json.sh` 生成 `releases.json`，
挂到本次发布并覆盖到 GitHub latest 那版上（官网只读 latest 的）。

**代码签名走 SignPath Foundation 免费开源证书**（申请、面板与仓库配置、审批时序见 `docs/design/code-signing.md`，
一、二、五步是一次性配置）：CI 里 `secrets.SIGNPATH_API_TOKEN` 与 `vars.SIGNPATH_PROJECT_SLUG` 配齐即启用——
构建五个 PE（含语音 Worker）→ 送 SignPath 签回 → 打包（已签文件原样进包）→ 安装包再签 → 建 Release。
签名请求要审批人在 SignPath 面板手动批准（基金会条款：每个 Release 都批），CI 挂起最长等 2 小时。
签名生效后 Server 自动嵌 uiAccess（`QINGJIAN_UIACCESS=1`，签名链就绪才置位）：候选窗不再被任务栏搜索 /
「设置」这类高 z-band 宿主盖住；TSF DLL 签名后也能进 UWP / 系统应用。没配齐时退回旧行为（不签名、uiAccess=0），不挡发版。
SmartScreen 信誉仍要随版本积累（OV 级证书不即时放行），Release 说明里已向用户解释。

## 提交前检查与 CI

本地 `git config core.hooksPath .githooks` 启用一次后，每次提交前 `.githooks/pre-commit` 先拒绝装饰性分隔注释（`// ====` / `// ────`，只做视觉分组不带「为什么」），再跑 `cargo fmt --check` 与 `cargo clippy -D warnings`（增量几十秒）；
`.githooks/pre-push` 在推之前跑全 workspace 测试。外部 PR 走同一套 `ci.yml`，不过不合。

供应链：workflow 里的 actions 一律钉到 commit（注释写对应标签），`.github/dependabot.yml` 每周一提 Cargo 与 actions 的更新 PR；`audit.yml` 每周与 Cargo.lock 变动时跑 `cargo audit`；
cargo 命令全 `--locked`（含 `build.ps1`）。普通 CI 只有 `contents: read`，checkout 不留凭据；release 的 secrets 不放顶层 env，只注入用它的那一步。

发版门禁（`release.yml` 第一步）：版本号与标签一致且不带 `-dev`；标签指向的提交必须在 `main` 上（`git merge-base --is-ancestor`）；产品数据下载后按 `data` Release 的 `SHA256SUMS` 校验，摘要写进 `build-info.json` 的 `data_sha256`。
**正式版前还欠**：产品数据改成不可变 tag 并在仓库里锁定版本（现在滚动覆盖，同一源码 tag 重跑可能拿到不同数据）、安装包内容验证（词库 / 模型 / 许可齐不齐、签名校验）。

## 两个 workflow

| 文件 | 触发 | 做什么 |
|---|---|---|
| `.github/workflows/ci.yml` | push main、PR | Linux 上 `cargo fmt --check` / clippy / test（Core 与协议层与平台无关）；Windows 上编 Server / TSF DLL / Settings 并跑测试 |
| `.github/workflows/release.yml` | 推 `windows-v*`（含 `windows-v*-bundled`）标签 | 下载产品数据 → 构建 → SignPath 签回二进制（配齐时）→ `build.ps1` 打 Inno Setup 安装包 → 签安装包 → 建 Release → 生成 `releases.json` |

## bundled 变体（带本地模型的安装包）

`windows-v<版本>-bundled` 标签走同一份 `release.yml`，但走旁支产出一个 `Zizai-<版本>-bundled-Setup.exe`，
**追加**到该版本标准 release 的 asset 列表（不新建 release，不动 `latest`）。

- 与标准版共享 `[AppId]`、同一份 `qingjian.iss`，所以从普通版直接装 bundled 是覆盖升级、模型被卸走；
  反过来从 bundled 装到普通版同理。升级提示里 `Keep [voice]` 之类配置和用户词照旧保留（`%APPDATA%\Qingjian` 不动）。
- 随包的数据：标准版那 8 套（`.qj`、英文表、`model.qjm`、emoji、`xiaohe` 等）以外，加 SenseVoice `model.int8.onnx` + `tokens.txt`、
  标点恢复 `model.int8.onnx`、同音词 `lexicon.txt` + `replace.fst`。安装包大小由 ~80 MB 涨到 ~280 MB。
- tag 门禁：与标准版同样——`apps/windows/server/Cargo.toml` 版本号 = 标签去掉 `-bundled` 后缀；
  `0.1.11-bundled` 允许指向 **已经发过版的 commit**（也就是 `f1ae90a` 或更早带 dev 的 `1313253`，只要 5 份 windows 子 crate 还是 `0.1.11` 即可），
  不要求是最新 main HEAD。这样一个版本号可以补几个日期的 bundled asset，而不踩「-dev 不让 tag」的规则。
- 第三方模型许可（Apache-2.0）随 `THIRD_PARTY_NOTICES.md` 进安装包；release notes 里多一段写明「本安装包含本地模型」
  并列出每份资源的来源 tag，方便用户对照官方 release 做合规审计。

打一个 bundled 变体的具体步骤：

1. 选已经发过版的那个 commit（最简单：保持已发版的 `windows-v<版本>` commit 不动，或者用 newest main 上同一份 `0.1.11` 复刻）。
2. `git tag -a windows-v<版本>-bundled -m "字在 Zizai Windows <版本> bundled"` 指向那份 commit，
   `git push origin windows-v<版本>-bundled`。
3. CI 跑 `release.yml`，多一个 `Pre-fetch bundled models` 步骤从 sherpa-onnx release 下载三份模型到 `target/bundled-models/`
   （SHA-256 由维护者手填到 workflow 注释里；下游 tag 不重发，CI 自动重打时 hash 不变则放行）。
4. build.ps1 `-VoiceModelDir / -PunctuationModelDir / -HrDir -BuildLabel bundled` 编出 `Zizai-<版本>-bundled-Setup.exe`。
5. `gh release upload` 追加到 BASE_TAG（即 `windows-v<版本>` 标准版）的 asset 列表；
   不动 `SHA256SUMS` / `build-info.json`（仍是标准版那次构建时写的内容）。
6. 重新生成 `releases.json`（按 BASE_TAG），新 asset 自动出现在官网下载页「其他平台/历史资产」那一档里。

## 产品数据从哪来

词库、语言模型、释义表（`data/generated/*.qj`、`dicts/*.qj`、英文词表）不在 git 里，体积约 85 MB 且由本机数据管道生成。
`tools/release/data-bundle.sh` 把它们打成 `qingjian-data.tar.gz`，把本地整句模型单文件 `data/model/model.qjm`
（训练仓库导出三件套到 `data/model/`，`tools/release/pack-model.sh` 打成一个 `.qj` 容器，fp16 约 56 MB，元数据也写在那个脚本里）
原样上传，连同 LLM 生成的续跑中间产物 `qingjian-llm-intermediates.tar.gz` 一起放到仓库里一个名为 `data` 的**预发布** Release
（预发布不会成为 GitHub 的 latest，官网取 latest 时不会拿到它）。
`release.yml` 用 `gh release download data` 取回，数据包解到 `data/generated/`、`model.qjm` 放到 `data/model/`；
`qingjian.iss` 把 `data\generated` 的 `dict.qj` / `lm.qj` / 英文词表 / `dicts\*.qj` 与 `data\model\model.qjm` 装进 `{app}`。
两者的 SHA-256 都记进 `build-info.json`（`data_sha256` / `model_sha256`）。

数据重生成之后（重跑 lexicon / bigram / gloss-gen export）或模型重训之后要重跑一次 `data-bundle.sh`（三件套比 `.qjm` 新会自动重打），
否则 CI 打的包还是旧数据。模型文件缺失时 CI 会失败（校验那一步），不会静默地发出不重排的包。

## releases.json：官网下载页的数据源

`tools/release/releases_json.py` 从 `CHANGELOG.md`（日期、渠道、更新日志）、GitHub Releases API（附件、地址、大小）
与每次发布的 `SHA256SUMS` / `build-info.json`（每个包的 sha256、提交哈希、构建时间、工具链）生成，挂在每个版本的 Release 上；官网固定取
`https://github.com/<repo>/releases/latest/download/releases.json`（仓库私有期间要带令牌走 API 下载附件）。

结构对应官网 `src/lib/releases.ts` 里的 `Release` / `Asset` 类型：

```json
{
  "generated": "2026-09-07T12:00:00Z",
  "repository": "owner/qingjian",
  "latest": "0.1.3",
  "releases": [
    {
      "version": "0.1.3",
      "date": "2026-09-07",
      "channel": "beta",
      "notes": ["整句输入：……", "本地整句模型重排候选……"],
      "commit": "869ad00…（40 位）",
      "built_at": "2026-09-07T08:38:12Z",
      "toolchain": "rustc 1.96.0 (ac68faa20 2026-05-25)",
      "assets": [
        { "platform": "windows", "arch": "x64", "file": "Zizai-0.1.3-Setup.exe",
          "url": "https://github.com/owner/qingjian/releases/download/windows-v0.1.3/Zizai-0.1.3-Setup.exe",
          "size": 79872000, "sha256": "…" }
      ]
    }
  ]
}
```

- `releases` 从新到旧，`latest` 是第一条的版本号；官网「当前版本」取它，历史版本列表就是整个数组。
- `channel` 是 `alpha` / `beta` / `rc` / `stable`，显示成什么字由官网定；`commit` / `built_at` / `sha256` 给用户核对下载的包，下载页应显示 sha256 与提交短哈希。
- 平台与架构由文件名判定（`-Setup.exe` → Windows x64），以后 Linux 的包在脚本的 `ASSET_KINDS` 里加一行。
- `SHA256SUMS` 与 `releases.json` 自己不列进 `assets`。

## 本机打包

`powershell -File apps/windows/installer/build.ps1`：release 构建 Server、语音 Worker、设置程序、TSF DLL + 32 位 DLL，再用 Inno Setup 编安装包，
成品在 `target\installer\Zizai-<版本>-Setup.exe`。数据或脚本改了、二进制没变时加 `-SkipBuild`；`-Sign` 用自签证书签产物（本机真机测用）。
**对外分发的包不要本地打**：走 `release.yml`（SignPath 签名，见 docs/design/code-signing.md）。CI 分段用的
`-NoPackage`（只构建）与 `-PackageOnly -PreSigned`（产物已被 SignPath 签回，打包前校验签名）一般只在 workflow 里用。

## 附录：CHANGELOG 起稿提示词模板

发版前把下面这段作为系统提示喂给模型，再把 `git log <上一个 windows-v* 标签>..HEAD --oneline` 的输出贴进用户消息。
模型只起稿，正文逐行人审。

```
你是「字在 Zizai」输入法的发版日志起草助手。维护者会审你写的每一条，必须 100% 面向用户、不带任何实现细节。

## 任务
- 输入：自上一个 windows-v* 标签以来的提交列表（Conventional Commits，第一行：<类型>(<范围>): <说明>）。
- 输出：一个版本节草稿，标题 ## <新版本> · <今天> · beta，一行一条 bullet，写入 CHANGELOG.md。

## 必守规则
1. **必须能讲成普通用户感受得到的事**。用户能感知的行为才有资格进 CHANGELOG：
   改了什么键、改了什么默认行为、改了候选 / 排序 / 主题 / 语音档位、修了一个「我会撞到」的 bug——
   任何一条都得让人读了能说「哦，这个我以前/现在能/不能……」。
2. **不能用工程内部词**。以下都是禁词，出现就改写或删：
   「StepDown」「FILE_FLAG_FIRST_PIPE_INSTANCE」「互斥体」「manifest」「embed-manifest」「uiAccess」
   「RefCell 重入」「IPC 协议」「自绘」「DLL」「Server 进程」「Worker」「Candidate」「CandidateKind」
   「Sentence」「整句路径」「LM」「bigram」「bigram 模型」「Local\Qingjian\… 事件名」「build.rs」
   「cross_join」「cfg」「dev」「-dev」后缀、「管道」「私有方法」「… impl 」的实施细节
   （实施细则词以 docs/design 与 crates/ 里的类型名为准，凡是不在 docs/user/ 的概念都不出现）。
3. 同一类别的提交合并成一条；零散提交若用户感觉不到也合并或舍弃。
4. 中文，简洁，一句话内可以；超过两句就拆。
5. 末尾给出该版本最关键的「已知问题」（无签名时写明候选窗在哪些界面被遮、SmartScreen 拦截提示；
   改动影响大但用户可能受惊时也写一条，例如「英文模式不再弹候选窗」已经发过的本版不必再写）。

## 不要做的事
- 不要解释为什么改、不要写修这个 bug 的根因
- 不要带提交短哈希
- 不要写「优化」「重构」这种空话，要写「**做**了什么」「**修了**什么」
- 任何字段不确定宁可省略也不要编

## 提交示例
- feat(voice): 添加语音停顿自动完成 → 「语音输入自动完成：停顿约 1.2 秒后自动结束并写入输入框，可在设置里关」
- refactor(windows): 单实例换成互斥体 + 构建标记，按会话号分管道 → 不出现在 CHANGELOG（用户感知不到）
- fix(core): 辅码筛空时回车 / 空格上屏整串，不再只剩主码 → 「辅码缩到没候选时，回车 / 空格上屏完整编码」
```
