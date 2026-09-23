# 架构

## 总体结构

```text
                    Qingjian Core
                         │
                         ▼
                  Windows Adapter
                        TSF
                         │
                         ▼
                   Candidate UI
```

词库、拼音解析、候选生成、排序、用户词频学习全部属于 Core。
平台层只做两件事：把系统输入事件翻译成 Core 的输入，把 Core 返回的候选画到候选窗口。

判断标准：把 TSF 壳换成别的壳，不应该需要改 Core 的任何一行。

## 架构约束

这些是核心设计决定，不要违反。

1. **Core 平台无关。** `qingjian-core` 及其兄弟 crate 不允许依赖任何平台 API。
   平台层里不允许出现排序逻辑、词库访问或文本变换。
2. **输入优先。** 任何为附加功能增加的延迟、弹窗、UI 干扰都是设计错误。

## Workspace 结构

```text
qingjian/
├── crates/
│   ├── qingjian-core/          # composition / parser / correction / candidate / ranking / sentence / engine …（下面单列）
│   ├── qingjian-dictionary/    # 词库加载与查询
│   ├── qingjian-translate/     # 候选翻译 annotation
│   ├── qingjian-learning/      # 用户词频、用户词、个人英文词、个人 n-gram、个人敲错表（user.tsv / user-words.tsv / user-english.tsv / user-ngram.tsv / user-typos.tsv）、输入日志（input-log.jsonl）、输入统计（usage.tsv）、词汇记录（user-vocab.tsv）
│   ├── qingjian-lm/            # 整句转换的 bigram 语言模型：LanguageModel 的实现
│   ├── qingjian-neural/        # 字级 Transformer 的本地推理（candle）：SentenceScorer 的实现，给整句前几条路径重打分
│   ├── qingjian-format/        # .qj 数据容器：mmap 打开、零拷贝视图、写入器、可落盘的哈希索引（dictionary / lm 依赖它）
│   └── qingjian-platform/      # 平台层共用的部分：配置文件、协议类型
│
├── apps/
│   ├── cli/                    # 测试工具：查询、逐键计时、输入日志回放评测、整句评测
│   ├── windows/                # Server 进程（IPC 分派 + Engine + 命名管道）
│   └── windows-tsf/            # TSF 文本服务 DLL（cdylib）：COM 链路 + 连 Server 的管道客户端
│
├── tools/
│   ├── dict-convert/           # 产品数据生成：lexicon / bigram / mine / english / emoji / pack
│   ├── gloss-gen/              # LLM 批量生成释义表与多音字标注
│   └── corpus/                 # 语料预处理脚本（uv）
│
├── assets/                     # 随仓库的产品数据源：词库源、释义表、emoji 表、词汇等级表（levels/，CEFR-J / Octanove / JLPT）、图标、样例
├── data/                       # gitignore：语料、Unihan、生成物 data/generated/
├── docs/
└── README.md
```

`qingjian-core` 内部模块：

```text
qingjian-core
├── composition     # 输入状态机：拼音缓冲、光标、上屏
├── parser          # 拼音切分（全拼 / 简拼 / 双拼）
├── candidate       # 候选数据模型（Candidate / Translation / Sense / PartOfSpeech / Language）；layout 是分页排布，各平台壳共用
├── ranking         # 候选排序
├── shortcut        # 快捷候选：日期 / 时间 / 星期，不查词库
├── english         # 英文模式候选：词表精确词 / 前缀补全 / 一处编辑纠正（edit.rs），大小写跟着敲的走
├── sentence        # 离线整句转换：词图 + bigram Viterbi + 束搜索，LanguageModel trait（qingjian-lm 实现，缺省退化为一元），UserNgram 个人 n-gram（二元 + 三元），Context 上文（前两个词），
│               #   SentenceScorer trait（qingjian-neural 实现）：convert_paths 出前 K 条路径，Engine（engine/rescoring）按 路径分 + λ·(神经分 − 静态分) 重排，异步时后台线程打分、壳停顿后取
├── emoji           # emoji 候选：EmojiTable（词 → emoji，Unicode CLDR 中文 annotations）
├── shuangpin       # 双拼：Scheme 四套方案的键位表，decode 把敲的键解成全拼（音节间带 '），Decoded 把上屏消耗换算回键数；切分之后全部复用全拼
├── engine          # 对外门面：Engine，以及 Translator / Learner trait 与空实现
└── storage         # 小文件落盘原语：write_atomic（临时文件 + fsync + 改名）、read_text_lossy；学习 crate 与配置都用它
```

词库内存布局（`qingjian-dictionary`）：词文本与拼音键各放一个连续 arena，词目只存 `u32` 偏移 + 词频，
键按字节序排好（当年 89 万条的测试词库约 150 MB RSS，比 `String` + `Vec<String>` 的朴素布局省三分之二；现在产品词库 8.7 万条，
且 `.qj` 是 mmap 直接映射，见「数据文件」）。
查询接口是 `lookup_pattern(&[SyllablePattern])`（命中音节数 ≥ 模式长度）与 `lookup_exact`（正好等长），每个位置可以是
完整音节或前缀 / 声母。实现是逐级前缀收窄：「以某段前缀开头的键」在排好序的索引里总是连续区间，完整音节直接二分到
`前缀 + 音节 + 空格`，简拼位置按区间里实际出现的音节跳块（每块看第一条键就能二分出块尾），区间小于 48 条就改线性比对；
代价与匹配到的音节组合数成正比，与首音节下有多少键无关。每个位置可以给多种写法（`lookup_pattern_alt` / `lookup_exact_alt`，
敲错变体用），同一位置的写法在每级逐个走、代价相加不相乘；调用方保证同一位置的写法互不覆盖。Engine 侧同一次查询里相同的前缀模式只查一遍，
排序键预计算、远超 500 条时先 `select_nth` 再排，候选最多给壳 500 条。

拼音切分（`parser`）：按位置做动态规划，每个位置只保留最优 8 种前缀切分，token 可以是完整音节、
声母（简拼）或末尾未打完的前缀。排序键：音节少 > 不完整音节少 > 前面的音节长。

