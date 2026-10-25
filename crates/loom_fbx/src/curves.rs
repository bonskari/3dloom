//! Curve filters for baked (linear) animation curves, as the FBX dialogs
//! apply them on export and import: the Constant Key Reducer drops keys
//! inside flat runs, the Key Reducer drops keys linear interpolation of
//! their neighbours reproduces within a precision.

/// The indices of the keys that stay. `constant`: the Constant Key
/// Reducer threshold (a key whose value is within it of both neighbours'
/// goes); `reducer`: the Key Reducer precision (a key the line between the
/// kept keys around it passes within it goes). The first and last keys
/// always stay, except that a curve that never changes keeps one key.
pub fn reduce(times: &[f32], values: &[f32], constant: Option<f32>, reducer: Option<f32>) -> Vec<usize> {
    let n = values.len().min(times.len());
    if n <= 2 {
        let all: Vec<usize> = (0..n).collect();
        return match constant {
            Some(eps) if n == 2 && (values[1] - values[0]).abs() <= eps => vec![0],
            _ => all,
        };
    }
    let mut kept: Vec<usize> = (0..n).collect();
    if let Some(eps) = constant {
        let eps = eps.max(0.0);
        let mut out = vec![0];
        for i in 1..n - 1 {
            let flat = (values[i] - values[i - 1]).abs() <= eps && (values[i + 1] - values[i]).abs() <= eps;
            if !flat {
                out.push(i);
            }
        }
        out.push(n - 1);
        if values.iter().all(|v| (v - values[0]).abs() <= eps) {
            out = vec![0];
        }
        kept = out;
    }
    if let Some(eps) = reducer {
        if kept.len() > 2 {
            let eps = eps.max(0.0);
            let mut out = vec![kept[0]];
            let mut a = 0usize; // index into `kept`
            while a + 1 < kept.len() {
                // The farthest kept key b the line a → b covers every key in between.
                let mut b = a + 1;
                while b + 1 < kept.len() {
                    let (i0, i1) = (kept[a], kept[b + 1]);
                    let fits = (i0 + 1..i1).all(|k| {
                        let t = (times[k] - times[i0]) / (times[i1] - times[i0]).max(1e-9);
                        let line = values[i0] + (values[i1] - values[i0]) * t;
                        (line - values[k]).abs() <= eps
                    });
                    if !fits {
                        break;
                    }
                    b += 1;
                }
                out.push(kept[b]);
                a = b;
            }
            kept = out;
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::reduce;

    #[test]
    fn constant_reducer_keeps_the_ends_of_flat_runs() {
        let t: Vec<f32> = (0..7).map(|i| i as f32).collect();
        let v = [0.0, 0.0, 0.0, 1.0, 2.0, 2.0, 2.0];
        assert_eq!(reduce(&t, &v, Some(1e-4), None), vec![0, 2, 3, 4, 6]);
        assert_eq!(reduce(&t, &[5.0; 7], Some(1e-4), None), vec![0], "a flat curve keeps one key");
        assert_eq!(reduce(&t, &v, None, None), (0..7).collect::<Vec<_>>());
    }

    #[test]
    fn key_reducer_keeps_the_corners_of_a_linear_curve() {
        let t: Vec<f32> = (0..9).map(|i| i as f32).collect();
        // Up linearly to frame 4, down linearly to frame 8.
        let v: Vec<f32> = t.iter().map(|x| if *x <= 4.0 { *x * 2.0 } else { 16.0 - *x * 2.0 }).collect();
        assert_eq!(reduce(&t, &v, None, Some(1e-3)), vec![0, 4, 8]);
        // A bump smaller than the precision goes too.
        let mut w = v.clone();
        w[2] += 0.5;
        assert_eq!(reduce(&t, &w, None, Some(1.0)), vec![0, 4, 8]);
        assert!(reduce(&t, &w, None, Some(0.1)).contains(&2));
    }
}
