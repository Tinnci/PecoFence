//! Flowcharts: a parser for Mermaid's `flowchart` / `graph` statements and a layered
//! (Sugiyama-style) layout. The layout ideas follow Pele (MIT): virtual vertices so
//! long links and link labels own a slot in the rank structure. No code is shared.

use super::{
    Box2, Diagnostic, Diagram, ErrorKind, MAX_EDGES, MAX_LABEL_CHARS, MAX_NODES, Severity, Source,
    TextMeasure, decode_entities, diagnostic, error, statements, text_width, unquote, wrap,
};
use std::collections::HashMap;

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
    /// Innermost `subgraph` the node was first mentioned in.
    pub(crate) cluster: Option<usize>,
}

/// A `subgraph … end` block; parents come before their children.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Cluster {
    pub(crate) title: String,
    pub(crate) parent: Option<usize>,
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
    pub(crate) clusters: Vec<Cluster>,
}

#[derive(Default)]
struct FlowBuilder {
    nodes: Vec<Node>,
    index: HashMap<String, usize>,
    edges: Vec<Edge>,
    clusters: Vec<Cluster>,
    open: Vec<usize>,
}

impl FlowBuilder {
    fn node(&mut self, id: &str, label: Option<(String, Shape)>) -> Result<usize, ErrorKind> {
        let cluster = self.open.last().copied();
        if let Some(&index) = self.index.get(id) {
            let node = &mut self.nodes[index];
            node.cluster = node.cluster.or(cluster);
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
            cluster,
        });
        Ok(self.nodes.len() - 1)
    }

    /// `subgraph id[Title]`, `subgraph "Title"` or `subgraph Title`.
    fn subgraph(&mut self, header: &str) -> Result<(), ErrorKind> {
        if self.clusters.len() >= MAX_NODES {
            return Err(ErrorKind::TooLarge);
        }
        let title = match header.find('[') {
            Some(open) if header.ends_with(']') => &header[open + 1..header.len() - 1],
            _ => header,
        };
        self.open.push(self.clusters.len());
        self.clusters.push(Cluster {
            title: clean_label(title),
            parent: self.open.iter().rev().nth(1).copied(),
        });
        Ok(())
    }
}

const IGNORED_STATEMENTS: [&str; 6] = [
    "style ",
    "classDef ",
    "class ",
    "linkStyle ",
    "click ",
    "direction ",
];

pub(super) fn parse(
    source: &Source,
    warnings: &mut Vec<Diagnostic>,
) -> Result<Diagram, Diagnostic> {
    let (header_line, line) = source.lines[0];
    // `graph TD; A --> B` may carry statements after the header.
    let header = statements(line).first().copied().unwrap_or(line);
    let direction = match header.split_whitespace().nth(1).unwrap_or("TB") {
        "LR" => Direction::LeftRight,
        "RL" => Direction::RightLeft,
        "BT" => Direction::BottomTop,
        "TB" | "TD" => Direction::TopBottom,
        _ => return Err(error(header_line, ErrorKind::UnexpectedText, header)),
    };
    let mut builder = FlowBuilder::default();
    for (k, &(number, line)) in source.lines.iter().enumerate() {
        for statement in statements(line).into_iter().skip(usize::from(k == 0)) {
            let result = if statement == "end" {
                builder.open.pop();
                Ok(())
            } else if let Some(header) = statement
                .strip_prefix("subgraph")
                .filter(|h| h.is_empty() || h.starts_with(char::is_whitespace))
            {
                builder.subgraph(header.trim())
            } else if let Some(p) = IGNORED_STATEMENTS
                .iter()
                .find(|p| statement.starts_with(*p))
            {
                let kind = ErrorKind::IgnoredStyle(p.trim().into());
                warnings.push(diagnostic(number, kind, line, Severity::Info));
                Ok(())
            } else {
                parse_statement(&mut builder, statement)
            };
            result.map_err(|kind| error(number, kind, line))?;
        }
    }
    Ok(Diagram::Flowchart(Flowchart {
        title: source.title.clone(),
        direction,
        nodes: builder.nodes,
        edges: builder.edges,
        clusters: builder.clusters,
    }))
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
            let end = label_end(rest, close).ok_or(ErrorKind::UnclosedLabel)?;
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

/// Where a label that `close` ends stops: after the closing quote when it is quoted,
/// so the delimiter may appear inside the quotes.
fn label_end(rest: &str, close: &str) -> Option<usize> {
    match rest.strip_prefix('"') {
        Some(quoted) => quoted
            .find('"')
            .map(|q| q + 2)
            .filter(|&q| rest[q..].trim_start().starts_with(close))
            .map(|q| q + rest[q..].len() - rest[q..].trim_start().len()),
        None => rest.find(close),
    }
}

fn clean_label(raw: &str) -> String {
    let text = unquote(raw.trim())
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<br>", "\n");
    decode_entities(&text)
        .chars()
        .take(MAX_LABEL_CHARS)
        .collect()
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
        let end = label_end(rest, "|").ok_or(ErrorKind::UnclosedLabel)?;
        label = Some(clean_label(&rest[..end]));
        cursor.at += end + 1;
    }
    Ok(Link {
        label: label.filter(|l| !l.is_empty()),
        stroke: stroke(run),
        arrow: arrow || bidirectional,
    })
}

// ---------------------------------------------------------------- layout

pub(crate) const LABEL_SIZE: f32 = 13.0;
/// Link labels draw at the caption size.
pub(crate) const EDGE_LABEL_SIZE: f32 = 12.0;
const LINE_HEIGHT: f32 = 18.0;
const RANK_GAP: f32 = 56.0;
const NODE_GAP: f32 = 20.0;
const NODE_MAX_W: f32 = 220.0;
const EDGE_LABEL_MAX_W: f32 = 160.0;
/// Subgraph border padding per nesting level, and the title strip above members.
const CLUSTER_PAD: f32 = 8.0;
/// Least room per link end along a node side, and between turn tracks in a gap.
const PORT_GAP: f32 = 8.0;
const TRACK_GAP: f32 = 8.0;
pub(crate) const CLUSTER_TITLE: f32 = 18.0;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlacedEdge {
    pub(crate) points: Vec<(f32, f32)>,
    /// Wrapped text and the box reserved for it.
    pub(crate) label: Option<(String, Box2)>,
    pub(crate) stroke: Stroke,
    pub(crate) arrow: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FlowLayout {
    /// One box and one wrapped label per node, in `Flowchart::nodes` order.
    pub(crate) boxes: Vec<Box2>,
    pub(crate) labels: Vec<String>,
    pub(crate) edges: Vec<PlacedEdge>,
    /// One border per subgraph; `None` only for a subgraph without nodes.
    pub(crate) clusters: Vec<Option<Box2>>,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

/// Reorders one gap's turns `(from, to, link, step)` so a link leaving cross `c` turns
/// before any link arriving near `c`; otherwise the two would run along each other
/// between their tracks. Kahn's algorithm keeps the given (non-crossing) order where
/// it can; two links that swap crosses form a cycle and keep it as is.
fn order_turns(list: &mut Vec<(f32, f32, usize, usize)>) {
    let n = list.len();
    let mut after: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut waiting = vec![0usize; n];
    for a in 0..n {
        for b in 0..n {
            if a != b && (list[a].0 - list[b].1).abs() < TRACK_GAP {
                after[a].push(b);
                waiting[b] += 1;
            }
        }
    }
    let mut done = vec![false; n];
    let mut out = Vec::with_capacity(n);
    while out.len() < n {
        let next = (0..n)
            .find(|&i| !done[i] && waiting[i] == 0)
            .or_else(|| (0..n).find(|&i| !done[i]))
            .unwrap();
        done[next] = true;
        out.push(list[next]);
        for &b in &after[next] {
            waiting[b] = waiting[b].saturating_sub(1);
        }
    }
    *list = out;
}

/// Lexicographic order of (subgraph key, kind, id) paths; see `layout_flowchart`.
fn by_path(a: &[(f32, u8, usize)], b: &[(f32, u8, usize)]) -> std::cmp::Ordering {
    a.iter()
        .zip(b)
        .map(|(x, y)| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)).then(x.2.cmp(&y.2)))
        .find(|o| o.is_ne())
        .unwrap_or_else(|| a.len().cmp(&b.len()))
}