中英混输（`Engine::insert_english`）：整段输入（不含 `'`）在英文词表里就加一个 `CandidateKind::English` 候选，
上屏吃掉整段输入。排第一的条件：切不动（有未切分尾部），或最优切分除末尾外还有不完整音节
（`hello` → `he l l o`）；否则排第二（`china` 是干净的 `chi na`）。词表 `WordList` 在 dictionary crate。

代码组织约定（2026-09-03 起）：一个 struct / enum / trait 及其 impl 单独一个文件，
模块文件只做 `mod` 声明、re-export 和自由函数；结构体字段逐条 `///` 注释并用空行分隔；
`thiserror` 的 `#[error]` 文案用英文，日志与 UI 文案用中文。

`Engine` 的会话 API：`set_input / push / backspace` 喂拼音，`query()` 返回不带译文的
`Query { segmentations, candidates, timings }`，`annotate(&mut CandidateList)` 补译文，
`commit(&Candidate)` 上屏并喂给 Learner（词频、词转移、自动造词）。`Learner::flush()` 由壳在退出 / 停用时调用，
失败只记日志不返回错误；壳停用时还调 `break_chain()`，之后上屏的词按句首记；
`Engine::flush_learning()` 把学习数据与输入日志一起落盘且不作废格子缓存，壳激活期间也定时调它。

### 崩溃不丢：原子写、损坏容忍、panic 隔离

输入法进程随时会被 launchd 杀掉或自己崩掉，用户攒的学习数据和正在打的字都不能因此没了：

- **原子写**（`qingjian_core::storage::write_atomic`）：学习 crate 的六张 TSV、输入统计 `usage.tsv`、词汇记录 `user-vocab.tsv`、`config.toml`、`.env` 都先写同目录的临时文件，
  flush + fsync 后改名覆盖；任何时刻磁盘上要么是旧文件要么是新文件。输入日志 `input-log.jsonl` 是追加写不走这条路，
  崩溃最多留半行，回放工具按行跳过坏行并计数。
- **损坏容忍**：学习数据各文件按行解析，格式不对的行记一条警告跳过（下次落盘就清掉了），编码坏掉的字节按替换字符读进来；
  只有权限、坏盘这类真正的 io 错误才算读失败，这时壳退回只在内存里学习（不带路径，不会拿空表覆盖用户的文件），输入法照常启动。
- **panic 隔离**：TSF DLL 被加载进应用进程，Server 与设置程序也各有自己的进程。DLL 的 TSF 回调与 Server 的 UI / 分派边界都用 `catch_unwind` 拦住，
  拦下后把缓冲区里的字母原样交给应用、清引擎状态、收窗口，按键交还给应用；
  `main.rs` 装的 panic hook 只记位置与 backtrace 进日志。
- **回调重入**：Server 的会话状态按 `SessionId` 分开，UI 线程与工人线程之间只过消息，不共享可变状态；
  DLL 侧的回调在 TSF 主线程上跑，凡是要等应用回话的调用都不在借用期间做。
- **有界丢失**：学习数据除了停用时保存，激活期间借每秒看配置文件的定时器每 60 秒 flush 一次（没有新数据时是空操作），
  被杀最多丢一分钟的学习。

实际结构会随开发调整，调整后同步更新这里。

## crate 依赖方向

```text
qingjian-dictionary        （纯数据加载与查询，不依赖任何兄弟 crate）
        ▲
qingjian-core              （定义 Translator / Learner trait，依赖 dictionary）
        ▲           ▲            ▲
qingjian-translate  qingjian-learning  qingjian-lm  qingjian-neural   （实现 core 的 trait，依赖 core；learning 另依赖 translate 的 LevelTable 做词汇按级汇总）
        ▲           ▲            ▲
qingjian-platform          （配置文件 Config：general / shortcut 分节，toml_edit 原地改键保留注释；协议类型，可序列化；依赖 core）
        ▲
apps/*                     （组装：Engine::new(dict).with_translator(..).with_learner(..)）
```

`apps/cli` 是 Phase 1 的测试壳：`cargo run -p qingjian-cli -- kaifa` 直接查询，
不带参数进入交互模式（拼音查询、序号上屏、`:q` 退出），`--user-dict` 指定用户词频文件，
`--language en|ja|es` 或环境变量 `QINGJIAN_LEARNING_LANGUAGE` 选学习语言。
`--typing` 是性能测试模式：把输入当一键一键敲进去，每个前缀查一次并标注译文，一行一键打印各阶段耗时
（这是输入法每键的真实工作量，联想在后台线程不算），启动日志里带各数据文件的加载耗时。
性能改动要用 release 构建跑它看数字，目标每键 10 ms 以内。

- Core 只依赖 dictionary，不依赖 translate 和 learning。翻译与学习通过 trait 注入（`Translator` / `Learner` / `InputLogger` / `UsageMeter` / `VocabularyTracker`，
  缺省实现都是空操作），这样 Core 的单元测试和 CLI 工具不需要真实词典也能跑。
- `qingjian-platform` 里的类型必须可序列化（serde）：Core 在独立 Server 进程，同一套协议类型 DLL 与 Server 两边都用。
- `storage` 只放 Core 自己的持久化原语，用户词频的数据模型归 `qingjian-learning`。

## 翻译的异步模型

Core 的会话 API 分两步返回：`update(input) -> CandidateList` 立即返回不带译文的候选；
译文由 translate 在后台查表，通过 `poll_annotations()` 或回调补上。
本地查表通常在一次事件循环内就绪，但接口上必须允许「候选先到、译文后到」，
平台层收到 annotation 更新后只重绘对应行。

## Core 的关键技术决定

### 多词库

