//! A dependency-free subset of Mermaid for the diagram widget: flowcharts and Gantt
//! charts, parsed and laid out natively (the visual approach follows Pele, MIT;
//! no code is shared). Everything here is platform-free and measured in DIPs.

use chrono::NaiveDate;
use std::collections::HashMap;

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

/// Parse Mermaid source. Front matter `title:` and `%%` comments are honoured.
pub(crate) fn parse(source: &str) -> Result<Diagram, ParseError> {
    let mut lines = source
        .lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l))
        .peekable();
    let mut title = None;
    // Optional YAML front matter: only `title:` is read.
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
    let body: Vec<(usize, &str)> = lines
        .map(|(n, l)| (n, strip_comment(l).trim()))
        .filter(|(_, l)| !l.is_empty())
        .collect();
    let Some(&(header_line, header)) = body.first() else {
        return Err(error(0, ErrorKind::UnknownType, ""));
    };
    let keyword = header.split_whitespace().next().unwrap_or("");
    match keyword {
        "flowchart" | "graph" => {
            parse_flowchart(header_line, header, &body[1..], title).map(Diagram::Flowchart)
        }
        "gantt" => parse_gantt(&body[1..], title).map(Diagram::Gantt),
        "sequenceDiagram" | "classDiagram" | "stateDiagram" | "stateDiagram-v2" | "erDiagram"
        | "journey" | "pie" | "timeline" | "gitGraph" | "mindmap" | "quadrantChart"
        | "requirementDiagram" | "C4Context" | "sankey-beta" | "xychart-beta" | "block-beta"
        | "packet-beta" | "kanban" | "architecture-beta" => Err(error(
            header_line,
            ErrorKind::UnsupportedType(keyword.into()),
            header,
        )),
        _ => Err(error(header_line, ErrorKind::UnknownType, header)),
    }
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

// ---------------------------------------------------------------- flowchart

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Direction {
    LeftRight,
    RightLeft,
    TopBottom,
    BottomTop,
}

