//! Interface strings for the 问题 workspace. Keys are English; zh-CN is the default locale.
//! SPM strings (issue titles, messages, people names) are shown verbatim and never translated.
use spm_contracts::{
    AssignmentBasis, CriterionKind, DefectSeverity, GatePhase, GateTrackState, IssueKind,
    IssueQueue, MatchPrecision, MatchReason, PersonRelation, ProposalStatus, ReasonKind,
    SearchChannel, SearchKind, SearchMode, SearchScope, SearchWhy, Standing, SuggestionKind,
};

#[derive(Clone, Copy)]
pub struct Lang {
    pub zh: bool,
}

impl Lang {
    pub fn new(locale: &str) -> Self {
        Self {
            zh: locale.starts_with("zh"),
        }
    }

    pub fn t(self, en: &'static str) -> &'static str {
        if !self.zh {
            return en;
        }
        match en {
            "Issues" => "问题",
            "Needs my reply" => "需要我回应",
            "Awaiting assignment confirmation" => "待确认分配",
            "Blocks next gate" => "阻塞下一评审点",
            "Legacy pending" => "遗留问题",
            "Source conflict" => "来源冲突",
            "Passed" => "已过",
            "Current" => "当前",
            "Upcoming" => "未到",
            "Ready for the gate" => "可以通过评审点",
            "Not ready" => "未就绪",
            "Pass" => "通过",
            "Fail" => "未通过",
            "DI within threshold" => "DI 不超过阈值",
            "No open fatal or major" => "无未关闭的致命或严重缺陷",
            "Legacy approved by PDT" => "遗留问题已由 PDT 批准",
            "Every AR has a confirmed owner" => "每个 AR 都有确认的负责人",
            "High risks closed or accepted" => "高风险已关闭或已接受",
            "Reaches the threshold" => "走完这些步骤后达到阈值",
            "Does not reach the threshold" => "这些步骤仍达不到阈值",
            "Unknown effort, listed last" => "未估工时，已排在最后",
            "Confirm and update Meegle" => "确认并更新 Meegle",
            "Record locally only" => "仅本地记录",
            "Not this" => "不是这样",
            "Reply in Lark" => "在 Lark 中回复",
            "Meegle update is not available yet; the confirmation is recorded locally." => {
                "Meegle 更新暂不可用；确认已在本地记录。"
            }
            "Proposed" => "待你确认",
            "Confirmed" => "已确认",
            "Recorded locally" => "仅本地记录",
            "Rejected" => "已否认",
            "Explicit statement" => "明确声明",
            "Not specified" => "未指定",
            "Unresolved" => "无法解析",
            "Unknown" => "未知",
            "Disputed" => "有争议",
            "Token" => "业务编号",
            "Link" => "链接",
            "Reply chain" => "回复链",
            "Exact" => "原文",
            "Snippet" => "摘录",
            "Index match" => "索引片段",
            "Lexical" => "字面",
            "Semantic" => "语义",
            "Hybrid" => "混合",
            "All" => "全部",
            "Messages" => "消息",
            "People" => "人",
            "Changes" => "变更",
            "Exact term" => "字面命中",
            "Same meaning" => "语义相近",
            "Linked to issue" => "已关联问题",
            "Participant" => "参与者",
            "Semantic similarity, not a confidence score" => "语义相似度，不是置信度",
            "Semantic search unavailable: no local embedding cache; lexical results shown" => {
                "语义检索不可用：没有可用的本地向量缓存，已改用字面检索"
            }
            "Source text required; excerpts were not used" => "需要保留原文；未用摘录替代",
            "Dismissed suggestions hidden" => "已忽略的建议不再出现",
            "Lark discussion" => "Lark 讨论",
            "Who to ask" => "可以问谁",
            "Suggested" => "建议",
            "See" => "查看",
            "items" => "条",
            "Trace" => "追溯",
            "Gate" => "评审点",
            "Phase" => "阶段",
            "Blocks gate" => "阻塞评审点",
            "Gate due" => "评审日期",
            "Severity" => "严重度",
            "Flag" => "标记",
            "Overdue" => "已逾期",
            "Source conflict flag" => "来源冲突",
            "Legacy flag" => "遗留",
            "Owner unconfirmed" => "负责人未确认",
            "Refresh failed" => "刷新失败",
            "Demo data" => "演示数据",
            "Loading issues…" => "正在读取问题…",
            "Refresh" => "刷新",
            "Search" => "搜索",
            "Search issues, messages or people" => "搜索问题、消息或人 · 按字面或意思",
            "Lark is not configured as a source" => "Lark 尚未配置为来源，无法打开",
            "Next gate" => "下一评审点",
            "Due" => "到期",
            "Affects" => "影响",
            "Back to my issues" => "返回我的问题",
            "My issues" => "我的问题",
            "Nothing needs you right now" => "目前没有需要你处理的问题",
            "Select an issue" => "选择一个问题",
            "Some assignments were not read" => "部分分配未读取",
            "Assignment from Lark" => "Lark 中的分配",
            "assigned by" => "，分配人",
            "Authority" => "分配权",
            "Acceptance" => "接受",
            "Evidence" => "证据",
            "Replies to a message that was not stored" => "回复的原消息未保存",
            "PDT and related people" => "PDT 与相关的人",
            "Who else to ask" => "还可以问谁",
            "rank score, not a confidence" => "排序分，不是置信度",
            "Shortest path to" => "到入口的最短路径",
            "Type a question; words and meaning both count" => "输入问题；字面和意思都会匹配",
            "No results" => "没有结果",
            "Ranking fuses word and meaning ranks; similar means related, not the same" => {
                "排序融合字面与语义排名；相似表示相关，不代表是同一件事"
            }
            "Suggestions for" => "建议 ·",
            "similarity" => "相似度",
            "IPD rule" => "IPD 规则",
            "Same module and time" => "同模块、时间相近",
            "Accept" => "关联",
            "Dismiss" => "忽略",
            "Suggestions are proposals: nothing is linked or changed until you accept" => {
                "建议只是提议：你确认之前不会关联或改动任何内容"
            }
            _ => en,
        }
    }

    pub fn phase(self, phase: GatePhase) -> &'static str {
        let en = match phase {
            GatePhase::Concept => "Concept",
            GatePhase::Plan => "Plan",
            GatePhase::Development => "Development",
            GatePhase::Verification => "Verification",
            GatePhase::Release => "Release",
            GatePhase::Lifecycle => "Lifecycle",
        };
        if self.zh {
            match phase {
                GatePhase::Concept => "概念",
                GatePhase::Plan => "计划",
                GatePhase::Development => "开发",
                GatePhase::Verification => "验证",
                GatePhase::Release => "发布",
                GatePhase::Lifecycle => "生命周期",
            }
        } else {
            en
        }
    }

