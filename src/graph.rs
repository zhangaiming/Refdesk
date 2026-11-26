use crate::git::Commit;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum Span {
    Top,
    Middle,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Rail {
    pub from: usize,
    pub to: usize,
    pub start: Span,
    pub end: Span,
    pub color: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct GraphRow {
    pub lane: usize,
    pub color: usize,
    pub rails: Vec<Rail>,
}

#[cfg(test)]
pub struct Graph {
    pub rows: Vec<GraphRow>,
    pub lanes: usize,
}

#[derive(Clone)]
struct Pending {
    id: String,
    color: usize,
}

#[cfg(test)]
impl Graph {
    pub fn build(commits: &[Commit]) -> Self {
        let mut builder = GraphBuilder::default();
        let rows = commits.iter().map(|commit| builder.push(commit)).collect();
        Self {
            rows,
            lanes: builder.lanes,
        }
    }
}

#[derive(Clone, Default)]
pub struct GraphBuilder {
    pending: Vec<Option<Pending>>,
    color: usize,
    pub lanes: usize,
}

impl GraphBuilder {
    pub fn push(&mut self, commit: &Commit) -> GraphRow {
        let existing = occupied_by(&self.pending, &commit.id);
        let lane = existing.unwrap_or_else(|| free_lane(&self.pending, 0));
        let mut rails = Vec::new();
        for (index, entry) in self.pending.iter().enumerate() {
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

        let dot_color = existing
            .and_then(|index| self.pending[index].take())
            .map_or_else(|| self.next_color(), |entry| entry.color);
        for (index, parent) in commit.parents.iter().enumerate() {
            let to = occupied_by(&self.pending, parent).unwrap_or_else(|| {
                let to = if index == 0 {
                    lane
                } else {
                    free_lane(&self.pending, lane + 1)
                };
                let color = if index == 0 {
                    dot_color
                } else {
                    self.next_color()
                };
                if self.pending.len() <= to {
                    self.pending.resize(to + 1, None);
                }
                self.pending[to] = Some(Pending {
                    id: parent.clone(),
                    color,
                });
                to
            });
            rails.push(Rail {
                from: lane,
                to,
                start: Span::Middle,
                end: Span::Bottom,
                color: self.pending[to].as_ref().expect("allocated lane").color,
            });
        }
        while self.pending.last().is_some_and(Option::is_none) {
            self.pending.pop();
        }
        self.lanes = self.lanes.max(lane + 1).max(self.pending.len());
        GraphRow {
            lane,
            color: dot_color,
            rails,
        }
    }

    fn next_color(&mut self) -> usize {
        let color = self.color;
        self.color += 1;
        color
    }
}

fn occupied_by(slots: &[Option<Pending>], id: &str) -> Option<usize> {
    slots
        .iter()
        .position(|entry| entry.as_ref().is_some_and(|entry| entry.id == id))
}

fn free_lane(slots: &[Option<Pending>], start: usize) -> usize {
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