/// One container's layout in its own cross coordinates: its child subgraphs' left
/// edges, its own vertices' left edges, and the cross extent of both.
struct Packed {
    groups: Vec<(usize, f32)>,
    vertices: Vec<(usize, f32)>,
    lo: f32,
    hi: f32,
}

fn node_size(measure: &dyn TextMeasure, label: &str, shape: Shape) -> (f32, f32) {
    let lines = label.lines().count().max(1) as f32;
    let w = (text_width(measure, label, LABEL_SIZE) + 28.0).clamp(64.0, NODE_MAX_W);
    let h = 36.0 + (lines - 1.0) * LINE_HEIGHT;
    match shape {
        Shape::Diamond => (w + 28.0, h + 20.0),
        Shape::Circle => {
            let d = w.max(h);
            (d, d)
        }
        _ => (w, h),
    }
}

fn label_size(measure: &dyn TextMeasure, label: &str) -> (f32, f32) {
    let lines = label.lines().count().max(1) as f32;
    (
        text_width(measure, label, EDGE_LABEL_SIZE) + 10.0,
        EDGE_LABEL_SIZE * (0.25 + 1.25 * lines),
    )
}

/// Longest-path ranks; links that close a DFS back-path do not constrain them.
fn rank_nodes(chart: &Flowchart) -> Vec<usize> {
    let n = chart.nodes.len();
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
    rank
}