impl Direction {
    pub(crate) fn horizontal(self) -> bool {
        matches!(self, Direction::LeftRight | Direction::RightLeft)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shape {
    Rect,
    Round,
    Stadium,
    Circle,
    Diamond,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stroke {
    Solid,
    Dotted,
    Thick,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Node {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) shape: Shape,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Edge {
    pub(crate) from: usize,
    pub(crate) to: usize,
    pub(crate) label: Option<String>,
    pub(crate) stroke: Stroke,
    pub(crate) arrow: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Flowchart {
    pub(crate) title: Option<String>,
    pub(crate) direction: Direction,
    pub(crate) nodes: Vec<Node>,
    pub(crate) edges: Vec<Edge>,
}

struct FlowBuilder {
    nodes: Vec<Node>,
    index: HashMap<String, usize>,
    edges: Vec<Edge>,
}

impl FlowBuilder {
    fn node(&mut self, id: &str, label: Option<(String, Shape)>) -> Result<usize, ErrorKind> {
        if let Some(&index) = self.index.get(id) {
            if let Some((label, shape)) = label {
                self.nodes[index].label = label;
                self.nodes[index].shape = shape;
            }
            return Ok(index);
        }
        if self.nodes.len() >= MAX_NODES {
            return Err(ErrorKind::TooLarge);
        }
        let (label, shape) = label.unwrap_or_else(|| (id.to_string(), Shape::Rect));
        self.index.insert(id.to_string(), self.nodes.len());
        self.nodes.push(Node {
            id: id.to_string(),
            label,
            shape,
        });
        Ok(self.nodes.len() - 1)
    }
}

const IGNORED_STATEMENTS: [&str; 7] = [
    "style ",
    "classDef ",
    "class ",
    "linkStyle ",
    "click ",
    "subgraph",
    "direction ",
];

fn parse_flowchart(
    header_line: usize,
    header: &str,
    body: &[(usize, &str)],
    title: Option<String>,
) -> Result<Flowchart, ParseError> {
    let direction = match header.split_whitespace().nth(1).unwrap_or("TB") {
        "LR" => Direction::LeftRight,
        "RL" => Direction::RightLeft,
        "BT" => Direction::BottomTop,
        "TB" | "TD" => Direction::TopBottom,
        _ => return Err(error(header_line, ErrorKind::UnexpectedText, header)),
    };
    let mut builder = FlowBuilder {
        nodes: Vec::new(),
        index: HashMap::new(),
        edges: Vec::new(),
    };
    for &(number, line) in body {
        for statement in line.split(';').map(str::trim).filter(|s| !s.is_empty()) {
            if statement == "end" || IGNORED_STATEMENTS.iter().any(|p| statement.starts_with(p)) {
                continue;
            }
            parse_statement(&mut builder, statement).map_err(|kind| error(number, kind, line))?;
        }
    }
    Ok(Flowchart {
        title,
        direction,
        nodes: builder.nodes,
        edges: builder.edges,
    })
}

/// `A & B --> C -- text --> D` : node groups joined by links.
fn parse_statement(builder: &mut FlowBuilder, statement: &str) -> Result<(), ErrorKind> {
    let mut cursor = Cursor {
        text: statement,
        at: 0,
    };
    let mut previous = parse_group(builder, &mut cursor)?;
    loop {
        cursor.skip_spaces();
        if cursor.done() {
            return Ok(());
        }
        let link = parse_link(&mut cursor)?;
        cursor.skip_spaces();
        if cursor.done() {
            return Err(ErrorKind::MissingTarget);
        }
        let next = parse_group(builder, &mut cursor)?;
        for &from in &previous {
            for &to in &next {
                if builder.edges.len() >= MAX_EDGES {
                    return Err(ErrorKind::TooLarge);
                }
                builder.edges.push(Edge {
                    from,
                    to,
                    label: link.label.clone(),
                    stroke: link.stroke,
                    arrow: link.arrow,
                });
            }
        }
        previous = next;
    }
}

struct Cursor<'a> {
    text: &'a str,
    at: usize,
}

impl Cursor<'_> {
    fn rest(&self) -> &str {
        &self.text[self.at..]
    }
    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }
    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.at += c.len_utf8();
        Some(c)
    }
    fn done(&self) -> bool {
        self.at >= self.text.len()
    }
    fn skip_spaces(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }
    fn eat(&mut self, token: &str) -> bool {
        if self.rest().starts_with(token) {
            self.at += token.len();
            true
        } else {
            false
        }
    }
}

fn id_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || (!c.is_ascii() && !c.is_whitespace())
}

fn parse_group(builder: &mut FlowBuilder, cursor: &mut Cursor) -> Result<Vec<usize>, ErrorKind> {
    let mut group = vec![parse_node(builder, cursor)?];
    loop {
        let save = cursor.at;
        cursor.skip_spaces();
        if cursor.eat("&") {
            cursor.skip_spaces();
            group.push(parse_node(builder, cursor)?);
        } else {
            cursor.at = save;
            return Ok(group);
        }
    }
}

fn parse_node(builder: &mut FlowBuilder, cursor: &mut Cursor) -> Result<usize, ErrorKind> {
    let start = cursor.at;
    while cursor.peek().is_some_and(id_char) {
        cursor.bump();
    }
    let id = &cursor.text[start..cursor.at];
    if id.is_empty() {
        return Err(if cursor.done() {
            ErrorKind::MissingTarget
        } else {
            ErrorKind::UnexpectedText
        });
    }
    // Longest openers first so `((` is not read as `(`.
    const SHAPES: [(&str, &str, Shape); 10] = [
        ("(((", ")))", Shape::Circle),
        ("((", "))", Shape::Circle),
        ("([", "])", Shape::Stadium),
        ("[[", "]]", Shape::Rect),
        ("[(", ")]", Shape::Rect),
        ("{{", "}}", Shape::Diamond),
        ("[", "]", Shape::Rect),
        ("(", ")", Shape::Round),
        ("{", "}", Shape::Diamond),
        (">", "]", Shape::Rect),
    ];
    let mut label = None;
    for (open, close, shape) in SHAPES {
        if cursor.eat(open) {
            let rest = cursor.rest();
            let end = if let Some(quoted) = rest.strip_prefix('"') {
                quoted
                    .find('"')
                    .map(|q| q + 2)
                    .filter(|&q| rest[q..].starts_with(close))
            } else {
                rest.find(close)
            }
            .ok_or(ErrorKind::UnclosedLabel)?;
            let text = clean_label(&rest[..end]);
            cursor.at += end + close.len();
            label = Some((text, shape));
            break;
        }
    }
    // `:::class` suffixes only style the node.
    if cursor.eat(":::") {
        while cursor.peek().is_some_and(id_char) {
            cursor.bump();
        }
    }
    builder.node(id, label)
}

