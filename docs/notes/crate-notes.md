# 各 crate 的实现要点

CLAUDE.md 只保留目录地图与规则，每个 crate / app / tool 的实现细节收在这里：入口类型、数据文件、常数、生成命令。
改了实现要同步改这里；与代码冲突时以代码为准。

## crates/qingjian-dictionary

词库（TSV 解析或 `.qj` mmap），键按字节序排好，查询逐音节位置二分收窄（简拼位置按音节块跳扫），
`lookup_pattern`（≥ 模式长度）与 `lookup_exact`（正好等长）同一套实现。词库键以 `v` 表示 ü，
TSV 解析、查询与生成工具把 `lue` / `nue` 统一成 `lve` / `nve`。
旧 `.qj` 含这些键时，加载器建立规范化的内存词库。新 `.qj` 继续使用 mmap。

## crates/qingjian-core

模块：`composition` / `parser` / `correction`（拼写纠错：整段一处编辑的候选纠正 + `typo` 音节级敲错变体表，后者进整句词图当带代价的边）/
`candidate` / `ranking` / `shortcut` / `sentence` / `shuangpin`（双拼：四套方案键位表、键 → 全拼解码与消耗换算）/ `emoji` /
`english`（英文词候选：精确词 / 前缀补全 / 一处编辑纠正，给中文模式下的中英混输用；英文模式本身是纯直通，不出候选）/
`fuma`（辅码：字级形码表 `FumaTable`，每行 `字=两码`，表在 `assets/fuma/xiaohe.txt`；
  词组辅码不存表，`expected_codes` 运行时按「单字两码、多字首字第 1 码 + 末字第 1 码」现算，`matches` 严格过滤，首末字不在表里即不匹配；
  同字重复后一条覆盖前一条，与水杉引擎 `HelpcodeUtils` 的语义一致。表权利归方案作者，随包分发前要先拿授权，否则改为运行时用户导入）/
  `engine/fuma.rs`（辅码键判定 `fuma_input`，两档：**一个小写键**（`ljm`）与「下一个字的声母」有歧义，
  不从解码里剥掉（简拼词 蓝莓 照常出），匹配的候选靠 `Scored::fuma_hit` 顶到排序最前（`FumaCodes::First`）；
  **两个键**（切不成一个音节的一对小写键，或含大写）没有歧义，从解码里剥掉并只留匹配的候选（`FumaCodes::Both`），
  第一键大写时两码反转。判定挂在每次 `Engine::decode` 上，
  所以写成零分配：末键按字节看，`decode_keys` 返回 `Cow`，辅码关着时原样借用键串。
  两码那档在 `query_inner` rank 前按 `expected_codes` 严格过滤，
  不出英文候选与 emoji，整句在 `plain_sentence` 里同规则过滤；首码那档只置顶，什么都不排除。
  上屏消耗：两码那档盖满拼音就连 2 键一起吃，盖不满的由 `commit::consume_scope` 丢掉；
  首码那档由 `commit::consumed_first_code` 判断——盖满辅码键之前那段拼音、首码又对得上的才多吃那一键，
  普通前缀候选（蓝）不吃，那一键留着当下一个字的声母。`take_raw` 剥两码那档；`delete_syllable_backward` / 音节光标跳认辅码键为字母。
  显示：`Candidate::fuma` 是这条候选自己的两码（敲了辅码才填）；只敲首码时 Server 取高亮候选的第二码
  淡画在拼音行末尾当「下一键」幽灵提示（`fuma_hint`，标在候选旁会让候选框高度跟着变）；
  `Query::fuma` 是要补画进拼音行的辅码段（只有两码那档，首码那档的键还在拼音里），
  经 `MarkedKind::Fuma` → `PreeditKind::Fuma` → `PreeditStyle::Fuma` 一路镜像，两条绘制路径都画淡。
  配置 `[general] fuma`，`Engine::set_fuma` 收 `Arc<FumaTable>`（表几千条，Server 与 Engine 共用一份，热加载只克隆指针）/ `fuma_enabled`）/
  `engine`（`query::EnglishTail`：句末英文词并入整句，`woxiangxuehaorust` → 我想学好rust，尾段也像拼音时按分数与拼音读法比）。
