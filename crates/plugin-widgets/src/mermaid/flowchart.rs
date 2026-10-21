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
    /// One border per subgraph; `None` when it would enclose outside nodes or labels.
    pub(crate) clusters: Vec<Option<Box2>>,
    pub(crate) width: f32,
    pub(crate) height: f32,
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
    // Members of one top-level subgraph stay contiguous within each layer.
    let top = |mut k: usize| {
        while let Some(parent) = chart.clusters[k].parent {
            k = parent;
        }
        k
    };
    let group = |v: usize| chart.nodes.get(v).and_then(|node| node.cluster).map(top);
    for sweep in 0..6 {
        let order: Vec<usize> = if sweep % 2 == 0 {
            (1..count).collect()
        } else {
            (0..count.saturating_sub(1)).rev().collect()
        };
        for l in order {
            let adjacent = if sweep % 2 == 0 { l - 1 } else { l + 1 };
            let mut keyed: Vec<(f32, usize)> = layers[l]
                .iter()
                .map(|&v| {
                    let near: Vec<f32> = neighbours[v]
                        .iter()
                        .filter(|&&w| layer_of[w] == adjacent)
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
            let mut mean = vec![(0f32, 0f32); chart.clusters.len()];
            for &(key, v) in &keyed {
                if let Some(g) = group(v) {
                    mean[g] = (mean[g].0 + key, mean[g].1 + 1.0);
                }
            }
            let sort_key = |&(key, v): &(f32, usize)| match group(v) {
                Some(g) => (mean[g].0 / mean[g].1, g, key),
                None => (key, usize::MAX, key),
            };
            keyed.sort_by(|a, b| {
                let (a, b) = (sort_key(a), sort_key(b));
                a.0.total_cmp(&b.0)
                    .then(a.1.cmp(&b.1))
                    .then(a.2.total_cmp(&b.2))
            });
            layers[l] = keyed.into_iter().map(|(_, v)| v).collect();
            reindex(&layers, &mut position);
        }
    }
    // Coordinates in a frame where `main` runs along the layers and `cross` across.
    let horizontal = chart.direction.horizontal();
    let frame = |(w, h): (f32, f32)| if horizontal { (w, h) } else { (h, w) };
    let main_size = |v: usize| frame(sizes[v]).0;
    let cross_size = |v: usize| frame(sizes[v]).1;
    let extent: Vec<f32> = layers
        .iter()
        .map(|layer| layer.iter().map(|&v| main_size(v)).fold(0.0, f32::max))
        .collect();
    let mut start = vec![0f32; count];
    let mut cursor = 0.0;
    for l in 0..count {
        if l > 0 {
            cursor += if odd[l] || odd[l - 1] {
                RANK_GAP / 2.0
            } else {
                RANK_GAP
            };
        }
        start[l] = cursor;
        cursor += extent[l];
    }
    let main_total = cursor;
    // Neighbours from different subgraphs leave room for a border (and its title).
    let title = if horizontal { CLUSTER_TITLE } else { 0.0 };
    let gap = |a: usize, b: usize| {
        if group(a) == group(b) {
            NODE_GAP
        } else {
            NODE_GAP + 2.0 * CLUSTER_PAD + title
        }
    };
    let mut corner = vec![(0f32, 0f32); vertices];
    for (l, layer) in layers.iter().enumerate() {
        let total: f32 = layer.iter().map(|&v| cross_size(v)).sum::<f32>()
            + layer.windows(2).map(|p| gap(p[0], p[1])).sum::<f32>();
        let mut along = -total / 2.0;
        for (i, &v) in layer.iter().enumerate() {
            corner[v] = (start[l] + (extent[l] - main_size(v)) / 2.0, along);
            along += cross_size(v) + layer.get(i + 1).map_or(0.0, |&w| gap(v, w));
        }
    }
    // Back from the frame to x/y, mirrored for right-to-left and bottom-to-top.
    let flip = matches!(chart.direction, Direction::RightLeft | Direction::BottomTop);
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
    let centre = |v: usize| corner[v].1 + cross_size(v) / 2.0;
    // Ports (Pele's aim of separate ends): the ends on one side of a node spread
    // along it, ordered by where their links head so they do not cross. Side 0 faces
    // the lower layers, side 1 the higher; a self-loop takes the first out-port.
    let mut ends: Vec<[Vec<(f32, usize)>; 2]> = vec![[Vec::new(), Vec::new()]; n];
    for (i, chain) in chains.iter().enumerate() {
        let (lo, hi, last) = (chain[0], chain[chain.len() - 1], chain.len() - 1);
        if last == 0 {
            ends[lo][1].push((f32::MIN, i));
        } else {
            ends[lo][1].push((centre(chain[1]), i));
            ends[hi][0].push((centre(chain[last - 1]), i));
        }
    }
    let mut port = vec![[(0f32, 0f32); 2]; chains.len()];
    for (v, sides) in ends.iter_mut().enumerate() {
        let ((m, c0), ms, cs) = (corner[v], main_size(v), cross_size(v));
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
                port[i][side] = (if side == 0 { m + inset } else { m + ms - inset }, c);
            }
        }
    }
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
        let (g0, g1) = (start[l - 1] + extent[l - 1], start[l]);
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
    let mut clusters = cluster_boxes(chart, &boxes, &edges);
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

