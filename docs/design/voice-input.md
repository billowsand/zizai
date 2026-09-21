# Windows 语音输入

## 目标

在不改变拼音输入热路径的前提下，把 `auto-voice` 的本地语音识别能力接进 Windows 输入法：
按一下可配置的语音键开始录音，再按一下后离线识别；也可开启“停顿后自动完成”，在确认收到有效语音后用连续静音结束录音。识别结果经 TSF 编辑会话直接写进开始录音时的文本框。

当前纵向切片包含 SenseVoice、可选同音词 FST、实时电平、临时转写、设置开关和原生上屏。LLM 润色和模型下载后续再接，
不能让附加能力阻塞键盘输入或基本语音输入；临时转写失败也不影响松键后的完整识别。

## 边界

- 语音是一次临时动作，不是中 / 英模式，也不是一种拼音方案。
- `qingjian-core` 不依赖音频、Windows API 或 ASR；语音采集与上屏属于 Windows 壳。
- ASR、重采样和确定性后处理放独立 `qingjian-voice` crate，Windows Worker 只负责进程入口与 IPC。
- TSF DLL 不加载模型、不访问麦克风、不做文本后处理；它只收语音键、同步状态并经 TSF 编辑会话写文档。
- 模型不进仓库。安装包可把语音做成可选组件；没有模型或 Worker 失败时，键盘输入必须照常可用。

## 进程与数据流

```text
RightAlt release (toggle)
       │
       ▼
TSF DLL ── VoiceStart / VoiceStop ──▶ Qingjian Server
  ▲                                       │
  │                                       ▼
  │                              qingjian-voice-worker
  │                              采音 → 重采样 → ASR → FST
  │                                       │
  └── ModeSync.voice / VoiceAck ◀─────────┘
       │
       ▼
RequestEditSession → 开始录音时的文本框
```

Worker 与 Server 分进程，原因是模型体积大，且音频 / ONNX 原生代码故障不能拖垮输入法 Server。
功能启用时 Worker 启动并随 Server 常驻；Server 负责焦点、会话、状态同步和失败降级。两者用带长度前缀的 stdio 私有协议通信，
不再额外暴露一条命名管道。

**Server 这侧的管道读写不在工人循环上。** Router 是单线程的，所有应用的按键都排在它后面；在那上面同步等 Worker
回话，等于把「Worker 卡住」变成「全系统打不出字」——分进程就白分了。`ProcessVoiceBackend` 因此只留两个不阻塞的接口：
命令投进队列，状态读共享快照；管道由 `qingjian-voice-ipc` 线程独占，有活儿时 40 ms 刷一次快照、空闲 500 ms 一次。
进程起不来、崩溃或管道坏掉都写成带原因的 `Failed` 快照，下一次 Start 才重新拉起干净进程；停用时先请它自己退出，
300 ms 宽限期内没走就杀，不做无上限的 `wait`。

## 界面融合

语音不是独立 OSD。TSF 在开始录音时用只读编辑会话量当前插入点，Server 继续使用原候选窗口的 HWND、定位、字体、主题、圆角与阴影，只把内容从候选帧切成语音帧：

```text
┌────────────────────────────────────────┐
│ ▁▂▅▇▄▂▆█▅▃  正在听                    │
│ 我们把语音输入融入现有候选窗口         │
└────────────────────────────────────────┘
```

- 第一次按键：波形从左向右滚动，右侧显示本轮录音时长；还没有临时文字时提示开始说话，持续两秒未收到有效电平则提示声音较小。
- 录音中：临时转写显示在第二行，只作反馈，不参与上屏与 ACK。
- 再次按键：同一窗口切到“正在识别”，冻结本轮时长并保留最后一条临时转写，避免界面跳空。
- 开启停顿自动完成时：至少 160 ms 的连续有效声音才算开始说话，随后连续静音达到配置时长即走同一条识别流程；瞬时敲键声不会触发。
- 完成：短暂显示最终文字并由 TSF 编辑会话写入；写入成功 ACK 后收窗。
- 普通拼音输入到来时，窗口直接恢复候选帧，不存在两个浮层争抢位置。

## 状态与交付语义

一次听写有单调递增的 `request_id`，状态为：

```text
Disabled / Loading / Idle → Recording → Recognizing → (Polishing) → Ready
                                      ↘ Failed
```

`Recording` / `Recognizing` / `Polishing` / `Ready` 是「进行中」的四个阶段，三处判定必须一起改：
Server 的 `VoiceCoordinator::is_active`（挡住通用收窗）、下发给 DLL 的 `VoiceSync::is_active`（80 ms 同步与 Esc 取消）、
候选窗的 `reconcile_voice`（这一帧画什么）。漏一个阶段就是「界面停在上一阶段、Esc 失灵、再按一次被当成新开始」。

每个会停住的阶段都有墙钟保险：识别 45 秒、润色 20 秒、`Ready` 等 ACK 10 秒。到点取消 Worker 请求、回到
`Failed` 并给短提示——`Ready` 卡住尤其要兜，它会一直占着候选窗口。