Engine 查词的词库是一个列表：主词库（随包 `dict.qj`）、附加词库（`Engine::set_extra_dictionaries`）、用户词（Learner 持有）。
三者一起进词级查询和整句词图；附加词库不带语言模型，它的词在路径上按词频兜底打分（`sentence::fallback_log_prob`），
所以领域词库、导入的第三方词库只影响「有没有这个词」和它的词频，不改变语言模型的尺度。
**领域词拆开**（2026-09-06）：`dict-convert lexicon` 把 THUOCL 领域词按来源文件拆成 11 本 `dicts/<领域>.qj`（法律 / 医学 / 地名 / 成语 /
诗词名句 / IT / 财经 / 饮食 / 动物 / 汽车 / 历史人物，各带 META），只有语料里出现 ≥ 50 次的领域词（`--domain-keep-min`）留在基础词库
（它们其实是通用词：医疗器械、侵权行为）；基础词库从 22 万条降到 8.7 万条、`dict.qj` 10 MB → 3 MB，领域词库合计 13 万条 7 MB。
分词统计语料时仍把 `dicts/*.tsv` 一起当词表，词表与拆分前一致，语言模型不用重跑。
壳负责装配，附加词库有两处：随包的领域词库在 `.app` 的 `Resources/dicts/`，缺省关闭，配置 `[dictionaries] domains` 列出打开的
（缺省只有 `idioms`，偏好设置「词库」页可勾选、不能移除）；用户自己导入的放用户目录 `dicts/`（`%APPDATA%\Qingjian\dicts\`），
目录里的 `.qj` / TSV 文件全部加载，配置 `[dictionaries] disabled` 列出要关掉的文件名；导入 = `qingjian_dictionary::import`
把青简 TSV / Rime `.dict.yaml` / `.qj` 转成 `.qj` 放进去（Rime 的 YAML 头只取 `name:`，权重非整数当 1），
移除 = 文件挪到 `dicts/removed/`，开关 = 改配置，三个动作之后 `Host::reload_dictionaries` 重新装配。这也是第三方词库带着自己许可证单独分发的落点：
`.qj` 的 `META` 里有名称与许可证，偏好设置里直接显示。

### 数据文件：`.qj` 容器

词库、语言模型这类常驻数据用自己的二进制容器 `.qj`（`crates/qingjian-format`），原则是**内存布局就是文件布局**：
从 TSV 解析出来的几段连续数组（词库的词文本 arena、拼音键 arena、键索引、词目；语言模型的词 arena、词表、哈希索引、
CSR 偏移与后继）原样落盘，打开时 mmap 整个文件、校验一遍头与分节边界，不反序列化。启动从 0.9 s 降到 50 ms。

- 文件 = 32 字节头（魔数 `QINGJIAN`、格式版本、数据种类 `Kind`、分节数）+ 分节表（4 字节标签 + 偏移 + 长度，正文 8 字节对齐）
  + 各分节。第一节固定是 `META`：TOML 的 `Metadata`（名称、许可证 SPDX、署名、来源、版本、条数、生成者），
  偏好设置里的词库列表直接显示它，第三方词库各带各的许可证靠的就是这一节。
- 数据种类：词库、语言模型、释义表、emoji 表、英文词表，以及本地整句模型 `Kind::Model`——扩展名换成 `.qjm`，
  三节 `CONF` / `VOCB` / `SAFT` 原样装训练仓库导出的 `config.json` / `vocab.json` / `model.safetensors`（safetensors 是不透明载荷，
  mmap 后切片给 candle，张量搬上设备后容器即丢；`META.entries` 记参数量）。`qingjian-neural::find_model(dir)` 先找 `.qjm`、没有再认三件套目录，
  所以开发直接加载训练直出的目录，随包与用户目录只有一个文件；`dict-convert pack model`（`tools/release/pack-model.sh` 带元数据调它）打包。
- 数值小端、原生对齐，crate 在大端机器上拒绝编译。字符串分节打开时校验一次 UTF-8，之后 `Text::deref` 走 unchecked
  （曾经每次 deref 都重新校验 30 MB，CLI 直接卡死）。定长结构体用 `zerocopy` 派生，`#[repr(C)]` 且手工排字段消灭填充
  （`KeyIndex` 16 字节、`Slot` 12 字节、`WordEntry` 12 字节、`Successor` 8 字节）。
- 两种视图：`Table<T>`（`Owned(Vec<T>)` / `Mapped`，`Deref<Target = [T]>`）与 `Text`（`Owned(String)` / `Mapped`，
  `Deref<Target = str>`）。解析路径与映射路径产出同一种结构，查询代码不区分。
- 文件里的哈希索引（`qingjian_format::hash`）：开放寻址、槽里放条目编号、键留在 arena；哈希函数必须跨进程、跨版本稳定
  （写文件的进程和读文件的进程算出来要一样），用 FNV-1a 64 加 fmix64 终混（FNV 低位对 UTF-8 中文这种字节模式相近的短串分布差，
  只用低位选槽会长链）。**为什么手写而不是用库**：标准库与 foldhash 的哈希器带随机种子，不能用；blake3 / SHA 这类密码学哈希
  一次几百纳秒、且是为抗碰撞设计的，这里每键要算几千次、只要分布均匀；xxh3 / wyhash 这类非密码学库能用，但任何依赖升级
  悄悄改了算法（或换了默认种子）就会让用户机器上所有 `.qj` 失效，而这个函数总共 12 行、有测试钉死输出值，
  自己写风险最小。若以后碰撞或分布出问题，换成 xxh3 并把 `FORMAT_VERSION` 加一。
- 写文件先写同目录 `.qj.tmp` 再改名；数据文件只整体替换，从不就地修改（mmap 的安全前提）。
- 生成：`cargo run --release -p qingjian-dict-convert -- pack dict --name … --license … --source …` → `dict.qj`，
  `pack lm --name … --license …` → `lm.qj`（`bundle.sh` 在 TSV 比 `.qj` 新时自动重打）。`Dictionary::from_path` 按魔数自动选
  `.qj` / TSV 路径，`BigramModel::from_path`（`.qj`）与 `from_paths`（TSV）分开；输入法与 CLI 有 `.qj` 就用它。
  释义表、emoji 表、英文词表还是 TSV（加载各 20 ms 以内，等有需要再进容器）。