    pub fn track(self, state: GateTrackState) -> &'static str {
        self.t(match state {
            GateTrackState::Passed => "Passed",
            GateTrackState::Current => "Current",
            GateTrackState::Upcoming => "Upcoming",
        })
    }

    pub fn severity(self, severity: DefectSeverity) -> &'static str {
        match (severity, self.zh) {
            (DefectSeverity::Fatal, false) => "Fatal",
            (DefectSeverity::Major, false) => "Major",
            (DefectSeverity::Normal, false) => "Normal",
            (DefectSeverity::Hint, false) => "Hint",
            (DefectSeverity::Fatal, true) => "致命",
            (DefectSeverity::Major, true) => "严重",
            (DefectSeverity::Normal, true) => "一般",
            (DefectSeverity::Hint, true) => "提示",
        }
    }

    pub fn kind(self, kind: IssueKind) -> &'static str {
        match kind {
            IssueKind::Ar => "AR",
            IssueKind::Sr => "SR",
            IssueKind::Ir => "IR",
            IssueKind::Dts => "DTS",
            IssueKind::Risk => "RISK",
            IssueKind::Ai => "AI",
        }
    }

    pub fn queue(self, queue: IssueQueue, next_gate: &str) -> String {
        match queue {
            IssueQueue::NeedsMyReply => self.t("Needs my reply").into(),
            IssueQueue::AwaitingAssignmentConfirmation => {
                self.t("Awaiting assignment confirmation").into()
            }
            IssueQueue::BlocksNextGate => {
                if self.zh {
                    format!("阻塞 {next_gate}")
                } else {
                    format!("Blocks {next_gate}")
                }
            }
            IssueQueue::LegacyPending => self.t("Legacy pending").into(),
            IssueQueue::SourceConflict => self.t("Source conflict").into(),
        }
    }

    pub fn criterion(self, kind: CriterionKind) -> &'static str {
        self.t(match kind {
            CriterionKind::DiThreshold => "DI within threshold",
            CriterionKind::NoOpenFatalOrMajor => "No open fatal or major",
            CriterionKind::LegacyApproved => "Legacy approved by PDT",
            CriterionKind::ArOwnerConfirmed => "Every AR has a confirmed owner",
            CriterionKind::HighRisksClosedOrAccepted => "High risks closed or accepted",
        })
    }

    pub fn proposal_status(self, status: ProposalStatus) -> &'static str {
        self.t(match status {
            ProposalStatus::Proposed => "Proposed",
            ProposalStatus::Confirmed => "Confirmed",
            ProposalStatus::LocalOnly => "Recorded locally",
            ProposalStatus::Rejected => "Rejected",
        })
    }

    pub fn basis(self, basis: AssignmentBasis) -> &'static str {
        self.t(match basis {
            AssignmentBasis::ExplicitStatement => "Explicit statement",
            AssignmentBasis::Unspecified => "Not specified",
            AssignmentBasis::Unresolved => "Unresolved",
        })
    }

    pub fn standing(self, standing: Standing) -> &'static str {
        self.t(match standing {
            Standing::Unknown => "Unknown",
            Standing::Confirmed => "Confirmed",
            Standing::Disputed => "Disputed",
        })
    }

    pub fn reason(self, reason: MatchReason) -> &'static str {
        self.t(match reason {
            MatchReason::Token => "Token",
            MatchReason::Link => "Link",
            MatchReason::ReplyChain => "Reply chain",
        })
    }

    pub fn precision(self, precision: MatchPrecision) -> &'static str {
        self.t(match precision {
            MatchPrecision::Exact => "Exact",
            MatchPrecision::Snippet => "Snippet",
            MatchPrecision::IndexSubstring => "Index match",
        })
    }

    pub fn search_mode(self, mode: SearchMode) -> &'static str {
        self.t(match mode {
            SearchMode::Lexical => "Lexical",
            SearchMode::Semantic => "Semantic",
            SearchMode::Hybrid => "Hybrid",
        })
    }

    pub fn search_scope(self, scope: SearchScope) -> &'static str {
        self.t(match scope {
            SearchScope::All => "All",
            SearchScope::Issues => "Issues",
            SearchScope::Messages => "Messages",
            SearchScope::People => "People",
            SearchScope::Changes => "Changes",
        })
    }

    pub fn search_kind(self, kind: SearchKind) -> &'static str {
        self.t(match kind {
            SearchKind::Issue => "Issues",
            SearchKind::Message => "Messages",
            SearchKind::Person => "People",
            SearchKind::Change => "Changes",
        })
    }

    pub fn relation(self, relation: PersonRelation) -> &'static str {
        match (relation, self.zh) {
            (PersonRelation::PdtManager, true) => "PDT 经理",
            (PersonRelation::PdtRepresentative, true) => "PDT 代表",
            (PersonRelation::DevelopmentRepresentative, true) => "开发代表",
            (PersonRelation::TestRepresentative, true) => "测试代表",
            (PersonRelation::ProposedOwner, true) => "提议负责人",
            (PersonRelation::Mentioned, true) => "被提到",
            (PersonRelation::Participant, true) => "参与者",
            (PersonRelation::PdtManager, false) => "PDT manager",
            (PersonRelation::PdtRepresentative, false) => "PDT representative",
            (PersonRelation::DevelopmentRepresentative, false) => "Development rep.",
            (PersonRelation::TestRepresentative, false) => "Test rep.",
            (PersonRelation::ProposedOwner, false) => "Proposed owner",
            (PersonRelation::Mentioned, false) => "Mentioned",
            (PersonRelation::Participant, false) => "Participant",
        }
    }

    pub fn reason_kind(self, reason: ReasonKind) -> &'static str {
        match (reason, self.zh) {
            (ReasonKind::ChangesInArea, true) => "相关代码变更",
            (ReasonKind::DiscussionInArea, true) => "相关讨论",
            (ReasonKind::MentionedInThread, true) => "讨论中被提到",
            (ReasonKind::SharedLabel, true) => "相同标签",
            (ReasonKind::RecentActivity, true) => "近期活跃",
            (ReasonKind::ExplicitAssignment, true) => "明确分配",
            (ReasonKind::ChangesInArea, false) => "Changes in this area",
            (ReasonKind::DiscussionInArea, false) => "Discussed this area",
            (ReasonKind::MentionedInThread, false) => "Mentioned in the thread",
            (ReasonKind::SharedLabel, false) => "Shared label",
            (ReasonKind::RecentActivity, false) => "Recently active",
            (ReasonKind::ExplicitAssignment, false) => "Explicitly assigned",
        }
    }

    pub fn suggestion_kind(self, kind: SuggestionKind) -> &'static str {
        match (kind, self.zh) {
            (SuggestionKind::RelatedDiscussion, true) => "可能相关的讨论",
            (SuggestionKind::DuplicateCandidate, true) => "可能重复",
            (SuggestionKind::WhoToAsk, true) => "建议请教",
            (SuggestionKind::MissingIpdInfo, true) => "缺少 IPD 信息",
            (SuggestionKind::RelatedDiscussion, false) => "Related discussion",
            (SuggestionKind::DuplicateCandidate, false) => "Possible duplicate",
            (SuggestionKind::WhoToAsk, false) => "Who to ask",
            (SuggestionKind::MissingIpdInfo, false) => "Missing IPD information",
        }
    }

    pub fn channel(self, channel: SearchChannel) -> &'static str {
        self.t(match channel {
            SearchChannel::Lexical => "Lexical",
            SearchChannel::Semantic => "Semantic",
        })
    }

    pub fn search_why(self, why: SearchWhy) -> &'static str {
        self.t(match why {
            SearchWhy::ExactTerm => "Exact term",
            SearchWhy::SameMeaning => "Same meaning",
            SearchWhy::LinkedToIssue => "Linked to issue",
            SearchWhy::Participant => "Participant",
        })
    }
}
