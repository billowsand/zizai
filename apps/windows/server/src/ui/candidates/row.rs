//! 候选窗口的一行：[`Candidate`] → 渲染器的 [`Row`]（序号、候选词、annotation 片段）。

use qingjian_core::{Candidate, CandidateKind};
use qingjian_render::{Row, Tone};

/// `position` 是页内下标（从 0 起）。
pub(crate) fn from_candidate(position: usize, candidate: &Candidate) -> Row {
    let mut annotation = Vec::new();
    // 辅码不标在候选旁：标注行一出现候选框高度就变。只敲首码时剩下的那一码由拼音行的
    // 幽灵提示给（RenderData::fuma_hint），两码敲完时辅码段本来就在拼音行里。
    if let Some(reading) = &candidate.reading {
        if !annotation.is_empty() {
            annotation.push((" · ".to_owned(), Tone::Faint));
        }
        annotation.push((reading.clone(), Tone::Gloss));
    }
    if let Some(translation) = &candidate.translation {
        for (i, sense) in translation.senses().iter().enumerate() {
            if i > 0 || !annotation.is_empty() {
                annotation.push((" · ".to_owned(), Tone::Faint));
            }
            if let Some(pos) = sense.part_of_speech {
                annotation.push((format!("{pos} "), Tone::Faint));
            }
            let tone = if sense.fresh {
                Tone::Fresh
            } else {
                Tone::Gloss
            };
            for segment in sense.furigana() {
                annotation.push((segment.text, tone));
                if let Some(reading) = segment.reading {
                    annotation.push((format!("({reading})"), Tone::Faint));
                }
            }
        }
    }
    Row {
        index: (position + 1).to_string(),
        text: candidate.text.clone(),
        annotation,
        cloud: false,
        sentence: candidate.kind == CandidateKind::Sentence,
    }
}
