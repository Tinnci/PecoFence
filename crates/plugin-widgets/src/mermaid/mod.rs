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
    /// A Mermaid directive read but not applied; the result may differ from Mermaid.
    Ignored(String),
    /// Styling or display-only statements that do not change what is drawn.
    IgnoredStyle(String),
    BadExclude(String),
    ExcludesEverything,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Severity {
    Info,
    Warning,
    /// Fatal: nothing could be drawn from the source.
    Error,
}

/// A problem in the source: fatal as `Err`, otherwise one of [`Parsed::warnings`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Diagnostic {
    /// 1-based source line, 0 when the problem is not tied to one line.
    pub(crate) line: usize,
    pub(crate) kind: ErrorKind,
    pub(crate) text: String,
    pub(crate) severity: Severity,
}

/// A drawable diagram and the non-fatal problems found on the way.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Parsed {
    pub(crate) diagram: Diagram,
    pub(crate) warnings: Vec<Diagnostic>,
}

impl Diagnostic {
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
            (UnsupportedDateFormat, true) => {
                "仅支持 dateFormat YYYY-MM-DD，日期按此格式读取".into()
            }
            (UnsupportedDateFormat, false) => {
                "Only dateFormat YYYY-MM-DD is supported; dates are read as YYYY-MM-DD".into()
            }
            (Ignored(what), true) => format!("暂不支持 {what}，已忽略，日期可能与 Mermaid 不同"),
            (Ignored(what), false) => {
                format!("{what} is not supported and was ignored; dates may differ from Mermaid")
            }
            (IgnoredStyle(what), true) => format!("{what} 不影响绘制，已忽略"),
            (IgnoredStyle(what), false) => format!("{what} is ignored when drawing"),
            (BadExclude(what), true) => {
                format!("excludes 无法识别 {what}；可用 weekends、星期名或 YYYY-MM-DD")
            }
            (BadExclude(what), false) => {
                format!("excludes cannot read {what}; use weekends, a weekday name or YYYY-MM-DD")
            }
            (ExcludesEverything, true) => "excludes 排除了每一天，已忽略按星期排除".into(),
            (ExcludesEverything, false) => {
                "excludes leaves no working day; weekday exclusions were ignored".into()
            }
        }
    }
}

fn diagnostic(line: usize, kind: ErrorKind, text: &str, severity: Severity) -> Diagnostic {
    Diagnostic {
        line,
        kind,
        text: text.trim().chars().take(60).collect(),
        severity,
    }
}

fn error(line: usize, kind: ErrorKind, text: &str) -> Diagnostic {
    diagnostic(line, kind, text, Severity::Error)
}

// ---------------------------------------------------------------- pipeline

/// Source after preprocessing: front matter read, comment and blank lines gone.
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
    // As in Mermaid, `%%` starts a comment only as a line's first text (this also
    // drops `%%{init}%%` directives); elsewhere it is ordinary label text.
    let lines = lines
        .map(|(n, l)| (n, l.trim()))
        .filter(|(_, l)| !l.is_empty() && !l.starts_with("%%"))
        .collect();
    Source { title, lines }
}

/// One supported diagram type: the header keywords it answers to and its parser.
/// A new type is a module with a `parse` function and one entry here.
struct DiagramType {
    keywords: &'static [&'static str],
    parse: fn(&Source, &mut Vec<Diagnostic>) -> Result<Diagram, Diagnostic>,
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
fn detect(source: &Source) -> Result<&'static DiagramType, Diagnostic> {
    let Some(&(line, header)) = source.lines.first() else {
        return Err(error(0, ErrorKind::UnknownType, ""));
    };
    let keyword = header
        .split(|c: char| c.is_whitespace() || c == ';')
        .next()
        .unwrap_or("");
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

/// Parse Mermaid source. Front matter `title:` and `%%` comment lines are honoured.
pub(crate) fn parse(source: &str) -> Result<Parsed, Diagnostic> {
    let source = preprocess(source);
    let mut warnings = Vec::new();
    let diagram = (detect(&source)?.parse)(&source, &mut warnings)?;
    Ok(Parsed { diagram, warnings })
}