/// Layered layout. Node ranks sit on even slots and link labels on odd slots between
/// them; a link gets a virtual vertex on every occupied slot it crosses (its label
/// box on one of them), so long links and labels have reserved room and never cross
/// a node. Barycenter sweeps order each layer, layers are centred on a shared axis so
/// parallel branches sit symmetrically, and links run orthogonally, turning only in
/// the empty gaps between layers. Links against the ranks are routed reversed.
pub(crate) fn layout_flowchart(chart: &Flowchart, measure: &dyn TextMeasure) -> FlowLayout {
    let n = chart.nodes.len();
    let labels: Vec<String> = chart
        .nodes
        .iter()
        .map(|node| wrap(measure, &node.label, LABEL_SIZE, NODE_MAX_W - 28.0))
        .collect();
    let mut sizes: Vec<(f32, f32)> = chart
        .nodes
        .iter()
        .zip(&labels)
        .map(|(node, label)| node_size(measure, label, node.shape))
        .collect();
    let rank = rank_nodes(chart);
    let slots = rank.iter().max().map_or(0, |r| 2 * r + 1);
    let ends = |edge: &Edge| {
        if rank[edge.from] <= rank[edge.to] {
            (edge.from, edge.to)
        } else {
            (edge.to, edge.from)
        }
    };
    // A label sits in the middle gap of its link's span.
    let label_slot = |lo: usize, hi: usize| 2 * lo + 1 + 2 * ((hi - lo - 1) / 2);
    let mut occupied: Vec<bool> = (0..slots).map(|s| s % 2 == 0).collect();
    for edge in chart
        .edges
        .iter()
        .filter(|e| e.label.is_some() && e.from != e.to)
    {
        let (lo, hi) = ends(edge);
        occupied[label_slot(rank[lo], rank[hi])] = true;
    }
    // Vertices: nodes first, then the virtual ones each link adds, low end to high end.
    let mut slot_of: Vec<usize> = rank.iter().map(|r| 2 * r).collect();
    let mut chains: Vec<Vec<usize>> = Vec::with_capacity(chart.edges.len());
    let mut edge_labels: Vec<Option<(usize, String)>> = vec![None; chart.edges.len()];
    for (i, edge) in chart.edges.iter().enumerate() {
        let (lo, hi) = ends(edge);
        let mut chain = vec![lo];
        if lo != hi {
            let target = edge.label.as_ref().map(|_| label_slot(rank[lo], rank[hi]));
            let (from, to) = (slot_of[lo] + 1, slot_of[hi]);
            for slot in (from..to).filter(|&s| occupied[s]) {
                let v = slot_of.len();
                slot_of.push(slot);
                let mut size = (0.0, 0.0);
                if let (Some(text), true) = (&edge.label, target == Some(slot)) {
                    let text = wrap(measure, text, EDGE_LABEL_SIZE, EDGE_LABEL_MAX_W);
                    size = label_size(measure, &text);
                    edge_labels[i] = Some((v, text));
                }
                sizes.push(size);
                chain.push(v);
            }
            chain.push(hi);
        }
        chains.push(chain);
    }
    let vertices = slot_of.len();
    // Compact the occupied slots into layers.
    let mut layer_at = vec![0usize; slots];
    let mut odd: Vec<bool> = Vec::new();
    for slot in (0..slots).filter(|&s| occupied[s]) {
        layer_at[slot] = odd.len();
        odd.push(slot % 2 == 1);
    }
    let layer_of: Vec<usize> = slot_of.iter().map(|&s| layer_at[s]).collect();
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); odd.len()];
    for v in 0..vertices {
        layers[layer_of[v]].push(v);
    }
    // Barycenter sweeps against the neighbouring layer.
    let mut neighbours: Vec<Vec<usize>> = vec![Vec::new(); vertices];
    for chain in &chains {
        for pair in chain.windows(2) {
            neighbours[pair[0]].push(pair[1]);
            neighbours[pair[1]].push(pair[0]);
        }
    }
    let mut position = vec![0f32; vertices];
    let reindex = |layers: &Vec<Vec<usize>>, position: &mut Vec<f32>| {
        for layer in layers {
            for (i, &v) in layer.iter().enumerate() {
                position[v] = i as f32;
            }
        }
    };
    reindex(&layers, &mut position);
    let count = layers.len();
    // Each vertex's chain of subgraphs, outermost first: a node's own, and for the
    // virtual vertices of a link the innermost subgraph holding both of its ends, so a
    // link between subgraphs never routes (or puts its label) inside either of them.
    let path_of = |cluster: Option<usize>| {
        let mut path = Vec::new();
        let mut k = cluster;
        while let Some(c) = k {
            path.push(c);
            k = chart.clusters[c].parent;
        }
        path.reverse();
        path
    };
    let mut paths: Vec<Vec<usize>> = (0..n).map(|v| path_of(chart.nodes[v].cluster)).collect();
    for (edge, chain) in chart.edges.iter().zip(&chains) {
        let (a, b) = (
            path_of(chart.nodes[edge.from].cluster),
            path_of(chart.nodes[edge.to].cluster),
        );
        let common: Vec<usize> = a
            .iter()
            .zip(&b)
            .take_while(|(x, y)| x == y)
            .map(|(x, _)| *x)
            .collect();
        for _ in 1..chain.len().saturating_sub(1) {
            paths.push(common.clone());
        }
    }
    debug_assert_eq!(paths.len(), vertices);
    let clusters = chart.clusters.len();
    // A subgraph's order key is the mean position of everything inside it, the same in
    // every layer: subgraphs keep one order across layers and stay contiguous.
    let group_keys = |position: &[f32]| {
        let mut sum = vec![(0f32, 0f32); clusters];
        for (v, path) in paths.iter().enumerate() {
            for &k in path {
                sum[k] = (sum[k].0 + position[v], sum[k].1 + 1.0);
            }
        }
        sum.into_iter()
            .map(|(s, c)| if c > 0.0 { s / c } else { 0.0 })
            .collect::<Vec<f32>>()
    };
    for sweep in 0..6 {
        let order: Vec<usize> = if sweep % 2 == 0 {
            (1..count).collect()
        } else {
            (0..count.saturating_sub(1)).rev().collect()
        };
        for l in order {
            let adjacent = if sweep % 2 == 0 { l - 1 } else { l + 1 };
            let groups = group_keys(&position);
            // Sort by (subgraph key, …, own barycenter), outermost subgraph first.
            let mut keyed: Vec<(Vec<(f32, u8, usize)>, usize)> = layers[l]
                .iter()
                .map(|&v| {
                    let near: Vec<f32> = neighbours[v]
                        .iter()
                        .filter(|&&w| layer_of[w] == adjacent)
                        .map(|&w| position[w])
                        .collect();
                    let own = if near.is_empty() {
                        position[v]
                    } else {
                        near.iter().sum::<f32>() / near.len() as f32
                    };
                    let key = paths[v]
                        .iter()
                        .map(|&k| (groups[k], 0, k))
                        .chain([(own, 1, v)])
                        .collect();
                    (key, v)
                })
                .collect();
            keyed.sort_by(|(a, _), (b, _)| by_path(a, b));
            layers[l] = keyed.into_iter().map(|(_, v)| v).collect();
            reindex(&layers, &mut position);
        }
    }
    // The sweeps' keys drift as positions change, so layers sorted early may order two
    // subgraphs differently from layers sorted late. One last pass with fixed keys
    // gives every layer the same subgraph order, keeping each layer's order otherwise.
    let groups = group_keys(&position);
    let sort_pos = position.clone();
    for layer in &mut layers {
        let mut keyed: Vec<(Vec<(f32, u8, usize)>, usize)> = layer
            .iter()
            .map(|&v| {
                let key = paths[v]
                    .iter()
                    .map(|&k| (groups[k], 0, k))
                    .chain([(sort_pos[v], 1, v)])
                    .collect();
                (key, v)
            })
            .collect();
        keyed.sort_by(|(a, _), (b, _)| by_path(a, b));
        *layer = keyed.into_iter().map(|(_, v)| v).collect();
    }
    reindex(&layers, &mut position);
    // Coordinates in a frame where `main` runs along the layers and `cross` across.
    let horizontal = chart.direction.horizontal();
    let flip = matches!(chart.direction, Direction::RightLeft | Direction::BottomTop);
    // Capacity: every end on a node side gets at least PORT_GAP of it, so many links
    // never crowd into one point; the node grows across the flow to make room.
    let mut side_count = vec![[0usize; 2]; n];
    for chain in &chains {
        side_count[chain[0]][1] += 1;
        if chain.len() > 1 {
            side_count[chain[chain.len() - 1]][0] += 1;
        }
    }
    for (v, counts) in side_count.iter().enumerate() {
        let k = counts[0].max(counts[1]);
        if k < 2 {
            continue;
        }
        let need = (k + 1) as f32 * PORT_GAP;
        let (w, h) = sizes[v];
        let (ms, cs) = if horizontal { (w, h) } else { (h, w) };
        // Mirrors the port margins below: what is left of the side after them.
        let cs = match chart.nodes[v].shape {
            Shape::Rect => cs.max(need + 12.0),
            Shape::Round => cs.max(need + 24.0),
            Shape::Stadium if !horizontal => cs.max(need + ms),
            Shape::Diamond => cs.max(need * 2.0),
            _ => cs.max(need / 0.6),
        };
        sizes[v] = match chart.nodes[v].shape {
            Shape::Circle => (cs.max(ms), cs.max(ms)),
            _ if horizontal => (w, cs),
            _ => (cs, h),
        };
    }
    let frame = |(w, h): (f32, f32)| if horizontal { (w, h) } else { (h, w) };
    let main_size = |v: usize| frame(sizes[v]).0;
    let cross_size = |v: usize| frame(sizes[v]).1;
    let extent: Vec<f32> = layers
        .iter()
        .map(|layer| layer.iter().map(|&v| main_size(v)).fold(0.0, f32::max))
        .collect();
    // Subgraphs are containers: each holds one cross interval through all its layers
    // and lays its own members out inside, so a border can never take in outside
    // content. Packed bottom-up (a subgraph opens after its parent, so children have
    // larger indices), then placed top-down. Horizontal flows put the title strip on
    // the cross side; vertical ones above the first layer (see `head`).
    let mut present = vec![false; clusters];
    for path in &paths {
        for &k in path {
            present[k] = true;
        }
    }
    let title_cross = if horizontal { CLUSTER_TITLE } else { 0.0 };
    // The layers each subgraph spans; it keeps its interval in all of them, including
    // layers where only outside links pass through.
    let mut span = vec![(usize::MAX, 0usize); clusters];
    for (v, path) in paths.iter().enumerate() {
        for &k in path {
            span[k] = (span[k].0.min(layer_of[v]), span[k].1.max(layer_of[v]));
        }
    }
    // What a vertex is inside `container`: itself (Ok) or one of its child subgraphs.
    let item = |v: usize, container: Option<usize>| -> Option<Result<usize, usize>> {
        let path = &paths[v];
        let depth = match container {
            None => 0,
            Some(c) => path.iter().position(|&k| k == c)? + 1,
        };
        Some(path.get(depth).map_or(Ok(v), |&g| Err(g)))
    };
    let run = |free: &[Result<usize, usize>]| {
        free.iter()
            .map(|it| it.map_or(0.0, cross_size))
            .sum::<f32>()
            + NODE_GAP * free.len().saturating_sub(1) as f32
    };
    let pack = |container: Option<usize>, width: &[f32]| -> Packed {
        let mut children: Vec<usize> = (0..clusters)
            .filter(|&k| present[k] && chart.clusters[k].parent == container)
            .collect();
        children.sort_by(|&a, &b| groups[a].total_cmp(&groups[b]).then(a.cmp(&b)));
        // Each layer's items in order; a child spanning the layer without a vertex in
        // it goes where its key sorts, as the final ordering pass would put it.
        let key = |it: &Result<usize, usize>| match *it {
            Ok(v) => (sort_pos[v], 1, v),
            Err(g) => (groups[g], 0, g),
        };
        let sequences: Vec<Vec<Result<usize, usize>>> = layers
            .iter()
            .enumerate()
            .map(|(l, layer)| {
                let mut seq: Vec<Result<usize, usize>> = Vec::new();
                for it in layer.iter().filter_map(|&v| item(v, container)) {
                    if it.is_ok() || seq.last() != Some(&it) {
                        seq.push(it);
                    }
                }
                for &g in &children {
                    if (span[g].0..=span[g].1).contains(&l) && !seq.contains(&Err(g)) {
                        let k = key(&Err(g));
                        let at = seq
                            .iter()
                            .position(|it| by_path(&[key(it)], &[k]).is_gt())
                            .unwrap_or(seq.len());
                        seq.insert(at, Err(g));
                    }
                }
                seq
            })
            .collect();
        // Each child sits right of the one before it, with room in every layer for the
        // free vertices that layer orders between them.
        let mut left = vec![0f32; clusters];
        let mut previous: Option<f32> = None;
        for &g in &children {
            let mut x = previous.map_or(0.0, |end| end + NODE_GAP);
            for seq in &sequences {
                let Some(at) = seq.iter().position(|it| *it == Err(g)) else {
                    continue;
                };
                if let Some(p) = seq[..at].iter().rposition(Result::is_err) {
                    let (before, free) = (seq[p].unwrap_err(), &seq[p + 1..at]);
                    let need = if free.is_empty() {
                        NODE_GAP
                    } else {
                        run(free) + 2.0 * NODE_GAP
                    };
                    x = x.max(left[before] + width[before] + need);
                }
            }
            left[g] = x;
            previous = Some(x + width[g]);
        }
        let (mut lo, mut hi) = children.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &g| {
            (lo.min(left[g]), hi.max(left[g] + width[g]))
        });
        // Layers without a child centre on the children (or on 0), as a plain graph's do.
        let reference = if children.is_empty() {
            0.0
        } else {
            (lo + hi) / 2.0
        };
        let mut placed = Vec::new();
        for seq in &sequences {
            let (mut i, mut before) = (0, None::<usize>);
            loop {
                let end = seq[i..]
                    .iter()
                    .position(Result::is_err)
                    .map_or(seq.len(), |p| i + p);
                let free = &seq[i..end];
                if !free.is_empty() {
                    let w = run(free);
                    let after = seq.get(end).map(|it| it.unwrap_err());
                    let mut c = match (before, after) {
                        (Some(p), Some(g)) => (left[p] + width[p] + left[g] - w) / 2.0,
                        (None, Some(g)) => left[g] - NODE_GAP - w,
                        (Some(p), None) => left[p] + width[p] + NODE_GAP,
                        (None, None) => reference - w / 2.0,
                    };
                    for v in free.iter().filter_map(|it| it.ok()) {
                        placed.push((v, c));
                        (lo, hi) = (lo.min(c), hi.max(c + cross_size(v)));
                        c += cross_size(v) + NODE_GAP;
                    }
                }
                if end >= seq.len() {
                    break;
                }
                before = Some(seq[end].unwrap_err());
                i = end + 1;
            }
        }
        if lo > hi {
            (lo, hi) = (0.0, 0.0);
        }
        Packed {
            groups: children.iter().map(|&g| (g, left[g])).collect(),
            vertices: placed,
            lo,
            hi,
        }
    };
    let mut width = vec![0f32; clusters];
    let mut packs: Vec<Option<Packed>> = (0..clusters).map(|_| None).collect();
    for k in (0..clusters).rev().filter(|&k| present[k]) {
        let packed = pack(Some(k), &width);
        width[k] = packed.hi - packed.lo + 2.0 * CLUSTER_PAD + title_cross;
        packs[k] = Some(packed);
    }
    let root = pack(None, &width);
    let mut cross_at = vec![0f32; vertices];
    let mut interval = vec![(0f32, 0f32); clusters];
    let mut adopt = |packed: &Packed, offset: f32, interval: &mut Vec<(f32, f32)>| {
        for &(v, c) in &packed.vertices {
            cross_at[v] = c + offset;
        }
        for &(g, c) in &packed.groups {
            interval[g] = (c + offset, c + offset + width[g]);
        }
    };
    adopt(&root, 0.0, &mut interval);
    for k in (0..clusters).filter(|&k| present[k]) {
        let packed = packs[k].as_ref().unwrap();
        let offset = interval[k].0 + CLUSTER_PAD + title_cross - packed.lo;
        adopt(packed, offset, &mut interval);
    }
    let centre_of = |cross_at: &[f32], v: usize| cross_at[v] + cross_size(v) / 2.0;
    // Ports (Pele's aim of separate ends): the ends on one side of a node spread
    // along it, ordered by where their links head so they do not cross. Side 0 faces
    // the lower layers, side 1 the higher; a self-loop takes the first out-port. Here
    // each port is (inset from its side, cross); the side's position follows below.
    let mut ends: Vec<[Vec<(f32, usize)>; 2]> = vec![[Vec::new(), Vec::new()]; n];
    for (i, chain) in chains.iter().enumerate() {
        let (lo, hi, last) = (chain[0], chain[chain.len() - 1], chain.len() - 1);
        if last == 0 {
            ends[lo][1].push((f32::MIN, i));
        } else {
            ends[lo][1].push((centre_of(&cross_at, chain[1]), i));
            ends[hi][0].push((centre_of(&cross_at, chain[last - 1]), i));
        }
    }
    let mut port = vec![[(0f32, 0f32); 2]; chains.len()];
    for (v, sides) in ends.iter_mut().enumerate() {
        let (c0, ms, cs) = (cross_at[v], main_size(v), cross_size(v));
        let shape = chart.nodes[v].shape;
        let margin = match shape {
            Shape::Rect => 6.0,
            Shape::Round => 12.0,
            Shape::Stadium if !horizontal => ms / 2.0,
            Shape::Diamond => cs * 0.25,
            _ => cs * 0.2,
        };
        for (side, list) in sides.iter_mut().enumerate() {
            list.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            let k = list.len() as f32;
            for (j, &(_, i)) in list.iter().enumerate() {
                let c = c0 + margin + (cs - 2.0 * margin) * (j as f32 + 1.0) / (k + 1.0);
                // How far the outline sits inside the box at this offset.
                let d = (c - c0 - cs / 2.0).abs();
                let r = cs / 2.0;
                let inset = match shape {
                    Shape::Diamond => ms * d / cs,
                    Shape::Circle => r - (r * r - d * d).max(0.0).sqrt(),
                    Shape::Stadium if horizontal => r - (r * r - d * d).max(0.0).sqrt(),
                    _ => 0.0,
                };
                port[i][side] = (inset, c);
            }
        }
    }
    // Two links that swap places between adjacent layers at nearly the same crosses
    // would run along each other for a track's length, whichever turns first. Nudge
    // one of their virtual vertices a track apart; same-layer neighbours sit NODE_GAP
    // away, so it keeps clear of them, and it stays inside its own subgraph borders
    // and out of others. Ends on a node use their port's cross.
    for l in 1..count {
        let layer_of = &layer_of;
        let at = |cross_at: &[f32], i: usize, k: usize| {
            let chain = &chains[i];
            match k {
                0 => port[i][1].1,
                _ if k + 1 == chain.len() => port[i][0].1,
                _ => centre_of(cross_at, chain[k]),
            }
        };
        let segments: Vec<(usize, usize)> = chains
            .iter()
            .enumerate()
            .flat_map(|(i, chain)| {
                (0..chain.len().saturating_sub(1))
                    .filter(move |&k| layer_of[chain[k]] + 1 == l && layer_of[chain[k + 1]] == l)
                    .map(move |k| (i, k))
            })
            .collect();
        for (a, &(i, k)) in segments.iter().enumerate() {
            for &(j, m) in &segments[a + 1..] {
                let near = |p: (usize, usize), q: (usize, usize)| {
                    (at(&cross_at, p.0, p.1) - at(&cross_at, q.0, q.1)).abs() < TRACK_GAP
                };
                if !(near((i, k), (j, m + 1)) && near((j, m), (i, k + 1))) {
                    continue;
                }
                let options = [
                    ((j, m + 1), (i, k)),
                    ((i, k + 1), (j, m)),
                    ((i, k), (j, m + 1)),
                    ((j, m), (i, k + 1)),
                ];
                for ((ci, ck), partner) in options {
                    let v = chains[ci][ck];
                    if v < n {
                        continue;
                    }
                    let away = if at(&cross_at, ci, ck) >= at(&cross_at, partner.0, partner.1) {
                        1.0
                    } else {
                        -1.0
                    };
                    let fits = |c: f32| {
                        let clear = layers[layer_of[v]].iter().filter(|&&x| x != v).all(|&x| {
                            c + cross_size(v) + NODE_GAP / 2.0 <= cross_at[x]
                                || cross_at[x] + cross_size(x) + NODE_GAP / 2.0 <= c
                        });
                        let inside = paths[v].last().is_none_or(|&g| {
                            interval[g].0 + CLUSTER_PAD / 2.0 + title_cross <= c
                                && c + cross_size(v) <= interval[g].1 - CLUSTER_PAD / 2.0
                        });
                        let outside = (0..clusters)
                            .filter(|&g| present[g] && !paths[v].contains(&g))
                            .all(|g| {
                                !(span[g].0..=span[g].1).contains(&layer_of[v])
                                    || c + cross_size(v) + CLUSTER_PAD / 2.0 <= interval[g].0
                                    || interval[g].1 + CLUSTER_PAD / 2.0 <= c
                            });
                        clear && inside && outside
                    };
                    let moved = [away, -away]
                        .into_iter()
                        .map(|d| cross_at[v] + d * TRACK_GAP)
                        .find(|&c| fits(c));
                    if let Some(c) = moved {
                        cross_at[v] = c;
                        break;
                    }
                }
            }
        }
    }
    let centre = |v: usize| cross_at[v] + cross_size(v) / 2.0;
    // Turns per gap, from the cross positions alone: each needs a track of its own.
    let mut turn_count = vec![0usize; count];
    for (i, chain) in chains.iter().enumerate().filter(|(_, c)| c.len() > 1) {
        let mut before = port[i][1].1;
        for (k, &v) in chain.iter().enumerate().skip(1) {
            let c = if k + 1 == chain.len() {
                port[i][0].1
            } else {
                centre(v)
            };
            if (before - c).abs() >= 0.5 {
                turn_count[layer_of[v]] += 1;
                before = c;
            }
        }
    }
    // Room a border needs along `main` before its first layer (`head`) and after its
    // last (`tail`): padding per nesting level, and the title strip on top when the
    // layers run vertically (after the last layer when they run upwards).
    let title = if horizontal { 0.0 } else { CLUSTER_TITLE };
    let (title_head, title_tail) = if flip { (0.0, title) } else { (title, 0.0) };
    let (mut head, mut tail) = (vec![0f32; clusters], vec![0f32; clusters]);
    for k in (0..clusters).rev().filter(|&k| present[k]) {
        head[k] += CLUSTER_PAD + title_head;
        tail[k] += CLUSTER_PAD + title_tail;
        if let Some(p) = chart.clusters[k].parent {
            if span[k].0 == span[p].0 {
                head[p] = head[p].max(head[k]);
            }
            if span[k].1 == span[p].1 {
                tail[p] = tail[p].max(tail[k]);
            }
        }
    }
    // Gaps grow to hold their tracks (TRACK_GAP apart) between the borders that end
    // before them and start after them; tracks use only the free middle (`region`).
    let mut start = vec![0f32; count];
    let mut region = vec![(0f32, 0f32); count];
    let mut cursor = 0.0;
    for l in 0..count {
        if l > 0 {
            let edge_room = |at: &dyn Fn(usize) -> bool, room: &[f32]| {
                (0..clusters)
                    .filter(|&k| present[k] && at(k))
                    .map(|k| room[k])
                    .fold(0.0, f32::max)
            };
            let before = edge_room(&|k| span[k].1 == l - 1, &tail);
            let after = edge_room(&|k| span[k].0 == l, &head);
            let base = if odd[l] || odd[l - 1] {
                RANK_GAP / 2.0
            } else {
                RANK_GAP
            };
            let tracks = (turn_count[l] + 1) as f32 * TRACK_GAP;
            let gap = base.max(before + tracks + after);
            region[l] = (cursor + before, cursor + gap - after);
            cursor += gap;
        }
        start[l] = cursor;
        cursor += extent[l];
    }
    let main_total = cursor;
    let mut corner = vec![(0f32, 0f32); vertices];
    for (l, layer) in layers.iter().enumerate() {
        for &v in layer {
            corner[v] = (start[l] + (extent[l] - main_size(v)) / 2.0, cross_at[v]);
        }
    }
    for (i, chain) in chains.iter().enumerate() {
        for (side, v) in [(0, chain[chain.len() - 1]), (1, chain[0])] {
            let (m, inset) = (corner[v].0, port[i][side].0);
            port[i][side].0 = if side == 0 {
                m + inset
            } else {
                m + main_size(v) - inset
            };
        }
    }
    // Back from the frame to x/y, mirrored for right-to-left and bottom-to-top.
    let point = |(m, c): (f32, f32)| {
        let m = if flip { main_total - m } else { m };
        if horizontal { (m, c) } else { (c, m) }
    };
    let place = |v: usize| {
        let (m, c) = corner[v];
        let (x, y) = point((if flip { m + main_size(v) } else { m }, c));
        Box2 {
            x,
            y,
            w: sizes[v].0,
            h: sizes[v].1,
        }
    };
    let mut boxes: Vec<Box2> = (0..n).map(place).collect();
    // Waypoints low end to high end as (entry, exit, cross); a change of cross
    // before layer `l` asks for a track in the gap ahead of it.
    let mut ways: Vec<Vec<(f32, f32, f32)>> = Vec::with_capacity(chains.len());
    let mut turns: Vec<Vec<(f32, f32, usize, usize)>> = vec![Vec::new(); count];
    for (i, chain) in chains.iter().enumerate() {
        let mut way: Vec<(f32, f32, f32)> = Vec::with_capacity(chain.len());
        for (k, &v) in chain.iter().enumerate().filter(|_| chain.len() > 1) {
            let l = layer_of[v];
            let (entry, exit, mut c) = if k == 0 {
                (0.0, port[i][1].0, port[i][1].1)
            } else if k + 1 == chain.len() {
                (port[i][0].0, 0.0, port[i][0].1)
            } else {
                (start[l], start[l] + extent[l], centre(v))
            };
            if let Some(&(_, _, before)) = way.last() {
                if (before - c).abs() < 0.5 {
                    c = before;
                } else {
                    turns[l].push((before, c, i, k));
                }
            }
            way.push((entry, exit, c));
        }
        ways.push(way);
    }
    // Every turn in a gap gets its own track. Links heading to larger cross turn
    // later the further up they start (and the reverse), so tracks do not cross.
    let mut turn_at: Vec<Vec<f32>> = chains.iter().map(|c| vec![0.0; c.len()]).collect();
    for (l, list) in turns.iter_mut().enumerate().skip(1) {
        let key = |&(from, to, i, _): &(f32, f32, usize, usize)| {
            (to > from, if to > from { -from } else { from }, i)
        };
        list.sort_by(|a, b| {
            let (a, b) = (key(a), key(b));
            a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2))
        });
        order_turns(list);
        let (g0, g1) = region[l];
        let k = list.len() as f32;
        for (j, &(_, _, i, step)) in list.iter().enumerate() {
            turn_at[i][step] = g0 + (g1 - g0) * (j as f32 + 1.0) / (k + 1.0);
        }
    }
    let mut edges: Vec<PlacedEdge> = chart
        .edges
        .iter()
        .zip(&chains)
        .zip(edge_labels)
        .enumerate()
        .map(|(i, ((edge, chain), label))| {
            let mut points: Vec<(f32, f32)> = Vec::new();
            let v = chain[0];
            let (m, c) = corner[v];
            if edge.from == edge.to {
                let ((px, pc), mid, out) =
                    (port[i][1], m + main_size(v) / 2.0, m + main_size(v) + 6.0);
                points = vec![
                    (px, pc),
                    (out, pc),
                    (out, c - 12.0),
                    (mid, c - 12.0),
                    (mid, c),
                ];
            }
            for (k, &(entry, exit, c)) in ways[i].iter().enumerate() {
                if let Some(&(_, before)) = points.last() {
                    if before != c {
                        points.extend([(turn_at[i][k], before), (turn_at[i][k], c)]);
                    }
                    points.push((entry, c));
                }
                if k + 1 < ways[i].len() {
                    points.push((exit, c));
                }
            }
            if rank[edge.from] > rank[edge.to] {
                points.reverse();
            }
            let label = match label {
                Some((v, text)) => Some((text, place(v))),
                // A self-loop's label rides above its loop.
                None => edge
                    .label
                    .as_ref()
                    .filter(|_| edge.from == edge.to)
                    .map(|text| {
                        let (w, h) = label_size(measure, text);
                        let (x, y) = point((m + main_size(v) / 2.0, c - 12.0));
                        let b = Box2 {
                            x: x - w / 2.0,
                            y: y - h / 2.0,
                            w,
                            h,
                        };
                        (text.clone(), b)
                    }),
            };
            PlacedEdge {
                points: tidy(points.into_iter().map(point).collect()),
                label,
                stroke: edge.stroke,
                arrow: edge.arrow,
            }
        })
        .collect();
    // Borders follow the containers: their cross interval through their layers.
    let mut clusters: Vec<Option<Box2>> = (0..clusters)
        .map(|k| {
            present[k].then(|| {
                let (a, b) = span[k];
                let (m0, m1) = (start[a] - head[k], start[b] + extent[b] + tail[k]);
                let (c0, c1) = interval[k];
                let ((x0, y0), (x1, y1)) = (point((m0, c0)), point((m1, c1)));
                Box2 {
                    x: x0.min(x1),
                    y: y0.min(y1),
                    w: (x1 - x0).abs(),
                    h: (y1 - y0).abs(),
                }
            })
        })
        .collect();
    // Shift everything to the origin; self-loops may poke past the node boxes.
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    let corners = boxes
        .iter()
        .chain(clusters.iter().flatten())
        .chain(edges.iter().filter_map(|e| e.label.as_ref().map(|l| &l.1)))
        .flat_map(|b| [(b.x, b.y), (b.x + b.w, b.y + b.h)]);
    for (x, y) in corners.chain(edges.iter().flat_map(|e| e.points.iter().copied())) {
        (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
    }
    if x0 > x1 {
        (x0, y0, x1, y1) = (0.0, 0.0, 0.0, 0.0);
    }
    for b in boxes.iter_mut().chain(clusters.iter_mut().flatten()).chain(
        edges
            .iter_mut()
            .filter_map(|e| e.label.as_mut().map(|l| &mut l.1)),
    ) {
        b.x -= x0;
        b.y -= y0;
    }
    for p in edges.iter_mut().flat_map(|e| &mut e.points) {
        *p = (p.0 - x0, p.1 - y0);
    }
    FlowLayout {
        boxes,
        labels,
        edges,
        clusters,
        width: x1 - x0,
        height: y1 - y0,
    }
}