- 格式版本不兼容时 `FORMAT_VERSION` 加一，读旧版的代码按需保留；`Kind` 编号只增不改。

### 候选生成需要整句转换

词级候选（trie 查词库）只能做到「能用」。日常可用的门槛是整句转换：
bigram 语言模型 + Viterbi，加上简拼、双拼。没有整句输入，开发者自己都不会切换过来用，
学习功能就没有承载体。

词库与语言模型不自造，见 [landscape.md](landscape.md) 的数据源一节。

### 翻译只是 annotation

翻译 annotation 只对词典词候选有意义。整句引擎产出的候选多数不是词典词
（一整句，或者「的」这种单字），引擎越好，能标注的候选反而越少。

待确认的策略：
- 只给词典词候选标注，句子候选和单字虚词留空。
- 一词多义（开发 → develop / development）只取最高频义项，不展开。

### 翻译数据本地化

翻译走本地查表，运行时不调网络。这样天然满足「翻译不阻塞候选」的约束。
随包的释义表由 `tools/gloss-gen` 用 LLM 离线批量生成（2026-09-04 定的路线，不用有道等网页接口：逆向接口不稳，
攒下来的结果再分发有版权问题；LLM 输出许可干净）：从我们自己统计的一元词频表挑常用词（次数 ≥ 200、不超过 4 个字，约 6.4 万词），
每个词一次请求同时要「词性 + 英文译词 + 日文译词与假名读音」，结果 JSONL 可续跑，`export` 转成 `glossary-en.tsv` / `glossary-ja.tsv`。
释义格式 `词\t词性. 译词\t词性. 译词`，日文译词后 `|假名`；`Sense.reading` 存整词假名，`Sense::furigana()`（Core `candidate::furigana`）用译词里的假名段当锚点把读音对到各段汉字上，
候选窗口按 `開発(かいはつ)する` 显示，假名淡色；纯假名 / 片假名词不注，对不上就整体注在后面。不用罗马音。
CC-CEDICT 表（`dict-convert cedict`）保留为备用来源，覆盖面广但没有词性、释义偏长。
运行时查不到的词不做联网补齐。`Translator::translate` 本身不联网。

### 联想与个人模型的边界

- 整句转换里最贵的一步是词图格子查词（每个简拼位置都要在词库里逐音节块收窄）。敲键是增量的，第 n+1 键只新增以它结尾的
  最多 8 个格子，所以格子候选放在 `sentence::SpanCache`（键是格子模式含敲错写法，值是排好截好的 `SpanWord`）跨按键复用；
  候选与词库、用户词、选择次数、个人出现次数有关，Engine 在 commit / `learner_mut` / 换 Learner 时整个清掉。
  拼写纠错的上千个变体先过无分配的 `parser::is_fully_segmentable`，剩下几个才做真正的切分。
- 整句转换的语言模型通过 `LanguageModel` trait 注入（`sentence/language_model.rs`）：`log_prob(previous, word)`，
  模型不认识的词返回 `None`，Core 用词库词频兜底并扣分。`qingjian-lm::BigramModel` 从 `lm.qj`（或 `lm-unigram.tsv` / `lm-bigram.tsv`）
  加载：词表是 arena + 定长条目 + 文件里的开放寻址哈希索引；二元按前词分组成 CSR（`offsets[v]..offsets[v+1]` 是 v 的后继段，
  段内按后词编号二分，一次查找落在一两个缓存行里），P(w|v) = 0.8·c(v,w)/c(v) + 0.2·c(w)/N。
  数据由 `dict-convert bigram` 统计：用青简词库做一元最大概率分词（与词图同一套词表），连续汉字段为句，`<s>` 句首标记。
  词图每格只留词频前 6 个词（有简拼位置的格子留 20 个：`h` 下几十个常用字，留少了句子里要的那个进不来），每个位置束宽 8。
  简拼位置就是前缀模式（`SyllablePattern.complete = false`），词库层不区分；每条路径覆盖的简拼位置相同，不需要额外罚分。
- **个人 n-gram**（`sentence/user_ngram.rs` 的 `UserNgram`，由 Learner 持有、`Learner::user_ngram()` 暴露）：
  上屏的词序列转移计数，二元 (前词, 后词) 与三元 (前二词, 前词, 后词) 一起记（整句按路径上的词逐条记，连续选词也记；
  标点、透传、回车上屏拼音、切应用打断链，下一个词按句首 `<s>` 记，句首词不记三元）。上文是 `sentence::Context`（前一个词 + 再前一个词），
  Engine 的 `CommitChain` 记最近两个上屏的词；Viterbi 不扩状态，前二词取前驱节点的回指（它那条最优路径上的前一个词），是近似。
  打分时与静态模型（只看前一个词）插值：P = (1−μ)·P_静态 + μ·P_个人，μ = c(v)/(c(v)+8) 封顶 0.5，前词没见过就不插值。
  P_个人 先算二元 P₂ = 0.8·c(v,w)/c(v) + 0.2·c(w)/N；这对上文 (u,v) 见过时再套一层绝对折扣的三元
  P₃ = max(c(u,v,w) − D, 0)/c(u,v) + D·N₁₊(u,v,·)/c(u,v)·P₂（D = 0.75），没见过的接续只拿回退的份额，(u,v) 没见过就是 P₂。
  三元不训练、不平滑参数，就是在线计数；它分辨的是二元混在一起的接续（「我想 → 去」与「不想 → 要」）。
  封顶保证没见过的接续最多打折、不会被压死；K = 8 让一次误选翻不过强 bigram，选两次才翻。
  个人出现次数也参与词图每格的前 6 选择，保证用户常用的同音词进得了格子。二元 + 三元超过 20 万条时所有计数减半。
  持久化在 `qingjian-learning` 的 `user-ngram.tsv`：三列 `前词\t后词\t次数` 是二元，四列 `前二词\t前词\t后词\t次数` 是三元，旧的三列文件照读。
  撤销（退格删光重选）、删词（`forget_word`）、减半都同时覆盖二元与三元。
