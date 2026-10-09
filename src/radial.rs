//! The angles of a radial menu's items, shared by aiming and drawing so they always agree.

/// The smallest share a weight counts for, so every item keeps an arc you can aim at.
const MIN_WEIGHT: f32 = 0.25;

/// Each item's arc in degrees clockwise from straight up, as `(start, end)`. Arcs are sized by
/// their weights and fill the whole circle; the first is centred on up.
pub fn arcs(weights: &[f32]) -> Vec<(f32, f32)> {
    if weights.is_empty() {
        return Vec::new();
    }
    let weights: Vec<f32> = weights
        .iter()
        .map(|w| {
            if w.is_finite() {
                w.max(MIN_WEIGHT)
            } else {
                1.0
            }
        })
        .collect();
    let total: f32 = weights.iter().sum();
    let degrees = |w: f32| w / total * 360.0;
    let mut at = -degrees(weights[0]) / 2.0;
    weights
        .iter()
        .map(|&w| {
            let start = at;
            at += degrees(w);
            (start, at)
        })
        .collect()
}

/// The arcs of a radial menu's items: weighted, or equal ones for a menu that uses boxes.
pub fn for_menu(boxes: bool, weights: &[f32]) -> Vec<(f32, f32)> {
    if boxes {
        arcs(&vec![1.0; weights.len()])
    } else {
        arcs(weights)
    }
}

/// The item whose arc holds `degrees` (clockwise from up, any value), if there are any arcs.
pub fn index_at(arcs: &[(f32, f32)], degrees: f32) -> Option<usize> {
    let &(first, _) = arcs.first()?;
    let angle = (degrees - first).rem_euclid(360.0) + first;
    arcs.iter()
        .position(|&(start, end)| angle >= start && angle < end)
        .or(Some(arcs.len() - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_weights_give_equal_arcs_centred_on_up() {
        let arcs = arcs(&[1.0; 4]);
        assert_eq!(arcs.len(), 4);
        assert!(arcs.iter().all(|(s, e)| (e - s - 90.0).abs() < 1e-4));
        assert!((arcs[0].0 + 45.0).abs() < 1e-4);
    }

    #[test]
    fn equal_weights_pick_the_same_item_as_equal_slices() {
        let arcs = arcs(&[1.0; 5]);
        let slice = 360.0 / 5.0;
        for step in 0..720 {
            let angle = step as f32 * 0.5;
            let old = ((angle + slice / 2.0) / slice) as usize % 5;
            assert_eq!(index_at(&arcs, angle), Some(old), "at {angle}");
        }
    }

    #[test]
    fn weights_size_the_arcs_and_cover_the_circle() {
        let arcs = arcs(&[3.0, 1.0]);
        assert!((arcs[0].1 - arcs[0].0 - 270.0).abs() < 1e-3);
        assert!((arcs[1].1 - arcs[1].0 - 90.0).abs() < 1e-3);
        assert!((arcs[1].1 - arcs[0].0 - 360.0).abs() < 1e-3);
        assert_eq!(index_at(&arcs, 0.0), Some(0));
        assert_eq!(index_at(&arcs, 180.0), Some(1));
    }

    #[test]
    fn no_items_no_arcs_and_no_pick() {
        assert!(arcs(&[]).is_empty());
        assert_eq!(index_at(&[], 10.0), None);
    }
}