`Engine` 是对外唯一门面，`Translator` / `Learner` trait 在 `engine` 模块；词库是「主词库 + 附加词库（`set_extra_dictionaries`）+ 用户词」的列表。
- 中英混输的英文词位置：`Engine::set_chinese_first`（配置 `[general] chinese_first`，缺省关）关着时拼音不像话的输入英文排第一（`extras::insert_english`，
  用户老选中文词时仍让中文在前），开着时整句先插、英文词紧随其后排第二（`query_inner` 里两步的先后按开关掉转）；句末英文词并入整句（`EnglishTail`）不受它影响。
  缺省关是回放定的（9241 词 / 269 条英文上屏：缺省开英文首选 82.5% → 7.1%）。
`EngineSession` 保存可挂起的组句、标点、历史与学习链，`Engine::swap_session` 在同一个引擎里交换输入状态，共用词库与落盘服务。切换上下文时清除查询及异步预测缓存，并由平台恢复各自私密状态。

`Engine::discard_input` / `EngineSession::discard_input` 用于隐私能力变化时无痕清理输入，包括透传缓冲、学习链和暂存词汇曝光；`set_private` 只切换写入开关，保留已输入的组句。

## crates/qingjian-translate

`Glossary`，本地 TSV 释义表（词性 + 译文）；`LevelTable`，词汇等级表（`assets/levels/levels-{en,ja}.tsv`，CEFR A1–C2 / JLPT N5–N1，
`uv run tools/corpus/levels.py` 从 `data/levels/` 的原始 CSV 生成，来源与许可见 `assets/levels/README.md`），「统计」页按级数词汇用，不进候选。

## crates/qingjian-learning

- `FrequencyLearner`：用户选择次数（`user.tsv`）、按输入串记的选择（`user-choices.tsv`，词级排序里同输入串选过的优先）、用户词（`user-words.tsv`，主词库同格式，
  Engine 与主词库一起查）、个人英文词（`user-english.tsv`，中文模式下回车原样上屏的英文词与选过的英文候选，与随包英文词表一起出候选且在前）、
  个人敲错表（`user-typos.tsv`，接受过的 (敲的, 要的) 音节对，词图敲错边与整段纠错的代价按它打折）与个人 n-gram（`user-ngram.tsv`，Core `sentence::UserNgram`，
  二元 + 三元在线计数，整句转换与词级排序里与静态模型插值；
  连着选出的两个词记够次数自动造词进用户词，一段拼音分几次选完的合成词记两次也造）。
- `InputLog`：输入日志（`input-log.jsonl`，每次上屏一行：敲的键、切分、看到的前几个候选、选了第几个、来源、纠错、撤销，
  Core `InputLogger` trait 的落盘实现，`[general] input_log` 缺省开，只写本机，给离线回归评测与个人模型用）。
- `UsageStats`：输入统计（`usage.tsv`，按天记汉字 / 中文词 / 英文词 / 上屏次数，Core `UsageMeter` trait 的实现，Engine 每次上屏 `Usage::of_text` + 按来源定词数，
  整句按 `segment_text` 切词数；与输入日志无关，偏好设置「统计」页显示，`book_scale` 折成几本《某书》）。
- `VocabularyBook`：词汇记录（`user-vocab.tsv`，Core `VocabularyTracker` trait 的实现：学习语言的每条译词看到过几轮 / 上屏过 / ⌥+数字 打出过几次；Core 私密输入统一跳过曝光和提交写入，但仍可读取已有记录用于排序和生词标记；
  Engine `annotate` 据此填 `Sense::fresh`，看到轮次不到 `FRESH_UNTIL` = 3 的译词壳里画橙色；「看到」按上屏那一刻屏幕上那一页算，壳每次画完 `Engine::note_displayed` 告知当前页）。