- **自动造词**：用户自己连着选出的两个词（不是整句路径里的），合起来不超过 4 个字、词库与用户词里都没有，
  且这条转移已记够次数（同一段拼音里连着选的两次，分两段打的三次），就记成用户词并记一次选择。
  同一段拼音里自选一个词之后剩下的部分走整句候选（`jidiaole` 选 挤、剩下 掉了 按空格）时，接缝处第一个词的转移按自选记双份，
  但不参与两词造词（我 + 的… 这种接缝太常见、转移计数早就够了，回放里会把 我的 造成用户词，之后它就得和 沃德 按选择次数比）。
  另外一段拼音分几次选完（`CommitChain` 记着这段里上屏的每个词）时，合起来的文本按「整段字母 → 合成词」记一次选择（`user-choices.tsv`），
  记到两次且词库里没有、不超过 4 个字就造成用户词，下次整段打出来它靠选择次数直接排第一：这是「用户手动拼了一遍整句」最直接的信号，
  比等个人 n-gram 一份一份累到翻过静态模型快得多（「挤掉了」靠 n-gram 要选四次）。
  词级排序也用同一个语言模型：候选得分是 `log P(词 | 上一个上屏的词)`（个人 n-gram 插值，上文是链上的两个词）+ 封顶的选择次数加分，词库词频只做预选和兜底，
  所以 `ba` 在「做了」后面出 吧、句首看模型；同一输入串下选过的词（`user-choices.tsv`，键是候选覆盖的那段字母）排在得分之前，
  `mgs` 选过 美国式 下次就是首选；切分里非末尾简拼少的优先（`kaifa` 按 `kai fa` 读的 开放 压过按 `kai f a` 读的 开放啊）。
  拼写纠错（Core `correction`）两路：整段一处编辑的变体按噪声信道挑（纠正后整句得分扣编辑代价仍高于原样才纠），候选按纠正后的拼音出，
  消耗长度按编辑换算回原串；词图里的敲错边（`correction::typo`，每个完整音节的一处敲错变体当带代价的位置写法，复用 `Expanded`
  多写法机制，`SpanWord::penalty` 进路径得分，`Conversion::penalty` 让 Engine 知道路径不是原样读的）管「音节都合法、整句不通」的输入，
  只进整句词图不进词级候选。接受的纠正按原输入串记选择，回车原样上屏的串记 `<raw>` 以后不纠；两路接受的 (敲的, 要的) 音节对都记进个人敲错表
  （`Learner::record_typo`，`user-typos.tsv`），那条边与整段编辑的代价按次数打折（`TypoCosts::discounted`）。判断带缓存（按作用域），commit / take_raw 复用 query 的结果。
  候选音节对回敲的字母（消耗、记敲错）用 `Engine::align`。
  退格撤销（`LastCommit`，Engine 留最近 4 次上屏 `recent_commits`）：上屏后壳把组句外的退格告诉 Engine（`note_backspace`），退格从最近一次往前数，
  一次上屏的字删光了就候着；接着重打其中一段拼音（或其前缀）选了别的词，就把那次记的选择次数、输入串选择、词转移、整段合成词的选择全部退回（`Learner::unrecord*`）。
  删掉「沃德 书」两个词重打成「我的 书」也认得出（日志里这种错法一天十几次，以前只看最后一次上屏，一次都没撤回，错词越选越靠前）；
  重打后选的还是同一个词只把记录丢掉；重打的拼音谁都对不上就当在改别处，全忘掉；删得比记着的几次加起来还多也全忘掉。
  标点、英文词、原样上屏这些没学习的上屏也留一条只有长度的记录，退格数过它们才能数到更早的词。
  整句候选与某个词候选文本相同时（用户词、或按别的读音对上的词）不重复插，但把那个词提到整句该在的位置：整句转换认定的最好读法不该被词级排序（别的词选过更多次）压在后面。
  用户点选的转移记双份（`EXPLICIT_TRANSITION_WEIGHT`），整句路径里顺带的记一份：整句是模型自己算的，按空格接受会把它喂回模型形成回声，
  用户明确改选一次就要能压过去。选择次数在整句路径上的加分取对数并封顶（`viterbi::WEIGHT_CAP`），只管同音词偏好，不许它抬起拆分路径。
- 个人化优先用在线 n-gram，神经模型只做重排与离线联想，且要过评测门槛（见 roadmap Phase 7）。
- 两者都依赖本地输入历史，历史必须可查看、可清除。

## 平台层的技术决定

### Windows：TSF

- TSF DLL 会被加载进每一个应用进程，核心逻辑必须放在进程外。
  采用 Weasel（WeaselServer）和水杉（Server 进程）相同的结构：DLL 只做 IPC，Rust Core 跑在独立进程里。
- 使用 `windows` crate 的 COM `implement` 宏。
- TSF 是公认最难的输入法 API，工时预期要按整个项目一半来估。

**已落地（骨架）：**