/// Subgraph borders around their members (nested ones inside their parents), with a
/// title strip on top. A border that would take in an outside node, an outside
/// link's label or an unrelated border is left out rather than drawn misleadingly.
fn cluster_boxes(chart: &Flowchart, boxes: &[Box2], edges: &[PlacedEdge]) -> Vec<Option<Box2>> {
    let count = chart.clusters.len();
    let within = |mut cluster: Option<usize>, k: usize| {
        while let Some(c) = cluster {
            if c == k {
                return true;
            }
            cluster = chart.clusters[c].parent;
        }
        false
    };
    let mut depth = vec![0f32; count];
    for k in (0..count).rev() {
        if let Some(parent) = chart.clusters[k].parent {
            depth[parent] = depth[parent].max(depth[k] + 1.0);
        }
    }
    let mut out: Vec<Option<Box2>> = Vec::with_capacity(count);
    for (k, &depth) in depth.iter().enumerate() {
        let inside = |v: usize| within(chart.nodes[v].cluster, k);
        let members = (0..boxes.len()).filter(|&v| inside(v)).map(|v| boxes[v]);
        let Some((x0, y0, x1, y1)) = members.fold(None, |acc: Option<(f32, f32, f32, f32)>, b| {
            let (x0, y0, x1, y1) = acc.unwrap_or((b.x, b.y, b.x + b.w, b.y + b.h));
            Some((
                x0.min(b.x),
                y0.min(b.y),
                x1.max(b.x + b.w),
                y1.max(b.y + b.h),
            ))
        }) else {
            out.push(None);
            continue;
        };
        let (pad, head) = (
            CLUSTER_PAD * (1.0 + depth),
            (CLUSTER_PAD + CLUSTER_TITLE) * (1.0 + depth),
        );
        let border = Box2 {
            x: x0 - pad,
            y: y0 - head,
            w: x1 - x0 + 2.0 * pad,
            h: y1 - y0 + pad + head,
        };
        let hits = |b: &Box2| {
            border.x < b.x + b.w
                && b.x < border.x + border.w
                && border.y < b.y + b.h
                && b.y < border.y + border.h
        };
        let foreign_node = (0..boxes.len()).any(|v| !inside(v) && hits(&boxes[v]));
        let foreign_label = chart.edges.iter().zip(edges).any(|(edge, placed)| {
            !(inside(edge.from) && inside(edge.to))
                && placed.label.as_ref().is_some_and(|l| hits(&l.1))
        });
        let unrelated = out
            .iter()
            .enumerate()
            .any(|(j, other)| !within(Some(k), j) && other.as_ref().is_some_and(hits));
        out.push((!foreign_node && !foreign_label && !unrelated).then_some(border));
    }
    out
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
                    assert!(apart >= 3.0, "{source}: ends {a:?} {b:?} share node {v}");
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
        for (k, border) in layout.clusters.iter().enumerate() {
            let Some(border) = border else { continue };
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
