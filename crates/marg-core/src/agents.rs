//! Struct-of-arrays agent store (plan/02-architecture.md §2).
//!
//! Agents are kept densely; exits are removed by a stable compaction at the
//! end of each step, so the store's order is deterministic.

/// Column-oriented storage of all vehicles in the network.
#[derive(Debug, Default, Clone)]
pub struct AgentStore {
    pub id: Vec<u64>,
    pub vtype: Vec<u8>,
    pub class: Vec<u8>,
    pub entry: Vec<u16>,
    pub ribbon: Vec<u32>,
    pub s: Vec<f64>,
    pub s_prev: Vec<f64>,
    pub y: Vec<f64>,
    pub v: Vec<f64>,
    pub acc: Vec<f64>,
    pub length: Vec<f64>,
    pub width: Vec<f64>,
    pub v0: Vec<f64>,
    pub time_gap: Vec<f64>,
    pub standstill_gap: Vec<f64>,
    pub max_accel: Vec<f64>,
    pub comfortable_decel: Vec<f64>,
    pub t_arrival: Vec<f64>,
    pub t_entry: Vec<f64>,
    pub entry_y: Vec<f64>,
    pub distance: Vec<f64>,
    pub stops: Vec<u32>,
    pub stop_armed: Vec<bool>,
    /// Index of the nearest physically overlapping leader, or `u32::MAX`.
    pub phys_leader: Vec<u32>,
    /// Set during the step when the agent leaves the network.
    pub exited: Vec<bool>,
}

/// Everything needed to add one agent.
#[derive(Debug, Clone, Copy)]
pub struct NewAgent {
    pub id: u64,
    pub vtype: u8,
    pub class: u8,
    pub entry: u16,
    pub ribbon: u32,
    pub s: f64,
    pub y: f64,
    pub v: f64,
    pub length: f64,
    pub width: f64,
    pub v0: f64,
    pub time_gap: f64,
    pub standstill_gap: f64,
    pub max_accel: f64,
    pub comfortable_decel: f64,
    pub t_arrival: f64,
    pub t_entry: f64,
}

/// Marker for a removed agent in a remap table.
pub const REMOVED: u32 = u32::MAX;

macro_rules! for_each_column {
    ($self:ident, $f:ident $(, $arg:expr)*) => {{
        $self.id.$f($($arg),*);
        $self.vtype.$f($($arg),*);
        $self.class.$f($($arg),*);
        $self.entry.$f($($arg),*);
        $self.ribbon.$f($($arg),*);
        $self.s.$f($($arg),*);
        $self.s_prev.$f($($arg),*);
        $self.y.$f($($arg),*);
        $self.v.$f($($arg),*);
        $self.acc.$f($($arg),*);
        $self.length.$f($($arg),*);
        $self.width.$f($($arg),*);
        $self.v0.$f($($arg),*);
        $self.time_gap.$f($($arg),*);
        $self.standstill_gap.$f($($arg),*);
        $self.max_accel.$f($($arg),*);
        $self.comfortable_decel.$f($($arg),*);
        $self.t_arrival.$f($($arg),*);
        $self.t_entry.$f($($arg),*);
        $self.entry_y.$f($($arg),*);
        $self.distance.$f($($arg),*);
        $self.stops.$f($($arg),*);
        $self.stop_armed.$f($($arg),*);
        $self.phys_leader.$f($($arg),*);
        $self.exited.$f($($arg),*);
    }};
}

impl AgentStore {
    pub fn len(&self) -> usize {
        self.id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.id.is_empty()
    }

