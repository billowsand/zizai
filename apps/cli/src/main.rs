//! 青简 CLI：Phase 1 的测试工具。
//!
//! 输入拼音，打印候选（词性 + 译文）和各阶段耗时；输入序号上屏并记入用户词频。
//! 不依赖任何平台 API，是 Core 的第一个「壳」。

mod args;
mod display;
mod error;
mod eval;
mod logging;
mod repl;
mod replay;
mod rescoring;
mod tuning;

use std::time::Instant;

use clap::Parser;
use qingjian_core::{EmojiTable, Engine, FumaScheme, FumaTable, Language};
use qingjian_dictionary::{Dictionary, WordList};
use qingjian_learning::FrequencyLearner;
use qingjian_lm::BigramModel;
use qingjian_platform::Config;
use qingjian_translate::Glossary;

use crate::args::Args;
use crate::error::CliError;

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), CliError> {
    dotenvy::dotenv().ok();
    let args = Args::parse();
    let _log_guard = logging::init()?;

    let started = Instant::now();
    let mut engine = build_engine(&args)?;
    tracing::info!(total_ms = started.elapsed().as_millis(), "Engine 就绪");
    engine.set_english_mode(args.english_mode);
    engine.set_chinese_first(args.chinese_first);
    tuning::apply(&mut engine, &args.tune)?;
    if let Some(path) = &args.replay {
        let report = replay::run(&mut engine, path, args.misses)?;
        print!("{report}");
        return Ok(());
    }
    if !args.eval_text.is_empty() {
        let report = eval::run(
            &mut engine,
            &args.eval_text,
            args.eval_save.as_deref(),
            args.misses,
        )?;
        print!("{report}");
        return Ok(());
    }
    if args.inputs.is_empty() {
        repl::run(&mut engine, args.limit)?;
    } else {
        for input in &args.inputs {
            println!("> {input}");
            if args.typing {
                display::show_typing(&mut engine, input);
            } else {
                display::show(&mut engine, input, args.limit);
            }
        }
    }
    engine.learner_mut().flush();
    Ok(())
}