- **IPC 协议**：`qingjian-platform::protocol`，Server ↔ DLL 两端共用、全部 serde。`ClientMessage`（DLL → Server：
  开 / 关会话、按键、上屏、回上下文、回选区、报中英模式）与 `ServerMessage`（Server → DLL：按键结果、上屏结果、异步重绘、请求上下文、请求选区）；
  失焦 / 停用时 DLL 发 `Commit`，Server 回 `Committed { text }`（缓冲区原样交出），
  DLL 用最近收键记下的 `ITfContext` 经编辑会话落进文档；应用强行终止组句（`OnCompositionTerminated`）时拼音已被框架定成普通文本，
  DLL 只记「Server 缓冲过期」，下次说话前先 `Commit` 并丢掉交出的文本，不再插一次。
  中英模式：按本地习惯，单击 Shift 在中 / 英间翻转。
  单击 Shift 的判定在**击键 sink** 里（`com/key/shift.rs`，喂 `OnTestKeyDown` / `OnTestKeyUp`：按下 Shift 到抬起之间没有别的键插进来就是一次单击；
  微软 SampleIME 的 `OnTestKeyDown` 同样处理 VK_SHIFT，sink 收得到独立修饰键）。之前用线程级 `WH_KEYBOARD` 钩子判定，但钩子**看不到被 TSF 吃掉的键**
  （msctf 在队列层把它们改成 WM_NULL），Shift + 数字（删候选 / 第二译词）会被误判成单击而切换模式，2026-09-11 真机确认 sink 收得到 Shift 后钩子已删。
  DLL 记 `english_mode` 持久状态；任务栏的中 / 英指示器靠 `GUID_LBI_INPUTMODE` 语言栏按钮渲染
  （`com/mode/button.rs`，图标现画「中」/「A」，第三方 TIP 单写转换模式 compartment 不出这个指示器），另外顺带写一份转换模式 compartment
  （`GUID_COMPARTMENT_KEYBOARD_INPUTMODE_CONVERSION` 的 `TF_CONVERSIONMODE_NATIVE` 位，`com/mode/mod.rs`）。这条 compartment 还**反向同步**：激活时对它挂
  `ITfCompartmentEventSink`（`com/mode/conversion.rs`），用户点任务栏中 / 英（或别的输入指示器途径）改了转换模式时 `OnChange` 读回 `NATIVE` 位、与当前
  `english_mode` 不同才翻转（相同即我们自己写的那次，忽略以防回环），翻转顺带走 `update_mode_indicator` → 悬浮状态条也一起同步；Caps Lock 只管大小写，
  亮着无论中英模式都直接出大写英文（微软拼音式）。`KeyModifiers` 因此带 `caps`（大小写）与 `english_mode`（持久模式）两个非物理位，
  字母大小写按 `shift XOR caps`。Router（`dispatch/key/input.rs`）里 `english = caps || english_mode`，**英文状态一律纯直通**：
  字母不进缓冲区、由壳直接插进输入框，没有候选窗（0.1.5 删掉了英文候选那条路径与 `[general] english_candidates` / `[apps] english_candidates_off` 两个开关，
  理由见 candidate-ui.md）。「先上屏、再把这个键交给应用」在 Windows 上会乱序
  （放行是同步的、上屏走异步编辑会话），所以组句中的空格 / 标点 / Shift 大写字母改成吃掉，连同上屏文本一起插入。
  应用标识（宿主进程的 exe 文件名，DLL 里 `GetModuleFileNameW(NULL)` 取到就随 `OpenSession { app }` 报一次）仍逐会话记着，交给 Engine 做统计；
  `[shortcut]` 的修饰键 + 数字（译词上屏 / 删候选，`dispatch/key/shortcut.rs`）：配置里的 `Modifiers` 按 option→Alt、control→Ctrl、command→Win
  落到 `KeyModifiers`，Router 按键码认数字、去掉 Caps 位后与配置比；DLL 见 Ctrl / Alt / Win 仍一律放行，只有组句中的修饰键 + 数字送 Server 判，
  没配到的 Router 回 Passthrough。删候选的那句反馈（「已删除…」/「没什么可删」）随下一帧的 `Frame::notice` 下发，自绘候选窗画在拼音行下方、
  显示到下一次按键（`dispatch/key/shortcut.rs` 填、`handle_key` 开头清；画在拼音行右侧）。
  一次要绘制的状态是 `Frame`（preedit 分段 + 候选页 + 删候选提示 `notice`，后者不参与 `Frame::is_empty`），preedit 用 `PreeditSegment`（Core `MarkedSegment` 的可序列化镜像，
  协议不耦合 Core 内部枚举），候选直接嵌 `qingjian_core::CandidateList`。同词干类型收进子目录：`key/{event,outcome}`、`frame/preedit/{kind,segment}`。
- **Server 进程**：`apps/windows/server`（package `qingjian-windows-server`，bin `qingjian-server`）。`dispatch::Router` 按 `SessionId` 分派多会话（Windows 一个 Server 服务多个应用进程，
  每会话各持组句状态）。会话开 / 关、按键与上屏、Engine 装配、命名管道传输（`\\.\pipe\qingjian`）都已跑通，Windows 上端到端测过。
  Server 单实例：管道第一个实例带 `FILE_FLAG_FIRST_PIPE_INSTANCE`；建不出时不是直接退出，而是往会话内接管事件发一次信号、等现任让位后接管
  （现任是老版本就等到超时按老行为退出）。**UI 起不来（或 UI 线程中途死掉）的 Server 直接退出、不占管道**，免得变成「能打字、没窗口」且谁也接管不了的状态。
- **语音 Worker**：`apps/windows/voice-worker`（bin `qingjian-voice-worker`）只在 `[voice] enabled` 时由 Server 启动；
  `crates/qingjian-voice` 抽取 auto-voice 的 SenseVoice、麦克风与 16 kHz 重采样实现（MIT），移除全局键盘钩子、剪贴板、模拟粘贴、托盘与 LLM。
  Server 与 Worker 用长度前缀 JSON 的 stdio 私有协议通信；TSF 的语音键按下 / 松开经 Server 绑定到 `SessionId`，最终文本通过异步编辑会话直写并在成功后 ACK。
  普通键入、失焦、切换输入法与会话关闭都取消未完成请求；密码框不拦语音键。完整状态机与交付语义见 `design/voice-input.md`。