fn clean_label(raw: &str) -> String {
    let text = unquote(raw.trim())
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<br>", "\n");
    text.chars().take(MAX_LABEL_CHARS).collect()
}

struct Link {
    label: Option<String>,
    stroke: Stroke,
    arrow: bool,
}

fn parse_link(cursor: &mut Cursor) -> Result<Link, ErrorKind> {
    let bidirectional = cursor.eat("<");
    let start = cursor.at;
    while cursor.peek().is_some_and(|c| matches!(c, '-' | '=' | '.')) {
        cursor.bump();
    }
    let run = &cursor.text[start..cursor.at];
    if run.len() < 2 && !(run == "-" && cursor.peek() == Some('.')) {
        return Err(ErrorKind::UnexpectedText);
    }
    let stroke = |run: &str| {
        if run.contains('.') {
            Stroke::Dotted
        } else if run.contains('=') {
            Stroke::Thick
        } else {
            Stroke::Solid
        }
    };
    let head = |cursor: &mut Cursor| -> bool {
        if cursor.eat(">") {
            return true;
        }
        // `--x` / `--o` heads; draw them as plain arrows.
        let rest = cursor.rest();
        if (rest.starts_with('x') || rest.starts_with('o'))
            && rest[1..]
                .chars()
                .next()
                .is_none_or(|c| c.is_whitespace() || c == '|')
        {
            cursor.bump();
            return true;
        }
        false
    };
    let mut arrow = head(cursor);
    let mut label = None;
    if !arrow && cursor.peek().is_some_and(char::is_whitespace) && matches!(run, "--" | "==" | "-.")
    {
        // `A -- text --> B`: the label runs up to the closing link.
        let rest = cursor.rest();
        let closers = ["-->", "---", ".->", ".-", "==>", "==="];
        let (index, closer) = closers
            .iter()
            .filter_map(|c| rest.find(c).map(|i| (i, *c)))
            .min_by_key(|(i, _)| *i)
            .ok_or(ErrorKind::MissingTarget)?;
        label = Some(clean_label(&rest[..index]));
        cursor.at += index + closer.len();
        while cursor.peek().is_some_and(|c| matches!(c, '-' | '=' | '.')) {
            cursor.bump();
        }
        arrow = closer.ends_with('>') || head(cursor);
    }
    cursor.skip_spaces();
    if cursor.eat("|") {
        let rest = cursor.rest();
        let end = rest.find('|').ok_or(ErrorKind::UnclosedLabel)?;
        label = Some(clean_label(&rest[..end]));
        cursor.at += end + 1;
    }
    Ok(Link {
        label: label.filter(|l| !l.is_empty()),
        stroke: stroke(run),
        arrow: arrow || bidirectional,
    })
}

