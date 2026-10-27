//! Candidate exact parallel recursion for the original edge quicksort.
//! Initially compiled only by tests; not connected to production preparation.
use super::{QuickSortEdges, SEdge};

pub(super) fn sort(edges: &mut [SEdge], channel: usize, mut seed: u32, pool: &bevy_tasks::TaskPool, depth: usize) {
    if edges.len() < 8192 || depth == 0 || pool.thread_num() < 2 {
        // The original routine handles empty/single/two-element ranges and
        // supplies the exact tie order at each serial leaf.
        unsafe { QuickSortEdges(edges.as_mut_ptr(), 0, edges.len() as i32 - 1, channel as i32, seed); }
        return;
    }
    let rotate = seed & 31;
    let rotate = seed.rotate_left(rotate) | seed.rotate_right(32u32.wrapping_sub(rotate));
    seed = seed.wrapping_add(rotate).wrapping_add(3);
    let pivot = unsafe { edges[(seed % edges.len() as u32) as usize].array[channel] };
    let mut left = 0i32;
    let mut right = edges.len() as i32 - 1;
    loop {
        while unsafe { edges[left as usize].array[channel] } < pivot { left += 1; }
        while unsafe { edges[right as usize].array[channel] } > pivot { right -= 1; }
        if left <= right {
            edges.swap(left as usize, right as usize);
            left += 1;
            right -= 1;
        }
        if left > right { break; }
    }
    let (prefix, right_edges) = edges.split_at_mut(left as usize);
    let left_edges = &mut prefix[..(right + 1) as usize];
    // Parent partition and seed are identical to upstream. These disjoint
    // recursive ranges cannot change one another's values or tie ordering.
    pool.scope(|scope| {
        scope.spawn(async { sort(left_edges, channel, seed, pool, depth - 1); });
        scope.spawn(async { sort(right_edges, channel, seed, pool, depth - 1); });
    });
}

#[test]
fn original_edge_order_including_ties_is_preserved() {
    let pool = bevy_tasks::TaskPoolBuilder::new().num_threads(4).build();
    for count in [0, 1, 2, 3, 31, 8192, 18001] {
        for channel in 0..3 {
            for seed in [0u32, 39871946, u32::MAX] {
                let mut random = 17u32;
                let data: Vec<_> = (0..count).map(|i| {
                    random = random.wrapping_mul(1664525).wrapping_add(1013904223);
                    SEdge { array: [(random % 17) as i32, i as i32, (random % 13) as i32] }
                }).collect();
                let mut reference = data.clone();
                let mut actual = data;
                unsafe { QuickSortEdges(reference.as_mut_ptr(), 0, count as i32 - 1, channel as i32, seed); }
                sort(&mut actual, channel, seed, &pool, 3);
                let values = |data: &[SEdge]| data.iter().map(|edge| unsafe { edge.array }).collect::<Vec<_>>();
                assert_eq!(values(&actual), values(&reference), "count={count}, channel={channel}, seed={seed}");
            }
        }
    }
}
