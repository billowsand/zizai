# 发版流程

2026-09-07 搭起来的：GitHub Actions 按标签打包、建 Release、生成官网下载页用的 `releases.json`。
这里记怎么发一版、各环节的依赖，以及官网怎么消费产物。本项目只发 Windows。

## 一次发版做什么

1. 改 `apps/windows/{server,tsf,settings}/Cargo.toml` 的 `version`（三个一起改；打包脚本与 workflow 读 `server` 那份）。
   **发版之间版本号一直带 `-dev`**（Rust nightly / Firefox Nightly 那套）：Cargo.toml 写 `0.1.3-dev`，`build.ps1` 打包时再接上 git 短哈希，
   本地装的、CI 中间构建的都显示 `0.1.3-dev-1a2b3c4`（工作区有改动加 `+`），测试时一眼知道装的是哪个提交；版本号干净的一定是线上包；
   带 `-dev` 的标签 CI 直接拒绝。Inno 的 `VersionInfoVersion` 只认数字点号，`build.ps1` 去掉后缀再传，安装包与 DLL 文件名保留完整版本。
2. `CHANGELOG.md` 顶上加一节 `## <版本> · <日期> · <渠道>`（渠道是 `alpha` / `beta` / `rc` / `stable`），一行一条、面向用户的措辞。
   **更新日志手写，不由提交自动生成**：提交信息里有大量内部改动（拆模块、修 RefCell 重入），用户看不懂也不关心；
   做法是发版前按上个标签以来的 `git log` 起草几条，人审一遍再定稿。
3. 提交，打注释标签并推：`git tag -a windows-v0.1.3 -m "青简 Windows 0.1.3" && git push origin main windows-v0.1.3`
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
| `.github/workflows/release.yml` | 推 `windows-v*` 标签 | 下载产品数据 → 构建 → SignPath 签回二进制（配齐时）→ `build.ps1` 打 Inno Setup 安装包 → 签安装包 → 建 Release → 生成 `releases.json` |

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