- **候选窗口（Server 进程自绘 + uiAccess）**：候选窗从前在**应用进程内的 DLL** 自绘，普通置顶窗被微软商店 / 任务栏搜索这些**更高 z-band** 的宿主盖住。现改由 **Server 进程**自绘（`server/src/ui/`：一条专用 UI 线程注册窗口类 + 建 GDI 分层窗 + 跑消息循环，HWND 只在该线程碰；工人线程经 `Sender<UiCommand>` + `PostThreadMessageW(WM_APP)` 把「显示(`Frame`+屏幕矩形) / 隐藏」marshal 过去；进程级 `SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2)` 按物理像素对齐应用报来的矩形）。DLL 只量光标屏幕矩形（`GetTextExt`）发 `PositionCandidates{rect}`，并在组句于 DLL 侧结束（应用终止组句 / 断线，`OnCompositionTerminated` 这条 Server 无从知晓）时发 `HideCandidates`；Server 握着 `Frame` 直接自绘，重排的异步更新也直接刷自己的窗、不回传 DLL（渲染代码——词性 + 译文 + 分页 + 柔和阴影，整块从 DLL 搬到 Server）。**盖过高 z-band 宿主**靠 Server exe 的 `uiAccess="true"` manifest（`server/build.rs` 用 embed-manifest 嵌）+ 代码签名 + 装 Program Files 三者齐备（`SetWindowPos(HWND_TOPMOST)` 才自动升进 UIAccess 高带）：开发自签 + 本机受信任根（`installer/sign-local.ps1`），发版换 Certum 开源代码签名证书；uiAccess exe 不能 CreateProcess 拉起（报 740），装完 / 登录都走 ShellExecute（安装器完成页 `ShellExecAsOriginalUser` + `{commonstartup}` 启动快捷方式由 Explorer 拉起才授 uiAccess，故不用计划任务）。候选窗每显示一页，Server 调 `Engine::note_displayed`（收窗传空）告知当前页——生词「看到轮次」据此推进、橙色标记满 `FRESH_UNTIL` 轮才毕业。
  **窗口贴光标上方还是下方**（`server/src/ui/candidates/placement.rs`）：缺省贴下方，放不下贴上方。只看「这一帧放不放得下」会让窗口在同一行里上下乱跳——
  候选条数、释义、页码、提示行都改变窗口高度，光标离屏幕底边不远时高一点放不下、矮一点又放得下。所以记住上次贴的边与当时的光标行（`Placement`），
  新锚点与它在竖直方向有重叠就算同一行、沿用那一边，除非那边真放不下了。锚点本身也得稳：有些应用偶尔量不出矩形（刚起组句还没排版、自绘输入框给全零），
  DLL 从前退到鼠标位置，窗口就跳到指针那儿去，现在改为沿用本段组句上次量到的矩形（`Shared::last_anchor`，组句一结束就忘），一次都没量到过才退鼠标。
- **悬浮状态条（Server 进程自绘，可拖动 / 记位置）**：桌面上常驻的小浮窗，显示当前中 / 英（普通背景上的「中 / A」；双拼在右侧附「鹤 / 自 / 微 / 搜」单字标记），与任务栏的中 / 英指示器（语言栏按钮）并存。跟候选窗**同一条 UI 线程**、复用同一套分层窗口合成器（`server/src/ui/layered/`：圆角背景 + 四周柔和阴影，从候选窗的 `surface.rs` 抽出来两边共用）与主题（字体 / 配色 / DPI / 深浅）；自己一个窗口类与窗口过程（`server/src/ui/status/`）：四格 `[Logo][模式][，。/ ,.][⚙]`，左侧主题色品牌 Logo 取代六点握柄并作为唯一拖拽区域。按下鼠标先 `DragDetect`，挪出阈值就交给系统移动循环（`WM_NCLBUTTONDOWN` + `HTCAPTION`，结束时 `WM_EXITSIZEMOVE` 报新位置），没挪就是点击、按 x 落进哪格；`WM_MOUSEACTIVATE` 回 `MA_NOACTIVATE` 点它不抢应用焦点；窗口过程按 HWND 从 thread_local 表查到对象。点格 / 拖动结束经 `StatusEvent`（`dispatch/status/`）投回工人线程（工人循环收的是 `ipc::Work`：DLL 消息或状态条事件），Router 写回配置（`[status_bar] x/y`、`[general] full_width_punctuation`，热加载再读回）；齿轮由 UI 线程直接起设置程序。中英模式只在 DLL 侧（单击 Shift 翻转），DLL 在切换 / 激活 / 获焦时用 `ClientMessage::ModeChanged { english }` 把当前会话的模式推来（`com/service/mode.rs::refresh_mode_indicator` 的单一咽喉点）；状态条上点模式格时 Server 只能记下目标模式（`pending_mode`）等 DLL 来取：DLL 的轮询定时器在没组句、本线程前台时每几拍发 `SyncMode`，`ModeSync { english: Some(_) }` 就切并回报 `ModeChanged`。**这次点击只交给状态条正显示的那个会话**（`status_session`：最近报过 `ModeChanged` 的那个，有键落到别的会话就转过去）——每个激活了 TSF 的进程都在按同一节拍发 `SyncMode`，而跨进程的 `OnSetFocus(FALSE)` 并不可靠、好几个进程会同时自认在前台，不认会话就成了「谁先问到谁切」，用户所在的应用反而切不动（2026-09-21 真机日志：连点四下分别落在四个不同 pid 上，前台应用与任务栏指示器纹丝不动）。归属换人或归属的会话退出时，没取走的那次点击作废，不隔着应用补切。状态条**常驻桌面**，只跟「当前输入法是不是字在」走：第一次 `ModeChanged` 显示，DLL 挂 `ITfActiveLanguageProfileNotifySink`（`com/profile.rs`）在别的 TIP 被激活时用一条临时连接发 `ImeSwitched` 收起（此时自己已被停用、会话连接已关），应用退出（`CloseSession`）不收。双拼方案 Server 从自己的 `[general] shuangpin` 配置知道，不必带。开关与记住的位置在 `[status_bar]`（`enabled` / `x/y`），热加载即时生效；uiAccess 高 z-band 与候选窗同进程天然继承。参考微软水杉的 FTB 形态（`~/Desktop/MSIME-Windows`，它用 D2D + DirectComposition 且不记位置），落地时选沿用本项目已有的 GDI 分层窗那套以保持视觉语言一致、并加了位置持久化。
- **帧编解码**：长度前缀 JSON 帧的 `read_message` / `write_message` 与缺省管道名放在 `qingjian-platform::protocol`，Server 与 DLL 共用（DLL 不必依赖整个 Server 库）。
- **TSF DLL**：`apps/windows/tsf`（package `qingjian-windows-tsf`，`cdylib`，产物 `qingjian_tsf.dll`，依赖官方 `windows` crate 的 COM `implement` 宏）。「引擎层」不是 Engine 而是连 Server 的**管道客户端** `EngineClient`（平台无关、可端到端测）；
  COM 层：`DllGetClassObject` → `IClassFactory` → `#[implement(ITfTextInputProcessor, ITfKeyEventSink, ITfDisplayAttributeProvider)]` → `Activate` 挂击键 sink + 语言栏中英按钮 + 连管道 → `OnKeyDown` 转发按键、经异步编辑会话（`TF_ES_READWRITE`，不带 SYNC）写组句 / 上屏；`DllRegisterServer` 写 InprocServer32 并经 `ITfInputProcessorProfiles` / `ITfCategoryMgr` 注册文本服务与各能力类别。
  组句拼音的**内联下划线**（组句内联下划线）走 TSF 显示属性协议（`com/display_attribute/`）：注册 `GUID_TFCAT_DISPLAYATTRIBUTEPROVIDER` 类别 + 一个自定义显示属性 GUID（细实线、`TF_ATTR_INPUT`），
  `ITfDisplayAttributeProvider`（实现在 TextService 上）把 GUID 对应的 `TF_DISPLAYATTRIBUTE` 交给系统；收键写组句时用 `ITfCategoryMgr::RegisterGUID` 把 GUID 换成 atom，`SetValue` 进组句范围的 `GUID_PROP_ATTRIBUTE` 属性，宿主据此在拼音底下画线。
  运行异常与生命周期记进 `%LOCALAPPDATA%\Qingjian\logs\tsf.<日期>.log`（与 Server / 设置程序同目录，按天一个文件、留 7 天；多进程追加同一文件）；成功收键不做文件 I/O，也不记录字符 / preedit，热路径只用原子计数按 `<1 / 2 / 4 / 8 / 16 / 32 / >=32 ms` 分桶，TSF 停用时写一条同步路径耗时汇总。候选窗口不再由 DLL 自绘（已搬到 Server 进程，见上「候选窗口」），DLL 侧只做 preedit 内联 + 上报光标矩形。