/// Split a line into statements at `;`, except inside quotes, `[]`, `()`, `{}`,
/// `|…|` link labels and `#…;` entity codes.
pub(crate) fn statements(line: &str) -> Vec<&str> {
    let (mut out, mut start, mut at) = (Vec::new(), 0, 0);
    let (mut depth, mut quoted, mut piped) = (0usize, false, false);
    // Byte scan: every delimiter is ASCII, and UTF-8 continuation bytes never are.
    while let Some(&b) = line.as_bytes().get(at) {
        match b {
            b'"' => quoted = !quoted,
            _ if quoted => {}
            b'#' if entity_len(&line[at..]).is_some() => {
                at += entity_len(&line[at..]).unwrap_or(1);
                continue;
            }
            b'[' | b'(' | b'{' => depth += 1,
            b']' | b')' | b'}' => depth = depth.saturating_sub(1),
            b'|' if depth == 0 => piped = !piped,
            b';' if depth == 0 && !piped => {
                out.push(line[start..at].trim());
                start = at + 1;
            }
            _ => {}
        }
        at += 1;
    }
    out.push(line[start..].trim());
    out.retain(|s| !s.is_empty());
    out
}

/// Length of a Mermaid entity code such as `#59;` or `#quot;` at the start of `text`.
fn entity_len(text: &str) -> Option<usize> {
    let body = text.strip_prefix('#')?;
    let n = body.bytes().take_while(u8::is_ascii_alphanumeric).count();
    ((1..=8).contains(&n) && body.as_bytes().get(n) == Some(&b';')).then_some(n + 2)
}

