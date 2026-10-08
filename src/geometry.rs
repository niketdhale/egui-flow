//! Edge routing. Every edge is reduced to a polyline, which is used both for
//! drawing and for hit-testing.

use egui::{Pos2, Rect, Vec2, pos2, vec2};

use crate::types::{EdgeKind, Side};

const BEZIER_SEGMENTS: usize = 40;
const STEP_STUB: f32 = 20.0;
const CORNER_RADIUS: f32 = 8.0;

/// Polyline from `s` (leaving through `ss`) to `t` (entering through `ts`).
pub fn edge_path(kind: EdgeKind, s: Pos2, ss: Side, t: Pos2, ts: Side) -> Vec<Pos2> {
    match kind {
        EdgeKind::Straight => vec![s, t],
        EdgeKind::Bezier => bezier(s, ss, t, ts),
        EdgeKind::Step => orthogonal(s, ss, t, ts, 0.0),
        EdgeKind::SmoothStep => orthogonal(s, ss, t, ts, CORNER_RADIUS),
    }
}

/// React Flow's control-point rule: proportional when the target is ahead,
/// a square-root fall-off when the edge has to loop backwards.
fn control_offset(distance: f32) -> f32 {
    const CURVATURE: f32 = 0.25;
    if distance >= 0.0 {
        0.5 * distance
    } else {
        CURVATURE * 25.0 * (-distance).sqrt()
    }
}

fn control_point(p: Pos2, side: Side, other: Pos2) -> Pos2 {
    match side {
        Side::Right => pos2(p.x + control_offset(other.x - p.x), p.y),
        Side::Left => pos2(p.x - control_offset(p.x - other.x), p.y),
        Side::Bottom => pos2(p.x, p.y + control_offset(other.y - p.y)),
        Side::Top => pos2(p.x, p.y - control_offset(p.y - other.y)),
    }
}

fn bezier(s: Pos2, ss: Side, t: Pos2, ts: Side) -> Vec<Pos2> {
    let c1 = control_point(s, ss, t);
    let c2 = control_point(t, ts, s);
    (0..=BEZIER_SEGMENTS)
        .map(|i| {
            let u = i as f32 / BEZIER_SEGMENTS as f32;
            let v = 1.0 - u;
            let w = [v * v * v, 3.0 * v * v * u, 3.0 * v * u * u, u * u * u];
            pos2(
                w[0] * s.x + w[1] * c1.x + w[2] * c2.x + w[3] * t.x,
                w[0] * s.y + w[1] * c1.y + w[2] * c2.y + w[3] * t.y,
            )
        })
        .collect()
}

fn orthogonal(s: Pos2, ss: Side, t: Pos2, ts: Side, radius: f32) -> Vec<Pos2> {
    finish(step_points(s, ss, t, ts), radius)
}

fn finish(pts: Vec<Pos2>, radius: f32) -> Vec<Pos2> {
    if radius > 0.0 {
        round_corners(&pts, radius)
    } else {
        pts
    }
}

fn step_points(s: Pos2, ss: Side, t: Pos2, ts: Side) -> Vec<Pos2> {
    let p1 = s + ss.dir() * STEP_STUB;
    let p2 = t + ts.dir() * STEP_STUB;
    let mut pts = vec![s, p1];
    match (ss.is_horizontal(), ts.is_horizontal()) {
        (true, true) => {
            let mx = (p1.x + p2.x) / 2.0;
            pts.extend([pos2(mx, p1.y), pos2(mx, p2.y)]);
        }
        (false, false) => {
            let my = (p1.y + p2.y) / 2.0;
            pts.extend([pos2(p1.x, my), pos2(p2.x, my)]);
        }
        (true, false) => pts.push(pos2(p2.x, p1.y)),
        (false, true) => pts.push(pos2(p1.x, p2.y)),
    }
    pts.extend([p2, t]);
    pts.dedup_by(|a, b| (*a - *b).length_sq() < 1e-6);
    pts
}

/// Clear space kept around a node by [`edge_path_around`].
const AVOID_MARGIN: f32 = 14.0;
/// Extra cost of a bend, in flow units of detour it is worth.
const BEND_COST: f32 = 40.0;
/// Only nodes this close to the edge's bounding box are considered.
const AVOID_WINDOW: f32 = 200.0;

