//! 同音词替换配置。

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct HrConfig {
    pub(crate) lexicon: Option<String>,

    pub(crate) rule_fsts: Option<String>,
}

impl HrConfig {
    pub(crate) fn is_enabled(&self) -> bool {
        self.lexicon.is_some() || self.rule_fsts.is_some()
    }
}