- 各表落盘走 Core `storage::write_atomic`（临时文件 + fsync + 改名），加载按行容错（坏行警告跳过，真读不了壳退回内存学习），
  壳激活期间每 60 秒 `Engine::flush_learning`；IMK 回调边界 `imk::catch_panic` 拦 panic、缓冲区字母原样上屏（见 architecture.md「崩溃不丢」）。

## crates/qingjian-format

`.qj` 数据容器（`Container` mmap 读、`Writer` 写、`Table<T>` / `Text` 零拷贝视图、`hash` 可落盘哈希索引、`Metadata` 名称 / 许可证 / 署名）。
词库与语言模型都能 `write_qj` / 从 `.qj` 打开，启动 50 ms；`cargo run --release -p qingjian-dict-convert -- pack dict|lm --name … --license …`
生成 `data/generated/{dict,lm}.qj`，`bundle.sh` 在 TSV 更新时自动重打并只把 `.qj` 打进包。设计见 `docs/design/architecture.md`「数据文件：`.qj` 容器」。

## crates/qingjian-neural

`CharScorer`，Core `sentence::SentenceScorer` trait 的实现：candle 加载字级 Transformer（GPT-2 风格 decoder，训练仓库（本地 `../train`，私有，不在本仓库）导出的
`model.safetensors` + `config.json` + `vocab.json`），给「前文 + 整句」按字累加 log 概率；前文的每层 K / V 缓存（`PrefixCache`），
同一段前文只算一次，每个候选只算自己那几个字（64 字前文 × 8 条 28 ms，Metal）。features `accelerate` / `metal` 换后端，壳用 `metal`。

Engine 侧在 `engine/rescoring/`：接了打分器就取 Viterbi 前 `RESCORE_PATHS` = 6 条路径按 `路径分 + λ·(神经分 − 静态二元分)` 重排（λ `NEURAL_WEIGHT` 0.5，
个人 n-gram / 用户加分 / 代价不动），分走「前文 + 文本 → 神经分」缓存 `NeuralCache`；同步打分器（`with_sentence_scorer`，CLI 评测）当场补分，
异步的（`with_async_sentence_scorer`，后台线程 `RescoreWorker`）查询不等模型：缺分的记下来，壳停键后 `request_rescoring`、`poll_rescoring` 到了再 `query` 一次。
前文优先用壳给的应用光标前文（`set_rescoring_context`），没有用本会话最近 64 个上屏字符。CLI `--neural <导出目录>`（`--neural-weight` / `--neural-context` / `--neural-async`）。

## crates/qingjian-lm

`BigramModel`，Core `sentence::LanguageModel` trait 的实现，从 `data/generated/lm.qj`（或 `lm-unigram.tsv` / `lm-bigram.tsv`）加载
（没有这两个文件就退化为一元词频整句）。数据由 `tools/corpus/parquet_to_text.py`（uv 脚本，HF parquet → 简体纯文本）加
`cargo run --release -p qingjian-dict-convert -- bigram --phrases assets/lexicon/phrases.tsv --phrases assets/lexicon/domain_words.tsv --brand assets/lexicon/brand.tsv --brand assets/lexicon/mixed_words.tsv data/corpus/*.txt` 生成；语料在 `data/corpus/`（gitignore）。
短语层不当 token 统计（分词时摘掉、统计完按成分合成一元 / 二元，短语得分等于原来两个词的路径，见 `bigram.rs` 模块注释），品牌词按给定次数写进一元与句首二元。

## crates/qingjian-platform

`Config`（TOML 配置文件，`[general]` / `[shortcut]` / `[dictionaries]` 分节，首次运行写模板，
`set_value` 用 toml_edit 原地改键保留注释；`[model] enabled` 本地整句模型开关，`LocalModelConfig`）；`extra_dictionaries` 列出 / 加载随包领域词库与用户 `dicts/`
（Windows Server 与设置程序共用，同名 `.qj` 优先于 `.tsv`）；`protocol` 模块是 Windows Server ↔ TSF DLL 的 IPC 协议类型
（`ClientMessage` / `ServerMessage` / `Frame` / `PreeditSegment`，全 serde，两端共用，见 `docs/design/architecture.md`「Windows：TSF」）。

