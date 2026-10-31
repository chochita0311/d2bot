//! Frozen MSVC 14.44.35207 selection order used by the reference ORB build.
//! Ties are observable through descriptor/match order; a stable sort changes it.
//! Algorithm attribution is in THIRD-PARTY-NOTICES.md.
use super::orb::Keypoint;

fn before(points: &[Keypoint], left: usize, right: usize) -> bool {
    points[left].response > points[right].response
}
fn median(points: &mut [Keypoint], first: usize, middle: usize, last: usize) {
    if before(points, middle, first) {
        points.swap(middle, first);
    }
    if before(points, last, middle) {
        points.swap(last, middle);
        if before(points, middle, first) {
            points.swap(middle, first);
        }
    }
}
fn partition(points: &mut [Keypoint], first: usize, last: usize) -> (usize, usize) {
    let middle = first + (last - first) / 2;
    let end = last - 1;
    if end - first > 40 {
        let step = (end - first + 1) >> 3;
        median(points, first, first + step, first + 2 * step);
        median(points, middle - step, middle, middle + step);
        median(points, end - 2 * step, end - step, end);
        median(points, first + step, middle, end - step);
    } else {
        median(points, first, middle, end);
    }
    let (mut pivot_first, mut pivot_last) = (middle, middle + 1);
    while first < pivot_first && points[pivot_first - 1].response == points[pivot_first].response {
        pivot_first -= 1;
    }
    while pivot_last < last && points[pivot_last].response == points[pivot_first].response {
        pivot_last += 1;
    }
    let (mut scan_first, mut scan_last) = (pivot_last, pivot_first);
    loop {
        while scan_first < last {
            if before(points, pivot_first, scan_first) {
                scan_first += 1;
                continue;
            }
            if before(points, scan_first, pivot_first) {
                break;
            }
            if pivot_last != scan_first {
                points.swap(pivot_last, scan_first);
            }
            pivot_last += 1;
            scan_first += 1;
        }
        while first < scan_last {
            let previous = scan_last - 1;
            if before(points, previous, pivot_first) {
                scan_last -= 1;
                continue;
            }
            if before(points, pivot_first, previous) {
                break;
            }
            pivot_first -= 1;
            if pivot_first != previous {
                points.swap(pivot_first, previous);
            }
            scan_last -= 1;
        }
        if scan_last == first && scan_first == last {
            return (pivot_first, pivot_last);
        }
        if scan_last == first {
            if pivot_last != scan_first {
                points.swap(pivot_first, pivot_last);
            }
            pivot_last += 1;
            points.swap(pivot_first, scan_first);
            pivot_first += 1;
            scan_first += 1;
        } else if scan_first == last {
            scan_last -= 1;
            pivot_first -= 1;
            if scan_last != pivot_first {
                points.swap(scan_last, pivot_first);
            }
            pivot_last -= 1;
            points.swap(pivot_first, pivot_last);
        } else {
            scan_last -= 1;
            points.swap(scan_first, scan_last);
            scan_first += 1;
        }
    }
}
pub(super) fn retain_best(points: &mut Vec<Keypoint>, count: usize) {
    if count == 0 {
        points.clear();
        return;
    }
    if points.len() <= count {
        return;
    }
    let nth = count - 1;
    let (mut first, mut last) = (0, points.len());
    while last - first > 32 {
        let (low, high) = partition(points, first, last);
        if high <= nth {
            first = high;
        } else if low <= nth {
            break;
        } else {
            last = low;
        }
    }
    if last - first <= 32 {
        for next in first + 1..last {
            let mut hole = next;
            while hole > first && before(points, hole, hole - 1) {
                points.swap(hole, hole - 1);
                hole -= 1;
            }
        }
    }
    let boundary = points[nth].response;
    // MSVC std::partition bidirectional predicate loop, preserving its swaps.
    let (mut first, mut last) = (count, points.len());
    loop {
        while first != last && points[first].response >= boundary {
            first += 1;
        }
        if first == last {
            break;
        }
        loop {
            last -= 1;
            if first == last || points[last].response >= boundary {
                break;
            }
        }
        if first == last {
            break;
        }
        points.swap(first, last);
        first += 1;
    }
    points.truncate(first);
}