/// Like [`edge_path`], but `Step` and `SmoothStep` edges detour around `obstacles` (node
/// rectangles) when the plain route would cross one. Other kinds, and edges that cross
/// nothing, are routed as usual; if no clear route exists the plain one is kept.
pub fn edge_path_around(
    kind: EdgeKind,
    s: Pos2,
    ss: Side,
    t: Pos2,
    ts: Side,
    obstacles: &[Rect],
) -> Vec<Pos2> {
    let radius = match kind {
        EdgeKind::Step => 0.0,
        EdgeKind::SmoothStep => CORNER_RADIUS,
        _ => return edge_path(kind, s, ss, t, ts),
    };
    let plain = step_points(s, ss, t, ts);
    let window = Rect::from_two_pos(s, t).expand(AVOID_WINDOW);
    let boxes: Vec<Rect> = obstacles
        .iter()
        .filter(|r| r.intersects(window))
        .map(|r| r.expand(AVOID_MARGIN))
        .collect();
    let crosses = plain
        .windows(2)
        .any(|w| boxes.iter().any(|b| segment_blocked(w[0], w[1], b)));
    if !crosses {
        return finish(plain, radius);
    }
    let (p1, p2) = (s + ss.dir() * STEP_STUB, t + ts.dir() * STEP_STUB);
    match grid_route(p1, ss, p2, &boxes) {
        Some(mid) => {
            let mut pts = vec![s];
            pts.extend(mid);
            pts.push(t);
            pts.dedup_by(|a, b| (*a - *b).length_sq() < 1e-6);
            finish(pts, radius)
        }
        None => finish(plain, radius),
    }
}

/// Whether the axis-aligned segment `a`–`b` passes through the inside of `r`.
fn segment_blocked(a: Pos2, b: Pos2, r: &Rect) -> bool {
    let (lo, hi) = (a.min(b), a.max(b));
    lo.x < r.max.x && hi.x > r.min.x && lo.y < r.max.y && hi.y > r.min.y
}

/// Cheapest orthogonal route from `p1` (leaving along `dir`) to `p2` that stays out of
/// `boxes`: Dijkstra over the grid of lines through the boxes' edges, penalising bends.
// ponytail: rebuilt every frame for edges that cross a node; cache by endpoints if graphs
// with hundreds of such edges make it slow.
fn grid_route(p1: Pos2, dir: Side, p2: Pos2, boxes: &[Rect]) -> Option<Vec<Pos2>> {
    fn axis(mut v: Vec<f32>) -> Vec<f32> {
        v.sort_by(f32::total_cmp);
        v.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
        v
    }
    let xs = axis(
        boxes
            .iter()
            .flat_map(|b| [b.min.x, b.max.x])
            .chain([p1.x, p2.x])
            .collect(),
    );
    let ys = axis(
        boxes
            .iter()
            .flat_map(|b| [b.min.y, b.max.y])
            .chain([p1.y, p2.y])
            .collect(),
    );
    let nearest = |v: &[f32], x: f32| {
        (0..v.len())
            .min_by(|&i, &j| (v[i] - x).abs().total_cmp(&(v[j] - x).abs()))
            .unwrap_or(0)
    };
    let (si, sj) = (nearest(&xs, p1.x), nearest(&ys, p1.y));
    let (gi, gj) = (nearest(&xs, p2.x), nearest(&ys, p2.y));
    const DIRS: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)]; // right, down, left, up
    let d0 = match dir {
        Side::Right => 0,
        Side::Bottom => 1,
        Side::Left => 2,
        Side::Top => 3,
    };
    let (nx, ny) = (xs.len(), ys.len());
    let idx = |i: usize, j: usize, d: usize| (j * nx + i) * 4 + d;
    let mut cost = vec![u32::MAX; nx * ny * 4];
    let mut prev = vec![usize::MAX; nx * ny * 4];
    let mut heap = std::collections::BinaryHeap::new();
    cost[idx(si, sj, d0)] = 0;
    heap.push(std::cmp::Reverse((0u32, si, sj, d0)));
    let mut goal = None;
    while let Some(std::cmp::Reverse((c, i, j, d))) = heap.pop() {
        if c > cost[idx(i, j, d)] {
            continue;
        }
        if (i, j) == (gi, gj) {
            goal = Some((i, j, d));
            break;
        }
        for (nd, (dx, dy)) in DIRS.iter().enumerate() {
            if nd == (d + 2) % 4 {
                continue; // no U-turns
            }
            let (ni, nj) = (i as i32 + dx, j as i32 + dy);
            if ni < 0 || nj < 0 || ni >= nx as i32 || nj >= ny as i32 {
                continue;
            }
            let (ni, nj) = (ni as usize, nj as usize);
            let (a, b) = (pos2(xs[i], ys[j]), pos2(xs[ni], ys[nj]));
            if boxes.iter().any(|r| segment_blocked(a, b, r)) {
                continue;
            }
            let step = ((b - a).length() * 4.0) as u32
                + if nd == d { 0 } else { (BEND_COST * 4.0) as u32 };
            let nc = c + step;
            let k = idx(ni, nj, nd);
            if nc < cost[k] {
                cost[k] = nc;
                prev[k] = idx(i, j, d);
                heap.push(std::cmp::Reverse((nc, ni, nj, nd)));
            }
        }
    }
    let (i, j, d) = goal?;
    let mut cells = vec![(i, j)];
    let mut k = idx(i, j, d);
    while prev[k] != usize::MAX {
        k = prev[k];
        let (cell, _) = (k / 4, k % 4);
        cells.push((cell % nx, cell / nx));
    }
    cells.reverse();
    let mut pts: Vec<Pos2> = cells.iter().map(|&(i, j)| pos2(xs[i], ys[j])).collect();
    pts[0] = p1;
    *pts.last_mut()? = p2;
    // Keep only the corners.
    let mut out = vec![pts[0]];
    for w in pts.windows(3) {
        let (a, b, c) = (w[0], w[1], w[2]);
        if (b - a).normalized() != (c - b).normalized() {
            out.push(b);
        }
    }
    out.push(*pts.last()?);
    Some(out)
}