协议的跨版本兼容是硬要求：升级安装时 DLL 换不掉已经开着的应用（按版本并排装），那些进程里的旧 DLL 要继续跟新 Server 说得上话。
**改了 `protocol/` 里任何类型就把 `PROTOCOL_VERSION` +1**，`protocol/tests.rs` 里的样例 JSON 会盯着这件事。兼容靠三层：
结构体整个 `#[serde(default)]`（`Frame` / `PreeditSegment` / `KeyEvent` / `ScreenRect` / `KeyModifiers`，还有协议直接传的 Core 类型 `Candidate` / `CandidateList`），
所以加字段、删字段、改名都不炸对面；枚举认不出的名字退到安全的一档（`PreeditKind` 退 `Typed`、`KeyOutcome` 退 `Passthrough`、`CandidateKind` 退 `Chinese`）；
消息变体是最弱的一环——只管收的那端用 `read_incoming` 得到 `Incoming::Unknown` 可以跳过，但一问一答的那端等不到应答仍会失败，
所以新消息要么等旧 DLL 淘汰、要么由 Server 按会话版本降级发（`dispatch::composed` 对 `PreeditKind::Fuma` 就是这么做的）。
0.1.6 删 `Frame::layout` 时字段还是必填的，所有没重启的应用每键都失败、只能重启系统，前两层就是为这个加的。
那批 DLL 已经发出去了，只能反过来迁就：`Frame::legacy_layout` 是个只写不读的坟墓字段，恒发 `"layout":"horizontal"`，
让 0.1.6 之前的 DLL 还能解析新 Server 的帧；等它们淘汰干净（再发一两个版本）就删掉，删时 `PROTOCOL_VERSION` 照例 +1。

## crates/qingjian-render

自绘渲染器：候选窗一帧 + 主题 → 预乘 RGBA 位图，tiny-skia 栅格 + cosmic-text 文字（fontdb 按平台清单只加载几个字体文件、不扫系统），
自己解析 `trak` 字距表、按主题 gamma 加深笔画；cosmic-text 打了 `opsz` 光学字号补丁（qingjian-team/cosmic-text 分支 `qingjian-opsz`，workspace `[patch.crates-io]` 钉 rev）。
`examples/preview.rs` 出 PNG 与真机截图并排比、`--measure` 量宽度。Windows 壳 `server/src/ui/painter/` 贴位图；
`[general] font` 是候选窗字族名（空为系统字体，壳按字族名找出字体文件交给渲染器只加载那几个，没装就回系统字体）。
主题颜色按角色拆开：品牌强调 / 输入光标 / 云服务 / 生词 / 纠错各有独立字段；`Palette` 提供奶油、字在蓝、紫藤拿铁、森林四套浅 / 深色，值见 `docs/design/brand.md`。
设计与验收见 `docs/design/rendering.md`。

## crates/qingjian-voice

