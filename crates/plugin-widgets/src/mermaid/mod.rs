//! A dependency-free subset of Mermaid for the diagram widget, parsed and laid out
//! natively. The pipeline follows Pele's (MIT) shape — preprocess, detect, then a
//! per-type parser and layout behind a small registry — with no code or data shared.
//! Everything here is platform-free and measured in DIPs.

mod flowchart;
mod gantt;

pub(crate) use flowchart::{
    CLUSTER_TITLE, EDGE_LABEL_SIZE, FlowLayout, Flowchart, LABEL_SIZE, Shape, Stroke,
    layout_flowchart,
};
pub(crate) use gantt::Gantt;

pub(crate) const MAX_NODES: usize = 200;
pub(crate) const MAX_EDGES: usize = 400;
pub(crate) const MAX_TASKS: usize = 120;
const MAX_LABEL_CHARS: usize = 80;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Diagram {
    Flowchart(Flowchart),
    Gantt(Gantt),
}

impl Diagram {
    pub(crate) fn title(&self) -> Option<&str> {
        match self {
            Diagram::Flowchart(chart) => chart.title.as_deref(),
            Diagram::Gantt(chart) => chart.title.as_deref(),
        }
    }
}

/// What went wrong, independent of the UI language.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ErrorKind {
    UnknownType,
    UnsupportedType(String),
    UnexpectedText,
    MissingTarget,
    UnclosedLabel,
    EmptyLabel,
    TooLarge,
    BadDate,
    BadDuration,
    UnknownTask(String),
    UnsupportedDateFormat,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParseError {
    /// 1-based source line, 0 when the problem is not tied to one line.
    pub(crate) line: usize,
    pub(crate) kind: ErrorKind,
    pub(crate) text: String,
}

impl ParseError {
    pub(crate) fn message(&self, chinese: bool) -> String {
        use ErrorKind::*;
        match (&self.kind, chinese) {
            (UnknownType, true) => "第一行需写明图表类型，例如 flowchart LR 或 gantt".into(),
            (UnknownType, false) => {
                "Start with a diagram type, such as flowchart LR or gantt".into()
            }
            (UnsupportedType(kind), true) => {
                format!("暂不支持 {kind}；目前支持 flowchart 和 gantt")
            }
            (UnsupportedType(kind), false) => {
                format!("{kind} is not supported yet; use flowchart or gantt")
            }
            (UnexpectedText, true) => "无法识别这一行".into(),
            (UnexpectedText, false) => "This line could not be read".into(),
            (MissingTarget, true) => "箭头缺少目标节点".into(),
            (MissingTarget, false) => "An arrow is missing its target".into(),
            (UnclosedLabel, true) => "标签缺少右括号或右引号".into(),
            (UnclosedLabel, false) => "A label is missing its closing bracket or quote".into(),
            (EmptyLabel, true) => "任务名称不能为空".into(),
            (EmptyLabel, false) => "A task needs a name".into(),
            (TooLarge, true) => format!(
                "图表过大（最多 {MAX_NODES} 个节点、{MAX_EDGES} 条连线、{MAX_TASKS} 个任务）"
            ),
            (TooLarge, false) => format!(
                "Diagram too large (at most {MAX_NODES} nodes, {MAX_EDGES} links, {MAX_TASKS} tasks)"
            ),
            (BadDate, true) => "日期应为 YYYY-MM-DD".into(),
            (BadDate, false) => "Dates must be YYYY-MM-DD".into(),
            (BadDuration, true) => "时长应为天数或周数，例如 3d 或 2w".into(),
            (BadDuration, false) => "Durations must be days or weeks, such as 3d or 2w".into(),
            (UnknownTask(id), true) => format!("找不到任务 {id}"),
            (UnknownTask(id), false) => format!("No task has the id {id}"),
            (UnsupportedDateFormat, true) => "目前仅支持 dateFormat YYYY-MM-DD".into(),
            (UnsupportedDateFormat, false) => "Only dateFormat YYYY-MM-DD is supported".into(),
        }
    }
}

fn error(line: usize, kind: ErrorKind, text: &str) -> ParseError {
    ParseError {
        line,
        kind,
        text: text.trim().chars().take(60).collect(),
    }
}

// ---------------------------------------------------------------- pipeline

/// Source after preprocessing: front matter read, `%%` comments and blank lines gone.
pub(crate) struct Source<'a> {
    /// Front matter `title:`, the only key read.
    pub(crate) title: Option<String>,
    /// Statements with their 1-based line numbers; the first is the type header.
    pub(crate) lines: Vec<(usize, &'a str)>,
}