/// Drop repeated points and the middle of straight runs.
fn tidy(points: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    let mut out: Vec<(f32, f32)> = Vec::with_capacity(points.len());
    for p in points {
        if out
            .last()
            .is_some_and(|q| (q.0 - p.0).abs() < 0.01 && (q.1 - p.1).abs() < 0.01)
        {
            continue;
        }
        if let &[.., a, b] = out.as_slice() {
            let straight = |i: fn(&(f32, f32)) -> f32| {
                (i(&a) - i(&b)).abs() < 0.01 && (i(&b) - i(&p)).abs() < 0.01
            };
            if straight(|q| q.0) || straight(|q| q.1) {
                out.pop();
            }
        }
        out.push(p);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::{Estimate, parse};
    use super::*;

    fn flow(source: &str) -> Flowchart {
        match parse(source).unwrap().diagram {
            Diagram::Flowchart(chart) => chart,
            other => panic!("expected flowchart, got {other:?}"),
        }
    }

    fn layout(source: &str) -> (Flowchart, FlowLayout) {
        let chart = flow(source);
        let layout = layout_flowchart(&chart, &Estimate);
        (chart, layout)
    }

    /// Interiors overlap by more than rounding.
    fn overlaps(a: Box2, b: Box2) -> bool {
        a.x + 0.5 < b.x + b.w
            && b.x + 0.5 < a.x + a.w
            && a.y + 0.5 < b.y + b.h
            && b.y + 0.5 < a.y + a.h
    }

    /// An axis-aligned segment enters the interior of `b`.
    fn crosses(p: (f32, f32), q: (f32, f32), b: Box2) -> bool {
        let segment = Box2 {
            x: p.0.min(q.0),
            y: p.1.min(q.1),
            w: (p.0 - q.0).abs(),
            h: (p.1 - q.1).abs(),
        };
        let inner = Box2 {
            x: b.x + 0.5,
            y: b.y + 0.5,
            w: b.w - 1.0,
            h: b.h - 1.0,
        };
        segment.x <= inner.x + inner.w
            && inner.x <= segment.x + segment.w
            && segment.y <= inner.y + inner.h
            && inner.y <= segment.y + segment.h
    }

    /// Two axis-aligned segments share a line and overlap by more than a touch.
    fn collinear(p: &[(f32, f32)], q: &[(f32, f32)]) -> bool {
        let span = |a: f32, b: f32| (a.min(b), a.max(b));
        let run = |(a0, a1): (f32, f32), (b0, b1): (f32, f32)| a1.min(b1) - a0.max(b0) > 0.5;
        let level = |a: f32, b: f32, c: f32, d: f32| {
            (a - b).abs() < 0.01 && (c - d).abs() < 0.01 && (a - c).abs() < 0.01
        };
        (level(p[0].1, p[1].1, q[0].1, q[1].1) && run(span(p[0].0, p[1].0), span(q[0].0, q[1].0)))
            || (level(p[0].0, p[1].0, q[0].0, q[1].0)
                && run(span(p[0].1, p[1].1), span(q[0].1, q[1].1)))
    }

    /// Distance between two parallel axis-aligned segments whose spans overlap by more
    /// than a touch; `None` when they are not parallel or do not overlap.
    fn parallel_distance(p: &[(f32, f32)], q: &[(f32, f32)]) -> Option<f32> {
        let overlap = |a0: f32, a1: f32, b0: f32, b1: f32| {
            a0.max(a1).min(b0.max(b1)) - a0.min(a1).max(b0.min(b1)) > 0.5
        };
        let level = |s: &[(f32, f32)]| (s[0].1 - s[1].1).abs() < 0.01;
        let upright = |s: &[(f32, f32)]| (s[0].0 - s[1].0).abs() < 0.01;
        if level(p) && level(q) && overlap(p[0].0, p[1].0, q[0].0, q[1].0) {
            Some((p[0].1 - q[0].1).abs())
        } else if upright(p) && upright(q) && overlap(p[0].1, p[1].1, q[0].1, q[1].1) {
            Some((p[0].0 - q[0].0).abs())
        } else {
            None
        }
    }

    /// Pele-style layout invariants that every flowchart must satisfy.
    fn check_invariants(source: &str) {
        let (chart, layout) = layout(source);
        let boxes = &layout.boxes;
        for (i, a) in boxes.iter().enumerate() {
            for b in &boxes[i + 1..] {
                assert!(!overlaps(*a, *b), "{source}: nodes overlap {a:?} {b:?}");
            }
        }
        for (edge, placed) in chart.edges.iter().zip(&layout.edges) {
            assert!(placed.points.len() >= 2);
            let last = placed.points.len() - 2;
            for (s, pair) in placed.points.windows(2).enumerate() {
                let (p, q) = (pair[0], pair[1]);
                assert!(
                    p.0 == q.0 || p.1 == q.1,
                    "{source}: diagonal segment {p:?} {q:?}"
                );
                for (v, b) in boxes.iter().enumerate() {
                    // Ends on a diamond or circle start inside their own box.
                    let own = (s == 0 && v == edge.from) || (s == last && v == edge.to);
                    assert!(
                        own || !crosses(p, q, *b),
                        "{source}: {p:?}-{q:?} crosses {b:?}"
                    );
                }
            }
            if let (Some((_, label)), false) = (&placed.label, edge.from == edge.to) {
                for b in boxes {
                    assert!(
                        !overlaps(*label, *b),
                        "{source}: label {label:?} on node {b:?}"
                    );
                }
                let on_line = placed.points.windows(2).any(|pair| {
                    let mid = (label.x + label.w / 2.0, label.y + label.h / 2.0);
                    crosses(
                        pair[0],
                        pair[1],
                        Box2 {
                            x: mid.0 - 1.0,
                            y: mid.1 - 1.0,
                            w: 2.0,
                            h: 2.0,
                        },
                    )
                });
                assert!(on_line, "{source}: label off its link");
            }
            let all = placed
                .points
                .iter()
                .flat_map(|&(x, y)| [x, y])
                .chain(boxes.iter().flat_map(|b| [b.x, b.y]));
            for value in all {
                assert!(
                    value.is_finite() && value >= -0.01,
                    "{source}: {value} out of bounds"
                );
            }
        }
        for b in boxes {
            assert!(b.x + b.w <= layout.width + 0.01 && b.y + b.h <= layout.height + 0.01);
        }
        // Pele's aims 1 and 4: distinct ends on a node never share a point, and no
        // two links run along each other.
        for v in 0..boxes.len() {
            let ends: Vec<(f32, f32)> = chart
                .edges
                .iter()
                .zip(&layout.edges)
                .flat_map(|(e, p)| {
                    let first = (e.from == v).then(|| p.points[0]);
                    first
                        .into_iter()
                        .chain((e.to == v).then(|| *p.points.last().unwrap()))
                })
                .collect();
            for (i, a) in ends.iter().enumerate() {
                for b in &ends[i + 1..] {
                    let apart = (a.0 - b.0).abs().max((a.1 - b.1).abs());
                    assert!(
                        apart >= PORT_GAP - 0.01,
                        "{source}: ends {a:?} {b:?} crowd node {v}"
                    );
                }
            }
        }
        for (i, a) in layout.edges.iter().enumerate() {
            for b in &layout.edges[i + 1..] {
                for p in a.points.windows(2) {
                    for q in b.points.windows(2) {
                        assert!(!collinear(p, q), "{source}: {p:?} runs along {q:?}");
                    }
                }
            }
        }
        // Parallel stretches of different links stay at least half a track apart.
        for (i, a) in layout.edges.iter().enumerate() {
            for b in &layout.edges[i + 1..] {
                for p in a.points.windows(2) {
                    for q in b.points.windows(2) {
                        assert!(
                            parallel_distance(p, q).is_none_or(|d| d >= TRACK_GAP / 2.0 - 0.01),
                            "{source}: {p:?} runs close to {q:?}"
                        );
                    }
                }
            }
        }
        for (k, border) in layout.clusters.iter().enumerate() {
            let populated = chart.nodes.iter().any(|node| {
                let mut cluster = node.cluster;
                while cluster.is_some_and(|c| c != k) {
                    cluster = chart.clusters[cluster.unwrap()].parent;
                }
                cluster.is_some()
            });
            assert_eq!(border.is_some(), populated, "{source}: border {k}");
            let Some(border) = border else { continue };
            for (j, other) in layout.clusters.iter().enumerate() {
                let Some(other) = other else { continue };
                let nested = |inner: usize, outer: usize| {
                    let mut c = Some(inner);
                    while c.is_some_and(|c| c != outer) {
                        c = chart.clusters[c.unwrap()].parent;
                    }
                    c.is_some()
                };
                if j != k && !nested(j, k) && !nested(k, j) {
                    assert!(
                        !overlaps(*border, *other),
                        "{source}: borders {k} and {j} overlap"
                    );
                }
            }
            for (edge, placed) in chart.edges.iter().zip(&layout.edges) {
                let inside = |v: usize| {
                    let mut c = chart.nodes[v].cluster;
                    while c.is_some_and(|c| c != k) {
                        c = chart.clusters[c.unwrap()].parent;
                    }
                    c.is_some()
                };
                if let (Some((_, label)), false) =
                    (&placed.label, inside(edge.from) && inside(edge.to))
                {
                    assert!(
                        !overlaps(*border, *label),
                        "{source}: border {k} takes in label {label:?}"
                    );
                }
            }
            for (node, b) in chart.nodes.iter().zip(boxes) {
                let mut cluster = node.cluster;
                while cluster.is_some_and(|c| c != k) {
                    cluster = chart.clusters[cluster.unwrap()].parent;
                }
                if cluster.is_some() {
                    assert!(border.x < b.x && b.x + b.w < border.x + border.w);
                    assert!(border.y < b.y && b.y + b.h < border.y + border.h);
                } else {
                    assert!(!overlaps(*border, *b), "{source}: border takes in {b:?}");
                }
            }
        }
        assert_eq!(
            layout,
            layout_flowchart(&chart, &Estimate),
            "{source}: not deterministic"
        );
    }

    /// A deterministic pseudo-random graph with long links, cycles and labels.
    fn tangle(nodes: usize, edges: usize, seed: u32) -> String {
        let mut state = seed;
        let mut next = |bound: usize| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 8) as usize % bound
        };
        let mut source = String::from("flowchart TD\n");
        for _ in 0..edges {
            let (a, b) = (next(nodes), next(nodes));
            if a == b {
                continue;
            }
            let label = if next(3) == 0 {
                format!("|标签 {a}|")
            } else {
                String::new()
            };
            source += &format!("n{a}[节点 {a}] -->{label} n{b}[Node {b} with a long name]\n");
        }
        source
    }

    /// `tangle`, with its nodes declared inside a few (sometimes nested) subgraphs.
    fn clustered_tangle(nodes: usize, edges: usize, seed: u32, direction: &str) -> String {
        let mut state = seed ^ 0x9e37_79b9;
        let mut next = |bound: usize| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 8) as usize % bound
        };
        let mut source = format!("flowchart {direction}\n");
        let groups = 2 + next(3);
        for g in 0..groups {
            source += &format!("subgraph s{g} [组 {g}]\n");
            for v in (0..nodes).filter(|v| v % (groups + 1) == g) {
                source += &format!("  n{v}[节点 {v}]\n");
            }
            if next(2) == 0 {
                source += &format!("  subgraph s{g}x [内层 {g}]\n");
                let v = (g + groups + 1) % nodes;
                source +=
                    &format!("    m{g}[Inner {g}] --> m{g}b[Inner {g}b]\n  end\n  n{v} --> m{g}\n");
            }
            source += "end\n";
        }
        source += &tangle(nodes, edges, seed).replacen("flowchart TD\n", "", 1);
        source
    }

    #[test]
    fn subgraphs_always_keep_their_borders_and_exclude_outsiders() {
        check_invariants(SUBGRAPHS);
        for seed in 1..9 {
            for direction in ["LR", "RL", "TB", "BT"] {
                check_invariants(&clustered_tangle(18, 28, seed, direction));
            }
        }
        // The case that used to lose its border: an outside node whose rank sits inside
        // a subgraph's span is placed beside the container, not within it.
        let source = "flowchart TD\nsubgraph g [组]\n  A --> B --> C\nend\nX --> Y\nA --> Y --> C";
        check_invariants(source);
        let (_, layout) = layout(source);
        assert!(layout.clusters[0].is_some());
    }

    #[test]
    fn ports_and_tracks_grow_with_the_number_of_links() {
        for n in [3, 6, 9, 12, 20] {
            for direction in ["LR", "RL", "TB", "BT"] {
                let mut fan = format!("flowchart {direction}\n");
                for i in 0..n {
                    fan += &format!(" A --> B{i}\n C{i} --> D\n");
                }
                check_invariants(&fan);
            }
        }
        // Nine links out of a plain box: the box grows across the flow to fit them.
        let source: String = std::iter::once("flowchart LR\n".to_string())
            .chain((0..9).map(|i| format!(" A --> B{i}\n")))
            .collect();
        let (_, layout) = layout(&source);
        let mut ends: Vec<f32> = layout.edges.iter().map(|e| e.points[0].1).collect();
        ends.sort_by(f32::total_cmp);
        assert!(
            ends.windows(2).all(|w| w[1] - w[0] >= PORT_GAP - 0.01),
            "{ends:?}"
        );
        assert!(layout.boxes[0].h >= 10.0 * PORT_GAP + 12.0);
    }

    #[test]
    fn layouts_keep_nodes_links_and_labels_apart() {
        let mut sources = vec![
            "flowchart LR\nA[需求冻结] --> SYS[SYS 开发] & VND[VND 开发]\nSYS & VND --> I[集成] --> V[验证]".to_string(),
            "graph TD\nA --> B --> C --> D\nA --> D\nA -- skip all --> D\nD -.->|retry| A\nC --> C".to_string(),
            "flowchart LR\nA -->|very long label that needs wrapping to fit| B\nA --> C{决策}\nC -- 是 --> D((End))\nC -- 否 --> B".to_string(),
            "flowchart LR\nA & B & C --> D & E\nE --> A".to_string(),
            SUBGRAPHS.to_string(),
            SUBGRAPHS.replacen("flowchart LR", "flowchart TD", 1),
            DIAMOND.to_string(),
            MERGE.to_string(),
        ];
        for base in [DIAMOND, MERGE] {
            for direction in ["LR", "RL", "TB", "BT"] {
                let header = base.lines().next().unwrap();
                sources.push(base.replacen(header, &format!("flowchart {direction}"), 1));
            }
        }
        for direction in ["LR", "RL", "TB", "BT"] {
            sources.push(sources[1].replacen("graph TD", &format!("flowchart {direction}"), 1));
        }
        for seed in 1..6 {
            sources.push(tangle(24, 40, seed));
        }
        for source in &sources {
            check_invariants(source);
        }
    }

    /// Shared ends on a diamond's side, with a back link into the same side.
    const DIAMOND: &str = "flowchart LR\n A[开始] --> B{检查通过?}\n B -->|是| C[发布 release build]\n B -->|否| D[修复]\n D --> B\n";
    /// A short and a long link arriving at the same node from one layer apart.
    const MERGE: &str = "flowchart TD\n subgraph 构建\n  A[编译] --> B[单元测试]\n end\n B --> C[打包]\n C -.-> D((完成))\n A ==> C\n";

    #[test]
    fn shared_sides_get_separate_ports_and_tracks() {
        let (_, l) = layout(MERGE);
        let c = l.boxes[2];
        // B --> C and A ==> C arrive at separate points on C's top.
        let [b_in, a_in] = [1, 3].map(|e| *l.edges[e].points.last().unwrap());
        assert_eq!((b_in.1, a_in.1), (c.y, c.y));
        assert!((b_in.0 - a_in.0).abs() >= 8.0);
        let (_, l) = layout(DIAMOND);
        let b = l.boxes[1];
        // Three ends on the diamond's right vertex sit on its two slanted edges.
        let e = &l.edges;
        let right = [e[1].points[0], e[2].points[0], *e[3].points.last().unwrap()];
        for p in &right {
            let along = (p.0 - (b.x + b.w / 2.0)) / (b.w / 2.0)
                + (p.1 - (b.y + b.h / 2.0)).abs() / (b.h / 2.0);
            assert!((along - 1.0).abs() < 0.01, "{p:?} off the outline of {b:?}");
        }
    }

    const SUBGRAPHS: &str = "flowchart LR\nS[Start] --> A1\nsubgraph dev [开发]\n  A1[SYS] --> A2[VND]\n  subgraph inner\n    A3[Review]\n  end\n  A2 --> A3\nend\nsubgraph \"Test\"\n  T1 --> T2\nend\nA3 --> T1 --> E[End]\nS --> E";

    #[test]
    fn subgraphs_nest_and_draw_borders_around_their_members() {
        let (chart, layout) = layout(SUBGRAPHS);
        let titles: Vec<_> = chart
            .clusters
            .iter()
            .map(|c| (c.title.as_str(), c.parent))
            .collect();
        assert_eq!(titles, [("开发", None), ("inner", Some(0)), ("Test", None)]);
        let cluster = |id: &str| chart.nodes.iter().find(|n| n.id == id).unwrap().cluster;
        assert_eq!(
            [cluster("S"), cluster("A1"), cluster("A3"), cluster("T2")],
            [None, Some(0), Some(1), Some(2)]
        );
        let [outer, inner, test] = [0, 1, 2].map(|k| layout.clusters[k].unwrap());
        assert!(outer.x < inner.x && inner.x + inner.w < outer.x + outer.w);
        assert!(outer.y < inner.y && !overlaps(outer, test));
        // `end` closes only the innermost block; a stray one is harmless.
        assert!(parse("flowchart LR\nend\nA --> B").is_ok());
    }

    #[test]
    fn labelled_links_reserve_a_slot_between_ranks() {
        let (_, plain) = layout("flowchart LR\nA --> B");
        let (_, labelled) = layout("flowchart LR\nA -->|a long label| B");
        let gap = |l: &FlowLayout| l.boxes[1].x - (l.boxes[0].x + l.boxes[0].w);
        let (_, label) = labelled.edges[0].label.clone().unwrap();
        assert_eq!(gap(&plain), RANK_GAP);
        assert_eq!(gap(&labelled), RANK_GAP + label.w);
        assert!(label.x > labelled.boxes[0].x + labelled.boxes[0].w);
        assert!(label.x + label.w < labelled.boxes[1].x);
    }

    #[test]
    fn long_links_route_around_intermediate_nodes() {
        let (_, l) = layout("flowchart LR\nA --> B --> C\nA --> C");
        let skip = &l.edges[2];
        assert!(skip.points.len() > 2);
        assert!(skip.points.iter().all(|p| !crosses(*p, *p, l.boxes[1])));
    }

    #[test]
    fn long_labels_wrap_instead_of_clipping() {
        let (chart, l) =
            layout("flowchart TD\nA[Integration and verification of the vendor build]");
        assert!(l.labels[0].contains('\n'));
        assert!(text_width(&Estimate, &l.labels[0], LABEL_SIZE) + 28.0 <= l.boxes[0].w);
        assert_eq!(chart.nodes[0].label.lines().count(), 1);
        assert!(l.boxes[0].h > 36.0);
    }

    #[test]
    fn fan_out_multiplies_groups_but_stays_within_the_link_cap() {
        let (chart, layout) = layout("flowchart LR\nA & B & C --> D & E");
        assert_eq!(chart.edges.len(), 6);
        assert_eq!(layout.edges.len(), 6);
        let ids: Vec<String> = (0..21).map(|i| format!("a{i}")).collect();
        let targets: Vec<String> = (0..20).map(|i| format!("b{i}")).collect();
        let source = format!(
            "flowchart LR\n{} --> {}",
            ids.join(" & "),
            targets.join(" & ")
        );
        let err = parse(&source).unwrap_err();
        assert_eq!((err.line, err.kind), (2, ErrorKind::TooLarge));
    }

    #[test]
    fn parallel_branches_share_a_rank_and_sit_symmetrically() {
        let (_, layout) = layout("flowchart LR\nA --> S & V\nS & V --> I\nI --> X");
        let [a, s, v, i, x] = [0, 1, 2, 3, 4].map(|k| layout.boxes[k]);
        assert_eq!(s.x, v.x);
        assert!(a.x + a.w < s.x && s.x + s.w < i.x && i.x + i.w < x.x);
        let mid = |b: Box2| b.y + b.h / 2.0;
        assert!((mid(a) - (mid(s) + mid(v)) / 2.0).abs() < 0.5);
        assert!((mid(a) - mid(i)).abs() < 0.5);
        // Links leave the right edge and arrive at the left edge, turning in the gap.
        // A's two ends spread along its right side, the upper one heading up.
        let first = &layout.edges[0];
        assert_eq!(first.points[0].0, a.x + a.w);
        assert!(first.points[0].1 < mid(a) && mid(a) < layout.edges[1].points[0].1);
        assert_eq!(*first.points.last().unwrap(), (s.x, mid(s)));
        assert!(layout.width > 0.0 && layout.height > 0.0);
    }

    #[test]
    fn cycles_and_top_down_layouts_stay_finite() {
        let (_, layout) = layout("graph TD\nA --> B --> C --> A\nC --> C");
        assert!(layout.boxes[0].y < layout.boxes[1].y);
        // The back link C --> A is routed reversed and ends on A's lower edge.
        let back = &layout.edges[2];
        assert!(back.points.len() > 2);
        let a = layout.boxes[0];
        assert_eq!(back.points.last().unwrap().1, a.y + a.h);
    }
}