- **交叉编译验证**：`qingjian-core` / `-dictionary` / `-format` / `-lm` / `-platform` / `apps/windows/{server,tsf}` 已能
  `cargo check --target x86_64-pc-windows-gnu` 通过（借此修掉 `qingjian-format` 里 unix 专有的 `Mmap::advise` 未 `cfg` 的移植 bug）；
  本机只 `check`，真正编译在 Windows 机器上做（`qingjian-neural` 的 candle 后端在 Windows 走 CPU，已接进 Server，见下「本地整句模型」）。
- **本地整句模型（Server 进程）**：`server/src/dispatch/rescore/`。启动时 `find_model`（用户目录 `%APPDATA%\Qingjian\model\` 优先，否则随包 `data\model\`；`.qjm` 单文件或三件套目录）；`[model] enabled` 开着就起线程加载并预热（`ModelLoader`），下一次按键 / tick 接上 `set_async_sentence_scorer`。
  Server 没有定时器：缓冲变化后 `schedule_rescoring` 起防抖，工人循环 `recv_timeout(router.next_tick())` 按 `RescoreState` 的节拍醒来（防抖 80 ms → `request_rescoring`；然后 20 ms 一次 `poll_rescoring`，最多等 2 s），DLL 组句期间每 80 ms 的 `Poll` 也顺带 `tick`。分到了重查一次、重建候选布局、由 Server 自绘的候选窗直接重画，DLL 下一次 `Poll` 拿到新帧更新内联 preedit；翻过页 / 动过高亮不动。热加载 `[model]` 变了才重载 / 卸载。
  前文：DLL 在**起组句的那次读写编辑会话**里顺手读选区起点前 64 个 UTF-16 单元（`com/edit/surrounding.rs::text_before_caret`，拼音还没插进去、不用再开一次会话），随 `ClientMessage::Surrounding` 单向送来。**密码框与私密输入**（2026-09-12 查了微软文档 / SampleIME / Chromium 源码后定）：
  TSF 规定键盘类 TIP 必须看上下文的 `GUID_COMPARTMENT_KEYBOARD_DISABLED`（微软文档明说密码框应禁用文本服务、`IS_PASSWORD` 只是标注不提供保护；Chromium 给密码框的上下文设的就是它），
  DLL 在 `OnTestKeyDown` / `OnKeyDown` / 保留键里没在组句时先查它（连同 `EMPTYCONTEXT`，`com/context.rs`），非零整键放行、不组句——与密码框中不组句同一语义；
  输入范围（`GUID_PROP_INPUTSCOPE`）只在起组句那次编辑会话里读一次（`com/edit/surrounding.rs::input_context`）：含 `IS_PRIVATE` / 密码 / PIN 之一算**私密**——Chromium 源码里密码框与不学习的输入框映射成 `IS_PRIVATE`（含义「别学」；2026-09-12 box 实测 Edge InPrivate 的网页文本框报的仍是 `IS_SEARCH`，`IS_PRIVATE` 只在密码框见过，这条是兜底）——私密时不读前文，并随 `ClientMessage::Privacy` 告诉 Server（客户端只在变了时发；记事本等不支持该属性的应用 `GetValue` 失败按不私密）。
  Server 按会话记 `private`、焦点切换时重设，Core `Engine::set_private`：学习器与输入日志外面各套一层 `Muted*`（写吞掉、读照常，排序不变）。协议版本 4。
- **版本与发布**：见 `docs/notes/release.md`；`apps/windows/server/Cargo.toml` 写死自己的 `version`，
  发布标签用 `windows-v<版本>`（Windows app package 各自 `Cargo.toml` 记版本；不合成一个 crate，因为 DLL 不能带 Engine、音频与模型依赖树）。