- `VoiceStart` 把请求绑定到当前 `SessionId`；同一时刻全局只有一次录音。
- `VoiceStop` 只结束同一请求；重复按键和系统自动重复不新建请求。
- `VoiceCancel` 直接回到 `Idle`，不保留中间音频与结果。
- Server 只把 `Ready` 文本交给绑定且仍在前台的会话。失焦、关闭会话、切换输入法或用户继续键入都会取消待交付结果。
- `ModeSync` 带可选 `VoiceSync`，其中电平用 `0..=1000` 整数传输、临时转写只发给绑定会话。旧 DLL 会忽略新字段，新 DLL 读旧 Server 缺失字段时退到禁用。
- DLL 收到 `VoiceDelivery { request_id, text }` 后只排队一次编辑会话；写入成功再发 `VoiceAck`。Server 在确认前保留交付，避免响应丢失；
  DLL 本地记住已排队的 id，避免轮询重复写入。

## 热键与轮询

- 快捷键配置放 `[shortcut] voice`，首版支持单个物理键，缺省 `right_alt`；`off` 关闭。
- TSF 的 `OnTestKeyDown` / `OnKeyDown` 和 `OnTestKeyUp` / `OnKeyUp` 单独处理语音键，不把它硬塞进普通 `KeyEvent`；动作在 KeyUp 切换，以兼容只向 TSF 交付右 Alt 松开事件的应用。
- 按住语音键期间按过别的键，松手不触发：右 Alt / 右 Ctrl 同时也是用户的组合键，不能让「右 Alt + 某键」顺带开一次录音。
- 没在录音时沿用 320 ms 的状态同步；录音、识别或待确认期间按现有 80 ms 定时器同步。
- 语音 IPC 或模型加载不进入 `OnKeyDown` 同步热路径；按键回调只发一条有界消息。

## 隐私与失败处理

- `KEYBOARD_DISABLED` 的密码框不拦语音键、不启动麦克风。
- 首版不写识别正文日志，只记状态、耗时和字符数；不开 LLM、不联网。
- 焦点变化后到达的旧结果丢弃并记字符数，不记录正文。
- Worker 不存在、模型缺失、麦克风打不开或识别失败时，在候选窗 / 状态条给短提示；TSF 不断连，普通按键不受影响。
- 复用自 `auto-voice` 的源码保留 MIT 版权与许可声明；模型许可证独立核对，不随源码许可推断。

## 配置

```toml
[shortcut]
voice = "right_alt" # right_alt / right_ctrl / caps_lock / scroll_lock / off

[voice]
enabled = false
model = "data/voice/sense-voice/model.int8.onnx"
tokens = "data/voice/sense-voice/tokens.txt"
language = "auto"
input_device = ""
auto_stop_ms = 0 # 0 为关闭；设置页打开时使用 1200 ms
hr_lexicon = ""
hr_rule_fsts = ""
```

路径相对安装根目录解析，绝对路径供开发与自定义模型使用。配置热加载只改变下一次录音；正在录音的一次使用开始时的快照。
设置页打开时通过 WASAPI 列出输入设备并只保存设备名；真正打开麦克风仍在 Worker 中完成。空名称跟随 Windows 默认设备，指定设备不可用时明确失败，不静默换到其他麦克风。

## 分阶段实施

1. 协议与状态机：配置、`VoiceAction` / `VoiceSync` / `VoiceAck`、会话和重复交付测试。
2. Worker：抽取 auto-voice 的 SenseVoice、重采样与按需麦克风，建立私有 stdio 协议和模型加载状态。
3. Server：启动 / 重连 Worker，绑定会话，合并到状态同步，错误退化。
4. TSF：收按下 / 松开、语音期 80 ms 同步、编辑会话提交和成功 Ack。
5. 产品面：设置页、Worker 安装及与候选窗融合的波形 / 临时转写 UI 已接；模型下载 / 可选打包、真机应用矩阵待完成。
6. 增强：LLM 双候选、从用户词与领域词库生成语音热词。

## 首版验收

- RightAlt 第一次按键才占用麦克风，第二次按键立即关闭；空录音不上屏。
- 记事本、浏览器、Electron 与终端里不经剪贴板即可上屏中文。
- 英文直输且没有拼音组句时，语音面板仍锚定当前插入点；录音、识别、最终上屏全程不出现第二个独立浮窗。
- 录音后切换窗口，结果不会写到新窗口；重试 / 重连不会重复上屏。
- 密码框不启动语音；日志不含识别正文。
- Worker 未安装、模型缺失、Worker 崩溃时仍可正常键盘输入。
- `qingjian-platform` 协议兼容测试、Server 会话测试、TSF 协议环路测试和 workspace 格式 / lint / 测试通过。

## 来源

语音识别、麦克风采集与重采样实现源自 `auto-voice`（MIT，Copyright (c) 2026 billowsand），合入时保留许可文本与署名。