/// Replace each interior vertex by a quadratic arc.
fn round_corners(pts: &[Pos2], radius: f32) -> Vec<Pos2> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut out = vec![pts[0]];
    for w in pts.windows(3) {
        let (a, b, c) = (w[0], w[1], w[2]);
        let r = radius
            .min((b - a).length() / 2.0)
            .min((c - b).length() / 2.0);
        let dir_in = (b - a).normalized();
        let dir_out = (c - b).normalized();
        let start = b - dir_in * r;
        let end = b + dir_out * r;
        for i in 0..=6 {
            let u = i as f32 / 6.0;
            let v = 1.0 - u;
            out.push(pos2(
                v * v * start.x + 2.0 * v * u * b.x + u * u * end.x,
                v * v * start.y + 2.0 * v * u * b.y + u * u * end.y,
            ));
        }
    }
    out.push(*pts.last().unwrap());
    out
}

/// Distance from `p` to the segment `a`–`b`.
pub fn dist_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len_sq = ab.length_sq();
    if len_sq == 0.0 {
        return (p - a).length();
    }
    let u = ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    (p - (a + ab * u)).length()
}

/// Distance from `p` to a polyline.
pub fn dist_to_path(p: Pos2, path: &[Pos2]) -> f32 {
    path.windows(2)
        .map(|w| dist_to_segment(p, w[0], w[1]))
        .fold(f32::INFINITY, f32::min)
}

/// Point `frac` (0..=1) of the way along the polyline, by length.
pub fn point_at(path: &[Pos2], frac: f32) -> Pos2 {
    let total: f32 = path.windows(2).map(|w| (w[1] - w[0]).length()).sum();
    let mut left = total * frac.clamp(0.0, 1.0);
    for w in path.windows(2) {
        let len = (w[1] - w[0]).length();
        if left <= len && len > 0.0 {
            return w[0] + (w[1] - w[0]) * (left / len);
        }
        left -= len;
    }
    path.last().copied().unwrap_or(Pos2::ZERO)
}

/// Direction of the final segment, for orienting arrowheads.
pub fn end_direction(path: &[Pos2]) -> Vec2 {
    path.windows(2)
        .rev()
        .map(|w| w[1] - w[0])
        .find(|d| d.length_sq() > 1e-6)
        .map(|d| d.normalized())
        .unwrap_or(Vec2::X)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bezier_hits_endpoints() {
        let (s, t) = (pos2(0.0, 0.0), pos2(200.0, 100.0));
        let p = edge_path(EdgeKind::Bezier, s, Side::Right, t, Side::Left);
        assert_eq!(p.first().copied(), Some(s));
        assert!((*p.last().unwrap() - t).length() < 1e-3);
    }

    #[test]
    fn step_is_axis_aligned() {
        let p = edge_path(
            EdgeKind::Step,
            pos2(0.0, 0.0),
            Side::Right,
            pos2(200.0, 100.0),
            Side::Left,
        );
        for w in p.windows(2) {
            assert!(w[0].x == w[1].x || w[0].y == w[1].y, "{w:?}");
        }
    }

    #[test]
    fn smoothstep_keeps_endpoints() {
        let (s, t) = (pos2(0.0, 0.0), pos2(-50.0, 100.0));
        let p = edge_path(EdgeKind::SmoothStep, s, Side::Right, t, Side::Left);
        assert_eq!(p[0], s);
        assert_eq!(*p.last().unwrap(), t);
    }

    #[test]
    fn segment_distance() {
        assert_eq!(
            dist_to_segment(pos2(5.0, 3.0), pos2(0.0, 0.0), pos2(10.0, 0.0)),
            3.0
        );
        assert_eq!(
            dist_to_segment(pos2(-4.0, 3.0), pos2(0.0, 0.0), pos2(10.0, 0.0)),
            5.0
        );
    }

    #[test]
    fn point_at_walks_the_path() {
        let p = [pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(10.0, 10.0)];
        assert_eq!(point_at(&p, 0.0), pos2(0.0, 0.0));
        assert_eq!(point_at(&p, 0.25), pos2(5.0, 0.0));
        assert_eq!(point_at(&p, 0.75), pos2(10.0, 5.0));
        assert_eq!(point_at(&p, 1.0), pos2(10.0, 10.0));
    }

    #[test]
    fn midpoint_of_line() {
        let m = point_at(&[pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(10.0, 10.0)], 0.5);
        assert_eq!(m, pos2(10.0, 0.0));
    }
}