/// 组装 Engine：这是 Core 之外唯一知道具体 Translator / Learner 类型的地方。
fn build_engine(args: &Args) -> Result<Engine, CliError> {
    let language: Language = args
        .language
        .parse()
        .map_err(|_| CliError::Language(args.language.clone()))?;
    if language == Language::Chinese {
        return Err(CliError::Language(args.language.clone()));
    }
    let dict_path = args
        .dict
        .clone()
        .unwrap_or_else(|| args::default_data_file("dict.tsv"));
    let glossary_path = args
        .glossary
        .clone()
        .unwrap_or_else(|| args::default_data_file(&format!("glossary-{}.tsv", language.code())));

    let started = Instant::now();
    let dictionary = Dictionary::from_path(&dict_path)?;
    let dict_load = started.elapsed();
    let started = Instant::now();
    let glossary = Glossary::from_path(language, &glossary_path)?;
    let glossary_load = started.elapsed();
    let english_path = args.english.clone().or_else(|| {
        let path = args::default_data_file("english.tsv");
        path.is_file().then_some(path)
    });
    let started = Instant::now();
    let english = english_path.as_ref().map(WordList::from_path).transpose()?;
    let english_load = started.elapsed();
    let learner = match &args.user_dict {
        Some(path) => FrequencyLearner::from_path(path)?,
        None => FrequencyLearner::default(),
    };
    tracing::info!(
        dict = %dict_path.display(),
        entries = dictionary.len(),
        glossary = %glossary_path.display(),
        glosses = glossary.len(),
        english = english.as_ref().map_or(0, WordList::len),
        learned = learner.len(),
        dict_ms = dict_load.as_millis(),
        glossary_ms = glossary_load.as_millis(),
        english_ms = english_load.as_millis(),
        "加载完成"
    );
    let mut engine = Engine::new(dictionary)
        .with_translator(Box::new(glossary))
        .with_learner(Box::new(learner));
    if !args.extra_dict.is_empty() {
        let mut extras = Vec::new();
        for path in &args.extra_dict {
            let dictionary = Dictionary::from_path(path)?;
            tracing::info!(path = %path.display(), entries = dictionary.len(), "附加词库已加载");
            extras.push(dictionary);
        }
        engine.set_extra_dictionaries(extras);
    }
    // 英文候选的中文释义可选
    let zh_glossary = args::default_data_file("glossary-zh.tsv");
    if zh_glossary.is_file() {
        let glossary = Glossary::from_path(Language::Chinese, &zh_glossary)?;
        tracing::info!(glosses = glossary.len(), "英→中释义表已加载");
        engine = engine.with_english_translator(Box::new(glossary));
    }
    if let Some(words) = english {
        engine = engine.with_english(words);
    }
    // emoji 表随仓库提供（Unicode License）：中文表 + 英文表合成一张，一张都没有就不出 emoji 候选
    let mut emoji: Option<EmojiTable> = None;
    let started = Instant::now();
    for name in ["emoji-zh.tsv", "emoji-en.tsv"] {
        let path = std::path::PathBuf::from("assets/emoji").join(name);
        if !path.is_file() {
            continue;
        }
        let table = EmojiTable::from_path(&path)?;
        match &mut emoji {
            Some(all) => all.merge(table),
            None => emoji = Some(table),
        }
    }
    if let Some(table) = emoji {
        tracing::info!(
            words = table.len(),
            load_ms = started.elapsed().as_millis(),
            "emoji 表已加载"
        );
        engine = engine.with_emoji(table);
    }
    // 语言模型可选：没有就退化成一元词频整句；打包过的 lm.qj 优先
    let packed = std::path::PathBuf::from("data/generated/lm.qj");
    let unigram = std::path::PathBuf::from("data/generated/lm-unigram.tsv");
    let bigram = std::path::PathBuf::from("data/generated/lm-bigram.tsv");
    if packed.is_file() || (unigram.is_file() && bigram.is_file()) {
        let started = Instant::now();
        let model = if packed.is_file() {
            BigramModel::from_path(&packed)?
        } else {
            BigramModel::from_paths(&unigram, &bigram)?
        };
        tracing::info!(
            words = model.word_count(),
            bigrams = model.bigram_count(),
            load_ms = started.elapsed().as_millis(),
            "语言模型已加载"
        );
        engine = engine.with_language_model(Box::new(model));
    }
    if let Some(dir) = &args.neural {
        let started = Instant::now();
        let scorer = qingjian_neural::CharScorer::load(dir)?;
        tracing::info!(
            load_ms = started.elapsed().as_millis(),
            weight = args.neural_weight.unwrap_or(qingjian_core::NEURAL_WEIGHT),
            "神经重打分已启用"
        );
        engine = if args.neural_async {
            engine.with_async_sentence_scorer(
                Box::new(scorer),
                args.neural_weight,
                args.neural_margin,
                args.neural_context,
            )
        } else {
            engine.with_sentence_scorer(
                Box::new(scorer),
                args.neural_weight,
                args.neural_margin,
                args.neural_context,
            )
        };
    }
    let config_path = args
        .config
        .clone()
        .unwrap_or_else(args::default_config_file);
    let mut config = Config::load(&config_path)?;
    if let Some(scheme) = &args.shuangpin {
        config.general.shuangpin = if scheme == "off" {
            String::new()
        } else {
            scheme.clone()
        };
    }
    if let Some(scheme) = &args.fuma {
        config.general.fuma = if scheme == "off" {
            String::new()
        } else {
            scheme.clone()
        };
    }
    engine.set_fuma_hint(config.general.fuma_hint());
    if let Some(scheme) = config.general.fuma() {
        match load_fuma(&scheme) {
            Ok(table) => {
                tracing::info!(scheme = scheme.key(), words = table.len(), "辅码已启用");
                engine.set_fuma(Some(std::sync::Arc::new(table)));
            }
            Err(error) => tracing::error!(scheme = scheme.key(), %error, "辅码表加载失败，辅码关"),
        }
    }
    if let Some(scheme) = config.general.shuangpin() {
        tracing::info!(%scheme, "双拼已启用");
    }
    engine.set_shuangpin(config.general.shuangpin());
    Ok(engine)
}

/// 辅码表随仓库提供（`assets/fuma/<方案>.txt`），在仓库里跑 CLI 时可用。
fn load_fuma(scheme: &FumaScheme) -> Result<FumaTable, String> {
    let path = std::path::PathBuf::from("assets")
        .join("fuma")
        .join(format!("{}.txt", scheme.key()));
    FumaTable::from_path(&path).map_err(|error| format!("{}: {error}", path.display()))
}