// ---------------------------------------------------------------- gantt

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Task {
    pub(crate) name: String,
    pub(crate) section: Option<String>,
    pub(crate) start: NaiveDate,
    /// Exclusive end; equal to `start` for milestones.
    pub(crate) end: NaiveDate,
    pub(crate) done: bool,
    pub(crate) active: bool,
    pub(crate) critical: bool,
    pub(crate) milestone: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Gantt {
    pub(crate) title: Option<String>,
    pub(crate) tasks: Vec<Task>,
}

impl Gantt {
    pub(crate) fn span(&self) -> Option<(NaiveDate, NaiveDate)> {
        let start = self.tasks.iter().map(|t| t.start).min()?;
        let end = self.tasks.iter().map(|t| t.end.max(t.start)).max()?;
        Some((start, end.max(start.succ_opt().unwrap_or(start))))
    }
}

fn parse_date(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d").ok()
}

fn parse_duration(text: &str) -> Option<i64> {
    let text = text.trim();
    let (number, multiplier) = if let Some(number) = text.strip_suffix('d') {
        (number, 1.0)
    } else {
        (text.strip_suffix('w')?, 7.0)
    };
    let number: f64 = number.trim().parse().ok()?;
    let days = number * multiplier;
    (days.is_finite() && (0.0..=3650.0).contains(&days)).then(|| days.ceil() as i64)
}

fn parse_gantt(body: &[(usize, &str)], mut title: Option<String>) -> Result<Gantt, ParseError> {
    let mut section = None;
    let mut tasks: Vec<Task> = Vec::new();
    let mut ids: HashMap<String, usize> = HashMap::new();
    let mut default_start: Option<NaiveDate> = None;
    for &(number, line) in body {
        let fail = |kind| error(number, kind, line);
        if let Some(value) = line.strip_prefix("title ") {
            title = Some(value.trim().to_string());
            continue;
        }
        if let Some(value) = line.strip_prefix("dateFormat ") {
            if value.trim() != "YYYY-MM-DD" {
                return Err(fail(ErrorKind::UnsupportedDateFormat));
            }
            continue;
        }
        if let Some(value) = line.strip_prefix("section ") {
            section = Some(value.trim().to_string());
            continue;
        }
        if [
            "axisFormat",
            "excludes",
            "todayMarker",
            "tickInterval",
            "weekday",
            "includes",
            "inclusiveEndDates",
            "topAxis",
            "displayMode",
        ]
        .iter()
        .any(|k| line.starts_with(k))
        {
            continue;
        }
        let Some((name, meta)) = line.split_once(':') else {
            return Err(fail(ErrorKind::UnexpectedText));
        };
        let name = name.trim();
        if name.is_empty() {
            return Err(fail(ErrorKind::EmptyLabel));
        }
        if tasks.len() >= MAX_TASKS {
            return Err(fail(ErrorKind::TooLarge));
        }
        let mut parts: Vec<&str> = meta.split(',').map(str::trim).collect();
        let mut task = Task {
            name: name.chars().take(MAX_LABEL_CHARS).collect(),
            section: section.clone(),
            start: NaiveDate::MIN,
            end: NaiveDate::MIN,
            done: false,
            active: false,
            critical: false,
            milestone: false,
        };
        while let Some(&tag) = parts.first() {
            match tag {
                "done" => task.done = true,
                "active" => task.active = true,
                "crit" => task.critical = true,
                "milestone" => task.milestone = true,
                _ => break,
            }
            parts.remove(0);
        }
        // One part: duration after the previous task; two: start, end;
        // three: id, start, end (Mermaid's own reading).
        let (id, start, end) = match parts.as_slice() {
            [end] => (None, None, *end),
            [start, end] => (None, Some(*start), *end),
            [id, start, end] => (Some(*id), Some(*start), *end),
            _ => return Err(fail(ErrorKind::UnexpectedText)),
        };
        let start = match start {
            Some(text) if text.starts_with("after ") => {
                let mut latest = None;
                for other in text["after ".len()..].split_whitespace() {
                    let index = *ids
                        .get(other)
                        .ok_or_else(|| fail(ErrorKind::UnknownTask(other.into())))?;
                    latest = latest.max(Some(tasks[index].end));
                }
                latest.ok_or_else(|| fail(ErrorKind::BadDate))?
            }
            Some(text) => parse_date(text).ok_or_else(|| fail(ErrorKind::BadDate))?,
            None => tasks
                .last()
                .map(|t| t.end)
                .or(default_start)
                .ok_or_else(|| fail(ErrorKind::BadDate))?,
        };
        let end_date = if let Some(date) = parse_date(end) {
            date
        } else {
            let days = parse_duration(end).ok_or_else(|| fail(ErrorKind::BadDuration))?;
            start
                .checked_add_signed(chrono::Duration::days(days))
                .ok_or_else(|| fail(ErrorKind::BadDuration))?
        };
        task.start = start;
        task.end = if task.milestone {
            start
        } else {
            end_date.max(start)
        };
        default_start.get_or_insert(start);
        if let Some(id) = id {
            ids.insert(id.to_string(), tasks.len());
        }
        tasks.push(task);
    }
    Ok(Gantt { title, tasks })
}

// ---------------------------------------------------------------- layout

/// Rough advance of `text` at `size` DIPs in Segoe UI / YaHei: full-width
/// characters take one em, Latin about 0.56 em. Labels are clipped, never measured
/// again at paint time, so this only needs to be a good upper bound.
pub(crate) fn text_width(text: &str, size: f32) -> f32 {
    text.lines()
        .map(|line| {
            line.chars()
                .map(|c| {
                    if c.is_ascii() {
                        if c.is_ascii_uppercase() { 0.64 } else { 0.56 }
                    } else {
                        1.0
                    }
                })
                .sum::<f32>()
                * size
        })
        .fold(0.0, f32::max)
}

pub(crate) const LABEL_SIZE: f32 = 13.0;
const LINE_HEIGHT: f32 = 18.0;
const RANK_GAP: f32 = 56.0;
const NODE_GAP: f32 = 20.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Box2 {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) w: f32,
    pub(crate) h: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlacedEdge {
    pub(crate) points: Vec<(f32, f32)>,
    pub(crate) label: Option<(String, f32, f32)>,
    pub(crate) stroke: Stroke,
    pub(crate) arrow: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FlowLayout {
    /// One box per node, in `Flowchart::nodes` order.
    pub(crate) boxes: Vec<Box2>,
    pub(crate) edges: Vec<PlacedEdge>,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

fn node_size(node: &Node) -> (f32, f32) {
    let lines = node.label.lines().count().max(1) as f32;
    let w = (text_width(&node.label, LABEL_SIZE) + 28.0).clamp(64.0, 220.0);
    let h = 36.0 + (lines - 1.0) * LINE_HEIGHT;
    match node.shape {
        Shape::Diamond => (w + 28.0, h + 20.0),
        Shape::Circle => {
            let d = w.max(h);
            (d, d)
        }
        _ => (w, h),
    }
}

/// Layered (Sugiyama-style) layout: longest-path ranks, barycenter ordering, ranks
/// centred on a shared axis so parallel branches sit symmetrically, and orthogonal
/// links that turn in the gap between ranks.
pub(crate) fn layout_flowchart(chart: &Flowchart) -> FlowLayout {
    let n = chart.nodes.len();
    let sizes: Vec<(f32, f32)> = chart.nodes.iter().map(node_size).collect();
    // Break cycles: edges that close a DFS back-path do not constrain ranks.
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); n];
    for edge in &chart.edges {
        if edge.from != edge.to {
            out[edge.from].push(edge.to);
        }
    }
    let mut forward = vec![Vec::new(); n];
    let mut state = vec![0u8; n]; // 0 new, 1 on stack, 2 done
    for root in 0..n {
        if state[root] != 0 {
            continue;
        }
        let mut stack = vec![(root, 0usize)];
        state[root] = 1;
        while let Some((v, i)) = stack.pop() {
            if i < out[v].len() {
                stack.push((v, i + 1));
                let w = out[v][i];
                match state[w] {
                    0 => {
                        forward[v].push(w);
                        state[w] = 1;
                        stack.push((w, 0));
                    }
                    2 => forward[v].push(w),
                    _ => {} // back edge
                }
            } else {
                state[v] = 2;
            }
        }
    }
    // Longest path ranks in topological order of the forward graph.
    let mut indegree = vec![0usize; n];
    for targets in &forward {
        for &w in targets {
            indegree[w] += 1;
        }
    }
    let mut queue: Vec<usize> = (0..n).filter(|&v| indegree[v] == 0).collect();
    let mut rank = vec![0usize; n];
    let mut head = 0;
    while head < queue.len() {
        let v = queue[head];
        head += 1;
        for &w in &forward[v] {
            rank[w] = rank[w].max(rank[v] + 1);
            indegree[w] -= 1;
            if indegree[w] == 0 {
                queue.push(w);
            }
        }
    }
    let ranks = rank.iter().copied().max().map_or(0, |r| r + 1);
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); ranks];
    for v in 0..n {
        layers[rank[v]].push(v);
    }
    // Barycenter sweeps against the neighbouring layer.
    let mut neighbours: Vec<Vec<usize>> = vec![Vec::new(); n];
    for edge in &chart.edges {
        if edge.from != edge.to {
            neighbours[edge.from].push(edge.to);
            neighbours[edge.to].push(edge.from);
        }
    }
    let mut position = vec![0f32; n];
    let reindex = |layers: &Vec<Vec<usize>>, position: &mut Vec<f32>| {
        for layer in layers {
            for (i, &v) in layer.iter().enumerate() {
                position[v] = i as f32;
            }
        }
    };
    reindex(&layers, &mut position);
    for sweep in 0..6 {
        let order: Vec<usize> = if sweep % 2 == 0 {
            (1..ranks).collect()
        } else {
            (0..ranks.saturating_sub(1)).rev().collect()
        };
        for r in order {
            let adjacent = if sweep % 2 == 0 { r - 1 } else { r + 1 };
            let mut keyed: Vec<(f32, usize)> = layers[r]
                .iter()
                .map(|&v| {
                    let near: Vec<f32> = neighbours[v]
                        .iter()
                        .filter(|&&w| rank[w] == adjacent)
                        .map(|&w| position[w])
                        .collect();
                    let key = if near.is_empty() {
                        position[v]
                    } else {
                        near.iter().sum::<f32>() / near.len() as f32
                    };
                    (key, v)
                })
                .collect();
            keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
            layers[r] = keyed.into_iter().map(|(_, v)| v).collect();
            reindex(&layers, &mut position);
        }
    }
    // Coordinates: `main` runs along ranks, `cross` across them.
    let horizontal = chart.direction.horizontal();
    let main_size = |v: usize| if horizontal { sizes[v].0 } else { sizes[v].1 };
    let cross_size = |v: usize| if horizontal { sizes[v].1 } else { sizes[v].0 };
    let rank_extent: Vec<f32> = layers
        .iter()
        .map(|layer| layer.iter().map(|&v| main_size(v)).fold(0.0, f32::max))
        .collect();
    let mut main_start = vec![0f32; ranks];
    let mut cursor = 0.0;
    for r in 0..ranks {
        main_start[r] = cursor;
        cursor += rank_extent[r] + RANK_GAP;
    }
    let main_total = (cursor - RANK_GAP).max(0.0);
    let mut boxes = vec![
        Box2 {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        };
        n
    ];
    let mut cross_min = 0f32;
    let mut cross_max = 0f32;
    for (r, layer) in layers.iter().enumerate() {
        let total: f32 = layer.iter().map(|&v| cross_size(v)).sum::<f32>()
            + NODE_GAP * layer.len().saturating_sub(1) as f32;
        let mut along = -total / 2.0;
        for &v in layer {
            let main = main_start[r] + (rank_extent[r] - main_size(v)) / 2.0;
            let (x, y) = if horizontal {
                (main, along)
            } else {
                (along, main)
            };
            boxes[v] = Box2 {
                x,
                y,
                w: sizes[v].0,
                h: sizes[v].1,
            };
            along += cross_size(v) + NODE_GAP;
        }
        cross_min = cross_min.min(-total / 2.0);
        cross_max = cross_max.max(total / 2.0);
    }
    // Mirror for right-to-left and bottom-to-top.
    for b in &mut boxes {
        match chart.direction {
            Direction::RightLeft => b.x = main_total - b.x - b.w,
            Direction::BottomTop => b.y = main_total - b.y - b.h,
            _ => {}
        }
    }
    // Leave a lane below (or right of) everything for links that run backwards.
    let has_back = chart
        .edges
        .iter()
        .any(|e| e.from != e.to && rank[e.to] <= rank[e.from]);
    let lane = if has_back { 24.0 } else { 0.0 };
    let (shift_x, shift_y) = if horizontal {
        (0.0, -cross_min)
    } else {
        (-cross_min, 0.0)
    };
    for b in &mut boxes {
        b.x += shift_x;
        b.y += shift_y;
    }
    let cross_extent = cross_max - cross_min;
    let (width, height) = if horizontal {
        (main_total, cross_extent + lane)
    } else {
        (cross_extent + lane, main_total)
    };
    let back_lane = cross_extent + lane / 2.0;
    let edges = chart
        .edges
        .iter()
        .map(|edge| {
            route(
                chart.direction,
                &boxes,
                edge,
                back_lane,
                rank[edge.to] <= rank[edge.from],
            )
        })
        .collect();
    FlowLayout {
        boxes,
        edges,
        width,
        height,
    }
}