/// Decode Mermaid's entity codes: `#NNN;` (decimal code point) and named ones such
/// as `#quot;` or `#59;` for `;`. Unknown codes stay as written.
pub(crate) fn decode_entities(text: &str) -> String {
    const NAMED: [(&str, char); 18] = [
        ("quot", '"'),
        ("amp", '&'),
        ("lt", '<'),
        ("gt", '>'),
        ("apos", '\''),
        ("nbsp", '\u{a0}'),
        ("num", '#'),
        ("semi", ';'),
        ("colon", ':'),
        ("vert", '|'),
        ("lpar", '('),
        ("rpar", ')'),
        ("lsqb", '['),
        ("rsqb", ']'),
        ("lbrack", '['),
        ("rbrack", ']'),
        ("lcub", '{'),
        ("rcub", '}'),
    ];
    let (mut out, mut rest) = (String::with_capacity(text.len()), text);
    while let Some(at) = rest.find('#') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let decoded = entity_len(rest).and_then(|n| {
            let name = &rest[1..n - 1];
            let c = match name.parse::<u32>() {
                Ok(code) => char::from_u32(code).filter(|c| !c.is_control()),
                Err(_) => NAMED.iter().find(|(k, _)| *k == name).map(|&(_, c)| c),
            };
            c.map(|c| (n, c))
        });
        let (n, c) = decoded.unwrap_or((1, '#'));
        out.push(c);
        rest = &rest[n..];
    }
    out.push_str(rest);
    out
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
        match parse(source).unwrap().diagram {
            Diagram::Flowchart(chart) => chart,
            other => panic!("expected flowchart, got {other:?}"),
        }
    }

    #[test]
    fn parses_shapes_chains_groups_and_link_labels() {
        let chart = flow(
            "---\ntitle: 版本计划\n---\nflowchart LR\n  A[需求冻结] --> SYS[SYS 开发] & VND(VND 开发)\n  SYS & VND --> I{集成}\n  I -- 通过 --> V((验证))\n  I -.->|返工| SYS\n  V ==> E([发布])\n  %% done\n  style I stroke:#f00\n",
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
        let Diagram::Gantt(chart) = parse(source).unwrap().diagram else {
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
        let warnings = parse("gantt\ndateFormat DD-MM\n").unwrap().warnings;
        assert_eq!(
            (warnings[0].kind.clone(), warnings[0].severity),
            (ErrorKind::UnsupportedDateFormat, Severity::Warning)
        );
    }

    fn gantt(source: &str) -> (Gantt, Vec<Diagnostic>) {
        match parse(source).unwrap() {
            Parsed {
                diagram: Diagram::Gantt(chart),
                warnings,
            } => (chart, warnings),
            other => panic!("expected gantt, got {other:?}"),
        }
    }

    #[test]
    fn gantt_excludes_stretch_durations_over_closed_days() {
        let d = |m, day| NaiveDate::from_ymd_opt(2026, m, day).unwrap();
        // 2026-10-19 is a Monday. Five working days end on Friday (exclusive Saturday);
        // the next task starts on Monday; a Friday start runs Fri, Mon, Tue.
        let source = "gantt\n A :a, 2026-10-19, 5d\n B :after a, 2d\n C :2026-10-23, 3d\n D :d, 2026-10-19, 2026-10-25\n E :after d, 1d\n excludes weekends\n";
        let (chart, warnings) = gantt(source);
        let spans: Vec<_> = chart.tasks.iter().map(|t| (t.start, t.end)).collect();
        assert_eq!(
            spans,
            [
                (d(10, 19), d(10, 24)),
                (d(10, 26), d(10, 28)),
                (d(10, 23), d(10, 28)),
                // An explicit end stands as written, as in Mermaid.
                (d(10, 19), d(10, 25)),
                (d(10, 25), d(10, 27)),
            ]
        );
        assert!(warnings.is_empty());
        // `weekend friday` makes Friday and Saturday the weekend; dates and
        // weekday names close single days.
        let (chart, _) = gantt(
            "gantt\n excludes weekends, 2026-10-20 thursday\n weekend friday\n A :2026-10-19, 3d\n",
        );
        assert_eq!(chart.tasks[0].end, d(10, 26));
        let (chart, _) = gantt("gantt\n A :2026-10-19, 5d\n");
        assert_eq!(chart.tasks[0].end, d(10, 24));
    }

    #[test]
    fn gantt_warns_about_directives_it_cannot_apply() {
        let (chart, warnings) = gantt(
            "gantt\n dateFormat YYYY-MM-DD\n axisFormat %m-%d\n includes 2026-10-24\n todayMarker off\n tickInterval 1week\n weekday monday\n inclusiveEndDates\n excludes weekends, someday\n weekend sunday\n A :2026-10-19, 1d\n",
        );
        assert_eq!(chart.tasks.len(), 1);
        let found: Vec<_> = warnings
            .iter()
            .map(|w| (w.line, w.severity, &w.kind))
            .collect();
        use ErrorKind::*;
        use Severity::*;
        assert_eq!(
            found,
            [
                (9, Warning, &BadExclude("someday".into())),
                (10, Warning, &BadExclude("sunday".into())),
                (3, Info, &IgnoredStyle("axisFormat".into())),
                (4, Warning, &Ignored("includes".into())),
                (5, Warning, &Ignored("todayMarker".into())),
                (6, Warning, &Ignored("tickInterval".into())),
                (7, Warning, &Ignored("weekday".into())),
                (8, Warning, &Ignored("inclusiveEndDates".into())),
            ]
        );
        assert!(found.iter().all(|w| !matches!(w.2, UnsupportedDateFormat)));
        let (chart, warnings) = gantt(
            "gantt\n excludes weekends monday tuesday wednesday thursday friday\n A :2026-10-19, 2d\n",
        );
        assert_eq!(warnings[0].kind, ExcludesEverything);
        assert_eq!(
            chart.tasks[0].end,
            NaiveDate::from_ymd_opt(2026, 10, 21).unwrap()
        );
    }

    #[test]
    fn statements_split_only_outside_quotes_brackets_and_link_labels() {
        assert_eq!(statements(" A --> B ;; C ; "), ["A --> B", "C"]);
        assert_eq!(
            statements(r#"A["x; y"] --> B[p; q]; C(r;s) -->|a;b| D{t;u}; E"#),
            [r#"A["x; y"] --> B[p; q]"#, "C(r;s) -->|a;b| D{t;u}", "E"]
        );
        assert_eq!(
            statements("A -- a#59;b --> B; C"),
            ["A -- a#59;b --> B", "C"]
        );
        assert_eq!(
            statements(r#"A -->|"a|b;"| B;C"#),
            [r#"A -->|"a|b;"| B"#, "C"]
        );
    }

    #[test]
    fn entity_codes_decode_and_unknown_ones_stay() {
        assert_eq!(
            decode_entities("#quot;a#quot; #59; #35;1 #9829;"),
            "\"a\" ; #1 ♥"
        );
        assert_eq!(decode_entities("#nope; #0; # #;"), "#nope; #0; # #;");
    }

    /// Quoting and label cases adapted from Mermaid's flowchart parser specs
    /// (`flow-text.spec.js`, `flow-singlenode.spec.js`; MIT, © Knut Sveidqvist and
    /// Mermaid contributors), plus the statement-splitting cases this parser missed.
    #[test]
    fn flowchart_labels_follow_mermaid_quoting() {
        type Case = (
            &'static str,
            &'static [(&'static str, &'static str)],
            Option<&'static str>,
        );
        let cases: [Case; 18] = [
            ("graph TD;A-->B;", &[("A", "A"), ("B", "B")], None),
            (
                "graph TD;A--x|text including URL space|B;",
                &[("B", "B")],
                Some("text including URL space"),
            ),
            (
                "graph TD;A-- text including URL space and send -->B;",
                &[],
                Some("text including URL space and send"),
            ),
            (
                "graph TD;A-->|text including space|B;",
                &[],
                Some("text including space"),
            ),
            (
                "graph TD;A-->|\"quoted | and ; text\"|B;",
                &[],
                Some("quoted | and ; text"),
            ),
            (
                "graph TD;V-- \"test string()\" -->a[v]",
                &[("a", "v")],
                Some("test string()"),
            ),
            (
                "graph TD;A[chimpansen hoppar]-->C;",
                &[("A", "chimpansen hoppar")],
                None,
            ),
            (
                "graph TD;A(Chimpansen hoppar åäö-ÅÄÖ)-->C;",
                &[("A", "Chimpansen hoppar åäö-ÅÄÖ")],
                None,
            ),
            (
                "graph TD;A(Chimpansen hoppar åäö  <br> -  ÅÄÖ)-->C;",
                &[("A", "Chimpansen hoppar åäö  \n -  ÅÄÖ")],
                None,
            ),
            (
                "graph TD;A{chimpansen hoppar}-->C;",
                &[("A", "chimpansen hoppar")],
                None,
            ),
            (
                "graph TD;A[\"#quot;Hi#quot; #59; there\"]-->B",
                &[("A", "\"Hi\" ; there")],
                None,
            ),
            ("graph TD;A[#9829; love]-->B", &[("A", "♥ love")], None),
            (
                "flowchart LR\nA[hello; world] --> B",
                &[("A", "hello; world")],
                None,
            ),
            (
                "flowchart LR\nA[\"hello; world\"] --> B",
                &[("A", "hello; world")],
                None,
            ),
            (
                "flowchart LR\nA[hello %% world] --> B",
                &[("A", "hello %% world")],
                None,
            ),
            (
                "flowchart LR\nA[\"hello %% world\"] --> B",
                &[("A", "hello %% world")],
                None,
            ),
            ("flowchart LR\nA -->|a;b| B", &[("B", "B")], Some("a;b")),
            (
                "flowchart LR\n  %% a comment line\nA[\"x ] y\"] --> B",
                &[("A", "x ] y")],
                None,
            ),
        ];
        for (source, labels, link) in cases {
            let chart = flow(source);
            for &(id, label) in labels {
                let node = chart.nodes.iter().find(|n| n.id == id).unwrap();
                assert_eq!(node.label, label, "{source}");
            }
            assert_eq!(chart.edges.len(), 1, "{source}");
            assert_eq!(chart.edges[0].label.as_deref(), link, "{source}");
        }
    }

    #[test]
    fn preprocess_reads_front_matter_and_drops_comments() {
        let source = preprocess(
            "\n---\ntitle: \"计划\"\nother: x\n---\n%% note\ngantt\n  %%{init: {}}%%\n  A :2026-10-19, 1d %% kept\n",
        );
        assert_eq!(source.title.as_deref(), Some("计划"));
        // Only lines that start with `%%` are comments, as in Mermaid.
        assert_eq!(
            source.lines,
            [(7, "gantt"), (9, "A :2026-10-19, 1d %% kept")]
        );
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