Windows 本地语音输入的可复用层：`Controller` 在后台线程加载 sherpa-onnx SenseVoice，按命令打开 / 关闭默认或指定麦克风，
把交错 PCM 混成单声道并重采样到 16 kHz，最终只返回非空识别文本。采音按设备自己的混音格式建流（f32 / i16 / u16 / i32
都收，回调里转 f32），共享模式下写死 f32 会让整数格式的麦克风直接打不开。`WorkerRequest` / `WorkerResponse` 使用平台层的
长度前缀 JSON 帧在 stdio 上传输；不含全局键盘钩子、剪贴板或模拟粘贴。
SenseVoice 固定开启 ITN，但短句常不输出标点。`[voice] punctuation = true`（缺省开）时，识别结果先交给
sherpa-onnx CT-Transformer 离线标点恢复（`asr::Punctuator`）再上屏；Server 按随包根的
`data\voice\punctuation\model.int8.onnx` 自动发现模型（settings-egui「语音输入」页只有开/关），文件不在或模型坏只降级到原文。
大模型整理按 `[voice] polish` 档位选：`off` / `spoken`（只去口水词，删除量红线 15%）/ `written`（转书面，
编辑距离红线 45%）。整理在 `VoiceState::Polishing`（候选窗标题「正在转化」）里整段一次调用，完不成退原文；
Server 把用户自己建立的写法（custom phrases + `user.tsv` 高频 + `user-words.tsv` 用户词）作热词注入
system prompt，让模型按原样保留。请求剥掉思考标签后按档位红线（编辑距离与 ASCII 段一致校验）检查，超时、
增删正文或返回异常时使用标点恢复（或原文）结果。程序不携带、启动或管理大模型。
实现源自 auto-voice（MIT，Copyright (c) 2026 billowsand），完整许可见根目录 `THIRD_PARTY_NOTICES.md`。

## apps/cli

测试工具，`cargo run -p qingjian-cli -- kaifa`。

- `--typing` 逐键计时（性能测试用 release 构建跑，目标每键 10 ms 以内）。
- `--chinese-first` 打开中文优先（`[general] chinese_first = true` 的排法），配合 `--replay` 比两种英文词位置。
- `--replay <input-log.jsonl>` 回放评测：把日志里每次上屏的键重新喂给引擎，按来源算首选 / 前五命中率、平均名次、不在候选的条数，打印没命中的例子（`--misses N`）；
  只在内存里学习不写文件，加 `--user-dict` 可带上现有学习数据。
- `--tune 名=值`（逗号分隔）覆盖个人 n-gram 插值与敲错代价的常数扫网格（名字见 `apps/cli/src/tuning.rs`，Core 侧是 `Engine::set_interpolation` / `set_typo_costs`，壳只用缺省值）。
- `--eval-text <文本>...` 整句评测：把用户自己写的中文文本按标点切句、按词库读音转成全拼，冷启动喂给引擎看整句能不能还原原句
  （首选命中率 / 字准确率 / 查询耗时；不依赖日志里当时选了什么，给整句排序与语言模型的改动当尺子），`--eval-save` 冻结成 `句子\t拼音\t上文` 三列文件，
  之后直接 `--eval-text` 它保证比的是同一份句子（本机的在 `data/eval/sentences.tsv`）。排序、整句、纠错的改动先跑它们再合。

## apps/windows

Windows 产品由 `server`（IPC 分派 + Engine + 自绘候选窗与悬浮状态条）、`voice-worker`（独立语音采集 / 识别进程）与
`tsf`（TSF 文本服务 DLL，lib 名固定 `qingjian_tsf`）组成，
外加 `settings-egui`（设置程序，打包缺省用它）、`settings`（WinUI 3 的那份，`build.ps1 -WinUiSettings` 才打，见 `notes/egui-settings-spike.md`）
与 `installer`（Inno Setup）。不合成一个 crate，因为 DLL 不能带 Engine 的依赖树，见 `apps/windows/README.md`；
协议类型在 `qingjian-platform::protocol`，设计见 `docs/design/architecture.md`「Windows：TSF」。