pub(crate) fn preprocess(source: &str) -> Source<'_> {
    let mut lines = source
        .lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l))
        .peekable();
    let mut title = None;
    while lines.peek().is_some_and(|(_, l)| l.trim().is_empty()) {
        lines.next();
    }
    if lines.peek().is_some_and(|(_, l)| l.trim() == "---") {
        lines.next();
        for (_, line) in lines.by_ref() {
            let line = line.trim();
            if line == "---" {
                break;
            }
            if let Some(value) = line.strip_prefix("title:") {
                title = Some(unquote(value.trim()).to_string());
            }
        }
    }
    let lines = lines
        .map(|(n, l)| (n, strip_comment(l).trim()))
        .filter(|(_, l)| !l.is_empty())
        .collect();
    Source { title, lines }
}

/// One supported diagram type: the header keywords it answers to and its parser.
/// A new type is a module with a `parse` function and one entry here.
struct DiagramType {
    keywords: &'static [&'static str],
    parse: fn(&Source) -> Result<Diagram, ParseError>,
}

static TYPES: [DiagramType; 2] = [
    DiagramType {
        keywords: &["flowchart", "graph"],
        parse: flowchart::parse,
    },
    DiagramType {
        keywords: &["gantt"],
        parse: gantt::parse,
    },
];

/// Mermaid types recognised only to say they are not supported yet.
const UNSUPPORTED: [&str; 19] = [
    "sequenceDiagram",
    "classDiagram",
    "stateDiagram",
    "stateDiagram-v2",
    "erDiagram",
    "journey",
    "pie",
    "timeline",
    "gitGraph",
    "mindmap",
    "quadrantChart",
    "requirementDiagram",
    "C4Context",
    "sankey-beta",
    "xychart-beta",
    "block-beta",
    "packet-beta",
    "kanban",
    "architecture-beta",
];

/// The first keyword of the first statement picks the type.
fn detect(source: &Source) -> Result<&'static DiagramType, ParseError> {
    let Some(&(line, header)) = source.lines.first() else {
        return Err(error(0, ErrorKind::UnknownType, ""));
    };
    let keyword = header.split_whitespace().next().unwrap_or("");
    if let Some(kind) = TYPES.iter().find(|t| t.keywords.contains(&keyword)) {
        return Ok(kind);
    }
    let kind = if UNSUPPORTED.contains(&keyword) {
        ErrorKind::UnsupportedType(keyword.into())
    } else {
        ErrorKind::UnknownType
    };
    Err(error(line, kind, header))
}

/// Parse Mermaid source. Front matter `title:` and `%%` comments are honoured.
pub(crate) fn parse(source: &str) -> Result<Diagram, ParseError> {
    let source = preprocess(source);
    (detect(&source)?.parse)(&source)
}

fn strip_comment(line: &str) -> &str {
    match line.find("%%") {
        Some(index) => &line[..index],
        None => line,
    }
}

fn unquote(text: &str) -> &str {
    text.strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or(text)
}

// ---------------------------------------------------------------- text measurement

/// Advance widths for layout. The widget passes the host's DirectWrite measurer;
/// tests, and hosts without one, use the deterministic [`Estimate`].
pub(crate) trait TextMeasure {
    /// Width of one line of `text` at `size` DIPs.
    fn line_width(&self, text: &str, size: f32) -> f32;
}

/// Deterministic per-character estimate for Segoe UI / Noto Sans SC: Latin about
/// 0.56 em (capitals 0.64), East Asian wide and fullwidth one em, combining marks,
/// controls and zero-width characters nothing, everything else 0.6 em.
pub(crate) struct Estimate;

impl TextMeasure for Estimate {
    fn line_width(&self, text: &str, size: f32) -> f32 {
        text.chars().map(char_em).sum::<f32>() * size
    }
}

fn char_em(c: char) -> f32 {
    match c as u32 {
        0x20..=0x7e if c.is_ascii_uppercase() => 0.64,
        0x20..=0x7e => 0.56,
        0..=0x1f
        | 0x7f..=0x9f
        | 0xad
        | 0x300..=0x36f
        | 0x483..=0x489
        | 0x591..=0x5bd
        | 0x610..=0x61a
        | 0x64b..=0x65f
        | 0x1160..=0x11ff
        | 0x1ab0..=0x1aff
        | 0x1dc0..=0x1dff
        | 0x200b..=0x200f
        | 0x202a..=0x202e
        | 0x2060..=0x2064
        | 0x20d0..=0x20ff
        | 0xfe00..=0xfe0f
        | 0xfe20..=0xfe2f
        | 0xfeff
        | 0xe0000..=0xe01ef => 0.0,
        0x1100..=0x115f
        | 0x2e80..=0x303e
        | 0x3041..=0x33ff
        | 0x3400..=0x4dbf
        | 0x4e00..=0x9fff
        | 0xa000..=0xa4cf
        | 0xa960..=0xa97f
        | 0xac00..=0xd7a3
        | 0xf900..=0xfaff
        | 0xfe30..=0xfe4f
        | 0xff00..=0xff60
        | 0xffe0..=0xffe6
        | 0x1f300..=0x1f64f
        | 0x1f900..=0x1f9ff
        | 0x20000..=0x3fffd => 1.0,
        _ => 0.6,
    }
}

