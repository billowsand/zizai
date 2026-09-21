//! 语音润色用的专名保护表：把输入法里“用户自己建立”的写法收集给大模型按原样保留。

use std::collections::HashSet;
use std::path::Path;

use qingjian_core::CustomPhrase;

/// 单条与总量上限；占位过滤在 Worker 那侧还有一道兜底。
const MAX_WORD: usize = 20;
const MAX_WORDS: usize = 60;

/// 汇总热词：自定义短语 + 用户词 + 高频上屏词。读不了的文件静默跳过。
pub fn collect(user_dir: Option<&Path>, custom_phrases: &[CustomPhrase]) -> Vec<String> {
    let mut words = Vec::new();
    let mut seen = HashSet::new();
    let push = |word: &str, seen: &mut HashSet<String>, words: &mut Vec<String>| {
        let word = word.trim();
        if word.is_empty()
            || word.chars().count() > MAX_WORD
            || !word.chars().any(|character| character.is_alphanumeric())
            || !seen.insert(word.to_owned())
        {
            return;
        }
        words.push(word.to_owned());
    };
    for phrase in custom_phrases {
        push(&phrase.text, &mut seen, &mut words);
    }
    if let Some(dir) = user_dir {
        // 用户自造词（词库里没有但用户确认过的）：最需要按原样保护。
        let words_path = dir.join("user-words.tsv");
        if let Ok(content) = std::fs::read_to_string(&words_path) {
            for line in content.lines() {
                let first = line.split('\t').next().unwrap_or("");
                push(first, &mut seen, &mut words);
            }
        }
        // 个人词频上屏最高的，按次数从高到低取。
        let frequency_path = dir.join("user.tsv");
        if let Ok(content) = std::fs::read_to_string(&frequency_path) {
            let mut counted: Vec<(String, u32)> = content
                .lines()
                .filter_map(|line| {
                    let (word, count) = line.split_once('\t')?;
                    count
                        .trim()
                        .parse::<u32>()
                        .ok()
                        .map(|count| (word.to_owned(), count))
                })
                .collect();
            counted.sort_unstable_by_key(|entry| std::cmp::Reverse(entry.1));
            for (word, _) in counted {
                push(&word, &mut seen, &mut words);
            }
        }
    }
    words.truncate(MAX_WORDS);
    words
}

#[cfg(test)]
mod tests {
    use super::collect;
    use qingjian_core::CustomPhrase;

    fn word(text: &str) -> CustomPhrase {
        CustomPhrase {
            code: "aa".into(),
            text: text.to_owned(),
            position: 1,
            enabled: true,
        }
    }

    #[test]
    fn merges_sources_dedupes_and_caps() {
        let dir = std::env::temp_dir().join("qingjian-hotword-test");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("user-words.tsv"), "你好世界科技\tihao\n").unwrap();
        std::fs::write(dir.join("user.tsv"), "王经理\t12\n你好\t4\n").unwrap();

        let collected = collect(Some(&dir), &[word("你好世界科技"), word("自造短句扩展")]);
        assert_eq!(collected.first().map(String::as_str), Some("你好世界科技"));
        assert!(collected.contains(&"王经理".to_owned()));
        assert!(collected.iter().all(|item| item.chars().count() <= 20));
        assert!(collected.len() <= 60);
    }

    #[test]
    fn skips_punctuation_only_entries() {
        let collected = collect(None, &[word("："), word("目标")]);
        assert_eq!(collected, vec!["目标".to_owned()]);
    }
}