fn route(
    direction: Direction,
    boxes: &[Box2],
    edge: &Edge,
    lane: f32,
    backwards: bool,
) -> PlacedEdge {
    let a = boxes[edge.from];
    let b = boxes[edge.to];
    let horizontal = direction.horizontal();
    let flip = matches!(direction, Direction::RightLeft | Direction::BottomTop);
    // Work in a frame where ranks grow along +main, then map back.
    let to_frame = |(x, y): (f32, f32)| if horizontal { (x, y) } else { (y, x) };
    let ext = |bx: &Box2| {
        let (m, c) = to_frame((bx.x, bx.y));
        let (mw, cw) = to_frame((bx.w, bx.h));
        (m, c, mw, cw)
    };
    let (am, ac, amw, acw) = ext(&a);
    let (bm, bc, bmw, bcw) = ext(&b);
    let (a_out, b_in) = if flip { (am, bm + bmw) } else { (am + amw, bm) };
    let step = if flip { -1.0 } else { 1.0 };
    let (ay, by) = (ac + acw / 2.0, bc + bcw / 2.0);
    let points: Vec<(f32, f32)> = if edge.from == edge.to {
        let top = ac;
        vec![
            (a_out, ay),
            (a_out + step * 14.0, ay),
            (a_out + step * 14.0, top - 12.0),
            (am + amw / 2.0, top - 12.0),
            (am + amw / 2.0, top),
        ]
    } else if backwards {
        let out = a_out + step * RANK_GAP / 3.0;
        let into = b_in - step * RANK_GAP / 3.0;
        vec![
            (a_out, ay),
            (out, ay),
            (out, lane),
            (into, lane),
            (into, by),
            (b_in, by),
        ]
    } else if (ay - by).abs() < 0.5 {
        vec![(a_out, ay), (b_in, by)]
    } else {
        let turn = b_in - step * RANK_GAP / 2.0;
        vec![(a_out, ay), (turn, ay), (turn, by), (b_in, by)]
    };
    let points: Vec<(f32, f32)> = points
        .into_iter()
        .map(|(m, c)| if horizontal { (m, c) } else { (c, m) })
        .collect();
    let label = edge.label.as_ref().map(|text| {
        // Centre the label on the longest segment.
        let (p, q) = points
            .windows(2)
            .map(|w| (w[0], w[1]))
            .max_by(|x, y| {
                let len = |(p, q): &((f32, f32), (f32, f32))| (q.0 - p.0).abs() + (q.1 - p.1).abs();
                len(x).total_cmp(&len(y))
            })
            .unwrap_or((points[0], points[0]));
        (text.clone(), (p.0 + q.0) / 2.0, (p.1 + q.1) / 2.0)
    });
    PlacedEdge {
        points,
        label,
        stroke: edge.stroke,
        arrow: edge.arrow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn parallel_branches_share_a_rank_and_sit_symmetrically() {
        let chart = flow("flowchart LR\nA --> S & V\nS & V --> I\nI --> X");
        let layout = layout_flowchart(&chart);
        let [a, s, v, i, x] = [0, 1, 2, 3, 4].map(|k| layout.boxes[k]);
        assert_eq!(s.x, v.x);
        assert!(a.x + a.w < s.x && s.x + s.w < i.x && i.x + i.w < x.x);
        let mid = |b: Box2| b.y + b.h / 2.0;
        assert!((mid(a) - (mid(s) + mid(v)) / 2.0).abs() < 0.5);
        assert!((mid(a) - mid(i)).abs() < 0.5);
        // Links leave the right edge and arrive at the left edge, turning in the gap.
        let first = &layout.edges[0];
        assert_eq!(first.points[0], (a.x + a.w, mid(a)));
        assert_eq!(*first.points.last().unwrap(), (s.x, mid(s)));
        assert!(layout.width > 0.0 && layout.height > 0.0);
    }

    #[test]
    fn cycles_and_top_down_layouts_stay_finite() {
        let chart = flow("graph TD\nA --> B --> C --> A\nC --> C");
        let layout = layout_flowchart(&chart);
        assert!(layout.boxes[0].y < layout.boxes[1].y);
        for edge in &layout.edges {
            assert!(
                edge.points
                    .iter()
                    .all(|p| p.0.is_finite() && p.1.is_finite())
            );
        }
        let back = &layout.edges[2];
        assert!(back.points.len() > 2);
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
}