语音输入由 Server 启动同目录的 `qingjian-voice-worker.exe`，通过私有 stdio 帧协议控制。管道读写全在
`voice/process` 的 `qingjian-voice-ipc` 线程上，Router 的工人循环只投命令、读共享快照（有活儿 40 ms 刷一次、
空闲 500 ms），**不要**把任何等 Worker 回话的调用搬回工人循环——它是单线程的，卡住就是全系统吞键。
Worker 崩溃或管道坏掉写成带原因的 `Failed` 快照，下一次 Start 才重新拉进程；停用时请它自退、300 ms 后强杀。
`VoiceCoordinator` 对每个会停住的阶段都有墙钟保险（识别 45 s / 润色 20 s / 等 ACK 10 s）。TSF 拦截 `[shortcut] voice`
配置的语音开关键（按住期间按过别的键就只当组合键，松手不开录音），经既有 `SyncMode` 轮询获得 `VoiceSync`；
最终文本由异步 `RequestEditSession` 直接写入开始录音时的文档，成功后发 `VoiceAck`。
Server 在 ACK 前重复交付，TSF 记录“已排队 / 已写入未确认”请求号，因此重连不会重复插字。
普通键入、失焦、切走输入法或关闭会话会取消当前听写；密码框的键盘禁用 compartment 直接放行语音键。
配置 `[voice]` 缺省关闭，模型路径相对随包根；`polish_enabled` 单独控制大模型语句整理，缺省关闭；`punctuation` 与 `hr`
是随包资源（`data\voice\punctuation` / `data\voice\hr`）的加载开关，缺省开、资源不在自动降级；配置变化会停掉当前请求并重启 Worker。

用户可见品牌是「字在」，内部 crate、可执行文件、`Qingjian` 数据与安装目录、`.qj` 格式名暂不迁移。图标矢量源在 `assets/icon/logo.svg`，
`assets/icon/generate.py` 生成主 PNG 与 TSF / 设置 / Server / 安装器共用的多尺寸 `qingjian.ico`。
设置程序把 `assets/icon/logo.png` 用 `include_bytes!` 编进 exe（`nav.rs`）作为几何遮罩，导航品牌条、无边框标题栏与「关于」页按当前色系和系统明暗实时换色，不依赖随包文件；
「候选窗口」页用 egui painter 和渲染器 `Palette` 实时画四套浅 / 深主题卡，示例文字走当前候选字体，不嵌静态预览图。状态条的 `StatusCell::Logo` 按 `assets/readme/brand-system.png` 画主题色品牌 Logo 并承担拖拽；`StatusCell::Mode` 不再有独立色块，「中 / A」与双拼单字继续跟随候选字体，双拼方案收成 `鹤 / 自 / 微 / 搜`。最后一格使用当前主题次级文字色的矢量齿轮打开设置。
正文与候选窗共用 `[general] font`；Segoe Fluent Icons 单独登记为 `zizai-icons` 字族，导航、设置行、提示记号都显式走它，不能进用户字体回退链。分节页、卡片与排版取值见 `docs/design/candidate-ui.md`「设置程序」。

TSF 原有数字 / OEM 标点 / 空格键码按当前布局用 `ToUnicodeEx` 解析（bit 2 避免改变键盘状态），
仅接受单个非代理项 UTF-16 单元。字母、小键盘和 AltGr 处理不变，不保证组合音符输入。

TSF 正常收键不写逐键文件日志，也不记录字符、preedit 或上屏正文；同步路径耗时只在进程内用原子计数按
`<1 / 2 / 4 / 8 / 16 / 32 / >=32 ms` 分桶，停用文本服务时向当天日志写一条汇总。连接、协议、编辑会话等低频异常仍即时记日志。

Server 没起来时 DLL 自己拉（`client/launch.rs`）：管道不在就 `ShellExecuteW` 起同目录的 `qingjian-server.exe`
（uiAccess 的 exe 只能经外壳拉起，`CreateProcess` 报 740），`Local\Qingjian.ServerLaunch` 互斥体跨进程去重，
每进程 10 秒最多拉一次。拉起后由 `service/connection.rs` 每 40 ms 重试连接、最多 400 ms
（这段在应用的 UI 线程上，久了 TSF 看门狗会切走输入法），没等到也不慌：刚拉起过的 10 秒内重连退避缩到 300 ms。
**别用 `WaitNamedPipeW` 等**——它只等「管道在、实例都忙」，管道还没建出来时立刻就失败，
2026-09-18 真机踩过：拉起了却要三秒才连上（Server 从 `ShellExecute` 返回到管道就绪只要 120 ms）。
开机时「启动」文件夹要等 Explorer 放行（实测登录到 Server 就绪隔了一分钟），升级安装、用户结束进程后也各有空窗，
靠这个把「切过去打不出字」的窗口从分钟级压到一次按键。**连不上 Server 的键一律放行**（从前是吃掉），没有 Server 也变不出中文，
不如让字母直接进应用当英文打。协议对不上（本进程还加载着升级前的旧 DLL）时合上 `client/mismatch.rs` 的进程级闸：
之后整键放行、不再连 Server、日志只记一次，重启这个应用即恢复。