    /// Append an agent; returns its index.
    pub fn push(&mut self, a: NewAgent) -> u32 {
        let i = self.len() as u32;
        self.id.push(a.id);
        self.vtype.push(a.vtype);
        self.class.push(a.class);
        self.entry.push(a.entry);
        self.ribbon.push(a.ribbon);
        self.s.push(a.s);
        self.s_prev.push(a.s);
        self.y.push(a.y);
        self.v.push(a.v);
        self.acc.push(0.0);
        self.length.push(a.length);
        self.width.push(a.width);
        self.v0.push(a.v0);
        self.time_gap.push(a.time_gap);
        self.standstill_gap.push(a.standstill_gap);
        self.max_accel.push(a.max_accel);
        self.comfortable_decel.push(a.comfortable_decel);
        self.t_arrival.push(a.t_arrival);
        self.t_entry.push(a.t_entry);
        self.entry_y.push(a.y);
        self.distance.push(0.0);
        self.stops.push(0);
        self.stop_armed.push(true);
        self.phys_leader.push(REMOVED);
        self.exited.push(false);
        i
    }

    /// Remove every agent with `exited` set, keeping the others in order.
    /// Writes `remap[old] = new index or REMOVED` and returns whether anything moved.
    pub fn compact(&mut self, remap: &mut Vec<u32>) -> bool {
        let n = self.len();
        remap.clear();
        remap.reserve(n);
        let mut next = 0u32;
        for i in 0..n {
            if self.exited[i] {
                remap.push(REMOVED);
            } else {
                remap.push(next);
                next += 1;
            }
        }
        if next as usize == n {
            return false;
        }
        let keep: Vec<bool> = self.exited.iter().map(|e| !e).collect();
        macro_rules! retain_col {
            ($col:expr) => {{
                let mut k = keep.iter();
                $col.retain(|_| *k.next().unwrap());
            }};
        }
        retain_col!(self.id);
        retain_col!(self.vtype);
        retain_col!(self.class);
        retain_col!(self.entry);
        retain_col!(self.ribbon);
        retain_col!(self.s);
        retain_col!(self.s_prev);
        retain_col!(self.y);
        retain_col!(self.v);
        retain_col!(self.acc);
        retain_col!(self.length);
        retain_col!(self.width);
        retain_col!(self.v0);
        retain_col!(self.time_gap);
        retain_col!(self.standstill_gap);
        retain_col!(self.max_accel);
        retain_col!(self.comfortable_decel);
        retain_col!(self.t_arrival);
        retain_col!(self.t_entry);
        retain_col!(self.entry_y);
        retain_col!(self.distance);
        retain_col!(self.stops);
        retain_col!(self.stop_armed);
        retain_col!(self.phys_leader);
        retain_col!(self.exited);
        true
    }

    /// Drop all agents.
    pub fn clear(&mut self) {
        for_each_column!(self, clear);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: u64, s: f64) -> NewAgent {
        NewAgent {
            id,
            vtype: 0,
            class: 0,
            entry: 0,
            ribbon: 0,
            s,
            y: 0.0,
            v: 1.0,
            length: 4.0,
            width: 1.8,
            v0: 10.0,
            time_gap: 1.0,
            standstill_gap: 2.0,
            max_accel: 1.0,
            comfortable_decel: 1.0,
            t_arrival: 0.0,
            t_entry: 0.0,
        }
    }

    #[test]
    fn compaction_is_stable() {
        let mut st = AgentStore::default();
        for i in 0..6 {
            st.push(agent(i, i as f64));
        }
        st.exited[1] = true;
        st.exited[4] = true;
        let mut remap = Vec::new();
        assert!(st.compact(&mut remap));
        assert_eq!(st.id, vec![0, 2, 3, 5]);
        assert_eq!(st.s, vec![0.0, 2.0, 3.0, 5.0]);
        assert_eq!(remap, vec![0, REMOVED, 1, 2, REMOVED, 3]);
        assert_eq!(st.exited.len(), 4);
    }

    #[test]
    fn many_cycles_do_not_leak() {
        let mut st = AgentStore::default();
        let mut remap = Vec::new();
        for round in 0..1000u64 {
            for k in 0..10 {
                st.push(agent(round * 10 + k, 0.0));
            }
            for e in st.exited.iter_mut() {
                *e = true;
            }
            st.compact(&mut remap);
            assert!(st.is_empty());
        }
        st.clear();
        assert!(st.id.capacity() < 1000);
    }
}