/// Width of the widest line of `text`.
pub(crate) fn text_width(measure: &dyn TextMeasure, text: &str, size: f32) -> f32 {
    text.lines()
        .map(|line| measure.line_width(line, size))
        .fold(0.0, f32::max)
}

/// Greedy wrap to `max` DIPs: breaks after spaces and around wide characters, and
/// inside a word only when the word alone is too wide.
pub(crate) fn wrap(measure: &dyn TextMeasure, text: &str, size: f32, max: f32) -> String {
    let mut out = String::new();
    for (i, line) in text.lines().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if measure.line_width(line, size) <= max {
            out.push_str(line);
            continue;
        }
        let (mut start, mut cut, mut wide_before) = (0, None, false);
        for (at, c) in line.char_indices() {
            let wide = char_em(c) == 1.0;
            if at > start && (wide || wide_before) {
                cut = Some(at);
            }
            let end = at + c.len_utf8();
            if at > start && c != ' ' && measure.line_width(&line[start..end], size) > max {
                let at = cut.filter(|&b| b > start).unwrap_or(at);
                out.push_str(line[start..at].trim_end());
                out.push('\n');
                start = at + line[at..].len() - line[at..].trim_start().len();
                cut = None;
            }
            if c == ' ' {
                cut = Some(end);
            }
            wide_before = wide;
        }
        out.push_str(&line[start..]);
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Box2 {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) w: f32,
    pub(crate) h: f32,
}

#[cfg(test)]
mod tests {
    use super::flowchart::Direction;
    use super::*;
    use chrono::NaiveDate;

    fn flow(source: &str) -> Flowchart {
        match parse(source).unwrap() {
            Diagram::Flowchart(chart) => chart,
            other => panic!("expected flowchart, got {other:?}"),
        }
    }

