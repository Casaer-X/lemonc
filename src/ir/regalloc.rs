use std::collections::{BinaryHeap, HashMap};

#[derive(Debug, Clone)]
pub struct LiveInterval {
    pub vreg: u32,
    pub start: u32,
    pub end: u32,
}

impl PartialEq for LiveInterval {
    fn eq(&self, other: &Self) -> bool {
        self.vreg == other.vreg
    }
}

impl Eq for LiveInterval {}

impl PartialOrd for LiveInterval {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.end.cmp(&other.end))
    }
}

impl Ord for LiveInterval {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.end.cmp(&other.end)
    }
}

pub struct LinearScanAllocator {
    num_registers: usize,
    assignments: HashMap<u32, u32>,
    spill_slots: HashMap<u32, u32>,
    register_available: Vec<bool>,
}

impl LinearScanAllocator {
    pub fn new() -> Self {
        let num_registers = 14usize;
        Self {
            num_registers,
            assignments: HashMap::new(),
            spill_slots: HashMap::new(),
            register_available: vec![true; num_registers],
        }
    }

    pub fn allocate(&mut self, intervals: Vec<LiveInterval>) -> &HashMap<u32, u32> {
        let mut sorted = intervals;
        sorted.sort_by(|a, b| a.start.cmp(&b.start));

        let mut active: BinaryHeap<LiveInterval> = BinaryHeap::new();

        for interval in sorted {
            self.expire_old_intervals(&mut active, interval.start);

            if active.len() >= self.num_registers {
                self.spill_at_interval(&mut active, &interval);
            } else {
                self.assign_register(&mut active, &interval);
            }
        }

        &self.assignments
    }

    fn expire_old_intervals(&mut self, active: &mut BinaryHeap<LiveInterval>, current_pos: u32) {
        let mut expired = Vec::new();
        let remaining: Vec<LiveInterval> = active.drain().collect();
        for iv in remaining {
            if iv.end < current_pos {
                if let Some(&preg) = self.assignments.get(&iv.vreg) {
                    self.register_available[preg as usize] = true;
                }
                expired.push(iv);
            } else {
                active.push(iv);
            }
        }
    }

    fn assign_register(&mut self, active: &mut BinaryHeap<LiveInterval>, interval: &LiveInterval) {
        for (i, available) in self.register_available.iter_mut().enumerate() {
            if *available {
                *available = false;
                self.assignments.insert(interval.vreg, i as u32);
                active.push(interval.clone());
                return;
            }
        }
    }

    fn spill_at_interval(&mut self, active: &mut BinaryHeap<LiveInterval>, interval: &LiveInterval) {
        if let Some(spill_vreg) = self.find_spill_candidate(active) {
            let slot = self.spill_slots.len() as u32;
            self.spill_slots.insert(spill_vreg, slot);
            if let Some(&preg) = self.assignments.get(&spill_vreg) {
                self.register_available[preg as usize] = true;
            }
            self.assign_register(active, interval);
        } else {
            let slot = self.spill_slots.len() as u32;
            self.spill_slots.insert(interval.vreg, slot);
        }
    }

    fn find_spill_candidate(&self, active: &BinaryHeap<LiveInterval>) -> Option<u32> {
        active.iter().max_by_key(|iv| iv.end).map(|iv| iv.vreg)
    }

    pub fn get_assignment(&self, vreg: u32) -> Option<u32> {
        self.assignments.get(&vreg).copied()
    }

    pub fn get_spill_slot(&self, vreg: u32) -> Option<u32> {
        self.spill_slots.get(&vreg).copied()
    }
}