Server 的单实例与接管（`instance/`，名字在 `qingjian_platform::instance`，安装脚本手抄了其中两串）：

- **管道按会话分**：`\\.\pipe\qingjian.<会话号>`。管道名是整机共用的，不带会话号时快速切换用户 / 远程桌面下
  第二个用户的 DLL 会连到第一个用户的 Server（按键进了别人的进程、候选窗画在别人桌面上）。DLL 连上后还用
  `GetNamedPipeServerSessionId` 核对对端在本会话。Server 另外尽力在不带会话号的旧名字上也听一份，给升级后
  没重启的应用里的旧 DLL（`ipc::pipe::listen_legacy`），旧 DLL 淘汰干净后删。
- **单实例互斥体** `Local\Qingjian.Server.<后缀>`：Server 一起来（**装配 Engine、读学习数据之前**）就抢，主线程持有到
  进程退出，进程没了系统遗弃它。已被占时看现任挂的**构建标记**（`Local\Qingjian.ServerBuild.<后缀>.<构建号>`，
  构建号 = 版本 + exe 路径 + 大小 + 修改时间）与**降级标记**（现任被单独提权、或该有 uiAccess 却没拿到）：
  同一份程序且现任不比自己差 → 本进程 `exit(0)`，不打扰现任（开机时 DLL 先拉起一个、一分钟后启动项再拉一个就是这样）；
  否则（升级后的新构建、现任降级、`--replace`）往**让位事件** `Local\Qingjian.ServerStepDown.<后缀>` `SetEvent`，
  等互斥体被遗弃（最多 5 秒）再装配。现任收到后工人循环收尾：学习数据落盘、`mem::forget` 掉 Router
  （语音 Worker 的 join 可能拖好几秒）直接退出。
- 接管在装配之前做完是为了学习数据：学习表是整表覆盖写的，后来者若先读了旧快照、现任再落盘，后来者下一次落盘
  就会把现任最后那段学到的东西盖掉。
- 让位事件只由现任建（后来者只开不建），接管后先 `ResetEvent`：事件被别的句柄撑着还留着上一轮信号时，
  新现任一开始等就会「收到」让位请求、刚接管就退出。互斥体 / 事件 / 标记的 DACL 只放行本用户与 SYSTEM
  （完整性标 Medium，提权起的现任也能被普通后来者叫停）；**不**像管道那样放行 Everyone / AppContainer，
  否则任何沙箱应用都能随时让输入法退出、再趁空档抢注管道名。
- DLL 看到本会话的单实例互斥体在就不拉 Server（它正在起来或正在接管，管道一会儿就有），宿主是被单独提权的进程、
  服务账号或服务会话时也不拉（`client/host.rs`）：拉起的 Server 会继承宿主的令牌与环境。Server 自己也查：
  服务会话 / 服务账号里直接退，环境缺 `APPDATA` / `LOCALAPPDATA` 时按已知文件夹补上（`known_folders.rs`）。
- 安装器：`PrepareToInstall` 与卸载开始时先持有 DLL 的拉起互斥体 `Local\Qingjian.ServerLaunch` 到安装程序退出
  （旧 DLL 也认它，文件替换到一半时不会被拉起旧 exe），再发让位事件等 Server 落盘退出，最后 `taskkill` 兜底。
  安装器等到遗弃的互斥体后要立刻 `ReleaseMutex`，否则装完起的 Server 会以为现任还在。

