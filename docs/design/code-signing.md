# Windows 代码签名方案（SignPath Foundation）

2026-09-18 定的：走 **SignPath Foundation** 的免费开源代码签名，构建与签名全部在 GitHub Actions 里完成，
本地不再打对外分发的包。本文是唯一的操作手册：申请、面板配置、仓库配置、发布时序、排查。

## 为什么是它

输入法（IME）在 Windows 上有签名硬门槛：

- 未签名的 TSF DLL **进不了 UWP / 系统应用**（任务栏搜索、「设置」、商店应用）——TSF 声明了
  `GUID_TFCAT_TIPCAP_IMMERSIVESUPPORT` 也没用，宿主进程只加载受信任签名的 DLL。
- 未签名安装包被 SmartScreen 强拦截（「Windows 已保护你的电脑」）。
- Server 带 uiAccess（候选窗盖过系统界面）的前提是 exe 已签名且装进 Program Files。

签名路径对比（2026-09 调研，详见当时的调研结论）：

| 方案 | 费用 | 中国大陆个人 | 结论 |
|---|---|---|---|
| **SignPath Foundation** | **0** | ✅ | **选定**。证书签发给基金会、私钥在基金会 HSM，代价是构建必须全程 CI 可验证 |
| Certum Open Source | €25–69/年 + 运费 | ✅ | 备选。发布者带 "Open Source Developer" 前缀 |
| 微软 Artifact Signing | $9.99/月 | ❌ 个人仅限美国/加拿大 | 有组织实体在美加欧英时可切换 |
| OV / EV 证书 | $150–400+/年 | ✅ | EV 自 2024 年起也不再即时过 SmartScreen，不划算 |
| 自签 | 0 | — | 只值开发机自测，对外无意义 |

## 整体时序

```
推 windows-v* 标签
  → CI 构建五个 PE（TSF DLL x64/x86、Server、语音 Worker、设置程序）
  → 上传为 GitHub artifact → SignPath 签名请求 ①（二进制）
  → 审批人在 SignPath 面板点批准（基金会条款：每次发布都人工批）
  → 签回、覆盖回 target\ → Inno 打包（已签文件原样进包）
  → 再传 artifact → SignPath 签名请求 ②（安装包外层）
  → 审批 → 签回 → SHA256SUMS / build-info.json → 建 Release
```

`release.yml` 里 SignPath 的 secrets/vars 配齐才走签名；没配齐自动退回旧行为（不签名、uiAccess=0），
所以**申请期间不挡发版**。

## 一、申请（一次性，人工）

1. 打开 <https://signpath.org/apply.html> 提交申请，填仓库地址（`billowsand/qingjian`）、下载页
   （`https://qingjian.app`）、许可（GPL-3.0，OSI 认可 ✅）与项目简介。