/// Direction of travel at the start of a path.
pub fn start_direction(path: &[Pos2]) -> Vec2 {
    path.windows(2)
        .map(|w| w[1] - w[0])
        .find(|d| d.length_sq() > 1e-6)
        .map(|d| d.normalized())
        .unwrap_or(Vec2::X)
}

/// A line to draw while dragging nodes: a vertical one at `x = at` or a
/// horizontal one at `y = at`, spanning `from..to` along the other axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Guide {
    pub vertical: bool,
    pub at: f32,
    pub from: f32,
    pub to: f32,
}

/// Offset that snaps `group` onto the closest edge/centre line of any of
/// `others` within `threshold`, per axis, plus the guide lines to draw.
pub(crate) fn align_to(group: Rect, others: &[Rect], threshold: f32) -> (Vec2, Vec<Guide>) {
    let lines = |lo: f32, mid: f32, hi: f32| [lo, mid, hi];
    let mut best: [Option<(f32, f32, Rect)>; 2] = [None, None];
    for other in others {
        let mine = [
            lines(group.min.x, group.center().x, group.max.x),
            lines(group.min.y, group.center().y, group.max.y),
        ];
        let theirs = [
            lines(other.min.x, other.center().x, other.max.x),
            lines(other.min.y, other.center().y, other.max.y),
        ];
        for axis in 0..2 {
            for m in mine[axis] {
                for t in theirs[axis] {
                    let d = t - m;
                    if d.abs() <= threshold
                        && best[axis].is_none_or(|(bd, _, _)| d.abs() < bd.abs())
                    {
                        best[axis] = Some((d, t, *other));
                    }
                }
            }
        }
    }
    let off = vec2(best[0].map_or(0.0, |b| b.0), best[1].map_or(0.0, |b| b.0));
    let moved = group.translate(off);
    let mut guides = Vec::new();
    if let Some((_, at, other)) = best[0] {
        guides.push(Guide {
            vertical: true,
            at,
            from: moved.min.y.min(other.min.y),
            to: moved.max.y.max(other.max.y),
        });
    }
    if let Some((_, at, other)) = best[1] {
        guides.push(Guide {
            vertical: false,
            at,
            from: moved.min.x.min(other.min.x),
            to: moved.max.x.max(other.max.x),
        });
    }
    (off, guides)
}

#[cfg(test)]
mod guide_tests {
    use super::*;
    use egui::pos2;

    #[test]
    fn snaps_to_nearest_edge_within_threshold() {
        let other = Rect::from_min_size(pos2(100.0, 0.0), vec2(50.0, 50.0));
        // Left edge 3 units short of the other's left edge; top far away.
        let group = Rect::from_min_size(pos2(97.0, 200.0), vec2(40.0, 30.0));
        let (off, guides) = align_to(group, &[other], 6.0);
        assert_eq!(off, vec2(3.0, 0.0));
        assert_eq!(guides.len(), 1);
        assert!(guides[0].vertical && guides[0].at == 100.0);
        assert_eq!((guides[0].from, guides[0].to), (0.0, 230.0));
    }

    #[test]
    fn aligns_centres_and_ignores_far_nodes() {
        let other = Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 100.0));
        let group = Rect::from_min_size(pos2(500.0, 44.0), vec2(20.0, 20.0)); // centre y = 54
        let (off, guides) = align_to(group, &[other], 6.0);
        assert_eq!(off.x, 0.0, "nothing aligns horizontally");
        assert_eq!(
            off.y, -4.0,
            "centre 54 -> other's centre 50 (closer than any edge)"
        );
        assert!(guides.iter().all(|g| !g.vertical));
        let (off, guides) = align_to(group, &[], 6.0);
        assert_eq!(off, Vec2::ZERO);
        assert!(guides.is_empty());
    }
}