配套的自我保护（`main.rs::serve`）：**UI 起不来就 `exit`**；UI 线程中途死掉（`UiHandle::is_alive` 转假）也收尾退出。
否则会变成「能打字、没窗口」的 Server，后来的同版本 Server 以为它好好的、自己退出，用户永远等不到候选窗与状态条。
2026-09-23 真机踩过：一次剥了环境（连 `LOCALAPPDATA` 都没有）的上下文拉起的 Server 建不出窗口又占着管道，卡了二十分钟。
还没覆盖的：窗口建出来了但画不出 / 看不见（`alive` 仍为真、管道也在，DLL 不会补拉），只能 `qingjian-server --replace` 或重新登录。

## assets

- `assets/sample/`：手写样例词库与释义表，不是产品数据。
- `assets/emoji/emoji-zh.tsv` / `emoji-en.tsv`：Unicode CLDR 中文 / 英文 annotations 转出的 emoji 表（Unicode License v3，可发布；中文词与英文词各配 emoji，两张表加载时合成一张），
  `cargo run --release -p qingjian-dict-convert -- --out-dir assets/emoji emoji --language zh data/cldr/annotations-zh.json data/cldr/annotationsDerived-zh.json`（en 同理）。
- 英文词表词频：`uv run tools/corpus/english_frequency.py data/generated/english.tsv -o data/generated/english-frequency.tsv`，再 `... english <词表> --frequency <那个文件>`。

## tools/gloss-gen

用 LLM 批量生成释义表：`cargo run --release -p qingjian-gloss-gen -- generate`（密钥读 `QINGJIAN_API_KEY`，结果 JSONL 在 `data/generated/`，不进 git、可续跑，`--limit 80` 试跑）
再 `... export`（写 `glossary-{en,ja}.tsv`，产品数据在 `assets/glossary/`，见那里的 README；格式 `词\t词性. 译词[|假名]`）。CLI 与 bundle.sh 用的就是这两个文件。

## tools/dict-convert

产品数据的生成工具，输出到 `data/generated/`（gitignore）。

- `lexicon`：从 `assets/lexicon/`（自建词库源：规范字 + 常用词 + THUOCL 领域词）加 Unihan 读音（`data/unihan/Unihan_Readings.txt`）、LLM 多音字标注（`gloss-gen pinyin`，
  结果 `data/generated/pinyin-llm.jsonl`，不进 git）、语料词频（`lm-unigram.tsv`）建基础词库 `dict.tsv`（8.7 万条），并把 THUOCL 领域词按语料次数 < 50 拆成
  `dicts/<领域>.tsv` + `.qj`（11 本、13 万条，`--domain-keep-min`），流程见 `assets/lexicon/QINGJIAN.md`；`--extra-words` 并入人工挑的领域词 `assets/lexicon/domain_words.tsv`。
- `english`：转 `assets/lexicon/05_english/00_all_words.tsv`；`cedict`：释义表备用来源。
- `bigram`：统计语料；`--phrases` 给短语层、`--brand` 给品牌词（`assets/lexicon/brand.tsv`，青简 210）与中英混杂词（`mixed_words.tsv`，C盘 / B站：合成计数要成分词在语料里，C 不是 token，只能直接给一元，次数对着同音竞争词定），领域词也走合成计数（语料里只有几十次的词当 token 统计会吸走成分词的二元证据）。
- `mine`：从语料挖词库没收的高频词并过滤（`oov_filter.rs`：虚词规则 + 相邻字对 PMI≥3，`--candidates` 只重过滤）。
- `phrases`：挖短语层（两遍扫语料：相邻两词、两段二元都够频的相邻三词，总次数与对话语料次数都 ≥ 2000 + 边界规则，读音由成分词拼出；我的 / 不知道 / 有没有 这类常用词表不收的组合，
  `assets/lexicon/phrases.tsv`；词库已并入过短语时重跑加 `--refresh`）。
- `pack dict|lm|glossary`：打 `.qj`（释义表也进容器）。