2. 资格自查（[基金会条款](https://signpath.org/terms.html)，违反会被拒/吊销）：
   - 所有组件 OSI 许可、无私有代码、无商业双许可 ✅（GPL-3.0 全仓库）
   - 项目已公开发布、持续维护、有下载页与功能描述 ✅
   - 只签自己从自己的源码构建的产物 ✅（上游未签名的 OSS 二进制**不许**代签，可以装进已签安装包）
   - 团队成员：GitHub 与 SignPath 都开 MFA；角色分 Authors（可直接推代码）/ Reviewers（审 PR）/
     Approvers（批签名请求），并在项目首页公示（见下「官网文案」）
   - **每个 Release 的人工审批**是条款一部分，不能绕过
3. 等审批（会看仓库与下载页，几天量级）。

## 二、SignPath 面板配置（获批后，一次性）

### 1. 项目与证书

基金会会建好 project 并把证书挂好（证书 CN 是 SignPath Foundation，发布者就显示它）。记下：

- **Organization ID**：用户菜单 → Organizations
- **Project slug**：项目设置里

### 2. Trusted Build System 与 GitHub App

- 组织里确认预置的 **GitHub.com** trusted build system 已存在并链接到本项目
  （它是 origin verification 的基础：SignPath 会向 GitHub 核实产物确实由这个仓库的 workflow 构建）。
- 给组织装 **SignPath GitHub App** 并授权本仓库（审计日志评估要用；不装也能签，但策略评估能力受限）。

### 3. 两个 Artifact Configuration（签名规则）

Projects → 本项目 → Artifact Configurations → Add → Custom，XML 如下（**原样贴**，slugs 自定，
建议 `binaries` 与 `installer`，写进下面的 GitHub vars）。`${version}` 由 CI 以构建版本传入，
`product-name` / `product-version` 是对 PE 里 VERSIONINFO 的**强制校验**，与 `qingjian.iss`
的 `VersionInfoProductName/ProductVersion` 和各 crate `build.rs` 嵌的值对应——改了版本元数据约定要同步改这里。

**binaries**（五个 PE 打成 zip 提交，zip 根元素）：

```xml
<artifact-configuration xmlns="http://signpath.io/artifact-configuration/v1">
  <parameters>
    <parameter name="version" required="true" />
  </parameters>
  <zip-file>
    <pe-file path="qingjian_tsf.dll" product-name="Qingjian" product-version="${version}">
      <authenticode-sign />
    </pe-file>
    <pe-file path="qingjian_tsf-x86.dll" product-name="Qingjian" product-version="${version}">
      <authenticode-sign />
    </pe-file>
    <pe-file path="qingjian-server.exe" product-name="Qingjian" product-version="${version}">
      <authenticode-sign />
    </pe-file>
    <pe-file path="qingjian-voice-worker.exe" product-name="Qingjian" product-version="${version}">
      <authenticode-sign />
    </pe-file>
    <pe-file path="qingjian-settings-egui.exe" product-name="Qingjian" product-version="${version}">
      <authenticode-sign />
    </pe-file>
  </zip-file>
</artifact-configuration>
```

**installer**（安装包单文件打成 zip 提交）：

```xml
<artifact-configuration xmlns="http://signpath.io/artifact-configuration/v1">
  <parameters>
    <parameter name="version" required="true" />
  </parameters>
  <zip-file>
    <pe-file path="Zizai-${version}-Setup.exe" product-name="Qingjian" product-version="${version}">
      <authenticode-sign />
    </pe-file>
  </zip-file>
</artifact-configuration>
```

文件清单与 `release.yml` 的 `Stage unsigned binaries` / `Upload unsigned installer` 步骤一一对应，
改名要先改两边。时间戳由 SignPath 自动打（证书过期不影响已签文件验证）。

### 4. Signing policy 与 API token

- Policies：先用 `test-signing`（自动批准，适合联调），跑通后换 `release-signing`
  （人工批准，正式启用）。两个 slug 写进 GitHub vars，切换只改变量。
- 用户菜单 → API tokens：建一个 token（submitter 权限），只给本项目用。

### 5. GitHub 仓库配置

| 位置 | 名称 | 值 |
|---|---|---|
| Secrets | `SIGNPATH_API_TOKEN` | 上一步的 token |
| Variables | `SIGNPATH_ORGANIZATION_ID` | Organization ID |
| Variables | `SIGNPATH_PROJECT_SLUG` | 项目 slug |
| Variables | `SIGNPATH_SIGNING_POLICY_SLUG` | `test-signing`（联调）/ `release-signing`（正式） |
| Variables | `SIGNPATH_ARTIFACT_CONFIG_BINARIES` | `binaries` |
| Variables | `SIGNPATH_ARTIFACT_CONFIG_INSTALLER` | `installer` |

## 三、发版时的操作

1. 正常发版（`docs/notes/release.md`）：改版本号、CHANGELOG、推 `windows-v*` 标签。
2. CI 构建完会挂在签名请求上等批准：**Approvers 去 SignPath 面板 → Signing requests 点批准**
   （二进制与安装包两笔，都会邮件通知）。超时不批，CI 那步 2 小时后失败，重跑该 job 即可。
3. 批完自动继续，Release 创建成功即完成。

## 四、元数据约定（改之前先读这里）

- `ProductName` 统一 **`Qingjian`**（ASCII：winresource 生成的 `.rc` 对非 ASCII 不可靠，中文产品名
  只出现在安装向导与 VersionInfo 之外的地方）。
- `ProductVersion` 统一安装包版本：CI 设 `QINGJIAN_PRODUCT_VERSION`，`server` / `tsf` /
  `settings-egui` 的 `build.rs` 都读它（本地构建回落各自 crate 版本）。**同一次构建内必须一致**，
  这是基金会条款的强制校验，不一致签名请求直接失败。
- 安装包的 `VersionInfoProductName/ProductVersion` 在 `qingjian.iss`。
- WinUI 对比包（`-WinUiSettings`）不走签名流程，也没嵌版本元数据——想给它签名要先补
  `apps/windows/settings/build.rs` 的元数据。

## 五、官网「Code signing policy」文案（基金会条款要求）

官网仓库（qingjian-team/qingjian-web）需有一个页面/章节，标题或链接名必须是
**Code signing policy**，包含以下要素（英文模板，可照贴后把角色链接换成真的）：

```markdown
## Code signing policy

Free code signing provided by [SignPath.io](https://signpath.io), certificate by [SignPath Foundation](https://signpath.org).

- Committers and reviewers: [Members team](https://github.com/orgs/<org>/teams/members)
- Approvers: [Owners](https://github.com/orgs/<org>/people?query=role%3Aowner)

This program will not transfer any information to other networked systems unless
specifically requested by the user or the person installing or operating it.
```

## 六、排查

| 现象 | 原因与处理 |
|---|---|
| 签名请求报 product-name/version 不符 | 某个 PE 或安装包的 VERSIONINFO 与 artifact configuration 不一致。本机用 `Get-AuthenticodeSignature` 只能看签名；看元数据用资源管理器属性→详细信息，或 `(Get-Item x.exe).VersionInfo.ProductName`。先查 CI 是否真把 `QINGJIAN_PRODUCT_VERSION` 传进了 cargo |
| 请求报文件找不到 | `Stage unsigned binaries` 的文件名与 artifact configuration 的 `path` 对不上（改名要两边同步） |
| 请求被拒：origin verification failed | 构建 job 必须全在 GitHub-hosted runner（我们是 `windows-latest` ✅）；workflow 改动后确认没引入 self-hosted job |
| action 等超时失败 | 没人批准。去面板批，然后重跑失败的 job（release job 从头跑，有 rust-cache 与重下数据，约十分钟） |
| 安装后 UWP 里还是切不到字在 | 看 `%ProgramFiles%\Qingjian\qingjian_tsf-<版本>.dll` 右键→属性有没有「数字签名」页；没有说明签回环节没生效，查 CI 日志里 `Sign binaries` 步骤 |
| SmartScreen 仍提示 | 正常。OV 级证书要积累信誉，持续用同一身份签名会随版本继承；确保别频繁换证书来源 |