#[cfg(test)]
mod route_tests {
    use super::*;

    fn hits(path: &[Pos2], r: Rect) -> bool {
        path.windows(2).any(|w| segment_blocked(w[0], w[1], &r))
    }

    fn orthogonal_path(path: &[Pos2]) -> bool {
        path.windows(2)
            .all(|w| (w[0].x - w[1].x).abs() < 1e-3 || (w[0].y - w[1].y).abs() < 1e-3)
    }

    const S: Pos2 = pos2(0.0, 100.0);
    const T: Pos2 = pos2(400.0, 100.0);

    #[test]
    fn a_step_edge_goes_round_a_node_in_its_way() {
        // A bus bar straddling the straight run between the two ends.
        let bar = Rect::from_min_size(pos2(150.0, 60.0), vec2(100.0, 80.0));
        let plain = edge_path(EdgeKind::Step, S, Side::Right, T, Side::Left);
        assert!(hits(&plain, bar), "the plain route really crosses it");
        for kind in [EdgeKind::Step, EdgeKind::SmoothStep] {
            let p = edge_path_around(kind, S, Side::Right, T, Side::Left, &[bar]);
            assert!(!hits(&p, bar), "{kind:?} avoids the node: {p:?}");
            assert_eq!(p.first().copied(), Some(S));
            assert_eq!(p.last().copied(), Some(T));
        }
        let p = edge_path_around(EdgeKind::Step, S, Side::Right, T, Side::Left, &[bar]);
        assert!(orthogonal_path(&p), "{p:?}");
    }

    #[test]
    fn the_detour_keeps_a_margin_and_is_the_shorter_way_round() {
        let bar = Rect::from_min_size(pos2(150.0, 90.0), vec2(100.0, 30.0));
        let p = edge_path_around(EdgeKind::Step, S, Side::Right, T, Side::Left, &[bar]);
        assert!(!hits(&p, bar.expand(AVOID_MARGIN - 1.0)), "{p:?}");
        // Going over the top (y = 76) is shorter than under (y = 134).
        assert!(p.iter().any(|q| q.y < bar.min.y), "{p:?}");
    }

    #[test]
    fn edges_that_cross_nothing_are_left_alone() {
        let far = Rect::from_min_size(pos2(150.0, 300.0), vec2(100.0, 30.0));
        for kind in [EdgeKind::Step, EdgeKind::SmoothStep] {
            assert_eq!(
                edge_path_around(kind, S, Side::Right, T, Side::Left, &[far]),
                edge_path(kind, S, Side::Right, T, Side::Left)
            );
        }
        assert_eq!(
            edge_path_around(EdgeKind::Step, S, Side::Right, T, Side::Left, &[]),
            edge_path(EdgeKind::Step, S, Side::Right, T, Side::Left)
        );
    }

    #[test]
    fn bezier_and_straight_edges_ignore_obstacles() {
        let bar = Rect::from_min_size(pos2(150.0, 60.0), vec2(100.0, 80.0));
        for kind in [EdgeKind::Bezier, EdgeKind::Straight] {
            assert_eq!(
                edge_path_around(kind, S, Side::Right, T, Side::Left, &[bar]),
                edge_path(kind, S, Side::Right, T, Side::Left)
            );
        }
    }

    #[test]
    fn it_goes_round_a_wall_of_nodes_and_gives_up_politely_when_boxed_in() {
        // Two tall nodes side by side: the route must climb over both.
        let wall = [
            Rect::from_min_size(pos2(120.0, 20.0), vec2(60.0, 160.0)),
            Rect::from_min_size(pos2(220.0, 20.0), vec2(60.0, 160.0)),
        ];
        let p = edge_path_around(EdgeKind::Step, S, Side::Right, T, Side::Left, &wall);
        assert!(wall.iter().all(|w| !hits(&p, *w)), "{p:?}");
        assert!(orthogonal_path(&p));

        // The target's own approach is walled in: nothing clear exists, so keep the plain route.
        let cell = Rect::from_center_size(T, vec2(200.0, 200.0));
        let p = edge_path_around(EdgeKind::Step, S, Side::Right, T, Side::Left, &[cell]);
        assert_eq!(p, edge_path(EdgeKind::Step, S, Side::Right, T, Side::Left));
    }
}
