//! Deterministic maths: everything goes through `libm` to get identical results
//! on native and in WASM (required for network rollback).

use bevy::math::Vec3;

pub fn sin(x: f32) -> f32 {
    libm::sinf(x)
}
pub fn cos(x: f32) -> f32 {
    libm::cosf(x)
}
pub fn atan2(y: f32, x: f32) -> f32 {
    libm::atan2f(y, x)
}
pub fn sqrt(x: f32) -> f32 {
    libm::sqrtf(x)
}

/// Forward of a fighter oriented by `yaw` (rotation around Y, 0 = +Z).
pub fn forward(yaw: f32) -> Vec3 {
    Vec3::new(sin(yaw), 0.0, cos(yaw))
}

/// Right of a fighter oriented by `yaw` (right-handed frame, Y up).
pub fn right(yaw: f32) -> Vec3 {
    Vec3::new(-cos(yaw), 0.0, sin(yaw))
}

/// Yaw matching a horizontal direction.
pub fn yaw_of(dir: Vec3) -> f32 {
    atan2(dir.x, dir.z)
}

/// Wraps an angle into ]-π, π].
pub fn wrap(a: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    let mut a = a % TAU;
    if a > PI {
        a -= TAU;
    } else if a <= -PI {
        a += TAU;
    }
    a
}

/// Turns `from` towards `to` by at most `max_step` radians.
pub fn turn_towards(from: f32, to: f32, max_step: f32) -> f32 {
    let d = wrap(to - from);
    wrap(from + d.clamp(-max_step, max_step))
}

/// Horizontal length (XZ).
pub fn flat_len(v: Vec3) -> f32 {
    sqrt(v.x * v.x + v.z * v.z)
}

/// Converts a point from the local frame (x right, y up, z forward) to world.
pub fn local_to_world(pos: Vec3, yaw: f32, p: [f32; 3]) -> Vec3 {
    pos + right(yaw) * p[0] + Vec3::Y * p[1] + forward(yaw) * p[2]
}

/// Rotates a local point around Y (positive = to the left).
pub fn rotate_local(p: [f32; 3], deg: f32) -> [f32; 3] {
    let a = deg.to_radians();
    // Left = -x in our frame: rotation that sends +z to -x for a positive angle.
    let (s, c) = (sin(a), cos(a));
    [p[0] * c - p[2] * s, p[1], p[0] * s + p[2] * c]
}

/// Minimum distance between two segments [p1,q1] and [p2,q2].
pub fn segment_distance(p1: Vec3, q1: Vec3, p2: Vec3, q2: Vec3) -> f32 {
    let d1 = q1 - p1;
    let d2 = q2 - p2;
    let r = p1 - p2;
    let a = d1.dot(d1);
    let e = d2.dot(d2);
    let f = d2.dot(r);
    let eps = 1e-6;
    let (s, t);
    if a <= eps && e <= eps {
        return sqrt(r.dot(r));
    }
    if a <= eps {
        s = 0.0;
        t = (f / e).clamp(0.0, 1.0);
    } else {
        let c = d1.dot(r);
        if e <= eps {
            t = 0.0;
            s = (-c / a).clamp(0.0, 1.0);
        } else {
            let b = d1.dot(d2);
            let denom = a * e - b * b;
            let mut s0 = if denom > eps { ((b * f - c * e) / denom).clamp(0.0, 1.0) } else { 0.0 };
            let mut t0 = (b * s0 + f) / e;
            if t0 < 0.0 {
                t0 = 0.0;
                s0 = (-c / a).clamp(0.0, 1.0);
            } else if t0 > 1.0 {
                t0 = 1.0;
                s0 = ((b - c) / a).clamp(0.0, 1.0);
            }
            s = s0;
            t = t0;
        }
    }
    let c1 = p1 + d1 * s;
    let c2 = p2 + d2 * t;
    let d = c1 - c2;
    sqrt(d.dot(d))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basis_is_consistent() {
        let yaw = 0.7;
        let f = forward(yaw);
        let r = right(yaw);
        assert!(f.dot(r).abs() < 1e-6);
        // right = forward × up in a right-handed frame
        assert!((f.cross(Vec3::Y) - r).length() < 1e-6);
        assert!((yaw_of(f) - yaw).abs() < 1e-6);
    }

    #[test]
    fn rotate_left_is_positive() {
        // A point in front, rotated 90° to the left, must end up on the left (x < 0).
        let p = rotate_local([0.0, 0.0, 1.0], 90.0);
        assert!(p[0] < -0.99 && p[2].abs() < 1e-5);
    }

    #[test]
    fn segments() {
        let d = segment_distance(
            Vec3::new(0., 0., 0.),
            Vec3::new(0., 2., 0.),
            Vec3::new(1., 1., -1.),
            Vec3::new(1., 1., 1.),
        );
        assert!((d - 1.0).abs() < 1e-5);
    }
}
