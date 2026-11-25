use crate::git::Commit;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Span {
    Top,
    Middle,
    Bottom,
}

#[derive(Clone, Copy, Debug)]
pub struct Rail {
    pub from: usize,
    pub to: usize,
    pub start: Span,
    pub end: Span,
    pub color: usize,
}

#[derive(Clone, Debug)]
pub struct GraphRow {
    pub lane: usize,
    pub color: usize,
    pub rails: Vec<Rail>,
}

#[derive(Default)]
pub struct Graph {
    pub rows: Vec<GraphRow>,
    pub lanes: usize,
}

#[derive(Clone)]
struct Pending<'a> {
    id: &'a str,
    color: usize,
}

impl Graph {
    pub fn build(commits: &[Commit]) -> Self {
        let mut graph = Self::default();
        let mut pending: Vec<Option<Pending<'_>>> = Vec::new();
        let mut color = 0;
        let mut next_color = || {
            let value = color;
            color += 1;
            value
        };

        for commit in commits {
            let incoming = pending.clone();
            let existing = occupied_by(&pending, &commit.id);
            let lane = existing.unwrap_or_else(|| free_lane(&pending, 0));
            let dot_color = existing
                .and_then(|index| pending[index].take())
                .map_or_else(&mut next_color, |entry| entry.color);

            let mut rails = Vec::new();
            for (index, entry) in incoming.iter().enumerate() {
                if let Some(entry) = entry {
                    rails.push(Rail {
                        from: index,
                        to: index,
                        start: Span::Top,
                        end: if index == lane {
                            Span::Middle
                        } else {
                            Span::Bottom
                        },
                        color: entry.color,
                    });
                }
            }
            for (index, parent) in commit.parents.iter().enumerate() {
                let to = occupied_by(&pending, parent).unwrap_or_else(|| {
                    let to = if index == 0 {
                        lane
                    } else {
                        free_lane(&pending, lane + 1)
                    };
                    let rail_color = if index == 0 { dot_color } else { next_color() };
                    if pending.len() <= to {
                        pending.resize(to + 1, None);
                    }
                    pending[to] = Some(Pending {
                        id: parent,
                        color: rail_color,
                    });
                    to
                });
                rails.push(Rail {
                    from: lane,
                    to,
                    start: Span::Middle,
                    end: Span::Bottom,
                    color: pending[to].as_ref().expect("allocated lane").color,
                });
            }
            while pending.last().is_some_and(Option::is_none) {
                pending.pop();
            }
            graph.lanes = graph.lanes.max(lane + 1).max(pending.len());
            graph.rows.push(GraphRow {
                lane,
                color: dot_color,
                rails,
            });
        }
        graph
    }
}

fn occupied_by(slots: &[Option<Pending<'_>>], id: &str) -> Option<usize> {
    slots
        .iter()
        .position(|entry| entry.as_ref().is_some_and(|entry| entry.id == id))
}

fn free_lane(slots: &[Option<Pending<'_>>], start: usize) -> usize {
    (start..slots.len())
        .find(|&index| slots[index].is_none())
        .or_else(|| (0..start.min(slots.len())).find(|&index| slots[index].is_none()))
        .unwrap_or(slots.len().max(start))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn commit(id: &str, parents: &[&str]) -> Commit {
        Commit {
            id: id.into(),
            parents: parents.iter().map(|p| (*p).into()).collect(),
            short: id.into(),
            author: String::new(),
            date: String::new(),
            subject: String::new(),
        }
    }

    #[test]
    fn merge_retains_two_lanes_until_the_paths_meet() {
        let graph = Graph::build(&[
            commit("merge", &["main", "topic"]),
            commit("main", &["base"]),
            commit("topic", &["base"]),
            commit("base", &[]),
        ]);
        assert_eq!(graph.lanes, 2);
        assert_eq!(
            graph.rows[0]
                .rails
                .iter()
                .filter(|rail| rail.start == Span::Middle)
                .count(),
            2
        );
        assert_eq!(graph.rows[1].lane, 0);
        assert_eq!(graph.rows[2].lane, 1);
        assert!(
            graph.rows[2]
                .rails
                .iter()
                .any(|rail| rail.from == 1 && rail.to == 0)
        );
        assert_eq!(graph.rows[3].lane, 0);
    }

    #[test]
    fn octopus_and_unrelated_tips_keep_each_parent_distinct() {
        let graph = Graph::build(&[
            commit("merge", &["a", "b", "c"]),
            commit("unrelated", &[]),
            commit("c", &[]),
            commit("b", &[]),
            commit("a", &[]),
        ]);
        assert_eq!(graph.lanes, 4);
        assert_eq!(graph.rows[0].rails.len(), 3);
        assert_eq!(graph.rows[1].lane, 3);
        assert_eq!(graph.rows[2].lane, 2);
    }
}