    #[test]
    fn parses_shapes_chains_groups_and_link_labels() {
        let chart = flow(
            "---\ntitle: 版本计划\n---\nflowchart LR\n  A[需求冻结] --> SYS[SYS 开发] & VND(VND 开发)\n  SYS & VND --> I{集成}\n  I -- 通过 --> V((验证))\n  I -.->|返工| SYS\n  V ==> E([发布]) %% done\n  style I stroke:#f00\n",
        );
        assert_eq!(chart.title.as_deref(), Some("版本计划"));
        assert_eq!(chart.direction, Direction::LeftRight);
        let shapes: Vec<_> = chart
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), n.shape))
            .collect();
        assert_eq!(
            shapes,
            [
                ("A", Shape::Rect),
                ("SYS", Shape::Rect),
                ("VND", Shape::Round),
                ("I", Shape::Diamond),
                ("V", Shape::Circle),
                ("E", Shape::Stadium)
            ]
        );
        assert_eq!(chart.edges.len(), 7);
        let pass = &chart.edges[4];
        assert_eq!(pass.label.as_deref(), Some("通过"));
        assert!(pass.arrow);
        let rework = &chart.edges[5];
        assert_eq!(
            (rework.stroke, rework.label.as_deref()),
            (Stroke::Dotted, Some("返工"))
        );
        assert_eq!(chart.edges[6].stroke, Stroke::Thick);
    }

    #[test]
    fn reports_line_numbers_and_unsupported_types() {
        let err = parse("flowchart LR\n  A --> B\n  SYS -->\n").unwrap_err();
        assert_eq!((err.line, &err.kind), (3, &ErrorKind::MissingTarget));
        assert_eq!(err.text, "SYS -->");
        let err = parse("flowchart TD\n  A[open").unwrap_err();
        assert_eq!(err.kind, ErrorKind::UnclosedLabel);
        let err = parse("sequenceDiagram\n A->>B: hi").unwrap_err();
        assert_eq!(
            err.kind,
            ErrorKind::UnsupportedType("sequenceDiagram".into())
        );
        assert_eq!(parse("  \n").unwrap_err().kind, ErrorKind::UnknownType);
        assert!(err.message(true).contains("flowchart"));
    }

    #[test]
    fn gantt_rejects_bad_duration_unicode_without_panicking() {
        for duration in ["三天", "1天", "😀"] {
            let source = format!("gantt\n A :2026-10-19, {duration}\n");
            assert_eq!(parse(&source).unwrap_err().kind, ErrorKind::BadDuration);
        }
    }

    #[test]
    fn gantt_rejects_bad_duration_date_overflow_without_panicking() {
        let source = format!("gantt\n A :{}, 1d\n", NaiveDate::MAX.format("%Y-%m-%d"));
        assert_eq!(parse(&source).unwrap_err().kind, ErrorKind::BadDuration);
    }

    #[test]
    fn gantt_supports_after_durations_and_milestones() {
        let source = "gantt\n  title 版本计划\n  dateFormat YYYY-MM-DD\n  section 开发\n  SYS 开发 :sys, 2026-10-19, 18d\n  VND 开发 :vnd, 2026-10-19, 3w\n  section 集成\n  集成 :crit, int, after sys vnd, 11d\n  验证 :10d\n  发布 :milestone, after int, 0d\n";
        let Diagram::Gantt(chart) = parse(source).unwrap() else {
            panic!("expected gantt");
        };
        assert_eq!(chart.title.as_deref(), Some("版本计划"));
        let d = |m, day| NaiveDate::from_ymd_opt(2026, m, day).unwrap();
        assert_eq!(chart.tasks[1].end, d(11, 9));
        assert_eq!(chart.tasks[2].start, d(11, 9));
        assert!(chart.tasks[2].critical);
        assert_eq!(chart.tasks[3].start, d(11, 20));
        assert!(chart.tasks[4].milestone);
        assert_eq!(chart.tasks[4].start, chart.tasks[4].end);
        assert_eq!(chart.span(), Some((d(10, 19), d(11, 30))));
        let err = parse("gantt\n  A :x, after nope, 2d").unwrap_err();
        assert_eq!(err.kind, ErrorKind::UnknownTask("nope".into()));
        assert_eq!(
            parse("gantt\ndateFormat DD-MM\n").unwrap_err().kind,
            ErrorKind::UnsupportedDateFormat
        );
    }

    #[test]
    fn preprocess_reads_front_matter_and_drops_comments() {
        let source = preprocess(
            "\n---\ntitle: \"计划\"\nother: x\n---\n%% note\ngantt %% type\n\n  A :2026-10-19, 1d\n",
        );
        assert_eq!(source.title.as_deref(), Some("计划"));
        assert_eq!(source.lines, [(7, "gantt"), (9, "A :2026-10-19, 1d")]);
    }

    #[test]
    fn detect_routes_keywords_through_the_registry() {
        for (header, keyword) in [
            ("graph TD", "flowchart"),
            ("flowchart LR", "flowchart"),
            ("gantt", "gantt"),
        ] {
            let kind = detect(&preprocess(header)).ok().unwrap();
            assert!(kind.keywords.contains(&keyword));
        }
        let err = detect(&preprocess("\n\npie title x")).err().unwrap();
        assert_eq!(
            (err.line, err.kind),
            (3, ErrorKind::UnsupportedType("pie".into()))
        );
        let err = detect(&preprocess("flow LR")).err().unwrap();
        assert_eq!(err.kind, ErrorKind::UnknownType);
    }

    #[test]
    fn estimate_ignores_marks_and_zero_width_and_widens_east_asian_text() {
        let em = |text: &str| Estimate.line_width(text, 10.0);
        assert_eq!(em("e\u{301}"), em("e"));
        assert_eq!(em("a\u{200b}\u{200d}\u{fe0f}\u{feff}b"), em("ab"));
        for wide in ["中", "한", "Ａ", "の"] {
            assert_eq!(em(wide), 10.0);
        }
        assert!(em("W") > em("w"));
        assert_eq!(text_width(&Estimate, "ab\n中文", 10.0), 20.0);
    }

    #[test]
    fn wrap_breaks_at_spaces_then_between_wide_characters_then_inside_words() {
        let fit = |text: &str, max: f32| wrap(&Estimate, text, 10.0, max);
        assert_eq!(fit("short", 100.0), "short");
        assert_eq!(fit("alpha beta gamma", 50.0), "alpha\nbeta\ngamma");
        assert_eq!(fit("需求冻结评审", 40.0), "需求冻结\n评审");
        assert_eq!(fit("abcdefghij", 30.0), "abcde\nfghij");
        for line in fit("integration verification 集成验证 done", 80.0).lines() {
            assert!(text_width(&Estimate, line, 10.0) <= 80.0, "{line}");
        }
    }
}
